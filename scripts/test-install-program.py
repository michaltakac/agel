#!/usr/bin/env python3
"""Host installer regressions; no emulator or built kernel is needed."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("installer", Path(__file__).with_name("install-program.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.image = Path(self.directory.name) / "disk.img"
        # A small region exercises exactly the same layout and bounds.
        installer.TABLE, installer.LAST, installer.MAGIC = 1, 5, b"AGELPR1\0"
        self.image.write_bytes(b"\0" * (7 * installer.SECTOR))

    def test_full_replacement_preserves_every_byte(self):
        installer.install(self.image, "first", b"A" * 512)
        installer.install(self.image, "second", b"B" * 512)
        before = self.image.read_bytes()
        with self.assertRaisesRegex(ValueError, "full"):
            installer.install(self.image, "first", b"C" * 2048)
        self.assertEqual(self.image.read_bytes(), before)

    def test_replacement_compacts_and_preserves_other_entries(self):
        installer.install(self.image, "first", b"A" * 512)
        installer.install(self.image, "second", b"B" * 512)
        installer.install(self.image, "first", b"C" * 1024)
        rows = installer.read_table(self.image)
        self.assertEqual([row["name"] for row in rows], ["second", "first"])
        disk = self.image.read_bytes()
        for row, expected in zip(rows, [b"B" * 512, b"C" * 1024]):
            start = row["start"] * installer.SECTOR
            self.assertEqual(disk[start:start + row["length"]], expected)

    def test_invalid_entries_are_refused_before_writing(self):
        for name, data in [("a\0b", b"x"), ("a b", b"x"), ("x", b"")]:
            with self.subTest(name=name, data=data):
                before = self.image.read_bytes()
                with self.assertRaises(ValueError):
                    installer.install(self.image, name, data)
                self.assertEqual(self.image.read_bytes(), before)

    def test_corrupt_table_is_refused_before_repacking(self):
        installer.install(self.image, "first", b"A" * 512)
        rows = installer.read_table(self.image)
        rows[0]["start"] = 0
        installer.write_table(self.image, rows)
        before = self.image.read_bytes()
        with self.assertRaisesRegex(ValueError, "outside"):
            installer.install(self.image, "second", b"B")
        self.assertEqual(self.image.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
