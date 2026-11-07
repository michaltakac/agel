#!/usr/bin/env python3
"""Exercise Agel packet logic over fragmented TCP, optionally against the world."""
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time

CLIENT = "target/debug/examples/osrs"
HELLO = bytes([15]) + (241).to_bytes(4, "big") + b"".join(n.to_bytes(4, "big") for n in [1, 2, 3, 4])


def call(port, program):
    return subprocess.run([CLIENT, str(port), str(program)], capture_output=True, text=True, timeout=20)


def fixture(program, exchanges, expected, success=True):
    errors = []
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(10)

        def serve():
            try:
                with listener.accept()[0] as stream:
                    stream.settimeout(5)
                    for request, response in exchanges:
                        received = b""
                        while len(received) < len(request):
                            piece = stream.recv(len(request) - len(received))
                            if not piece:
                                raise RuntimeError(f"early client EOF: {received!r}")
                            received += piece
                        assert received == request, (received, request)
                        for at in range(0, len(response), 7):
                            stream.sendall(response[at:at + 7])
                            time.sleep(.001)
            except Exception as problem:
                errors.append(repr(problem))

        worker = threading.Thread(target=serve, daemon=True)
        worker.start()
        result = call(listener.getsockname()[1], program)
        worker.join(timeout=12)
        assert not worker.is_alive(), "fixture failed to finish"
        assert not errors, (errors, result.stderr)
        assert (result.returncode == 0) == success, (result.stdout, result.stderr)
        assert expected in result.stdout + result.stderr, (result.stdout, result.stderr)


fixture("examples/osrs/js5.agel", [(HELLO, b"\0")], "phase js5 revision 241 status 0")
fixture("examples/osrs/wrong-revision.agel", [(HELLO[:4] + b"\xf0" + HELLO[5:], b"\6")], "detail 6")
fixture("examples/osrs/challenge.agel", [(b"\x0e", bytes([0, 255, 254, 253, 252, 251, 250, 249, 248]))],
        "session-bytes (255 254 253 252 251 250 249 248)")
fixture("examples/osrs/challenge.agel", [(b"\x0e", b"\0\1\2")], "osrs/eof", False)

with tempfile.TemporaryDirectory(prefix="agel-osrs-wire-") as directory:
    program = Path(directory) / "group.agel"
    program.write_text("(import agel/osrs-wire) (import agel/sequence) (osrs-js5-open 241 '(1 2 3 4)) "
                       "(let ((g (osrs-js5-group 255 255))) "
                       "(list (get g 'length) (foldl (fn (n _) (+ n 1)) 0 (get g 'bytes)) "
                       "(foldl + 0 (get g 'bytes))))")
    body = bytes(n % 256 for n in range(1100))
    header = b"\xff\0\xff\0" + len(body).to_bytes(4, "big")
    packet = header + body[:504] + b"\xff" + body[504:1015] + b"\xff" + body[1015:]
    fixture(program, [(HELLO, b"\0"), (b"\1\xff\0\xff", packet)], f"(1100 1100 {sum(body)})")
    fixture(program, [(HELLO, b"\0"), (b"\1\xff\0\xff", packet[:512] + b"\0")],
            "osrs/block", False)
    wrong = bytes([1]) + header[1:]
    fixture(program, [(HELLO, b"\0"), (b"\1\xff\0\xff", wrong)], "osrs/group", False)
    compressed_header = b"\xff\0\xff\2" + (1096).to_bytes(4, "big")
    fixture(program, [(HELLO, b"\0"), (b"\1\xff\0\xff", compressed_header + packet[8:])],
            f"(1096 1100 {sum(body)})")
    serviced = Path(directory) / "serviced.agel"
    serviced.write_text("""
      (import agel/osrs-wire)
      (def chunks '((1 2) (3) (4 5) (6)))
      (def pulses 0)
      (def wire-read (fn (_) (let ((head (car chunks))) (def chunks (cdr chunks)) head)))
      (def pulse (fn () (def pulses (+ 1 pulses))))
      (def fragmented (osrs-read-exact-serviced 5 pulse))
      (def complete (osrs-read-exact-serviced 1 pulse))
      (list fragmented complete pulses)
    """)
    result = call(1, serviced)
    assert result.returncode == 0 and result.stdout.strip() == "((1 2 3 4 5) (6) 2)", result
print("OSRS wire: fragmented handshake, unsigned challenge, revision refusal, EOF and cache blocks passed")

if "--live" in sys.argv:
    for program, expected in [
        ("js5.agel", "phase js5 revision 241 status 0"),
        ("wrong-revision.agel", "detail 6"),
        ("challenge.agel", "phase login-challenge status 0"),
        ("master-index.agel", "archive 255 group 255 compression 0"),
    ]:
        result = call(43594, Path("examples/osrs") / program)
        assert result.returncode == 0 and expected in result.stdout, (result.stdout, result.stderr)
        print(f"live {program}: {result.stdout.strip()}")
