use agel_core::{Digest, ModelRequest};
use agel_effects::byte_carrier::Carrier;
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
        mpsc::{self, Receiver, RecvTimeoutError},
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
        out.write_all(b"\x1d")
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
                return Err(format!(
                    "guest ledger acknowledgement unavailable for request {id}"
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

struct Job {
    id: u64,
    result: Receiver<String>,
}
fn outcome_reply(outcome: Result<String, agel_model::ProviderError>) -> String {
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
    format!("{status} {}", encode(text.as_bytes()))
}
fn poll_job(slot: &mut Option<Job>, id: u64) -> Result<String, String> {
    let job = slot
        .as_ref()
        .filter(|job| job.id == id)
        .ok_or("unknown inference job")?;
    match job.result.recv_timeout(Duration::from_millis(250)) {
        Ok(reply) => {
            *slot = None;
            Ok(reply)
        }
        Err(RecvTimeoutError::Timeout) => Ok(format!("pending {id}")),
        Err(RecvTimeoutError::Disconnected) => {
            *slot = None;
            Err("inference worker ended without a result".into())
        }
    }
}
fn request(id: u64, header: &str, body: &str) -> Result<ModelRequest, String> {
    if body.len() > 128 * 1024 {
        return Err("inference prompt exceeds carrier bound".into());
    }
    Ok(ModelRequest {
        id,
        world_id: 0,
        requester: 1,
        reply_to: 1,
        provider: header.into(),
        prompt: decode(body)?,
        prompt_digest: Digest::ZERO,
        effect_key: Digest::ZERO,
    })
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
    let providers = Arc::new(providers);
    let mut carrier = env::var("AGEL_BYTE_ENDPOINT")
        .ok()
        .map(|s| s.parse().map(Carrier::new))
        .transpose()?;
    let mut job: Option<Job> = None;
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
    let mut byte_number = None;
    let mut byte_block = String::new();
    let started = Instant::now();
    let deadline = started + Duration::from_secs(900);
    loop {
        let lines = wire.lines(&mut cursor);
        for line in lines.lines() {
            if number.is_none()
                && byte_number.is_none()
                && !line.starts_with("model-request ")
                && !line.starts_with("byte-request ")
            {
                println!("{line}");
            }
            let line = line.trim_start_matches("live-desktop> ");
            if let Some(process) = args[2].split_once(" exited").map(|(prefix, _)| prefix) {
                if line == format!("{process} never yielded; tick budget exhausted; stopped") {
                    return Err("guest watchdog stopped the requested process".into());
                }
            }
            if line == "byte-input refused" {
                return Err("guest process input queue refused a frame".into());
            }
            if line.starts_with(&args[2]) {
                return Ok(());
            }
            if line == "model-request end" {
                let id = number.take().ok_or("inference block has no request id")?;
                let reply: Result<String, String> = (|| {
                    let text = block.strip_suffix('\n').unwrap_or(&block);
                    if let Some(id) = text.strip_prefix("AGEL-INFER/2 poll ") {
                        let result = poll_job(&mut job, id.parse().map_err(|_| "invalid job id")?);
                        if let Ok(reply) = &result {
                            if !reply.starts_with("pending ") {
                                println!(
                                    "inference-bridge: completed job {id} at-ms {} status {}",
                                    started.elapsed().as_millis(),
                                    reply.split_whitespace().next().unwrap_or("invalid")
                                );
                            }
                        }
                        return result;
                    }
                    let (header, body) = text
                        .split_once('\n')
                        .ok_or("inference block has no payload")?;
                    let (route, asynchronous) =
                        if let Some(route) = header.strip_prefix("AGEL-INFER/2 start ") {
                            (route, true)
                        } else {
                            (
                                header
                                    .strip_prefix("AGEL-INFER/1 ")
                                    .ok_or("unknown inference framing")?,
                                false,
                            )
                        };
                    if job.is_some() {
                        return Err("an inference job is already outstanding".into());
                    }
                    if !providers.is_enabled(route) {
                        return Err("provider route is not configured".into());
                    }
                    let request = request(id, route, body)?;
                    if asynchronous {
                        println!(
                            "inference-bridge: started job {id} provider {route} at-ms {}",
                            started.elapsed().as_millis()
                        );
                        let providers = Arc::clone(&providers);
                        let journal = Arc::clone(&journal);
                        let (sender, result) = mpsc::channel();
                        thread::spawn(move || {
                            let outcome = providers.infer(&request);
                            // A failed durable receipt never becomes an accepted model result.
                            let reply = match journal.check() {
                                Ok(()) => outcome_reply(outcome),
                                Err(problem) => format!("error {}", encode(problem.as_bytes())),
                            };
                            let _ = sender.send(reply);
                        });
                        job = Some(Job { id, result });
                        Ok(format!("pending {id}"))
                    } else {
                        let outcome = providers.infer(&request);
                        journal.check()?;
                        Ok(outcome_reply(outcome))
                    }
                })();
                let reply =
                    reply.unwrap_or_else(|error| format!("error {}", encode(error.as_bytes())));
                wire.raw(&format!(":model-reply {id} {reply}"))?;
                block.clear();
            } else if let Some(id) = line
                .strip_prefix("model-request ")
                .and_then(|s| s.strip_suffix(':'))
                .and_then(|s| s.parse::<u64>().ok())
            {
                number = Some(id);
                block.clear();
            } else if let Some(id) = line
                .strip_prefix("byte-request ")
                .and_then(|s| s.strip_suffix(':'))
                .and_then(|s| s.parse::<u64>().ok())
            {
                byte_number = Some(id);
                byte_block.clear();
            } else if line == "byte-request end" {
                let id = byte_number.take().ok_or("byte block has no request id")?;
                let arrived = started.elapsed().as_millis();
                let result = carrier
                    .as_mut()
                    .ok_or("byte endpoint is not configured".to_owned())
                    .and_then(|c| c.exchange(&byte_block));
                let reply =
                    result.unwrap_or_else(|error| format!("error {}", encode(error.as_bytes())));
                println!(
                    "byte-bridge: request {id} operation {} at-ms {arrived} duration-ms {} status {}",
                    byte_block.split_whitespace().nth(1).unwrap_or("invalid"),
                    started.elapsed().as_millis() - arrived,
                    reply.split_whitespace().next().unwrap_or("invalid")
                );
                wire.raw(&format!(":byte-reply {id} {reply}"))?;
            } else if byte_number.is_some() {
                if !byte_block.is_empty() || line.len() > 1024 {
                    return Err("byte block exceeds framing".into());
                }
                byte_block.push_str(line);
            } else if number.is_some() {
                if block.len() + line.len() > 128 * 1024 + 128 {
                    return Err("model block exceeds framing".into());
                }
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
    fn polling_retains_work_until_the_matching_job_claims_its_result() {
        let (sender, result) = mpsc::channel();
        let mut slot = Some(Job { id: 19, result });
        assert!(poll_job(&mut slot, 20).is_err());
        assert_eq!(poll_job(&mut slot, 19).unwrap(), "pending 19");
        sender.send("ok 6162".into()).unwrap();
        assert_eq!(poll_job(&mut slot, 19).unwrap(), "ok 6162");
        assert!(poll_job(&mut slot, 19).is_err());
        let (sender, result) = mpsc::channel();
        slot = Some(Job { id: 20, result });
        drop(sender);
        assert!(poll_job(&mut slot, 20).is_err());
        assert!(slot.is_none());
    }

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
        let expected = format!("\x1d{line}\n");
        assert!(expected.len() <= 4096);
        let mut received = vec![0; expected.len()];
        reader.read_exact(&mut received).unwrap();
        assert_eq!(received, expected.as_bytes());
    }
}
