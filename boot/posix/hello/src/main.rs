//! The first program the Agel supervisor loads from disk: it writes a line
//! to the console through the process protocol and exits with a status.
#![no_std]
#![no_main]

use agel_process_abi as abi;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    // Safety: the supervisor entered this domain with its shared page here.
    let process = unsafe { abi::Process::new(shared_page) };
    process.write(1, b"hello from a loaded process\n");
    process.exit(42)
}
