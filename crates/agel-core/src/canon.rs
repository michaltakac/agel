//! A canonical encoding of a world's state, for the digests that bind
//! evidence to it.
//!
//! Every value, agent, event, macro, module and model record is written as
//! tagged, length-delimited bytes in a fixed order: the same state gives
//! the same bytes on any build, and no two different states give the same
//! bytes. The encoding is versioned in the digest's prefix and changes only
//! with it. It is not a serialization: nothing decodes it, and closures'
//! environments are written by value, however they are shared.

use alloc::string::String;
use alloc::vec::Vec;

/// The bytes of one encoding.
pub(crate) struct Encoder {
    bytes: Vec<u8>,
}

/// A type with a canonical encoding.
pub(crate) trait Canon {
    fn canon(&self, out: &mut Encoder);
}

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

impl Canon for String {
    fn canon(&self, out: &mut Encoder) {
        out.text(self);
    }
}

impl Canon for u64 {
    fn canon(&self, out: &mut Encoder) {
        out.u64(*self);
    }
}

impl<T: Canon> Canon for Vec<T> {
    fn canon(&self, out: &mut Encoder) {
        out.items(self.iter());
    }
}
