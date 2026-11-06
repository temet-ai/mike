"""Finished task output is graded once, including failure and missing verdicts."""
from pathlib import Path
import subprocess
import tempfile
import unittest


class FinishedTaskGradeTests(unittest.TestCase):
    def test_grade_only_never_polls_and_preserves_failures(self):
        script = Path(__file__).resolve().parents[1] / "task-wait"
        cases = [
            ("GATE: PASS\n", 0),
            ("REAL_EXIT_STATUS=7\n", 7),
            ("GATE: FAIL (make check, exit 9)\nGATE: PASS\n", 9),
            ("REAL_EXIT_STATUS=2\nREAL_EXIT_STATUS=0\n", 2),
            ("DONE, but no graded command result\n", 3),
            ("", 3),
            (None, 3),
        ]
        with tempfile.TemporaryDirectory(prefix="task-grade-") as temp:
            output = Path(temp) / "finished.output"
            for content, expected in cases:
                with self.subTest(content=content):
                    if content is None:
                        output.unlink(missing_ok=True)
                    else:
                        output.write_text(content)
                    result = subprocess.run(
                        [str(script), str(output), "--grade-only"],
                        capture_output=True, text=True, timeout=2,
                    )
                    self.assertEqual(result.returncode, expected, result.stderr)


if __name__ == "__main__":
    unittest.main()
