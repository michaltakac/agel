# Native byte streams

`agel/byte-stream` adapts a caller-supplied `ask` function to open, close, read,
write and entropy operations. Its request format is `AGEL-BYTES/1 OPERATION`.
The native `byte-request` bootstrap word moves request/reply text through the
console. The external `byte_bridge` executable owns a TCP socket to an explicit
CLI-selected numeric endpoint and returns bounded byte transfers. Neither
layer recognizes a game packet, account, skill, item, cipher or agent policy.

Load `bytes.agel`, `byte-stream.agel` and `buffered-stream.agel` in a standard
Agel world before importing these optional modules. In the native process:

```lisp
(import agel/byte-stream)
(import agel/buffered-stream)
(def stream
  (buffered-stream (byte-stream byte-request) 256
    (list (request-capability "byte/relay" "*"))))
((get stream 'open))
((get stream 'write) '(0 255 10 128))
((get stream 'read) 4)
((get stream 'close))
```

The raw adapter reads at most 256 bytes per transport call. Its public read
count is 1–65,536; a read returns available bytes up to its bound, including nil
at EOF. Writes accept up to 65,536 bytes and validate the whole input before
sending it in 256-byte pieces. Entropy reads are 1–256 bytes. Short acknowledgments,
invalid replies and wrong lengths signal errors. Writes and socket operations
are external effects, so a later Agel failure cannot undo already sent bytes.

`buffered-stream` owns its receive buffer in an Agel agent. Its synchronous
facade runs that agent from the top-level controller; it is not a concurrent
multi-reader protocol. The returned `agent` handle supports inspection of its
heap and physical-read counter. Buffer bytes are retained as compact hex text,
avoiding repeated large byte-vector copies in actor history. Small application
reads consume buffered data before issuing another transport request. Open and
close clear the buffer and EOF latch. An I/O/service failure latches failure;
close/reopen is required before the stream can be read again.

`start-service(function, state)` installs an Agel callback before physical
receive refills. It returns `{state ..., bytes ...}`; the worker writes those
bytes, checks the acknowledgment, then keeps the new service state. The callback
is not called for reads served from the buffer. `stop-service()` returns its
current state and removes it. If the following read fails, acknowledged service
state remains available. Callers must not transparently replay a failed write.
This is a general maintenance hook, not a scheduler that runs while the whole
process is blocked elsewhere or performing a long computation.

The native companion is launched as:

```sh
cargo build --release -p agel-stdlib --example byte_bridge
# The socket belongs to a running Agel QEMU session.
target/release/examples/byte_bridge SOCKET 127.0.0.1:PORT ':exec agel -- --no-stdlib /data/PROGRAM'
```

It uses the bounded console frame `0x1d PAYLOAD newline`. Payloads are UTF-8;
the in-band byte protocol represents binary data as hex. A fresh separator
resynchronizes an incomplete frame. Oversized/invalid frames, closed/full input
or absence of a running process are refused. The frame queues process input
without displaying its payload or evaluating it as a workshop command. The
usual foreground console and older `:stdin` mechanism remain available. This
requires the matching v0.3.16 native runtime/kernel. It is a single-owner
bootstrap transport, not a native NIC or TCP stack inside Agel OS.

The model-inference broker remains separate. Simultaneous inference and live
socket ownership still need integration; the byte companion does not invoke
providers or append fabricated judge costs. Request timing lines are carrier
diagnostics. Application observations and decisions remain Agel's responsibility.
