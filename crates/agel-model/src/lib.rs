use agel_core::{ModelOutcome, ModelRequest};
use agel_effects::{EffectError, Principal, ProcessSandbox, ProcessSpec};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

pub use agel_effects::ProcessLimits as CommandLimits;
pub use agel_effects::{AuditOutcome, AuditRecord};

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

    fn audit_records(&self) -> Vec<AuditRecord> {
        Vec::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderAuditRecord {
    pub provider: String,
    pub record: AuditRecord,
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

    pub fn audit_records(&self) -> Vec<ProviderAuditRecord> {
        self.providers
            .iter()
            .flat_map(|(provider, adapter)| {
                adapter
                    .audit_records()
                    .into_iter()
                    .map(|record| ProviderAuditRecord {
                        provider: provider.clone(),
                        record,
                    })
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct ClaudeCodeProvider {
    executable: PathBuf,
    model: Option<String>,
    max_budget_usd: Option<String>,
    sandbox: ProcessSandbox,
}

impl ClaudeCodeProvider {
    pub fn new(executable: impl Into<PathBuf>, limits: CommandLimits) -> Self {
        let executable = executable.into();
        Self {
            sandbox: process_sandbox(&executable, &limits),
            executable,
            model: None,
            max_budget_usd: None,
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

    pub fn audit_log(&self) -> agel_effects::AuditLog {
        self.sandbox.audit_log()
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
        run_command(
            &self.executable,
            &args,
            &request.prompt,
            request,
            &self.sandbox,
        )
    }

    fn audit_records(&self) -> Vec<AuditRecord> {
        self.audit_log().records()
    }
}

#[derive(Clone, Debug)]
pub struct CodexProvider {
    executable: PathBuf,
    model: Option<String>,
    limits: CommandLimits,
    sandbox: ProcessSandbox,
}

impl CodexProvider {
    pub fn new(executable: impl Into<PathBuf>, limits: CommandLimits) -> Self {
        let executable = executable.into();
        Self {
            sandbox: process_sandbox(&executable, &limits),
            executable,
            model: None,
            limits,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn audit_log(&self) -> agel_effects::AuditLog {
        self.sandbox.audit_log()
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
        run_command(
            &self.executable,
            &args,
            &request.prompt,
            request,
            &self.sandbox,
        )
    }

    fn audit_records(&self) -> Vec<AuditRecord> {
        self.audit_log().records()
    }
}

fn run_command(
    executable: &std::path::Path,
    arguments: &[String],
    prompt: &str,
    request: &ModelRequest,
    sandbox: &ProcessSandbox,
) -> Result<String, ProviderError> {
    let output = sandbox
        .run(
            Principal {
                world: request.world_id,
                agent: Some(request.requester),
            },
            format!("model/infer/{}/request/{}", request.provider, request.id),
            ProcessSpec {
                executable: executable.to_owned(),
                arguments: arguments.to_vec(),
                stdin: prompt.as_bytes().to_vec(),
            },
        )
        .map_err(provider_effect_error)?;
    let stderr = String::from_utf8(output.stderr).map_err(|_| ProviderError::InvalidUtf8)?;
    if output.status != 0 {
        return Err(ProviderError::Failed {
            code: (output.status >= 0).then_some(output.status),
            stderr: stderr.trim().to_owned(),
        });
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim_end().to_owned())
        .map_err(|_| ProviderError::InvalidUtf8)
}

fn process_sandbox(executable: &std::path::Path, limits: &CommandLimits) -> ProcessSandbox {
    ProcessSandbox::new(CommandLimits {
        timeout: limits.timeout,
        max_output_bytes: limits.max_output_bytes,
        workspace: limits.workspace.clone(),
    })
    .allow_executable(executable)
    .inherit_environment([
        "HOME",
        "PATH",
        "USER",
        "LOGNAME",
        "SHELL",
        "TMPDIR",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "CODEX_HOME",
        "CLAUDE_CONFIG_DIR",
    ])
}

fn provider_effect_error(error: EffectError) -> ProviderError {
    match error {
        EffectError::TimedOut => ProviderError::TimedOut,
        EffectError::OutputLimitExceeded => ProviderError::OutputLimitExceeded,
        other => ProviderError::Io(other.to_string()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

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
            world_id: 11,
            requester: 1,
            reply_to: 1,
            provider: provider.into(),
            prompt: "explain (cons 'agent future)".into(),
            prompt_digest: agel_core::Digest::ZERO,
            effect_key: agel_core::Digest::ZERO,
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
        let audit = provider.audit_log().records();
        assert_eq!(audit.len(), 2);
        assert!(matches!(audit[0].outcome, AuditOutcome::Allowed));
        assert!(matches!(
            audit[1].outcome,
            AuditOutcome::Succeeded { status: 0 }
        ));
        assert_eq!(audit[0].intent.principal.world, 11);
        assert_eq!(audit[0].intent.principal.agent, Some(1));
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
