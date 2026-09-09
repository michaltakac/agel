use agel_core::{
    read_all, Commit, EvaluationOptions, ModelCompletion, ModelOutcome, ModelRequest, ReadError,
    Snapshot, Value, World,
};
use agel_image::{Image, ImageSession, ImageStore};
use agel_integrity::{encode_hex, Digest, SigningKey, VerifyingKey};
use agel_model::{
    ClaudeCodeProvider, CodexProvider, CommandLimits, ProviderError, ProviderRegistry,
};
use agel_verify::{Evidence, Proposal, TestCase, Verifier};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug)]
struct CliConfig {
    claude: bool,
    codex: bool,
    claude_bin: PathBuf,
    codex_bin: PathBuf,
    claude_model: Option<String>,
    codex_model: Option<String>,
    claude_max_budget_usd: Option<String>,
    workspace: PathBuf,
    timeout: Duration,
    max_output_bytes: usize,
    stdlib: bool,
    image: Option<PathBuf>,
    signing_key: Option<PathBuf>,
    trust_key: Option<PathBuf>,
    keygen: Option<PathBuf>,
}

impl CliConfig {
    fn from_args(arguments: impl IntoIterator<Item = String>) -> Result<Option<Self>, String> {
        let mut config = Self {
            claude: false,
            codex: false,
            claude_bin: "claude".into(),
            codex_bin: "codex".into(),
            claude_model: None,
            codex_model: None,
            claude_max_budget_usd: None,
            workspace: std::env::current_dir().map_err(|error| error.to_string())?,
            timeout: Duration::from_secs(300),
            max_output_bytes: 1_048_576,
            stdlib: true,
            image: None,
            signing_key: None,
            trust_key: None,
            keygen: None,
        };
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--help" | "-h" => return Ok(None),
                "--enable-claude" => config.claude = true,
                "--enable-codex" => config.codex = true,
                "--no-stdlib" => config.stdlib = false,
                "--image" => config.image = Some(required_value(&mut arguments, &argument)?.into()),
                "--signing-key" => {
                    config.signing_key = Some(required_value(&mut arguments, &argument)?.into())
                }
                "--trust-key" => {
                    config.trust_key = Some(required_value(&mut arguments, &argument)?.into())
                }
                "--keygen" => {
                    config.keygen = Some(required_value(&mut arguments, &argument)?.into())
                }
                "--claude-bin" => {
                    config.claude_bin = required_value(&mut arguments, &argument)?.into()
                }
                "--codex-bin" => {
                    config.codex_bin = required_value(&mut arguments, &argument)?.into()
                }
                "--claude-model" => {
                    config.claude_model = Some(required_value(&mut arguments, &argument)?)
                }
                "--codex-model" => {
                    config.codex_model = Some(required_value(&mut arguments, &argument)?)
                }
                "--claude-max-budget-usd" => {
                    config.claude_max_budget_usd = Some(required_value(&mut arguments, &argument)?)
                }
                "--model-workspace" => {
                    config.workspace = required_value(&mut arguments, &argument)?.into()
                }
                "--model-timeout-seconds" => {
                    let value = required_value(&mut arguments, &argument)?;
                    let seconds = value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid {argument}: {value}"))?;
                    config.timeout = Duration::from_secs(seconds);
                }
                "--model-max-output-bytes" => {
                    let value = required_value(&mut arguments, &argument)?;
                    config.max_output_bytes = value
                        .parse::<usize>()
                        .map_err(|_| format!("invalid {argument}: {value}"))?;
                }
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
        }
        Ok(Some(config))
    }
}

fn required_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
}

/// The world the REPL talks to: either a volatile in-memory world, or a world
/// reconstructed from and appended to a portable image on disk.
///
/// In image mode every committed input, capability grant, model claim, and
/// model completion is appended to the tamper-evident image and the file is
/// atomically replaced after each commit. Rollback and snapshot restore are
/// refused there: an image is an append-only log of committed inputs, and
/// rewinding the live world without rewinding the log would make the file
/// disagree with the world it claims to reconstruct.
// One runtime exists per process, so the size difference between a volatile
// world and an image session with its signing key is irrelevant.
#[allow(clippy::large_enum_variant)]
enum Runtime {
    Volatile {
        world: World,
        options: EvaluationOptions,
    },
    Image {
        session: ImageSession,
        store: ImageStore,
        saved: Option<Digest>,
        signing: Option<SigningKey>,
    },
}

/// Keys the operator supplied. A signing key implies trust in its own public
/// half; an explicit trust key must match it, so a world is never appended
/// with a signature the next start would refuse.
#[derive(Clone, Debug, Default)]
struct Keys {
    signing: Option<SigningKey>,
    trusted: Option<VerifyingKey>,
}

impl Keys {
    fn load(signing: Option<&Path>, trust: Option<&Path>) -> Result<Self, String> {
        let signing = match signing {
            Some(path) => {
                let text = std::fs::read_to_string(path).map_err(|error| {
                    format!("cannot read signing key {}: {error}", path.display())
                })?;
                Some(SigningKey::from_hex(&text).map_err(|error| {
                    format!(
                        "signing key {} is not a 32-byte hex seed: {error}",
                        path.display()
                    )
                })?)
            }
            None => None,
        };
        let trusted = match trust {
            Some(path) => {
                let text = std::fs::read_to_string(path).map_err(|error| {
                    format!("cannot read trust key {}: {error}", path.display())
                })?;
                Some(VerifyingKey::from_hex(&text).map_err(|error| {
                    format!(
                        "trust key {} is not an Ed25519 public key: {error}",
                        path.display()
                    )
                })?)
            }
            None => None,
        };
        match (&signing, trusted) {
            (Some(signing), Some(trusted)) if signing.verifying_key() != trusted => {
                return Err(
                    "--trust-key does not match the public half of --signing-key; \
                            commits would be signed with a key the next start refuses"
                        .into(),
                );
            }
            (None, Some(_)) => {
                return Err(
                    "--trust-key without --signing-key would make every commit unpersistable; \
                     supply the matching --signing-key"
                        .into(),
                );
            }
            _ => {}
        }
        let trusted = trusted.or_else(|| signing.as_ref().map(SigningKey::verifying_key));
        Ok(Self { signing, trusted })
    }
}

/// Write a fresh 32-byte seed from the operating system's random source as a
/// hex file readable only by its owner, and return the public key.
fn generate_key(path: &Path) -> Result<VerifyingKey, String> {
    let mut seed = [0_u8; 32];
    {
        use std::io::Read as _;
        let mut source = std::fs::File::open("/dev/urandom")
            .map_err(|error| format!("cannot open /dev/urandom: {error}"))?;
        source
            .read_exact(&mut seed)
            .map_err(|error| format!("cannot read /dev/urandom: {error}"))?;
    }
    if seed == [0; 32] {
        return Err("random source produced an all-zero seed".into());
    }
    let key = SigningKey::from_seed(seed);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    writeln!(file, "{}", encode_hex(&seed)).map_err(|error| error.to_string())?;
    Ok(key.verifying_key())
}

impl Runtime {
    fn volatile() -> Self {
        Self::Volatile {
            world: World::default(),
            options: EvaluationOptions::default(),
        }
    }

    fn open_image(path: &Path, keys: &Keys) -> Result<(Self, bool), String> {
        let store = ImageStore::new(path);
        let existing = match &keys.trusted {
            Some(trusted) => store.load_verified(trusted),
            None => store.load(),
        }
        .map_err(|error| format!("cannot load image {}: {error}", path.display()))?;
        let (session, saved) = match existing {
            Some(image) => {
                let session = image
                    .rebuild()
                    .map_err(|error| format!("cannot rebuild image {}: {error}", path.display()))?;
                (session, Some(image.digest()))
            }
            None => (ImageSession::new(64, agel_core::Budget::default()), None),
        };
        let restored = saved.is_some();
        Ok((
            Self::Image {
                session,
                store,
                saved,
                signing: keys.signing.clone(),
            },
            restored,
        ))
    }

    fn world(&self) -> &World {
        match self {
            Self::Volatile { world, .. } => world,
            Self::Image { session, .. } => session.world(),
        }
    }

    fn options(&self) -> &EvaluationOptions {
        match self {
            Self::Volatile { options, .. } => options,
            Self::Image { session, .. } => session.options(),
        }
    }

    fn image(&self) -> Option<&Image> {
        match self {
            Self::Volatile { .. } => None,
            Self::Image { session, .. } => Some(session.image()),
        }
    }

    fn signer(&self) -> Option<VerifyingKey> {
        match self {
            Self::Image { signing, .. } => signing.as_ref().map(SigningKey::verifying_key),
            Self::Volatile { .. } => None,
        }
    }

    fn evaluate(&mut self, source: &str) -> Result<Commit, String> {
        match self {
            Self::Volatile { world, options } => world
                .evaluate_with(source, options)
                .map_err(|error| error.to_string()),
            Self::Image { session, .. } => {
                let commit = session
                    .evaluate(source)
                    .map_err(|error| error.to_string())?;
                self.persist()?;
                Ok(commit)
            }
        }
    }

    fn grant(&mut self, kind: &str, scope: &str) -> Result<(), String> {
        match self {
            Self::Volatile { world, options } => {
                let capability = world
                    .issue_capability(kind, scope)
                    .map_err(|error| error.to_string())?;
                options.capabilities.push(capability);
                Ok(())
            }
            Self::Image { session, .. } => {
                // A reconstructed image already replayed its grants; do not
                // append a duplicate entry on every start.
                if session
                    .options()
                    .capabilities
                    .iter()
                    .any(|capability| capability.kind() == kind && capability.scope() == scope)
                {
                    return Ok(());
                }
                session
                    .grant(kind, scope)
                    .map_err(|error| error.to_string())?;
                self.persist()
            }
        }
    }

    fn claim_model_request(&mut self, id: u64) -> Result<(Commit, ModelRequest), String> {
        match self {
            Self::Volatile { world, options } => world
                .claim_model_request(id, options)
                .map_err(|error| error.to_string()),
            Self::Image { session, .. } => {
                let result = session
                    .claim_model_request(id)
                    .map_err(|error| error.to_string())?;
                self.persist()?;
                Ok(result)
            }
        }
    }

    fn complete_model_request(&mut self, completion: ModelCompletion) -> Result<Commit, String> {
        match self {
            Self::Volatile { world, options } => world
                .complete_model_request(completion, options)
                .map_err(|error| error.to_string()),
            Self::Image { session, .. } => {
                let commit = session
                    .complete_model_request(completion)
                    .map_err(|error| error.to_string())?;
                self.persist()?;
                Ok(commit)
            }
        }
    }

    fn rollback(&mut self) -> Result<Option<u64>, String> {
        match self {
            Self::Volatile { world, .. } => Ok(world.rollback()),
            Self::Image { .. } => Err(
                "an image is an append-only log of committed inputs; rollback is not recorded there"
                    .into(),
            ),
        }
    }

    fn restore_snapshot(&mut self, snapshot: &Snapshot) -> Result<u64, String> {
        match self {
            Self::Volatile { world, .. } => world
                .restore_snapshot(snapshot)
                .map_err(|error| error.to_string()),
            Self::Image { .. } => Err(
                "an image is an append-only log of committed inputs; restore is not recorded there"
                    .into(),
            ),
        }
    }

    /// Atomically replace the image file with the current log. The in-memory
    /// world has already advanced; a failed save is reported and retried on
    /// the next commit against the same expected root, so a concurrent writer
    /// is detected rather than silently overwritten.
    fn persist(&mut self) -> Result<(), String> {
        let Self::Image {
            session,
            store,
            saved,
            signing,
        } = self
        else {
            return Ok(());
        };
        let root = match signing {
            Some(key) => store.save_signed(session.image(), *saved, key),
            None => store.save(session.image(), *saved),
        }
        .map_err(|error| format!("image not saved to {}: {error}", store.path().display()))?;
        *saved = Some(root);
        Ok(())
    }
}

/// A verified proposal waiting for an explicit promotion decision.
struct PendingProposal {
    path: PathBuf,
    proposal: Proposal,
    evidence: Evidence,
}

/// Read a proposal file. Ordinary lines are Agel source. `;effect NAME`
/// declares an effect and `;test EXPR => EXPECTED` adds an executable test
/// whose expected value is evaluated in an empty world.
fn read_proposal(world: &World, path: &Path, effects: &[String]) -> Result<Proposal, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut source = String::new();
    let mut proposal_effects = effects.to_vec();
    let mut tests = Vec::new();
    for line in text.lines() {
        if let Some(effect) = line.strip_prefix(";effect ") {
            proposal_effects.push(effect.trim().to_owned());
        } else if let Some(test) = line.strip_prefix(";test ") {
            let (expression, expected) = test
                .split_once("=>")
                .ok_or_else(|| format!("test line needs `EXPR => EXPECTED`: {line}"))?;
            let expected = World::default()
                .evaluate(expected.trim())
                .map_err(|error| format!("invalid expected value {:?}: {error}", expected.trim()))?
                .values
                .pop()
                .unwrap_or(Value::Nil);
            tests.push(TestCase::new(expression.trim(), expected));
        } else {
            source.push_str(line);
            source.push('\n');
        }
    }
    let mut proposal = Proposal::new(world, source);
    for effect in proposal_effects {
        proposal = proposal.declares(effect);
    }
    for test in tests {
        proposal = proposal.tests(test);
    }
    Ok(proposal)
}

fn main() -> io::Result<()> {
    let Some(config) = CliConfig::from_args(std::env::args().skip(1))
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?
    else {
        print_usage();
        return Ok(());
    };
    if let Some(path) = &config.keygen {
        let public = generate_key(path).map_err(io::Error::other)?;
        println!("wrote signing seed to {}", path.display());
        println!("public key {public}");
        return Ok(());
    }
    let keys = Keys::load(config.signing_key.as_deref(), config.trust_key.as_deref())
        .map_err(io::Error::other)?;
    let (mut runtime, restored) = match &config.image {
        Some(path) => Runtime::open_image(path, &keys).map_err(io::Error::other)?,
        None => (Runtime::volatile(), false),
    };
    let mut providers = ProviderRegistry::default();
    let installed_modules = if config.stdlib && !restored {
        let commit = runtime.evaluate(agel_stdlib::SOURCE).map_err(|error| {
            io::Error::other(format!("cannot install standard library: {error}"))
        })?;
        commit
            .values
            .into_iter()
            .filter_map(|value| match value {
                Value::Module(name) => Some(name),
                _ => None,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut limits = CommandLimits::new(&config.workspace);
    limits.timeout = config.timeout;
    limits.max_output_bytes = config.max_output_bytes;
    if config.claude {
        let mut provider = ClaudeCodeProvider::new(&config.claude_bin, limits.clone());
        if let Some(model) = config.claude_model {
            provider = provider.with_model(model);
        }
        if let Some(amount) = config.claude_max_budget_usd {
            provider = provider.with_max_budget_usd(amount);
        }
        providers.register(provider);
        runtime
            .grant("model/infer", "claude")
            .map_err(io::Error::other)?;
    }
    if config.codex {
        let mut provider = CodexProvider::new(&config.codex_bin, limits);
        if let Some(model) = config.codex_model {
            provider = provider.with_model(model);
        }
        providers.register(provider);
        runtime
            .grant("model/infer", "codex")
            .map_err(io::Error::other)?;
    }
    let stdin = io::stdin();
    let mut line = String::new();
    let mut source = String::new();
    let mut last_steps = 0;
    let mut snapshots = BTreeMap::<String, Snapshot>::new();
    let mut pending = None::<PendingProposal>;

    println!(
        "Agel agentic runtime — world revision {}",
        runtime.world().revision()
    );
    if let (Some(path), Some(image)) = (&config.image, runtime.image()) {
        println!(
            "Portable image {}: {} committed inputs, root {}{}",
            path.display(),
            image.len(),
            image.digest(),
            match (restored, &keys.trusted) {
                (true, Some(_)) => " (signature verified, reconstructed by replay)",
                (true, None) => " (unsigned, reconstructed by replay)",
                (false, _) => " (new)",
            }
        );
        if let Some(signer) = runtime.signer() {
            println!("Commits are signed by {signer}");
        }
    }
    if config.stdlib && !restored {
        println!(
            "Standard library installed: {}",
            installed_modules.join(", ")
        );
    }
    if providers.names().next().is_none() {
        println!("Model providers disabled; opt in with --enable-claude or --enable-codex.");
    } else {
        println!(
            "Enabled model providers: {} (invocation still requires :dispatch)",
            providers.names().collect::<Vec<_>>().join(", ")
        );
    }
    println!("Enter Lisp forms or :help.");

    loop {
        if source.is_empty() {
            print!("agel[{}]> ", runtime.world().revision());
        } else {
            print!("       ... ");
        }
        io::stdout().flush()?;
        line.clear();
        if stdin.read_line(&mut line)? == 0 {
            println!();
            break;
        }

        if source.is_empty() && line.trim().starts_with(':') {
            let mut command = line.split_whitespace();
            match command.next().expect("a command starts with ':'") {
                ":quit" | ":q" => break,
                ":help" => print_help(),
                ":revision" => println!("revision {}", runtime.world().revision()),
                ":stats" => println!(
                    "revision {}, last transaction {} steps",
                    runtime.world().revision(),
                    last_steps
                ),
                ":budget" => {
                    let budget = &runtime.options().budget;
                    println!(
                        "fuel={}, call-depth={}, collection={}, source-bytes={}, parse-depth={}",
                        budget.fuel,
                        budget.max_call_depth,
                        budget.max_collection_len,
                        budget.max_source_bytes,
                        budget.max_parse_depth
                    )
                }
                ":providers" => {
                    let names = providers.names().collect::<Vec<_>>();
                    if names.is_empty() {
                        println!("no model providers enabled");
                    } else {
                        println!("enabled: {}", names.join(", "));
                    }
                }
                ":effects" => {
                    for entry in providers.audit_records() {
                        println!(
                            "{} #{} {} {:?} {:?}",
                            entry.provider,
                            entry.record.sequence,
                            entry.record.key,
                            entry.record.intent.kind,
                            entry.record.outcome
                        );
                    }
                }
                ":requests" => {
                    for request in runtime.world().pending_model_requests() {
                        println!(
                            "#{} {} agent:{} -> agent:{} ({} prompt bytes)",
                            request.id,
                            request.provider,
                            request.requester,
                            request.reply_to,
                            request.prompt.len()
                        );
                    }
                    for request in runtime.world().dispatching_model_requests() {
                        println!(
                            "#{} {} agent:{} -> agent:{} (dispatching/in-doubt)",
                            request.id, request.provider, request.requester, request.reply_to
                        );
                    }
                }
                ":dispatch" => dispatch_pending(&mut runtime, &providers),
                ":rollback" => match runtime.rollback() {
                    Ok(Some(revision)) => println!("restored revision {revision}"),
                    Ok(None) => println!("no retained revision to restore"),
                    Err(error) => eprintln!("cannot roll back: {error}"),
                },
                ":events" => {
                    for event in runtime.world().events() {
                        println!(
                            "#{} {} agent:{} {}",
                            event.sequence,
                            event.kind.name(),
                            event.agent,
                            event.detail
                        );
                    }
                }
                ":snapshot" => match command.next() {
                    Some(name) if command.next().is_none() => {
                        let snapshot = runtime.world().snapshot();
                        println!(
                            "saved {name} at revision {} digest {:016x}",
                            snapshot.revision(),
                            snapshot.digest()
                        );
                        snapshots.insert(name.to_owned(), snapshot);
                    }
                    _ => eprintln!("usage: :snapshot NAME"),
                },
                ":restore" => match command.next() {
                    Some(name) if command.next().is_none() => match snapshots.get(name) {
                        Some(snapshot) => match runtime.restore_snapshot(snapshot) {
                            Ok(revision) => println!("restored {name} as revision {revision}"),
                            Err(error) => eprintln!("cannot restore {name}: {error}"),
                        },
                        None => eprintln!("unknown snapshot: {name}"),
                    },
                    _ => eprintln!("usage: :restore NAME"),
                },
                ":snapshots" => {
                    for (name, snapshot) in &snapshots {
                        println!(
                            "{name}: revision {} digest {:016x}",
                            snapshot.revision(),
                            snapshot.digest()
                        );
                    }
                }
                ":image" => match (&config.image, runtime.image()) {
                    (Some(path), Some(image)) => println!(
                        "{}: {} committed inputs, root {}, budget fuel {}, signer {}",
                        path.display(),
                        image.len(),
                        image.digest(),
                        image.budget().fuel,
                        runtime
                            .signer()
                            .map_or("none (unsigned)".to_owned(), |key| key.to_hex())
                    ),
                    _ => {
                        println!("no portable image; start with --image PATH to persist this world")
                    }
                },
                ":propose" => match command.next() {
                    Some(path) => {
                        let effects = command.map(str::to_owned).collect::<Vec<_>>();
                        pending = None;
                        match read_proposal(runtime.world(), Path::new(path), &effects) {
                            Ok(proposal) => match Verifier::verify(runtime.world(), &proposal) {
                                Ok(evidence) => {
                                    print_evidence(&proposal, &evidence);
                                    println!("enter :promote to adopt it or :discard to drop it");
                                    pending = Some(PendingProposal {
                                        path: path.into(),
                                        proposal,
                                        evidence,
                                    });
                                }
                                Err(error) => eprintln!("proposal rejected: {error}"),
                            },
                            Err(error) => eprintln!("{error}"),
                        }
                    }
                    None => eprintln!("usage: :propose FILE [EFFECT ...]"),
                },
                ":proposal" => match &pending {
                    Some(pending) => {
                        println!("{}", pending.path.display());
                        print_evidence(&pending.proposal, &pending.evidence);
                    }
                    None => println!("no verified proposal is pending"),
                },
                ":promote" => match pending.take() {
                    Some(candidate) => {
                        match promote(&mut runtime, &candidate.proposal, &candidate.evidence) {
                            Ok(commit) => {
                                last_steps = commit.steps_used;
                                println!(
                                    "promoted {} as revision {}",
                                    candidate.path.display(),
                                    commit.revision
                                );
                            }
                            Err(error) => eprintln!("promotion refused: {error}"),
                        }
                    }
                    None => eprintln!("nothing to promote; verify a proposal with :propose FILE"),
                },
                ":discard" => {
                    if pending.take().is_some() {
                        println!("proposal discarded; the live world is unchanged");
                    } else {
                        println!("no verified proposal is pending");
                    }
                }
                unknown => eprintln!("unknown command: {unknown}"),
            }
            continue;
        }

        source.push_str(&line);
        match read_all(&source) {
            Err(error) if is_incomplete(&error) => continue,
            _ => {}
        }

        match runtime.evaluate(&source) {
            Ok(commit) => {
                last_steps = commit.steps_used;
                for value in commit.values {
                    println!("{value}");
                }
                if pending.take().is_some() {
                    println!("pending proposal dropped: its base revision is no longer live");
                }
            }
            Err(error) => eprintln!("{error} (transaction aborted)"),
        }
        source.clear();
    }

    Ok(())
}

/// Promote a verified proposal through the runtime, so that in image mode the
/// promoted source is recorded like any other committed input. The evidence
/// binding is rechecked against the live world immediately before submission.
fn promote(
    runtime: &mut Runtime,
    proposal: &Proposal,
    evidence: &Evidence,
) -> Result<Commit, String> {
    Verifier::check_promotion(runtime.world(), proposal, evidence)
        .map_err(|error| error.to_string())?;
    runtime.evaluate(&proposal.source)
}

fn print_evidence(proposal: &Proposal, evidence: &Evidence) {
    println!(
        "proposal {} on base revision {}",
        evidence.proposal_digest, proposal.base_revision
    );
    println!(
        "  declared effects: {}",
        join_or_none(proposal.declared_effects.iter())
    );
    println!(
        "  inferred effects: {}",
        join_or_none(evidence.inferred_effects.iter())
    );
    println!(
        "  {} tests passed in a zero-authority canary; candidate digest {}",
        evidence.tests_passed, evidence.candidate_digest
    );
}

fn join_or_none<'a>(values: impl Iterator<Item = &'a String>) -> String {
    let joined = values.cloned().collect::<Vec<_>>().join(", ");
    if joined.is_empty() {
        "none".into()
    } else {
        joined
    }
}

fn dispatch_pending(runtime: &mut Runtime, providers: &ProviderRegistry) {
    let requests = runtime.world().pending_model_requests();
    if requests.is_empty() {
        println!("no pending model requests");
        return;
    }
    for request in requests {
        if !providers.is_enabled(&request.provider) {
            eprintln!(
                "request #{} remains pending: provider {} is not enabled",
                request.id, request.provider
            );
            continue;
        }
        let request = match runtime.claim_model_request(request.id) {
            Ok((_, request)) => request,
            Err(error) => {
                eprintln!("could not claim request #{}: {error}", request.id);
                continue;
            }
        };
        println!(
            "dispatching request #{} to {}...",
            request.id, request.provider
        );
        let outcome = match providers.infer(&request) {
            Ok(text) => {
                println!("request #{} completed ({} bytes)", request.id, text.len());
                ModelOutcome::Success(text)
            }
            Err(error) => {
                eprintln!("request #{} failed: {error}", request.id);
                provider_failure(error)
            }
        };
        if let Err(error) = runtime.complete_model_request(ModelCompletion {
            request_id: request.id,
            effect_key: request.effect_key,
            outcome,
        }) {
            eprintln!("could not commit request #{} result: {error}", request.id);
        }
    }
}

fn provider_failure(error: ProviderError) -> ModelOutcome {
    error.into_outcome()
}

fn is_incomplete(error: &ReadError) -> bool {
    error.message.starts_with("unterminated") || error.message == "expected an expression"
}

fn print_help() {
    println!(":revision  show the current committed revision");
    println!(":rollback  restore the preceding retained revision (volatile worlds only)");
    println!(":stats     show revision and last transaction fuel use");
    println!(":budget    show default deterministic resource limits");
    println!(":events    show the agent event log");
    println!(":providers show model providers enabled at startup");
    println!(":effects   show typed host-effect decisions and outcomes");
    println!(":requests  show committed model requests awaiting dispatch");
    println!(":dispatch  explicitly invoke enabled providers for pending requests");
    println!(":snapshot NAME  save an in-memory world snapshot");
    println!(":restore NAME   restore a snapshot as a new revision (volatile worlds only)");
    println!(":snapshots      list saved snapshots");
    println!(":image          show the portable image this world is persisted to");
    println!(":propose FILE [EFFECT ...]  verify a proposal file in a zero-authority canary");
    println!(":proposal       show the pending verified proposal and its evidence");
    println!(":promote        atomically commit the pending verified proposal");
    println!(":discard        drop the pending proposal without changing the world");
    println!(":quit      exit the REPL");
    println!("Balanced multi-line forms commit as one transaction.");
    println!(
        "Proposal files are Agel source plus `;effect NAME` and `;test EXPR => EXPECTED` lines."
    );
}

fn print_usage() {
    println!("Usage: agel-cli [OPTIONS]");
    println!("  --image PATH                 persist every committed input to a portable image");
    println!(
        "  --signing-key FILE           sign every image root with this hex seed and verify loads"
    );
    println!("  --trust-key FILE             public key loads must verify against (must match --signing-key)");
    println!("  --keygen FILE                write a fresh signing seed to FILE, print its public key, exit");
    println!("  --enable-claude              enable restricted Claude Code dispatch");
    println!("  --enable-codex               enable read-only Codex dispatch");
    println!("  --no-stdlib                  start with only the postcard-sized core");
    println!("  --claude-bin PATH            Claude executable (default: claude)");
    println!("  --codex-bin PATH             Codex executable (default: codex)");
    println!("  --claude-model NAME          select a Claude model");
    println!("  --codex-model NAME           select a Codex model");
    println!("  --claude-max-budget-usd N    cap one Claude CLI invocation");
    println!("  --model-workspace PATH       provider working directory");
    println!("  --model-timeout-seconds N    process timeout (default: 300)");
    println!("  --model-max-output-bytes N   captured output limit (default: 1048576)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use agel_model::{Provider, ProviderError};

    struct FakeProvider;

    impl Provider for FakeProvider {
        fn name(&self) -> &str {
            "claude"
        }

        fn infer(&self, request: &ModelRequest) -> Result<String, ProviderError> {
            Ok(format!("fake answer to: {}", request.prompt))
        }
    }

    fn temporary_path(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("agel-cli-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&directory).unwrap();
        directory.join("world.image")
    }

    const MODEL_AGENT: &str = "(def cap (request-capability 'model/infer \"claude\"))
         (def behavior
           (fn (self heap message)
             (if (= (car message) 'ask)
                 (begin (model-request 'claude (car (cdr message)) self) heap)
                 (car (cdr (cdr (cdr message)))))))
         (def agent (spawn \"model-agent\" behavior nil nil nil 'stop 0 (list cap)))
         (send agent '(ask \"hello\"))
         (run)";

    #[test]
    fn only_recoverable_reader_errors_request_more_input() {
        assert!(is_incomplete(&ReadError {
            offset: 0,
            message: "unterminated list".into(),
        }));
        assert!(!is_incomplete(&ReadError {
            offset: 0,
            message: "unexpected ')'".into(),
        }));
    }

    #[test]
    fn dispatch_commits_fake_provider_output_for_an_agent() {
        let mut runtime = Runtime::volatile();
        runtime.grant("model/infer", "claude").unwrap();
        runtime.evaluate(MODEL_AGENT).unwrap();
        let mut providers = ProviderRegistry::default();
        providers.register(FakeProvider);
        dispatch_pending(&mut runtime, &providers);
        runtime.evaluate("(run)").unwrap();
        assert_eq!(
            runtime
                .evaluate("(get (agent-info agent) 'heap)")
                .unwrap()
                .values[0]
                .to_string(),
            "\"fake answer to: hello\""
        );
    }

    #[test]
    fn providers_remain_disabled_without_explicit_flags() {
        let config = CliConfig::from_args(Vec::new()).unwrap().unwrap();
        assert!(!config.claude);
        assert!(!config.codex);
        assert!(config.image.is_none());
    }

    #[test]
    fn image_mode_persists_commits_grants_and_model_results_across_restarts() {
        let path = temporary_path("persist");
        let _ = std::fs::remove_file(&path);
        {
            let (mut runtime, restored) = Runtime::open_image(&path, &Keys::default()).unwrap();
            assert!(!restored);
            runtime.grant("model/infer", "claude").unwrap();
            runtime.evaluate("(def answer (+ 20 22))").unwrap();
            runtime.evaluate(MODEL_AGENT).unwrap();
            let mut providers = ProviderRegistry::default();
            providers.register(FakeProvider);
            dispatch_pending(&mut runtime, &providers);
            runtime.evaluate("(run)").unwrap();
            assert!(runtime.rollback().is_err());
            let snapshot = runtime.world().snapshot();
            assert!(runtime.restore_snapshot(&snapshot).is_err());
            assert_eq!(runtime.image().unwrap().len(), 6);
        }
        let (runtime, restored) = Runtime::open_image(&path, &Keys::default()).unwrap();
        assert!(restored);
        assert_eq!(runtime.world().binding("answer"), Some(&Value::Int(42)));
        // The provider was never re-invoked: the recorded completion replays.
        assert_eq!(
            runtime
                .world()
                .binding("agent")
                .and_then(|value| match value {
                    Value::Agent(id) => runtime.world().agent_name(*id).map(str::to_owned),
                    _ => None,
                })
                .as_deref(),
            Some("model-agent")
        );
        let mut runtime = runtime;
        assert_eq!(
            runtime
                .evaluate("(get (agent-info agent) 'heap)")
                .unwrap()
                .values[0]
                .to_string(),
            "\"fake answer to: hello\""
        );
        assert_eq!(runtime.image().unwrap().len(), 7);
        assert!(ImageStore::new(&path).load().unwrap().is_some());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn proposals_are_verified_in_a_canary_and_promoted_atomically() {
        let directory = temporary_path("proposal");
        let directory = directory.parent().unwrap().to_path_buf();
        let proposal_path = directory.join("upgrade.agel");
        let mut runtime = Runtime::volatile();
        runtime
            .evaluate("(def transform (fn (x) (+ x 1)))")
            .unwrap();

        std::fs::write(
            &proposal_path,
            ";test (transform 9) => 10\n(def transform (fn (x) (/ x 0)))\n",
        )
        .unwrap();
        let broken = read_proposal(runtime.world(), &proposal_path, &[]).unwrap();
        assert_eq!(broken.tests.len(), 1);
        assert!(Verifier::verify(runtime.world(), &broken).is_err());
        assert_eq!(
            runtime.evaluate("(transform 9)").unwrap().values[0],
            Value::Int(10)
        );

        std::fs::write(
            &proposal_path,
            ";test (transform 9) => 10\n;test (transform -1) => 0\n;test (transform-list '(1)) => '(2)\n\
             (def transform (fn (x) (+ 1 x)))\n(def transform-list (fn (xs) (list (transform (car xs)))))\n",
        )
        .unwrap();
        let proposal = read_proposal(runtime.world(), &proposal_path, &[]).unwrap();
        assert_eq!(proposal.tests.len(), 3);
        let evidence = Verifier::verify(runtime.world(), &proposal).unwrap();
        assert_eq!(evidence.tests_passed, 3);
        // An intervening commit invalidates the evidence binding.
        runtime.evaluate("(def intervening 1)").unwrap();
        assert!(promote(&mut runtime, &proposal, &evidence).is_err());
        let proposal = read_proposal(runtime.world(), &proposal_path, &[]).unwrap();
        let evidence = Verifier::verify(runtime.world(), &proposal).unwrap();
        let commit = promote(&mut runtime, &proposal, &evidence).unwrap();
        assert_eq!(commit.values.len(), 2);
        assert_eq!(
            runtime.evaluate("(transform-list '(41))").unwrap().values[0],
            Value::List(vec![Value::Int(42)])
        );

        std::fs::write(
            &proposal_path,
            ";effect model/infer\n(def ask (fn (self) (model-request 'claude \"x\" self)))\n",
        )
        .unwrap();
        let effectful = read_proposal(runtime.world(), &proposal_path, &[]).unwrap();
        assert!(effectful.declared_effects.contains("model/infer"));
        let evidence = Verifier::verify(runtime.world(), &effectful).unwrap();
        assert!(evidence.inferred_effects.contains("model/infer"));
        let undeclared = read_proposal(runtime.world(), &proposal_path, &[]).map(|mut p| {
            p.declared_effects.clear();
            p
        });
        assert!(Verifier::verify(runtime.world(), &undeclared.unwrap()).is_err());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn signed_images_verify_on_restart_and_refuse_other_keys() {
        let path = temporary_path("signed");
        let _ = std::fs::remove_file(&path);
        let seed_path = path.parent().unwrap().join("seed.hex");
        let _ = std::fs::remove_file(&seed_path);
        let public = generate_key(&seed_path).unwrap();
        let seed = std::fs::read_to_string(&seed_path).unwrap();
        assert_eq!(seed.trim().len(), 64);
        let keys = Keys::load(Some(&seed_path), None).unwrap();
        assert_eq!(keys.trusted, Some(public));
        {
            let (mut runtime, restored) = Runtime::open_image(&path, &keys).unwrap();
            assert!(!restored);
            runtime.evaluate("(def signed 42)").unwrap();
            assert_eq!(runtime.signer(), Some(public));
        }
        let (runtime, restored) = Runtime::open_image(&path, &keys).unwrap();
        assert!(restored);
        assert_eq!(runtime.world().binding("signed"), Some(&Value::Int(42)));
        // An unsigned reader refuses the signed store rather than downgrading.
        assert!(Runtime::open_image(&path, &Keys::default()).is_err());
        // A different key refuses it too.
        let other_path = path.parent().unwrap().join("other.hex");
        let _ = std::fs::remove_file(&other_path);
        generate_key(&other_path).unwrap();
        let other = Keys::load(Some(&other_path), None).unwrap();
        assert!(Runtime::open_image(&path, &other).is_err());
        // Mismatched trust and signing keys are refused at startup.
        let trust_path = path.parent().unwrap().join("trust.hex");
        std::fs::write(&trust_path, other.trusted.unwrap().to_hex()).unwrap();
        assert!(Keys::load(Some(&seed_path), Some(&trust_path)).is_err());
        assert!(Keys::load(None, Some(&trust_path)).is_err());
        std::fs::write(&trust_path, public.to_hex()).unwrap();
        assert!(Keys::load(Some(&seed_path), Some(&trust_path)).is_ok());
        assert!(
            generate_key(&seed_path).is_err(),
            "must not overwrite a seed"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn promotion_in_image_mode_is_recorded_as_a_committed_input() {
        let path = temporary_path("promotion");
        let _ = std::fs::remove_file(&path);
        let proposal_path = path.parent().unwrap().join("upgrade.agel");
        std::fs::write(
            &proposal_path,
            ";test (square 9) => 81\n(def square (fn (x) (* x x)))\n",
        )
        .unwrap();
        {
            let (mut runtime, _) = Runtime::open_image(&path, &Keys::default()).unwrap();
            let proposal = read_proposal(runtime.world(), &proposal_path, &[]).unwrap();
            let evidence = Verifier::verify(runtime.world(), &proposal).unwrap();
            promote(&mut runtime, &proposal, &evidence).unwrap();
            assert_eq!(runtime.image().unwrap().len(), 1);
        }
        let (runtime, restored) = Runtime::open_image(&path, &Keys::default()).unwrap();
        assert!(restored);
        assert!(runtime.world().binding("square").is_some());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
