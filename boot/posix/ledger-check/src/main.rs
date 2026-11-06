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
pub extern "C" fn _start(page: u64) -> ! {
    // The supervisor supplies the shared ABI page at entry.
    let process = unsafe { abi::Process::new(page) };
    let token = process.ledger_reserve(1024);
    check(&process, token > 0, b"reserve failed\n");
    check(
        &process,
        process.ledger_reserve(512) == -28,
        b"overlapping reservation admitted\n",
    );
    let mut block = [b' '; 512];
    block[..9].copy_from_slice(b"leased-a\n");
    let descriptor = process.open(b"judge-ledger", abi::O_WRONLY | abi::O_APPEND);
    check(&process, descriptor >= 0, b"append descriptor failed\n");
    check(
        &process,
        process.write(descriptor as u64, &block) as i64 == -28,
        b"ordinary append consumed reserved space\n",
    );
    process.close(descriptor as u64);
    check(
        &process,
        process.ledger_append(token as u64, &block) == 512,
        b"first leased write failed\n",
    );
    check(
        &process,
        process.ledger_release(token as u64) == 0,
        b"release failed\n",
    );
    check(
        &process,
        process.ledger_append(token as u64, &block) == -13,
        b"stale token accepted\n",
    );
    let next = process.ledger_reserve(512);
    check(
        &process,
        next > 0 && next != token,
        b"reservation token reused\n",
    );
    block[7] = b'b';
    check(
        &process,
        process.ledger_append(next as u64, &block) == 512,
        b"second leased write failed\n",
    );
    check(
        &process,
        process.ledger_release(next as u64) == 0,
        b"final release failed\n",
    );
    check(
        &process,
        process.ledger_reserve(512) == -28,
        b"full store admitted reservation\n",
    );
    process.write(
        1,
        b"ledger-check: reserved space and stale-token checks passed\n",
    );
    process.exit(0)
}
