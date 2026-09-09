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
//! Agel evaluator now runs in a domain in v0.1.6. An unprivileged evaluator needs
//! somewhere to print, and it must not be handed the device to do it. The
//! driver domain is what it prints through.

use crate::arch;
#[cfg(not(feature = "native-graphics"))]
use crate::world::PAYLOAD_BYTES;
use crate::world::{shared, Stop};
#[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
use agel_kernel_abi::Status;
#[cfg(not(feature = "native-graphics"))]
use core::fmt;

/// Which device a service domain drives. The kind decides how the domain is
/// rebuilt on restart: the same entry point, the same grant, a new generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceKind {
    /// The console: COM1 on x86-64, the UART page elsewhere.
    Console,
    /// The disk: the primary ATA controller on x86-64, a virtio block device
    /// behind a virtio-mmio transport elsewhere.
    Storage,
    /// The 8042 keyboard controller, x86-64 graphics only.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    Input,
}

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
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
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
    /// The service ran and its device reported a problem, identified by the
    /// driver's status code. The driver carries codes, never text.
    Device(u64),
}

impl ServiceError {
    /// The contract status this corresponds to.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn status(self) -> Status {
        match self {
            Self::Stale => Status::StaleGeneration,
            Self::Stopped | Self::Faulted => Status::FaultedDomain,
            Self::Device(_) => Status::ResourceExhausted,
        }
    }

    /// A short name for serial reports.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn name(self) -> &'static str {
        match self {
            Self::Stale => "stale-generation",
            Self::Stopped => "faulted-domain",
            Self::Faulted => "faulted-domain",
            Self::Device(_) => "device-error",
        }
    }
}

/// An unprivileged driver domain the supervisor can lose and replace.
pub struct ServiceDomain {
    domain: arch::Domain,
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    kind: ServiceKind,
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    entry: u64,
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    ticks: u32,
    generation: u32,
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    restarts: u32,
    /// Fault injection for the persistence suite: how many more sector writes
    /// this machine survives. The write the count lands on is torn, half of
    /// it reaching the disk, and the machine halts as if power had failed.
    #[cfg(feature = "isolated-repl")]
    power_cut: Option<u32>,
}

impl ServiceDomain {
    /// Adopt `domain` as generation one of a `kind` service entered at `entry`.
    pub fn new(domain: arch::Domain, _kind: ServiceKind, _entry: u64, _ticks: u32) -> Self {
        Self {
            domain,
            #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
            kind: _kind,
            #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
            entry: _entry,
            #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
            ticks: _ticks,
            generation: 1,
            #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
            restarts: 0,
            #[cfg(feature = "isolated-repl")]
            power_cut: None,
        }
    }

    /// Ask the console driver for one input byte, if one is waiting.
    #[cfg(any(feature = "isolated-repl", feature = "native-graphics"))]
    pub fn read_console(&mut self, handle: ServiceHandle) -> Result<Option<u8>, ServiceError> {
        self.check(handle)?;
        match self.domain.provoke(shared::COMMAND_READ_CONSOLE) {
            Stop::Replied => {}
            _ => return Err(ServiceError::Faulted),
        }
        if self.domain.core().read_shared(shared::STATUS) == 0 {
            return Ok(None);
        }
        Ok(Some(self.domain.core().read_shared(shared::VALUES) as u8))
    }

    /// Ask the input driver for one raw byte and whether the pointer sent it.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub fn read_input(
        &mut self,
        handle: ServiceHandle,
    ) -> Result<Option<(bool, u8)>, ServiceError> {
        self.check(handle)?;
        match self.domain.provoke(shared::COMMAND_READ_INPUT) {
            Stop::Replied => {}
            _ => return Err(ServiceError::Faulted),
        }
        if self.domain.core().read_shared(shared::STATUS) == 0 {
            return Ok(None);
        }
        let byte = self.domain.core().read_shared(shared::VALUES) as u8;
        let auxiliary = self.domain.core().read_shared(shared::VALUES + 1) != 0;
        Ok(Some((auxiliary, byte)))
    }

    /// Ask the input driver to enable the pointer; `false` when no pointer
    /// acknowledged, which leaves the keyboard usable.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub fn enable_pointer(&mut self, handle: ServiceHandle) -> Result<bool, ServiceError> {
        self.check(handle)?;
        match self.domain.provoke(shared::COMMAND_ENABLE_POINTER) {
            Stop::Replied => {}
            _ => return Err(ServiceError::Faulted),
        }
        Ok(self.domain.core().read_shared(shared::STATUS) != 0)
    }

    /// Ask the storage driver to read sector `lba` into `sector`.
    pub fn read_sector(
        &mut self,
        handle: ServiceHandle,
        lba: u32,
        sector: &mut [u8; crate::world::BLOCK_BYTES],
    ) -> Result<(), ServiceError> {
        self.block_request(handle, shared::COMMAND_READ_SECTOR, lba)?;
        for (offset, byte) in sector.iter_mut().enumerate() {
            *byte = self.domain.core().read_block(offset);
        }
        Ok(())
    }

    /// Arm a power cut: the `writes`-th sector write from now is torn and
    /// the machine halts. A test hook, reachable only from the serial
    /// workshop's `:cut-power`, so that every write in a save can be the one
    /// the power fails on.
    #[cfg(feature = "isolated-repl")]
    pub fn cut_power_after(&mut self, writes: u32) {
        self.power_cut = Some(writes.max(1));
    }

    /// Ask the storage driver to write `sector` to sector `lba`.
    #[cfg(any(feature = "isolated-repl", feature = "native-graphics"))]
    pub fn write_sector(
        &mut self,
        handle: ServiceHandle,
        lba: u32,
        sector: &[u8; crate::world::BLOCK_BYTES],
    ) -> Result<(), ServiceError> {
        self.check(handle)?;
        #[cfg(feature = "isolated-repl")]
        if let Some(remaining) = self.power_cut {
            if remaining <= 1 {
                // The tear: the first half of the sector reaches the disk and
                // the second half keeps what was there before. Then nothing.
                let mut torn = [0_u8; crate::world::BLOCK_BYTES];
                let _ = self.read_sector(handle, lba, &mut torn);
                torn[..256].copy_from_slice(&sector[..256]);
                for (offset, byte) in torn.iter().enumerate() {
                    self.domain.core().write_block(offset, *byte);
                }
                let _ = self.block_request(handle, shared::COMMAND_WRITE_SECTOR, lba);
                let _ = self.block_request(handle, shared::COMMAND_FLUSH_DISK, 0);
                crate::kprint!("power cut injected: sector {} torn; halting\n", lba);
                arch::exit(true);
            }
            self.power_cut = Some(remaining - 1);
        }
        for (offset, byte) in sector.iter().enumerate() {
            self.domain.core().write_block(offset, *byte);
        }
        self.block_request(handle, shared::COMMAND_WRITE_SECTOR, lba)
    }

    /// Ask the storage driver to flush the disk's write cache.
    #[cfg(any(feature = "isolated-repl", feature = "native-graphics"))]
    pub fn flush(&mut self, handle: ServiceHandle) -> Result<(), ServiceError> {
        self.block_request(handle, shared::COMMAND_FLUSH_DISK, 0)
    }

    fn check(&self, handle: ServiceHandle) -> Result<(), ServiceError> {
        if handle.generation != self.generation {
            return Err(ServiceError::Stale);
        }
        if self.domain.stopped().is_some() {
            return Err(ServiceError::Stopped);
        }
        Ok(())
    }

    fn block_request(
        &mut self,
        handle: ServiceHandle,
        command: u64,
        lba: u32,
    ) -> Result<(), ServiceError> {
        self.check(handle)?;
        self.domain
            .core()
            .write_shared(shared::ARGUMENTS, u64::from(lba));
        match self.domain.provoke(command) {
            Stop::Replied => {}
            _ => return Err(ServiceError::Faulted),
        }
        match self.domain.core().read_shared(shared::STATUS) {
            0 => Ok(()),
            code => Err(ServiceError::Device(code)),
        }
    }

    /// A handle valid against the current generation.
    pub fn handle(&self) -> ServiceHandle {
        ServiceHandle {
            generation: self.generation,
        }
    }

    /// The current generation.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// How many times this service has been replaced.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    /// Whether the service is currently stopped.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn stopped(&self) -> Option<Stop> {
        self.domain.stopped()
    }

    /// Ask the driver to print `bytes`.
    ///
    /// The handle is checked before anything else, so a caller holding a stale
    /// one is refused without the service being entered at all.
    #[cfg(not(feature = "native-graphics"))]
    pub fn write_console(
        &mut self,
        handle: ServiceHandle,
        bytes: &[u8],
    ) -> Result<(), ServiceError> {
        self.check(handle)?;
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
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn provoke(&mut self, command: u64) -> Stop {
        self.domain.provoke(command)
    }

    /// Replace the service with a fresh domain at a new generation.
    ///
    /// The old domain's frames are not reclaimed; the frame pool never frees,
    /// which is stated rather than hidden here as everywhere else. What matters
    /// for the restart claim is that the replacement is a different domain with
    /// a different address space, not a resumed one.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn restart(&mut self, machine: &mut arch::Machine) -> Result<(), &'static str> {
        let replacement = match self.kind {
            ServiceKind::Console => machine.create_console_world(self.entry, self.ticks)?,
            ServiceKind::Storage => machine.create_storage_world(self.entry, self.ticks)?,
            #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
            ServiceKind::Input => machine.create_input_world(self.entry, self.ticks)?,
        };
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
#[cfg(not(feature = "native-graphics"))]
pub struct ServiceWriter<'a> {
    service: &'a mut ServiceDomain,
    handle: ServiceHandle,
    buffer: [u8; PAYLOAD_BYTES],
    filled: usize,
    failure: Option<ServiceError>,
}

#[cfg(not(feature = "native-graphics"))]
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
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
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

#[cfg(not(feature = "native-graphics"))]
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
