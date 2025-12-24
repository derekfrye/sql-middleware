use std::collections::BTreeMap;

use rand::Rng;
use rand_chacha::ChaCha8Rng;

pub(crate) struct FakeClock {
    pub(crate) now_ms: u64,
}

impl FakeClock {
    pub(crate) fn new() -> Self {
        Self { now_ms: 0 }
    }
}

pub(crate) struct Scheduler {
    ready: Vec<usize>,
    timers: BTreeMap<u64, Vec<usize>>,
    pub(crate) clock: FakeClock,
}

impl Scheduler {
    pub(crate) fn new(task_count: usize) -> Self {
        let ready = (0..task_count).collect();
        Self {
            ready,
            timers: BTreeMap::new(),
            clock: FakeClock::new(),
        }
    }

    pub(crate) fn sleep(&mut self, task_id: usize, duration_ms: u64) {
        let wake_at = self.clock.now_ms.saturating_add(duration_ms.max(1));
        self.timers.entry(wake_at).or_default().push(task_id);
    }

    pub(crate) fn advance_time(&mut self, elapsed_ms: u64) {
        self.clock.now_ms = self.clock.now_ms.saturating_add(elapsed_ms.max(1));
        self.wake_due();
    }

    pub(crate) fn next_ready(&mut self, rng: &mut ChaCha8Rng) -> Option<usize> {
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

    pub(crate) fn mark_ready(&mut self, task_id: usize) {
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
