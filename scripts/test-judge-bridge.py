"""Compatibility entry point for the framed judge ledger transport test."""
import runpy
from pathlib import Path

runpy.run_path(str(Path(__file__).with_name("test-ledger.py")), run_name="__main__")
