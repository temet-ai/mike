#!/usr/bin/env python3
"""Select and prune Cargo artifacts while holding a profile's Cargo locks."""
import contextlib
import fcntl
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

LOCKS = ('.cargo-lock', '.cargo-build-lock', '.cargo-artifact-lock')
OBJECT = re.compile(r'.+-[0-9a-f]{16}\..+\.rcgu\.o$')
HASH = re.compile(r'-[0-9a-f]{16}')
INCREMENTAL = re.compile(r'-[0-9a-z]{11,13}$')
MACHO = {bytes.fromhex(value) for value in (
    'feedface', 'cefaedfe', 'feedfacf', 'cffaedfe',
    'cafebabe', 'bebafeca', 'cafebabf', 'bfbafeca',
)}


def superseded(folder, age):
    if not folder.is_dir():
        return []
    entries = sorted(folder.iterdir(), key=lambda path: path.lstat().st_mtime, reverse=True)
    seen, old = set(), []
    for path in entries:
        if path.is_symlink() or OBJECT.fullmatch(path.name):
            continue
        match = HASH.search(path.name) or INCREMENTAL.search(path.name)
        if not match:
            continue
        key = path.name[:match.start()] + '|' + path.name[match.end():]
        if key in seen and (age == 0 or time.time() - path.stat().st_mtime >= (age + 1) * 86400):
            old.append(path)
        seen.add(key)
    return old


def macho_references(profile):
    """macOS unpacked DWARF lives in rcgu.o files named by linked binaries."""
    referenced, inspected = set(), set()
    def fail(error):
        raise error

    for folder, dirs, files in os.walk(profile, followlinks=False, onerror=fail):
        dirs[:] = [name for name in dirs if name not in {'.fingerprint', 'incremental'}
                   and not Path(folder, name).is_symlink() and not name.endswith('.dSYM')]
        for name in files:
            path = Path(folder, name)
            if path.is_symlink() or path.suffix in {'.o', '.rlib', '.rmeta', '.d'}:
                continue
            stat = path.stat()
            identity = (stat.st_dev, stat.st_ino)
            if identity in inspected or not path.is_file():
                continue
            inspected.add(identity)
            with path.open('rb') as stream:
                if stream.read(4) not in MACHO:
                    continue
            result = subprocess.run(['nm', '-ap', str(path)], capture_output=True,
                                    text=True, errors='replace', timeout=60)
            if result.returncode:
                raise RuntimeError(f'cannot inspect debug references in {path}: {result.stderr.strip()}')
            for line in result.stdout.splitlines():
                match = re.search(r'\sOSO\s+(.+)$', line)
                if not match:
                    continue
                archive = re.fullmatch(r'(.+\.(?:rlib|a))\(.+\)', match[1])
                if archive:
                    referenced.add(Path(archive[1]).resolve())
                elif match[1].endswith('.o'):
                    referenced.add(Path(match[1]).resolve())
    return referenced


def unused_objects(profile, age):
    deps = profile / 'deps'
    if profile.name == 'debugging' or not deps.is_dir():
        return []
    candidates = [path for path in deps.iterdir()
                  if OBJECT.fullmatch(path.name) and path.is_file() and not path.is_symlink()
                  and (age == 0 or time.time() - path.stat().st_mtime >= (age + 1) * 86400)]
    if not candidates:
        return []
    return candidates


def remove(paths, dry):
    for path in paths:
        if dry:
            continue
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()
    return len(paths)


def sweep(profile, mode, age, dry):
    with contextlib.ExitStack() as stack:
        for name in LOCKS:
            path = profile / name
            if path.is_symlink():
                raise RuntimeError(f'refusing symlinked Cargo lock: {path}')
            if dry and not path.exists():
                continue
            handle = stack.enter_context(path.open('rb' if dry else 'ab'))
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # Select only after acquiring the locks: a just-finished build may have
        # replaced the files that a scan before locking thought were obsolete.
        if mode == 'incremental':
            paths = [profile / 'incremental'] if (profile / 'incremental').is_dir() else []
            return (0, 0, remove(paths, dry))
        obsolete = [path for name in ('deps', '.fingerprint', 'build', 'incremental')
                    for path in superseded(profile / name, age)]
        objects = unused_objects(profile, age)
        # Preserve both unpacked objects and archives containing debug objects,
        # even when their crate/hash would otherwise be treated as superseded.
        references = macho_references(profile) if sys.platform == 'darwin' and (obsolete or objects) else set()
        protected = references | {parent for path in references for parent in path.parents}
        obsolete = [path for path in obsolete if path.resolve() not in protected]
        canonical_deps = (profile / 'deps').resolve()
        objects = [path for path in objects if canonical_deps / path.name not in references]
        return (remove(obsolete, dry), remove(objects, dry), 0)


def main():
    profile, mode, age, dry = sys.argv[1:]
    try:
        counts = sweep(Path(profile), mode, int(age), dry == '1')
    except BlockingIOError:
        print('0 0 0')
        return 75
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f'cargo-sweep: {error}', file=sys.stderr)
        print('0 0 0')
        return 1
    print(*counts)
    return 0


if __name__ == '__main__':
    sys.exit(main())
