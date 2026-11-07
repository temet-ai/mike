"""Storybook batches cover every story once, and a stall cannot hang a run."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
FAKE_PNPM = """#!/usr/bin/env bash
case "$*" in
  *src/lib/components/hang/*)
    echo " RUN  hanging story"
    sleep 300 &
    echo $! >"$PID_FILE"
    wait
    ;;
  *)
    echo " Test Files  1 passed (1)"
    echo "      Tests  1 passed (1)"
    ;;
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

    def test_a_silent_batch_is_stopped_named_and_the_next_batch_still_runs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "scripts").mkdir()
            shutil.copy2(ROOT / "scripts/sb-test", root / "scripts/sb-test")
            for area in ("hang", "okay"):
                folder = root / "frontend/src/lib/components" / area
                folder.mkdir(parents=True)
                (folder / "x.stories.svelte").write_text("")
            (root / "frontend/src/app.css").write_text("")
            bin_dir = root / "bin"
            bin_dir.mkdir()
            pnpm = bin_dir / "pnpm"
            pnpm.write_text(FAKE_PNPM)
            pnpm.chmod(0o700)
            pid_file = root / "pid"
            started = time.monotonic()
            result = subprocess.run(
                ["bash", str(root / "scripts/sb-test"), "--owned-test-child", "--storybook",
                 "--skip-typecheck"], capture_output=True, text=True, timeout=60,
                env={**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}",
                     "SB_STALL_SECONDS": "2", "SB_BATCH_SIZE": "1", "PID_FILE": str(pid_file)})
            output = result.stdout + result.stderr
            self.assertLess(time.monotonic() - started, 30, output)
            self.assertNotEqual(result.returncode, 0, output)
            self.assertIn("STALLED", output)
            self.assertIn("hanging story", output)
            self.assertIn("b02-okay", output)
            self.assertIn("✓", output)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_file.read_text()), 0)
            logs = list((root / "frontend/test-results/sb-test").rglob("*.log"))
            self.assertEqual(len(logs), 2, output)


if __name__ == "__main__":
    unittest.main()
