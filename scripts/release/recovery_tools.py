"""Explicit host-tool selection without trusting ambient PATH or writable links."""

import os
from pathlib import Path
import stat

from common import ReleaseError


def selected_tools(directory, names):
    """Freeze host command names once; container tools still resolve inside Docker."""
    if directory is None:
        return {name: name for name in names}
    directory = Path(directory)
    if not directory.is_absolute() or ".." in directory.parts:
        raise ReleaseError("tool directory must be an explicit absolute physical directory")
    try:
        # Walk without resolving links. Root-owned sticky temporary ancestors are
        # safe only when every child down to the tools is independently owned.
        # Never permit a shared-writable selected directory, even with sticky bit.
        for path in reversed((directory, *directory.parents)):
            info = path.lstat()
            sticky_ancestor = (path != directory and info.st_uid == 0 and
                               info.st_mode & stat.S_ISVTX)
            if (not stat.S_ISDIR(info.st_mode) or info.st_uid not in (0, os.getuid()) or
                    (info.st_mode & 0o022 and not sticky_ancestor)):
                raise ReleaseError("tool directory ancestors must be plain, trusted and not shared-writable")
        result = {}
        for name in names:
            path = directory / name
            info = path.lstat()
            if (not stat.S_ISREG(info.st_mode) or info.st_uid not in (0, os.getuid()) or
                    info.st_nlink != 1 or info.st_mode & 0o6022 or not os.access(path, os.X_OK)):
                raise ReleaseError("selected tools must be executable, plain trusted files without shared writes or privilege bits")
            result[name] = str(path)
        return result
    except OSError as exc:
        raise ReleaseError("selected tool directory is missing required readable executable tools") from exc
