//! Byte sinks for records produced by Agel accounting policy.
use agel_core::{Value, World};
use std::fmt::Debug;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub trait JudgeJournal: Debug + Send + Sync {
    fn append(&self, bytes: &[u8]) -> Result<(), String>;
}

#[derive(Debug)]
pub struct FileJudgeJournal {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileJudgeJournal {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }
}

impl JudgeJournal for FileJudgeJournal {
    fn append(&self, bytes: &[u8]) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|error| error.to_string())?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_data())
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn record(function: &str, arguments: Vec<Value>) -> Result<Vec<u8>, String> {
    let mut world = World::default();
    world
        .evaluate(include_str!("../../agel-stdlib/judge-accounting.agel"))
        .map_err(|error| error.to_string())?;
    let call = format!(
        "(import agel/judge-accounting) (apply {function} '{})",
        Value::List(arguments)
    );
    let value = world
        .evaluate(&call)
        .map_err(|error| error.to_string())?
        .values
        .pop()
        .ok_or("accounting policy returned no record")?;
    Ok(format!("{value}\n").into_bytes())
}
