//! Agel plays DOOM, and the loop is Agel in the OS.
//!
//! Since v0.2.74 the perceive-decide-act loop is an Agel program in the
//! desktop's own native evaluator (`boot/desktop/doom-agent.agel`, or
//! `doom-agent-model.agel` when a model decides). The desktop pauses the
//! game, shows the program the window and the engine's state line through
//! its `look` words, asks it which keys to hold, and injects them. This
//! host program is only the bridge the OS reaches through when the Agel
//! loop asks a model: it boots the image, loads the program, runs `:play`,
//! answers each `model-request` the OS prints by calling a provider through
//! Agel's typed, audited `model/infer` effect, and records the run.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use agel_core::ModelRequest;
use agel_model::{ClaudeCodeProvider, CodexProvider, CommandLimits, Provider};

/// The window's content on the screen: where the desktop opens the
/// game's window (slot 0, 640 by 400).
const CONTENT: (usize, usize, usize, usize) = (560, 160, 640, 400);
const SCREEN_WIDTH: usize = 1920;
/// The ASCII frame a model reads: eighty by twenty-five cells.
const ASCII_COLUMNS: usize = 80;
const ASCII_ROWS: usize = 25;
const SHADES: &[u8] = b" .:-=+*#%@";

struct Options {
    image: PathBuf,
    doom: PathBuf,
    wad: PathBuf,
    out: PathBuf,
    steps: usize,
    policy: String,
    hold: usize,
    claude_bin: PathBuf,
    codex_bin: PathBuf,
    model: Option<String>,
}

fn options() -> Result<Options, String> {
    let mut options = Options {
        image: PathBuf::from("target/boot/agel-v1.img"),
        doom: PathBuf::from("boot/posix/target/c/x86_64/doom"),
        wad: PathBuf::from("target/doom1.wad"),
        out: PathBuf::from("target/doom-runs/latest"),
        steps: 8,
        policy: "scripted".to_owned(),
        hold: 4000,
        claude_bin: PathBuf::from("claude"),
        codex_bin: PathBuf::from("codex"),
        model: None,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let mut value = || arguments.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--image" => options.image = PathBuf::from(value()?),
            "--doom" => options.doom = PathBuf::from(value()?),
            "--wad" => options.wad = PathBuf::from(value()?),
            "--out" => options.out = PathBuf::from(value()?),
            "--steps" => options.steps = value()?.parse().map_err(|_| "--steps wants a number")?,
            "--policy" => options.policy = value()?,
            "--hold" => options.hold = value()?.parse().map_err(|_| "--hold wants a number")?,
            "--claude-bin" => options.claude_bin = PathBuf::from(value()?),
            "--codex-bin" => options.codex_bin = PathBuf::from(value()?),
            "--model" => options.model = Some(value()?),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(options)
}

/// The serial console, read on its own thread into a buffer the agent
/// searches; keys go through QMP, never through here, so the workshop's
/// line stays the workshop's.
struct Serial {
    stream: UnixStream,
    received: Arc<Mutex<Vec<u8>>>,
}

impl Serial {
    fn connect(path: &Path) -> Result<Self, String> {
        let stream = connect(path)?;
        let received = Arc::new(Mutex::new(Vec::new()));
        let mut reader = stream.try_clone().map_err(|error| error.to_string())?;
        let sink = Arc::clone(&received);
        thread::spawn(move || {
            let mut chunk = [0_u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => sink
                        .lock()
                        .expect("serial buffer")
                        .extend_from_slice(&chunk[..count]),
                }
            }
        });
        Ok(Self { stream, received })
    }

    /// Wait until `wanted` appears after `from`, and answer where the
    /// buffer ends then.
    fn wait_for(&self, from: usize, wanted: &[u8], timeout: Duration) -> Result<usize, String> {
        let deadline = Instant::now() + timeout;
        loop {
            {
                let buffer = self.received.lock().expect("serial buffer");
                if let Some(at) = find(&buffer[from.min(buffer.len())..], wanted) {
                    return Ok(from + at + wanted.len());
                }
            }
            if Instant::now() > deadline {
                let buffer = self.received.lock().expect("serial buffer");
                let tail = String::from_utf8_lossy(&buffer[buffer.len().saturating_sub(1500)..])
                    .into_owned();
                return Err(format!(
                    "the console did not say {:?}; it ends: {tail}",
                    String::from_utf8_lossy(wanted)
                ));
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn text_from(&self, from: usize) -> String {
        let buffer = self.received.lock().expect("serial buffer");
        String::from_utf8_lossy(&buffer[from.min(buffer.len())..]).into_owned()
    }

    fn len(&self) -> usize {
        self.received.lock().expect("serial buffer").len()
    }

    /// Type a line into the workshop, byte by byte as the console echoes
    /// them, and wait for the prompt; the reply text.
    fn submit(&mut self, line: &str, timeout: Duration) -> Result<String, String> {
        let start = self.len();
        for byte in line.bytes() {
            self.stream
                .write_all(&[byte])
                .map_err(|error| error.to_string())?;
            self.wait_for_byte(byte, Duration::from_secs(5))?;
        }
        self.stream
            .write_all(b"\n")
            .map_err(|error| error.to_string())?;
        let end = self.wait_for(start, b"live-desktop> ", timeout)?;
        Ok(self.text_from(start)[..end - start].to_owned())
    }

    /// Type a line at the workshop prompt, echo-waited, without waiting for
    /// the next prompt: `:play` speaks for a long time before returning.
    fn send_line(&mut self, line: &str) -> Result<(), String> {
        for byte in line.bytes() {
            self.stream
                .write_all(&[byte])
                .map_err(|error| error.to_string())?;
            self.wait_for_byte(byte, Duration::from_secs(5))?;
        }
        self.stream
            .write_all(b"\n")
            .map_err(|error| error.to_string())
    }

    /// Write a line straight to the guest without waiting for an echo: while
    /// `:play` runs, the kernel reads the console without echoing, so the
    /// model reply cannot be echo-waited.
    fn write_raw(&mut self, line: &str) -> Result<(), String> {
        self.stream
            .write_all(line.as_bytes())
            .map_err(|error| error.to_string())?;
        self.stream
            .write_all(b"\n")
            .map_err(|error| error.to_string())
    }

    fn wait_for_byte(&self, byte: u8, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut seen = self.len().saturating_sub(1);
        loop {
            let buffer = self.received.lock().expect("serial buffer");
            if buffer[seen.min(buffer.len())..].contains(&byte) {
                return Ok(());
            }
            seen = buffer.len().saturating_sub(1);
            drop(buffer);
            if Instant::now() > deadline {
                return Err(format!("the console did not echo {byte:#x}"));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn connect(path: &Path) -> Result<UnixStream, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match UnixStream::connect(path) {
            Ok(stream) => return Ok(stream),
            Err(error) if Instant::now() > deadline => {
                return Err(format!("QEMU did not open {}: {error}", path.display()))
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// QEMU's monitor protocol over a socket: JSON in, JSON out, one command
/// at a time.
struct Monitor {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Monitor {
    fn connect(path: &Path) -> Result<Self, String> {
        let stream = connect(path)?;
        let reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
        let mut monitor = Self { stream, reader };
        monitor.read_line()?; // the greeting
        monitor.command(r#"{"execute":"qmp_capabilities"}"#)?;
        Ok(monitor)
    }

    fn read_line(&mut self) -> Result<String, String> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        Ok(line)
    }

    fn command(&mut self, json: &str) -> Result<String, String> {
        self.stream
            .write_all(json.as_bytes())
            .map_err(|error| error.to_string())?;
        self.stream
            .write_all(b"\n")
            .map_err(|error| error.to_string())?;
        loop {
            let line = self.read_line()?;
            if line.contains("\"error\"") {
                return Err(format!("QEMU refused {json}: {line}"));
            }
            if line.contains("\"return\"") {
                return Ok(line);
            }
        }
    }

    fn screendump(&mut self, path: &Path) -> Result<(), String> {
        self.command(&format!(
            r#"{{"execute":"screendump","arguments":{{"filename":"{}","format":"ppm"}}}}"#,
            path.display()
        ))
        .map(|_| ())
    }
}

/// A frame: the window's content as bytes of RGB, from a screendump.
struct Frame {
    rgb: Vec<u8>,
}

impl Frame {
    fn read(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        // P6\n1920 1080\n255\n then the pixels.
        let mut fields = 0;
        let mut at = 0;
        while fields < 3 && at < bytes.len() {
            if bytes[at] == b'\n' {
                fields += 1;
            }
            at += 1;
        }
        let data = &bytes[at..];
        let (x, y, width, height) = CONTENT;
        let mut rgb = Vec::with_capacity(width * height * 3);
        for row in y..y + height {
            let start = (row * SCREEN_WIDTH + x) * 3;
            rgb.extend_from_slice(
                data.get(start..start + width * 3)
                    .ok_or("the screendump is short")?,
            );
        }
        Ok(Self { rgb })
    }

    /// The content as eighty by twenty-five characters of luminance.
    fn ascii(&self) -> String {
        let (_, _, width, height) = CONTENT;
        let cell_w = width / ASCII_COLUMNS;
        let cell_h = height / ASCII_ROWS;
        let mut text = String::with_capacity((ASCII_COLUMNS + 1) * ASCII_ROWS);
        for row in 0..ASCII_ROWS {
            for column in 0..ASCII_COLUMNS {
                let mut sum = 0_u64;
                for py in row * cell_h..(row + 1) * cell_h {
                    for px in column * cell_w..(column + 1) * cell_w {
                        let at = (py * width + px) * 3;
                        let (r, g, b) = (
                            self.rgb[at] as u64,
                            self.rgb[at + 1] as u64,
                            self.rgb[at + 2] as u64,
                        );
                        sum += (r * 299 + g * 587 + b * 114) / 1000;
                    }
                }
                let mean = sum / (cell_w * cell_h) as u64;
                let shade = (mean as usize * (SHADES.len() - 1)) / 255;
                text.push(SHADES[shade] as char);
            }
            text.push('\n');
        }
        text
    }
}

trait Policy {
    fn name(&self) -> &str;

    /// The Agel program the desktop loads to run the loop.
    fn program(&self) -> &str;

    /// Answer a model request the Agel loop made: the text the desktop
    /// printed between `model-request N:` and `model-request end` (the
    /// program's prompt, the engine's state line, and the window as shades).
    /// The action word (`forward back left right fire use`) and a reason, or
    /// `None` for a policy that never asks.
    fn answer(&mut self, _block: &str) -> Option<(String, String)> {
        None
    }
}

/// The scripted policy asks nothing: it loads the plain `doom-agent`, whose
/// forms alone choose every step. It proves the loop is in the OS.
struct Scripted;

impl Policy for Scripted {
    fn name(&self) -> &str {
        "scripted"
    }

    fn program(&self) -> &str {
        "doom-agent"
    }
}

/// A policy that answers instantly with a fixed action, for proving the
/// request/reply round-trip without an external model.
struct Echo {
    action: String,
}

impl Policy for Echo {
    fn name(&self) -> &str {
        "echo"
    }

    fn program(&self) -> &str {
        "doom-agent-model"
    }

    fn answer(&mut self, _block: &str) -> Option<(String, String)> {
        Some((self.action.clone(), "echo policy".to_owned()))
    }
}

/// The words the model program understands; the reply's action must be one.
const WORDS: &[&str] = &["forward", "back", "left", "right", "fire", "use"];

/// A model provider decides, reached through Agel's effect boundary. It loads
/// `doom-agent-model`, whose forms call `model-request`; the bridge answers.
struct Model {
    provider: Box<dyn Provider>,
    next_id: u64,
}

impl Model {
    fn prompt(block: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are playing DOOM (shareware, E1M1) on the Agel operating system, one step at a time. ");
        prompt.push_str("The game is paused while you decide. Below is what the Agel agent in the OS asked, its engine state line, and the window as 64x25 ASCII shades (space dark, @ bright). ");
        prompt.push_str("Choose exactly one action from: ");
        prompt.push_str(&WORDS.join(", "));
        prompt.push_str(". Reply with one line: ACTION: <name> | REASON: <a few words>.\n\n");
        prompt.push_str(block);
        prompt
    }
}

impl Policy for Model {
    fn name(&self) -> &str {
        self.provider.name()
    }

    fn program(&self) -> &str {
        "doom-agent-model"
    }

    fn answer(&mut self, block: &str) -> Option<(String, String)> {
        let prompt = Self::prompt(block);
        let prompt_digest = agel_integrity::sha256(prompt.as_bytes());
        let request = ModelRequest {
            id: self.next_id,
            world_id: 0,
            requester: 0,
            reply_to: 0,
            provider: self.provider.name().to_owned(),
            prompt,
            prompt_digest,
            effect_key: agel_integrity::sha256(
                format!("agel-play:{}:{}", self.next_id, prompt_digest.to_hex()).as_bytes(),
            ),
        };
        self.next_id += 1;
        let (word, reason) = match self.provider.infer(&request) {
            Ok(answer) => {
                let line = answer
                    .lines()
                    .find(|line| line.contains("ACTION:"))
                    .unwrap_or("")
                    .to_owned();
                let name = line
                    .split("ACTION:")
                    .nth(1)
                    .and_then(|rest| rest.split('|').next())
                    .unwrap_or("")
                    .trim()
                    .to_lowercase();
                let reason = line
                    .split("REASON:")
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .replace(['\n', '\r'], " ");
                let word = WORDS
                    .iter()
                    .find(|word| name.starts_with(*word))
                    .copied()
                    .unwrap_or("forward")
                    .to_owned();
                (
                    word,
                    if reason.is_empty() {
                        format!("model said {name}")
                    } else {
                        reason
                    },
                )
            }
            Err(error) => ("forward".to_owned(), format!("provider error: {error}")),
        };
        // A reason on one line, bounded to what the request area holds.
        let reason: String = reason.chars().take(120).collect();
        Some((word, reason))
    }
}

fn main() -> Result<(), String> {
    let options = options()?;
    fs::create_dir_all(&options.out).map_err(|error| error.to_string())?;
    let disk = options.out.join("disk.img");
    fs::copy(&options.image, &disk).map_err(|error| format!("copying the image: {error}"))?;
    {
        // A blank workspace, records and filesystem region, as the tests use.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&disk)
            .map_err(|error| error.to_string())?;
        use std::io::Seek;
        file.seek(std::io::SeekFrom::Start(1024 * 512))
            .map_err(|error| error.to_string())?;
        file.write_all(&vec![0_u8; 1024 * 512])
            .map_err(|error| error.to_string())?;
    }
    run_script(&[
        "scripts/install-program.py",
        disk.to_str().unwrap_or(""),
        "c-doom",
        options.doom.to_str().unwrap_or(""),
    ])?;
    run_script(&[
        "scripts/install-program.py",
        "--region",
        "data",
        disk.to_str().unwrap_or(""),
        "doom1.wad",
        options.wad.to_str().unwrap_or(""),
    ])?;

    let mut policy: Box<dyn Policy> = match options.policy.as_str() {
        "scripted" => Box::new(Scripted),
        "echo" => Box::new(Echo {
            action: "forward".to_owned(),
        }),
        "claude" | "codex" => {
            let mut limits = CommandLimits::new(&options.out);
            limits.timeout = Duration::from_secs(120);
            limits.max_output_bytes = 64 * 1024;
            let provider: Box<dyn Provider> = if options.policy == "claude" {
                let mut provider = ClaudeCodeProvider::new(&options.claude_bin, limits);
                if let Some(model) = &options.model {
                    provider = provider.with_model(model);
                }
                Box::new(provider)
            } else {
                let mut provider = CodexProvider::new(&options.codex_bin, limits);
                if let Some(model) = &options.model {
                    provider = provider.with_model(model);
                }
                Box::new(provider)
            };
            Box::new(Model {
                provider,
                next_id: 1,
            })
        }
        other => return Err(format!("unknown policy {other}; scripted, claude or codex")),
    };

    let sockets = options.out.join("sockets");
    fs::create_dir_all(&sockets).map_err(|error| error.to_string())?;
    let qmp = sockets.join("qmp");
    let serial = sockets.join("serial");
    let _ = fs::remove_file(&qmp);
    let _ = fs::remove_file(&serial);
    let mut qemu = Machine::start(&disk, &qmp, &serial)?;
    let outcome = play(&mut qemu, &qmp, &serial, &options, policy.as_mut());
    qemu.stop();
    outcome
}

fn run_script(arguments: &[&str]) -> Result<(), String> {
    let status = Command::new("python3")
        .args(arguments)
        .stdout(Stdio::null())
        .status()
        .map_err(|error| format!("running python3: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} failed", arguments.join(" ")))
    }
}

struct Machine {
    child: Child,
}

impl Machine {
    fn start(disk: &Path, qmp: &Path, serial: &Path) -> Result<Self, String> {
        let child = Command::new("qemu-system-x86_64")
            .args([
                "-machine",
                "pc,accel=tcg",
                "-m",
                "64M",
                "-display",
                "none",
                "-no-reboot",
                "-vga",
                "std",
            ])
            .args(["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"])
            .args([
                "-qmp",
                &format!("unix:{},server=on,wait=off", qmp.display()),
            ])
            .args([
                "-chardev",
                &format!(
                    "socket,id=serial0,path={},server=on,wait=on",
                    serial.display()
                ),
            ])
            .args(["-serial", "chardev:serial0", "-boot", "order=c,strict=on"])
            .args([
                "-drive",
                &format!(
                    "format=raw,file={},if=ide,index=0,media=disk",
                    disk.display()
                ),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("starting QEMU: {error}"))?;
        Ok(Self { child })
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Boot the desktop, load the Agel program, start the engine, and run the
/// in-OS `:play` loop, answering the model requests it makes and recording
/// each step. The loop is the OS's; this only bridges the model and writes
/// the dataset.
fn play(
    _qemu: &mut Machine,
    qmp: &Path,
    serial_path: &Path,
    options: &Options,
    policy: &mut dyn Policy,
) -> Result<(), String> {
    let mut serial = Serial::connect(serial_path)?;
    let mut monitor = Monitor::connect(qmp)?;
    serial.wait_for(0, b"live-desktop> ", Duration::from_secs(120))?;
    let formatted = serial.submit(":fs-format", Duration::from_secs(30))?;
    if !formatted.contains("formatted") {
        return Err(format!("the filesystem did not format: {formatted}"));
    }
    let loaded = serial.submit(
        &format!(":load {}", policy.program()),
        Duration::from_secs(30),
    )?;
    if !loaded.contains("READY") {
        return Err(format!("the agent did not load: {loaded}"));
    }
    let started = serial.submit(
        ":exec c-doom -- -iwad /data/doom1.wad -mb 8 -warp 1 -skill 2",
        Duration::from_secs(120),
    )?;
    if !started.contains("PROCESS RUNNING") {
        return Err(format!("the game did not start: {started}"));
    }
    serial.wait_for(0, b"doom: frame 0 ", Duration::from_secs(300))?;
    thread::sleep(Duration::from_secs(3));

    let dataset = options.out.join("steps.jsonl");
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&dataset)
        .map_err(|error| error.to_string())?;
    println!(
        "agel-play: {} steps by the {} Agel program ({}) into {}",
        options.steps,
        policy.name(),
        policy.program(),
        options.out.display()
    );

    // Start the in-OS loop; it runs for the whole game, speaking as it goes.
    let mut cursor = serial.len();
    serial.send_line(&format!(":play {} {}", options.steps, options.hold))?;

    let mut block = String::new();
    let mut in_block = false;
    let mut request_number = 0_u64;
    let mut state_line = String::new();
    let deadline = Instant::now() + Duration::from_secs(1800);
    let mut leftover = String::new();
    loop {
        if Instant::now() > deadline {
            return Err("the in-OS play loop did not finish in time".to_owned());
        }
        let fresh = serial.text_from(cursor);
        cursor = serial.len();
        if fresh.is_empty() {
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        leftover.push_str(&fresh);
        let mut done = false;
        while let Some(at) = leftover.find('\n') {
            let line = leftover[..at].trim_end_matches('\r').to_owned();
            leftover = leftover[at + 1..].to_owned();
            if line.starts_with("doom: state") {
                state_line = line.trim_end_matches(" paused").to_owned();
            }
            if line == "model-request end" {
                in_block = false;
                if let Some((word, reason)) = policy.answer(&block) {
                    serial.write_raw(&format!(":model-reply {request_number} {word} {reason}"))?;
                    println!("agel-play: model reply {request_number}: {word} ({reason})");
                } else {
                    serial.write_raw(&format!(":model-reply {request_number} forward none"))?;
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("model-request ") {
                request_number = rest
                    .chars()
                    .take_while(|character| character.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
                block.clear();
                block.push_str(&line);
                block.push('\n');
                in_block = true;
                continue;
            }
            if in_block {
                block.push_str(&line);
                block.push('\n');
                continue;
            }
            if let Some(rest) = line.strip_prefix("play: step ") {
                let index: usize = rest
                    .split(' ')
                    .next()
                    .and_then(|digits| digits.parse().ok())
                    .unwrap_or(0);
                let keys = rest
                    .split("keys ")
                    .nth(1)
                    .and_then(|rest| rest.split(" reason ").next())
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                let reason = rest
                    .split(" reason ")
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                let frame_path = options.out.join(format!("step-{index:04}.ppm"));
                let ascii = match monitor
                    .screendump(&frame_path)
                    .and_then(|_| Frame::read(&frame_path))
                {
                    Ok(frame) => frame.ascii(),
                    Err(_) => String::new(),
                };
                let record = format!(
                    "{{\"step\":{index},\"frame\":{:?},\"policy\":{:?},\"program\":{:?},\"state\":{:?},\"keys\":{:?},\"reason\":{:?},\"ascii\":{:?}}}\n",
                    frame_path.display().to_string(),
                    policy.name(),
                    policy.program(),
                    state_line,
                    keys,
                    reason.trim_matches('"'),
                    ascii
                );
                log.write_all(record.as_bytes())
                    .map_err(|error| error.to_string())?;
                println!("agel-play: step {index}: {keys} [{state_line}]");
            }
            if line.contains("PLAYED") || line.contains("PROCESS ENDED") {
                done = true;
            }
        }
        if done {
            break;
        }
    }
    println!("agel-play: done; the dataset is {}", dataset.display());
    Ok(())
}
