#!/usr/bin/env python3
"""Render the tap formulas of a published alpha release from its checksums.txt.

The release workflow runs this after it publishes the archives, so the formulas pin
the bytes it uploaded. It is not the qualified tap route of homebrew.py, which stays
the gated path; no Homebrew, Ruby or network is used here.
"""

import argparse
import json
from pathlib import Path
import re
import sys
from urllib.parse import quote

from common import ReleaseError
from homebrew_proposal import DESCRIPTIONS, FORMULAE, formula_methods

MAC_TARGET = "aarch64-apple-darwin"
# Homebrew block inside `on_linux` -> release target.
LINUX_TARGETS = (("on_arm", "aarch64-unknown-linux-gnu"), ("on_intel", "x86_64-unknown-linux-gnu"))
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+-alpha\.[0-9]+")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")
CHECKSUM_LINE = re.compile(r"([a-fA-F0-9]{64})\s+\*?(\S+)")


def parse_checksums(text):
    digests = {}
    for line in text.splitlines():
        match = CHECKSUM_LINE.fullmatch(line)
        if not match:
            raise ReleaseError(f"invalid checksum line: {line!r}")
        digest, filename = match.groups()
        digests[filename.removeprefix("./")] = digest.lower()
    return digests


def render(name, version, repository, digests):
    if not VERSION.fullmatch(version):
        raise ReleaseError("version must be an alpha SemVer, such as 0.1.0-alpha.1")
    if not REPOSITORY.fullmatch(repository):
        raise ReleaseError("release repository must be owner/repo")
    tag = f"v{version}"
    lines = [f"class {FORMULAE[name]} < Formula", f"  desc {json.dumps(DESCRIPTIONS[name])}",
             f'  homepage "https://github.com/{repository}"', '  license "MIT"', f'  version "{version}"']


    def archive(target, indent):
        filename = f"{name}-{tag}-{target}.tar.gz"
        if filename not in digests:
            raise ReleaseError(f"missing {filename} in checksums")
        url = f"https://github.com/{repository}/releases/download/{quote(tag, safe='')}/{quote(filename, safe='')}"
        return [f"{indent}url {json.dumps(url)}", f'{indent}sha256 "{digests[filename]}"']

    lines.extend(["", "  on_macos do", "    depends_on arch: :arm64", *archive(MAC_TARGET, "    "), "  end",
                  "", "  on_linux do"])
    for block, target in LINUX_TARGETS:
        lines.extend([f"    {block} do", *archive(target, "      "), "    end"])
    lines.extend(["  end", "", *formula_methods(name, version), "end", ""])
    return "\n".join(lines)


def write_formulas(version, repository, checksums, output):
    digests = parse_checksums(checksums.read_text())
    formulas = {name: render(name, version, repository, digests) for name in FORMULAE}
    output.mkdir(parents=True, exist_ok=True)
    for name, content in formulas.items():
        (output / f"{name}.rb").write_text(content)
    return sorted(formulas)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", help="SemVer without the leading v, such as 0.1.0-alpha.1")
    parser.add_argument("release_repo", help="public owner/repo that holds the release assets")
    parser.add_argument("checksums", type=Path, help="checksums.txt of the release assets")
    parser.add_argument("output", type=Path, help="folder that receives mike.rb and mike-mcp.rb (the tap's Formula)")
    args = parser.parse_args(argv)
    try:
        for name in write_formulas(args.version, args.release_repo, args.checksums, args.output):
            print(args.output / f"{name}.rb")
    except (ReleaseError, OSError) as error:
        print(f"homebrew_alpha.py: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
