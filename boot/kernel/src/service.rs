//! Restartable driver domains.
//!
//! Phase 3 of the native roadmap is "put each risky driver in its own
//! restartable domain". This is the machinery for the second half of that
//! sentence: a driver is not merely somewhere else, it is something the
//! supervisor can lose and replace while continuing to run.
//!
//! The restart rule is the one the contract already names. A service that
//! restarts gets a new generation, and a handle from before the restart fails
//! closed with [`Status::StaleGeneration`] rather than being quietly accepted
//! against a server that no longer remembers the conversation. Until now no
//! backend had any use for that status; a driver that can die is what makes it
//! real.
//!
//! Why the console first, when the roadmap could have started anywhere: the
//! Agel evaluator still runs privileged, and moving it into a domain is the
//! next rung after this one. An unprivileged evaluator needs somewhere to
//! print, and it must not be handed the device to do it. The driver domain is
//! what it will print through.

use crate::arch;
use crate::world::{shared, Stop, PAYLOAD_BYTES};
use agel_kernel_abi::Status;
use core::fmt;

/// A capability-shaped reference to a service.
///
/// It carries the generation it was issued against. That is the whole content:
/// holding one is not authority over the service, it is a claim about *which*
/// service, and the claim is checked.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ServiceHandle {
    generation: u32,
}

impl ServiceHandle {
    /// The generation this handle was issued against.
    pub fn generation(self) -> u32 {
        self.generation
    }
}

/// Why a request to a service did not happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceError {
    /// The handle predates a restart. It fails closed rather than being
    /// forgiven, because a caller that has not noticed a restart is a caller
    /// whose assumptions about the service are stale too.
    Stale,
    /// The service is stopped and has not been restarted.
    Stopped,
    /// The service faulted while handling this request.
    Faulted,
}

impl ServiceError {
    /// The contract status this corresponds to.
    pub fn status(self) -> Status {
        match self {
            Self::Stale => Status::StaleGeneration,
            Self::Stopped | Self::Faulted => Status::FaultedDomain,
        }
    }

    /// A short name for serial reports.
    pub fn name(self) -> &'static str {
        match self {
            Self::Stale => "stale-generation",
            Self::Stopped => "faulted-domain",
            Self::Faulted => "faulted-domain",
        }
    }
}

/// An unprivileged driver domain the supervisor can lose and replace.
pub struct ServiceDomain {
    domain: arch::Domain,
    entry: u64,
    ticks: u32,
    generation: u32,
    restarts: u32,
}

impl ServiceDomain {
    /// Adopt `domain` as generation one of a service entered at `entry`.
    pub fn new(domain: arch::Domain, entry: u64, ticks: u32) -> Self {
        Self {
            domain,
            entry,
            ticks,
            generation: 1,
            restarts: 0,
        }
    }

    /// A handle valid against the current generation.
    pub fn handle(&self) -> ServiceHandle {
        ServiceHandle {
            generation: self.generation,
        }
    }

    /// The current generation.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// How many times this service has been replaced.
    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    /// Whether the service is currently stopped.
    pub fn stopped(&self) -> Option<Stop> {
        self.domain.stopped()
    }

    /// Ask the driver to print `bytes`.
    ///
    /// The handle is checked before anything else, so a caller holding a stale
    /// one is refused without the service being entered at all.
    pub fn write_console(
        &mut self,
        handle: ServiceHandle,
        bytes: &[u8],
    ) -> Result<(), ServiceError> {
        if handle.generation != self.generation {
            return Err(ServiceError::Stale);
        }
        if self.domain.stopped().is_some() {
            return Err(ServiceError::Stopped);
        }
        let count = bytes.len().min(PAYLOAD_BYTES);
        for (offset, byte) in bytes.iter().take(count).enumerate() {
            self.domain.core().write_payload(offset, *byte);
        }
        self.domain
            .core()
            .write_shared(shared::ARGUMENTS, count as u64);
        match self.domain.provoke(shared::COMMAND_WRITE_CONSOLE) {
            Stop::Replied => Ok(()),
            _ => Err(ServiceError::Faulted),
        }
    }

    /// Ask the driver to do something that will stop it, for the test that
    /// proves the supervisor survives losing it.
    pub fn provoke(&mut self, command: u64) -> Stop {
        self.domain.provoke(command)
    }

    /// Replace the service with a fresh domain at a new generation.
    ///
    /// The old domain's frames are not reclaimed; the frame pool never frees,
    /// which is stated rather than hidden here as everywhere else. What matters
    /// for the restart claim is that the replacement is a different domain with
    /// a different address space, not a resumed one.
    pub fn restart(&mut self, machine: &mut arch::Machine) -> Result<(), &'static str> {
        let replacement = machine.create_console_world(self.entry, self.ticks)?;
        self.domain = replacement;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("service generation space exhausted")?;
        self.restarts += 1;
        Ok(())
    }
}

/// A `core::fmt` sink that prints through a driver domain.
///
/// Text is buffered here rather than in the domain's page so that a restart in
/// the middle of a line cannot leave half a message in a page that no longer
/// belongs to anyone.
pub struct ServiceWriter<'a> {
    service: &'a mut ServiceDomain,
    handle: ServiceHandle,
    buffer: [u8; PAYLOAD_BYTES],
    filled: usize,
    failure: Option<ServiceError>,
}

impl<'a> ServiceWriter<'a> {
    /// Write through `service`, using a handle taken at its current generation.
    pub fn new(service: &'a mut ServiceDomain) -> Self {
        let handle = service.handle();
        Self {
            service,
            handle,
            buffer: [0; PAYLOAD_BYTES],
            filled: 0,
            failure: None,
        }
    }

    /// Write through `service` using a handle the caller already holds, which
    /// may be older than the service's current generation.
    pub fn with_handle(service: &'a mut ServiceDomain, handle: ServiceHandle) -> Self {
        Self {
            service,
            handle,
            buffer: [0; PAYLOAD_BYTES],
            filled: 0,
            failure: None,
        }
    }

    /// Send whatever is buffered.
    pub fn flush(&mut self) {
        if self.filled == 0 || self.failure.is_some() {
            return;
        }
        if let Err(error) = self
            .service
            .write_console(self.handle, &self.buffer[..self.filled])
        {
            self.failure = Some(error);
        }
        self.filled = 0;
    }

    /// The first failure this writer met, if any.
    ///
    /// Printing that silently does nothing is worse than printing that fails,
    /// so the failure is kept rather than dropped on the floor.
    pub fn failure(&self) -> Option<ServiceError> {
        self.failure
    }

    fn push(&mut self, byte: u8) {
        if self.filled == PAYLOAD_BYTES {
            self.flush();
        }
        if self.failure.is_some() {
            return;
        }
        self.buffer[self.filled] = byte;
        self.filled += 1;
        if byte == b'\n' {
            self.flush();
        }
    }
}

impl fmt::Write for ServiceWriter<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                self.push(b'\r');
            }
            self.push(byte);
        }
        Ok(())
    }
}
