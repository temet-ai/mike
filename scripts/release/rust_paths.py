"""Release-only Rust source path remapping; no ambient compiler flag overrides."""

import json
from pathlib import Path

from common import ReleaseError

PATH_DESTINATIONS = {"repository": "/mike", "cargo_home": "/cargo"}


def remap_flags(root, env):
    root = Path(root).absolute()
    cargo_home = Path(env.get("CARGO_HOME") or Path(env.get("HOME") or Path.home()) / ".cargo")
    if not cargo_home.is_absolute():
        cargo_home = root / cargo_home
    prefixes = {}
    for path, destination in ((root, PATH_DESTINATIONS["repository"]),
                              (cargo_home, PATH_DESTINATIONS["cargo_home"])):
        # Cargo and rustc may retain either spelling when a cache is symlinked.
        for prefix in (str(path), str(path.resolve())):
            if prefix in prefixes and prefixes[prefix] != destination:
                raise ReleaseError("repository and Cargo home must have distinct source path prefixes")
            prefixes[prefix] = destination
    # rustc applies the last matching prefix; keep nested roots more specific.
    return [f"--remap-path-prefix={prefix}={prefixes[prefix]}"
            for prefix in sorted(prefixes, key=lambda value: (len(value), value))]


def cargo_path_config(root, env):
    # Cargo merges this array with committed target cfg rustflags. RUSTFLAGS or
    # CARGO_ENCODED_RUSTFLAGS would instead discard the repository's deny lints.
    # A TOML array keeps spaces, quotes and '=' in paths inside one argument.
    return "target.'cfg(all())'.rustflags=" + json.dumps(remap_flags(root, env), ensure_ascii=False)
