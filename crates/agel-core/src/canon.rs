//! A canonical encoding of a world's state: the digests that bind evidence
//! to it are taken over these bytes, and a world file is these bytes.
//!
//! Every value, agent, event, macro, module and model record is written as
//! tagged, length-delimited bytes in a fixed order: the same state gives
//! the same bytes on any build, and no two different states give the same
//! bytes. The encoding is versioned in the digest's prefix and in a world
//! file's header, and changes only with them. Closures' environments are
//! written by value, however they are shared: decoding gives each closure
//! its own copy of what it captured, which nothing can tell apart.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The bytes of one encoding.
pub(crate) struct Encoder {
    bytes: Vec<u8>,
}

/// A type with a canonical encoding, both ways.
pub(crate) trait Canon: Sized {
    fn canon(&self, out: &mut Encoder);
    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError>;

    fn decode(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        if input.depth >= MAX_DECODE_DEPTH {
            return input.fail("canonical nesting exceeds the depth limit");
        }
        input.depth += 1;
        let result = Self::decode_inner(input);
        input.depth -= 1;
        result
    }
}

/// What a decoding refused: the byte offset, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for CanonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "canonical encoding at byte {}: {}",
            self.offset, self.message
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CanonError {}

impl Encoder {
    pub(crate) fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }

    /// A variant or field tag: its name, as text.
    pub(crate) fn tag(&mut self, name: &str) {
        self.bytes.push(b'T');
        self.delimited(name.as_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.push(b'U');
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn i64(&mut self, value: i64) {
        self.bytes.push(b'I');
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn bool(&mut self, value: bool) {
        self.bytes.push(if value { b'1' } else { b'0' });
    }

    pub(crate) fn text(&mut self, value: &str) {
        self.bytes.push(b'S');
        self.delimited(value.as_bytes());
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.bytes.push(b'D');
        self.delimited(value);
    }

    /// The start of a sequence of `count` items, which follow.
    pub(crate) fn seq(&mut self, count: usize) {
        self.bytes.push(b'L');
        self.bytes.extend_from_slice(&(count as u64).to_be_bytes());
    }

    /// Nothing, where a value may be absent.
    pub(crate) fn none(&mut self) {
        self.bytes.push(b'N');
    }

    pub(crate) fn option<T: Canon>(&mut self, value: Option<&T>) {
        match value {
            Some(value) => value.canon(self),
            None => self.none(),
        }
    }

    pub(crate) fn items<'a, T: Canon + 'a>(&mut self, items: impl ExactSizeIterator<Item = &'a T>) {
        self.seq(items.len());
        for item in items {
            item.canon(self);
        }
    }

    /// A map's entries in its own (sorted) order: key text, then value.
    pub(crate) fn entries<'a, T: Canon + 'a>(
        &mut self,
        entries: impl ExactSizeIterator<Item = (&'a String, &'a T)>,
    ) {
        self.seq(entries.len());
        for (key, value) in entries {
            self.text(key);
            value.canon(self);
        }
    }

    fn delimited(&mut self, bytes: &[u8]) {
        self.bytes
            .extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        self.bytes.extend_from_slice(bytes);
    }
}

/// A reader over an encoding, refusing anything the encoder never writes.
pub(crate) struct Decoder<'a> {
    bytes: &'a [u8],
    at: usize,
    depth: usize,
}

/// Items a sequence may name before its bytes are read: a bound on what
/// a claimed count allocates ahead of the data.
const RESERVE_AT_MOST: usize = 4096;
/// Bound recursive values and closure environments independently of file size.
pub(crate) const MAX_DECODE_DEPTH: usize = 256;

impl<'a> Decoder<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            depth: 0,
        }
    }

    pub(crate) fn finished(&self) -> bool {
        self.at == self.bytes.len()
    }

    pub(crate) fn fail<T>(&self, message: impl Into<String>) -> Result<T, CanonError> {
        Err(CanonError {
            offset: self.at,
            message: message.into(),
        })
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], CanonError> {
        match self.bytes.get(self.at..self.at.saturating_add(count)) {
            Some(bytes) if bytes.len() == count => {
                self.at += count;
                Ok(bytes)
            }
            _ => self.fail(format!("{count} bytes wanted past the end")),
        }
    }

    fn byte(&mut self) -> Result<u8, CanonError> {
        Ok(self.take(1)?[0])
    }

    fn marker(&mut self, wanted: u8, what: &str) -> Result<(), CanonError> {
        let found = self.byte()?;
        if found == wanted {
            Ok(())
        } else {
            self.at -= 1;
            self.fail(format!("{what} wanted, found {:?}", found as char))
        }
    }

    fn raw_u64(&mut self) -> Result<u64, CanonError> {
        let bytes = self.take(8)?;
        Ok(u64::from_be_bytes(bytes.try_into().expect("eight bytes")))
    }

    fn delimited(&mut self) -> Result<&'a [u8], CanonError> {
        let length = self.raw_u64()?;
        let Ok(length) = usize::try_from(length) else {
            return self.fail("a length this machine cannot hold");
        };
        self.take(length)
    }

    fn utf8(&mut self) -> Result<&'a str, CanonError> {
        let bytes = self.delimited()?;
        match core::str::from_utf8(bytes) {
            Ok(text) => Ok(text),
            Err(_) => self.fail("text that is not UTF-8"),
        }
    }

    /// The tag that comes next, consumed.
    pub(crate) fn tag(&mut self) -> Result<&'a str, CanonError> {
        self.marker(b'T', "a tag")?;
        self.utf8()
    }

    /// The tag that comes next, not consumed.
    pub(crate) fn peek_tag(&mut self) -> Result<&'a str, CanonError> {
        let at = self.at;
        let tag = self.tag();
        self.at = at;
        tag
    }

    pub(crate) fn expect(&mut self, name: &str) -> Result<(), CanonError> {
        let at = self.at;
        let found = self.tag()?;
        if found == name {
            Ok(())
        } else {
            self.at = at;
            self.fail(format!("tag {name} wanted, found {found}"))
        }
    }

    pub(crate) fn u64(&mut self) -> Result<u64, CanonError> {
        self.marker(b'U', "an unsigned integer")?;
        self.raw_u64()
    }

    pub(crate) fn i64(&mut self) -> Result<i64, CanonError> {
        self.marker(b'I', "an integer")?;
        Ok(self.raw_u64()? as i64)
    }

    pub(crate) fn bool(&mut self) -> Result<bool, CanonError> {
        match self.byte()? {
            b'1' => Ok(true),
            b'0' => Ok(false),
            _ => {
                self.at -= 1;
                self.fail("a boolean wanted")
            }
        }
    }

    pub(crate) fn text(&mut self) -> Result<String, CanonError> {
        self.marker(b'S', "text")?;
        Ok(self.utf8()?.into())
    }

    pub(crate) fn bytes(&mut self) -> Result<Vec<u8>, CanonError> {
        self.marker(b'D', "bytes")?;
        Ok(self.delimited()?.to_vec())
    }

    /// The count a sequence claims; its items follow.
    pub(crate) fn seq(&mut self) -> Result<usize, CanonError> {
        self.marker(b'L', "a sequence")?;
        let count = self.raw_u64()?;
        match usize::try_from(count) {
            Ok(count) if count <= self.bytes.len() - self.at => Ok(count),
            _ => self.fail("a sequence longer than the bytes that follow"),
        }
    }

    /// Whether nothing comes next, consumed when it does.
    pub(crate) fn none(&mut self) -> bool {
        if self.bytes.get(self.at) == Some(&b'N') {
            self.at += 1;
            true
        } else {
            false
        }
    }

    pub(crate) fn option<T: Canon>(&mut self) -> Result<Option<T>, CanonError> {
        if self.none() {
            Ok(None)
        } else {
            T::decode(self).map(Some)
        }
    }

    pub(crate) fn items<T: Canon>(&mut self) -> Result<Vec<T>, CanonError> {
        let count = self.seq()?;
        let mut items = Vec::with_capacity(count.min(RESERVE_AT_MOST));
        for _ in 0..count {
            items.push(T::decode(self)?);
        }
        Ok(items)
    }

    pub(crate) fn entries<T: Canon>(&mut self) -> Result<BTreeMap<String, T>, CanonError> {
        self.keyed_entries()
    }

    pub(crate) fn keyed_entries<K: Canon + Ord, V: Canon>(
        &mut self,
    ) -> Result<BTreeMap<K, V>, CanonError> {
        let count = self.seq()?;
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let key = K::decode(self)?;
            if entries
                .last_key_value()
                .is_some_and(|(previous, _)| previous >= &key)
            {
                return self.fail("map keys must be unique and in canonical order");
            }
            let value = V::decode(self)?;
            entries.insert(key, value);
        }
        Ok(entries)
    }
}

impl Canon for String {
    fn canon(&self, out: &mut Encoder) {
        out.text(self);
    }

    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.text()
    }
}

impl Canon for u64 {
    fn canon(&self, out: &mut Encoder) {
        out.u64(*self);
    }

    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.u64()
    }
}

impl<T: Canon> Canon for Vec<T> {
    fn canon(&self, out: &mut Encoder) {
        out.items(self.iter());
    }

    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.items()
    }
}

/// Bytes of a known length, or a refusal naming what they were for.
pub(crate) fn fixed<const N: usize>(
    input: &mut Decoder<'_>,
    what: &str,
) -> Result<[u8; N], CanonError> {
    let bytes = input.bytes()?;
    match <[u8; N]>::try_from(bytes) {
        Ok(bytes) => Ok(bytes),
        Err(_) => input.fail(format!("{what} of {N} bytes wanted")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deeply_nested_values_fail_without_exhausting_the_stack() {
        let mut out = Encoder::new();
        for _ in 0..1000 {
            out.tag("list");
            out.seq(1);
        }
        out.tag("nil");
        let bytes = out.finish();
        let error = crate::Value::decode(&mut Decoder::new(&bytes)).unwrap_err();
        assert!(error.message.contains("depth limit"), "{error}");
    }

    #[test]
    fn maps_reject_duplicate_and_out_of_order_keys() {
        for keys in [["a", "a"], ["b", "a"]] {
            let mut out = Encoder::new();
            out.seq(2);
            for key in keys {
                out.text(key);
                out.u64(1);
            }
            let bytes = out.finish();
            let error = Decoder::new(&bytes).entries::<u64>().unwrap_err();
            assert!(error.message.contains("canonical order"), "{error}");
        }
    }

    #[test]
    fn primitives_round_trip_and_refuse_what_was_never_written() {
        let mut out = Encoder::new();
        out.tag("t");
        out.u64(7);
        out.i64(-7);
        out.bool(true);
        out.text("x");
        out.bytes(&[1, 2]);
        out.none();
        let bytes = out.finish();
        let mut input = Decoder::new(&bytes);
        assert_eq!(input.tag().unwrap(), "t");
        assert_eq!(input.u64().unwrap(), 7);
        assert_eq!(input.i64().unwrap(), -7);
        assert!(input.bool().unwrap());
        assert_eq!(input.text().unwrap(), "x");
        assert_eq!(input.bytes().unwrap(), vec![1, 2]);
        assert!(input.none());
        assert!(input.finished());
        let mut input = Decoder::new(&bytes);
        let error = input.u64().unwrap_err();
        assert_eq!(error.offset, 0);
        assert!(error.message.contains("unsigned integer wanted"), "{error}");
        // A sequence may not claim more items than bytes remain.
        let mut out = Encoder::new();
        out.seq(1_000_000);
        let bytes = out.finish();
        assert!(Decoder::new(&bytes).seq().is_err());
    }
}
