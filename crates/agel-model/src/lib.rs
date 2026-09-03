use agel_core::{ModelOutcome, ModelRequest};
use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct CommandLimits {
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub workspace: PathBuf,
}

impl CommandLimits {
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_output_bytes: 1_048_576,
            workspace: workspace.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderError {
    Io(String),
    TimedOut,
    OutputLimitExceeded,
    Failed { code: Option<i32>, stderr: String },
    InvalidUtf8,
    UnknownProvider(String),
}

impl ProviderError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "provider/io",
            Self::TimedOut => "provider/timeout",
            Self::OutputLimitExceeded => "provider/output-limit",
            Self::Failed { .. } => "provider/failed",
            Self::InvalidUtf8 => "provider/invalid-utf8",
            Self::UnknownProvider(_) => "provider/unknown",
        }
    }

    pub fn into_outcome(self) -> ModelOutcome {
        ModelOutcome::Failure {
            kind: self.kind().into(),
            message: self.to_string(),
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "provider process I/O failed: {message}"),
            Self::TimedOut => f.write_str("provider process timed out"),
            Self::OutputLimitExceeded => f.write_str("provider output exceeded its byte limit"),
            Self::Failed { code, stderr } => {
                write!(f, "provider exited with status {code:?}")?;
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
            Self::InvalidUtf8 => f.write_str("provider returned non-UTF-8 output"),
            Self::UnknownProvider(name) => write!(f, "provider is not enabled: {name}"),
        }
    }
}

impl std::error::Error for ProviderError {}

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn infer(&self, request: &ModelRequest) -> Result<String, ProviderError>;
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, Box<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn register(&mut self, provider: impl Provider + 'static) {
        self.providers
            .insert(provider.name().to_owned(), Box::new(provider));
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.providers.keys().map(String::as_str)
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    pub fn infer(&self, request: &ModelRequest) -> Result<String, ProviderError> {
        self.providers
            .get(&request.provider)
            .ok_or_else(|| ProviderError::UnknownProvider(request.provider.clone()))?
            .infer(request)
    }
}

#[derive(Clone, Debug)]
pub struct ClaudeCodeProvider {
    executable: PathBuf,
    model: Option<String>,
    max_budget_usd: Option<String>,
    limits: CommandLimits,
}

impl ClaudeCodeProvider {
    pub fn new(executable: impl Into<PathBuf>, limits: CommandLimits) -> Self {
        Self {
            executable: executable.into(),
            model: None,
            max_budget_usd: None,
            limits,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_max_budget_usd(mut self, amount: impl Into<String>) -> Self {
        self.max_budget_usd = Some(amount.into());
        self
    }
}

impl Provider for ClaudeCodeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn infer(&self, request: &ModelRequest) -> Result<String, ProviderError> {
        let mut args = vec![
            "--print".into(),
            "--output-format".into(),
            "text".into(),
            "--no-session-persistence".into(),
            "--restricted".into(),
            "--permission-prompts".into(),
            "none".into(),
        ];
        if let Some(model) = &self.model {
            args.extend(["--model".into(), model.clone()]);
        }
        if let Some(amount) = &self.max_budget_usd {
            args.extend(["--max-budget-usd".into(), amount.clone()]);
        }
        run_command(&self.executable, &args, &request.prompt, &self.limits)
    }
}

#[derive(Clone, Debug)]
pub struct CodexProvider {
    executable: PathBuf,
    model: Option<String>,
    limits: CommandLimits,
}

impl CodexProvider {
    pub fn new(executable: impl Into<PathBuf>, limits: CommandLimits) -> Self {
        Self {
            executable: executable.into(),
            model: None,
            limits,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

impl Provider for CodexProvider {
    fn name(&self) -> &str {
        "codex"
    }

    fn infer(&self, request: &ModelRequest) -> Result<String, ProviderError> {
        let mut args = vec![
            "exec".into(),
            "--ephemeral".into(),
            "--ignore-user-config".into(),
            "--sandbox".into(),
            "read-only".into(),
            "--color".into(),
            "never".into(),
            "--skip-git-repo-check".into(),
            "--cd".into(),
            self.limits.workspace.to_string_lossy().into_owned(),
        ];
        if let Some(model) = &self.model {
            args.extend(["--model".into(), model.clone()]);
        }
        args.push("-".into());
        run_command(&self.executable, &args, &request.prompt, &self.limits)
    }
}

fn run_command(
    executable: &Path,
    arguments: &[String],
    prompt: &str,
    limits: &CommandLimits,
) -> Result<String, ProviderError> {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(&limits.workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = command.spawn().map_err(io_error)?;

    let mut stdin = child.stdin.take().expect("piped stdin exists");
    let prompt = prompt.as_bytes().to_vec();
    let input = thread::spawn(move || stdin.write_all(&prompt));
    let stdout = child.stdout.take().expect("piped stdout exists");
    let stderr = child.stderr.take().expect("piped stderr exists");
    let output_limit = limits.max_output_bytes;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, output_limit));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, output_limit));

    let deadline = Instant::now()
        .checked_add(limits.timeout)
        .ok_or_else(|| ProviderError::Io("provider timeout is out of range".into()))?;
    let status = loop {
        match child.try_wait().map_err(io_error)? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                terminate_process_group(&mut child);
                join_input(input)?;
                let _ = join_reader(stdout_reader)?;
                let _ = join_reader(stderr_reader)?;
                return Err(ProviderError::TimedOut);
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    };
    join_input(input)?;
    let (stdout, stdout_exceeded) = join_reader(stdout_reader)?;
    let (stderr, stderr_exceeded) = join_reader(stderr_reader)?;
    if stdout_exceeded || stderr_exceeded {
        return Err(ProviderError::OutputLimitExceeded);
    }
    let stderr = String::from_utf8(stderr).map_err(|_| ProviderError::InvalidUtf8)?;
    if !status.success() {
        return Err(ProviderError::Failed {
            code: status.code(),
            stderr: stderr.trim().to_owned(),
        });
    }
    String::from_utf8(stdout)
        .map(|text| text.trim_end().to_owned())
        .map_err(|_| ProviderError::InvalidUtf8)
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut retained = Vec::new();
    let mut exceeded = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..read.min(remaining)]);
        exceeded |= read > remaining;
    }
    Ok((retained, exceeded))
}

fn join_input(handle: thread::JoinHandle<io::Result<()>>) -> Result<(), ProviderError> {
    handle
        .join()
        .map_err(|_| ProviderError::Io("stdin writer thread panicked".into()))?
        .map_err(io_error)
}

fn join_reader(
    handle: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
) -> Result<(Vec<u8>, bool), ProviderError> {
    handle
        .join()
        .map_err(|_| ProviderError::Io("output reader thread panicked".into()))?
        .map_err(io_error)
}

fn io_error(error: io::Error) -> ProviderError {
    ProviderError::Io(error.to_string())
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_group(child: &mut Child) {
    let group = format!("-{}", child.id());
    let _ = Command::new("/bin/kill").args(["-KILL", &group]).status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_process_group(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    fn fixture(script: &str) -> (PathBuf, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "agel-model-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let executable = directory.join("provider");
        fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&executable, permissions).unwrap();
        (directory, executable)
    }

    fn request(provider: &str) -> ModelRequest {
        ModelRequest {
            id: 7,
            requester: 1,
            reply_to: 1,
            provider: provider.into(),
            prompt: "explain (cons 'agent future)".into(),
        }
    }

    #[test]
    fn claude_is_noninteractive_restricted_and_receives_prompt_on_stdin() {
        let (directory, executable) = fixture(
            "printf 'ARGS:'; for arg in \"$@\"; do printf '<%s>' \"$arg\"; done; printf '\\nPROMPT:'; cat",
        );
        let provider = ClaudeCodeProvider::new(&executable, CommandLimits::new(&directory))
            .with_model("sonnet")
            .with_max_budget_usd("0.25");
        let output = provider.infer(&request("claude")).unwrap();
        assert!(output.contains("<--print><--output-format><text>"));
        assert!(output.contains("<--no-session-persistence><--restricted>"));
        assert!(output.contains("<--permission-prompts><none>"));
        assert!(output.contains("<--model><sonnet><--max-budget-usd><0.25>"));
        assert!(output.ends_with("PROMPT:explain (cons 'agent future)"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn codex_is_ephemeral_read_only_and_receives_prompt_on_stdin() {
        let (directory, executable) = fixture(
            "printf 'ARGS:'; for arg in \"$@\"; do printf '<%s>' \"$arg\"; done; printf '\\nPROMPT:'; cat",
        );
        let provider =
            CodexProvider::new(&executable, CommandLimits::new(&directory)).with_model("gpt-test");
        let output = provider.infer(&request("codex")).unwrap();
        assert!(output.contains("<exec><--ephemeral><--ignore-user-config>"));
        assert!(output.contains("<--sandbox><read-only><--color><never>"));
        assert!(output.contains("<--skip-git-repo-check><--cd>"));
        assert!(output.contains("<--model><gpt-test><->"));
        assert!(output.ends_with("PROMPT:explain (cons 'agent future)"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn process_timeout_and_output_limits_are_enforced() {
        let (timeout_dir, timeout_executable) = fixture("while :; do :; done");
        let mut limits = CommandLimits::new(&timeout_dir);
        limits.timeout = Duration::from_millis(20);
        let timeout = ClaudeCodeProvider::new(&timeout_executable, limits)
            .infer(&request("claude"))
            .unwrap_err();
        assert_eq!(timeout, ProviderError::TimedOut);
        fs::remove_dir_all(timeout_dir).unwrap();

        let (output_dir, output_executable) = fixture("printf 123456789");
        let mut limits = CommandLimits::new(&output_dir);
        limits.max_output_bytes = 4;
        let excessive = ClaudeCodeProvider::new(&output_executable, limits)
            .infer(&request("claude"))
            .unwrap_err();
        assert_eq!(excessive, ProviderError::OutputLimitExceeded);
        fs::remove_dir_all(output_dir).unwrap();
    }
}
