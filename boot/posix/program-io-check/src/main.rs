#![no_std]
#![no_main]

use agel_process_abi as abi;

fn check(process: &abi::Process, passed: bool) {
    if !passed {
        process.write(2, b"program-io-check failed\n");
        process.exit(1);
    }
}

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(page: u64) -> ! {
    let process = unsafe { abi::Process::new(page) };
    let info = process.program_info();
    let mut sector = [0; 512];
    if info == -13 {
        check(&process, process.program_read(0, &mut sector) == -13);
        check(&process, process.program_write(0, &sector) == -13);
        check(&process, process.program_flush() == -13);
        check(&process, process.open(b"/judge-log", abi::O_RDONLY) == -13);
        check(
            &process,
            process.open(b"/no-canary-write", abi::O_WRONLY | abi::O_CREAT) == -13,
        );
        process.write(1, b"no-files\n");
    } else {
        check(&process, info > 0);
        let count = (info as u64) >> 32;
        check(&process, process.program_read(0, &mut sector) == 0);
        check(&process, &sector[..8] == b"AGELPR1\0");
        check(&process, process.program_read(count, &mut sector) == -22);
        check(&process, process.program_read(u64::MAX, &mut sector) == -22);
        let write = process.program_write(count, &sector);
        if write == -13 {
            check(&process, process.program_flush() == -13);
            process.write(1, b"read-only\n");
        } else {
            check(&process, write == -22);
            check(&process, process.program_write(u64::MAX, &sector) == -22);
            check(&process, process.program_flush() == 0);
            process.write(1, b"bounded-program-io\n");
        }
    }
    process.exit(0)
}
