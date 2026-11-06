#!/usr/bin/env python3
"""Independent bit-field fixtures for the pinned revision's NPC info layout."""
import random
import sys
sys.path.insert(0, "scripts/osrs")
from test_support import evaluate, form


def packet(fields):
    text = ''.join(format(value, '0' + str(width) + 'b') for width, value in fields if width)
    text += '0' * (-len(text) % 8)
    return int(text or '0', 2).to_bytes(len(text) // 8, 'big')


def spawn(index, dx, dz, width, type_id, type_code=0, clock_code=None):
    fields = [(16, index), (width, dz % (1 << width)), (1, 1), (3, 6), (1, int(clock_code is not None))]
    if clock_code is not None:
        size = [18, 19, 20, 32][clock_code]
        fields += [(2, clock_code), (size, (1 << size) - 1)]
    fields += [(1, 1), (2, type_code), ([12, 14, 17, 24][type_code], type_id), (width, dx % (1 << width))]
    return fields


rng = random.Random(241)
checks, expected = [], []
for _ in range(120):
    data = rng.randbytes(8)
    offset = rng.randrange(33)
    width = rng.randrange(33)
    source = "(get (bits-read (get (bits-read (bits-new '" + form(data) + ") " + str(offset) + ") 'reader) " + str(width) + ") 'value)"
    checks.append(source)
    expected.append((int.from_bytes(data, 'big') >> (64 - offset - width)) & ((1 << width) - 1))
result = evaluate('(import agel/osrs-bits) (list ' + ' '.join(checks) + ')')
assert result.returncode == 0 and result.stdout.strip() == form(expected), result.stderr

first = packet([(8, 0)] + spawn(17, -32, 31, 6, 2854, clock_code=3) + [(16, 65535)]) + b'\xaa\xbb'
second = packet([(8, 1), (1, 1), (2, 2), (1, 1), (3, 2), (3, 5), (1, 1)] +
                spawn(18, 3, -2, 6, 200000, type_code=3, clock_code=0) + [(16, 65535)])
third = packet([(8, 2), (1, 1), (2, 3), (1, 0), (16, 65535)])
large = packet([(8, 0)] + spawn(65534, 127, -128, 8, 16000, type_code=1, clock_code=2) + [(16, 65535)])
medium = packet([(8, 0)] + spawn(123, 7, -2, 6, 70000, type_code=2, clock_code=1) + [(16, 65535)])
source = "(import agel/osrs-npcs) (def a (npcs-decode (npcs-new) '" + form(first) + " #f)) "
source += "(def b (npcs-decode a '" + form(second) + " #f)) "
source += "(def c (npcs-decode b '" + form(third) + " #f)) "
source += "(def d (npcs-decode c '(0) #f)) "
source += "(def e (npcs-decode (npcs-new) '" + form(large) + " #t)) "
source += "(def f (npcs-decode (npcs-new) '" + form(medium) + " #f)) "
source += "(list (get a 'order) (get (get (get a 'npcs) 17) 'type) "
source += "(get (get (get a 'npcs) 17) 'spawn-dx) (get (get (get a 'npcs) 17) 'spawn-dz) "
source += "(get b 'order) (get (get (get b 'npcs) 18) 'type) (get c 'order) (get d 'order) "
source += "(get (get (get e 'npcs) 65534) 'spawn-dx) (get (get (get e 'npcs) 65534) 'spawn-dz) "
source += "(get (get (get f 'npcs) 123) 'type) (get (get (get b 'npcs) 17) 'movement))"
result = evaluate(source)
assert result.returncode == 0, result.stderr
assert result.stdout.strip() == '((17) 2854 -32 31 (17 18) 200000 (18) nil 127 -128 70000 {kind 2 run 1 code 21})', result.stdout
result = evaluate("(import agel/osrs-npcs) (npcs-decode (npcs-new) '(1) #f)")
assert result.returncode != 0 and 'osrs/npc' in result.stderr, result.stderr
for cut in range(2, len(first) - 3):
    result = evaluate("(import agel/osrs-npcs) (npcs-decode (npcs-new) '" + form(first[:cut]) + " #f)")
    assert result.returncode != 0 and 'osrs/bits' in result.stderr, (cut, result.stdout, result.stderr)
print('OSRS NPCs: bit offsets/widths, spawn variants, clocks, retained movement, removal, truncation and distance signs passed')
