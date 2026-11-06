"""Fabricated procfs/sampler contracts, not measured installed-process evidence."""
from contextlib import contextmanager
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest.mock import Mock, patch
import unittest

import journey_reliability_resources as resources
from journey_process import JourneyError
from test_journey_reliability import OfflineTests
from test_journey_reliability_profiles import BUDGETS


def snapshot(rss=1000, peak=1200, fds=5):
    return {"rss_current_bytes": rss, "rss_lifetime_peak_bytes": peak, "fd_count": fds}


def stat(pid=123, parent=122, start=456):
    fields = ["S", str(parent), str(parent), str(parent)] + ["0"] * 18
    fields[19] = str(start)
    return f"{pid} (name with ) parentheses) " + " ".join(fields)


class ProcfsTests(OfflineTests):
    @contextmanager
    def procfs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            process = root / "123"
            process.mkdir(); (process / "fd").mkdir()
            (process / "stat").write_text(stat())
            (process / "status").write_text("VmRSS:\t10 kB\nVmHWM:\t12 kB\n")
            for fd in (0, 1, 2, 6):
                (process / "fd" / str(fd)).touch()
            child = SimpleNamespace(pid=123, _anchor=SimpleNamespace(pid=122), poll=lambda: None)
            with patch.object(resources, "Path", return_value=root):
                yield process, child

    def test_exact_owned_process_identity_and_kernel_units(self):
        with self.procfs() as (path, child):
            source = resources.OwnedResources(child)
            self.assertEqual(source.read(), snapshot(10 * 1024, 12 * 1024, 4))
            self.assertEqual(source.identity, (123, 122, 122, 122, 456))

    def test_foreign_parent_reused_pid_and_changed_start_fail_closed(self):
        with self.procfs() as (path, child):
            (path / "stat").write_text(stat(parent=999))
            with self.assertRaisesRegex(JourneyError, "NOT_OWNED"):
                resources.OwnedResources(child)
            (path / "stat").write_text(stat())
            source = resources.OwnedResources(child)
            (path / "stat").write_text(stat(start=457))
            with self.assertRaisesRegex(JourneyError, "PROCESS_CHANGED"):
                source.read()

    def test_missing_procfs_or_memory_fields_is_unavailable_not_zero(self):
        with self.procfs() as (path, child):
            source = resources.OwnedResources(child)
            for raw in ("", "VmRSS: 4 kB\n", "VmRSS: -4 kB\nVmHWM: 12 kB\n", "VmRSS: 14 kB\nVmHWM: 12 kB\n"):
                (path / "status").write_text(raw)
                with self.subTest(raw=raw), self.assertRaisesRegex(JourneyError, "MEMORY_SCHEMA"):
                    source.read()
            (path / "stat").unlink()
            with self.assertRaisesRegex(JourneyError, "PROCFS_UNAVAILABLE"):
                source.read()
            with self.assertRaisesRegex(JourneyError, "PROCFS_UNAVAILABLE"):
                resources.OwnedResources(child)


class SamplerTests(OfflineTests):
    def sampler(self, values):
        source = self.enterContext(patch.object(resources, "OwnedResources"))
        source.return_value.read.side_effect = values
        report = {"unmeasured": {"os_rss": "NOT_RUN", "os_file_descriptors": "NOT_RUN", "browser_responsiveness": "NOT_RUN"}}
        sampler = resources.ResourceSampler(object(), BUDGETS, report)
        sampler.worker = Mock(is_alive=lambda: False)
        return sampler, report

    def test_baseline_peaks_and_final_deltas_are_distinct_and_scoped(self):
        sampler, report = self.sampler([snapshot(), snapshot(2000, 2400, 9), snapshot(1500, 2400, 6)])
        sampler.sample(); sampler.sample()
        sampler.__exit__(None, None, None)
        observation = report["resources"]
        self.assertEqual(observation["samples"], 3)
        self.assertEqual(observation["rss_sampled_peak_bytes"], 2000)
        self.assertEqual(observation["rss_lifetime_peak_bytes"], 2400)
        self.assertEqual(observation["fd_sampled_peak"], 9)
        self.assertEqual(observation["rss_current_delta_bytes"], 500)
        self.assertEqual(observation["fd_delta"], 1)
        self.assertIn("lower_bound", observation["fd_peak_kind"])
        self.assertEqual(report["unmeasured"], {"browser_responsiveness": "NOT_RUN"})

    def test_budget_violations_and_sampler_errors_never_become_pass(self):
        for value, code in ((snapshot(1000, BUDGETS["rss_peak_bytes"] + 1), "RSS_BUDGET"),
                            (snapshot(fds=BUDGETS["fd_peak"] + 1), "FD_BUDGET")):
            sampler, report = self.sampler([value])
            with self.subTest(code=code), self.assertRaisesRegex(JourneyError, code):
                sampler.sample()
        sampler, report = self.sampler([OSError("private process path")])
        sampler.stop = Mock(wait=Mock(return_value=False))
        sampler.run()
        self.assertEqual(report["resources"]["state"], "FAILED")
        self.assertEqual(sampler.failure, "RELIABILITY_RESOURCE_SAMPLER_FAILED")
        with self.assertRaisesRegex(JourneyError, "SAMPLER_FAILED"):
            sampler.__exit__(None, None, None)

    def test_synchronous_entry_final_sample_and_cleanup_failures_are_terminal(self):
        for phase in ("entry", "final", "cleanup"):
            sampler, report = self.sampler([JourneyError("RELIABILITY_SYNCHRONOUS_FAILURE")])
            with self.subTest(phase=phase), self.assertRaises(JourneyError):
                if phase == "entry":
                    sampler.__enter__()
                else:
                    sampler.worker.is_alive = lambda: phase == "cleanup"
                    sampler.__exit__(None, None, None)
            self.assertEqual(report["resources"]["state"], "FAILED")
            self.assertEqual(report["unmeasured"]["os_rss"], "NOT_RUN")

    def test_failed_workload_still_stops_and_joins_sampler_without_claiming_measurements(self):
        sampler, report = self.sampler([snapshot()])
        sampler.__exit__(RuntimeError, RuntimeError("private"), None)
        sampler.worker.join.assert_called_once_with(timeout=2)
        self.assertTrue(sampler.stop.is_set())
        self.assertEqual(report["resources"]["state"], "FAILED")
        self.assertEqual(report["unmeasured"]["os_rss"], "NOT_RUN")


if __name__ == "__main__":
    unittest.main()
