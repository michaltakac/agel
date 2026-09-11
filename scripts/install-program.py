#!/usr/bin/env python3
"""Install a static ELF into an Agel disk image's program region.

    install-program.py IMAGE NAME ELF      add or replace NAME
    install-program.py IMAGE --list        print the table

The program region is sectors 2048 through 3071: a table sector ("AGELPR1",
a count, then 32-byte rows of name, start sector, length and CRC-32) followed
by the images. The supervisor's `:exec NAME` reads the table, checks the
CRC, parses the ELF and loads it into a fresh protection domain.
"""

from __future__ import annotations

import sys
import zlib

SECTOR = 512
TABLE = 2048
LAST = 3071
MAGIC = b"AGELPR1\0"
NAME_BYTES = 16
ROW = 32
MAX_ROWS = (SECTOR - 16) // ROW


def read_table(image: str) -> list[dict]:
    with open(image, "rb") as disk:
        disk.seek(TABLE * SECTOR)
        sector = disk.read(SECTOR)
    if sector[:8] != MAGIC:
        return []
    count = min(int.from_bytes(sector[8:12], "little"), MAX_ROWS)
    rows = []
    for index in range(count):
        row = sector[16 + index * ROW : 16 + (index + 1) * ROW]
        name = row[:NAME_BYTES].split(b"\0", 1)[0].decode("ascii")
        rows.append(
            {
                "name": name,
                "start": int.from_bytes(row[16:20], "little"),
                "length": int.from_bytes(row[20:24], "little"),
                "crc": int.from_bytes(row[24:28], "little"),
            }
        )
    return rows


def write_table(image: str, rows: list[dict]) -> None:
    sector = bytearray(SECTOR)
    sector[:8] = MAGIC
    sector[8:12] = len(rows).to_bytes(4, "little")
    for index, row in enumerate(rows):
        base = 16 + index * ROW
        sector[base : base + NAME_BYTES] = row["name"].encode("ascii").ljust(NAME_BYTES, b"\0")
        sector[base + 16 : base + 20] = row["start"].to_bytes(4, "little")
        sector[base + 20 : base + 24] = row["length"].to_bytes(4, "little")
        sector[base + 24 : base + 28] = row["crc"].to_bytes(4, "little")
    with open(image, "r+b") as disk:
        disk.seek(TABLE * SECTOR)
        disk.write(sector)
        disk.flush()


def install(image: str, name: str, elf: bytes) -> dict:
    if not name or len(name) > NAME_BYTES or not name.isascii():
        raise ValueError("a program name is 1 to 16 ASCII bytes")
    rows = [row for row in read_table(image) if row["name"] != name]
    if len(rows) >= MAX_ROWS:
        raise ValueError("the program table is full")
    next_free = TABLE + 1
    for row in rows:
        next_free = max(next_free, row["start"] + -(-row["length"] // SECTOR))
    sectors = -(-len(elf) // SECTOR)
    if next_free + sectors - 1 > LAST:
        raise ValueError("the program region is full")
    with open(image, "r+b") as disk:
        disk.seek(next_free * SECTOR)
        disk.write(elf.ljust(sectors * SECTOR, b"\0"))
        disk.flush()
    row = {"name": name, "start": next_free, "length": len(elf), "crc": zlib.crc32(elf) & 0xFFFFFFFF}
    rows.append(row)
    write_table(image, rows)
    return row


def main() -> int:
    if len(sys.argv) == 3 and sys.argv[2] == "--list":
        for row in read_table(sys.argv[1]):
            print(f"{row['name']:16} sectors {row['start']}.. {row['length']} bytes crc {row['crc']:08x}")
        return 0
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    image, name, path = sys.argv[1:]
    with open(path, "rb") as file:
        elf = file.read()
    row = install(image, name, elf)
    print(f"installed {name}: {row['length']} bytes at sector {row['start']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
