use clap::{Parser, ValueEnum};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::Level;
use tracing_subscriber::fmt::writer::MakeWriter;

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
enum BackendKind {
    Sqlite,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Deterministic sql-middleware simulator")]
struct Args {
    #[arg(long, value_enum, default_value = "sqlite")]
    backend: BackendKind,
    #[arg(long, value_parser = humantime::parse_duration)]
    duration: Option<Duration>,
    #[arg(long)]
    iterations: Option<u64>,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long, default_value_t = 8)]
    pool_size: usize,
    #[arg(long, default_value_t = 16)]
    tasks: usize,
    #[arg(long, default_value_t = 0.02)]
    ddl_rate: f64,
    #[arg(long, default_value_t = 0.01)]
    busy_rate: f64,
    #[arg(long, default_value_t = 0.001)]
    panic_rate: f64,
    #[arg(long, default_value_t = 0.05)]
    sleep_rate: f64,
    #[arg(long, default_value_t = 4)]
    max_in_flight_tx: usize,
    #[arg(long)]
    log: Option<PathBuf>,
    #[arg(long)]
    quick: bool,
    #[arg(long)]
    stress: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SimConfig {
    backend: BackendKind,
    duration_ms: Option<u64>,
    iterations: Option<u64>,
    seed: u64,
    pool_size: usize,
    tasks: usize,
    ddl_rate: f64,
    busy_rate: f64,
    panic_rate: f64,
    sleep_rate: f64,
    max_in_flight_tx: usize,
    log: Option<PathBuf>,
    preset: Option<String>,
    first_steps: usize,
    tail_steps: usize,
}

impl SimConfig {
    fn from_args(args: Args) -> Self {
        let mut config = SimConfig {
            backend: args.backend,
            duration_ms: args.duration.map(|d| d.as_millis() as u64),
            iterations: args.iterations,
            seed: args.seed.unwrap_or_else(random_seed),
            pool_size: args.pool_size,
            tasks: args.tasks,
            ddl_rate: clamp_rate(args.ddl_rate),
            busy_rate: clamp_rate(args.busy_rate),
            panic_rate: clamp_rate(args.panic_rate),
            sleep_rate: clamp_rate(args.sleep_rate),
            max_in_flight_tx: args.max_in_flight_tx.max(1),
            log: args.log,
            preset: None,
            first_steps: 30,
            tail_steps: 80,
        };

        if args.quick {
            config.apply_quick();
        }
        if args.stress {
            config.apply_stress();
        }

        config
    }

    fn apply_quick(&mut self) {
        self.preset = Some("quick".to_string());
        self.iterations = Some(10_000);
        self.duration_ms = None;
        self.pool_size = 4;
        self.tasks = 4;
        self.ddl_rate = 0.01;
        self.busy_rate = 0.01;
        self.panic_rate = 0.0005;
        self.sleep_rate = 0.05;
        self.max_in_flight_tx = 2;
    }

    fn apply_stress(&mut self) {
        self.preset = Some("stress".to_string());
        self.iterations = Some(250_000);
        self.duration_ms = None;
        self.pool_size = 16;
        self.tasks = 64;
        self.ddl_rate = 0.05;
        self.busy_rate = 0.03;
        self.panic_rate = 0.002;
        self.sleep_rate = 0.08;
        self.max_in_flight_tx = 8;
    }
}

fn clamp_rate(value: f64) -> f64 {
    if value.is_nan() {
        0.0
    } else if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}

fn random_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    now.as_secs() ^ (now.subsec_nanos() as u64)
}

#[derive(Debug, Clone)]
enum Op {
    Checkout,
    Return,
    Begin,
    Commit,
    Rollback,
    Execute,
    Select,
    Ddl,
    Sleep(u64),
}

#[derive(Debug, Clone)]
struct TaskState {
    id: usize,
    conn_id: Option<usize>,
    in_tx: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnState {
    Idle,
    InTx,
    Busy,
    Broken,
}

#[derive(Debug, Clone)]
struct ConnSlot {
    state: ConnState,
    checked_out_by: Option<usize>,
}

#[derive(Debug, Clone)]
struct PoolModel {
    conns: Vec<ConnSlot>,
}

impl PoolModel {
    fn new(size: usize) -> Self {
        let mut conns = Vec::with_capacity(size);
        for _ in 0..size {
            conns.push(ConnSlot {
                state: ConnState::Idle,
                checked_out_by: None,
            });
        }
        Self { conns }
    }

    fn available_conn_ids(&self) -> Vec<usize> {
        self.conns
            .iter()
            .enumerate()
            .filter_map(|(id, slot)| {
                if slot.state == ConnState::Idle && slot.checked_out_by.is_none() {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
enum SimError {
    Busy,
    Io,
    Panic,
    PoolEmpty,
}

#[derive(Debug, Clone)]
struct StepOutcome {
    result: Result<(), SimError>,
    conn_id: Option<usize>,
}

struct BackendShim {
    pool: PoolModel,
    config: SimConfig,
}

impl BackendShim {
    fn new(config: SimConfig) -> Self {
        Self {
            pool: PoolModel::new(config.pool_size),
            config,
        }
    }

    fn apply(&mut self, task: &mut TaskState, op: Op, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        match op {
            Op::Sleep(_) => Ok(StepOutcome {
                result: Ok(()),
                conn_id: task.conn_id,
            }),
            Op::Checkout => self.checkout(task, rng),
            Op::Return => self.return_conn(task),
            Op::Begin => self.begin_tx(task, rng),
            Op::Commit => self.commit_tx(task, rng),
            Op::Rollback => self.rollback_tx(task, rng),
            Op::Execute | Op::Select | Op::Ddl => self.execute(task, rng),
        }
    }

    fn checkout(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        if task.conn_id.is_some() {
            return Err("task attempted double checkout".to_string());
        }
        let available = self.pool.available_conn_ids();
        if available.is_empty() {
            return Ok(StepOutcome {
                result: Err(SimError::PoolEmpty),
                conn_id: None,
            });
        }
        let idx = rng.random_range(0..available.len());
        let conn_id = available[idx];
        let slot = &mut self.pool.conns[conn_id];
        slot.checked_out_by = Some(task.id);
        slot.state = ConnState::Idle;
        task.conn_id = Some(conn_id);
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn return_conn(&mut self, task: &mut TaskState) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task returned without a checkout".to_string())?;
        if task.in_tx {
            return Err("task returned while in transaction".to_string());
        }
        let slot = &mut self.pool.conns[conn_id];
        slot.checked_out_by = None;
        if slot.state != ConnState::Broken {
            slot.state = ConnState::Idle;
        }
        task.conn_id = None;
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn begin_tx(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task began without a checkout".to_string())?;
        if task.in_tx {
            return Err("task attempted nested begin".to_string());
        }
        if let Some(err) = self.inject_error(task, conn_id, rng) {
            return Ok(err);
        }
        let slot = &mut self.pool.conns[conn_id];
        slot.state = ConnState::InTx;
        task.in_tx = true;
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn commit_tx(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task committed without a checkout".to_string())?;
        if !task.in_tx {
            return Err("task committed without an active tx".to_string());
        }
        if let Some(err) = self.inject_error(task, conn_id, rng) {
            return Ok(err);
        }
        let slot = &mut self.pool.conns[conn_id];
        slot.state = ConnState::Idle;
        task.in_tx = false;
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn rollback_tx(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task rolled back without a checkout".to_string())?;
        if !task.in_tx {
            return Err("task rolled back without an active tx".to_string());
        }
        if let Some(err) = self.inject_error(task, conn_id, rng) {
            return Ok(err);
        }
        let slot = &mut self.pool.conns[conn_id];
        slot.state = ConnState::Idle;
        task.in_tx = false;
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn execute(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task executed without a checkout".to_string())?;
        if let Some(err) = self.inject_error(task, conn_id, rng) {
            return Ok(err);
        }
        if !task.in_tx {
            let slot = &mut self.pool.conns[conn_id];
            if slot.state != ConnState::Broken {
                slot.state = ConnState::Idle;
            }
        }
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn inject_error(&mut self, task: &mut TaskState, conn_id: usize, rng: &mut ChaCha8Rng) -> Option<StepOutcome> {
        let roll: f64 = rng.random();
        if roll < self.config.panic_rate {
            self.break_connection(task, conn_id);
            return Some(StepOutcome {
                result: Err(SimError::Panic),
                conn_id: Some(conn_id),
            });
        }
        if roll < self.config.panic_rate + self.config.busy_rate {
            if !task.in_tx {
                let slot = &mut self.pool.conns[conn_id];
                slot.state = ConnState::Busy;
            }
            return Some(StepOutcome {
                result: Err(SimError::Busy),
                conn_id: Some(conn_id),
            });
        }
        if roll < self.config.panic_rate + self.config.busy_rate + 0.01 {
            self.break_connection(task, conn_id);
            return Some(StepOutcome {
                result: Err(SimError::Io),
                conn_id: Some(conn_id),
            });
        }
        None
    }

    fn break_connection(&mut self, task: &mut TaskState, conn_id: usize) {
        let slot = &mut self.pool.conns[conn_id];
        slot.state = ConnState::Broken;
        slot.checked_out_by = None;
        task.conn_id = None;
        task.in_tx = false;
    }
}

struct Oracle;

impl Oracle {
    fn check(tasks: &[TaskState], pool: &PoolModel) -> Result<(), String> {
        let mut seen = HashMap::new();
        for (conn_id, slot) in pool.conns.iter().enumerate() {
            if let Some(task_id) = slot.checked_out_by {
                if slot.state == ConnState::Broken {
                    return Err(format!("conn {conn_id} is broken but checked out by task {task_id}"));
                }
                if seen.insert(conn_id, task_id).is_some() {
                    return Err(format!("conn {conn_id} checked out multiple times"));
                }
                let task = tasks.get(task_id).ok_or_else(|| format!("task {task_id} missing"))?;
                if task.conn_id != Some(conn_id) {
                    return Err(format!("task {task_id} and conn {conn_id} mismatch"));
                }
                if slot.state == ConnState::InTx && !task.in_tx {
                    return Err(format!("conn {conn_id} is in tx but task {task_id} is not"));
                }
            }
            if slot.state == ConnState::InTx && slot.checked_out_by.is_none() {
                return Err(format!("conn {conn_id} in tx without owner"));
            }
        }

        for task in tasks {
            if task.in_tx && task.conn_id.is_none() {
                return Err(format!("task {} in tx without conn", task.id));
            }
            if let Some(conn_id) = task.conn_id {
                let slot = pool
                    .conns
                    .get(conn_id)
                    .ok_or_else(|| format!("task {} references missing conn {}", task.id, conn_id))?;
                if slot.checked_out_by != Some(task.id) {
                    return Err(format!("task {} claims conn {} without ownership", task.id, conn_id));
                }
            }
        }

        Ok(())
    }
}

struct FakeClock {
    now_ms: u64,
}

impl FakeClock {
    fn new() -> Self {
        Self { now_ms: 0 }
    }
}

struct Scheduler {
    ready: Vec<usize>,
    timers: BTreeMap<u64, Vec<usize>>,
    clock: FakeClock,
}

impl Scheduler {
    fn new(task_count: usize) -> Self {
        let ready = (0..task_count).collect();
        Self {
            ready,
            timers: BTreeMap::new(),
            clock: FakeClock::new(),
        }
    }

    fn sleep(&mut self, task_id: usize, duration_ms: u64) {
        let wake_at = self.clock.now_ms.saturating_add(duration_ms.max(1));
        self.timers.entry(wake_at).or_default().push(task_id);
    }

    fn advance_time(&mut self, elapsed_ms: u64) {
        self.clock.now_ms = self.clock.now_ms.saturating_add(elapsed_ms.max(1));
        self.wake_due();
    }

    fn next_ready(&mut self, rng: &mut ChaCha8Rng) -> Option<usize> {
        if self.ready.is_empty() {
            if let Some((wake_at, mut tasks)) = self.timers.pop_first() {
                self.clock.now_ms = wake_at;
                self.ready.append(&mut tasks);
                self.wake_due();
            } else {
                return None;
            }
        }
        let idx = rng.random_range(0..self.ready.len());
        Some(self.ready.swap_remove(idx))
    }

    fn mark_ready(&mut self, task_id: usize) {
        self.ready.push(task_id);
    }

    fn wake_due(&mut self) {
        loop {
            let _wake_at = match self.timers.first_key_value() {
                Some((time, _)) if *time <= self.clock.now_ms => *time,
                _ => break,
            };
            if let Some((_, mut tasks)) = self.timers.pop_first() {
                self.ready.append(&mut tasks);
            } else {
                break;
            }
        }
    }
}

struct EventLog {
    first_steps: Vec<String>,
    tail_steps: VecDeque<String>,
    first_limit: usize,
    tail_limit: usize,
}

impl EventLog {
    fn new(first_limit: usize, tail_limit: usize) -> Self {
        Self {
            first_steps: Vec::with_capacity(first_limit),
            tail_steps: VecDeque::with_capacity(tail_limit),
            first_limit,
            tail_limit,
        }
    }

    fn record(&mut self, message: String) {
        if self.first_steps.len() < self.first_limit {
            self.first_steps.push(message.clone());
        }
        if self.tail_limit > 0 {
            if self.tail_steps.len() == self.tail_limit {
                self.tail_steps.pop_front();
            }
            self.tail_steps.push_back(message.clone());
        }
        tracing::info!("{}", message);
    }

    fn dump_failure(&self, reason: &str) {
        tracing::error!("failure: {}", reason);
        if !self.first_steps.is_empty() {
            tracing::error!("first steps:");
            for line in &self.first_steps {
                tracing::error!("{}", line);
            }
        }
        if !self.tail_steps.is_empty() {
            tracing::error!("tail steps:");
            for line in &self.tail_steps {
                tracing::error!("{}", line);
            }
        }
    }
}

#[derive(Clone)]
struct LogWriter {
    file: Option<Arc<Mutex<File>>>,
}

impl LogWriter {
    fn new(path: Option<PathBuf>) -> io::Result<Self> {
        let file = match path {
            Some(path) => Some(Arc::new(Mutex::new(File::create(path)?))),
            None => None,
        };
        Ok(Self { file })
    }
}

struct LogWriterGuard {
    file: Option<Arc<Mutex<File>>>,
}

impl<'a> MakeWriter<'a> for LogWriter {
    type Writer = LogWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        LogWriterGuard {
            file: self.file.clone(),
        }
    }
}

impl Write for LogWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut stdout = io::stdout();
        stdout.write_all(buf)?;
        if let Some(file) = &self.file {
            let mut handle = file.lock().expect("log file lock");
            handle.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()?;
        if let Some(file) = &self.file {
            let mut handle = file.lock().expect("log file lock");
            handle.flush()?;
        }
        Ok(())
    }
}

fn next_op(task: &TaskState, in_flight_tx: usize, config: &SimConfig, rng: &mut ChaCha8Rng) -> Op {
    if rng.random::<f64>() < config.sleep_rate {
        return Op::Sleep(rng.random_range(1..=50));
    }

    if task.conn_id.is_none() {
        return Op::Checkout;
    }

    if task.in_tx {
        let weights = [
            (Op::Execute, 0.45),
            (Op::Select, 0.25),
            (Op::Commit, 0.15),
            (Op::Rollback, 0.10),
            (Op::Ddl, config.ddl_rate),
        ];
        return choose_weighted(&weights, rng);
    }

    let mut weights = vec![
        (Op::Execute, 0.35),
        (Op::Select, 0.25),
        (Op::Return, 0.15),
        (Op::Ddl, config.ddl_rate),
    ];
    if in_flight_tx < config.max_in_flight_tx {
        weights.push((Op::Begin, 0.20));
    }
    choose_weighted(&weights, rng)
}

fn choose_weighted(items: &[(Op, f64)], rng: &mut ChaCha8Rng) -> Op {
    let total: f64 = items.iter().map(|(_, weight)| weight.max(0.0)).sum();
    if total <= f64::EPSILON {
        return items.first().map(|(op, _)| op.clone()).unwrap_or(Op::Sleep(1));
    }
    let mut target = rng.random::<f64>() * total;
    for (op, weight) in items {
        let w = weight.max(0.0);
        if target <= w {
            return op.clone();
        }
        target -= w;
    }
    items.last().map(|(op, _)| op.clone()).unwrap_or(Op::Sleep(1))
}

fn format_op(op: &Op) -> String {
    match op {
        Op::Sleep(ms) => format!("Sleep({}ms)", ms),
        other => format!("{:?}", other),
    }
}

fn main() {
    let args = Args::parse();
    let config = SimConfig::from_args(args);
    let writer = LogWriter::new(config.log.as_ref().map(|p| p.to_path_buf())).unwrap_or_else(|err| {
        eprintln!("failed to open log file: {err}");
        std::process::exit(1);
    });

    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_target(false)
        .with_max_level(Level::INFO)
        .init();

    let config_json = serde_json::to_string_pretty(&config).unwrap_or_else(|_| "{}".to_string());
    tracing::info!("config: {}", config_json);

    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
    let mut backend = BackendShim::new(config.clone());
    let mut tasks: Vec<TaskState> = (0..config.tasks)
        .map(|id| TaskState {
            id,
            conn_id: None,
            in_tx: false,
        })
        .collect();
    let mut scheduler = Scheduler::new(config.tasks);
    let mut events = EventLog::new(config.first_steps, config.tail_steps);

    let max_steps = config.iterations.unwrap_or(u64::MAX);
    let max_time = config.duration_ms.unwrap_or(u64::MAX);

    let mut step: u64 = 0;
    while step < max_steps && scheduler.clock.now_ms <= max_time {
        let task_id = match scheduler.next_ready(&mut rng) {
            Some(id) => id,
            None => break,
        };
        let in_flight_tx = tasks.iter().filter(|t| t.in_tx).count();
        let op = next_op(&tasks[task_id], in_flight_tx, &config, &mut rng);
        let op_display = format_op(&op);
        let outcome = backend.apply(&mut tasks[task_id], op.clone(), &mut rng);

        match outcome {
            Ok(step_outcome) => {
                if let Op::Sleep(ms) = op {
                    scheduler.sleep(task_id, ms);
                } else {
                    scheduler.mark_ready(task_id);
                }
                let result_label = match step_outcome.result {
                    Ok(()) => "Ok".to_string(),
                    Err(ref err) => format!("Err({err:?})"),
                };
                let conn_label = step_outcome
                    .conn_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let message = format!(
                    "step={} time={}ms task={} op={} conn={} result={}",
                    step,
                    scheduler.clock.now_ms,
                    task_id,
                    op_display,
                    conn_label,
                    result_label
                );
                events.record(message);
            }
            Err(reason) => {
                events.dump_failure(&reason);
                std::process::exit(1);
            }
        }

        if let Err(reason) = Oracle::check(&tasks, &backend.pool) {
            events.dump_failure(&reason);
            std::process::exit(1);
        }
        scheduler.advance_time(1);
        step += 1;
    }

    let summary = format!(
        "complete: steps={} time={}ms tasks={} pool_size={}",
        step,
        scheduler.clock.now_ms,
        config.tasks,
        config.pool_size
    );
    tracing::info!("{}", summary);
}
