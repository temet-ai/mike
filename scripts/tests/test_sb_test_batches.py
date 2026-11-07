"""A run covers every story once, finishes despite failures, and reports them all."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
AREA = "src/lib/components/mixed"
FAKE_PNPM = f"""#!/usr/bin/env bash
hang() {{ sleep 300 & echo $! >"$PID_FILE"; wait; }}
case "$*" in
  *svelte-kit*) exit 0 ;;
  *svelte-check*) echo "Error: types are wrong in a.svelte"; echo "Error: types are wrong in b.svelte"; exit 1 ;;
  *"--project unit"*)
    echo " FAIL  |unit| src/lib/logic.test.ts > adds"
    echo "      Tests  1 failed | 4 passed (5)"
    exit 1 ;;
  *{AREA}/hang.stories.svelte*)
    echo " RUN  hanging story"
    hang ;;
  *{AREA}/late.stories.svelte*)
    echo " ✓ |storybook (chromium)| {AREA}/late.stories.svelte > Late 9ms"
    echo " × |storybook (chromium)| {AREA}/late.stories.svelte > LateBroken 9ms"
    echo " FAIL  |storybook (chromium)| {AREA}/late.stories.svelte > LateBroken"
    echo "      Tests  1 failed | 1 passed (2)"
    exit 1 ;;
  *{AREA}/bad.stories.svelte*)
    echo " × |storybook (chromium)| {AREA}/bad.stories.svelte > Broken 9ms"
    echo "      Tests  1 failed (1)"
    exit 1 ;;
  *{AREA}/*)
    echo " ✓ |storybook (chromium)| {AREA}/good.stories.svelte > Good 9ms"
    echo " × |storybook (chromium)| {AREA}/bad.stories.svelte > Broken 9ms"
    hang ;;
  *)
    echo " ✓ |storybook (chromium)| src/routes/okay/page.stories.svelte > Okay 9ms"
    echo " Test Files  1 passed (1)"
    echo "      Tests  1 passed (1)" ;;
esac
"""


def batches(frontend):
    result = subprocess.run(["bash", str(ROOT / "scripts/sb-test"), "--owned-test-child",
                             "--list-batches"], capture_output=True, text=True, check=True,
                            env={**os.environ, "SB_FRONTEND_ROOT": str(frontend)})
    return [line.split("\t") for line in result.stdout.splitlines()]


class SbTestBatches(unittest.TestCase):
    def test_every_story_file_is_in_exactly_one_batch(self):
        frontend = ROOT / "frontend"
        stories = sorted(path.relative_to(frontend).as_posix()
                         for path in (frontend / "src").rglob("*.stories.*"))
        rows = batches(frontend)
        filters = [item for row in rows for item in row[2].split(" ")]
        for story in stories:
            hits = [item for item in filters if story.startswith(item)]
            self.assertEqual(len(hits), 1, story)
        self.assertEqual(sum(int(row[1]) for row in rows), len(stories))
        self.assertEqual(len({row[0] for row in rows}), len(rows))

    def run_failing_project(self, *flags):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "scripts").mkdir()
            shutil.copy2(ROOT / "scripts/sb-test", root / "scripts/sb-test")
            for name in ("good", "bad", "hang", "late"):
                story = root / "frontend" / AREA / f"{name}.stories.svelte"
                story.parent.mkdir(parents=True, exist_ok=True)
                story.write_text("")
            okay = root / "frontend/src/routes/okay/page.stories.svelte"
            okay.parent.mkdir(parents=True)
            okay.write_text("")
            (root / "frontend/src/app.css").write_text("")
            bin_dir = root / "bin"
            bin_dir.mkdir()
            pnpm = bin_dir / "pnpm"
            pnpm.write_text(FAKE_PNPM)
            pnpm.chmod(0o700)
            pid_file = root / "pid"
            started = time.monotonic()
            result = subprocess.run(
                ["bash", str(root / "scripts/sb-test"), "--owned-test-child", *flags],
                capture_output=True, text=True, timeout=60,
                env={**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}",
                     "SB_STALL_SECONDS": "2", "SB_BATCH_SIZE": "4", "PID_FILE": str(pid_file)})
            output = result.stdout + result.stderr
            self.assertLess(time.monotonic() - started, 40, output)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_file.read_text()), 0)
            return result, output

    def test_one_run_reports_every_failure_and_stall_from_every_tier(self):
        result, output = self.run_failing_project()
        self.assertNotEqual(result.returncode, 0, output)
        summary = output[output.index("sb-test: summary:"):]
        self.assertIn("sb-test: summary: 7 tests passed, 5 failed, 1 stalled", summary)
        for failure in ("typecheck: Error: types are wrong in a.svelte",
                        "typecheck: Error: types are wrong in b.svelte",
                        "|unit| src/lib/logic.test.ts > adds",
                        f"{AREA}/bad.stories.svelte > Broken",
                        f"{AREA}/late.stories.svelte > LateBroken"):
            self.assertIn(failure, summary)
        self.assertIn(f"{AREA}/hang.stories.svelte", summary)
        self.assertIn("b02-okay", output)

    def test_json_names_every_failure_and_stall(self):
        result, output = self.run_failing_project("--storybook", "--skip-typecheck", "--json")
        verdict = json.loads(result.stdout.splitlines()[-1])
        self.assertEqual(verdict["verdict"], "fail")
        self.assertEqual(len(verdict["failures"]), 2, output)
        self.assertEqual(len(verdict["stalled"]), 1, output)
        self.assertIn(f"{AREA}/late.stories.svelte", verdict["failed_files"])


if __name__ == "__main__":
    unittest.main()
