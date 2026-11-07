//! Byte sinks for records produced by Agel accounting policy.
use agel_core::{Budget, EvaluationOptions, Value, World};
use std::fmt::Debug;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub trait JudgeJournal: Debug + Send + Sync {
    fn retry_allowed(&self) -> bool {
        true
    }
    fn reserve(&self, _bytes: u64) -> Result<(), String> {
        Ok(())
    }
    fn release(&self) -> Result<(), String> {
        Ok(())
    }
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
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
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

fn framing(function: &str, arguments: Vec<Value>) -> Result<Value, String> {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 200_000_000,
            max_source_bytes: 8 * 1024 * 1024,
            max_collection_len: 4 * 1024 * 1024,
            ..Budget::default()
        },
        ..EvaluationOptions::default()
    };
    world
        .evaluate_with(
            include_str!("../../agel-stdlib/journal-frame.agel"),
            &options,
        )
        .map_err(|error| error.to_string())?;
    let call = format!(
        "(import agel/journal-frame) (apply {function} '{})",
        Value::List(arguments)
    );
    world
        .evaluate_with(&call, &options)
        .map_err(|error| error.to_string())?
        .values
        .pop()
        .ok_or_else(|| "framing policy returned no value".into())
}

pub fn frames(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let values = framing("journal-pack", vec![Value::String(text.into())])?;
    let frames = match values {
        Value::List(frames) => frames,
        Value::Nil => Vec::new(),
        _ => return Err("framing policy returned no frames".into()),
    };
    frames
        .into_iter()
        .map(|value| match value {
            Value::String(text) if text.len() == 512 => Ok(text.into_bytes()),
            _ => Err("framing policy returned an invalid sector".into()),
        })
        .collect()
}

pub fn reservation(start: usize, limit: usize) -> Result<u64, String> {
    let start = i64::try_from(start).map_err(|error| error.to_string())?;
    let limit = i64::try_from(limit).map_err(|error| error.to_string())?;
    match framing(
        "journal-reservation",
        vec![Value::Int(start), Value::Int(limit)],
    )? {
        Value::Int(bytes) if bytes > 0 => Ok(bytes as u64),
        _ => Err("framing policy returned an invalid reservation".into()),
    }
}
