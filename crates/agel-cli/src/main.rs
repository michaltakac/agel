use agel_core::{
    read_all, EvaluationOptions, ModelCompletion, ModelOutcome, ReadError, Snapshot, World,
};
use agel_model::{
    ClaudeCodeProvider, CodexProvider, CommandLimits, ProviderError, ProviderRegistry,
};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
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
        };
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--help" | "-h" => return Ok(None),
                "--enable-claude" => config.claude = true,
                "--enable-codex" => config.codex = true,
                "--no-stdlib" => config.stdlib = false,
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

fn main() -> io::Result<()> {
    let Some(config) = CliConfig::from_args(std::env::args().skip(1))
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?
    else {
        print_usage();
        return Ok(());
    };
    let mut world = World::default();
    let mut options = EvaluationOptions::default();
    let mut providers = ProviderRegistry::default();
    if config.stdlib {
        agel_stdlib::install(&mut world, &options).map_err(|error| {
            io::Error::other(format!("cannot install standard library: {error}"))
        })?;
    }
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
        options.capabilities.push(
            world
                .issue_capability("model/infer", "claude")
                .expect("fresh world has capability ids"),
        );
    }
    if config.codex {
        let mut provider = CodexProvider::new(&config.codex_bin, limits);
        if let Some(model) = config.codex_model {
            provider = provider.with_model(model);
        }
        providers.register(provider);
        options.capabilities.push(
            world
                .issue_capability("model/infer", "codex")
                .expect("fresh world has capability ids"),
        );
    }
    let stdin = io::stdin();
    let mut line = String::new();
    let mut source = String::new();
    let mut last_steps = 0;
    let mut snapshots = BTreeMap::<String, Snapshot>::new();

    println!("Agel agentic runtime — world revision {}", world.revision());
    if config.stdlib {
        println!(
            "Standard library installed: agel/sequence, agel/result, agel/swarm, agel/meta, agel/ui, agel/vector, agel/ui-layout, agel/ui-vector, agel/desktop"
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
            print!("agel[{}]> ", world.revision());
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
                ":revision" => println!("revision {}", world.revision()),
                ":stats" => println!(
                    "revision {}, last transaction {} steps",
                    world.revision(),
                    last_steps
                ),
                ":budget" => println!(
                    "fuel={}, call-depth={}, collection={}, source-bytes={}, parse-depth={}",
                    options.budget.fuel,
                    options.budget.max_call_depth,
                    options.budget.max_collection_len,
                    options.budget.max_source_bytes,
                    options.budget.max_parse_depth
                ),
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
                    for request in world.pending_model_requests() {
                        println!(
                            "#{} {} agent:{} -> agent:{} ({} prompt bytes)",
                            request.id,
                            request.provider,
                            request.requester,
                            request.reply_to,
                            request.prompt.len()
                        );
                    }
                    for request in world.dispatching_model_requests() {
                        println!(
                            "#{} {} agent:{} -> agent:{} (dispatching/in-doubt)",
                            request.id, request.provider, request.requester, request.reply_to
                        );
                    }
                }
                ":dispatch" => dispatch_pending(&mut world, &providers, &options),
                ":rollback" => match world.rollback() {
                    Some(revision) => println!("restored revision {revision}"),
                    None => println!("no retained revision to restore"),
                },
                ":events" => {
                    for event in world.events() {
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
                        let snapshot = world.snapshot();
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
                        Some(snapshot) => match world.restore_snapshot(snapshot) {
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
                unknown => eprintln!("unknown command: {unknown}"),
            }
            continue;
        }

        source.push_str(&line);
        match read_all(&source) {
            Err(error) if is_incomplete(&error) => continue,
            _ => {}
        }

        match world.evaluate_with(&source, &options) {
            Ok(commit) => {
                last_steps = commit.steps_used;
                for value in commit.values {
                    println!("{value}");
                }
            }
            Err(error) => eprintln!("{error} (transaction aborted)"),
        }
        source.clear();
    }

    Ok(())
}

fn dispatch_pending(world: &mut World, providers: &ProviderRegistry, options: &EvaluationOptions) {
    let requests = world.pending_model_requests();
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
        let request = match world.claim_model_request(request.id, options) {
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
        if let Err(error) = world.complete_model_request(
            ModelCompletion {
                request_id: request.id,
                effect_key: request.effect_key,
                outcome,
            },
            options,
        ) {
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
    println!(":rollback  restore the preceding retained revision");
    println!(":stats     show revision and last transaction fuel use");
    println!(":budget    show default deterministic resource limits");
    println!(":events    show the agent event log");
    println!(":providers show model providers enabled at startup");
    println!(":effects   show typed host-effect decisions and outcomes");
    println!(":requests  show committed model requests awaiting dispatch");
    println!(":dispatch  explicitly invoke enabled providers for pending requests");
    println!(":snapshot NAME  save an in-memory world snapshot");
    println!(":restore NAME   restore a snapshot as a new revision");
    println!(":snapshots      list saved snapshots");
    println!(":quit      exit the REPL");
    println!("Balanced multi-line forms commit as one transaction.");
}

fn print_usage() {
    println!("Usage: agel-cli [MODEL OPTIONS]");
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
    use agel_core::ModelRequest;
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
        let mut world = World::default();
        let capability = world.issue_capability("model/infer", "claude").unwrap();
        let options = EvaluationOptions {
            capabilities: vec![capability],
            ..EvaluationOptions::default()
        };
        world
            .evaluate_with(
                "(def cap (request-capability 'model/infer \"claude\"))
                 (def behavior
                   (fn (self heap message)
                     (if (= (car message) 'ask)
                         (begin (model-request 'claude (car (cdr message)) self) heap)
                         (car (cdr (cdr (cdr message)))))))
                 (def agent (spawn \"model-agent\" behavior nil nil nil 'stop 0 (list cap)))
                 (send agent '(ask \"hello\"))
                 (run)",
                &options,
            )
            .unwrap();
        let mut providers = ProviderRegistry::default();
        providers.register(FakeProvider);
        dispatch_pending(&mut world, &providers, &options);
        world.evaluate_with("(run)", &options).unwrap();
        assert_eq!(
            world
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
    }
}
