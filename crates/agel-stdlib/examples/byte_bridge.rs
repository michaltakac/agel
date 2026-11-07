use agel_effects::byte_carrier::{trace_metadata, Carrier};
use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn command(
    stream: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for byte in text.bytes() {
        stream.write_all(&[byte])?;
        loop {
            let mut got = [0];
            reader.read_exact(&mut got)?;
            if got[0] == byte {
                break;
            }
        }
    }
    stream.write_all(b"\n")?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: byte_bridge SOCKET ENDPOINT COMMAND".into());
    }
    let mut stream = UnixStream::connect(&args[0])?;
    stream.set_read_timeout(Some(Duration::from_secs(180)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut greeting = Vec::new();
    while !greeting.ends_with(b"live-desktop> ") {
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        greeting.push(byte[0]);
    }
    command(&mut stream, &mut reader, &args[2])?;
    let mut carrier = Carrier::new(args[1].parse()?);
    let mut id = None;
    let mut payload = String::new();
    let started = Instant::now();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err("guest serial closed".into());
        }
        let line = line
            .trim_end_matches(['\r', '\n'])
            .trim_start_matches("live-desktop> ");
        if let Some(number) = line
            .strip_prefix("byte-request ")
            .and_then(|s| s.strip_suffix(':'))
        {
            id = Some(number.parse::<u64>()?);
            payload.clear();
        } else if line == "byte-request end" {
            let id = id.take().ok_or("request has no id")?;
            let arrived = started.elapsed().as_millis();
            let reply = match carrier.exchange(&payload) {
                Ok(reply) => reply,
                Err(error) => format!("error {}", hex(error.as_bytes())),
            };
            let operation = payload.split_whitespace().nth(1).unwrap_or("invalid");
            println!(
                "byte-bridge: request {id} operation {operation} at-ms {arrived} duration-ms {} status {}{}",
                started.elapsed().as_millis() - arrived,
                reply.split_whitespace().next().unwrap_or("invalid"),
                trace_metadata(operation, &reply)
            );
            writeln!(stream, "\x1d:byte-reply {id} {reply}")?;
        } else if id.is_some() {
            if !payload.is_empty() || line.len() > 1024 {
                return Err("byte request exceeds carrier framing".into());
            }
            payload.push_str(line);
        } else if line.starts_with(":stdin :byte-reply ") || line == "INPUT QUEUED" {
        } else {
            if line == "process agel never yielded; tick budget exhausted; stopped" {
                return Err("guest watchdog stopped agel".into());
            }
            if line == "byte-input refused" {
                return Err("guest byte queue refused input".into());
            }
            println!("{line}");
            if line.starts_with("process agel exited with status ") {
                if line == "process agel exited with status 0" {
                    return Ok(());
                }
                return Err("guest process failed".into());
            }
        }
    }
}
