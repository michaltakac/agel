#!/usr/bin/env python3
"""Exercise host-composed text against the real graphical Agel evaluator."""
import importlib.util
import sys
import tempfile

from pathlib import Path

spec = importlib.util.spec_from_file_location("graphical_console", Path(__file__).with_name("graphical-console.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

with tempfile.TemporaryDirectory(prefix="agel-input-test-", dir="/tmp") as directory:
    machine = module.Machine(sys.argv[1], directory, snapshot=True)
    try:
        assert "AGEL_GRAPHICS_OK" in machine.boot
        assert machine.frame().startswith(b"\x89PNG\r\n\x1a\n")
        for source, result in [
            ("(+ 20 22)", "42"),
            ("(def žluťučký 42)", "42"),
            ("žluťučký", "42"),
            ("(eval '(+ 19 23))", "42"),
            ("(def A_B 9)", "9"),
            ("A_B", "9"),
            ("(def ≤ 7)", "7"),
            ("≤", "7"),
            ('(+ 1 1) ; \'¨<>–[]^°}{\\*&^~$#@`≤?:_"!)(/ˇ%', "2"),
        ]:
            response = machine.submit(source)
            assert f"\r\n{result}\r\n" in response, (source, response)
        for source in ["x" * 257, "ž" * 129, "(+ 1 1)\n(+ 2 2)", "\x1b", ":shutdown"]:
            try:
                machine.submit(source)
                raise AssertionError("Invalid input accepted")
            except ValueError:
                pass
        assert "42" in machine.submit("(+ 20 22)")
        assert machine.frame().startswith(b"\x89PNG\r\n\x1a\n")
        print("Agel graphical console: Unicode, punctuation, limits, real VM evaluation and frame [ok]")
    finally:
        machine.close()
