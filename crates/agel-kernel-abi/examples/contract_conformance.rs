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
    let mut kernel = ModelKernel::new();

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
