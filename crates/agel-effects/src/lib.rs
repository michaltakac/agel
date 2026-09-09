//! Typed, auditable interposition for effects outside an Agel world.
//!
//! This crate is deliberately below language policy and above host APIs. It is
//! not a kernel security boundary: embedders must ensure untrusted code cannot
//! bypass it and call the host directly.

use agel_integrity::{sha256, Digest};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffectKind {
    FileRead,
    FileWrite,
    Process,
    Network,
    Clock,
    Random,
    Model,
}

impl EffectKind {
    fn name(&self) -> &'static str {
        match self {
            Self::FileRead => "file/read",
            Self::FileWrite => "file/write",
            Self::Process => "process/run",
            Self::Network => "network/access",
            Self::Clock => "clock/read",
            Self::Random => "random/read",
            Self::Model => "model/infer",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Principal {
    pub world: u64,
    pub agent: Option<u64>,
}

impl Principal {
    pub fn host() -> Self {
        Self {
            world: 0,
            agent: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectIntent {
    pub principal: Principal,
    pub kind: EffectKind,
    pub operation: String,
    pub resource: String,
    pub payload_digest: Digest,
}

impl EffectIntent {
    pub fn key(&self) -> Digest {
        let mut encoded = b"agel/effect-intent/v1\0".to_vec();
        field(&mut encoded, &self.principal.world.to_be_bytes());
        field(
            &mut encoded,
            &self.principal.agent.unwrap_or(u64::MAX).to_be_bytes(),
        );
        field(&mut encoded, self.kind.name().as_bytes());
        field(&mut encoded, self.operation.as_bytes());
        field(&mut encoded, self.resource.as_bytes());
        field(&mut encoded, self.payload_digest.as_bytes());
        sha256(&encoded)
    }
}

fn field(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Virtualize,
    Deny(String),
}

pub trait Policy: Send + Sync {
    fn decide(&self, intent: &EffectIntent) -> Decision;
}

impl fmt::Debug for dyn Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Policy")
    }
}

#[derive(Clone, Debug, Default)]
pub struct StaticPolicy {
    allowed: BTreeSet<(EffectKind, String)>,
    allowed_prefixes: BTreeSet<(EffectKind, String)>,
    virtualized: BTreeSet<EffectKind>,
}

impl StaticPolicy {
    pub fn allow(mut self, kind: EffectKind, operation: impl Into<String>) -> Self {
        self.allowed.insert((kind, operation.into()));
        self
    }

    /// Allow every operation of `kind` whose name starts with `prefix`. This is
    /// how a policy admits a family of per-request operations such as
    /// `model/infer/claude/request/N` without enumerating request numbers.
    pub fn allow_prefix(mut self, kind: EffectKind, prefix: impl Into<String>) -> Self {
        self.allowed_prefixes.insert((kind, prefix.into()));
        self
    }

    pub fn virtualize(mut self, kind: EffectKind) -> Self {
        self.virtualized.insert(kind);
        self
    }
}

impl Policy for StaticPolicy {
    fn decide(&self, intent: &EffectIntent) -> Decision {
        if self
            .allowed
            .contains(&(intent.kind.clone(), intent.operation.clone()))
            || self
                .allowed_prefixes
                .iter()
                .any(|(kind, prefix)| *kind == intent.kind && intent.operation.starts_with(prefix))
        {
            Decision::Allow
        } else if self.virtualized.contains(&intent.kind) {
            Decision::Virtualize
        } else {
            Decision::Deny(format!(
                "{} operation {:?} is not granted",
                intent.kind.name(),
                intent.operation
            ))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditOutcome {
    Allowed,
    Denied(String),
    Succeeded { status: i32 },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRecord {
    pub sequence: u64,
    pub key: Digest,
    pub intent: EffectIntent,
    pub outcome: AuditOutcome,
}

#[derive(Clone, Debug, Default)]
pub struct AuditLog(Arc<Mutex<Vec<AuditRecord>>>);

impl AuditLog {
    pub fn records(&self) -> Vec<AuditRecord> {
        self.0.lock().expect("audit mutex poisoned").clone()
    }

    fn append(&self, intent: &EffectIntent, outcome: AuditOutcome) {
        let mut records = self.0.lock().expect("audit mutex poisoned");
        let sequence = records.len() as u64 + 1;
        records.push(AuditRecord {
            sequence,
            key: intent.key(),
            intent: intent.clone(),
            outcome,
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtualPath(String);

impl VirtualPath {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, EffectError> {
        let path = path.as_ref();
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(value) => components.push(
                    value
                        .to_str()
                        .ok_or_else(|| EffectError::InvalidPath("path is not UTF-8".into()))?,
                ),
                Component::ParentDir => {
                    return Err(EffectError::InvalidPath(
                        "parent traversal is forbidden".into(),
                    ))
                }
                Component::Prefix(_) => {
                    return Err(EffectError::InvalidPath(
                        "host path prefixes are forbidden".into(),
                    ))
                }
            }
        }
        if components.is_empty() {
            return Err(EffectError::InvalidPath("a file path is required".into()));
        }
        Ok(Self(format!("/{}", components.join("/"))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Write { path: VirtualPath, bytes: Vec<u8> },
    Delete { path: VirtualPath },
}

#[derive(Clone, Debug, Default)]
pub struct CowWorkspace {
    base: BTreeMap<VirtualPath, Vec<u8>>,
    overlay: BTreeMap<VirtualPath, Option<Vec<u8>>>,
}

impl CowWorkspace {
    pub fn from_files(
        files: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> Result<Self, EffectError> {
        let mut workspace = Self::default();
        for (path, bytes) in files {
            workspace.base.insert(VirtualPath::new(path)?, bytes);
        }
        Ok(workspace)
    }

    pub fn read(&self, path: impl AsRef<Path>) -> Result<Option<&[u8]>, EffectError> {
        let path = VirtualPath::new(path)?;
        match self.overlay.get(&path) {
            Some(Some(bytes)) => Ok(Some(bytes)),
            Some(None) => Ok(None),
            None => Ok(self.base.get(&path).map(Vec::as_slice)),
        }
    }

    pub fn write(
        &mut self,
        path: impl AsRef<Path>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), EffectError> {
        self.overlay
            .insert(VirtualPath::new(path)?, Some(bytes.into()));
        Ok(())
    }

    pub fn delete(&mut self, path: impl AsRef<Path>) -> Result<(), EffectError> {
        self.overlay.insert(VirtualPath::new(path)?, None);
        Ok(())
    }

    pub fn diff(&self) -> Vec<Change> {
        self.overlay
            .iter()
            .filter_map(|(path, value)| match value {
                Some(bytes) if self.base.get(path) == Some(bytes) => None,
                Some(bytes) => Some(Change::Write {
                    path: path.clone(),
                    bytes: bytes.clone(),
                }),
                None if self.base.contains_key(path) => Some(Change::Delete { path: path.clone() }),
                None => None,
            })
            .collect()
    }

    pub fn commit(&mut self) -> Vec<Change> {
        let changes = self.diff();
        for change in &changes {
            match change {
                Change::Write { path, bytes } => {
                    self.base.insert(path.clone(), bytes.clone());
                }
                Change::Delete { path } => {
                    self.base.remove(path);
                }
            }
        }
        self.overlay.clear();
        changes
    }

    pub fn rollback(&mut self) {
        self.overlay.clear();
    }
}

/// A policy-mediated view of a [`CowWorkspace`]: every read, write and delete
/// is a typed `file/read` or `file/write` intent that the policy decides and the
/// audit log records before any byte moves.
///
/// `Allow` writes through to the base image immediately. `Virtualize` stages
/// the change in the copy-on-write overlay, where it is visible to later reads,
/// inspectable as a diff, and either committed or rolled back explicitly. `Deny`
/// changes nothing. The workspace is in-memory; this is the effect vocabulary
/// and decision point, not host filesystem confinement.
#[derive(Clone, Debug)]
pub struct WorkspaceBroker {
    policy: Arc<dyn Policy>,
    workspace: CowWorkspace,
    audit: AuditLog,
}

impl WorkspaceBroker {
    pub fn new(policy: impl Policy + 'static, workspace: CowWorkspace) -> Self {
        Self {
            policy: Arc::new(policy),
            workspace,
            audit: AuditLog::default(),
        }
    }

    pub fn audit_log(&self) -> AuditLog {
        self.audit.clone()
    }

    pub fn workspace(&self) -> &CowWorkspace {
        &self.workspace
    }

    pub fn read(
        &self,
        principal: Principal,
        path: impl AsRef<Path>,
    ) -> Result<Option<Vec<u8>>, EffectError> {
        let path = VirtualPath::new(path)?;
        let intent = file_intent(principal, EffectKind::FileRead, "read", &path, &[]);
        match self.policy.decide(&intent) {
            Decision::Deny(reason) => {
                self.audit
                    .append(&intent, AuditOutcome::Denied(reason.clone()));
                Err(EffectError::Denied(reason))
            }
            Decision::Allow | Decision::Virtualize => {
                self.audit.append(&intent, AuditOutcome::Allowed);
                let bytes = self.workspace.read(path.as_str())?.map(<[u8]>::to_vec);
                self.audit
                    .append(&intent, AuditOutcome::Succeeded { status: 0 });
                Ok(bytes)
            }
        }
    }

    pub fn write(
        &mut self,
        principal: Principal,
        path: impl AsRef<Path>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<Decision, EffectError> {
        let path = VirtualPath::new(path)?;
        let bytes = bytes.into();
        let intent = file_intent(principal, EffectKind::FileWrite, "write", &path, &bytes);
        self.mutate(intent, |workspace| workspace.write(path.as_str(), bytes))
    }

    pub fn delete(
        &mut self,
        principal: Principal,
        path: impl AsRef<Path>,
    ) -> Result<Decision, EffectError> {
        let path = VirtualPath::new(path)?;
        let intent = file_intent(principal, EffectKind::FileWrite, "delete", &path, &[]);
        self.mutate(intent, |workspace| workspace.delete(path.as_str()))
    }

    fn mutate(
        &mut self,
        intent: EffectIntent,
        change: impl FnOnce(&mut CowWorkspace) -> Result<(), EffectError>,
    ) -> Result<Decision, EffectError> {
        let decision = self.policy.decide(&intent);
        match &decision {
            Decision::Deny(reason) => {
                self.audit
                    .append(&intent, AuditOutcome::Denied(reason.clone()));
                return Err(EffectError::Denied(reason.clone()));
            }
            Decision::Allow | Decision::Virtualize => {
                self.audit.append(&intent, AuditOutcome::Allowed)
            }
        }
        // Stage in the overlay first so an invalid change cannot half-apply.
        let staged = self.workspace.diff();
        if let Err(error) = change(&mut self.workspace) {
            self.workspace.rollback();
            for change in staged {
                match change {
                    Change::Write { path, bytes } => {
                        self.workspace.overlay.insert(path, Some(bytes));
                    }
                    Change::Delete { path } => {
                        self.workspace.overlay.insert(path, None);
                    }
                }
            }
            self.audit
                .append(&intent, AuditOutcome::Failed(error.to_string()));
            return Err(error);
        }
        if decision == Decision::Allow {
            self.workspace.commit();
        }
        self.audit
            .append(&intent, AuditOutcome::Succeeded { status: 0 });
        Ok(decision)
    }

    pub fn diff(&self) -> Vec<Change> {
        self.workspace.diff()
    }

    pub fn commit(&mut self) -> Vec<Change> {
        self.workspace.commit()
    }

    pub fn rollback(&mut self) {
        self.workspace.rollback();
    }
}

fn file_intent(
    principal: Principal,
    kind: EffectKind,
    operation: &str,
    path: &VirtualPath,
    payload: &[u8],
) -> EffectIntent {
    EffectIntent {
        principal,
        kind,
        operation: operation.into(),
        resource: path.as_str().to_owned(),
        payload_digest: sha256(payload),
    }
}

#[derive(Clone, Debug)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub workspace: PathBuf,
}

impl ProcessLimits {
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_output_bytes: 1_048_576,
            workspace: workspace.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub stdin: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutput {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ProcessSandbox {
    allowed_executables: BTreeSet<PathBuf>,
    inherited_environment: BTreeSet<String>,
    limits: ProcessLimits,
    audit: AuditLog,
    policy: Option<Arc<dyn Policy>>,
}

impl ProcessSandbox {
    pub fn new(limits: ProcessLimits) -> Self {
        Self {
            allowed_executables: BTreeSet::new(),
            inherited_environment: BTreeSet::new(),
            limits,
            audit: AuditLog::default(),
            policy: None,
        }
    }

    /// Consult `policy` for every `process/run` intent before the executable
    /// allowlist. A denial is recorded and returned without spawning anything;
    /// `Virtualize` is also a denial here because no process virtualization
    /// exists. Without a policy the executable allowlist remains the only gate.
    pub fn with_policy(mut self, policy: impl Policy + 'static) -> Self {
        self.policy = Some(Arc::new(policy));
        self
    }

    pub fn allow_executable(mut self, executable: impl Into<PathBuf>) -> Self {
        self.allowed_executables.insert(executable.into());
        self
    }

    pub fn inherit_environment(mut self, names: impl IntoIterator<Item = &'static str>) -> Self {
        self.inherited_environment
            .extend(names.into_iter().map(str::to_owned));
        self
    }

    pub fn audit_log(&self) -> AuditLog {
        self.audit.clone()
    }

    pub fn run(
        &self,
        principal: Principal,
        operation: impl Into<String>,
        spec: ProcessSpec,
    ) -> Result<ProcessOutput, EffectError> {
        let intent = EffectIntent {
            principal,
            kind: EffectKind::Process,
            operation: operation.into(),
            resource: spec.executable.to_string_lossy().into_owned(),
            payload_digest: {
                let mut payload = b"agel/process/v2\0".to_vec();
                field(
                    &mut payload,
                    self.limits.workspace.to_string_lossy().as_bytes(),
                );
                field(&mut payload, &(spec.arguments.len() as u64).to_be_bytes());
                for argument in &spec.arguments {
                    field(&mut payload, argument.as_bytes());
                }
                field(&mut payload, &spec.stdin);
                sha256(&payload)
            },
        };
        if let Some(policy) = &self.policy {
            let refused = match policy.decide(&intent) {
                Decision::Allow => None,
                Decision::Deny(reason) => Some(reason),
                Decision::Virtualize => {
                    Some("process effects cannot be virtualized; refusing".into())
                }
            };
            if let Some(reason) = refused {
                self.audit
                    .append(&intent, AuditOutcome::Denied(reason.clone()));
                return Err(EffectError::Denied(reason));
            }
        }
        if !self.allowed_executables.contains(&spec.executable) {
            let reason = format!("executable {:?} is not allowlisted", spec.executable);
            self.audit
                .append(&intent, AuditOutcome::Denied(reason.clone()));
            return Err(EffectError::Denied(reason));
        }
        let workspace = self
            .limits
            .workspace
            .canonicalize()
            .map_err(|error| self.fail(&intent, EffectError::Io(error.to_string())))?;
        if !workspace.is_dir() {
            let error = EffectError::InvalidWorkspace("workspace is not a directory".into());
            return Err(self.fail(&intent, error));
        }
        self.audit.append(&intent, AuditOutcome::Allowed);
        let result = self.spawn(&workspace, spec);
        match &result {
            Ok(output) => self.audit.append(
                &intent,
                AuditOutcome::Succeeded {
                    status: output.status,
                },
            ),
            Err(error) => self
                .audit
                .append(&intent, AuditOutcome::Failed(error.to_string())),
        }
        result
    }

    fn fail(&self, intent: &EffectIntent, error: EffectError) -> EffectError {
        self.audit
            .append(intent, AuditOutcome::Failed(error.to_string()));
        error
    }

    fn spawn(&self, workspace: &Path, spec: ProcessSpec) -> Result<ProcessOutput, EffectError> {
        let deadline = Instant::now()
            .checked_add(self.limits.timeout)
            .ok_or_else(|| EffectError::Io("process timeout is out of range".into()))?;
        let mut command = Command::new(&spec.executable);
        command
            .args(&spec.arguments)
            .current_dir(workspace)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for name in &self.inherited_environment {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        configure_process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| EffectError::Io(error.to_string()))?;
        let mut stdin = child.stdin.take().expect("piped stdin exists");
        let input = thread::spawn(move || stdin.write_all(&spec.stdin));
        let stdout = child.stdout.take().expect("piped stdout exists");
        let stderr = child.stderr.take().expect("piped stderr exists");
        let output_limit = self.limits.max_output_bytes;
        let exceeded = Arc::new(AtomicBool::new(false));
        let stdout_exceeded = exceeded.clone();
        let stderr_exceeded = exceeded.clone();
        let stdout_reader =
            thread::spawn(move || read_bounded(stdout, output_limit, &stdout_exceeded));
        let stderr_reader =
            thread::spawn(move || read_bounded(stderr, output_limit, &stderr_exceeded));
        let mut status = None;
        let failure = loop {
            if exceeded.load(Ordering::Relaxed) {
                break Some(EffectError::OutputLimitExceeded);
            }
            if status.is_none() {
                match child.try_wait() {
                    Ok(result) => status = result,
                    Err(error) => break Some(EffectError::Io(error.to_string())),
                }
            }
            // A parent may exit while a descendant still owns a pipe. Do not
            // leave the deadline behind and block indefinitely in join().
            if status.is_some()
                && input.is_finished()
                && stdout_reader.is_finished()
                && stderr_reader.is_finished()
            {
                break None;
            }
            if Instant::now() >= deadline {
                break Some(EffectError::TimedOut);
            }
            thread::sleep(Duration::from_millis(10));
        };
        if let Some(error) = failure {
            terminate_process_group(&mut child);
            // Do not let a process that escaped its group extend the caller's
            // deadline through inherited pipes. This wrapper is not confinement.
            return Err(error);
        }
        join_input(input)?;
        let (stdout, stdout_exceeded) = join_reader(stdout_reader)?;
        let (stderr, stderr_exceeded) = join_reader(stderr_reader)?;
        if stdout_exceeded || stderr_exceeded {
            return Err(EffectError::OutputLimitExceeded);
        }
        Ok(ProcessOutput {
            status: status.expect("completed child").code().unwrap_or(-1),
            stdout,
            stderr,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectError {
    Denied(String),
    InvalidPath(String),
    InvalidWorkspace(String),
    Io(String),
    TimedOut,
    OutputLimitExceeded,
}

impl fmt::Display for EffectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied(message) => write!(f, "effect denied: {message}"),
            Self::InvalidPath(message) => write!(f, "invalid virtual path: {message}"),
            Self::InvalidWorkspace(message) => write!(f, "invalid workspace: {message}"),
            Self::Io(message) => write!(f, "effect I/O failed: {message}"),
            Self::TimedOut => f.write_str("effect timed out"),
            Self::OutputLimitExceeded => f.write_str("effect output exceeded its byte limit"),
        }
    }
}

impl std::error::Error for EffectError {}

fn read_bounded(
    mut reader: impl Read,
    limit: usize,
    overflow: &AtomicBool,
) -> io::Result<(Vec<u8>, bool)> {
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
        if exceeded {
            overflow.store(true, Ordering::Relaxed);
        }
    }
    Ok((retained, exceeded))
}

fn join_input(handle: thread::JoinHandle<io::Result<()>>) -> Result<(), EffectError> {
    handle
        .join()
        .map_err(|_| EffectError::Io("stdin writer thread panicked".into()))?
        .map_err(|error| EffectError::Io(error.to_string()))
}

fn join_reader(
    handle: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
) -> Result<(Vec<u8>, bool), EffectError> {
    handle
        .join()
        .map_err(|_| EffectError::Io("output reader thread panicked".into()))?
        .map_err(|error| EffectError::Io(error.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn shell(script: &str, timeout: Duration, limit: usize) -> Result<ProcessOutput, EffectError> {
        let mut limits = ProcessLimits::new(std::env::temp_dir());
        limits.timeout = timeout;
        limits.max_output_bytes = limit;
        ProcessSandbox::new(limits).allow_executable("/bin/sh").run(
            Principal::host(),
            "test",
            ProcessSpec {
                executable: "/bin/sh".into(),
                arguments: vec!["-c".into(), script.into()],
                stdin: vec![],
            },
        )
    }

    #[cfg(unix)]
    #[test]
    fn timeout_covers_descendant_pipes_after_parent_exit() {
        let start = Instant::now();
        assert_eq!(
            shell("/bin/sleep 2 & exit 0", Duration::from_millis(100), 100),
            Err(EffectError::TimedOut)
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn output_overflow_terminates_before_timeout() {
        let start = Instant::now();
        assert_eq!(
            shell(
                "while :; do printf abcdefghijklmnop; done",
                Duration::from_secs(3),
                64
            ),
            Err(EffectError::OutputLimitExceeded)
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn process_audit_binds_argument_boundaries() {
        let sandbox = ProcessSandbox::new(ProcessLimits::new(std::env::temp_dir()))
            .allow_executable("/bin/echo");
        for args in [vec!["a", "bc"], vec!["ab", "c"]] {
            sandbox
                .run(
                    Principal::host(),
                    "test",
                    ProcessSpec {
                        executable: "/bin/echo".into(),
                        arguments: args.into_iter().map(str::to_owned).collect(),
                        stdin: vec![],
                    },
                )
                .unwrap();
        }
        let records = sandbox.audit_log().records();
        assert_ne!(records[0].key, records[2].key);
    }

    #[test]
    fn policy_is_default_deny_and_intent_keys_bind_every_field() {
        let intent = EffectIntent {
            principal: Principal {
                world: 7,
                agent: Some(3),
            },
            kind: EffectKind::Network,
            operation: "connect".into(),
            resource: "example.test:443".into(),
            payload_digest: sha256(b"hello"),
        };
        assert!(matches!(
            StaticPolicy::default().decide(&intent),
            Decision::Deny(_)
        ));
        let allowed = StaticPolicy::default().allow(EffectKind::Network, "connect");
        assert_eq!(allowed.decide(&intent), Decision::Allow);
        let mut changed = intent.clone();
        changed.resource.push('0');
        assert_ne!(intent.key(), changed.key());
    }

    #[cfg(unix)]
    #[test]
    fn process_policy_is_consulted_before_the_executable_allowlist() {
        let limits = ProcessLimits::new(std::env::temp_dir());
        let spec = || ProcessSpec {
            executable: "/bin/echo".into(),
            arguments: vec!["ok".into()],
            stdin: vec![],
        };
        let denied = ProcessSandbox::new(limits.clone())
            .allow_executable("/bin/echo")
            .with_policy(StaticPolicy::default().allow(EffectKind::Process, "other"));
        assert!(matches!(
            denied.run(Principal::host(), "model/infer/claude/request/1", spec()),
            Err(EffectError::Denied(_))
        ));
        let virtualized = ProcessSandbox::new(limits.clone())
            .allow_executable("/bin/echo")
            .with_policy(StaticPolicy::default().virtualize(EffectKind::Process));
        assert!(matches!(
            virtualized.run(Principal::host(), "model/infer/claude/request/1", spec()),
            Err(EffectError::Denied(_))
        ));
        let records = denied.audit_log().records();
        assert_eq!(records.len(), 1);
        assert!(matches!(records[0].outcome, AuditOutcome::Denied(_)));
        let allowed = ProcessSandbox::new(limits)
            .allow_executable("/bin/echo")
            .with_policy(
                StaticPolicy::default().allow_prefix(EffectKind::Process, "model/infer/claude/"),
            );
        assert_eq!(
            allowed
                .run(Principal::host(), "model/infer/claude/request/7", spec())
                .unwrap()
                .stdout,
            b"ok\n"
        );
        assert!(matches!(
            allowed.run(Principal::host(), "model/infer/codex/request/7", spec()),
            Err(EffectError::Denied(_))
        ));
    }

    #[test]
    fn workspace_broker_routes_file_effects_through_policy() {
        let workspace =
            CowWorkspace::from_files([("/src/main.agel".into(), b"old".to_vec())]).unwrap();
        let policy = StaticPolicy::default()
            .allow(EffectKind::FileRead, "read")
            .virtualize(EffectKind::FileWrite);
        let mut broker = WorkspaceBroker::new(policy, workspace.clone());
        assert_eq!(
            broker.read(Principal::host(), "/src/main.agel").unwrap(),
            Some(b"old".to_vec())
        );
        assert_eq!(
            broker
                .write(Principal::host(), "/src/main.agel", b"new".to_vec())
                .unwrap(),
            Decision::Virtualize
        );
        assert_eq!(
            broker.read(Principal::host(), "/src/main.agel").unwrap(),
            Some(b"new".to_vec())
        );
        assert_eq!(broker.diff().len(), 1);
        assert_eq!(
            broker
                .workspace()
                .base
                .get(&VirtualPath::new("/src/main.agel").unwrap()),
            Some(&b"old".to_vec())
        );
        broker.rollback();
        assert!(broker.diff().is_empty());
        assert!(matches!(
            broker.write(Principal::host(), "../escape", b"x".to_vec()),
            Err(EffectError::InvalidPath(_))
        ));

        let mut denied = WorkspaceBroker::new(StaticPolicy::default(), workspace.clone());
        assert!(matches!(
            denied.read(Principal::host(), "/src/main.agel"),
            Err(EffectError::Denied(_))
        ));
        assert!(matches!(
            denied.delete(Principal::host(), "/src/main.agel"),
            Err(EffectError::Denied(_))
        ));
        assert!(denied
            .audit_log()
            .records()
            .iter()
            .all(|record| matches!(record.outcome, AuditOutcome::Denied(_))));
        assert_eq!(denied.audit_log().records().len(), 2);

        let mut direct = WorkspaceBroker::new(
            StaticPolicy::default().allow(EffectKind::FileWrite, "write"),
            workspace,
        );
        assert_eq!(
            direct
                .write(Principal::host(), "/src/main.agel", b"direct".to_vec())
                .unwrap(),
            Decision::Allow
        );
        assert!(direct.diff().is_empty());
        assert_eq!(
            direct
                .workspace()
                .base
                .get(&VirtualPath::new("/src/main.agel").unwrap()),
            Some(&b"direct".to_vec())
        );
    }

    #[test]
    fn copy_on_write_diff_commit_and_rollback_are_explicit() {
        let mut workspace = CowWorkspace::from_files([
            ("/src/main.agel".into(), b"old".to_vec()),
            ("/keep".into(), b"same".to_vec()),
        ])
        .unwrap();
        workspace.write("/src/main.agel", b"new".to_vec()).unwrap();
        workspace.write("/created", b"agent".to_vec()).unwrap();
        workspace.delete("/keep").unwrap();
        assert_eq!(workspace.read("/src/main.agel").unwrap(), Some(&b"new"[..]));
        assert_eq!(workspace.diff().len(), 3);
        workspace.rollback();
        assert_eq!(workspace.read("/src/main.agel").unwrap(), Some(&b"old"[..]));
        assert!(workspace.diff().is_empty());
        workspace
            .write("/src/main.agel", b"stable".to_vec())
            .unwrap();
        assert_eq!(workspace.commit().len(), 1);
        assert_eq!(
            workspace.read("/src/main.agel").unwrap(),
            Some(&b"stable"[..])
        );
    }

    #[test]
    fn virtual_paths_reject_escape() {
        assert!(matches!(
            VirtualPath::new("../secret"),
            Err(EffectError::InvalidPath(_))
        ));
        assert!(matches!(
            VirtualPath::new("safe/../../secret"),
            Err(EffectError::InvalidPath(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn process_boundary_denies_unknown_program_and_clears_environment() {
        let workspace = std::env::temp_dir();
        let limits = ProcessLimits::new(&workspace);
        let sandbox = ProcessSandbox::new(limits).allow_executable("/bin/sh");
        let denied = sandbox
            .run(
                Principal::host(),
                "test",
                ProcessSpec {
                    executable: "/usr/bin/false".into(),
                    arguments: vec![],
                    stdin: vec![],
                },
            )
            .unwrap_err();
        assert!(matches!(denied, EffectError::Denied(_)));
        let output = sandbox
            .run(
                Principal::host(),
                "test",
                ProcessSpec {
                    executable: "/bin/sh".into(),
                    arguments: vec!["-c".into(), "printf %s ${AGEL_AMBIENT-unset}".into()],
                    stdin: vec![],
                },
            )
            .unwrap();
        assert_eq!(output.stdout, b"unset");
        assert_eq!(sandbox.audit_log().records().len(), 3);
    }
}
