//! The research kernel's implementation of the Agel kernel contract.
//!
//! The object semantics are the independent implementation from
//! `agel-kernel-abi`: the one written from the contract document and the
//! corpus without reading the reference model, and the one the seL4 broker
//! answers with. Until v0.2.31 the research kernels linked the reference
//! model instead, so their transcripts proved the boundary held and nothing
//! about the semantics; now every backend answers with the same second
//! implementation, and the isolation self-test keeps the reference model in
//! the supervisor as the oracle it checks each answer against.
//!
//! What this module adds is the part a hosted implementation cannot have: the
//! object table lives in supervisor-only memory, the caller holds slot numbers
//! rather than references, and the only path from ring 3 to any of it is a
//! trap gate.

use agel_kernel_abi::independent::IndependentKernel;
use agel_kernel_abi::{Kernel, Operation, Request, Response};

/// Well-known slot through which a world hands control back to its supervisor.
///
/// This is a backend convention, not part of the conformance capability space:
/// it sits above every slot the corpus touches, so a world can yield without
/// the corpus ever observing that the slot exists.
pub const SUPERVISOR_ENDPOINT: u32 = agel_kernel_abi::CONFORMANCE_SLOTS - 1;

/// One domain's kernel objects.
///
/// A [`Domain`](crate::domain::Domain) owns one of these. It is stored in
/// kernel memory that the domain's own address space maps without the user
/// bit, so the world can invoke its capabilities and cannot read, forge, or
/// corrupt them.
pub struct DomainObjects {
    objects: IndependentKernel,
}

impl DomainObjects {
    /// A domain holding the conformance capability space.
    pub fn new() -> Self {
        Self {
            objects: IndependentKernel::new(),
        }
    }

    /// True when this invocation is the world yielding to its supervisor.
    pub fn is_supervisor_yield(&self, request: &Request) -> bool {
        matches!(request.operation, Operation::EndpointSend)
            && request.capability == SUPERVISOR_ENDPOINT
    }

    /// Answer one contract invocation.
    pub fn invoke(&mut self, request: &Request) -> Response {
        self.objects.invoke(request)
    }
}

impl Default for DomainObjects {
    fn default() -> Self {
        Self::new()
    }
}
