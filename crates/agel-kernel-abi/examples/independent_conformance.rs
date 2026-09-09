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
    let mut kernel = IndependentKernel::new();

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
