use crate::journal::JudgeJournal;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

type Exchange = dyn Fn(&str) -> Result<u64, String> + Send + Sync;

/// Framed guest storage over an acknowledged byte exchange.
pub struct GuestJudgeJournal {
    exchange: Box<Exchange>,
    recovery: Option<Arc<dyn JudgeJournal>>,
    failed: AtomicBool,
    retry_allowed: AtomicBool,
    token: AtomicU64,
}

impl std::fmt::Debug for GuestJudgeJournal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("GuestJudgeJournal")
    }
}

impl GuestJudgeJournal {
    pub fn new(exchange: impl Fn(&str) -> Result<u64, String> + Send + Sync + 'static) -> Self {
        Self {
            exchange: Box::new(exchange),
            recovery: None,
            failed: AtomicBool::new(false),
            retry_allowed: AtomicBool::new(true),
            token: AtomicU64::new(0),
        }
    }
    pub fn with_recovery(mut self, recovery: Arc<dyn JudgeJournal>) -> Self {
        self.recovery = Some(recovery);
        self
    }

    pub fn stop_retries(&self) {
        self.retry_allowed
            .store(false, std::sync::atomic::Ordering::Release);
    }

    pub fn check(&self) -> Result<(), String> {
        if self.failed.load(std::sync::atomic::Ordering::Relaxed) {
            Err(
                "the OS judge log did not acknowledge storage; stopped before delivering a result"
                    .into(),
            )
        } else {
            Ok(())
        }
    }
    fn exchange(&self, command: &str) -> Result<u64, String> {
        (self.exchange)(command)
    }
}

impl JudgeJournal for GuestJudgeJournal {
    fn retry_allowed(&self) -> bool {
        self.retry_allowed
            .load(std::sync::atomic::Ordering::Acquire)
    }

    fn reserve(&self, bytes: u64) -> Result<(), String> {
        self.check()?;
        if self
            .token
            .compare_exchange(
                0,
                u64::MAX,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            return Err("judge ledger reservation is already held".into());
        }
        match self.exchange(&format!(":ledger-reserve {bytes}")) {
            Ok(token) if token > 0 && token < (1 << 62) => {
                self.token
                    .store(token, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            }
            outcome => {
                self.failed
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                Err(format!("judge ledger reservation failed: {outcome:?}"))
            }
        }
    }

    fn release(&self) -> Result<(), String> {
        let token = self.token.load(std::sync::atomic::Ordering::Relaxed);
        if token == 0 {
            return Ok(());
        }
        let result = self.exchange(&format!(":ledger-release {token}"));
        self.token.store(0, std::sync::atomic::Ordering::Relaxed);
        result.map(|_| ())
    }

    fn append(&self, bytes: &[u8]) -> Result<(), String> {
        self.check()?;
        let result = (|| {
            let token = self.token.load(std::sync::atomic::Ordering::Relaxed);
            if token == 0 || token >= (1 << 62) {
                return Err("judge ledger has no reservation".into());
            }
            if let Some(recovery) = &self.recovery {
                recovery.append(bytes)?;
            }
            for chunk in crate::journal::frames(bytes)? {
                if chunk.len() != 512
                    || chunk[511] != b'\n'
                    || !chunk[..511].iter().all(|byte| (32..=126).contains(byte))
                {
                    return Err("ledger frame is not an ASCII sector".into());
                }
                let packet =
                    std::str::from_utf8(&chunk[..511]).map_err(|error| error.to_string())?;
                let written = self.exchange(&format!(":ledger {token} {packet}"))?;
                if written != 512 {
                    return Err("OS judge ledger short write".into());
                }
            }
            Ok(())
        })();
        if result.is_err() {
            self.failed
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn reservation_frames_and_release_use_acknowledged_transport() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&commands);
        let journal = GuestJudgeJournal::new(move |command| {
            captured.lock().unwrap().push(command.to_owned());
            if command.starts_with(":ledger-reserve ") {
                Ok(7)
            } else if command.starts_with(":ledger-release ") {
                Ok(0)
            } else {
                Ok(512)
            }
        });
        assert!(journal.append(b"no reservation").is_err());
        // A failed append permanently closes that transport instance.
        assert!(journal.check().is_err());
        let commands = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&commands);
        let journal = GuestJudgeJournal::new(move |command| {
            captured.lock().unwrap().push(command.to_owned());
            if command.starts_with(":ledger-reserve ") {
                Ok(7)
            } else if command.starts_with(":ledger-release ") {
                Ok(0)
            } else {
                Ok(512)
            }
        });
        journal.reserve(4096).unwrap();
        journal.append(b"record\n").unwrap();
        journal.release().unwrap();
        let values = commands.lock().unwrap();
        assert_eq!(values.first().unwrap(), ":ledger-reserve 4096");
        assert!(values[1].starts_with(":ledger 7 AGELJ1 "));
        assert_eq!(values.last().unwrap(), ":ledger-release 7");
        journal.check().unwrap();
    }

    #[test]
    fn result_reaches_recovery_when_guest_transport_drops() {
        #[derive(Debug, Default)]
        struct Recovery(Mutex<Vec<u8>>);

        impl JudgeJournal for Recovery {
            fn append(&self, bytes: &[u8]) -> Result<(), String> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(())
            }
        }

        let recovery = Arc::new(Recovery::default());
        let frames = AtomicU64::new(0);
        let journal = GuestJudgeJournal::new(move |command| {
            if command.starts_with(":ledger-reserve ") {
                Ok(7)
            } else if frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0 {
                Ok(512)
            } else {
                Err("guest disconnected".into())
            }
        })
        .with_recovery(recovery.clone());
        journal.reserve(4096).unwrap();
        journal.append(b"start\n").unwrap();
        assert!(journal.append(b"result\n").is_err());
        assert!(journal.check().is_err());
        assert_eq!(*recovery.0.lock().unwrap(), b"start\nresult\n");
    }

    #[test]
    fn missing_storage_acknowledgement_closes_the_transport() {
        let journal = GuestJudgeJournal::new(|_| Err("guest disconnected".into()));
        assert!(journal.reserve(4096).is_err());
        assert!(journal.check().is_err());
        assert!(journal.append(b"must not deliver").is_err());
    }
}
