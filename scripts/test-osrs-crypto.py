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
for bits in [30, 31, 32, 61, 62, 63, 95, 96]:
    modulus = rng.getrandbits(bits) | 1 | (1 << (bits - 1))
    encoded_modulus = modulus.to_bytes((bits + 7) // 8, "big").hex()
    source = '(import agel/crypto) (def c (rsa-context "' + encoded_modulus + '")) (list '
    expected = []
    for value in [0, 1, modulus - 1, rng.randrange(modulus)]:
        message = value.to_bytes(max(1, (value.bit_length() + 7) // 8), "big")
        source += "(rsa-encrypt '" + form(message) + " c) "
        encrypted = pow(value, 65537, modulus)
        encoded = encrypted.to_bytes(max(1, (encrypted.bit_length() + 7) // 8), "big")
        expected.append(b"\0" + encoded if encoded[0] >= 128 else encoded)
    result = evaluate(source + ')')
    assert result.returncode == 0 and result.stdout.strip() == '(' + ' '.join(form(v) for v in expected) + ')', result.stderr
print("General crypto: 31-bit RSA limb boundaries match Python pow")
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
for vector in vectors['vectors']:
    for capacity in [1, 7, 256, 512]:
        source = "(import agel/crypto) (import agel/sequence) "
        source += "(def draw (fn (state n out) (if (= n 0) (reverse out) "
        source += "(let ((next (isaac-bytes-next state))) (draw (get next 'state) (- n 1) (cons (get next 'value) out)))))) "
        source += "(draw (isaac-bytes-new '" + form(vector['seed']) + " " + str(capacity) + ") 600 nil)"
        result = evaluate(source)
        assert result.returncode == 0 and result.stdout.strip() == form([n % 256 for n in vector['draws']]), result.stderr
for capacity in [0, 513]:
    result = evaluate("(import agel/crypto) (isaac-bytes-new '(1 2 3 4) " + str(capacity) + ")")
    assert result.returncode != 0 and 'crypto/isaac' in result.stderr, result.stderr
print("General ISAAC byte buffers: capacities 1/7/256/512 match 600 pinned oracle bytes for both seeds, including refill")
vector = vectors['vectors'][0]
source = '''(import agel/sequence) (import agel/osrs-crypto) (import agel/osrs-game)
(def packets nil)
(def wire-write (fn (bytes) (begin (def packets (append packets (list bytes))) (count bytes))))
(def service nil) (def progress nil)
(def maintenance (dict
  'start (fn (function initial) (begin (def service function) (def progress initial) #t))
  'stop (fn () progress)))
(def pulse-service (fn () (let ((result (service progress)))
  (begin (wire-write (get result 'bytes)) (def progress (get result 'state)) #t))))
'''
source += "(def state (assoc (osrs-game-state '" + form(vector['seed']) + ") 'maintenance maintenance)) "
source += '''(osrs-service-start state) (pulse-service) (pulse-service)
(def state (osrs-service-stop state)) (def state (osrs-walk state 3223 3218))
(osrs-service-start state) (pulse-service) (pulse-service)
(def state (osrs-service-stop state))
(list (map car packets) (get state 'keepalives) (isaac-bytes-available (get state 'out)))'''
result = evaluate(source)
expected = [[(word + opcode) % 256 for word, opcode in zip(vector['draws'], [75, 75, 55, 75, 75])], 4, 507]
assert result.returncode == 0 and result.stdout.strip() == form(expected), result.stderr + result.stdout
print("OSRS adapter: repeated service start/stop and walking preserve the independent cipher sequence")
