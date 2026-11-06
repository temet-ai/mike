"""Direct Unix-daemon identity checks for explicit runtime use; never at import."""
import os
from pathlib import Path
import socket
import stat
import struct

from journey_process import JourneyError, require
from journey_reliability_native_runtime import observe_runtime

MAX_DAEMON_BYTES = 256 * 1024 ** 2


def start_ticks(root):
    with (root / 'stat').open('rb') as source:
        raw = source.read(4097)
    require(len(raw) <= 4096, 'ISOLATED_DAEMON_PROCESS_STAT')
    value = raw.decode('ascii')
    end = value.rfind(') ')
    fields = value[end + 2:].split() if end > 0 else []
    require(len(fields) >= 20 and fields[19].isdigit(), 'ISOLATED_DAEMON_PROCESS_STAT')
    return int(fields[19])


def inode(path):
    value = path.stat()
    return value.st_dev, value.st_ino


def observe_peer(endpoint):
    """Connect only to the selected local socket; send no daemon API request."""
    try:
        before = endpoint.lstat()
        require(stat.S_ISSOCK(before.st_mode) and before.st_uid == 0, 'ISOLATED_DAEMON_ROOT_SOCKET_REQUIRED')
        require(hasattr(socket, 'SO_PEERCRED'), 'ISOLATED_LINUX_PEER_CREDENTIALS_REQUIRED')
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(3)
            connection.connect(str(endpoint))
            raw = connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize('3i'))
            pid, uid, gid = struct.unpack('3i', raw)
            require(pid > 1 and uid == gid == 0, 'ISOLATED_DIRECT_ROOT_DOCKERD_REQUIRED')
            root = Path('/proc') / str(pid)
            started = start_ticks(root)
            executable = Path(os.readlink(root / 'exe'))
            require(executable.is_absolute() and executable.name == 'dockerd', 'ISOLATED_DIRECT_DOCKERD_EXECUTABLE_REQUIRED')
            binary = observe_runtime(executable, limit=MAX_DAEMON_BYTES)
            require(inode(root / "exe") == binary["metadata"][:2], "ISOLATED_DAEMON_EXECUTABLE_CHANGED")
            namespace = os.readlink(root / 'ns/mnt')
            filesystem_root = inode(Path('/'))
            require(namespace == os.readlink('/proc/self/ns/mnt')
                    and inode(root / 'root') == filesystem_root, 'ISOLATED_DAEMON_HOST_FILESYSTEM_VIEW')
            require(start_ticks(root) == started and os.readlink(root / 'exe') == str(executable)
                    and os.readlink(root / 'ns/mnt') == namespace
                    and inode(root / 'exe') == binary['metadata'][:2]
                    and inode(root / 'root') == filesystem_root == inode(Path('/')),
                    'ISOLATED_DAEMON_PROCESS_CHANGED')
        after = endpoint.lstat()
        require((after.st_dev, after.st_ino, after.st_uid, after.st_mode) ==
                (before.st_dev, before.st_ino, before.st_uid, before.st_mode), 'ISOLATED_DAEMON_SOCKET_CHANGED')
        return {'pid': pid, 'started': started, 'executable': binary,
                'mount_namespace': namespace, 'filesystem_root': filesystem_root,
                'socket_identity': (before.st_dev, before.st_ino)}
    except (OSError, ValueError, UnicodeError, struct.error) as error:
        raise JourneyError('ISOLATED_DIRECT_DOCKERD_UNAVAILABLE') from error


def verify_peer(endpoint, previous=None):
    observed = observe_peer(endpoint)
    require(previous is None or previous == observed, 'ISOLATED_DAEMON_PEER_CHANGED')
    return observed
