#!/usr/bin/env python3
"""Install a file into an Agel disk image's asset region.

    install-asset.py IMAGE NAME FILE      add or replace NAME
    install-asset.py IMAGE --list         print the table

The asset region is sectors 3072 through 6143, laid out like the program
region (a table sector "AGELAS1", then the files) and read by the graphics
supervisor at boot, which maps each asset read-only into the compositor.
"""

import runpy
import sys
from pathlib import Path

if __name__ == "__main__":
    sys.argv = [sys.argv[0]] + sys.argv[1:] + ["--region", "assets"]
    runpy.run_path(str(Path(__file__).with_name("install-program.py")), run_name="__main__")
