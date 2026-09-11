//! Reads `notes` from its namespace root and prints it, then shows what the
//! namespace refuses: a name outside it is `ENOENT` whether or not the file
//! exists on the disk, and climbing above the root is `EACCES`.
#![no_std]
#![no_main]

use agel_process_abi as abi;

fn refused(process: &abi::Process, path: &[u8]) {
    let descriptor = process.open(path, abi::O_RDONLY);
    process.write(1, b"reader: ");
    process.write(1, path);
    if descriptor < 0 {
        process.report(1, b": error ", -descriptor, b"\n");
    } else {
        process.write(1, b": opened, which the namespace should have refused\n");
        process.exit(1);
    }
}

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    // Safety: the supervisor entered this domain with its shared page here.
    let process = unsafe { abi::Process::new(shared_page) };
    let descriptor = process.open(b"notes", abi::O_RDONLY);
    if descriptor < 0 {
        process.report(2, b"reader: open notes: error ", -descriptor, b"\n");
        process.exit((-descriptor) as u64);
    }
    let mut buffer = [0_u8; 64];
    let count = process.read(descriptor as u64, &mut buffer);
    if count < 0 {
        process.report(2, b"reader: read notes: error ", -count, b"\n");
        process.exit((-count) as u64);
    }
    process.write(1, b"reader: notes: ");
    process.write(1, &buffer[..count as usize]);
    if process.read(descriptor as u64, &mut buffer) != 0 {
        process.write(2, b"reader: expected the end of the file\n");
        process.exit(1);
    }
    process.close(descriptor as u64);
    if process.read(descriptor as u64, &mut buffer) != -9 {
        process.write(2, b"reader: a closed descriptor should be EBADF\n");
        process.exit(1);
    }
    refused(&process, b"etc/secret");
    refused(&process, b"../etc/secret");
    process.exit(0)
}
