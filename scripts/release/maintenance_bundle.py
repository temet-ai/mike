"""Build the bounded, deterministic optional maintenance payload; no dependencies."""

import io
from pathlib import Path
import zipfile

from common import regular_file

# This is an explicit dependency closure, never a recursive source-tree copy.
MODULES = ("mod_assets", "archive", "common", "migration_inventory", "notice_inputs", "notice_inventory", "inputs", "install", "install_state", "owned_process", "recovery",
           "recovery_database", "recovery_files", "recovery_tools")
LAUNCHER = b'''#!/bin/bash
# Optional maintenance only. Core mike and mike-mcp do not need Python or Bash.
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
exec python3 -I -B "${SCRIPT_DIR}/mike-maintenance.pyz" "$@"
'''
MAIN = b'''import runpy
import sys

if sys.version_info < (3, 11):
    sys.exit("mike-maintenance requires optional Python 3.11+; core binaries do not")
if not sys.flags.isolated:
    sys.exit("Run mike-maintenance or python3 -I -B mike-maintenance.pyz")
sys.dont_write_bytecode = True
if len(sys.argv) > 1 and sys.argv[1] == "--_owned-process":
    from owned_process import supervise
    sys.exit(supervise(sys.argv[2:]))
module = "recovery"
if len(sys.argv) > 1 and sys.argv[1] == "--_candidate-inputs":
    del sys.argv[1]
    module = "inputs"
runpy.run_module(module, run_name="__main__")
'''


def payload():
    source = Path(__file__).resolve().parent
    entries = [("__main__.py", MAIN)]
    for name in MODULES:
        path = source / (name + ".py")
        regular_file(path)
        entries.append((path.name, path.read_bytes()))
    buffer = io.BytesIO()
    # Fixed timestamps, modes, order and uncompressed storage make bytes stable
    # across hosts and zlib versions. The outer candidate tarball is compressed.
    with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_STORED) as bundle:
        for name, data in entries:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            bundle.writestr(info, data)
    return [("mike-maintenance", LAUNCHER, 0o755),
            ("mike-maintenance.pyz", buffer.getvalue(), 0o644)]
