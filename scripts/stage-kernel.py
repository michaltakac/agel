#!/usr/bin/env python3
"""Stage a candidate kernel into the slot the boot stage is not trusting.

The x86-64 image has two kernel slots: A at sectors 1-254 and B at sectors
290-543. Sector 289 is the selector the 512-byte BIOS stage reads before it
loads anything: which slot is trusted, which is a candidate, how many boots
the candidate has been given, and whether one of them was healthy. Staging
writes the kernel into the slot that is not trusted and proposes it with a
fresh budget; the stage charges each boot, and after three boots without the
kernel reaching a healthy state it loads the trusted slot instead.

    stage-kernel.py IMAGE KERNEL_BIN [--key KEY | --signature HEX | --unsigned]
    stage-kernel.py IMAGE --status

A candidate is not loaded until a running kernel has admitted it: the kernel
hashes the slot with SHA-512 and verifies an Ed25519 signature over that
digest against the public key it was built with (bootstrap/kernel-signing.pub).
By default the signature is made with bootstrap/kernel-signing.key through
`cargo run -p agel-integrity --example kernel-sign`; `--unsigned` stages a
zero signature, which the kernel refuses.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_KEY = os.path.join(ROOT, "bootstrap", "kernel-signing.key")

SECTOR = 512
SELECTOR_SECTOR = 289
SLOT_SECTORS = 254
SLOT_BASE = {0: 1, 1: 290}
NO_CANDIDATE = 0xFF
MAGIC = b"AGKS"
VERSION = 2
BUDGET = 3


def read_selector(image: str) -> dict[str, int]:
    with open(image, "rb") as disk:
        disk.seek(SELECTOR_SECTOR * SECTOR)
        sector = disk.read(SECTOR)
    if sector[:4] != MAGIC or sector[4] != VERSION:
        return dict(EMPTY)
    return {
        "trusted": sector[5],
        "candidate": sector[6],
        "attempts": sector[7],
        "verified": sector[8],
        "admitted": sector[9],
        "length": int.from_bytes(sector[12:16], "little"),
        "signature": bytes(sector[64:128]),
    }


EMPTY = {
    "trusted": 0,
    "candidate": NO_CANDIDATE,
    "attempts": 0,
    "verified": 0,
    "admitted": 0,
    "length": 0,
    "signature": bytes(64),
}


def write_selector(image: str, selector: dict[str, int]) -> None:
    sector = bytearray(SECTOR)
    sector[:4] = MAGIC
    sector[4] = VERSION
    sector[5] = selector["trusted"]
    sector[6] = selector["candidate"]
    sector[7] = selector["attempts"]
    sector[8] = selector["verified"]
    sector[9] = selector["admitted"]
    sector[12:16] = selector["length"].to_bytes(4, "little")
    sector[64:128] = selector["signature"]
    with open(image, "r+b") as disk:
        disk.seek(SELECTOR_SECTOR * SECTOR)
        disk.write(sector)
        disk.flush()


def sign(kernel: bytes, key: str = DEFAULT_KEY) -> bytes:
    """Ed25519 over sha512(kernel), made by the hosted implementation."""
    with tempfile.NamedTemporaryFile(prefix="agel-kernel-", suffix=".bin") as file:
        file.write(kernel)
        file.flush()
        output = subprocess.run(
            ["cargo", "run", "-q", "-p", "agel-integrity", "--example", "kernel-sign", "--", "sign", key, file.name],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    signature = bytes.fromhex(output.strip())
    if len(signature) != 64:
        raise ValueError("kernel-sign did not print a 64-byte signature")
    return signature


def stage(image: str, kernel: bytes, signature: bytes) -> int:
    if len(kernel) > SLOT_SECTORS * SECTOR:
        raise ValueError(f"kernel is {len(kernel)} bytes; a slot holds {SLOT_SECTORS * SECTOR}")
    if len(signature) != 64:
        raise ValueError("a signature is 64 bytes")
    selector = read_selector(image)
    slot = 1 - selector["trusted"]
    with open(image, "r+b") as disk:
        disk.seek(SLOT_BASE[slot] * SECTOR)
        disk.write(kernel.ljust(SLOT_SECTORS * SECTOR, b"\0"))
        disk.flush()
    write_selector(
        image,
        {
            "trusted": selector["trusted"],
            "candidate": slot,
            "attempts": 0,
            "verified": 0,
            "admitted": 0,
            "length": len(kernel),
            "signature": signature,
        },
    )
    return slot


def slot_name(slot: int) -> str:
    return "none" if slot == NO_CANDIDATE else "AB"[slot]


def describe(selector: dict[str, int]) -> str:
    text = f"trusted slot {slot_name(selector['trusted'])}"
    if selector["candidate"] == NO_CANDIDATE:
        return text + "; no candidate"
    if not selector["admitted"]:
        return f"{text}; candidate slot {slot_name(selector['candidate'])} (staged, not admitted)"
    state = "verified" if selector["verified"] else "unverified"
    return (
        f"{text}; candidate slot {slot_name(selector['candidate'])}"
        f" ({state}, boots {selector['attempts']})"
    )


def main() -> int:
    arguments = sys.argv[1:]
    if len(arguments) == 2 and arguments[1] == "--status":
        print(describe(read_selector(arguments[0])))
        return 0
    if len(arguments) not in (2, 3, 4):
        print(__doc__, file=sys.stderr)
        return 2
    image, kernel_path, *options = arguments
    with open(kernel_path, "rb") as file:
        kernel = file.read()
    if options == ["--unsigned"]:
        signature = bytes(64)
    elif len(options) == 2 and options[0] == "--signature":
        signature = bytes.fromhex(options[1])
    elif len(options) == 2 and options[0] == "--key":
        signature = sign(kernel, options[1])
    elif not options:
        signature = sign(kernel)
    else:
        print(__doc__, file=sys.stderr)
        return 2
    slot = stage(image, kernel, signature)
    print(f"staged {kernel_path} as candidate kernel slot {slot_name(slot)}; {describe(read_selector(image))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
