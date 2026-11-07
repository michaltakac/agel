#!/usr/bin/env python3
import socket
import struct
import sys
import threading
sys.path.insert(0, "scripts/osrs")
from test_support import evaluate, form, xtea

p, q = (1 << 521) - 1, (1 << 127) - 1
n = p * q
d = pow(65537, -1, (p - 1) * (q - 1))
seed = [0xffffffff, 0x80000000, 0x7fffffff, 0]
challenge = bytes.fromhex("fedcba9876543210")
uuid = bytes(range(24))
crcs = [(0x87654321 + i * 991) & 0xffffffff for i in range(23)]
order = [(5,3),(4,3),(1,0),(17,0),(21,2),(9,3),(8,1),(22,2),(15,3),(19,1),(7,3),
         (0,3),(18,3),(12,3),(11,1),(14,2),(3,0),(13,3),(2,0),(10,2),(6,0),(16,0),(20,1)]


def exact(stream, count):
    out = b""
    while len(out) < count:
        piece = stream.recv(count - len(out))
        if not piece:
            raise RuntimeError("client EOF")
        out += piece
    return out


def check_packet(packet):
    assert packet[:15] == struct.pack('>III', 241, 1, 0) + bytes([1, 0, 0])
    size = int.from_bytes(packet[15:17], 'big')
    encrypted = int.from_bytes(packet[17:17 + size], 'big')
    raw = pow(encrypted, d, n)
    raw = raw.to_bytes((raw.bit_length() + 7) // 8, 'big')
    assert raw == b'\1' + struct.pack('>4I', *seed) + challenge + bytes([2,0,0,0,0,0]) + b'fixture-only-pass\0'
    body = xtea(packet[17 + size:], seed, decrypt=True)
    assert body.startswith(b'fixture01\0\2' + struct.pack('>HH', 640, 480) + uuid + bytes(6))
    crc_bytes = bytearray()
    permutations = [[0,1,2,3], [3,2,1,0], [2,3,0,1], [1,0,3,2]]
    for index, permutation in order:
        value = crcs[index].to_bytes(4, 'big')
        crc_bytes.extend(value[i] for i in permutations[permutation])
    assert body[-92:] == crc_bytes, "revision-specific CRC encoding differs"


source = '(import agel/osrs-login) (import agel/osrs-crypto) ' + \
    '(def context (rsa-context "' + format(n, 'x') + '")) ' + \
    '(osrs-login "fixture01" "fixture-only-pass" context \' ' + form(crcs) + \
    " '" + form(seed) + " '" + form(uuid) + ')'

for truncated in [False, True]:
    failures = []
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0)); listener.listen(); listener.settimeout(30)
        def serve():
            try:
                with listener.accept()[0] as stream:
                    stream.settimeout(30)
                    assert exact(stream, 1) == b'\x0e'
                    stream.sendall(b'\0' + challenge)
                    assert exact(stream, 1) == b'\x10'
                    size = int.from_bytes(exact(stream, 2), 'big')
                    check_packet(exact(stream, size))
                    stream.sendall(bytes([2, 37]) + bytes(10 if truncated else 34))
            except Exception as problem:
                failures.append(repr(problem))
        worker = threading.Thread(target=serve, daemon=True); worker.start()
        result = evaluate(source, listener.getsockname()[1])
        worker.join(timeout=35)
        assert not worker.is_alive() and not failures, (failures, result.stderr)
        if truncated:
            assert result.returncode != 0 and 'osrs/eof' in result.stderr, result.stderr
        else:
            assert result.returncode == 0 and 'phase logged-in' in result.stdout, result.stderr
print("OSRS login fixture: RSA private-key oracle, XTEA body, high-bit nonce/seed, CRC layout and truncated reply passed")
