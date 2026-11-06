#!/usr/bin/env bash
# Prepare a local, unqualified candidate. Never changes versions or publishes.
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
case "${1:-}" in
    --help|-h)
        cat <<'HELP'
Usage: scripts/release.sh [CURRENT_VERSION] [--target NATIVE_TRIPLE]
                         [--reviewed-notices /absolute/review/manifest.json]
       scripts/release.sh --verify CANDIDATE_DIRECTORY

Prepare both binaries from a clean, reviewed commit using the current workspace
version. Linux x86_64 and Apple Silicon macOS must each build on their own host.
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
