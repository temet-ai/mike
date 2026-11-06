"""Explicit normal-default routing that never executes candidate bytes on the host."""
from dataclasses import dataclass
from pathlib import Path
import re
import tempfile

from archive import BINARIES
from common import ReleaseError, sha256
from inputs import snapshot_inputs
from journey_reliability_profiles import NORMAL_PROFILE

MAX_NORMAL_INPUT_BYTES = 512 * 1024 * 1024


@dataclass(frozen=True)
class NormalRunner:
    image: str
    docker_binary: Path
    docker_endpoint: Path
    runtime_name: str


def runner_options(image=None, docker_binary=None, docker_endpoint=None, runtime_name=None):
    values = (image, docker_binary, docker_endpoint, runtime_name)
    if all(value is None for value in values):
        return None
    if any(value is None for value in values):
        raise ReleaseError("normal runner requires image, Docker executable, local endpoint and configured runtime")
    if not isinstance(image, str) or "://" in image or not re.fullmatch(r"[a-z0-9][a-z0-9._:/-]{0,240}@sha256:[0-9a-f]{64}", image):
        raise ReleaseError("normal runner image must be an explicit local repository@sha256 digest")
    if (not isinstance(runtime_name, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}", runtime_name)
            or runtime_name in ("runc", "default")):
        raise ReleaseError("normal runner requires an explicit custom runtime alias")
    binary, endpoint = Path(docker_binary), Path(docker_endpoint)
    if not binary.is_absolute() or not endpoint.is_absolute():
        raise ReleaseError("normal runner executable and Unix endpoint must be absolute paths")
    return NormalRunner(image, binary, endpoint, runtime_name)


def validate_request(profile, runner, *, other_journeys=False):
    if runner is not None:
        if not isinstance(runner, NormalRunner) or runner != runner_options(
                runner.image, runner.docker_binary, runner.docker_endpoint, runner.runtime_name):
            raise ReleaseError("invalid normal runner options")
        if profile != NORMAL_PROFILE:
            raise ReleaseError("normal runner options require the normal_defaults profile")
    if profile == NORMAL_PROFILE and other_journeys:
        raise ReleaseError("normal_defaults must run separately from host/native archive journeys")


def dispatch_runner(snapshot, manifest, manifest_sha256, *, runner,
                    duration_seconds, concurrent_sessions, resource_budgets):
    from journey_reliability_runner import run_isolated_normal_journey
    return run_isolated_normal_journey(
        snapshot, manifest, manifest_sha256,
        duration_seconds=duration_seconds, concurrent_sessions=concurrent_sessions,
        resource_budgets=resource_budgets, docker_binary=runner.docker_binary,
        docker_endpoint=runner.docker_endpoint, runner_image=runner.image, runtime_name=runner.runtime_name)


def input_identity(snapshot, manifest, manifest_sha256):
    version, target, source = (manifest.get(key) for key in ("version", "target", "source_commit"))
    if (target != "x86_64-unknown-linux-gnu" or not isinstance(version, str)
            or not re.fullmatch(r"[0-9A-Za-z.+-]{1,100}", version)
            or not isinstance(source, str) or not re.fullmatch(r"[0-9a-f]{40}", source)):
        raise ReleaseError("isolated normal runner requires a hash-bound Linux x86_64 candidate")
    archives = []
    for binary in BINARIES:
        filename = f"{binary}-v{version}-{target}.tar.gz"
        archives.append({"binary": binary, "archive": filename, "sha256": sha256(snapshot / filename)})
    return {"version": version, "target": target, "source_commit": source,
            "manifest_sha256": manifest_sha256, "archives": archives}


def validate_journey(journey):
    if (not isinstance(journey, dict) or journey.get("qualified") is not False
            or journey.get("state") not in ("INCOMPLETE", "FAILED")
            or not isinstance(journey.get("checks"), list)
            or not isinstance(journey.get("limits"), list)):
        raise ReleaseError("isolated runner returned invalid partial evidence")
    cases = journey.get("full_cases")
    expected = [f"AC{number:02}" for number in range(1, 28)]
    if (not isinstance(cases, list) or len(cases) != len(expected)
            or any(not isinstance(case, dict) or case.get("id") != identifier
                   or case.get("state") != "NOT_RUN" or case.get("required") is not True
                   for case, identifier in zip(cases, expected))):
        raise ReleaseError("isolated runner cannot qualify a complete acceptance case")


def run_normal_helpers(candidate, report, runner, *, duration_seconds,
                       concurrent_sessions, resource_budgets):
    report["isolation"] = {"host_candidate_executed": False,
                           "container_verification": "See isolated runner observations; not inferred from options."}
    verification = {"id": "isolated_normal_input_snapshot", "state": "RUNNING", "kind": "helper"}
    report["checks"].append(verification)
    if runner is None:
        verification.update(state="FAILED", code="RELIABILITY_ISOLATED_RUNNER_REQUIRED")
        raise ReleaseError("normal_defaults requires an explicitly configured isolated runner")
    try:
        with tempfile.TemporaryDirectory(prefix="mike-normal-input-", dir="/tmp") as temporary:
            snapshot = Path(temporary) / "candidate"
            snapshot.mkdir(mode=0o700)
            manifest = snapshot_inputs(Path(candidate).absolute(), snapshot, max_total_bytes=MAX_NORMAL_INPUT_BYTES)
            digest = sha256(snapshot / "manifest.json")
            identity = input_identity(snapshot, manifest, digest)
            report["candidate"] = identity
            for case in report["cases"]:
                case["runtime_input_identity"] = dict(identity)
            verification.update(state="PASSED", candidate_execution=False,
                                scope="Private input copy and byte hashes only; archive/runtime verification belongs to the container.")
            journey = dispatch_runner(snapshot, manifest, digest, runner=runner,
                                      duration_seconds=duration_seconds, concurrent_sessions=concurrent_sessions,
                                      resource_budgets=resource_budgets)
            validate_journey(journey)
            report["archive_reliability_journey"] = journey
            report["checks"].extend(journey["checks"])
            report["limits"].extend(journey["limits"])
            if journey["state"] == "FAILED":
                raise ReleaseError("isolated normal-default journey failed; inspect sanitized check codes")
    except Exception as error:
        if verification["state"] == "RUNNING":
            verification.update(state="FAILED", error_type=type(error).__name__)
        raise
