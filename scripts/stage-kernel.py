#!/usr/bin/env python3
"""Stage a candidate kernel into the slot the boot stage is not trusting.

The x86-64 image has two kernel slots: A at sectors 1-254 and B at sectors
290-543. Sector 289 is the selector the 512-byte BIOS stage reads before it
loads anything: which slot is trusted, which is a candidate, how many boots
the candidate has been given, and whether one of them was healthy. Staging
writes the kernel into the slot that is not trusted and proposes it with a
fresh budget; the stage charges each boot, and after three boots without the
kernel reaching a healthy state it loads the trusted slot instead.

    stage-kernel.py IMAGE KERNEL_BIN   stage KERNEL_BIN as the candidate
    stage-kernel.py IMAGE --status     print the selector
"""

from __future__ import annotations

import sys

SECTOR = 512
SELECTOR_SECTOR = 289
SLOT_SECTORS = 254
SLOT_BASE = {0: 1, 1: 290}
NO_CANDIDATE = 0xFF
MAGIC = b"AGKS"
VERSION = 1
BUDGET = 3


def read_selector(image: str) -> dict[str, int]:
    with open(image, "rb") as disk:
        disk.seek(SELECTOR_SECTOR * SECTOR)
        sector = disk.read(SECTOR)
    if sector[:4] != MAGIC or sector[4] != VERSION:
        return {"trusted": 0, "candidate": NO_CANDIDATE, "attempts": 0, "verified": 0}
    return {
        "trusted": sector[5],
        "candidate": sector[6],
        "attempts": sector[7],
        "verified": sector[8],
    }


def write_selector(image: str, selector: dict[str, int]) -> None:
    sector = bytearray(SECTOR)
    sector[:4] = MAGIC
    sector[4] = VERSION
    sector[5] = selector["trusted"]
    sector[6] = selector["candidate"]
    sector[7] = selector["attempts"]
    sector[8] = selector["verified"]
    with open(image, "r+b") as disk:
        disk.seek(SELECTOR_SECTOR * SECTOR)
        disk.write(sector)
        disk.flush()


def stage(image: str, kernel: bytes) -> int:
    if len(kernel) > SLOT_SECTORS * SECTOR:
        raise ValueError(f"kernel is {len(kernel)} bytes; a slot holds {SLOT_SECTORS * SECTOR}")
    selector = read_selector(image)
    slot = 1 - selector["trusted"]
    with open(image, "r+b") as disk:
        disk.seek(SLOT_BASE[slot] * SECTOR)
        disk.write(kernel.ljust(SLOT_SECTORS * SECTOR, b"\0"))
        disk.flush()
    write_selector(
        image,
        {"trusted": selector["trusted"], "candidate": slot, "attempts": 0, "verified": 0},
    )
    return slot


def slot_name(slot: int) -> str:
    return "none" if slot == NO_CANDIDATE else "AB"[slot]


def describe(selector: dict[str, int]) -> str:
    text = f"trusted slot {slot_name(selector['trusted'])}"
    if selector["candidate"] == NO_CANDIDATE:
        return text + "; no candidate"
    state = "verified" if selector["verified"] else "unverified"
    return (
        f"{text}; candidate slot {slot_name(selector['candidate'])}"
        f" ({state}, boots {selector['attempts']})"
    )


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    image, argument = sys.argv[1:]
    if argument == "--status":
        print(describe(read_selector(image)))
        return 0
    with open(argument, "rb") as file:
        kernel = file.read()
    slot = stage(image, kernel)
    print(f"staged {argument} as candidate kernel slot {slot_name(slot)}; {describe(read_selector(image))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
