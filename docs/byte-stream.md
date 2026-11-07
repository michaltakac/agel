# Native byte streams

`agel/byte-stream` adapts a caller-supplied `ask` function to open, close, read,
write and entropy operations. Its request format is `AGEL-BYTES/1 OPERATION`.
The native `byte-request` bootstrap word moves request/reply text through the
console. The external `byte_bridge` executable owns a TCP socket to an explicit
CLI-selected numeric endpoint and returns bounded byte transfers. Neither
layer recognizes a game packet, account, skill, item, cipher or agent policy.

Load `bytes.agel`, `byte-stream.agel`, `stream-buffer.agel`, `services.agel`
and `service-stream.agel` in a standard Agel world before importing these
optional modules. In the native process:

```lisp
(import agel/byte-stream)
(import agel/service-stream)
(def stream
  (service-stream (byte-stream byte-request) 256))
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

`service-stream` keeps its buffer in an Agel service. The `agel/services`
registry, serialized calls, state updates, nested-call handling, reentrancy
check and explicit fault recovery are Agel code. A service keeps its current
state, last request and call count. Application observations, actions and
version histories remain the application's responsibility. These services run
at the top level under the controller's authority; they share its protection
domain. Calling their mutation operations from a bootstrap actor is refused.

The returned `owner`/`service` handle identifies the service; `info()` returns
its state and counters. Buffer bytes are compact hex text. Small reads reuse
the buffer. Open/close clear it and the EOF latch. An I/O/service failure
latches failure; close/reopen is required before the stream can be read again.
These are serialized single-owner streams.

The optional `buffered-stream` adapter still provides the bootstrap actor
backend. Load `stream-buffer.agel` before it. It retains its `agent` handle
and adds the same `owner` and `info()` interfaces. Both adapters execute the
same `agel/stream-buffer` state machine and the same behavior tests. The
native runner uses Agel services to avoid retaining an opaque bootstrap heap
snapshot for every small buffered read. The service tests verify that this
backend creates no bootstrap actor events.

`start-service(function, state)` installs an Agel callback before physical
receive refills. It returns `{state ..., bytes ...}`; the worker writes those
bytes, checks the acknowledgment, then keeps the new service state. The callback
is not called for reads served from the buffer. `stop-service()` returns its
current state and removes it. `pulse()` explicitly invokes the same service
without reading; this lets an Agel controller maintain a connection between
inference polls. Stop a service before installing a replacement. If the
following read fails, acknowledged service
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

The combined `guest_inference` broker can own the same carrier when given
`AGEL_BYTE_ENDPOINT=127.0.0.1:PORT`. Its polling protocol lets Agel call
`pulse()` between model polls; see [the inference API](model-agents.md#polling-inference-during-a-live-byte-session).
The carrier does not invoke application policy or fabricate judge costs.
Request diagnostics contain operation/status information, excluding byte
payloads and credentials. Application observations and decisions remain Agel's
responsibility.

`bytes-pack-field(record, key)` stores a byte-list field as a tagged hex value;
`bytes-unpack-field(record, key)` restores it and also accepts legacy raw byte
lists. Metadata stays in the record. The codec preserves the difference between
nil and an explicit empty list and refuses malformed tagged values. This keeps
large retained observations compact without discarding their bytes. Hex
encoding uses bounded chunks and pairwise joins rather than repeatedly copying
the entire growing output. A 4096-byte pack/unpack probe fits two million
Agel evaluation steps.

`agel/crypto` additionally exports `isaac-bytes-new(seed, capacity)`,
`isaac-bytes-next(context)` and `isaac-bytes-available(context)`. Capacities
are 1–512. These return the low byte of each ordinary ISAAC word in the same
order, and refill as needed. They are separate from the full-word API. A
refill closure shares the immutable future cipher state, so ordinary byte
contexts and service histories avoid repeatedly copying the large raw maps.
The byte buffers are opaque contexts containing a callable, not inert data
for `data-codec`. Reconstruct them from the appropriate seed/position when
restoring a protocol; live network authority is not restored by a source
checkpoint.

For long encodes, `bytes-hex-serviced(bytes, service)` and
`bytes-pack-field-serviced(record, key, service)` invoke a no-argument Agel
callback before the first 64-byte chunk and between later chunks. Shorter
inputs need no callback. Native clients use this to service a connection during
CPU work, in addition to servicing between inference polls. A callback's external
effects remain acknowledged even if later validation or encoding fails.

`bytes-hex-quantum(bytes, service, quantum)` and
`bytes-pack-field-quantum(record, key, service, quantum)` allow callers to
choose the callback interval. The interval must be a multiple of 64 in
64–4096; the 64-byte internal chunks and output representation stay fixed.

The optional `agel/work-profile` module exposes
`profile-work(operation, service, now)`: the operation receives a metered
service callback, and `now` supplies monotonic microseconds. The result holds
the operation value, elapsed time, callback time and count, maximum work
interval, and maximum gap between acknowledged service calls. Native
`clock-us` uses the existing clock read capability. `profile-limit` estimates
a conservative upper bound from the measured work, budget and margin; a
candidate still needs replay and measurements. The estimate is not a deadline
guarantee for new workloads.

Load `record-chain.agel` to keep a history in shareable chunks.
`record-chain-push(chain, record)` accepts nil for an empty history and
returns an updated history. Each history holds at most 32 records directly
and refers to earlier chunks through Agel closures. Older snapshots remain
valid after an append. `record-chain-fold(step, initial, chain)` visits every
record from newest to oldest without flattening the history. This is an
in-process representation. Callers can fold over it to write inert records
to durable storage.
