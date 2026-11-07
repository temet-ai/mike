#!/usr/bin/env python3
"""The shared target table: every host resolves to its own target, and Linux ARM64 is treated as Linux."""

from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import archive
import common
from common import ReleaseError, TARGETS


class TargetTableTests(unittest.TestCase):
    def native(self, system, machine):
        answers = {"-s": system, "-m": machine}
        with patch("common.run", side_effect=lambda args, **_: answers[args[1]]):
            return common.native_target(Path("."), {})

    def test_each_host_resolves_to_its_target(self):
        for target, host in TARGETS.items():
            self.assertEqual(target, self.native(*host[:2]))

    def test_linux_arm64_and_apple_silicon_are_different_hosts(self):
        self.assertEqual("aarch64-unknown-linux-gnu", self.native("Linux", "aarch64"))
        self.assertEqual("aarch64-apple-darwin", self.native("Darwin", "arm64"))

    def test_a_host_outside_the_table_refuses(self):
        for host in (("Darwin", "x86_64"), ("Linux", "armv7l"), ("FreeBSD", "aarch64")):
            with self.assertRaisesRegex(ReleaseError, "unsupported native host"):
                self.native(*host)

    def test_linux_arm64_binaries_are_inspected_with_readelf(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "mike"
            with patch("archive.run", return_value="output") as run:
                result = archive.runtime_requirements(binary, "aarch64-unknown-linux-gnu", {})
        self.assertEqual("readelf (no ldd execution)", result["method"])
        self.assertEqual({"readelf"}, {call.args[0][0] for call in run.call_args_list})


if __name__ == "__main__":
    unittest.main()
