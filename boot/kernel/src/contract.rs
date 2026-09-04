//! The research kernel's implementation of the Agel kernel contract.
//!
//! The object semantics are the shared reference model from `agel-kernel-abi`.
//! That is deliberate, and it is the point rather than a shortcut: the research
//! backend's job in Phase 1 is not to be a second independent implementation —
//! seL4 will be that — but to put the *already specified* semantics behind a
//! real hardware privilege boundary and prove the boundary holds.
//!
//! So what this module adds to the model is exactly the part a hosted model
//! cannot have: the object table lives in supervisor-only memory, the caller
//! holds slot numbers rather than references, and the only path from ring 3 to
//! any of it is a trap gate.

use agel_kernel_abi::model::ModelKernel;
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
    model: ModelKernel,
}

impl DomainObjects {
    /// A domain holding the conformance capability space.
    pub fn new() -> Self {
        Self {
            model: ModelKernel::new(),
        }
    }

    /// True when this invocation is the world yielding to its supervisor.
    pub fn is_supervisor_yield(&self, request: &Request) -> bool {
        matches!(request.operation, Operation::EndpointSend)
            && request.capability == SUPERVISOR_ENDPOINT
    }

    /// Answer one contract invocation.
    pub fn invoke(&mut self, request: &Request) -> Response {
        self.model.invoke(request)
    }
}

impl Default for DomainObjects {
    fn default() -> Self {
        Self::new()
    }
}
