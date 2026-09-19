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
use agel_effects::Principal;
use agel_model::{
    ClaudeCodeProvider, CodexProvider, CommandLimits, JevProvider, JudgmentRequest, Provider,
};

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
    curl_bin: PathBuf,
    model: Option<String>,
    program: Option<String>,
    /// `doom`: the game in a window, `:play`; `desktop`: the desktop
    /// itself, `:drive`, no game installed.
    scene: String,
    /// The task the desktop-driving program is judged against.
    task: String,
    /// Instead of an episode: judge every step of this recorded dataset
    /// with the provider, writing `judged.jsonl` beside it.
    judge_dataset: Option<PathBuf>,
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
        curl_bin: PathBuf::from("curl"),
        model: None,
        program: None,
        scene: "doom".to_owned(),
        task: "list the files in the region, then finish".to_owned(),
        judge_dataset: None,
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
            "--curl-bin" => options.curl_bin = PathBuf::from(value()?),
            "--model" => options.model = Some(value()?),
            "--program" => options.program = Some(value()?),
            "--scene" => {
                options.scene = value()?;
                if options.scene != "doom" && options.scene != "desktop" {
                    return Err("--scene is doom or desktop".to_owned());
                }
            }
            "--task" => options.task = value()?,
            "--judge-dataset" => options.judge_dataset = Some(PathBuf::from(value()?)),
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

    /// Read complete lines and advance their byte cursor under one lock.
    /// Bytes appended afterwards belong to the next read, and a split UTF-8
    /// sequence stays buffered until its line is complete.
    fn take_lines(&self, cursor: &mut usize) -> String {
        let buffer = self.received.lock().expect("serial buffer");
        let start = (*cursor).min(buffer.len());
        let Some(last) = buffer[start..].iter().rposition(|byte| *byte == b'\n') else {
            return String::new();
        };
        *cursor = start + last + 1;
        String::from_utf8_lossy(&buffer[start..*cursor]).into_owned()
    }

    fn len(&self) -> usize {
        self.received.lock().expect("serial buffer").len()
    }

    /// Type a line into the workshop, byte by byte as the console echoes
    /// them, and wait for the prompt; the reply text.
    fn submit(&mut self, line: &str, timeout: Duration) -> Result<String, String> {
        let start = self.len();
        self.send_line(line)?;
        let end = self.wait_for(start, b"live-desktop> ", timeout)?;
        let buffer = self.received.lock().expect("serial buffer");
        Ok(String::from_utf8_lossy(&buffer[start..end]).into_owned())
    }

    /// Type a line at the workshop prompt, echo-waited, without waiting for
    /// the next prompt: `:play` speaks for a long time before returning.
    fn send_line(&mut self, line: &str) -> Result<(), String> {
        for byte in line.bytes() {
            let start = self.len();
            self.stream
                .write_all(&[byte])
                .map_err(|error| error.to_string())?;
            self.wait_for_byte(start, byte, Duration::from_secs(5))?;
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

    fn wait_for_byte(&self, mut seen: usize, byte: u8, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            let buffer = self.received.lock().expect("serial buffer");
            if buffer[seen.min(buffer.len())..].contains(&byte) {
                return Ok(());
            }
            seen = buffer.len();
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
        let count = self
            .reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("QEMU closed the monitor connection".to_owned());
        }
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
            let reply: serde_json::Value =
                serde_json::from_str(&line).map_err(|error| error.to_string())?;
            if reply.get("error").is_some() {
                return Err(format!("QEMU refused {json}: {line}"));
            }
            if reply.get("return").is_some() {
                return Ok(line);
            }
        }
    }

    fn screendump(&mut self, path: &Path) -> Result<(), String> {
        self.command(
            &serde_json::json!({
                "execute": "screendump",
                "arguments": {"filename": path.to_string_lossy(), "format": "ppm"},
            })
            .to_string(),
        )
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
    /// The reply line the program reads — for the model program an action
    /// word (`forward back left right fire use`) and a reason, for the
    /// judge program an answer line — or `None` for a policy that never
    /// asks.
    fn answer(&mut self, _block: &str) -> Option<String> {
        None
    }
}

/// What the desktop printed for one request, taken apart: the program's
/// own text, the engine's state line and the window as shades.
struct Block {
    prompt: String,
    state_line: String,
    frame: String,
    /// What the program decided on earlier in this run, oldest first: the
    /// bridge appends `history: LINE` lines to a desktop block, since a
    /// judge has no memory and the desktop's line shows only the last
    /// answer.
    history: Vec<String>,
}

impl Block {
    fn parse(block: &str) -> Self {
        let mut prompt = String::new();
        let mut state_line = String::new();
        let mut frame = String::new();
        let mut history = Vec::new();
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("history: ") {
                history.push(rest.to_owned());
                continue;
            }
            if let Some(rest) = line.strip_prefix("model-request ") {
                // `model-request N TEXT`: the number, a space, the program's text.
                prompt = rest
                    .trim_start_matches(|character: char| character.is_ascii_digit())
                    .strip_prefix(' ')
                    .unwrap_or("")
                    .to_owned();
            } else if let Some(rest) = line.strip_prefix("look-line: ") {
                state_line = rest.to_owned();
            } else if let Some(rest) = line.strip_prefix("look: ") {
                frame.push_str(rest);
                frame.push('\n');
            }
        }
        Self {
            prompt,
            state_line,
            frame,
            history,
        }
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

    fn answer(&mut self, _block: &str) -> Option<String> {
        Some(format!("{} echo policy", self.action))
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

    fn answer(&mut self, block: &str) -> Option<String> {
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
        Some(format!("{word} {reason}"))
    }
}

/// A System One model judges, through the `jev` provider: what the OS
/// printed becomes the state (the engine's state line and the window as
/// shades, as named fields), and the questions are the program's own when
/// it sent a `(judge ...)` form — then the reply is the answer line the
/// judge program reads — or, for the model program's plain prompt, one
/// choice among the action words, answered as `WORD reason`. Policy stays
/// in the Agel program; here is only the carrying.
struct Judge {
    provider: JevProvider,
    program: String,
    /// The task, when the program drives the desktop rather than the game:
    /// the judge sees it beside the desktop's own line and the shades.
    task: Option<String>,
    next_id: u64,
}

impl Judge {
    fn request(block: &Block, task: Option<&str>) -> Result<JudgmentRequest, String> {
        if let Some(task) = task {
            let mut request = JudgmentRequest::parse(&block.prompt)?;
            let history = if block.history.is_empty() {
                "nothing yet: this is the first step".to_owned()
            } else {
                block.history.join("; ")
            };
            request.add_fields([
                ("task".to_owned(), task.to_owned()),
                ("desktop".to_owned(), format!("The Agel desktop, as its own status line: the windows by slot with their titles, the focus, whether a process runs, and the last line its terminal finished. {}", block.state_line)),
                ("history".to_owned(), format!("The commands already typed in this run, oldest first; each shows what it showed when typed, so a command in the history need not be typed again: {history}")),
                ("frame".to_owned(), block.frame.clone()),
            ]);
            return Ok(request);
        }
        let mut request = if block.prompt.trim_start().starts_with("(judge") {
            JudgmentRequest::parse(&block.prompt)?
        } else {
            let criteria = WORDS
                .iter()
                .map(|word| format!("({word} \"hold {word}\")"))
                .collect::<Vec<_>>()
                .join(" ");
            JudgmentRequest::parse(&format!(
                "(judge (state (asked {:?})) (choice act \"Which one action should the player take now, given `state_line` and the window `frame`?\" {criteria}))",
                block.prompt
            ))?
        };
        request.add_fields([
            ("game".to_owned(), "DOOM shareware E1M1 on the Agel desktop; the game is paused while you decide. frame is the window as 64x25 luminance shades, space dark to @ bright.".to_owned()),
            ("state_line".to_owned(), block.state_line.clone()),
            ("history".to_owned(), if block.history.is_empty() {
                "the first step".to_owned()
            } else {
                format!("the last steps, oldest first, each the keys held and the state line before them. A position (x, y) that does not change while forward is held means a wall ahead: turn once. An angle that has already changed since then means the turn is done: go forward again rather than keep turning. {}", block.history.join("; "))
            }),
            ("frame".to_owned(), block.frame.clone()),
        ]);
        Ok(request)
    }
}

impl Policy for Judge {
    fn name(&self) -> &str {
        "jev"
    }

    fn program(&self) -> &str {
        &self.program
    }

    fn answer(&mut self, block: &str) -> Option<String> {
        let block = Block::parse(block);
        let typed = block.prompt.trim_start().starts_with("(judge");
        let id = self.next_id;
        self.next_id += 1;
        let judgment = Judge::request(&block, self.task.as_deref()).and_then(|request| {
            self.provider
                .judge(
                    &request,
                    Principal {
                        world: 0,
                        agent: Some(0),
                    },
                    &format!("model/infer/jev/request/{id}"),
                )
                .map_err(|error| error.to_string())
        });
        Some(match judgment {
            Ok(judgment) if typed => judgment.reply_text(),
            Ok(judgment) => match &judgment.answers[0].1 {
                agel_model::Answer::Choice {
                    choice,
                    confidence,
                    probabilities,
                } => format!(
                    "{choice} {} chose {choice} at {}% confidence: {}",
                    judgment.model,
                    agel_model::systemone::thousandths(*confidence) / 10,
                    WORDS
                        .iter()
                        .zip(probabilities)
                        .map(|(word, p)| format!(
                            "{word} {}%",
                            agel_model::systemone::thousandths(*p) / 10
                        ))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                _ => "forward the judge answered another kind".to_owned(),
            },
            // No answer is an answer the program can read: the model
            // program falls back to its default, the judge program to its
            // own policy, and the run records why.
            Err(error) if typed => {
                let reason: String = error.replace(['\n', '\r'], " ").chars().take(120).collect();
                format!("error {reason}")
            }
            Err(error) => {
                let reason: String = error.replace(['\n', '\r'], " ").chars().take(120).collect();
                format!("forward provider error: {reason}")
            }
        })
    }
}

fn main() -> Result<(), String> {
    let options = options()?;
    fs::create_dir_all(&options.out).map_err(|error| error.to_string())?;
    if let Some(dataset) = &options.judge_dataset {
        return judge_dataset(&options, dataset);
    }
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
    if options.scene == "doom" {
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
    }

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
        "jev" => {
            let key = std::env::var(agel_model::systemone::KEY_VARIABLE).map_err(|_| {
                format!(
                    "--policy jev needs {} in the environment",
                    agel_model::systemone::KEY_VARIABLE
                )
            })?;
            let mut limits = CommandLimits::new(&options.out);
            limits.timeout = Duration::from_secs(30);
            limits.max_output_bytes = 64 * 1024;
            let mut provider = JevProvider::new(&options.curl_bin, key, limits);
            if let Some(model) = &options.model {
                provider = provider.with_model(model);
            }
            let desktop = options.scene == "desktop";
            Box::new(Judge {
                provider,
                program: options.program.clone().unwrap_or_else(|| {
                    if desktop {
                        "desktop-agent".to_owned()
                    } else {
                        "doom-agent-judge".to_owned()
                    }
                }),
                task: desktop.then(|| options.task.clone()),
                next_id: 1,
            })
        }
        other => {
            return Err(format!(
                "unknown policy {other}; scripted, echo, claude, codex or jev"
            ))
        }
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

/// The model as a labeler: every step of a recorded episode judged after
/// the fact — was holding those keys a good move in that state, and how
/// was the player faring — with the same provider, into `judged.jsonl`
/// beside the dataset, and a summary on the console. Nothing is booted.
fn judge_dataset(options: &Options, dataset: &Path) -> Result<(), String> {
    if options.policy != "jev" {
        return Err("--judge-dataset needs --policy jev".to_owned());
    }
    let key = std::env::var(agel_model::systemone::KEY_VARIABLE).map_err(|_| {
        format!(
            "--policy jev needs {} in the environment",
            agel_model::systemone::KEY_VARIABLE
        )
    })?;
    let mut limits = CommandLimits::new(&options.out);
    limits.timeout = Duration::from_secs(30);
    limits.max_output_bytes = 64 * 1024;
    let mut provider = JevProvider::new(&options.curl_bin, key, limits);
    if let Some(model) = &options.model {
        provider = provider.with_model(model);
    }
    let text =
        fs::read_to_string(dataset).map_err(|error| format!("{}: {error}", dataset.display()))?;
    let judged_path = dataset.with_file_name("judged.jsonl");
    let mut judged = fs::File::create(&judged_path).map_err(|error| error.to_string())?;
    let (mut count, mut good_total, mut faring_total) = (0_i64, 0_i64, 0_i64);
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let record: serde_json::Value = serde_json::from_str(line)
            .map_err(|error| format!("{}: {error}", dataset.display()))?;
        let step = record["step"].as_u64().unwrap_or(0);
        let keys = record["keys"].as_str().unwrap_or("").to_owned();
        let mut request = JudgmentRequest::parse(
            "(judge (noul good \"Was holding these keys a good move for the player in this state?\" \"a good move: it advances, fights or escapes sensibly\" \"a poor move: it wastes the step, walks into a wall or into danger\") (score faring \"How is the player faring at this step?\" \"losing\" \"even\" \"winning\"))",
        )?;
        request.add_fields([
            ("game".to_owned(), "DOOM shareware E1M1 on the Agel desktop, a recorded episode judged after the fact; frame is the window as 80x25 luminance shades, space dark to @ bright.".to_owned()),
            ("state_line".to_owned(), record["state"].as_str().unwrap_or("").to_owned()),
            ("keys_held".to_owned(), keys.clone()),
            ("reason_given".to_owned(), record["reason"].as_str().unwrap_or("").to_owned()),
            ("frame".to_owned(), record["ascii"].as_str().unwrap_or("").to_owned()),
        ]);
        let judgment = provider
            .judge(
                &request,
                Principal {
                    world: 0,
                    agent: Some(0),
                },
                &format!("model/infer/jev/request/judged-{step}"),
            )
            .map_err(|error| format!("step {step}: {error}"))?;
        let mut good = 0;
        let mut faring = 0;
        for (id, answer) in &judgment.answers {
            match (id.as_str(), answer) {
                ("good", agel_model::Answer::Noul { yes }) => {
                    good = agel_model::systemone::thousandths(*yes)
                }
                ("faring", agel_model::Answer::Score { score, .. }) => {
                    faring = agel_model::systemone::level_thousandths(*score)
                }
                _ => {}
            }
        }
        count += 1;
        good_total += good;
        faring_total += faring;
        let line = judgment.reply_text();
        println!("agel-play: judged step {step}: {keys} good {good} faring {faring}");
        let out = serde_json::json!({
            "step": step,
            "keys": keys,
            "good": good,
            "faring": faring,
            "judgment": line,
            "model": judgment.model,
        })
        .to_string()
            + "\n";
        judged
            .write_all(out.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    if count == 0 {
        return Err(format!("{} holds no steps", dataset.display()));
    }
    println!(
        "agel-play: judged {count} steps: mean good {} thousandths, mean faring {} thousandths of a level; the judgments are in {}",
        good_total / count,
        faring_total / count,
        judged_path.display()
    );
    Ok(())
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
    let desktop = options.scene == "desktop";
    if !desktop {
        let started = serial.submit(
            ":exec c-doom -- -iwad /data/doom1.wad -mb 8 -warp 1 -skill 2",
            Duration::from_secs(120),
        )?;
        if !started.contains("PROCESS RUNNING") {
            return Err(format!("the game did not start: {started}"));
        }
        serial.wait_for(0, b"doom: frame 0 ", Duration::from_secs(300))?;
        thread::sleep(Duration::from_secs(3));
    }

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
    let loop_word = if desktop { ":drive" } else { ":play" };
    serial.send_line(&format!("{loop_word} {} {}", options.steps, options.hold))?;

    let mut block = String::new();
    let mut in_block = false;
    let mut request_number = 0_u64;
    let mut state_line = String::new();
    let mut history: Vec<String> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(1800);
    loop {
        if Instant::now() > deadline {
            return Err("the in-OS play loop did not finish in time".to_owned());
        }
        let fresh = serial.take_lines(&mut cursor);
        if fresh.is_empty() {
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        let mut done = false;
        for line in fresh.lines() {
            let line = line.trim_end_matches('\r');
            if line.starts_with("doom: state") {
                state_line = line.trim_end_matches(" paused").to_owned();
            }
            if line == "model-request end" {
                in_block = false;
                // A judge has no memory: what this run has done so far goes
                // in with the block, oldest first — all of it for the
                // desktop, the last eight steps for the game.
                let recent = if desktop {
                    0
                } else {
                    history.len().saturating_sub(8)
                };
                for decided in &history[recent..] {
                    block.push_str("history: ");
                    block.push_str(decided);
                    block.push('\n');
                }
                if let Some(reply) = policy.answer(&block) {
                    serial.write_raw(&format!(":model-reply {request_number} {reply}"))?;
                    println!("agel-play: model reply {request_number}: {reply}");
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
                block.push_str(line);
                block.push('\n');
                in_block = true;
                continue;
            }
            if in_block {
                block.push_str(line);
                block.push('\n');
                continue;
            }
            // `play: step N keys FORM reason R` from the game's loop, or
            // `drive: step N do LINE reason R` from the desktop's: the
            // decision is recorded either way.
            let stepped = line
                .strip_prefix("play: step ")
                .map(|rest| (rest, "keys "))
                .or_else(|| line.strip_prefix("drive: step ").map(|rest| (rest, "do ")));
            if let Some((rest, decided)) = stepped {
                let index: usize = rest
                    .split(' ')
                    .next()
                    .and_then(|digits| digits.parse().ok())
                    .unwrap_or(0);
                let keys = rest
                    .split(decided)
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
                history.push(if desktop {
                    format!("step {index}: {keys}")
                } else {
                    format!("step {index}: held {keys} at {state_line}")
                });
                let frame_path = options.out.join(format!("step-{index:04}.ppm"));
                let ascii = match monitor
                    .screendump(&frame_path)
                    .and_then(|_| Frame::read(&frame_path))
                {
                    Ok(frame) => frame.ascii(),
                    Err(_) => String::new(),
                };
                let record = serde_json::json!({
                    "step": index,
                    "frame": frame_path.to_string_lossy(),
                    "policy": policy.name(),
                    "program": policy.program(),
                    "state": state_line,
                    "keys": keys,
                    "reason": reason.trim_matches('"'),
                    "ascii": ascii,
                })
                .to_string()
                    + "\n";
                log.write_all(record.as_bytes())
                    .map_err(|error| error.to_string())?;
                println!("agel-play: step {index}: {keys} [{state_line}]");
            }
            if line.contains("PLAYED")
                || line.contains("PROCESS ENDED")
                || line.starts_with("DROVE ")
                || line.starts_with("DRIVE DONE")
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_keeps_incomplete_lines_and_split_utf8() {
        let (stream, _peer) = UnixStream::pair().unwrap();
        let serial = Serial {
            stream,
            received: Arc::new(Mutex::new(b"first\n\xc3".to_vec())),
        };
        let mut cursor = 0;
        assert_eq!(serial.take_lines(&mut cursor), "first\n");
        assert_eq!(serial.take_lines(&mut cursor), "");
        serial
            .received
            .lock()
            .unwrap()
            .extend_from_slice(b"\xa9\nnext\n");
        assert_eq!(serial.take_lines(&mut cursor), "é\nnext\n");
        assert_eq!(serial.take_lines(&mut cursor), "");
        assert!(serial.wait_for_byte(cursor, b'\n', Duration::ZERO).is_err());
        serial.received.lock().unwrap().push(b'\n');
        assert!(serial.wait_for_byte(cursor, b'\n', Duration::ZERO).is_ok());
    }

    #[test]
    fn monitor_escapes_paths_ignores_events_and_reports_eof() {
        let (stream, mut peer) = UnixStream::pair().unwrap();
        let mut monitor = Monitor {
            reader: BufReader::new(stream.try_clone().unwrap()),
            stream,
        };
        let path = "frame\"\\\t.ppm";
        let server = thread::spawn(move || {
            let mut reader = BufReader::new(peer.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["arguments"]["filename"], path);
            peer.write_all(
                b"{\"event\":\"notice\",\"data\":{\"error\":\"irrelevant\"}}\n{\"return\":{}}\n",
            )
            .unwrap();
        });
        monitor.screendump(Path::new(path)).unwrap();
        server.join().unwrap();
        assert!(monitor.read_line().unwrap_err().contains("closed"));
    }
}
