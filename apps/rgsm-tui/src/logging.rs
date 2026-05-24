use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::Local;

const MAX_LOG_ENTRIES: usize = 300;

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug)]
pub struct SessionLog {
    entries: VecDeque<LogEntry>,
    file_path: PathBuf,
}

impl SessionLog {
    pub fn new(data_dir: &Path) -> Result<Self> {
        fs::create_dir_all(data_dir)?;
        let file_path = data_dir.join("rgsm-tui.log");
        Ok(Self {
            entries: VecDeque::new(),
            file_path,
        })
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &LogEntry> {
        self.entries.iter()
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.push(LogLevel::Info, message.into());
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.push(LogLevel::Warning, message.into());
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.push(LogLevel::Error, message.into());
    }

    fn push(&mut self, level: LogLevel, message: String) {
        let timestamp = timestamp();
        self.entries.push_back(LogEntry {
            timestamp: timestamp.clone(),
            level: level.clone(),
            message: message.clone(),
        });
        while self.entries.len() > MAX_LOG_ENTRIES {
            self.entries.pop_front();
        }

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
        {
            let _ = writeln!(file, "{timestamp} [{level:?}] {message}");
        }
    }
}

fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}
