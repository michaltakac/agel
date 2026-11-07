//! Bounded TCP and entropy bytes for an explicitly configured endpoint.
use std::{
    fmt::Write as _,
    fs::File,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
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
pub struct Carrier {
    endpoint: SocketAddr,
    stream: Option<TcpStream>,
}
impl Carrier {
    pub fn new(endpoint: SocketAddr) -> Self {
        Self {
            endpoint,
            stream: None,
        }
    }
    pub fn exchange(&mut self, request: &str) -> Result<String, String> {
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
