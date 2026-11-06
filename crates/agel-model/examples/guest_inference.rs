use agel_core::{Digest, ModelRequest};
use agel_model::{
    guest_journal::GuestJudgeJournal, CodexProvider, CommandLimits, JevProvider, ProviderRegistry,
};
use std::{
    env,
    fmt::Write as _,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

struct Wire {
    output: Mutex<UnixStream>,
    input: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicBool>,
    sequence: AtomicU64,
}

impl Wire {
    fn connect(path: &str) -> Result<Arc<Self>, String> {
        let stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
        let mut reader = stream.try_clone().map_err(|e| e.to_string())?;
        let input = Arc::new(Mutex::new(Vec::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let sink = Arc::clone(&input);
        let ended = Arc::clone(&closed);
        thread::spawn(move || {
            let mut block = [0; 4096];
            while let Ok(count) = reader.read(&mut block) {
                if count == 0 {
                    break;
                }
                sink.lock()
                    .expect("wire input")
                    .extend_from_slice(&block[..count]);
            }
            ended.store(true, Ordering::Relaxed);
        });
        Ok(Arc::new(Self {
            output: Mutex::new(stream),
            input,
            closed,
            sequence: AtomicU64::new(1),
        }))
    }
    fn len(&self) -> usize {
        self.input.lock().expect("wire input").len()
    }
    fn wait(&self, start: usize, bytes: &[u8], seconds: u64) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            if self.input.lock().expect("wire input")[start..]
                .windows(bytes.len())
                .any(|w| w == bytes)
            {
                return Ok(());
            }
            if self.closed.load(Ordering::Relaxed) || Instant::now() > deadline {
                return Err("guest transport closed or timed out".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn raw(&self, line: &str) -> Result<(), String> {
        let mut out = self.output.lock().map_err(|e| e.to_string())?;
        out.write_all(b":stdin ")
            .and_then(|()| out.write_all(line.as_bytes()))
            .and_then(|()| out.write_all(b"\n"))
            .map_err(|e| e.to_string())
    }
    fn command(&self, line: &str) -> Result<(), String> {
        for byte in line.bytes() {
            let start = self.len();
            self.output
                .lock()
                .map_err(|e| e.to_string())?
                .write_all(&[byte])
                .map_err(|e| e.to_string())?;
            self.wait(start, &[byte], 10)?;
        }
        self.output
            .lock()
            .map_err(|e| e.to_string())?
            .write_all(b"\n")
            .map_err(|e| e.to_string())
    }
    fn lines(&self, cursor: &mut usize) -> String {
        let input = self.input.lock().expect("wire input");
        let start = *cursor;
        let Some(end) = input[start..].iter().rposition(|b| *b == b'\n') else {
            return String::new();
        };
        *cursor = start + end + 1;
        String::from_utf8_lossy(&input[start..*cursor]).replace('\r', "")
    }
    fn ledger(&self, command: &str) -> Result<u64, String> {
        let id = self.sequence.fetch_add(1, Ordering::Relaxed);
        let (word, rest) = command.split_once(' ').ok_or("invalid ledger request")?;
        let start = self.len();
        self.raw(&format!("{word} {id} {rest}"))?;
        let mut cursor = start;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            for line in self.lines(&mut cursor).lines() {
                let line = line.trim_start_matches("live-desktop> ");
                if let Some(value) = line.strip_prefix(&format!("ledger {id} ok ")) {
                    return value.parse::<u64>().map_err(|e| e.to_string());
                }
                if line.starts_with(&format!("ledger {id} error ")) {
                    return Err(line.into());
                }
            }
            if self.closed.load(Ordering::Relaxed) || Instant::now() > deadline {
                let input = self.input.lock().map_err(|e| e.to_string())?;
                return Err(format!(
                    "guest ledger acknowledgement unavailable: {}",
                    String::from_utf8_lossy(&input[start..])
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

fn encode(bytes: &[u8]) -> String {
    let mut hex = String::new();
    for byte in bytes {
        write!(hex, "{byte:02x}").expect("string formatting");
    }
    hex
}

fn decode(hex: &str) -> Result<String, String> {
    if hex.len() & 1 != 0 || !hex.is_ascii() {
        return Err("invalid inference hex".into());
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 4 {
        return Err("usage: guest_inference SOCKET WORKSPACE DONE-MARKER COMMAND...".into());
    }
    let wire = Wire::connect(&args[0])?;
    let transport = Arc::clone(&wire);
    let journal = Arc::new(GuestJudgeJournal::new(move |command| {
        transport.ledger(command)
    }));
    let mut limits = CommandLimits::new(PathBuf::from(&args[1]));
    limits.timeout = Duration::from_secs(90);
    limits.max_output_bytes = 64 * 1024;
    let mut providers = ProviderRegistry::default();
    providers.register(CodexProvider::new(
        env::var("AGEL_CODEX_BIN").unwrap_or_else(|_| "codex".into()),
        limits.clone(),
    ));
    providers.register(
        JevProvider::new(
            env::var("AGEL_CURL_BIN").unwrap_or_else(|_| "curl".into()),
            env::var("TYPESAFEAI_API_KEY")?,
            limits,
        )
        .with_journal(journal.clone()),
    );
    wire.wait(0, b"live-desktop> ", 120)?;
    for command in &args[3..args.len() - 1] {
        let start = wire.len();
        wire.command(command)?;
        wire.wait(start, b"live-desktop> ", 120)?;
    }
    let mut cursor = wire.len();
    wire.command(args.last().ok_or("missing command")?)?;
    let mut number = None;
    let mut block = String::new();
    let deadline = Instant::now() + Duration::from_secs(900);
    loop {
        let lines = wire.lines(&mut cursor);
        for line in lines.lines() {
            println!("{line}");
            let line = line.trim_start_matches("live-desktop> ");
            if line.starts_with(&args[2]) {
                return Ok(());
            }
            if line == "model-request end" {
                let id = number.take().ok_or("inference block has no request id")?;
                let (header, body) = block
                    .trim_end()
                    .split_once('\n')
                    .ok_or("inference block has no payload")?;
                let provider = header
                    .strip_prefix("AGEL-INFER/1 ")
                    .ok_or("unknown inference framing")?;
                let request = ModelRequest {
                    id,
                    world_id: 0,
                    requester: 1,
                    reply_to: 1,
                    provider: provider.into(),
                    prompt: decode(body)?,
                    prompt_digest: Digest::ZERO,
                    effect_key: Digest::ZERO,
                };
                let outcome = providers.infer(&request);
                if let Err(problem) = journal.check() {
                    return Err(format!("{problem}; provider outcome: {outcome:?}").into());
                }
                let (status, text) = match outcome {
                    Ok(text) if text.len() <= 1900 => ("ok", text),
                    Ok(_) => (
                        "error",
                        "inference reply exceeded native transport limit".into(),
                    ),
                    Err(e) if e.to_string().len() <= 1900 => ("error", e.to_string()),
                    Err(_) => (
                        "error",
                        "provider error exceeded native transport limit".into(),
                    ),
                };
                wire.raw(&format!(
                    ":model-reply {id} {status} {}",
                    encode(text.as_bytes())
                ))?;
                block.clear();
            } else if let Some(id) = line
                .strip_prefix("model-request ")
                .and_then(|s| s.strip_suffix(':'))
                .and_then(|s| s.parse::<u64>().ok())
            {
                number = Some(id);
                block.clear();
            } else if number.is_some() {
                block.push_str(line);
                block.push('\n');
            }
        }
        if wire.closed.load(Ordering::Relaxed) || Instant::now() > deadline {
            return Err("guest run closed or timed out".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_replies_use_the_queue_and_preserve_large_payloads() {
        let (writer, mut reader) = UnixStream::pair().unwrap();
        let wire = Wire {
            output: Mutex::new(writer),
            input: Arc::new(Mutex::new(Vec::new())),
            closed: Arc::new(AtomicBool::new(false)),
            sequence: AtomicU64::new(1),
        };
        let line = format!(":model-reply 1 ok {}", encode("x".repeat(1900).as_bytes()));
        wire.raw(&line).unwrap();
        let expected = format!(":stdin {line}\n");
        assert!(expected.len() <= 4096);
        let mut received = vec![0; expected.len()];
        reader.read_exact(&mut received).unwrap();
        assert_eq!(received, expected.as_bytes());
    }
}
