//! Print the canonical kernel-contract transcript of the reference model.
//!
//! `./scripts/test-kernel-contract.sh` diffs this against the frozen transcript
//! in `bootstrap/kernel-contract.trace` and against the transcript the
//! freestanding research kernel produces inside QEMU. Three artifacts, one set
//! of bytes.

use agel_kernel_abi::conformance;
use agel_kernel_abi::model::ModelKernel;
use std::fmt::Write as _;

fn main() {
    // `--profile v1.0` prints the transcript a backend publishing only the
    // v1.0 profile produces; the default is the full v1.1 profile.
    let profile = match std::env::args().nth(2).as_deref() {
        Some("v1.0") => agel_kernel_abi::model::group::V1_PROFILE,
        Some("v1.1") | None => agel_kernel_abi::model::group::V1_1_PROFILE,
        Some(other) => panic!("unknown profile {other}; use v1.0 or v1.1"),
    };
    let mut kernel = ModelKernel::with_profile(profile);

    conformance::check_invariants(&mut kernel)
        .unwrap_or_else(|failure| panic!("reference model violates the contract: {failure}"));

    let mut transcript = String::new();
    writeln!(
        transcript,
        "agel-kernel-contract v{}.{}.{} corpus={} steps",
        agel_kernel_abi::VERSION_MAJOR,
        agel_kernel_abi::VERSION_MINOR,
        agel_kernel_abi::VERSION_PATCH,
        conformance::CORPUS.len()
    )
    .expect("writing to a string cannot fail");
    conformance::transcribe(&mut kernel, &mut transcript).expect("writing to a string cannot fail");
    print!("{transcript}");
}
