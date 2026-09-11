//! A program that misbehaves: it announces itself and then writes to an
//! address it was never given. The supervisor must contain it, not trust it.
#![no_std]
#![no_main]

mod abi;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    let process = unsafe { abi::Process::new(shared_page) };
    process.write(1, b"hostile process about to write where it may not\n");
    // Safety: deliberately unsound; the point is that it faults.
    unsafe { (0x10_u64 as *mut u64).write_volatile(0xdead) };
    process.exit(0)
}
