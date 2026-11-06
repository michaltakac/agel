#![no_std]
#![no_main]

use agel_process_abi as abi;

fn check(process: &abi::Process, passed: bool, label: &[u8]) {
    if !passed {
        process.write(2, label);
        process.exit(1);
    }
}

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    // The supervisor supplies the process ABI page at entry.
    let process = unsafe { abi::Process::new(shared_page) };
    let descriptor = process.open(b"judge-log", abi::O_WRONLY | abi::O_CREAT | abi::O_APPEND);
    check(&process, descriptor >= 0, b"append open failed\n");
    check(
        &process,
        process.seek(descriptor as u64, 0, 0) == 0,
        b"seek failed\n",
    );
    check(
        &process,
        process.write(descriptor as u64, b"probe\n") == 6,
        b"append failed\n",
    );
    check(
        &process,
        process.ftruncate(descriptor as u64, 0) == -13,
        b"truncate allowed\n",
    );
    process.close(descriptor as u64);
    let descriptor = process.open(b"judge-log", abi::O_WRONLY);
    check(&process, descriptor >= 0, b"write open failed\n");
    check(
        &process,
        process.write(descriptor as u64, b"erase") as i64 == -13,
        b"overwrite allowed\n",
    );
    process.close(descriptor as u64);
    check(
        &process,
        process.unlink(b"./judge-log") == -13,
        b"unlink allowed\n",
    );
    check(
        &process,
        process.rename(b"judge-log", b"old-log") == -13,
        b"rename allowed\n",
    );
    let descriptor = process.open(b"ordinary", abi::O_WRONLY | abi::O_CREAT);
    check(&process, descriptor >= 0, b"ordinary open failed\n");
    process.close(descriptor as u64);
    check(
        &process,
        process.rename(b"ordinary", b"judge-log") == -13,
        b"replacement allowed\n",
    );
    process.write(1, b"journal-check: append and denial checks passed\n");
    process.exit(0)
}
