"""Read-only native runc prerequisite; no executing or repairing runtime binaries."""
import hashlib
import os
import re
from pathlib import Path
import stat
import struct

from journey_process import JourneyError, require

MAX_RUNTIME_BYTES = 64 * 1024 ** 2


def selected_runtime(name):
    require(isinstance(name, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,63}', name)
            and name not in ('runc', 'default'), 'ISOLATED_EXPLICIT_RUNTIME_ALIAS_REQUIRED')
    return name


def runtime_path(info, runtime_name):
    selected_runtime(runtime_name)
    runtimes = info.get('Runtimes')
    require(isinstance(runtimes, dict) and isinstance(runtimes.get(runtime_name), dict),
            'ISOLATED_RUNC_EXPLICIT_PATH_REQUIRED')
    value = runtimes[runtime_name]
    path = value.get('path')
    require(isinstance(path, str) and path.startswith('/') and os.path.normpath(path) == path
            and not value.get('runtimeArgs') and not value.get('runtimeType') and not value.get('options'),
            'ISOLATED_RUNC_EXPLICIT_PATH_REQUIRED')
    return Path(path)


def metadata(value):
    return (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns, value.st_ctime_ns,
            value.st_uid, value.st_gid, value.st_mode)


def native_elf(header):
    require(len(header) == 64 and header[:7] == b'\x7fELF\x02\x01\x01'
            and struct.unpack_from('<H', header, 16)[0] in (2, 3)
            and struct.unpack_from('<H', header, 18)[0] == 62
            and struct.unpack_from('<I', header, 20)[0] == 1
            and struct.unpack_from('<H', header, 52)[0] == 64,
            'ISOLATED_RUNC_NATIVE_AMD64_ELF_REQUIRED')


def root_owned(value, *, directory=False):
    require(value.st_uid == 0 and not value.st_mode & 0o022,
            'ISOLATED_RUNC_ROOT_OWNERSHIP_REQUIRED')
    require(stat.S_ISDIR(value.st_mode) if directory else stat.S_ISREG(value.st_mode)
            and bool(value.st_mode & 0o111), 'ISOLATED_RUNC_TRUSTED_EXECUTABLE_REQUIRED')


def trusted_path(path):
    require(not any(part.is_symlink() for part in (path, *path.parents)), 'ISOLATED_RUNC_PATH_SYMLINK')
    for parent in path.parents:
        root_owned(parent.lstat(), directory=True)
    root_owned(path.lstat())


def observe_runtime(path, *, limit=MAX_RUNTIME_BYTES):
    """Hash the same no-follow descriptor whose bounded native header is read."""
    try:
        trusted_path(path)
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor, 'rb') as source:
            value = os.fstat(source.fileno())
            before = metadata(value)
            root_owned(value)
            require(stat.S_ISREG(value.st_mode) and 64 <= value.st_size <= limit
                    and not value.st_mode & 0o022 and os.access(path, os.X_OK), 'ISOLATED_RUNC_TRUSTED_EXECUTABLE_REQUIRED')
            header = source.read(64)
            native_elf(header)
            digest = hashlib.sha256(header)
            remaining = value.st_size - 64
            while remaining:
                block = source.read(min(1024 ** 2, remaining))
                require(bool(block), 'ISOLATED_RUNC_IDENTITY_CHANGED')
                remaining -= len(block)
                digest.update(block)
            require(not source.read(1) and metadata(os.fstat(source.fileno())) == before
                    and metadata(path.lstat()) == before, 'ISOLATED_RUNC_IDENTITY_CHANGED')
            trusted_path(path)
        return {'path': path, 'metadata': before, 'sha256': digest.hexdigest()}
    except (OSError, ValueError, struct.error) as error:
        raise JourneyError('ISOLATED_RUNC_LOCAL_IDENTITY_UNAVAILABLE') from error


def verify_runtime(info, runtime_name, previous=None):
    observed = observe_runtime(runtime_path(info, runtime_name))
    require(previous is None or observed == previous, 'ISOLATED_RUNC_IDENTITY_CHANGED')
    return observed


def runtime_evidence(identity):
    return {'native_runc_elf_verified': True, 'runtime_elf_class': 64,
            'runtime_elf_machine': 62, 'runtime_executable_sha256': identity['sha256']}
