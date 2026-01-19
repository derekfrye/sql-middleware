use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::writer::MakeWriter;

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
