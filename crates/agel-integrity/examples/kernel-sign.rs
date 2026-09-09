//! Sign a kernel image for admission by a running Agel kernel.
//!
//! The kernel verifies an Ed25519 signature over the SHA-512 digest of the
//! staged slot's bytes against the public key it was built with
//! (`bootstrap/kernel-signing.pub`). This tool makes the matching signature.
//!
//! ```text
//! kernel-sign keygen KEY      write a fresh 32-byte seed to KEY; print the public key
//! kernel-sign public KEY      print the public key for KEY
//! kernel-sign sign KEY FILE   print the signature over sha512(FILE), as hex
//! ```

use agel_integrity::{encode_hex, sha512, SigningKey};
use std::io::Read as _;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["keygen", path] => keygen(path),
        ["public", key] => load(key).map(|key| key.verifying_key().to_hex()),
        ["sign", key, file] => sign(key, file),
        _ => Err("usage: kernel-sign keygen KEY | public KEY | sign KEY FILE".into()),
    };
    match result {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("kernel-sign: {error}");
            std::process::exit(2);
        }
    }
}

fn load(path: &str) -> Result<SigningKey, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    SigningKey::from_hex(&text).map_err(|error| format!("{path}: {error}"))
}

fn keygen(path: &str) -> Result<String, String> {
    let mut seed = [0_u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut seed))
        .map_err(|error| format!("/dev/urandom: {error}"))?;
    let key = SigningKey::from_seed(seed);
    std::fs::write(path, format!("{}\n", encode_hex(&seed)))
        .map_err(|error| format!("{path}: {error}"))?;
    Ok(key.verifying_key().to_hex())
}

fn sign(key: &str, file: &str) -> Result<String, String> {
    let key = load(key)?;
    let bytes = std::fs::read(file).map_err(|error| format!("{file}: {error}"))?;
    Ok(key.sign(&sha512(&bytes)).to_hex())
}
