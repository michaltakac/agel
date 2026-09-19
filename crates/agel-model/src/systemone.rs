//! Typed judgments from System One models.
//!
//! A System One model (TypeSafe's Jev is the first) does not write text. It
//! reads a *state* and answers typed *questions* about it — a choice among
//! named options, a yes/no probability, a position on ordered levels — with
//! a probability distribution and a confidence, in one parallel pass. That
//! is a function call, and this module is its contract in Agel's terms: a
//! request is an Agel form, an answer is a line of integers, and the model
//! is a provider behind the same `model/infer` effect boundary the text
//! providers use. Policy stays in the program; the model only judges.
//!
//! The request grammar, as an Agel form (the state is optional; a bridge
//! that sees more than the program does supplies it):
//!
//! ```text
//! (judge [STATE] QUESTION...)
//! STATE    := "text" | (state (NAME "text")...)
//! QUESTION := (noul ID "instructions" ["yes means" "no means"])
//!           | (choice ID "instructions" OPTION OPTION...)
//!           | (score ID "instructions" "level" "level"...)
//! OPTION   := name | (name "description")
//! ```
//!
//! The answer, one line of space-separated tokens, one group per question
//! in request order, every probability in thousandths (rounded, so a
//! distribution may sum to 999 or 1001):
//!
//! ```text
//! ID noul YES
//! ID choice COUNT OPTION CONFIDENCE P...   ; P in the request's option order
//! ID score COUNT SCORE CONFIDENCE P...     ; SCORE in thousandths of a level
//! ```
//!
//! The line is self-describing (each group carries its count), so a program
//! reads it with nothing but text words, and the same line comes from the
//! judge written in Agel (`agel/judgment`) as from the hosted model.

use crate::{process_sandbox, CommandLimits, Provider, ProviderError};
use agel_core::{read_all, Expr, ModelRequest};
use agel_effects::{AuditRecord, Principal, ProcessSandbox, ProcessSpec};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// The endpoint TypeSafe documents; any server that speaks the same
/// request and answer shapes (self-hosted reproductions do) can replace it.
pub const TYPESAFE_URL: &str = "https://api.typesafe.ai/v1/systemone";
/// The model alias TypeSafe documents as its current flagship.
pub const DEFAULT_MODEL: &str = "jev-latest";
/// The environment variable the host reads the key from.
pub const KEY_VARIABLE: &str = "TYPESAFEAI_API_KEY";

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Text(String),
    Fields(Vec<(String, String)>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Question {
    Noul {
        instructions: String,
        yes: Option<String>,
        no: Option<String>,
    },
    Choice {
        instructions: String,
        options: Vec<(String, Option<String>)>,
    },
    Score {
        instructions: String,
        levels: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct JudgmentRequest {
    pub state: Option<State>,
    pub questions: Vec<(String, Question)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Noul {
        yes: f64,
    },
    Choice {
        choice: String,
        confidence: f64,
        /// In the request's option order.
        probabilities: Vec<f64>,
    },
    Score {
        score: f64,
        confidence: f64,
        /// In the request's level order.
        probabilities: Vec<f64>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Judgment {
    pub model: String,
    pub answers: Vec<(String, Answer)>,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

fn text(expr: &Expr, what: &str) -> Result<String, String> {
    match expr {
        Expr::String(value) => Ok(value.clone()),
        other => Err(format!("{what} must be text, not {other:?}")),
    }
}

fn name(expr: &Expr, what: &str) -> Result<String, String> {
    match expr {
        Expr::Symbol(value) | Expr::String(value) => Ok(value.clone()),
        other => Err(format!("{what} must be a name, not {other:?}")),
    }
}

impl Question {
    fn parse(expr: &Expr) -> Result<(String, Self), String> {
        let Expr::List(items) = expr else {
            return Err(format!("a question is a list, not {expr:?}"));
        };
        let (Some(Expr::Symbol(kind)), Some(id), Some(instructions)) =
            (items.first(), items.get(1), items.get(2))
        else {
            return Err("a question is (KIND ID \"instructions\" ...)".to_owned());
        };
        let id = name(id, "a question id")?;
        let instructions = text(instructions, "instructions")?;
        let rest = &items[3..];
        let question = match kind.as_str() {
            "noul" => {
                if !rest.is_empty() && rest.len() != 2 {
                    return Err(format!("noul {id} takes yes and no descriptions together"));
                }
                Self::Noul {
                    instructions,
                    yes: rest
                        .first()
                        .map(|e| text(e, "the yes description"))
                        .transpose()?,
                    no: rest
                        .get(1)
                        .map(|e| text(e, "the no description"))
                        .transpose()?,
                }
            }
            "choice" => {
                let options = rest
                    .iter()
                    .map(|option| match option {
                        Expr::List(pair) if pair.len() == 2 => Ok((
                            name(&pair[0], "an option")?,
                            Some(text(&pair[1], "an option description")?),
                        )),
                        other => Ok((name(other, "an option")?, None)),
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                if options.len() < 2 {
                    return Err(format!("choice {id} needs at least two options"));
                }
                Self::Choice {
                    instructions,
                    options,
                }
            }
            "score" => {
                let levels = rest
                    .iter()
                    .map(|level| text(level, "a level"))
                    .collect::<Result<Vec<_>, String>>()?;
                if levels.len() < 2 {
                    return Err(format!("score {id} needs at least two levels"));
                }
                Self::Score {
                    instructions,
                    levels,
                }
            }
            other => return Err(format!("unknown question kind {other}")),
        };
        Ok((id, question))
    }

    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Noul {
                instructions,
                yes,
                no,
            } => {
                let mut question =
                    serde_json::json!({"type": "noul", "instructions": instructions});
                if let (Some(yes), Some(no)) = (yes, no) {
                    question["criteria"] = serde_json::json!({"true": yes, "false": no});
                }
                question
            }
            Self::Choice {
                instructions,
                options,
            } => {
                let criteria = options
                    .iter()
                    .map(|(option, description)| {
                        (
                            option.clone(),
                            description
                                .as_ref()
                                .map_or(serde_json::Value::Null, |d| d.as_str().into()),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>();
                serde_json::json!({"type": "choice", "instructions": instructions, "criteria": criteria})
            }
            Self::Score {
                instructions,
                levels,
            } => {
                serde_json::json!({"type": "score", "instructions": instructions, "criteria": levels})
            }
        }
    }
}

impl JudgmentRequest {
    /// Read a request from its form text.
    pub fn parse(source: &str) -> Result<Self, String> {
        let forms = read_all(source).map_err(|error| error.to_string())?;
        let [Expr::List(items)] = forms.as_slice() else {
            return Err("a judgment request is one (judge ...) form".to_owned());
        };
        if !matches!(items.first(), Some(Expr::Symbol(head)) if head == "judge") {
            return Err("a judgment request starts with judge".to_owned());
        }
        let mut rest = &items[1..];
        let state = match rest.first() {
            Some(Expr::String(text)) => {
                rest = &rest[1..];
                Some(State::Text(text.clone()))
            }
            Some(Expr::List(fields)) if matches!(fields.first(), Some(Expr::Symbol(head)) if head == "state") =>
            {
                rest = &rest[1..];
                let fields = fields[1..]
                    .iter()
                    .map(|field| match field {
                        Expr::List(pair) if pair.len() == 2 => {
                            Ok((name(&pair[0], "a field name")?, text(&pair[1], "a field")?))
                        }
                        other => Err(format!("a state field is (NAME \"text\"), not {other:?}")),
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Some(State::Fields(fields))
            }
            Some(Expr::List(_)) | None => None,
            Some(other) => return Err(format!("the state is text or (state ...), not {other:?}")),
        };
        let questions = rest
            .iter()
            .map(Question::parse)
            .collect::<Result<Vec<_>, String>>()?;
        if questions.is_empty() {
            return Err("a judgment request asks at least one question".to_owned());
        }
        for (index, (id, _)) in questions.iter().enumerate() {
            if questions[..index].iter().any(|(other, _)| other == id) {
                return Err(format!("question id {id} is asked twice"));
            }
        }
        Ok(Self { state, questions })
    }

    /// Add what the bridge sees to the state: named fields beside the
    /// program's own, which becomes the `text` field when it was plain.
    pub fn add_fields(&mut self, fields: impl IntoIterator<Item = (String, String)>) {
        let mut all = match self.state.take() {
            Some(State::Fields(fields)) => fields,
            Some(State::Text(text)) => vec![("text".to_owned(), text)],
            None => Vec::new(),
        };
        all.extend(fields);
        self.state = Some(State::Fields(all));
    }

    pub fn to_json(&self, model: &str) -> serde_json::Value {
        let state = match &self.state {
            Some(State::Text(text)) => serde_json::Value::String(text.clone()),
            Some(State::Fields(fields)) => fields
                .iter()
                .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
                .collect::<serde_json::Map<_, _>>()
                .into(),
            None => serde_json::Value::String(String::new()),
        };
        let questions = self
            .questions
            .iter()
            .map(|(id, question)| (id.clone(), question.to_json()))
            .collect::<serde_json::Map<_, _>>();
        serde_json::json!({"state": state, "model": model, "questions": questions})
    }
}

/// A probability as an integer in thousandths.
pub fn thousandths(probability: f64) -> i64 {
    if probability.is_nan() {
        return 0;
    }
    ((probability * 1000.0).round() as i64).clamp(0, 1000)
}

fn number(value: &serde_json::Value, what: &str) -> Result<f64, String> {
    value
        .as_f64()
        .ok_or_else(|| format!("{what} is not a number: {value}"))
}

impl Judgment {
    /// Read the model's answers for a request; probabilities come back in
    /// the request's option and level order.
    pub fn from_json(request: &JudgmentRequest, body: &serde_json::Value) -> Result<Self, String> {
        let model = body["model"].as_str().unwrap_or("").to_owned();
        let answers = request
            .questions
            .iter()
            .map(|(id, question)| {
                let answer = &body["answers"][id];
                if answer.is_null() {
                    return Err(format!("no answer for {id}"));
                }
                let answer = match question {
                    Question::Noul { .. } => Answer::Noul {
                        yes: number(&answer["noul"], "noul")?,
                    },
                    Question::Choice { options, .. } => {
                        let choice = answer["choice"]
                            .as_str()
                            .ok_or_else(|| format!("{id} has no choice"))?
                            .to_owned();
                        if !options.iter().any(|(option, _)| *option == choice) {
                            return Err(format!("{id} chose {choice}, not an option"));
                        }
                        Answer::Choice {
                            choice,
                            confidence: number(&answer["confidence"], "confidence")?,
                            probabilities: options
                                .iter()
                                .map(|(option, _)| {
                                    number(&answer["probabilities"][option], "a probability")
                                })
                                .collect::<Result<Vec<_>, _>>()?,
                        }
                    }
                    Question::Score { levels, .. } => Answer::Score {
                        score: number(&answer["score"], "score")?,
                        confidence: number(&answer["confidence"], "confidence")?,
                        probabilities: (0..levels.len())
                            .map(|level| {
                                number(&answer["probabilities"][level.to_string()], "a probability")
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    },
                };
                Ok((id.clone(), answer))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            model,
            answers,
            input_tokens: body["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: body["usage"]["output_tokens"].as_u64().unwrap_or(0),
        })
    }

    /// The answer line a program reads.
    pub fn reply_text(&self) -> String {
        let mut tokens = Vec::new();
        for (id, answer) in &self.answers {
            tokens.push(id.clone());
            match answer {
                Answer::Noul { yes } => {
                    tokens.push("noul".to_owned());
                    tokens.push(thousandths(*yes).to_string());
                }
                Answer::Choice {
                    choice,
                    confidence,
                    probabilities,
                } => {
                    tokens.push("choice".to_owned());
                    tokens.push(probabilities.len().to_string());
                    tokens.push(choice.clone());
                    tokens.push(thousandths(*confidence).to_string());
                    tokens.extend(probabilities.iter().map(|p| thousandths(*p).to_string()));
                }
                Answer::Score {
                    score,
                    confidence,
                    probabilities,
                } => {
                    tokens.push("score".to_owned());
                    tokens.push(probabilities.len().to_string());
                    tokens.push(thousandths(*score).to_string());
                    tokens.push(thousandths(*confidence).to_string());
                    tokens.extend(probabilities.iter().map(|p| thousandths(*p).to_string()));
                }
            }
        }
        tokens.join(" ")
    }
}

/// TypeSafe's System One endpoint as a provider: `curl` runs in the same
/// audited process sandbox as the text providers, the key goes to it on
/// standard input as a configuration line (never in an argument or the
/// environment, never on disk), and the request body waits in a file in
/// the workspace for as long as the call takes.
#[derive(Clone, Debug)]
pub struct JevProvider {
    curl: PathBuf,
    key: String,
    url: String,
    model: String,
    limits: CommandLimits,
    sandbox: ProcessSandbox,
}

/// Overloaded and rate-limited answers are retried after these pauses.
const RETRY_PAUSES: [Duration; 2] = [Duration::from_millis(300), Duration::from_millis(900)];

impl JevProvider {
    pub fn new(curl: impl Into<PathBuf>, key: impl Into<String>, limits: CommandLimits) -> Self {
        let curl = curl.into();
        Self {
            sandbox: process_sandbox("jev", &curl, &limits),
            curl,
            key: key.into(),
            url: TYPESAFE_URL.to_owned(),
            model: DEFAULT_MODEL.to_owned(),
            limits,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn audit_log(&self) -> agel_effects::AuditLog {
        self.sandbox.audit_log()
    }

    /// Ask the model, as `principal`, under the effect name `operation`
    /// (which must lie under `model/infer/jev/request/`).
    pub fn judge(
        &self,
        request: &JudgmentRequest,
        principal: Principal,
        operation: &str,
    ) -> Result<Judgment, ProviderError> {
        let body = request.to_json(&self.model).to_string();
        let file = self.limits.workspace.join(format!(
            "jev-request-{}-{}.json",
            std::process::id(),
            operation.rsplit('/').next().unwrap_or("0")
        ));
        fs::write(&file, body.as_bytes()).map_err(|error| ProviderError::Io(error.to_string()))?;
        let outcome = self.post(&file, principal, operation);
        let _ = fs::remove_file(&file);
        let (status, text) = outcome?;
        if status != 200 {
            return Err(ProviderError::Failed {
                code: Some(status),
                stderr: text.trim().chars().take(400).collect(),
            });
        }
        let body: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| ProviderError::Unexpected(format!("not JSON: {error}")))?;
        Judgment::from_json(request, &body).map_err(ProviderError::Unexpected)
    }

    fn post(
        &self,
        file: &std::path::Path,
        principal: Principal,
        operation: &str,
    ) -> Result<(i32, String), ProviderError> {
        let arguments = vec![
            "-q".to_owned(),
            "-sS".to_owned(),
            "-K".to_owned(),
            "-".to_owned(),
            "--max-time".to_owned(),
            self.limits.timeout.as_secs().max(1).to_string(),
            "-H".to_owned(),
            "Content-Type: application/json".to_owned(),
            "--data-binary".to_owned(),
            format!("@{}", file.display()),
            "-o".to_owned(),
            "-".to_owned(),
            "-w".to_owned(),
            "\n%{http_code}".to_owned(),
            self.url.clone(),
        ];
        let configuration = format!("header = \"Authorization: Bearer {}\"\n", self.key);
        let mut attempt = 0;
        loop {
            let output = self
                .sandbox
                .run(
                    principal.clone(),
                    operation,
                    ProcessSpec {
                        executable: self.curl.clone(),
                        arguments: arguments.clone(),
                        stdin: configuration.clone().into_bytes(),
                    },
                )
                .map_err(crate::provider_effect_error)?;
            if output.status != 0 {
                return Err(ProviderError::Failed {
                    code: (output.status >= 0).then_some(output.status),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                });
            }
            let text = String::from_utf8(output.stdout).map_err(|_| ProviderError::InvalidUtf8)?;
            let (body, code) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
            let status: i32 = code.trim().parse().map_err(|_| {
                ProviderError::Unexpected(format!("no HTTP status after the answer: {code:?}"))
            })?;
            if (status == 429 || status == 529) && attempt < RETRY_PAUSES.len() {
                std::thread::sleep(RETRY_PAUSES[attempt]);
                attempt += 1;
                continue;
            }
            return Ok((status, body.to_owned()));
        }
    }
}

impl Provider for JevProvider {
    fn name(&self) -> &str {
        "jev"
    }

    /// The prompt is a judgment request form; the answer is its reply line.
    fn infer(&self, request: &ModelRequest) -> Result<String, ProviderError> {
        let judgment = JudgmentRequest::parse(&request.prompt).map_err(ProviderError::Rejected)?;
        self.judge(
            &judgment,
            Principal {
                world: request.world_id,
                agent: Some(request.requester),
            },
            &format!("model/infer/jev/request/{}", request.id),
        )
        .map(|judgment| judgment.reply_text())
    }

    fn audit_records(&self) -> Vec<AuditRecord> {
        self.audit_log().records()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    const DOOM: &str = r#"(judge (state (game "DOOM E1M1"))
        (choice act "Best next move" forward back left right (fire "shoot") use)
        (noul foe "Is an enemy in view?" "one is" "none is")
        (score risk "How dangerous?" "safe" "wary" "lethal"))"#;

    const ANSWER: &str = r#"{"model":"jev-1.13.0","answers":{
        "act":{"type":"choice","choice":"fire","confidence":0.33,
               "probabilities":{"left":0.22,"fire":0.44,"use":0.0,"forward":0.32,"back":0.0149,"right":0.01}},
        "foe":{"type":"noul","noul":0.98},
        "risk":{"type":"score","score":0.74,"confidence":0.6,"legend":{"0":"safe","1":"wary","2":"lethal"},
                "probabilities":{"0":0.27,"1":0.73,"2":0.0}}},
        "usage":{"input_tokens":509,"output_tokens":89}}"#;

    fn fake_curl(script: &str) -> (PathBuf, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "agel-jev-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let executable = directory.join("curl");
        // The fake keeps its configuration (stdin) and the body it was
        // pointed at, then answers as the script says.
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\ncat > \"$(dirname \"$0\")/config\"\nfor arg in \"$@\"; do case \"$arg\" in @*) cp \"${{arg#@}}\" \"$(dirname \"$0\")/body\";; esac; done\nprintf 'ARGS:' > \"$(dirname \"$0\")/args\"; for arg in \"$@\"; do printf '<%s>' \"$arg\" >> \"$(dirname \"$0\")/args\"; done\n{script}\n"
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&executable, permissions).unwrap();
        (directory, executable)
    }

    fn model_request(prompt: &str) -> ModelRequest {
        ModelRequest {
            id: 7,
            world_id: 11,
            requester: 1,
            reply_to: 1,
            provider: "jev".into(),
            prompt: prompt.into(),
            prompt_digest: agel_core::Digest::ZERO,
            effect_key: agel_core::Digest::ZERO,
        }
    }

    #[test]
    fn a_request_form_becomes_the_documented_json() {
        let request = JudgmentRequest::parse(DOOM).unwrap();
        assert_eq!(request.questions.len(), 3);
        let json = request.to_json("jev-latest");
        assert_eq!(json["model"], "jev-latest");
        assert_eq!(json["state"]["game"], "DOOM E1M1");
        assert_eq!(json["questions"]["act"]["type"], "choice");
        assert_eq!(json["questions"]["act"]["criteria"]["fire"], "shoot");
        assert!(json["questions"]["act"]["criteria"]["forward"].is_null());
        assert_eq!(json["questions"]["foe"]["criteria"]["true"], "one is");
        assert_eq!(json["questions"]["risk"]["criteria"][2], "lethal");
        let mut with_frame = request.clone();
        with_frame.add_fields([("frame".to_owned(), "###".to_owned())]);
        let json = with_frame.to_json("jev-latest");
        assert_eq!(json["state"]["game"], "DOOM E1M1");
        assert_eq!(json["state"]["frame"], "###");
        let mut plain = JudgmentRequest::parse("(judge \"hello\" (noul q \"Is it?\"))").unwrap();
        plain.add_fields([("frame".to_owned(), "###".to_owned())]);
        assert_eq!(plain.to_json("m")["state"]["text"], "hello");
        let stateless = JudgmentRequest::parse("(judge (noul q \"Is it?\"))").unwrap();
        assert_eq!(stateless.state, None);
        assert_eq!(stateless.to_json("m")["state"], "");
    }

    #[test]
    fn malformed_requests_are_refused_with_a_reason() {
        for (source, reason) in [
            ("(ask (noul q \"?\"))", "starts with judge"),
            ("(judge \"s\")", "at least one question"),
            ("(judge (choice c \"?\" one))", "at least two options"),
            ("(judge (score s \"?\" \"low\"))", "at least two levels"),
            (
                "(judge (noul q \"?\" \"yes\"))",
                "yes and no descriptions together",
            ),
            ("(judge (noul q \"?\") (noul q \"?\"))", "asked twice"),
            ("(judge (guess q \"?\"))", "unknown question kind"),
            ("(judge 42 (noul q \"?\"))", "text or (state ...)"),
            (
                "(judge (noul q \"?\")) (judge (noul q \"?\"))",
                "one (judge ...) form",
            ),
            ("(judge (noul q", "unterminated"),
        ] {
            let error = JudgmentRequest::parse(source).unwrap_err();
            assert!(error.contains(reason), "{source}: {error}");
        }
    }

    #[test]
    fn answers_are_read_in_request_order_and_written_in_thousandths() {
        let request = JudgmentRequest::parse(DOOM).unwrap();
        let body: serde_json::Value = serde_json::from_str(ANSWER).unwrap();
        let judgment = Judgment::from_json(&request, &body).unwrap();
        assert_eq!(judgment.model, "jev-1.13.0");
        assert_eq!((judgment.input_tokens, judgment.output_tokens), (509, 89));
        assert_eq!(
            judgment.reply_text(),
            "act choice 6 fire 330 320 15 220 10 440 0 foe noul 980 risk score 3 740 600 270 730 0"
        );
        let mut wrong = body.clone();
        wrong["answers"]["act"]["choice"] = "reload".into();
        assert!(Judgment::from_json(&request, &wrong)
            .unwrap_err()
            .contains("not an option"));
        let mut missing = body.clone();
        missing["answers"]["risk"] = serde_json::Value::Null;
        assert!(Judgment::from_json(&request, &missing)
            .unwrap_err()
            .contains("no answer for risk"));
        assert_eq!(thousandths(f64::NAN), 0);
        assert_eq!(thousandths(1.7), 1000);
        assert_eq!(thousandths(-0.2), 0);
        assert_eq!(thousandths(0.0005), 1);
    }

    #[test]
    fn the_provider_posts_the_body_with_the_key_on_stdin_and_answers_a_line() {
        let (directory, curl) =
            fake_curl(&format!("printf '%s\\n200' '{}'", ANSWER.replace('\n', "")));
        let provider = JevProvider::new(&curl, "sk-test-key", CommandLimits::new(&directory))
            .with_model("jev-1.13.0")
            .with_url("https://example.test/v1/systemone");
        let reply = provider.infer(&model_request(DOOM)).unwrap();
        assert_eq!(
            reply,
            "act choice 6 fire 330 320 15 220 10 440 0 foe noul 980 risk score 3 740 600 270 730 0"
        );
        let config = fs::read_to_string(directory.join("config")).unwrap();
        assert_eq!(config, "header = \"Authorization: Bearer sk-test-key\"\n");
        let args = fs::read_to_string(directory.join("args")).unwrap();
        assert!(
            args.starts_with("ARGS:<-q><-sS><-K><-><--max-time>"),
            "{args}"
        );
        assert!(!args.contains("sk-test-key"));
        assert!(
            args.ends_with("<https://example.test/v1/systemone>"),
            "{args}"
        );
        let body: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(directory.join("body")).unwrap()).unwrap();
        assert_eq!(body["model"], "jev-1.13.0");
        assert_eq!(body["questions"]["foe"]["type"], "noul");
        assert!(
            !directory.join("jev-request-7.json").exists()
                && fs::read_dir(&directory).unwrap().all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("jev-request")),
            "the body file is removed after the call"
        );
        let audit = provider.audit_log().records();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[0].intent.principal.world, 11);
        assert_eq!(audit[0].intent.principal.agent, Some(1));
        assert!(audit[0]
            .intent
            .operation
            .starts_with("model/infer/jev/request/7"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn http_failures_and_unexpected_answers_are_errors_not_answers() {
        let (directory, curl) = fake_curl("printf '{\"error\":\"bad key\"}\\n401'");
        let provider = JevProvider::new(&curl, "k", CommandLimits::new(&directory));
        assert_eq!(
            provider.infer(&model_request(DOOM)).unwrap_err(),
            ProviderError::Failed {
                code: Some(401),
                stderr: "{\"error\":\"bad key\"}".to_owned()
            }
        );
        fs::remove_dir_all(directory).unwrap();

        let (directory, curl) = fake_curl("printf 'not json\\n200'");
        let provider = JevProvider::new(&curl, "k", CommandLimits::new(&directory));
        assert!(matches!(
            provider.infer(&model_request(DOOM)).unwrap_err(),
            ProviderError::Unexpected(message) if message.contains("not JSON")
        ));
        fs::remove_dir_all(directory).unwrap();

        let (directory, curl) = fake_curl("printf '{}\\n200'");
        let provider = JevProvider::new(&curl, "k", CommandLimits::new(&directory));
        assert!(matches!(
            provider.infer(&model_request(DOOM)).unwrap_err(),
            ProviderError::Unexpected(message) if message.contains("no answer for act")
        ));
        assert!(matches!(
            provider.infer(&model_request("tell me a story")).unwrap_err(),
            ProviderError::Rejected(message) if message.contains("one (judge ...) form")
        ));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn overloaded_answers_are_retried_a_bounded_number_of_times() {
        let (directory, curl) = fake_curl(&format!(
            "count=\"$(dirname \"$0\")/count\"; n=$(cat \"$count\" 2>/dev/null || echo 0); n=$((n+1)); echo $n > \"$count\"; if [ $n -lt 3 ]; then printf 'slow down\\n529'; else printf '%s\\n200' '{}'; fi",
            ANSWER.replace('\n', "")
        ));
        let provider = JevProvider::new(&curl, "k", CommandLimits::new(&directory));
        assert!(provider.infer(&model_request(DOOM)).is_ok());
        assert_eq!(
            fs::read_to_string(directory.join("count")).unwrap().trim(),
            "3"
        );
        fs::remove_dir_all(directory).unwrap();

        let (directory, curl) = fake_curl("printf 'busy\\n429'");
        let provider = JevProvider::new(&curl, "k", CommandLimits::new(&directory));
        assert_eq!(
            provider.infer(&model_request(DOOM)).unwrap_err(),
            ProviderError::Failed {
                code: Some(429),
                stderr: "busy".to_owned()
            }
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
