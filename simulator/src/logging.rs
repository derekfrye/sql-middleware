use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::writer::MakeWriter;

pub(crate) struct EventLog {
    first_steps: Vec<String>,
    tail_steps: VecDeque<String>,
    first_limit: usize,
    tail_limit: usize,
}

impl EventLog {
    pub(crate) fn new(first_limit: usize, tail_limit: usize) -> Self {
        Self {
            first_steps: Vec::with_capacity(first_limit),
            tail_steps: VecDeque::with_capacity(tail_limit),
            first_limit,
            tail_limit,
        }
    }

    pub(crate) fn record(&mut self, message: String) {
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

    pub(crate) fn dump_failure(&self, reason: &str) {
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
pub(crate) struct LogWriter {
    file: Option<Arc<Mutex<File>>>,
}

impl LogWriter {
    pub(crate) fn new(path: Option<PathBuf>) -> io::Result<Self> {
        let file = match path {
            Some(path) => Some(Arc::new(Mutex::new(File::create(path)?))),
            None => None,
        };
        Ok(Self { file })
    }
}

pub(crate) struct LogWriterGuard {
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
