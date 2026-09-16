//! Holds a descriptor across a restart of the filesystem service: opens
//! `notes`, reads it, sleeps long enough for the operator to `:fs-restart`,
//! and reads again, expecting `ESTALE`: the descriptor's authority came
//! from a service that no longer exists. Exits with 116 when it did, with
//! 1 when the read went through or failed some other way.
#![no_std]
#![no_main]

use agel_process_abi as abi;

/// How long the operator has to restart the service.
const SLEEP_MICROSECONDS: u64 = 6_000_000;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    // Safety: the supervisor entered this domain with its shared page here.
    let process = unsafe { abi::Process::new(shared_page) };
    let descriptor = process.open(b"notes", abi::O_RDONLY);
    if descriptor < 0 {
        process.report(2, b"stale: open notes: error ", -descriptor, b"\n");
        process.exit((-descriptor) as u64);
    }
    let mut buffer = [0_u8; 64];
    let count = process.read(descriptor as u64, &mut buffer);
    if count < 0 {
        process.report(2, b"stale: first read: error ", -count, b"\n");
        process.exit(1);
    }
    process.report(1, b"stale: read ", count, b" bytes before the restart\n");
    process.sleep(SLEEP_MICROSECONDS);
    let again = process.read(descriptor as u64, &mut buffer);
    if again == -116 {
        process.write(1, b"stale: read after the restart: error 116 (ESTALE)\n");
        process.close(descriptor as u64);
        process.exit(116)
    }
    process.report(2, b"stale: read after the restart answered ", again, b"\n");
    process.exit(1)
}
