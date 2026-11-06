"""Offline source contract checks, not GitHub's hosted workflow validation."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github/workflows"
GATES = ("rust-quality", "storybook", "e2e")


def block(text, key, indent=0):
    """Read our deliberately simple block-style workflow mapping sections."""
    prefix = " " * indent
    match = re.search(rf"^{prefix}{re.escape(key)}:\s*\n", text, re.M)
    if match is None:
        raise AssertionError(f"missing {key} mapping")
    lines = []
    for line in text[match.end():].splitlines():
        if line.strip() and not line.lstrip().startswith("#"):
            if len(line) - len(line.lstrip()) <= indent:
                break
        lines.append(line)
    return "\n".join(lines)


class SourceGateRoutingTests(unittest.TestCase):
    def test_each_gate_has_filtered_pr_and_unfiltered_main_manual_and_reusable_routes(self):
        for name in GATES:
            with self.subTest(workflow=name):
                text = (WORKFLOWS / f"{name}.yml").read_text()
                events = block(text, "on")
                self.assertIn("    paths:", block(events, "pull_request", 2))
                self.assertEqual(block(events, "push", 2).strip(), "branches: [main]")
                self.assertEqual(block(events, "workflow_dispatch", 2).strip(), "")
                self.assertEqual(block(events, "workflow_call", 2).strip(), "")
                self.assertNotRegex(block(text, "jobs"), r"(?m)^\s+if:")
                self.assertNotIn("continue-on-error:", text)
                self.assertEqual(block(text, "permissions").strip(), "contents: read")
                # Direct and called workflows must not cancel each other.
                self.assertIn("${{ github.workflow }}-${{ github.ref }}", text)
                self.assertNotRegex(text, r"(?m)^\s+ref:")

    def test_candidate_entrypoint_invokes_all_gates_at_the_selected_event_commit(self):
        text = (WORKFLOWS / "release-source-gates.yml").read_text()
        self.assertEqual(block(text, "on").strip(), "workflow_dispatch:")
        self.assertEqual(block(text, "permissions").strip(), "contents: read")
        called = re.findall(r"uses: (\./\.github/workflows/[^\s]+)", block(text, "jobs"))
        self.assertEqual(set(called), {f"./.github/workflows/{name}.yml" for name in GATES})
        self.assertEqual(len(called), len(GATES))
        self.assertIn("test_source_gate_routing.py", text)
        self.assertIn("test_frontend_audit.py", text)
        for forbidden in ("paths:", "branches:", "if:", "continue-on-error:", "secrets: inherit"):
            self.assertNotIn(forbidden, text)

    def test_rust_service_builds_authoritative_pg19_provider(self):
        provider = (ROOT / "crates/mike/src/setup/docker_resource.rs").read_text()
        match = re.search(r'const DEFAULT_IMAGE: &str =\s*"([^"]+)";', provider)
        self.assertIsNotNone(match)
        image = match.group(1)
        self.assertEqual(image, "mike/postgres-pgvector:19rc1-0.8.7")
        dockerfile = (ROOT / "scripts/capture-db/Dockerfile.pg19").read_text()
        self.assertEqual(dockerfile, (ROOT / "crates/mike/resources/postgres19.Dockerfile").read_text())
        self.assertRegex(dockerfile, r"FROM postgres:19[^@]+@sha256:[0-9a-f]{64}")
        workflow = (WORKFLOWS / "rust-quality.yml").read_text()
        self.assertIn(f"docker build -t {image}", workflow)
        self.assertIn("-f scripts/capture-db/Dockerfile.pg19", workflow)
        self.assertIn("scripts/check-test-tiers --require-db", workflow)
        self.assertIn("cargo deny check", workflow)
        self.assertNotIn("pgvector/pgvector:pg16", workflow)

    def test_frontend_advisory_gate_is_required_and_its_sources_trigger_pr_checks(self):
        text = (WORKFLOWS / "storybook.yml").read_text()
        self.assertIn("run: scripts/test-suite frontend-audit", block(text, "jobs"))
        pr = block(block(text, "on"), "pull_request", 2)
        for path in ("scripts/frontend-audit", "scripts/lib/frontend_audit.py",
                     "scripts/tests/test_frontend_audit.py", "scripts/test-suite"):
            self.assertIn(f"'{path}'", pr)
        for name in GATES:
            pr = block(block((WORKFLOWS / f"{name}.yml").read_text(), "on"), "pull_request", 2)
            self.assertIn("'.github/workflows/release-source-gates.yml'", pr)
            self.assertIn("'scripts/tests/test_source_gate_routing.py'", pr)

    def test_all_lane_includes_the_same_unweakened_local_audit(self):
        text = (ROOT / "scripts/test-suite").read_text()
        self.assertIn('phase "Frontend dependency advisories" "$SCRIPT_DIR/frontend-audit"', text)
        all_case = text.split("  all)\n", 1)[1].split("    ;;", 1)[0]
        self.assertIn("run_frontend_audit", all_case)
        self.assertNotIn("|| true", all_case)


if __name__ == "__main__":
    unittest.main()
