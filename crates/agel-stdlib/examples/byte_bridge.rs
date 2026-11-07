use std::{
    env,
    fmt::Write as _,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").unwrap();
    }
    out
}
fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() > 512 || text.len() % 2 != 0 || !text.is_ascii() {
        return Err("invalid byte payload".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "invalid hex".into()))
        .collect()
}
struct Carrier {
    endpoint: SocketAddr,
    stream: Option<TcpStream>,
}
impl Carrier {
    fn exchange(&mut self, request: &str) -> Result<String, String> {
        let request = request
            .strip_prefix("AGEL-BYTES/1 ")
            .ok_or("unknown byte framing")?;
        let (operation, argument) = request.split_once(' ').unwrap_or((request, ""));
        match operation {
            "open" if argument.is_empty() => {
                if self.stream.is_some() {
                    return Err("stream already open".into());
                }
                let stream = TcpStream::connect_timeout(&self.endpoint, Duration::from_secs(5))
                    .map_err(|e| e.to_string())?;
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .map_err(|e| e.to_string())?;
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .map_err(|e| e.to_string())?;
                stream.set_nodelay(true).map_err(|e| e.to_string())?;
                self.stream = Some(stream);
                Ok("ok".into())
            }
            "close" if argument.is_empty() => {
                self.stream = None;
                Ok("ok".into())
            }
            "read" | "entropy" => {
                let size: usize = argument.parse().map_err(|_| "invalid byte count")?;
                if !(1..=256).contains(&size) {
                    return Err("byte count exceeds carrier bound".into());
                }
                let mut bytes = vec![0; size];
                if operation == "entropy" {
                    File::open("/dev/urandom")
                        .and_then(|mut f| f.read_exact(&mut bytes))
                        .map_err(|e| e.to_string())?;
                } else {
                    let count = self
                        .stream
                        .as_mut()
                        .ok_or("stream closed")?
                        .read(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    bytes.truncate(count);
                }
                Ok(format!("ok {}", hex(&bytes)))
            }
            "write" => {
                let bytes = unhex(argument)?;
                self.stream
                    .as_mut()
                    .ok_or("stream closed")?
                    .write_all(&bytes)
                    .map_err(|e| e.to_string())?;
                Ok(format!("ok {}", bytes.len()))
            }
            _ => Err("unknown byte operation".into()),
        }
    }
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
    let mut carrier = Carrier {
        endpoint: args[1].parse()?,
        stream: None,
    };
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
            println!(
                "byte-bridge: request {id} operation {} at-ms {arrived} duration-ms {} status {}",
                payload.split_whitespace().nth(1).unwrap_or("invalid"),
                started.elapsed().as_millis() - arrived,
                if reply.starts_with("ok") {
                    "ok"
                } else {
                    "error"
                }
            );
            writeln!(stream, "\x1d:byte-reply {id} {reply}")?;
        } else if id.is_some() {
            if !payload.is_empty() || line.len() > 1024 {
                return Err("byte request exceeds carrier framing".into());
            }
            payload.push_str(line);
        } else if line.starts_with(":stdin :byte-reply ") || line == "INPUT QUEUED" {
        } else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread};
    #[test]
    fn arbitrary_bytes_and_eof_cross_the_carrier() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = server.local_addr().unwrap();
        let peer = thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            let mut bytes = [0; 4];
            socket.read_exact(&mut bytes).unwrap();
            assert_eq!(bytes, [0, 255, 10, 128]);
            socket.write_all(&bytes).unwrap();
        });
        let mut carrier = Carrier {
            endpoint,
            stream: None,
        };
        assert_eq!(carrier.exchange("AGEL-BYTES/1 open").unwrap(), "ok");
        assert_eq!(
            carrier.exchange("AGEL-BYTES/1 write 00ff0a80").unwrap(),
            "ok 4"
        );
        let mut returned = Vec::new();
        while returned.len() < 4 {
            let reply = carrier.exchange("AGEL-BYTES/1 read 4").unwrap();
            returned.extend(unhex(reply.strip_prefix("ok ").unwrap()).unwrap());
        }
        assert_eq!(returned, [0, 255, 10, 128]);
        peer.join().unwrap();
        assert_eq!(carrier.exchange("AGEL-BYTES/1 read 4").unwrap(), "ok ");
        assert!(carrier.exchange("AGEL-BYTES/1 read 257").is_err());
        assert!(carrier.exchange("AGEL-BYTES/1 write ff0").is_err());
    }
}
