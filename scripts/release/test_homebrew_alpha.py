#!/usr/bin/env python3
"""Synthetic checksums only: the alpha tap formulas and the unchanged gated proposal."""

import contextlib
import io
from pathlib import Path
import tempfile
import unittest

import homebrew_alpha
from common import ReleaseError
from homebrew_proposal import DESCRIPTIONS, formula_methods

VERSION = "0.1.0-alpha.1"
REPOSITORY = "temet-ai/mike"
ARCHIVES = [f"{name}-v{VERSION}-{target}.tar.gz" for name in ("mike", "mike-mcp")
            for target in ("aarch64-apple-darwin", "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu")]


def checksums(names=ARCHIVES):
    return "".join(f"{format(index + 1, '064x')}  {name}\n" for index, name in enumerate(names))


class HomebrewAlphaTests(unittest.TestCase):
    def render(self, name, text=None):
        return homebrew_alpha.render(name, VERSION, REPOSITORY, homebrew_alpha.parse_checksums(text or checksums()))

    def test_formula_pins_each_platform_archive_by_its_own_digest(self):
        formula = self.render("mike")
        base = f"https://github.com/{REPOSITORY}/releases/download/v{VERSION}/"
        mac = formula.index("on_macos do")
        linux = formula.index("on_linux do")
        self.assertLess(mac, linux)
        self.assertIn(f'url "{base}mike-v{VERSION}-aarch64-apple-darwin.tar.gz"', formula[mac:linux])
        self.assertIn(f'sha256 "{1:064x}"', formula[mac:linux])
        self.assertIn("depends_on arch: :arm64", formula[mac:linux])
        arm = formula.index("on_arm do", linux)
        intel = formula.index("on_intel do", arm)
        self.assertIn(f'url "{base}mike-v{VERSION}-aarch64-unknown-linux-gnu.tar.gz"', formula[arm:intel])
        self.assertIn(f'sha256 "{3:064x}"', formula[arm:intel])
        self.assertIn(f'url "{base}mike-v{VERSION}-x86_64-unknown-linux-gnu.tar.gz"', formula[intel:])
        self.assertIn(f'sha256 "{2:064x}"', formula[intel:])
        self.assertEqual(1, formula.count("on_linux do"))

    def test_mcp_formula_takes_the_mcp_archives(self):
        formula = self.render("mike-mcp")
        self.assertIn("class MikeMcp < Formula", formula)
        for index in (4, 5, 6):
            self.assertIn(f'sha256 "{index:064x}"', formula)
        self.assertNotIn("/mike-v", formula)

    def test_install_and_test_methods_are_the_ones_the_proposal_uses(self):
        for name in DESCRIPTIONS:
            formula = self.render(name)
            self.assertIn("\n".join(formula_methods(name, VERSION)), formula)
            self.assertTrue(formula.endswith("  end\nend\n"))
        self.assertIn('libexec.install "mike-maintenance", "mike-maintenance.pyz"', self.render("mike"))
        self.assertNotIn("libexec", self.render("mike-mcp"))

    def test_a_missing_archive_digest_refuses(self):
        for missing in ARCHIVES:
            name = "mike" if missing.startswith("mike-v") else "mike-mcp"
            with self.assertRaisesRegex(ReleaseError, "missing"):
                self.render(name, checksums([item for item in ARCHIVES if item != missing]))

    def test_inputs_are_validated(self):
        digests = homebrew_alpha.parse_checksums(checksums())
        for version in ("0.1.0", "v0.1.0-alpha.1", "0.1.0-beta.1", "0.1.0-alpha.1\nx"):
            with self.assertRaises(ReleaseError):
                homebrew_alpha.render("mike", version, REPOSITORY, digests)
        with self.assertRaises(ReleaseError):
            homebrew_alpha.render("mike", VERSION, "temet-ai/mike/extra", digests)
        with self.assertRaises(ReleaseError):
            homebrew_alpha.parse_checksums("not a checksum line\n")

    def test_checksum_forms_from_sha256sum_are_read(self):
        digests = homebrew_alpha.parse_checksums(f"{'A' * 64}  ./a.tar.gz\n{'b' * 64} *b.tar.gz\n")
        self.assertEqual({"a.tar.gz": "a" * 64, "b.tar.gz": "b" * 64}, digests)

    def test_command_writes_both_formulas_and_refuses_an_incomplete_release(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            good, bad = root / "good.txt", root / "bad.txt"
            good.write_text(checksums())
            bad.write_text(checksums(ARCHIVES[:3]))
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(0, homebrew_alpha.main([VERSION, REPOSITORY, str(good), str(root / "Formula")]))
            self.assertEqual(["mike-mcp.rb", "mike.rb"], sorted(path.name for path in (root / "Formula").iterdir()))
            self.assertIn("class Mike < Formula", (root / "Formula/mike.rb").read_text())
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(1, homebrew_alpha.main([VERSION, REPOSITORY, str(bad), str(root / "Other")]))
            self.assertFalse((root / "Other").exists())


if __name__ == "__main__":
    unittest.main()
