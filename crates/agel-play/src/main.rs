//! Agel plays DOOM.
//!
//! The game runs on Agel's own kernel in QEMU; this program is the hosted
//! agent of `docs/doom.md`: it boots the desktop image, starts the engine
//! from the workshop, and then, step by step, pauses the game, reads the
//! screen back, decides, unpauses and holds keys. Decisions come from a
//! scripted policy (for tests, with no model) or from a model provider
//! through Agel's typed, audited `model/infer` effect. Every step is
//! appended to a dataset: the frame, its ASCII rendering, the engine's own
//! state line, the action, the reason.

use std::collections::VecDeque;
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
    hold_ms: u64,
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
        steps: 10,
        policy: "scripted".to_owned(),
        hold_ms: 300,
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
            "--hold-ms" => {
                options.hold_ms = value()?.parse().map_err(|_| "--hold-ms wants a number")?
            }
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

    fn key(&mut self, qcode: &str, down: bool) -> Result<(), String> {
        self.command(&format!(
            r#"{{"execute":"input-send-event","arguments":{{"events":[{{"type":"key","data":{{"down":{down},"key":{{"type":"qcode","data":"{qcode}"}}}}}}]}}}}"#
        ))
        .map(|_| ())
    }

    fn tap(&mut self, qcode: &str) -> Result<(), String> {
        self.key(qcode, true)?;
        thread::sleep(Duration::from_millis(60));
        self.key(qcode, false)
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

    /// One flat colour over most of the content: the window's own surface
    /// with nothing drawn, or a screendump that caught the compositor
    /// between the surface and the frame.
    fn is_plain(&self) -> bool {
        let first = &self.rgb[..3];
        self.rgb.chunks(3).filter(|pixel| *pixel == first).count() * 10 > self.rgb.len() / 3 * 9
    }
}

/// What the agent may do: the keys held for one step.
#[derive(Clone, Copy)]
struct Action {
    name: &'static str,
    keys: &'static [&'static str],
}

const ACTIONS: &[Action] = &[
    Action {
        name: "forward",
        keys: &["up"],
    },
    Action {
        name: "back",
        keys: &["down"],
    },
    Action {
        name: "turn-left",
        keys: &["left"],
    },
    Action {
        name: "turn-right",
        keys: &["right"],
    },
    Action {
        name: "strafe-left",
        keys: &["alt", "left"],
    },
    Action {
        name: "strafe-right",
        keys: &["alt", "right"],
    },
    Action {
        name: "fire",
        keys: &["ctrl"],
    },
    Action {
        name: "forward-fire",
        keys: &["up", "ctrl"],
    },
    Action {
        name: "use",
        keys: &["spc"],
    },
    Action {
        name: "wait",
        keys: &[],
    },
];

fn action_named(name: &str) -> Option<Action> {
    ACTIONS
        .iter()
        .copied()
        .find(|action| action.name == name.trim())
}

/// The engine's own account of the player, from its heartbeat.
#[derive(Clone, Default)]
struct State {
    line: String,
}

struct Step {
    index: usize,
    frame: PathBuf,
    state: State,
    action: Action,
    reason: String,
}

trait Policy {
    fn name(&self) -> &str;
    fn decide(&mut self, ascii: &str, state: &State, history: &VecDeque<Step>) -> (Action, String);
}

/// A fixed dance for tests: it proves the loop without a model.
struct Scripted {
    at: usize,
}

impl Policy for Scripted {
    fn name(&self) -> &str {
        "scripted"
    }

    fn decide(
        &mut self,
        _ascii: &str,
        _state: &State,
        _history: &VecDeque<Step>,
    ) -> (Action, String) {
        const DANCE: &[&str] = &[
            "forward",
            "forward",
            "turn-left",
            "forward",
            "fire",
            "turn-right",
            "forward",
            "use",
        ];
        let name = DANCE[self.at % DANCE.len()];
        self.at += 1;
        (
            action_named(name).expect("a scripted action"),
            format!("scripted step {}", self.at),
        )
    }
}

/// A model provider behind Agel's effect boundary decides from the ASCII
/// frame, the engine's state and the last steps.
struct Model {
    provider: Box<dyn Provider>,
    next_id: u64,
}

impl Model {
    fn prompt(ascii: &str, state: &State, history: &VecDeque<Step>) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are playing DOOM (shareware, E1M1) on the Agel operating system, one step at a time. ");
        prompt.push_str("The game is paused while you decide. Below is the screen as 80x25 ASCII shades (space is dark, @ is bright), the engine's state line, and your recent steps. ");
        prompt.push_str("Choose exactly one action from: ");
        prompt.push_str(
            &ACTIONS
                .iter()
                .map(|action| action.name)
                .collect::<Vec<_>>()
                .join(", "),
        );
        prompt
            .push_str(". Reply with one line: ACTION: <name> | REASON: <a few words>.\n\nSTATE: ");
        prompt.push_str(&state.line);
        prompt.push_str("\n\nRECENT:\n");
        for step in history
            .iter()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            prompt.push_str(&format!(
                "- step {}: {} ({}); then {}\n",
                step.index, step.action.name, step.reason, step.state.line
            ));
        }
        prompt.push_str("\nSCREEN:\n");
        prompt.push_str(ascii);
        prompt
    }
}

impl Policy for Model {
    fn name(&self) -> &str {
        self.provider.name()
    }

    fn decide(&mut self, ascii: &str, state: &State, history: &VecDeque<Step>) -> (Action, String) {
        let prompt = Self::prompt(ascii, state, history);
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
        match self.provider.infer(&request) {
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
                    .to_owned();
                let reason = line.split("REASON:").nth(1).unwrap_or("").trim().to_owned();
                match action_named(&name) {
                    Some(action) => (action, reason),
                    None => (
                        action_named("forward").expect("forward"),
                        format!("unparsed answer: {}", answer.trim()),
                    ),
                }
            }
            Err(error) => (
                action_named("wait").expect("wait"),
                format!("provider error: {error}"),
            ),
        }
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
        "scripted" => Box::new(Scripted { at: 0 }),
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
    let started = serial.submit(
        ":exec c-doom -- -iwad /data/doom1.wad -mb 8 -warp 1 -skill 2",
        Duration::from_secs(120),
    )?;
    if !started.contains("PROCESS RUNNING") {
        return Err(format!("the game did not start: {started}"));
    }
    serial.wait_for(0, b"doom: frame 0 ", Duration::from_secs(300))?;
    thread::sleep(Duration::from_secs(3));
    let mut history: VecDeque<Step> = VecDeque::new();
    let dataset = options.out.join("steps.jsonl");
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&dataset)
        .map_err(|error| error.to_string())?;
    println!(
        "agel-play: {} steps by the {} policy into {}",
        options.steps,
        policy.name(),
        options.out.display()
    );
    for index in 0..options.steps {
        // Pause, so the model's time is not the game's; the engine says
        // where the player stands as it pauses, and the panel under the
        // window is repainted for that line before anything is captured.
        let before = serial.len();
        monitor.tap("p")?;
        serial.wait_for(before, b" paused\r\n", Duration::from_secs(20))?;
        thread::sleep(Duration::from_millis(700));
        // A frame the compositor was not in the middle of painting: two
        // captures a quarter second apart that agree, neither one flat.
        let frame_path = options.out.join(format!("step-{index:04}.ppm"));
        let probe_path = options.out.join("probe.ppm");
        let mut frame = None;
        for _ in 0..8 {
            monitor.screendump(&probe_path)?;
            thread::sleep(Duration::from_millis(250));
            monitor.screendump(&frame_path)?;
            thread::sleep(Duration::from_millis(100));
            let first = Frame::read(&probe_path)?;
            let second = Frame::read(&frame_path)?;
            if !second.is_plain() && first.rgb == second.rgb {
                frame = Some(second);
                break;
            }
        }
        let Some(frame) = frame else {
            return Err(format!(
                "the game's window never held still: nothing is being drawn, or every capture caught a repaint; the console ends: {}",
                serial.text_from(serial.len().saturating_sub(1500))
            ));
        };
        let text = serial.text_from(before);
        let state = State {
            line: text
                .lines()
                .rev()
                .find(|line| line.starts_with("doom: state"))
                .unwrap_or("")
                .trim_end_matches(" paused")
                .to_owned(),
        };
        let ascii = frame.ascii();
        let (action, reason) = policy.decide(&ascii, &state, &history);
        // Unpause and act: the keys held for the step's length.
        monitor.tap("p")?;
        for key in action.keys {
            monitor.key(key, true)?;
        }
        thread::sleep(Duration::from_millis(options.hold_ms));
        for key in action.keys.iter().rev() {
            monitor.key(key, false)?;
        }
        thread::sleep(Duration::from_millis(150));
        println!(
            "agel-play: step {index}: {} ({}) [{}]",
            action.name, reason, state.line
        );
        let record = format!(
            "{{\"step\":{index},\"frame\":{:?},\"policy\":{:?},\"state\":{:?},\"action\":{:?},\"reason\":{:?},\"ascii\":{:?}}}\n",
            frame_path.display().to_string(),
            policy.name(),
            state.line,
            action.name,
            reason,
            ascii
        );
        log.write_all(record.as_bytes())
            .map_err(|error| error.to_string())?;
        history.push_back(Step {
            index,
            frame: frame_path,
            state,
            action,
            reason,
        });
        if history.len() > 32 {
            history.pop_front();
        }
    }
    let last = history
        .back()
        .map(|step| step.frame.display().to_string())
        .unwrap_or_default();
    println!(
        "agel-play: done; the last frame is {last} and the dataset {}",
        dataset.display()
    );
    Ok(())
}
