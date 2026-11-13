"""The crates split from crate mike (docs/adr/0002), read from one list: the
"packages" object of docs/adr/0002-crate-map.json, package name -> the target
crates of the map it holds, bottom to top. Adding a crate means adding it there."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAP = ROOT / "docs" / "adr" / "0002-crate-map.json"


def packages():
    with open(MAP, encoding="utf-8") as handle:
        return json.load(handle)["packages"]


def names():
    return tuple(packages())


def src_prefix_pattern():
    """A regex alternation of the packages' src/ prefixes, e.g. `crates/(?:mike|mike-base)/src/`."""
    return "crates/(?:" + "|".join(sorted(names(), key=len, reverse=True)) + ")/src/"
