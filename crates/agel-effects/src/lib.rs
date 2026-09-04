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

#[derive(Clone, Debug, Default)]
pub struct StaticPolicy {
    allowed: BTreeSet<(EffectKind, String)>,
    virtualized: BTreeSet<EffectKind>,
}

impl StaticPolicy {
    pub fn allow(mut self, kind: EffectKind, operation: impl Into<String>) -> Self {
        self.allowed.insert((kind, operation.into()));
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
}

impl ProcessSandbox {
    pub fn new(limits: ProcessLimits) -> Self {
        Self {
            allowed_executables: BTreeSet::new(),
            inherited_environment: BTreeSet::new(),
            limits,
            audit: AuditLog::default(),
        }
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
            payload_digest: sha256(&spec.stdin),
        };
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
        let stdout_reader = thread::spawn(move || read_bounded(stdout, output_limit));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, output_limit));
        let deadline = Instant::now()
            .checked_add(self.limits.timeout)
            .ok_or_else(|| EffectError::Io("process timeout is out of range".into()))?;
        let status = loop {
            match child
                .try_wait()
                .map_err(|e| EffectError::Io(e.to_string()))?
            {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    terminate_process_group(&mut child);
                    join_input(input)?;
                    let _ = join_reader(stdout_reader)?;
                    let _ = join_reader(stderr_reader)?;
                    return Err(EffectError::TimedOut);
                }
                None => thread::sleep(Duration::from_millis(10)),
            }
        };
        join_input(input)?;
        let (stdout, stdout_exceeded) = join_reader(stdout_reader)?;
        let (stderr, stderr_exceeded) = join_reader(stderr_reader)?;
        if stdout_exceeded || stderr_exceeded {
            return Err(EffectError::OutputLimitExceeded);
        }
        Ok(ProcessOutput {
            status: status.code().unwrap_or(-1),
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
