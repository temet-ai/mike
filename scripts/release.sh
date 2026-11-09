#!/usr/bin/env bash
# Prepare a local, unqualified candidate. Never changes versions or publishes.
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
case "${1:-}" in
    --help|-h)
        cat <<'HELP'
Usage: scripts/release.sh [CURRENT_VERSION] [--target TRIPLE] [--glibc FLOOR]
                         [--reviewed-notices /absolute/review/manifest.json]
       scripts/release.sh --verify CANDIDATE_DIRECTORY

Prepare both binaries from a clean, reviewed commit using the current workspace
version. Each target builds on its own host, with one exception: Linux ARM64
builds on x86_64 Linux with --glibc, smoke-tested under qemu-aarch64.
--glibc (Linux only) links against that glibc with cargo-zigbuild, e.g. 2.35.
Tools and dependency caches must already be provisioned; builds are offline.
Archives, relative SHA256 files and a manifest go into ignored dist-out/.
--verify checks and smoke-tests those exact archived bytes without rebuilding.
--reviewed-notices records a source/target-bound offline review and embeds its
license texts; it does not approve rights or authorize publication.

Preparation is not release qualification. Publication (--publish) is unavailable.
See docs/RELEASING.md for prerequisites, remaining gates and local tests.
HELP
        exit 0
        ;;
esac
exec python3 "${SCRIPT_DIR}/release/prepare.py" "$@"
