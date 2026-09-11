//! Print the canonical kernel-contract transcript of the independent
//! implementation, the one the seL4 broker runs.
//!
//! `./scripts/test-kernel-contract.sh` diffs this against the frozen transcript
//! alongside the reference model's, so the two hosted implementations and every
//! native backend are held to one set of bytes.

use agel_kernel_abi::conformance;
use agel_kernel_abi::independent::IndependentKernel;
use std::fmt::Write as _;

fn main() {
    // `--profile v1.0` prints the transcript a backend publishing only the
    // v1.0 profile produces; the default is the full v1.1 profile.
    let profile = match std::env::args().nth(2).as_deref() {
        Some("v1.0") => agel_kernel_abi::model::group::V1_PROFILE,
        Some("v1.1") | None => agel_kernel_abi::model::group::V1_1_PROFILE,
        Some(other) => panic!("unknown profile {other}; use v1.0 or v1.1"),
    };
    let mut kernel = IndependentKernel::with_profile(profile);

    conformance::check_invariants(&mut kernel).unwrap_or_else(|failure| {
        panic!("independent implementation violates the contract: {failure}")
    });

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
