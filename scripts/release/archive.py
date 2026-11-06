"""Deterministic archive creation and verification of the same candidate bytes."""

from datetime import datetime, timezone
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import sys
import tarfile
import tempfile

from common import ReleaseError, native_target, regular_file, run, sha256
from migration_inventory import validate_migration_inventory
from notice_inputs import MAX_NOTICE_BYTES, verify_notice
from mod_assets import MAX_ASSET_BYTES, create_file, is_asset

BINARIES = ("mike", "mike-mcp")
SUPPORT_FILES = {"mike-maintenance": 0o755, "mike-maintenance.pyz": 0o644}
THIRD_PARTY = "THIRD-PARTY-NOTICES.md"


def pack(archive, binary, license_path, notice, epoch):
    contents = [(binary.name, binary.read_bytes(), 0o755),
                ("LICENSE", license_path.read_bytes(), 0o644),
                ("NOTICE.json", notice, 0o644)]
    third_party = license_path.parent / THIRD_PARTY
    if third_party.is_file() and not third_party.is_symlink():
        contents.append((THIRD_PARTY, third_party.read_bytes(), 0o644))
    if binary.name == "mike":
        # Build-only dependency, intentionally absent from the shipped zipapp.
        from maintenance_bundle import payload
        contents.extend(payload())
        from mod_assets import payload as mod_payload
        contents.extend(mod_payload(license_path.parent))
    with archive.open("wb") as output:
        with gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as tar:
                for name, data, mode in contents:
                    entry = tarfile.TarInfo(name)
                    entry.size, entry.mode, entry.mtime = len(data), mode, epoch
                    entry.uid = entry.gid = 0
                    tar.addfile(entry, io.BytesIO(data))
    return [{"path": name, "sha256": hashlib.sha256(data).hexdigest(),
             "size": len(data), "mode": oct(mode)} for name, data, mode in contents]


def checked_inventory(name, inventory):
    if name not in BINARIES or not isinstance(inventory, list):
        raise ReleaseError(f"invalid inventory for {name}")
    modes = {name: 0o755, "LICENSE": 0o644, "NOTICE.json": 0o644}
    if any(not isinstance(item, dict) or not isinstance(item.get("path"), str) for item in inventory):
        raise ReleaseError(f"invalid inventory for {name}")
    expected = {item["path"]: item for item in inventory}
    if THIRD_PARTY in expected:  # absent from candidates prepared before the file existed
        modes[THIRD_PARTY] = 0o644
    assets ={path: item for path, item in expected.items() if is_asset(path)}
    if name == "mike":
        if len(assets) > 4096:
            raise ReleaseError("too many mod assets")
        for path, item in assets.items():
            if item.get("mode") not in ("0o644", "0o755"):
                raise ReleaseError("invalid mod asset permissions")
            modes[path] = int(item["mode"], 8)
    # Preserve verified predecessor generations prepared before maintenance was
    # bundled. New pack() calls always ship both support files, never half a kit.
    if name == "mike" and set(expected) == set(modes) | set(SUPPORT_FILES):
        modes.update(SUPPORT_FILES)
    if set(expected) != set(modes) or len(expected) != len(inventory):
        raise ReleaseError(f"invalid inventory for {name}")
    for path, item in expected.items():
        limit = (MAX_NOTICE_BYTES if path in ("NOTICE.json", THIRD_PARTY) else
                 MAX_ASSET_BYTES if path in SUPPORT_FILES or is_asset(path) else 2 * 1024**3)
        if (set(item) != {"path", "size", "sha256", "mode"}
                or type(item["size"]) is not int or not 0 <= item["size"] <= limit
                or not isinstance(item["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", item["sha256"])
                or item["mode"] != oct(modes[path])):
            raise ReleaseError(f"invalid inventory entry: {path}")
    return expected


def extract_checked(archive, destination, name, inventory):
    expected = checked_inventory(name, inventory)
    destination = Path(os.path.abspath(destination))
    if any(p.is_symlink() for p in (destination, *destination.parents)):
        raise ReleaseError("archive destination must not traverse symlinks")
    descriptor = os.open(destination, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        info = os.fstat(descriptor)
        if info.st_uid != os.getuid() or info.st_mode & 0o022 or os.listdir(descriptor):
            raise ReleaseError("archive destination must be empty, owned and not shared-writable")
        extract_members(archive, expected, descriptor)
    finally:
        os.close(descriptor)


def extract_members(archive, expected, descriptor):
    with tarfile.open(archive, "r:gz") as tar:
        members, seen = [], set()
        # Reject unexpected/oversized headers before advancing over their data.
        # getmembers() would first decode an unbounded number of hostile headers.
        for member in tar:
            if member.name not in expected or member.name in seen:
                raise ReleaseError(f"unexpected archive paths in {archive.name}")
            seen.add(member.name)
            item = expected[member.name]
            if (member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE)
                    or member.mode != int(item["mode"], 8) or member.size != item["size"]
                    or member.uid != 0 or member.gid != 0 or member.uname or member.gname
                    or member.linkname or member.pax_headers):
                raise ReleaseError(f"unsafe archive member: {member.name}")
            members.append(member)
        if seen != set(expected):
            raise ReleaseError(f"unexpected archive paths in {archive.name}")
        for member in members:
            item = expected[member.name]
            data = tar.extractfile(member).read()
            if hashlib.sha256(data).hexdigest() != item["sha256"]:
                raise ReleaseError(f"archive member hash mismatch: {member.name}")
            output = create_file(descriptor, member.name, int(item["mode"], 8))
            with os.fdopen(output, "wb") as stream:
                stream.write(data)
                os.fchmod(stream.fileno(), int(item["mode"], 8))


def smoke(binary, version, cwd, home):
    env = {"HOME": str(home), "TMPDIR": str(home), "PATH": "", "LC_ALL": "C", "LANG": "C"}
    output = run([str(binary), "--version"], cwd=cwd, env=env, timeout=30)
    prefix = f"{binary.name} {version}"
    if output != prefix and not output.startswith(prefix + " ") and not output.startswith(prefix + "\n"):
        raise ReleaseError(f"{binary.name} executable version disagrees with workspace: {output!r}")
    args = ["--help"] if binary.name == "mike" else ["list-tools", "--json"]
    result = run([str(binary), *args], cwd=cwd, env=env, timeout=30)
    if not result:
        raise ReleaseError(f"{binary.name} smoke command returned empty output")
    if binary.name == "mike-mcp":
        try:
            catalogue = json.loads(result)
        except json.JSONDecodeError as exc:
            raise ReleaseError("mike-mcp returned an invalid JSON catalogue") from exc
        if (not isinstance(catalogue, list) or not catalogue
                or any(not isinstance(tool, dict) or not tool.get("name") for tool in catalogue)):
            raise ReleaseError("mike-mcp returned an empty or invalid tool catalogue")
    return {"version_output": output, "command": [binary.name, *args],
            "stdout_sha256": hashlib.sha256(result.encode()).hexdigest()}


def source_timestamp(epoch):
    if type(epoch) is not int or not 0 <= epoch <= 0xffffffff:
        raise ReleaseError("source_date_epoch must be an unsigned 32-bit UTC epoch")
    return datetime.fromtimestamp(epoch, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def binary_identity(binary, version, source_commit, epoch, cwd, home):
    expected = {"schema_version": 1, "product": binary.name, "version": version,
                "source_sha": source_commit, "source_dirty": False,
                "built_at": source_timestamp(epoch)}
    env = {"HOME": str(home), "TMPDIR": str(home), "PATH": "", "LC_ALL": "C", "LANG": "C"}
    output = run([str(binary), "--build-info"], cwd=cwd, env=env, timeout=30)
    try:
        observed = json.loads(output)
    except json.JSONDecodeError as exc:
        raise ReleaseError(f"{binary.name} returned invalid JSON build identity") from exc
    if (not isinstance(observed, dict) or observed != expected
            or observed.get("source_dirty") is not False
            or type(observed.get("schema_version")) is not int):
        raise ReleaseError(f"{binary.name} executable build identity disagrees with clean candidate source")
    return observed


def maintenance_smoke(extracted, cwd, home):
    env = {"HOME": str(home), "TMPDIR": str(home), "LC_ALL": "C", "LANG": "C",
           "PATH": str(Path(sys.executable).parent) + ":/usr/bin:/bin"}
    output = run([str(extracted / "mike-maintenance"), "--help"], cwd=cwd, env=env, timeout=30)
    if "upgrade-check" not in output or "Python 3.11+" not in output:
        raise ReleaseError("archived maintenance help is incomplete")
    return {"command": ["mike-maintenance", "--help"],
            "stdout_sha256": hashlib.sha256(output.encode()).hexdigest()}


def runtime_requirements(binary, target, env):
    cwd = binary.parent
    if target == "x86_64-unknown-linux-gnu":
        return {"method": "readelf (no ldd execution)",
                "program_headers": run(["readelf", "-l", binary.name], cwd=cwd, env=env),
                "dynamic_section": run(["readelf", "-d", binary.name], cwd=cwd, env=env),
                "symbol_versions": run(["readelf", "--version-info", binary.name], cwd=cwd, env=env)}
    return {"method": "otool", "libraries": run(["otool", "-L", binary.name], cwd=cwd, env=env),
            "load_commands": run(["otool", "-l", binary.name], cwd=cwd, env=env)}


def prepare_archives(root, staging, build_dir, version, target, epoch, env, commit, notice=None):
    notice = notice if notice is not None else (json.dumps({"project": "mike", "license": "MIT", "source_license": "LICENSE",
                         "third_party_notice_review": "not_run",
                         "status": "unqualified local candidate; not for publication"},
                        indent=2, sort_keys=True) + "\n").encode()
    regular_file(root / "LICENSE")
    artifacts = []
    with tempfile.TemporaryDirectory(prefix="mike-release-smoke-") as temp:
        sandbox = Path(temp).resolve()
        for name in BINARIES:
            binary = build_dir / target / "dist" / name
            regular_file(binary)
            if not os.access(binary, os.X_OK):
                raise ReleaseError(f"binary is not executable: {name}")
            filename = f"{name}-v{version}-{target}.tar.gz"
            archive = staging / filename
            inventory = pack(archive, binary, root / "LICENSE", notice, epoch)
            digest = sha256(archive)
            (staging / f"{filename}.sha256").write_text(f"{digest}  {filename}\n")
            extracted = sandbox / name
            extracted.mkdir()
            extract_checked(archive, extracted, name, inventory)
            home, cwd = extracted / "home", extracted / "unrelated-cwd"
            home.mkdir()
            cwd.mkdir()
            observed = smoke(extracted / name, version, cwd, home)
            identity = binary_identity(extracted / name, version, commit, epoch, cwd, home)
            artifacts.append({"binary": name, "archive": filename, "sha256": digest,
                              "contents": inventory, "smoke": observed, "build_info": identity,
                              "runtime_requirements": runtime_requirements(extracted / name, target, env)})
            if name == "mike":
                artifacts[-1]["maintenance_smoke"] = maintenance_smoke(extracted, cwd, home)
    return artifacts


def verify_candidate(candidate):
    regular_file(candidate / "manifest.json")
    manifest = json.loads((candidate / "manifest.json").read_text())
    if manifest["schema_version"] != 1 or manifest["state"] != "prepared" or manifest["qualified"] is not False:
        raise ReleaseError("unsupported candidate manifest")
    if not re.fullmatch(r"[0-9a-f]{40,64}", manifest["source_commit"]):
        raise ReleaseError("invalid candidate source commit")
    epoch = manifest.get("source_date_epoch")
    source_timestamp(epoch)
    target = native_target(candidate, os.environ.copy())
    if manifest["target"] != target:
        raise ReleaseError(f"candidate requires native {manifest['target']}, current host is {target}")
    version = manifest["version"]
    if not re.fullmatch(r"[0-9A-Za-z.+-]+", version):
        raise ReleaseError("invalid candidate version")
    artifacts = manifest["artifacts"]
    if sorted(a["binary"] for a in artifacts) != sorted(BINARIES):
        raise ReleaseError("candidate must contain exactly mike and mike-mcp")
    expected_files = {"manifest.json"}
    notice_digest = None
    with tempfile.TemporaryDirectory(prefix="mike-release-verify-") as temp:
        sandbox = Path(temp).resolve()
        for item in artifacts:
            name = item["binary"]
            expected_name = f"{name}-v{version}-{target}.tar.gz"
            if item["archive"] != expected_name:
                raise ReleaseError("unexpected archive filename")
            archive = candidate / expected_name
            checksum = candidate / f"{expected_name}.sha256"
            expected_files.update((archive.name, checksum.name))
            regular_file(archive)
            regular_file(checksum)
            digest = sha256(archive)
            if digest != item["sha256"] or checksum.read_text() != f"{digest}  {archive.name}\n":
                raise ReleaseError(f"archive/checksum mismatch: {archive.name}")
            extracted = sandbox / name
            extracted.mkdir()
            extract_checked(archive, extracted, name, item["contents"])
            notice = (extracted / "NOTICE.json").read_bytes()
            notice_state = verify_notice(notice, manifest)
            if notice_state != manifest.get("third_party_notice_inputs", "not_run"):
                raise ReleaseError("packaged notice evidence disagrees with candidate manifest")
            digest = hashlib.sha256(notice).hexdigest()
            if notice_digest is not None and digest != notice_digest:
                raise ReleaseError("paired archives carry different notice evidence")
            notice_digest = digest
            home, cwd = extracted / "home", extracted / "unrelated-cwd"
            home.mkdir()
            cwd.mkdir()
            if smoke(extracted / name, version, cwd, home) != item["smoke"]:
                raise ReleaseError(f"archived executable smoke output changed: {name}")
            identity = binary_identity(extracted / name, version, manifest["source_commit"], epoch, cwd, home)
            if identity != item.get("build_info"):
                raise ReleaseError(f"archived executable build identity evidence changed: {name}")
            if name == "mike" and (extracted / "mike-maintenance").exists():
                observed = maintenance_smoke(extracted, cwd, home)
                if "maintenance_smoke" in item and observed != item["maintenance_smoke"]:
                    raise ReleaseError("archived maintenance smoke output changed")
            elif "maintenance_smoke" in item:
                raise ReleaseError("candidate declares maintenance evidence without the shipped helper")
    if {p.name for p in candidate.iterdir()} != expected_files:
        raise ReleaseError("candidate directory contains unexpected files")
    validate_migration_inventory(manifest.get("migration_inventory"), manifest["source_commit"])
    return manifest
