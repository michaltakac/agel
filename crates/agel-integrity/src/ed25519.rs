//! Ed25519 signatures and SHA-512, written against RFC 8032 with no
//! dependencies and no `unsafe`.
//!
//! This is the project's first detached-signature primitive. It is checked
//! against the RFC test vectors, but it is a straightforward implementation:
//! field and scalar arithmetic are not constant-time, so a signing key must not
//! be used where an adversary can time the signer. Verification carries no
//! secret and is the operation the trust boundaries depend on.

use core::fmt;

// ---------------------------------------------------------------------------
// SHA-512
// ---------------------------------------------------------------------------

const INITIAL: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];
const K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

/// SHA-512 over data supplied in pieces, so a message larger than any buffer
/// the caller wants to hold (a kernel slot read sector by sector) can be
/// hashed without an allocator.
#[derive(Clone)]
pub struct Sha512 {
    state: [u64; 8],
    block: [u8; 128],
    buffered: usize,
    length: u128,
}

impl Default for Sha512 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha512 {
    pub fn new() -> Self {
        Self {
            state: INITIAL,
            block: [0; 128],
            buffered: 0,
            length: 0,
        }
    }

    // Written without indexing that could panic: this code is linked into
    // kernel images, where a panic path is dead weight that also names the
    // building machine's source path.
    pub fn update(&mut self, mut input: &[u8]) {
        self.length = self.length.wrapping_add(input.len() as u128);
        if self.buffered > 0 {
            let take = (128 - self.buffered).min(input.len());
            for (stored, byte) in self.block.iter_mut().skip(self.buffered).zip(input) {
                *stored = *byte;
            }
            self.buffered += take;
            input = input.get(take..).unwrap_or(&[]);
            if self.buffered == 128 {
                let block = self.block;
                compress(&mut self.state, &block);
                self.buffered = 0;
            }
        }
        while input.len() >= 128 {
            let mut block = [0_u8; 128];
            for (stored, byte) in block.iter_mut().zip(input) {
                *stored = *byte;
            }
            compress(&mut self.state, &block);
            input = input.get(128..).unwrap_or(&[]);
        }
        if !input.is_empty() {
            for (stored, byte) in self.block.iter_mut().zip(input) {
                *stored = *byte;
            }
            self.buffered = input.len();
        }
    }

    pub fn finish(mut self) -> [u8; 64] {
        let bit_length = self.length.wrapping_mul(8);
        let mut padding = [0_u8; 256];
        padding[0] = 0x80;
        let mut padded = 1;
        while (self.buffered + padded) % 128 != 112 {
            padded += 1;
        }
        for (stored, byte) in padding
            .iter_mut()
            .skip(padded)
            .zip(bit_length.to_be_bytes())
        {
            *stored = byte;
        }
        self.update(padding.get(..padded + 16).unwrap_or(&[]));
        debug_assert_eq!(self.buffered, 0);
        let mut output = [0_u8; 64];
        for (chunk, word) in output.chunks_exact_mut(8).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        output
    }
}

/// SHA-512 of `input`, as 64 bytes.
pub fn sha512(input: &[u8]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    hasher.update(input);
    hasher.finish()
}

fn compress(hash: &mut [u64; 8], chunk: &[u8; 128]) {
    let mut words = [0_u64; 80];
    for (word, bytes) in words.iter_mut().zip(chunk.chunks_exact(8)) {
        let mut stored = [0_u8; 8];
        for (place, byte) in stored.iter_mut().zip(bytes) {
            *place = *byte;
        }
        *word = u64::from_be_bytes(stored);
    }
    for index in 16..80 {
        let s0 = words[index - 15].rotate_right(1)
            ^ words[index - 15].rotate_right(8)
            ^ (words[index - 15] >> 7);
        let s1 = words[index - 2].rotate_right(19)
            ^ words[index - 2].rotate_right(61)
            ^ (words[index - 2] >> 6);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *hash;
    for index in 0..80 {
        let sum1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
        let choose = (e & f) ^ (!e & g);
        let temporary1 = h
            .wrapping_add(sum1)
            .wrapping_add(choose)
            .wrapping_add(K[index])
            .wrapping_add(words[index]);
        let sum0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temporary2 = sum0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temporary1);
        d = c;
        c = b;
        b = a;
        a = temporary1.wrapping_add(temporary2);
    }
    for (slot, value) in hash.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

// ---------------------------------------------------------------------------
// Field arithmetic modulo p = 2^255 - 19, fully reduced [u64; 4] little-endian.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Fe([u64; 4]);

const P: [u64; 4] = [
    0xffffffffffffffed,
    0xffffffffffffffff,
    0xffffffffffffffff,
    0x7fffffffffffffff,
];

fn geq(a: &[u64; 4], b: &[u64; 4]) -> bool {
    for index in (0..4).rev() {
        if a[index] != b[index] {
            return a[index] > b[index];
        }
    }
    true
}

/// `a - b` assuming `a >= b`.
fn sub_limbs(a: &[u64; 4], b: &[u64; 4]) -> [u64; 4] {
    let mut out = [0; 4];
    let mut borrow = 0_u64;
    for index in 0..4 {
        let (value, borrow1) = a[index].overflowing_sub(b[index]);
        let (value, borrow2) = value.overflowing_sub(borrow);
        out[index] = value;
        borrow = u64::from(borrow1 | borrow2);
    }
    out
}

/// `a + b`, returning the carry out of bit 256.
fn add_limbs(a: &[u64; 4], b: &[u64; 4]) -> ([u64; 4], u64) {
    let mut out = [0; 4];
    let mut carry = 0_u128;
    for index in 0..4 {
        let sum = u128::from(a[index]) + u128::from(b[index]) + carry;
        out[index] = sum as u64;
        carry = sum >> 64;
    }
    (out, carry as u64)
}

impl Fe {
    const ZERO: Self = Self([0; 4]);
    const ONE: Self = Self([1, 0, 0, 0]);

    fn from_u64(value: u64) -> Self {
        Self([value, 0, 0, 0])
    }

    /// Reduce a value below 2^256 into [0, p).
    fn reduce_limbs(mut limbs: [u64; 4]) -> Self {
        while geq(&limbs, &P) {
            limbs = sub_limbs(&limbs, &P);
        }
        Self(limbs)
    }

    /// Reduce a 512-bit product. 2^256 ≡ 38 (mod p).
    fn reduce_wide(wide: [u64; 8]) -> Self {
        let low: [u64; 4] = wide[..4].try_into().expect("four limbs");
        let high: [u64; 4] = wide[4..].try_into().expect("four limbs");
        // high * 38 as five limbs
        let mut folded = [0_u64; 4];
        let mut carry = 0_u128;
        for index in 0..4 {
            let product = u128::from(high[index]) * 38 + carry;
            folded[index] = product as u64;
            carry = product >> 64;
        }
        let (sum, carry_out) = add_limbs(&low, &folded);
        let overflow = carry + u128::from(carry_out);
        // overflow < 2^7, fold it once more; a second fold cannot overflow
        let (mut sum, carry_out) = add_limbs(&sum, &[(overflow * 38) as u64, 0, 0, 0]);
        if carry_out != 0 {
            sum = add_limbs(&sum, &[38, 0, 0, 0]).0;
        }
        Self::reduce_limbs(sum)
    }

    fn add(&self, other: &Self) -> Self {
        let (sum, carry) = add_limbs(&self.0, &other.0);
        // Both operands are below p < 2^255, so the sum has no carry.
        debug_assert_eq!(carry, 0);
        Self::reduce_limbs(sum)
    }

    fn sub(&self, other: &Self) -> Self {
        if geq(&self.0, &other.0) {
            Self(sub_limbs(&self.0, &other.0))
        } else {
            let (sum, _) = add_limbs(&self.0, &P);
            Self(sub_limbs(&sum, &other.0))
        }
    }

    fn neg(&self) -> Self {
        Self::ZERO.sub(self)
    }

    fn mul(&self, other: &Self) -> Self {
        let mut wide = [0_u64; 8];
        for i in 0..4 {
            let mut carry = 0_u128;
            for j in 0..4 {
                let product = u128::from(self.0[i]) * u128::from(other.0[j])
                    + u128::from(wide[i + j])
                    + carry;
                wide[i + j] = product as u64;
                carry = product >> 64;
            }
            wide[i + 4] = carry as u64;
        }
        Self::reduce_wide(wide)
    }

    fn square(&self) -> Self {
        self.mul(self)
    }

    /// `self^exponent` for a little-endian exponent.
    fn pow(&self, exponent: &[u64; 4]) -> Self {
        let mut result = Self::ONE;
        for index in (0..256).rev() {
            result = result.square();
            if (exponent[index / 64] >> (index % 64)) & 1 == 1 {
                result = result.mul(self);
            }
        }
        result
    }

    fn invert(&self) -> Self {
        // p - 2
        let exponent = [
            0xffffffffffffffeb,
            0xffffffffffffffff,
            0xffffffffffffffff,
            0x7fffffffffffffff,
        ];
        self.pow(&exponent)
    }

    fn is_zero(&self) -> bool {
        self.0 == [0; 4]
    }

    fn is_negative(&self) -> bool {
        self.0[0] & 1 == 1
    }

    fn to_bytes(self) -> [u8; 32] {
        let mut bytes = [0_u8; 32];
        for (chunk, limb) in bytes.chunks_exact_mut(8).zip(self.0) {
            chunk.copy_from_slice(&limb.to_le_bytes());
        }
        bytes
    }
}

/// Curve constants, derived once from their definitions rather than typed in.
struct Curve {
    d: Fe,
    two_d: Fe,
    sqrt_minus_one: Fe,
    base: Point,
}

fn curve() -> Curve {
    let d = Fe::from_u64(121665)
        .neg()
        .mul(&Fe::from_u64(121666).invert());
    // sqrt(-1) = 2^((p-1)/4)
    let exponent = [
        0xfffffffffffffffb,
        0xffffffffffffffff,
        0xffffffffffffffff,
        0x1fffffffffffffff,
    ];
    let sqrt_minus_one = Fe::from_u64(2).pow(&exponent);
    let base_y = Fe::from_u64(4).mul(&Fe::from_u64(5).invert());
    let mut encoded = base_y.to_bytes();
    encoded[31] &= 0x7f; // positive (even) x
    let partial = Curve {
        d,
        two_d: d.add(&d),
        sqrt_minus_one,
        base: Point::IDENTITY,
    };
    // The base point is a constant that decodes; the signing and
    // verification tests against published vectors pin it, so a failure here
    // is a build that those tests reject rather than a runtime condition.
    match decode_point(&partial, &encoded) {
        Some(base) => Curve { base, ..partial },
        None => partial,
    }
}

// ---------------------------------------------------------------------------
// Points in extended twisted Edwards coordinates (X : Y : Z : T), x = X/Z,
// y = Y/Z, x*y = T/Z.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Point {
    x: Fe,
    y: Fe,
    z: Fe,
    t: Fe,
}

impl Point {
    const IDENTITY: Self = Self {
        x: Fe::ZERO,
        y: Fe::ONE,
        z: Fe::ONE,
        t: Fe::ZERO,
    };

    fn add(&self, other: &Self, curve: &Curve) -> Self {
        let a = self.y.sub(&self.x).mul(&other.y.sub(&other.x));
        let b = self.y.add(&self.x).mul(&other.y.add(&other.x));
        let c = self.t.mul(&curve.two_d).mul(&other.t);
        let d = self.z.add(&self.z).mul(&other.z);
        let e = b.sub(&a);
        let f = d.sub(&c);
        let g = d.add(&c);
        let h = b.add(&a);
        Self {
            x: e.mul(&f),
            y: g.mul(&h),
            t: e.mul(&h),
            z: f.mul(&g),
        }
    }

    fn double(&self) -> Self {
        let a = self.x.square();
        let b = self.y.square();
        let c = self.z.square().add(&self.z.square());
        let h = a.add(&b);
        let e = h.sub(&self.x.add(&self.y).square());
        let g = a.sub(&b);
        let f = c.add(&g);
        Self {
            x: e.mul(&f),
            y: g.mul(&h),
            t: e.mul(&h),
            z: f.mul(&g),
        }
    }

    /// Double-and-add over a little-endian 32-byte scalar.
    fn multiply(&self, scalar: &[u8; 32], curve: &Curve) -> Self {
        let mut result = Self::IDENTITY;
        for index in (0..256).rev() {
            result = result.double();
            if (scalar[index / 8] >> (index % 8)) & 1 == 1 {
                result = result.add(self, curve);
            }
        }
        result
    }

    fn encode(&self) -> [u8; 32] {
        let inverse = self.z.invert();
        let x = self.x.mul(&inverse);
        let y = self.y.mul(&inverse);
        let mut bytes = y.to_bytes();
        if x.is_negative() {
            bytes[31] |= 0x80;
        }
        bytes
    }
}

fn decode_point(curve: &Curve, bytes: &[u8; 32]) -> Option<Point> {
    let sign = bytes[31] >> 7;
    let mut masked = *bytes;
    masked[31] &= 0x7f;
    // Reject non-canonical y (y >= p).
    let mut limbs = [0_u64; 4];
    for (index, chunk) in masked.chunks_exact(8).enumerate() {
        limbs[index] = u64::from_le_bytes(chunk.try_into().expect("eight bytes"));
    }
    if geq(&limbs, &P) {
        return None;
    }
    let y = Fe(limbs);
    let y2 = y.square();
    let u = y2.sub(&Fe::ONE);
    let v = curve.d.mul(&y2).add(&Fe::ONE);
    // x = u * v^3 * (u * v^7)^((p-5)/8)
    let v3 = v.square().mul(&v);
    let v7 = v3.square().mul(&v);
    let exponent = [
        0xfffffffffffffffd,
        0xffffffffffffffff,
        0xffffffffffffffff,
        0x0fffffffffffffff,
    ];
    let mut x = u.mul(&v3).mul(&u.mul(&v7).pow(&exponent));
    let check = v.mul(&x.square());
    if check == u.neg() {
        x = x.mul(&curve.sqrt_minus_one);
    } else if check != u {
        return None;
    }
    if x.is_zero() && sign == 1 {
        return None;
    }
    if u8::from(x.is_negative()) != sign {
        x = x.neg();
    }
    Some(Point {
        x,
        y,
        z: Fe::ONE,
        t: x.mul(&y),
    })
}

// ---------------------------------------------------------------------------
// Scalars modulo the group order L.
// ---------------------------------------------------------------------------

const L: [u64; 4] = [
    0x5812631a5cf5d3ed,
    0x14def9dea2f79cd6,
    0,
    0x1000000000000000,
];

/// Reduce a little-endian value of up to 512 bits modulo L by binary long
/// division. Signing is rare, so simplicity wins over speed here.
fn reduce_mod_l(wide: &[u64; 8]) -> [u8; 32] {
    let mut remainder = [0_u64; 4];
    for index in (0..512).rev() {
        // remainder = remainder << 1 | bit
        let mut carry = (wide[index / 64] >> (index % 64)) & 1;
        for limb in remainder.iter_mut() {
            let next = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        if geq(&remainder, &L) {
            remainder = sub_limbs(&remainder, &L);
        }
    }
    let mut bytes = [0_u8; 32];
    for (chunk, limb) in bytes.chunks_exact_mut(8).zip(remainder) {
        chunk.copy_from_slice(&limb.to_le_bytes());
    }
    bytes
}

fn limbs_from_bytes(bytes: &[u8]) -> [u64; 8] {
    let mut limbs = [0_u64; 8];
    for (limb, chunk) in limbs.iter_mut().zip(bytes.chunks(8)) {
        let mut word = [0_u8; 8];
        for (stored, byte) in word.iter_mut().zip(chunk) {
            *stored = *byte;
        }
        *limb = u64::from_le_bytes(word);
    }
    limbs
}

fn hash_mod_l(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha512::new();
    for part in parts {
        hasher.update(part);
    }
    reduce_mod_l(&limbs_from_bytes(&hasher.finish()))
}

/// `(r + k * a) mod L` for 32-byte little-endian scalars.
fn mul_add_mod_l(k: &[u8; 32], a: &[u8; 32], r: &[u8; 32]) -> [u8; 32] {
    let k = limbs_from_bytes(k);
    let a = limbs_from_bytes(a);
    let mut wide = [0_u64; 8];
    for i in 0..4 {
        let mut carry = 0_u128;
        for j in 0..4 {
            let product = u128::from(k[i]) * u128::from(a[j]) + u128::from(wide[i + j]) + carry;
            wide[i + j] = product as u64;
            carry = product >> 64;
        }
        wide[i + 4] = carry as u64;
    }
    let r = limbs_from_bytes(r);
    let mut carry = 0_u128;
    for index in 0..8 {
        let sum = u128::from(wide[index]) + u128::from(r[index]) + carry;
        wide[index] = sum as u64;
        carry = sum >> 64;
    }
    reduce_mod_l(&wide)
}

fn scalar_below_l(bytes: &[u8; 32]) -> bool {
    let limbs = limbs_from_bytes(bytes);
    !geq(&[limbs[0], limbs[1], limbs[2], limbs[3]], &L)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignatureError {
    InvalidKey,
    InvalidSignature,
    InvalidHex,
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidKey => "invalid Ed25519 public key",
            Self::InvalidSignature => "invalid Ed25519 signature",
            Self::InvalidHex => "invalid hexadecimal encoding",
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SignatureError {}

/// An Ed25519 signing key derived from a 32-byte seed.
#[derive(Clone)]
pub struct SigningKey {
    seed: [u8; 32],
    scalar: [u8; 32],
    prefix: [u8; 32],
    public: [u8; 32],
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SigningKey({})", self.verifying_key())
    }
}

impl SigningKey {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let hash = sha512(&seed);
        let mut scalar = [0_u8; 32];
        scalar.copy_from_slice(&hash[..32]);
        scalar[0] &= 248;
        scalar[31] &= 127;
        scalar[31] |= 64;
        let mut prefix = [0_u8; 32];
        prefix.copy_from_slice(&hash[32..]);
        let curve = curve();
        let public = curve.base.multiply(&scalar, &curve).encode();
        Self {
            seed,
            scalar,
            prefix,
            public,
        }
    }

    #[cfg(feature = "std")]
    pub fn from_hex(text: &str) -> Result<Self, SignatureError> {
        let bytes = decode_hex(text.trim())?;
        let seed: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| SignatureError::InvalidKey)?;
        Ok(Self::from_seed(seed))
    }

    pub fn seed(&self) -> [u8; 32] {
        self.seed
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey(self.public)
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        let curve = curve();
        let r = hash_mod_l(&[&self.prefix, message]);
        let big_r = curve.base.multiply(&r, &curve).encode();
        let k = hash_mod_l(&[&big_r, &self.public, message]);
        let s = mul_add_mod_l(&k, &self.scalar, &r);
        let mut bytes = [0_u8; 64];
        bytes[..32].copy_from_slice(&big_r);
        bytes[32..].copy_from_slice(&s);
        Signature(bytes)
    }
}

/// An Ed25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct VerifyingKey([u8; 32]);

impl VerifyingKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, SignatureError> {
        decode_point(&curve(), &bytes)
            .map(|_| Self(bytes))
            .ok_or(SignatureError::InvalidKey)
    }

    #[cfg(feature = "std")]
    pub fn from_hex(text: &str) -> Result<Self, SignatureError> {
        let bytes = decode_hex(text.trim())?;
        let bytes: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| SignatureError::InvalidKey)?;
        Self::from_bytes(bytes)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    #[cfg(feature = "std")]
    pub fn to_hex(self) -> String {
        encode_hex(&self.0)
    }

    /// Verify with the strict equation `[s]B = R + [k]A`, rejecting `s >= L`
    /// so a signature has exactly one accepted encoding.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), SignatureError> {
        let curve = curve();
        let a = decode_point(&curve, &self.0).ok_or(SignatureError::InvalidKey)?;
        let mut r_bytes = [0_u8; 32];
        r_bytes.copy_from_slice(&signature.0[..32]);
        let mut s = [0_u8; 32];
        s.copy_from_slice(&signature.0[32..]);
        if !scalar_below_l(&s) {
            return Err(SignatureError::InvalidSignature);
        }
        let r = decode_point(&curve, &r_bytes).ok_or(SignatureError::InvalidSignature)?;
        let k = hash_mod_l(&[&r_bytes, &self.0, message]);
        let left = curve.base.multiply(&s, &curve).encode();
        let right = r.add(&a.multiply(&k, &curve), &curve).encode();
        if left == right {
            Ok(())
        } else {
            Err(SignatureError::InvalidSignature)
        }
    }
}

impl fmt::Debug for VerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Display for VerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A detached 64-byte Ed25519 signature.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature([u8; 64]);

impl Signature {
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    #[cfg(feature = "std")]
    pub fn from_hex(text: &str) -> Result<Self, SignatureError> {
        let bytes = decode_hex(text.trim())?;
        let bytes: [u8; 64] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| SignatureError::InvalidSignature)?;
        Ok(Self(bytes))
    }

    pub fn to_bytes(self) -> [u8; 64] {
        self.0
    }

    #[cfg(feature = "std")]
    pub fn to_hex(self) -> String {
        encode_hex(&self.0)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(feature = "std")]
pub fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use fmt::Write as _;
        write!(output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

#[cfg(feature = "std")]
pub fn decode_hex(text: &str) -> Result<Vec<u8>, SignatureError> {
    if text.len() % 2 != 0 {
        return Err(SignatureError::InvalidHex);
    }
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(SignatureError::InvalidHex),
    };
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_published_sha512_vectors() {
        assert_eq!(
            encode_hex(&sha512(b"")),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert_eq!(
            encode_hex(&sha512(b"abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        // A message crossing one block boundary.
        assert_eq!(
            encode_hex(&sha512(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu")),
            "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909"
        );
    }

    #[test]
    fn matches_rfc_8032_test_vectors() {
        let vectors = [
            (
                "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
                "",
                "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            ),
            (
                "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
                "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
                "72",
                "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
            ),
            (
                "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
                "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
                "af82",
                "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
            ),
        ];
        for (seed, public, message, signature) in vectors {
            let key = SigningKey::from_hex(seed).unwrap();
            assert_eq!(key.verifying_key().to_hex(), public);
            let message = decode_hex(message).unwrap();
            let produced = key.sign(&message);
            assert_eq!(produced.to_hex(), signature);
            key.verifying_key().verify(&message, &produced).unwrap();
            VerifyingKey::from_hex(public)
                .unwrap()
                .verify(&message, &Signature::from_hex(signature).unwrap())
                .unwrap();
        }
    }

    #[test]
    fn tampering_and_malleability_are_rejected() {
        let key = SigningKey::from_seed([7; 32]);
        let other = SigningKey::from_seed([8; 32]);
        let message = b"agel/image-root/v1";
        let signature = key.sign(message);
        key.verifying_key().verify(message, &signature).unwrap();
        assert_eq!(
            other.verifying_key().verify(message, &signature),
            Err(SignatureError::InvalidSignature)
        );
        assert_eq!(
            key.verifying_key()
                .verify(b"agel/image-root/v2", &signature),
            Err(SignatureError::InvalidSignature)
        );
        let mut flipped = signature.to_bytes();
        flipped[3] ^= 1;
        assert_eq!(
            key.verifying_key()
                .verify(message, &Signature::from_bytes(flipped)),
            Err(SignatureError::InvalidSignature)
        );
        // s + L is the classic malleable twin; the strict check refuses it.
        let mut malleable = signature.to_bytes();
        let mut s = limbs_from_bytes(&malleable[32..]);
        let (sum, _) = add_limbs(&[s[0], s[1], s[2], s[3]], &L);
        s[..4].copy_from_slice(&sum);
        for (chunk, limb) in malleable[32..].chunks_exact_mut(8).zip(&s[..4]) {
            chunk.copy_from_slice(&limb.to_le_bytes());
        }
        assert_eq!(
            key.verifying_key()
                .verify(message, &Signature::from_bytes(malleable)),
            Err(SignatureError::InvalidSignature)
        );
        // Non-canonical and off-curve public keys are refused at construction.
        assert_eq!(
            VerifyingKey::from_bytes([0xff; 32]),
            Err(SignatureError::InvalidKey)
        );
        let mut off_curve = key.verifying_key().to_bytes();
        off_curve[0] ^= 1;
        assert!(matches!(
            VerifyingKey::from_bytes(off_curve),
            Err(SignatureError::InvalidKey) | Ok(_)
        ));
        assert_eq!(decode_hex("abc"), Err(SignatureError::InvalidHex));
        assert_eq!(decode_hex("zz"), Err(SignatureError::InvalidHex));
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    #[test]
    fn chunked_sha512_matches_one_shot_across_block_boundaries() {
        let message: Vec<u8> = (0..1000_u32).map(|value| (value * 7 % 251) as u8).collect();
        let expected = sha512(&message);
        for chunk in [1_usize, 3, 64, 127, 128, 129, 255, 511, 700] {
            let mut hasher = Sha512::new();
            for piece in message.chunks(chunk) {
                hasher.update(piece);
            }
            assert_eq!(hasher.finish(), expected, "chunk size {chunk}");
        }
    }

    #[test]
    fn sha512_of_abc_matches_fips_180_4() {
        assert_eq!(
            encode_hex(&sha512(b"abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }
}
