"""Linux procfs observations of the exact owned daemon, never machine-wide usage."""
import os
from pathlib import Path
import re
import threading

from journey_process import JourneyError, require

SAMPLE_INTERVAL = 0.25


def process_identity(path):
    raw = (path / "stat").read_text()
    pid, separator, rest = raw.partition(" (")
    _name, separator, fields = rest.rpartition(") ")
    fields = fields.split()
    require(separator and pid.isdigit() and len(fields) >= 22, "RELIABILITY_PROC_IDENTITY_SCHEMA")
    return int(pid), int(fields[1]), int(fields[2]), int(fields[3]), int(fields[19])


class OwnedResources:
    def __init__(self, child):
        require(type(child.pid) is int and child.pid > 0 and child.poll() is None,
                "RELIABILITY_RESOURCE_PROCESS_NOT_OWNED")
        self.path = Path("/proc") / str(child.pid)
        try:
            self.identity = process_identity(self.path)
            anchor = child._anchor.pid
            require(self.identity[:4] == (child.pid, anchor, anchor, anchor),
                    "RELIABILITY_RESOURCE_PROCESS_NOT_OWNED")
        except (OSError, ValueError, AttributeError) as error:
            raise JourneyError("RELIABILITY_OWNED_PROCFS_UNAVAILABLE") from error

    def read(self):
        try:
            require(process_identity(self.path) == self.identity, "RELIABILITY_RESOURCE_PROCESS_CHANGED")
            raw = (self.path / "status").read_text()
            values = {}
            for key in ("VmRSS", "VmHWM"):
                matches = re.findall(r"^" + key + r":\s+(\d+) kB$", raw, re.MULTILINE)
                require(len(matches) == 1, "RELIABILITY_PROC_MEMORY_SCHEMA")
                values[key] = int(matches[0]) * 1024
            with os.scandir(self.path / "fd") as entries:
                count = sum(1 for entry in entries if entry.name.isdigit())
            require(values["VmHWM"] >= values["VmRSS"] > 0 and count > 0,
                    "RELIABILITY_PROC_MEMORY_SCHEMA")
            require(process_identity(self.path) == self.identity, "RELIABILITY_RESOURCE_PROCESS_CHANGED")
            return {"rss_current_bytes": values["VmRSS"], "rss_lifetime_peak_bytes": values["VmHWM"], "fd_count": count}
        except (OSError, ValueError) as error:
            raise JourneyError("RELIABILITY_OWNED_PROCFS_UNAVAILABLE") from error


class ResourceSampler:
    def __init__(self, child, budgets, report):
        self.source = OwnedResources(child)
        self.budgets = budgets
        self.report = report
        self.stop = threading.Event()
        self.guard = threading.Lock()
        self.worker = None
        self.failure = None
        self.observation = {"state": "RUNNING", "source": "linux_procfs_owned_daemon_only",
            "scope": "Exact daemon PID/start time under retained owned supervisor; excludes PostgreSQL, supervisor, MCP and browser.",
            "sample_interval_seconds": SAMPLE_INTERVAL, "samples": 0,
            "fd_peak_kind": "sampled_lower_bound_not_kernel_high_water", "rss_peak_kind": "kernel_process_lifetime_high_water",
            "rss_sampled_peak_bytes": 0, "rss_lifetime_peak_bytes": 0, "fd_sampled_peak": 0}
        report["resources"] = self.observation

    def sample(self):
        with self.guard:
            if self.failure:
                raise JourneyError(self.failure)
            value = self.source.read()
            if self.observation["samples"] == 0:
                self.observation["baseline"] = dict(value)
            self.observation["final"] = dict(value)
            self.observation["samples"] += 1
            self.observation["rss_sampled_peak_bytes"] = max(self.observation["rss_sampled_peak_bytes"], value["rss_current_bytes"])
            self.observation["rss_lifetime_peak_bytes"] = max(self.observation["rss_lifetime_peak_bytes"], value["rss_lifetime_peak_bytes"])
            self.observation["fd_sampled_peak"] = max(self.observation["fd_sampled_peak"], value["fd_count"])
            require(value["rss_lifetime_peak_bytes"] <= self.budgets["rss_peak_bytes"], "RELIABILITY_RSS_BUDGET_EXCEEDED")
            require(value["fd_count"] <= self.budgets["fd_peak"], "RELIABILITY_FD_BUDGET_EXCEEDED")
            return value

    def run(self):
        while not self.stop.wait(SAMPLE_INTERVAL):
            try:
                self.sample()
            except Exception as error:
                self.failure = str(error) if isinstance(error, JourneyError) else "RELIABILITY_RESOURCE_SAMPLER_FAILED"
                self.observation["state"] = "FAILED"
                return

    def __enter__(self):
        try:
            self.sample()
            self.worker = threading.Thread(target=self.run, daemon=True)
            self.worker.start()
            return self
        except Exception:
            self.observation["state"] = "FAILED"
            raise

    def __exit__(self, kind, value, traceback):
        try:
            self.stop.set()
            self.worker.join(timeout=2)
            require(not self.worker.is_alive(), "RELIABILITY_RESOURCE_SAMPLER_CLEANUP")
            if kind is not None:
                self.observation["state"] = "FAILED"
                return False
            self.sample()
            baseline, final = self.observation["baseline"], self.observation["final"]
            self.observation.update(state="PASSED", rss_current_delta_bytes=final["rss_current_bytes"] - baseline["rss_current_bytes"],
                                    fd_delta=final["fd_count"] - baseline["fd_count"], observed_values_within_explicit_budgets=True)
            self.report["unmeasured"].pop("os_rss", None)
            self.report["unmeasured"].pop("os_file_descriptors", None)
        except Exception:
            self.observation["state"] = "FAILED"
            raise
