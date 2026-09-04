//! Canonical, tamper-evident, crash-safe Agel replay images.
//!
//! Images contain committed inputs rather than Rust heap layouts. Loading an
//! image replays those inputs into a fresh world, so authority is reissued for
//! the new world and the format remains independent of host object layout.

use agel_core::{
    AuthorityError, Budget, Capability, Commit, EvaluationOptions, ModelCompletion,
    ModelCompletionError, ModelDispatchError, ModelOutcome, ModelRequest, TransactionError, World,
};
use agel_integrity::{sha256, Digest};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"AGELIMG\0";
const FORMAT_VERSION: u16 = 1;
const DOMAIN: &[u8] = b"agel/image-chain/v1\0";
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 1_000_000;
const MAX_FIELD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageEntry {
    Grant {
        kind: String,
        scope: String,
    },
    Evaluate(String),
    ClaimModel(u64),
    CompleteModel {
        request_id: u64,
        outcome: ModelOutcome,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChainedEntry {
    entry: ImageEntry,
    digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    budget: Budget,
    history_limit: usize,
    entries: Vec<ChainedEntry>,
    root: Digest,
}

impl Image {
    pub fn new(budget: Budget, history_limit: usize) -> Self {
        let root = initial_digest(&budget, history_limit);
        Self {
            budget,
            history_limit,
            entries: Vec::new(),
            root,
        }
    }

    pub fn digest(&self) -> Digest {
        self.root
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = &ImageEntry> {
        self.entries.iter().map(|entry| &entry.entry)
    }

    pub fn budget(&self) -> &Budget {
        &self.budget
    }

    pub fn extends(&self, ancestor: &Self) -> bool {
        self.budget == ancestor.budget
            && self.history_limit == ancestor.history_limit
            && self.entries.len() >= ancestor.entries.len()
            && self.entries[..ancestor.entries.len()] == ancestor.entries
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(MAGIC);
        put_u16(&mut output, FORMAT_VERSION);
        put_u64(&mut output, self.history_limit as u64);
        encode_budget(&mut output, &self.budget);
        put_u64(&mut output, self.entries.len() as u64);
        for chained in &self.entries {
            let encoded = encode_entry(&chained.entry);
            put_bytes(&mut output, &encoded);
            output.extend_from_slice(chained.digest.as_bytes());
        }
        output.extend_from_slice(self.root.as_bytes());
        output
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(ImageError::Limit("image exceeds 64 MiB".into()));
        }
        let mut reader = Reader::new(bytes);
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(ImageError::Invalid("invalid image magic".into()));
        }
        let version = reader.u16()?;
        if version != FORMAT_VERSION {
            return Err(ImageError::UnsupportedVersion(version));
        }
        let history_limit = reader.usize("history limit")?;
        let budget = decode_budget(&mut reader)?;
        let count = reader.usize("entry count")?;
        if count > MAX_ENTRIES {
            return Err(ImageError::Limit("too many image entries".into()));
        }
        let mut entries = Vec::with_capacity(count);
        let mut root = initial_digest(&budget, history_limit);
        for _ in 0..count {
            let encoded = reader.bytes()?;
            let claimed = reader.digest()?;
            let entry = decode_entry(encoded)?;
            let calculated = chained_digest(root, encoded);
            if claimed != calculated {
                return Err(ImageError::Integrity("entry hash chain mismatch".into()));
            }
            root = calculated;
            entries.push(ChainedEntry {
                entry,
                digest: calculated,
            });
        }
        let claimed_root = reader.digest()?;
        if !reader.is_empty() {
            return Err(ImageError::Invalid("trailing image data".into()));
        }
        if root != claimed_root {
            return Err(ImageError::Integrity("image root mismatch".into()));
        }
        Ok(Self {
            budget,
            history_limit,
            entries,
            root,
        })
    }

    pub fn rebuild(&self) -> Result<ImageSession, ImageError> {
        let mut session = ImageSession {
            world: World::new(self.history_limit),
            options: EvaluationOptions {
                budget: self.budget.clone(),
                capabilities: Vec::new(),
            },
            image: Image::new(self.budget.clone(), self.history_limit),
        };
        for chained in &self.entries {
            session.apply(&chained.entry)?;
            session.image.append(chained.entry.clone());
        }
        if session.image.root != self.root {
            return Err(ImageError::Integrity(
                "rebuilt image commitment differs".into(),
            ));
        }
        Ok(session)
    }

    fn append(&mut self, entry: ImageEntry) {
        let encoded = encode_entry(&entry);
        self.root = chained_digest(self.root, &encoded);
        self.entries.push(ChainedEntry {
            entry,
            digest: self.root,
        });
    }
}

#[derive(Debug)]
pub struct ImageSession {
    world: World,
    options: EvaluationOptions,
    image: Image,
}

impl ImageSession {
    pub fn new(history_limit: usize, budget: Budget) -> Self {
        Self {
            world: World::new(history_limit),
            options: EvaluationOptions {
                budget: budget.clone(),
                capabilities: Vec::new(),
            },
            image: Image::new(budget, history_limit),
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn options(&self) -> &EvaluationOptions {
        &self.options
    }

    pub fn image(&self) -> &Image {
        &self.image
    }

    pub fn grant(
        &mut self,
        kind: impl Into<String>,
        scope: impl Into<String>,
    ) -> Result<Capability, ImageError> {
        let kind = kind.into();
        let scope = scope.into();
        let capability = self
            .world
            .issue_capability(kind.clone(), scope.clone())
            .map_err(ImageError::Authority)?;
        self.options.capabilities.push(capability.clone());
        self.image.append(ImageEntry::Grant { kind, scope });
        Ok(capability)
    }

    pub fn evaluate(&mut self, source: &str) -> Result<Commit, ImageError> {
        let commit = self
            .world
            .evaluate_with(source, &self.options)
            .map_err(ImageError::Transaction)?;
        if !commit.values.is_empty() {
            self.image.append(ImageEntry::Evaluate(source.into()));
        }
        Ok(commit)
    }

    pub fn claim_model_request(
        &mut self,
        request_id: u64,
    ) -> Result<(Commit, ModelRequest), ImageError> {
        let result = self
            .world
            .claim_model_request(request_id, &self.options)
            .map_err(ImageError::Dispatch)?;
        self.image.append(ImageEntry::ClaimModel(request_id));
        Ok(result)
    }

    pub fn complete_model_request(
        &mut self,
        completion: ModelCompletion,
    ) -> Result<Commit, ImageError> {
        let entry = ImageEntry::CompleteModel {
            request_id: completion.request_id,
            outcome: completion.outcome.clone(),
        };
        let commit = self
            .world
            .complete_model_request(completion, &self.options)
            .map_err(ImageError::Completion)?;
        self.image.append(entry);
        Ok(commit)
    }

    fn apply(&mut self, entry: &ImageEntry) -> Result<(), ImageError> {
        match entry {
            ImageEntry::Grant { kind, scope } => {
                let capability = self
                    .world
                    .issue_capability(kind.clone(), scope.clone())
                    .map_err(ImageError::Authority)?;
                self.options.capabilities.push(capability);
            }
            ImageEntry::Evaluate(source) => {
                self.world
                    .evaluate_with(source, &self.options)
                    .map_err(ImageError::Transaction)?;
            }
            ImageEntry::ClaimModel(request_id) => {
                self.world
                    .claim_model_request(*request_id, &self.options)
                    .map_err(ImageError::Dispatch)?;
            }
            ImageEntry::CompleteModel {
                request_id,
                outcome,
            } => {
                let request = self
                    .world
                    .dispatching_model_requests()
                    .into_iter()
                    .find(|request| request.id == *request_id)
                    .ok_or_else(|| {
                        ImageError::Invalid(format!(
                            "completion has no dispatching request {request_id}"
                        ))
                    })?;
                self.world
                    .complete_model_request(
                        ModelCompletion {
                            request_id: *request_id,
                            effect_key: request.effect_key,
                            outcome: outcome.clone(),
                        },
                        &self.options,
                    )
                    .map_err(ImageError::Completion)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ImageStore {
    path: PathBuf,
}

impl ImageStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<Image>, ImageError> {
        match read_image(&self.path) {
            Ok(image) => Ok(Some(image)),
            Err(ImageError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                match read_image(&sidecar(&self.path, "previous")) {
                    Ok(image) => Ok(Some(image)),
                    Err(ImageError::Io(previous))
                        if previous.kind() == std::io::ErrorKind::NotFound =>
                    {
                        Ok(None)
                    }
                    Err(error) => Err(error),
                }
            }
            Err(primary_error) => match read_image(&sidecar(&self.path, "previous")) {
                Ok(image) => Ok(Some(image)),
                Err(_) => Err(primary_error),
            },
        }
    }

    pub fn save(&self, image: &Image, expected: Option<Digest>) -> Result<Digest, ImageError> {
        let actual = self.load()?.map(|current| current.digest());
        if actual != expected {
            return Err(ImageError::Conflict { expected, actual });
        }
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(ImageError::Io)?;
        let temporary = sidecar(&self.path, "new");
        let previous = sidecar(&self.path, "previous");
        let encoded = image.encode();
        {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)
                .map_err(ImageError::Io)?;
            file.write_all(&encoded).map_err(ImageError::Io)?;
            file.sync_all().map_err(ImageError::Io)?;
        }
        if self.path.exists() {
            if previous.exists() {
                fs::remove_file(&previous).map_err(ImageError::Io)?;
            }
            fs::rename(&self.path, &previous).map_err(ImageError::Io)?;
        }
        fs::rename(&temporary, &self.path).map_err(ImageError::Io)?;
        sync_directory(parent)?;
        Ok(image.digest())
    }
}

fn read_image(path: &Path) -> Result<Image, ImageError> {
    let mut file = File::open(path).map_err(ImageError::Io)?;
    let length = file.metadata().map_err(ImageError::Io)?.len();
    if length > MAX_IMAGE_BYTES as u64 {
        return Err(ImageError::Limit("image exceeds 64 MiB".into()));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes).map_err(ImageError::Io)?;
    Image::decode(&bytes)
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(format!(".{suffix}"));
    value.into()
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ImageError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(ImageError::Io)
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ImageError> {
    Ok(())
}

#[derive(Debug)]
pub enum ImageError {
    Invalid(String),
    Integrity(String),
    Limit(String),
    UnsupportedVersion(u16),
    Conflict {
        expected: Option<Digest>,
        actual: Option<Digest>,
    },
    Io(std::io::Error),
    Authority(AuthorityError),
    Transaction(TransactionError),
    Dispatch(ModelDispatchError),
    Completion(ModelCompletionError),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid image: {message}"),
            Self::Integrity(message) => write!(f, "image integrity failure: {message}"),
            Self::Limit(message) => write!(f, "image limit exceeded: {message}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported image format version {version}")
            }
            Self::Conflict { expected, actual } => {
                write!(
                    f,
                    "image changed concurrently (expected {expected:?}, found {actual:?})"
                )
            }
            Self::Io(error) => error.fmt(f),
            Self::Authority(error) => error.fmt(f),
            Self::Transaction(error) => error.fmt(f),
            Self::Dispatch(error) => error.fmt(f),
            Self::Completion(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ImageError {}

fn chained_digest(previous: Digest, entry: &[u8]) -> Digest {
    let mut bytes = DOMAIN.to_vec();
    bytes.extend_from_slice(previous.as_bytes());
    put_bytes(&mut bytes, entry);
    sha256(&bytes)
}

fn initial_digest(budget: &Budget, history_limit: usize) -> Digest {
    let mut bytes = DOMAIN.to_vec();
    put_u16(&mut bytes, FORMAT_VERSION);
    put_u64(&mut bytes, history_limit as u64);
    encode_budget(&mut bytes, budget);
    sha256(&bytes)
}

fn encode_entry(entry: &ImageEntry) -> Vec<u8> {
    let mut output = Vec::new();
    match entry {
        ImageEntry::Grant { kind, scope } => {
            output.push(0);
            put_string(&mut output, kind);
            put_string(&mut output, scope);
        }
        ImageEntry::Evaluate(source) => {
            output.push(1);
            put_string(&mut output, source);
        }
        ImageEntry::ClaimModel(id) => {
            output.push(2);
            put_u64(&mut output, *id);
        }
        ImageEntry::CompleteModel {
            request_id,
            outcome,
        } => {
            output.push(3);
            put_u64(&mut output, *request_id);
            match outcome {
                ModelOutcome::Success(text) => {
                    output.push(0);
                    put_string(&mut output, text);
                }
                ModelOutcome::Failure { kind, message } => {
                    output.push(1);
                    put_string(&mut output, kind);
                    put_string(&mut output, message);
                }
            }
        }
    }
    output
}

fn decode_entry(bytes: &[u8]) -> Result<ImageEntry, ImageError> {
    let mut reader = Reader::new(bytes);
    let tag = reader.byte()?;
    let entry = match tag {
        0 => ImageEntry::Grant {
            kind: reader.string()?,
            scope: reader.string()?,
        },
        1 => ImageEntry::Evaluate(reader.string()?),
        2 => ImageEntry::ClaimModel(reader.u64()?),
        3 => {
            let request_id = reader.u64()?;
            let outcome = match reader.byte()? {
                0 => ModelOutcome::Success(reader.string()?),
                1 => ModelOutcome::Failure {
                    kind: reader.string()?,
                    message: reader.string()?,
                },
                tag => return Err(ImageError::Invalid(format!("unknown outcome tag {tag}"))),
            };
            ImageEntry::CompleteModel {
                request_id,
                outcome,
            }
        }
        tag => return Err(ImageError::Invalid(format!("unknown entry tag {tag}"))),
    };
    if !reader.is_empty() {
        return Err(ImageError::Invalid("trailing entry data".into()));
    }
    Ok(entry)
}

fn encode_budget(output: &mut Vec<u8>, budget: &Budget) {
    put_u64(output, budget.fuel);
    for value in [
        budget.max_call_depth,
        budget.max_collection_len,
        budget.max_source_bytes,
        budget.max_parse_depth,
        budget.max_model_prompt_bytes,
        budget.max_pending_model_requests,
    ] {
        put_u64(output, value as u64);
    }
}

fn decode_budget(reader: &mut Reader<'_>) -> Result<Budget, ImageError> {
    Ok(Budget {
        fuel: reader.u64()?,
        max_call_depth: reader.usize("call depth")?,
        max_collection_len: reader.usize("collection length")?,
        max_source_bytes: reader.usize("source bytes")?,
        max_parse_depth: reader.usize("parse depth")?,
        max_model_prompt_bytes: reader.usize("model prompt bytes")?,
        max_pending_model_requests: reader.usize("pending model requests")?,
    })
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
    put_u64(output, bytes.len() as u64);
    output.extend_from_slice(bytes);
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    put_bytes(output, value.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ImageError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| ImageError::Invalid("image offset overflow".into()))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| ImageError::Invalid("truncated image".into()))?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, ImageError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ImageError> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("two bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64, ImageError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }

    fn usize(&mut self, field: &str) -> Result<usize, ImageError> {
        usize::try_from(self.u64()?)
            .map_err(|_| ImageError::Limit(format!("{field} does not fit this host")))
    }

    fn bytes(&mut self) -> Result<&'a [u8], ImageError> {
        let count = self.usize("field length")?;
        if count > MAX_FIELD_BYTES {
            return Err(ImageError::Limit("field exceeds 16 MiB".into()));
        }
        self.take(count)
    }

    fn string(&mut self) -> Result<String, ImageError> {
        String::from_utf8(self.bytes()?.to_vec())
            .map_err(|_| ImageError::Invalid("string is not UTF-8".into()))
    }

    fn digest(&mut self) -> Result<Digest, ImageError> {
        Ok(Digest::from_bytes(
            self.take(32)?.try_into().expect("32 bytes"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    fn temp_image() -> PathBuf {
        std::env::temp_dir().join(format!(
            "agel-image-test-{}-{}.agel",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn canonical_image_round_trips_and_rebuilds_with_fresh_authority() {
        let mut session = ImageSession::new(8, Budget::default());
        let old = session.grant("model/infer", "claude").unwrap();
        session
            .evaluate("(def cap (request-capability 'model/infer \"claude\")) (def answer 42)")
            .unwrap();
        let encoded = session.image().encode();
        assert_eq!(encoded, session.image().encode());
        let decoded = Image::decode(&encoded).unwrap();
        let rebuilt = decoded.rebuild().unwrap();
        assert_eq!(
            rebuilt.world().binding("answer"),
            session.world().binding("answer")
        );
        let new = rebuilt.world().binding("cap").unwrap();
        assert!(
            matches!(new, agel_core::Value::Capability(cap) if cap.issuer_world() != old.issuer_world())
        );
        assert_eq!(rebuilt.image().digest(), session.image().digest());
    }

    #[test]
    fn model_effects_replay_exact_output_without_old_effect_keys() {
        let mut session = ImageSession::new(8, Budget::default());
        session.grant("model/infer", "claude").unwrap();
        session
            .evaluate(
                "(def cap (request-capability 'model/infer \"claude\"))
                 (defprotocol asks (ask))
                 (def worker
                   (spawn \"worker\"
                     (fn (self heap message)
                       (begin (model-request 'claude \"hello\" self) heap))
                     (dict) asks nil 'stop 0 (list cap)))
                 (send worker '(ask))
                 (run)",
            )
            .unwrap();
        let (_, request) = session.claim_model_request(1).unwrap();
        session
            .complete_model_request(ModelCompletion::success(&request, "recorded answer"))
            .unwrap();
        let rebuilt = Image::decode(&session.image().encode())
            .unwrap()
            .rebuild()
            .unwrap();
        assert!(rebuilt.world().pending_model_requests().is_empty());
        assert!(rebuilt.world().dispatching_model_requests().is_empty());
        assert_eq!(rebuilt.world().effect_journal().entries().len(), 1);
    }

    #[test]
    fn tampering_is_detected() {
        let mut session = ImageSession::new(8, Budget::default());
        session.evaluate("(def answer 42)").unwrap();
        let mut bytes = session.image().encode();
        let position = bytes
            .windows(b"answer".len())
            .position(|window| window == b"answer")
            .unwrap();
        bytes[position] = b'X';
        assert!(matches!(
            Image::decode(&bytes),
            Err(ImageError::Integrity(_))
        ));
    }

    #[test]
    fn store_is_atomic_optimistic_and_recovers_previous_image() {
        let path = temp_image();
        let store = ImageStore::new(&path);
        let mut first = ImageSession::new(8, Budget::default());
        first.evaluate("(def generation 1)").unwrap();
        let first_digest = store.save(first.image(), None).unwrap();
        let mut second = first.image().rebuild().unwrap();
        second.evaluate("(def generation 2)").unwrap();
        store.save(second.image(), Some(first_digest)).unwrap();
        assert!(matches!(
            store.save(first.image(), Some(first_digest)),
            Err(ImageError::Conflict { .. })
        ));
        fs::write(&path, b"torn").unwrap();
        let recovered = store.load().unwrap().unwrap().rebuild().unwrap();
        assert_eq!(
            recovered.world().binding("generation"),
            Some(&agel_core::Value::Int(1))
        );
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(sidecar(&path, "previous"));
        let _ = fs::remove_file(sidecar(&path, "new"));
    }
}
