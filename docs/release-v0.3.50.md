# v0.3.50 · Preserve judge receipts across guest process exit

The native bridge waits for outstanding inference work before shutting
down. The desktop routes protected ledger frames to the OS filesystem
service, so receipt storage can finish after the requesting Agel process
exits. A failed storage acknowledgement remains an error.

A QEMU regression starts a delayed fixture judge and exits immediately
after receiving the pending job ID. The disk then contains one matching
start and result, including the synthetic charge. Run it with
`python3 scripts/test-native-judge-exit.py` after building the native
desktop, Agel program, inference bridge, and Agel journal reader.

The combat scorer also accepts multiple recorded provider attempts for
one bridge job while requiring a unique start and receipt per attempt.
