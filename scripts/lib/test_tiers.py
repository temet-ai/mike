"""Conservative source classification; never equate this inventory with test execution."""
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = re.compile(
    r"\b(?:TestDb(?:Handle)?|acquire_ephemeral_db|MIKE_TEST_ADMIN_URL|"
    r"start_proxy_with_(?:shared_)?db\w*|PgPoolOptions|PgConnection|"
    r"PgPool::connect|PersistHandle::connect\w*|db_helpers|db_cap_helpers|"
    r"filter_proxy|test_db|test_fixture|ownership_test_support|"
    r"store::(?:persistence::)?test_support)\b"
)
TEST_ATTRIBUTE = re.compile(r"#\[\s*(?:[A-Za-z_][\w]*::)*test\b")
PATH_MODULE = re.compile(r'#\[path\s*=\s*"([^"\n]+)"\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*;')


SUFFIX = "(_[a-z0-9_]+)?"


def name_filter(names):
    forms = [name + SUFFIX if "::" in name else name for name in sorted(names)]
    return "test(/^(" + "|".join(forms) + ")::/)"


def module_path(parts):
    names = [Path(part).stem for part in parts]
    if names and names[-1] == "mod":
        names.pop()
    return "::".join(names)


def path_entry_is_fast(path, modules):
    return any(
        "::" in entry and re.fullmatch(re.escape(entry) + SUFFIX + "(::.*)?", path)
        for entry in modules
    )


sys.path.insert(0, str(Path(__file__).resolve().parent))
import split_crates  # noqa: E402

# Crate mike and the crates split from it (docs/adr/0002): their unit-test
# binaries share one module list because the split keeps module paths.
SPLIT_CRATES = tuple(sorted(split_crates.names()))


def db_filter(spec):
    fast = [f"binary(={name})" for name in spec["fast_mike_harnesses"]]
    libs = " | ".join(f"binary(={name.replace('-', '_')})" for name in SPLIT_CRATES)
    fast.append(f"(({libs}) & {name_filter(spec['fast_mike_lib_modules'])})")
    fast.append(f"(binary(=cli) & {name_filter(spec['fast_mike_cli_modules'])})")
    packages = " | ".join(f"package(={name})" for name in SPLIT_CRATES)
    return f"({packages}) & not (" + " | ".join(fast) + ")"


def source_is_fast(path, spec):
    parts = Path(path).parts
    if parts[0] != "crates" or len(parts) < 2 or parts[1] not in SPLIT_CRATES:
        return True
    if len(parts) < 4:
        return False
    area, name = parts[2], Path(parts[3]).stem
    if area == "src":
        modules = spec["fast_mike_lib_modules"]
        return name in modules or path_entry_is_fast(module_path(parts[3:]), modules)
    if area == "tests":
        if name == "cli" and len(parts) > 4:
            return Path(parts[4]).stem in spec["fast_mike_cli_modules"]
        return name in spec["fast_mike_harnesses"]
    return False


def inspect(root, spec):
    errors, records = [], []
    exceptions = spec["fixture_free_exceptions"]
    for relative, exception in exceptions.items():
        path = root / relative
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != exception["sha256"]:
            errors.append(f"{relative}: fixture-free exception changed; review it before updating its fingerprint")
    for path in sorted((root / "crates").rglob("*.rs")):
        relative = str(path.relative_to(root))
        source = path.read_text()
        markers = sorted(set(FIXTURES.findall(source)))
        fast = source_is_fast(relative, spec)
        if markers and fast and relative not in exceptions:
            errors.append(f"{relative}: DB fixture/connection markers in the fast tier: {', '.join(markers)}")
        if fast:
            for imported, imported_markers in imported_fixture_sources(path, root.resolve()):
                imported_relative = (str(imported.relative_to(root.resolve()))
                                     if imported.is_relative_to(root.resolve()) else "<outside checkout>")
                if imported_relative not in exceptions:
                    errors.append(f"{relative}: path-includes DB fixture source {imported_relative} in the fast tier: {', '.join(imported_markers)}")
        if markers or TEST_ATTRIBUTE.search(source):
            records.append({"source": relative, "tier": "fast" if fast else "db",
                            "fixture_markers": markers,
                            "test_attributes": len(TEST_ATTRIBUTE.findall(source)),
                            "ignore_attributes": len(re.findall(r"#\[\s*ignore\b", source))})
    return errors, records


def imported_fixture_sources(path, root, seen=None):
    seen = set() if seen is None else seen
    for target in PATH_MODULE.findall(path.read_text()):
        imported = (path.parent / target).resolve()
        if not imported.is_relative_to(root):
            yield imported, ["external path imports cannot be certified as database-free"]
            continue
        if imported in seen or not imported.is_file():
            continue
        seen.add(imported)
        directory = imported.parent if imported.name == "mod.rs" else imported.with_suffix("")
        sources = [imported, *directory.rglob("*.rs")] if directory.is_dir() else [imported]
        for source in sources:
            markers = sorted(set(FIXTURES.findall(source.read_text())))
            if markers:
                yield source, markers
            yield from imported_fixture_sources(source, root, seen)


def validate_config(root, spec):
    config = tomllib.loads((root / ".config/nextest.toml").read_text())
    profiles, expected = config["profile"], db_filter(spec)
    errors = []
    for profile, expression in [("default", f"not ({expected})"), ("db", expected),
                                ("ci", "all()"), ("timing", "all()")]:
        if profiles[profile]["default-filter"] != expression:
            errors.append(f"nextest {profile} filter differs from the reviewed static partition")
    for profile in ["db", "ci", "timing"]:
        if not any(item.get("filter") == expected and item.get("test-group") == "mike-db-heavy"
                   for item in profiles[profile].get("overrides", [])):
            errors.append(f"nextest {profile} does not throttle the complete DB partition")
    return errors


def write_config(root, spec):
    """Rewrites every copy of the DB partition in nextest.toml to the one the spec and
    the crate list give, e.g. after a crate is added to the map's "packages"."""
    path = root / ".config/nextest.toml"
    text = path.read_text()
    current = tomllib.loads(text)["profile"]["db"]["default-filter"]
    path.write_text(text.replace(json.dumps(current), json.dumps(db_filter(spec))))


def main(args):
    spec = json.loads((ROOT / ".config/test-tiers.json").read_text())
    if "--write" in args:
        write_config(ROOT, spec)
    errors, records = inspect(ROOT, spec)
    errors.extend(validate_config(ROOT, spec))
    if "--require-db" in args and not os.environ.get("MIKE_TEST_ADMIN_URL", "").strip():
        errors.append("required DB tier NOT RUN: export MIKE_TEST_ADMIN_URL for an explicitly owned disposable pgvector instance")
    summary = {
        "evidence": "static source inventory; no Rust tests were executed",
        "fixture_sources": sum(bool(record["fixture_markers"]) for record in records),
        "source_test_attributes": {
            tier: sum(record["test_attributes"] for record in records if record["tier"] == tier)
            for tier in ["fast", "db"]},
        "source_ignore_attributes": sum(record["ignore_attributes"] for record in records),
        "compiled_selected": None, "executed": None, "compiled_excluded": None,
        "errors": errors,
    }
    if "--json" in args:
        print(json.dumps({**summary, "sources": records}, indent=2))
    else:
        print("test-tiers: " + ("FAIL" if errors else "PASS") + " (static classification only)")
        print(json.dumps(summary, indent=2))
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
