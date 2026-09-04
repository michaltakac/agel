//! The frozen contract transcript is part of the repository, not a build
//! artifact, and this is what keeps it honest without needing an emulator.
//!
//! `./scripts/test-isolation.sh` proves that three freestanding backends
//! reproduce these bytes from an unprivileged protection domain. That takes
//! QEMU, three cross toolchains, and half a minute. This takes none of those,
//! so a contract change that nobody meant to make fails in `cargo test` on the
//! machine that made it.

use agel_kernel_abi::conformance;
use agel_kernel_abi::model::ModelKernel;
use std::fmt::Write as _;

/// Render the reference model's transcript exactly as the example does.
fn render() -> String {
    let mut kernel = ModelKernel::new();
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
    transcript
}

fn frozen() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../bootstrap/kernel-contract.trace"
    );
    std::fs::read_to_string(path).expect("the frozen transcript is checked in")
}

#[test]
fn the_reference_model_reproduces_the_frozen_transcript() {
    let rendered = render();
    let frozen = frozen();
    if rendered != frozen {
        // A diff of eighty near-identical lines is unreadable, so report the
        // first divergence and let the reader go to that step.
        let divergence = rendered
            .lines()
            .zip(frozen.lines())
            .enumerate()
            .find(|(_, (left, right))| left != right);
        match divergence {
            Some((line, (produced, expected))) => panic!(
                "contract transcript changed at line {}\n  produced: {produced}\n  frozen:   {expected}\n\
                 If the change is intended, bump the contract version and regenerate with\n\
                 `cargo run -q -p agel-kernel-abi --example contract_conformance > bootstrap/kernel-contract.trace`",
                line + 1
            ),
            None => panic!(
                "contract transcript changed length: produced {} lines, frozen has {}",
                rendered.lines().count(),
                frozen.lines().count()
            ),
        }
    }
}

#[test]
fn the_frozen_transcript_names_the_contract_version() {
    let first = frozen().lines().next().expect("a header line").to_owned();
    assert_eq!(
        first,
        format!(
            "agel-kernel-contract v{}.{}.{} corpus={} steps",
            agel_kernel_abi::VERSION_MAJOR,
            agel_kernel_abi::VERSION_MINOR,
            agel_kernel_abi::VERSION_PATCH,
            conformance::CORPUS.len()
        ),
        "the frozen transcript must state which contract version produced it"
    );
}

#[test]
fn every_corpus_step_appears_in_the_frozen_transcript_in_order() {
    let frozen = frozen();
    let mut lines = frozen.lines().skip(1);
    for step in conformance::CORPUS {
        let line = lines
            .next()
            .unwrap_or_else(|| panic!("the frozen transcript is missing {}", step.label));
        assert!(
            line.starts_with(&format!("{}: ", step.label)),
            "the frozen transcript is out of order: expected {}, found {line}",
            step.label
        );
    }
    assert_eq!(lines.next(), None, "the frozen transcript has extra lines");
}

#[test]
fn a_backend_that_answers_wrongly_is_detected() {
    // The comparison is only worth having if it fails when it should, so this
    // stands up a deliberately non-conformant backend and requires
    // `compare` to find the first place it lies.
    struct Liar(ModelKernel);

    impl agel_kernel_abi::Kernel for Liar {
        fn invoke(&mut self, request: &agel_kernel_abi::Request) -> agel_kernel_abi::Response {
            let honest = self.0.invoke(request);
            // Widening authority is the failure the corpus exists to catch.
            if matches!(request.operation, agel_kernel_abi::Operation::CapMint)
                && honest.status == agel_kernel_abi::Status::InsufficientRights
            {
                return agel_kernel_abi::Response::ok1(0xbad);
            }
            honest
        }

        fn reset_to_conformance_domain(&mut self) {
            self.0.reset_to_conformance_domain();
        }
    }

    let mut liar = Liar(ModelKernel::new());
    let divergence = conformance::compare(&mut ModelKernel::new(), &mut liar)
        .expect_err("a backend that widens rights must not pass the corpus");
    assert_eq!(divergence.label, "derive/mint-cannot-widen");
    assert_eq!(
        divergence.left.status,
        agel_kernel_abi::Status::InsufficientRights
    );
    assert_eq!(divergence.right.status, agel_kernel_abi::Status::Ok);

    assert!(conformance::check_invariants(&mut Liar(ModelKernel::new())).is_err());
}
