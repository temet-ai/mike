"""The one identity file of this machine, shared with the proxy.

The proxy (crates/mike-base/src/machine_identity.rs) reads and creates the same file, so every
Python tool and the proxy register checkouts under one machine_uuid.
"""
import json
import os
from pathlib import Path
import socket
import uuid

from .collector import atomic_write

FILE_NAME = 'machine.json'


def default_state_dir(env=None, home=None):
    """XDG_STATE_HOME/mike, or ~/.local/state/mike when XDG_STATE_HOME is unset or empty."""
    env = os.environ if env is None else env
    base = env.get('XDG_STATE_HOME')
    if base:
        return Path(base) / 'mike'
    return (Path(home) if home else Path.home()) / '.local' / 'state' / 'mike'


def machine_identity(state_dir=None):
    """Read machine.json from the state directory, creating it on first use."""
    path = Path(state_dir if state_dir is not None else default_state_dir()) / FILE_NAME
    if not path.exists():
        atomic_write(path, json.dumps({'machine_uuid': str(uuid.uuid4()),
                                       'machine_name': socket.gethostname()}, indent=2) + '\n')
    return json.loads(path.read_text())
