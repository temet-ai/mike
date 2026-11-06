"""Private install ownership, immutable generations, and same-directory commit."""

from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import stat
import tempfile

from archive import BINARIES, SUPPORT_FILES, checked_inventory
from common import ReleaseError, regular_file, sha256
from mod_assets import is_asset, parents

OWNER = {"schema_version": 1, "installer": "mike-local-archive", "uid": os.getuid()}
STATE_NAME = "mike-archive"


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def check_directory(path, *, owned=False):
    if path.is_symlink() or not path.is_dir():
        raise ReleaseError(f"not a plain directory: {path}")
    info = path.stat()
    if owned and (info.st_uid != os.getuid() or info.st_mode & 0o022):
        raise ReleaseError(f"installation directory must be owned by you and not shared-writable: {path}")


def safe_prefix(prefix):
    prefix = Path(os.path.abspath(prefix))
    prohibited = ("/usr", "/opt", "/nix", "/bin", "/sbin", "/etc", "/System", "/Library", "/var/lib")
    if prefix == Path("/") or any(prefix == Path(p) or Path(p) in prefix.parents for p in prohibited):
        raise ReleaseError("choose a user-owned prefix; system/package-manager locations are refused")
    if any(part in ("Cellar", "Caskroom", ".linuxbrew", "homebrew") for part in prefix.parts):
        raise ReleaseError("package-manager prefixes are refused; use that package manager to update")
    for path in reversed((prefix, *prefix.parents)):
        if path.exists() or path.is_symlink():
            check_directory(path)
    return prefix


def make_directory(path):
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    check_directory(path, owned=True)


def link_target(name):
    return f"../lib/{STATE_NAME}/current/{name}"


def check_links(prefix, allow_owned):
    for name in BINARIES:
        link = prefix / "bin" / name
        if not link.exists() and not link.is_symlink():
            continue
        if not allow_owned or not link.is_symlink() or os.readlink(link) != link_target(name):
            raise ReleaseError(f"refusing unrelated or package-manager executable: {link}")


def write_owner(path):
    pending = path / "owner.json.pending"
    with pending.open("x") as output:
        json.dump(OWNER, output, sort_keys=True)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    os.replace(pending, path / "owner.json")
    sync_directory(path)


def check_owner(path):
    check_directory(path, owned=True)
    marker = path / "owner.json"
    owned_file(marker)
    if json.loads(marker.read_text()) != OWNER:
        raise ReleaseError(f"unrecognized installation ownership: {path}")


def owned_file(path):
    regular_file(path)
    info = path.stat()
    if info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o022:
        raise ReleaseError(f"installation file must be owned, unshared and not hard-linked: {path.name}")


def initialize(prefix):
    state = prefix / "lib" / STATE_NAME
    check_links(prefix, state.exists() and not state.is_symlink())
    for directory in (prefix, prefix / "bin", prefix / "lib"):
        make_directory(directory)
    if not state.exists() and not state.is_symlink():
        temporary = Path(tempfile.mkdtemp(prefix=".mike-init-", dir=state.parent))
        try:
            write_owner(temporary)
            (temporary / "generations").mkdir(mode=0o700)
            sync_directory(temporary)
            try:
                temporary.rename(state)
                sync_directory(state.parent)
            except FileExistsError:
                pass
        finally:
            if temporary.exists():
                shutil.rmtree(temporary)
    check_owner(state)
    check_directory(state / "generations", owned=True)
    return state


@contextmanager
def installation_lock(state):
    descriptor = os.open(state / "lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "r+") as lock:
        info = os.fstat(lock.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ReleaseError("unsafe installation lock")
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise ReleaseError("another installer is using this prefix; retry after it finishes") from exc
        yield


def generation_files(manifest=None):
    paths = {"owner.json", "manifest.json", *BINARIES}
    for name in BINARIES:
        paths.update((name + "-notices", name + "-notices/LICENSE", name + "-notices/NOTICE.json"))
    if manifest is None:
        paths.update(SUPPORT_FILES)
    else:
        artifacts = manifest["artifacts"]
        if not isinstance(artifacts, list) or sorted(a["binary"] for a in artifacts) != sorted(BINARIES):
            raise ReleaseError("installed generation must contain the exact binary pair")
        for artifact in artifacts:
            inventory = checked_inventory(artifact["binary"], artifact["contents"])
            if "THIRD-PARTY-NOTICES.md" in inventory:  # absent from older candidates
                paths.add(artifact["binary"] + "-notices/THIRD-PARTY-NOTICES.md")
            assets = {path for path in inventory if is_asset(path)}
            paths.update(set(inventory) & set(SUPPORT_FILES))
            paths.update(assets | parents(assets))
    return paths


def clean_pending(stage):
    if not stage.exists() and not stage.is_symlink():
        return
    check_directory(stage, owned=True)
    entries = list(stage.iterdir())
    if not entries:
        stage.rmdir()
        return
    if not (stage / "owner.json").exists() and not (stage / "owner.json").is_symlink():
        # A kill before atomic ownership receipt commit can leave only its
        # bounded temporary receipt, never extracted or unrelated content.
        if len(entries) == 1 and entries[0].name == "owner.json.pending":
            regular_file(entries[0])
            entries[0].unlink()
            stage.rmdir()
            return
    check_owner(stage)
    # Interrupted extraction may have the temporary .mike-notices shape too.
    manifest_path = stage / "manifest.json"
    manifest = None
    if manifest_path.exists():
        owned_file(manifest_path)
        manifest = json.loads(manifest_path.read_text())
    allowed = generation_files(manifest)
    assets = {path for path in allowed if path == "share" or path.startswith("share/")}
    allowed.update(".mike/" + path for path in assets)
    for name in BINARIES:
        allowed.update(("." + name, "." + name + "/" + name,
                        "." + name + "/LICENSE", "." + name + "/NOTICE.json"))
    allowed.update(".mike/" + name for name in SUPPORT_FILES)
    for path in stage.rglob("*"):
        if path.is_symlink() or path.relative_to(stage).as_posix() not in allowed:
            raise ReleaseError("unrecognized pending installation content; refusing cleanup")
        if not path.is_file() and not path.is_dir():
            raise ReleaseError("unsafe pending installation content; refusing cleanup")
        if path.is_dir():
            check_directory(path, owned=True)
        else:
            owned_file(path)
    shutil.rmtree(stage)


def validate_generation(generation, identity):
    if not re.fullmatch(r"[0-9a-f]{64}", identity):
        raise ReleaseError("invalid installed generation identity")
    check_owner(generation)
    manifest_path = generation / "manifest.json"
    owned_file(manifest_path)
    if sha256(manifest_path) != identity:
        raise ReleaseError("installed manifest has changed; refusing replacement")
    manifest = json.loads(manifest_path.read_text())
    observed = {p.relative_to(generation).as_posix() for p in generation.rglob("*")}
    if observed != generation_files(manifest):
        raise ReleaseError("installed generation contains unexpected or missing files")
    for path in generation.rglob("*"):
        if path.is_symlink():
            raise ReleaseError("installed generation contains a symlink")
        if path.is_dir():
            check_directory(path, owned=True)
    for artifact in manifest["artifacts"]:
        for item in artifact["contents"]:
            path = generation / (item["path"] if item["path"] == artifact["binary"] or item["path"] in SUPPORT_FILES or is_asset(item["path"]) else
                                 artifact["binary"] + "-notices/" + item["path"])
            owned_file(path)
            if (sha256(path) != item["sha256"] or path.stat().st_size != item["size"]
                    or stat.S_IMODE(path.stat().st_mode) != int(item["mode"], 8)):
                raise ReleaseError(f"installed file changed; refusing replacement: {path.name}")
    return manifest


def current_generation(state):
    current = state / "current"
    if not current.exists() and not current.is_symlink():
        return None, None
    if not current.is_symlink():
        raise ReleaseError("current installation is not an owned generation link")
    target = os.readlink(current)
    if not re.fullmatch(r"generations/[0-9a-f]{64}", target):
        raise ReleaseError("current installation points outside owned generations")
    return target.split("/")[1], validate_generation(state / target, target.split("/")[1])


def commit_generation(prefix, state, identity):
    check_links(prefix, True)
    for name in BINARIES:
        link = prefix / "bin" / name
        if not link.is_symlink():
            link.symlink_to(link_target(name))
    sync_directory(prefix / "bin")
    pending = state / "next"
    if pending.exists() or pending.is_symlink():
        if not pending.is_symlink() or not re.fullmatch(r"generations/[0-9a-f]{64}", os.readlink(pending)):
            raise ReleaseError("unrecognized pending generation link; refusing replacement")
        pending.unlink()
    pending.symlink_to("generations/" + identity)
    sync_directory(state)
    os.replace(pending, state / "current")
    sync_directory(state)
