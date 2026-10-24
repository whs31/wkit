#!/usr/bin/env python3
"""Checks that the MMS plugin metadata has the fields MMS requires."""

import re
import sys
from pathlib import Path

META = Path(__file__).resolve().parent.parent / "src" / "plugin" / "assets" / "meta.lua"
REQUIRED = ("uuid", "name", "version", "dependencies")

text = META.read_text()
missing = [field for field in REQUIRED if not re.search(rf"^\s*{field}\s*=", text, re.MULTILINE)]
if missing:
  print(f"{META}: missing {', '.join(missing)}", file=sys.stderr)
  sys.exit(1)
