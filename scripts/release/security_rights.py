"""Local dependency/notice inventory, never a license-permission assertion."""

import json
import os
import re
import tomllib

from security_common import ReviewError, filename_categories, git, open_regular, reference


def inventory_rights(root, scanner):
    tracked = {p.decode("utf-8", "surrogateescape") for p in git(root, "ls-files", "-z").split(b"\0") if p}
    report = {"status": "NOT_RUN", "meaning": "Inventory only; rights and notice sufficiency need owner/legal review",
              "dependencies": [], "notices": [], "assets_and_fonts": [], "gaps": [],
              "upstream_license_texts": "NOT_FETCHED", "actual_linked_and_bundled_dependency_set": "NOT_VERIFIED"}
    for name in sorted(tracked):
        categories = filename_categories(name)
        if "license_notice" in categories:
            report["notices"].append({"path_ref": reference(name), "status": "UNREVIEWED"})
        if any(category in categories for category in ("font_rights_review", "image_manual_review")):
            report["assets_and_fonts"].append({"path_ref": reference(name), "categories": categories,
                                              "rights": "UNRESOLVED", "notice_mapping": "NOT_RUN"})
    if "Cargo.lock" in tracked:
        try:
            data = bounded_read(root, "Cargo.lock", scanner)
            for package in tomllib.loads(data.decode())["package"]:
                report["dependencies"].append(dependency("cargo", package["name"], package["version"],
                                                          "locked", source_type(package.get("source", ""))))
        except (ReviewError, KeyError, TypeError, ValueError, UnicodeError):
            report["gaps"].append("cargo_lock_unavailable_or_invalid")
    else:
        report["gaps"].append("cargo_lock_missing")
    if "frontend/package.json" in tracked:
        try:
            data = json.loads(bounded_read(root, "frontend/package.json", scanner))
            for field in ("dependencies", "devDependencies", "optionalDependencies"):
                for name, version in data.get(field, {}).items():
                    report["dependencies"].append(dependency("npm", name, str(version), field, "manifest"))
        except (ReviewError, KeyError, TypeError, ValueError):
            report["gaps"].append("frontend_manifest_unavailable_or_invalid")
    else:
        report["gaps"].append("frontend_manifest_missing")
    if "frontend/pnpm-lock.yaml" in tracked:
        try:
            parse_pnpm(bounded_read(root, "frontend/pnpm-lock.yaml", scanner).decode(), report)
        except (ReviewError, ValueError, UnicodeError):
            report["gaps"].append("pnpm_lock_unavailable_or_unsupported")
    else:
        report["gaps"].append("pnpm_lock_missing")
    for gap in report["gaps"]:
        scanner.gap(gap, "rights_inventory")
    report["dependency_count"] = len(report["dependencies"])
    return report


def bounded_read(root, name, scanner):
    with open_regular(root, name) as stream:
        limit = min(scanner.limits["file_bytes"], 16 * 1024 * 1024)
        if os.fstat(stream.fileno()).st_size > limit:
            raise ReviewError("manifest_too_large")
        data = stream.read(limit + 1)
        if len(data) > limit:
            raise ReviewError("manifest_too_large")
        return data


def dependency(ecosystem, name, version, scope, source):
    return {"ecosystem": ecosystem, "package_ref": reference(name), "version_ref": reference(version),
            "identity_ref": reference(ecosystem + ":" + name + "@" + version), "scope": scope,
            "source_kind": source, "category": "font" if name.startswith("@fontsource") else "dependency",
            "license": "UNVERIFIED", "rights": "UNRESOLVED", "shipped": "NOT_VERIFIED",
            "required_notice_mapping": "NOT_RUN"}


def source_type(source):
    if source.startswith("registry+"):
        return "registry"
    if source.startswith("git+"):
        return "git"
    return "workspace_or_path"


def parse_pnpm(text, report):
    """Recognize package keys from pnpm v9 only; not a general YAML parser."""
    if not re.search(r"(?m)^lockfileVersion: ['\"]?9\.0['\"]?\s*$", text):
        raise ValueError("unsupported_pnpm_lock_version")
    inside, count = False, 0
    for line in text.splitlines():
        if line == "packages:":
            inside = True
            continue
        if inside and line and not line[0].isspace():
            break
        if not inside or not line.startswith("  ") or line.startswith("   ") or not line.strip():
            continue
        match = re.fullmatch(r"  (?:'([^']+)'|\"([^\"]+)\"|([^:'\"]+)):\s*", line)
        if not match:
            raise ValueError("unsupported_pnpm_package_key")
        key = next(value for value in match.groups() if value is not None)
        name, separator, version = key.rpartition("@")
        if not name or not separator or not version or any(character in key for character in "&*!{}[]"):
            raise ValueError("unsupported_pnpm_package_identity")
        report["dependencies"].append(dependency("npm", name, version, "locked", "lockfile"))
        count += 1
    if not inside or not count:
        raise ValueError("pnpm_package_inventory_missing")
