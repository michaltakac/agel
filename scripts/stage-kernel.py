#!/usr/bin/env python3
"""Stage a candidate kernel into the slot the boot stage is not trusting.

The x86-64 image has two kernel slots of 4096 sectors (2 MiB): A at sector
65536 and B at 69760, past the data region (disk layout v3, v0.2.100). Sector 1057 is the selector the 512-byte BIOS stage reads before it
loads anything: which slot is trusted, which is a candidate, how many boots
the candidate has been given, and whether one of them was healthy. Staging
writes the kernel into the slot that is not trusted and proposes it with a
fresh budget; the stage charges each boot, and after three boots without the
kernel reaching a healthy state it loads the trusted slot instead.

    stage-kernel.py IMAGE KERNEL_BIN [--key KEY | --signature HEX | --unsigned]
    stage-kernel.py IMAGE --restore VERSION
    stage-kernel.py IMAGE --status

Staging from the command line retains the exact kernel and signature in
IMAGE.kernel-history. A restored version goes through normal admission.

A candidate is not loaded until a running kernel has admitted it: the kernel
hashes the slot with SHA-512 and verifies an Ed25519 signature over that
digest against the public key it was built with (bootstrap/kernel-signing.pub).
By default the signature is made with bootstrap/kernel-signing.key through
`cargo run -p agel-integrity --example kernel-sign`; `--unsigned` stages a
zero signature, which the kernel refuses.
"""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_KEY = os.path.join(ROOT, "bootstrap", "kernel-signing.key")

SECTOR = 512
SELECTOR_SECTOR = 1057
SLOT_SECTORS = 4096
SLOT_BASE = {0: 65536, 1: 69760}
NO_CANDIDATE = 0xFF
MAGIC = b"AGKS"
VERSION = 2
BUDGET = 3


def read_selector(image: str) -> dict[str, int | bytes]:
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
        "trial_digest": bytes(sector[128:192]),
    }


EMPTY = {
    "trusted": 0,
    "candidate": NO_CANDIDATE,
    "attempts": 0,
    "verified": 0,
    "admitted": 0,
    "length": 0,
    "signature": bytes(64),
    "trial_digest": bytes(64),
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
        os.fsync(disk.fileno())


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
    trusted = selector["trusted"]
    slot = 1 - trusted
    if selector["candidate"] != NO_CANDIDATE:
        # A reboot during replacement must load the trusted slot.
        write_selector(image, {**EMPTY, "trusted": trusted})
    with open(image, "r+b") as disk:
        disk.seek(SLOT_BASE[slot] * SECTOR)
        disk.write(kernel.ljust(SLOT_SECTORS * SECTOR, b"\0"))
        disk.flush()
        os.fsync(disk.fileno())
    write_selector(
        image,
        {
            "trusted": trusted,
            "candidate": slot,
            "attempts": 0,
            "verified": 0,
            "admitted": 0,
            "length": len(kernel),
            "signature": signature,
        },
    )
    return slot


def archived_candidate(image: str, version: str) -> tuple[bytes, bytes]:
    if len(version) != 128 or any(char not in "0123456789abcdef" for char in version):
        raise ValueError("expected a lowercase SHA-512 kernel history version")
    path = Path(image + ".kernel-history") / version
    with path.open("rb") as file:
        record = file.read(65 + SLOT_SECTORS * SECTOR)
    if not 64 < len(record) <= 64 + SLOT_SECTORS * SECTOR or hashlib.sha512(record).hexdigest() != version:
        raise ValueError("kernel history record does not match its version")
    return record[64:], record[:64]


def retain_candidate(image: str, kernel: bytes, signature: bytes) -> str:
    if not 0 < len(kernel) <= SLOT_SECTORS * SECTOR or len(signature) != 64:
        raise ValueError("invalid kernel or signature length")
    record = signature + kernel
    version = hashlib.sha512(record).hexdigest()
    history = Path(image + ".kernel-history")
    history.mkdir(mode=0o700, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=history) as file:
        file.write(record)
        file.flush()
        os.fsync(file.fileno())
        try:
            os.link(file.name, history / version)
        except FileExistsError:
            archived_candidate(image, version)
    for path in (history, history.parent):
        directory = os.open(path, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    return version


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
    if len(arguments) == 3 and arguments[1] == "--restore":
        image, _, version = arguments
        kernel, signature = archived_candidate(image, version)
        slot = stage(image, kernel, signature)
        print(f"restaged kernel version {version} as candidate slot {slot_name(slot)}")
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
    version = retain_candidate(image, kernel, signature)
    slot = stage(image, kernel, signature)
    print(f"staged {kernel_path} as candidate kernel slot {slot_name(slot)}; {describe(read_selector(image))}")
    print(f"retained kernel version {version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
