import json
import tempfile
import unittest
from pathlib import Path

from common import ReleaseError
from glibc_floor import check_manifest, highest_glibc, main


def report(*versions):
    return "\n".join(f"  0x0010: Name: GLIBC_{version}  Flags: none  Version: 3" for version in versions)


def manifest(target="x86_64-unknown-linux-gnu", mike=("2.17", "2.35"), mcp=("2.34",)):
    return {"target": target, "artifacts": [
        {"binary": "mike", "runtime_requirements": {"symbol_versions": report(*mike)}},
        {"binary": "mike-mcp", "runtime_requirements": {"symbol_versions": report(*mcp)}}]}


class GlibcFloorTests(unittest.TestCase):
    def test_highest_version_is_numeric_not_textual(self):
        self.assertEqual((2, 35, 0), highest_glibc(report("2.5", "2.35", "2.9")))
        self.assertEqual((2, 39, 0), highest_glibc(report("2.17", "2.39")))
        self.assertIsNone(highest_glibc("no versions here"))

    def test_candidate_at_or_below_the_floor_passes(self):
        self.assertEqual({"mike": "2.35", "mike-mcp": "2.34"}, check_manifest(manifest()))

    def test_candidate_needing_glibc_2_39_is_rejected(self):
        with self.assertRaisesRegex(ReleaseError, "glibc_floor_exceeded_mike"):
            check_manifest(manifest(mike=("2.17", "2.39")))
        with self.assertRaisesRegex(ReleaseError, "glibc_floor_exceeded_mike-mcp"):
            check_manifest(manifest(mcp=("2.36",)))

    def test_missing_or_unreadable_report_is_rejected_not_passed(self):
        value = manifest()
        value["artifacts"][0]["runtime_requirements"]["symbol_versions"] = "fixture runtime library report"
        with self.assertRaisesRegex(ReleaseError, "glibc_symbol_versions_not_observed"):
            check_manifest(value)
        del value["artifacts"][1]["runtime_requirements"]
        with self.assertRaises(ReleaseError):
            check_manifest(manifest() | {"artifacts": value["artifacts"][1:]})

    def test_macos_candidate_is_outside_the_glibc_floor(self):
        self.assertEqual({}, check_manifest({"target": "aarch64-apple-darwin", "artifacts": []}))

    def test_command_line_exit_status(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            good, bad = root / "good.json", root / "bad.json"
            good.write_text(json.dumps(manifest()))
            bad.write_text(json.dumps(manifest(mike=("2.39",))))
            self.assertEqual(0, main([str(good)]))
            self.assertEqual(1, main([str(bad)]))
            self.assertEqual(1, main([str(root / "absent.json")]))


if __name__ == "__main__":
    unittest.main()
