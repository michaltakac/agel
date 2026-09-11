//! Creates two files through its namespace: `etc/secret` and `app/notes`,
//! both relative to whatever the operator made its root. Run with a
//! read-only namespace, its first `open` is refused and it exits with the
//! error number.
#![no_std]
#![no_main]

use agel_process_abi as abi;

fn create(process: &abi::Process, path: &[u8], contents: &[u8]) {
    let descriptor = process.open(path, abi::O_WRONLY | abi::O_CREAT);
    if descriptor < 0 {
        process.write(2, b"writer: open ");
        process.write(2, path);
        process.report(2, b": error ", -descriptor, b"\n");
        process.exit((-descriptor) as u64);
    }
    let written = process.write(descriptor as u64, contents);
    if written as i64 != contents.len() as i64 {
        process.report(2, b"writer: short write: ", written as i64, b"\n");
        process.exit(5);
    }
    process.close(descriptor as u64);
}

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    // Safety: the supervisor entered this domain with its shared page here.
    let process = unsafe { abi::Process::new(shared_page) };
    create(&process, b"etc/secret", b"top secret\n");
    create(&process, b"app/notes", b"notes for the app\n");
    process.write(1, b"writer: wrote etc/secret and app/notes\n");
    process.exit(0)
}
