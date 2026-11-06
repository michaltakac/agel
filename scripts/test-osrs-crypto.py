#!/usr/bin/env python3
import random
import sys
import json
from pathlib import Path
sys.path.insert(0, "scripts/osrs")
from test_support import evaluate, form, xtea

rng = random.Random(241)
modulus = rng.getrandbits(1024) | 1 | (1 << 1023)
messages = [0, 1, modulus - 1, rng.getrandbits(376)]
source = '(import agel/osrs-crypto) (def c (rsa-context "' + format(modulus, 'x') + '")) (list '
expected = []
for value in messages:
    message = value.to_bytes(max(1, (value.bit_length() + 7) // 8), "big")
    source += "(rsa-encrypt '" + form(message) + " c) "
    result = pow(value, 65537, modulus)
    encoded = result.to_bytes(max(1, (result.bit_length() + 7) // 8), "big")
    if encoded[0] >= 128:
        encoded = b"\0" + encoded
    expected.append(encoded)
for size in [0, 7, 8, 9, 24, 31]:
    data = rng.randbytes(size)
    key = [rng.getrandbits(32) for _ in range(4)]
    source += "(xtea-encrypt '" + form(data) + " '" + form(key) + ") "
    expected.append(xtea(data, key))
source += "(xtea-encrypt '(0 0 0 0 0 0 0 0) '(0 0 0 0)))"
expected.append(bytes.fromhex("dee9d4d8f7131ed9"))
result = evaluate(source)
assert result.returncode == 0, result.stderr
assert result.stdout.strip() == '(' + ' '.join(form(v) if v else 'nil' for v in expected) + ')', "RSA/XTEA oracle mismatch"
for modulus in ["00", "01", "0001", "04", "ffg1"]:
    result = evaluate('(import agel/osrs-crypto) (rsa-context "' + modulus + '")')
    assert result.returncode != 0 and 'osrs/rsa' in result.stderr, result.stderr
print("OSRS crypto: 1024-bit RSA boundaries/Python pow, XTEA vectors/tails and invalid moduli passed")
vectors = json.loads(Path('crates/agel-stdlib/tests/osrs-isaac-vectors.json').read_text())
for vector in vectors['vectors']:
    source = "(import agel/osrs-crypto) (import agel/sequence) "
    source += "(def draw (fn (state n out) (if (= n 0) (reverse out) "
    source += "(let ((next (isaac-next state))) (draw (get next 'state) (- n 1) (cons (get next 'value) out)))))) "
    source += "(draw (isaac-new '" + form(vector['seed']) + ") 600 nil)"
    result = evaluate(source)
    assert result.returncode == 0 and result.stdout.strip() == form(vector['draws']), result.stderr
print("OSRS cipher: two 600-draw ISAAC sequences match the pinned server reference, including refill")
