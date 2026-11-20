#!/usr/bin/env bash
# rust-toolchain.sh — make sure the Rust toolchain pinned by rust-toolchain.toml is
# installed before the first cargo call.
#
# rustup 1.29 still installs a missing pinned toolchain the first time cargo (or rustc)
# runs, but warns that this auto-install is deprecated and may stop working
# (https://github.com/rust-lang/rustup/issues/4836). Every script that builds mike calls
# `ensure_pinned_rust_toolchain` first instead: scripts/cx, start-proxy.sh,
# deploy-proxy.sh, build-local-release, deploy-archive-build.sh, release.sh and, through
# ensure_build_toolchain, the make targets and session-up.sh.
#
#   ensure_pinned_rust_toolchain [<root>]
#     <root> is the directory holding rust-toolchain.toml; default: the checkout this
#     file belongs to. rustup resolves the pin from the directory it runs in, so the
#     check and the install run there (a worktree has its own rust-toolchain.toml).
#
# Behaviour:
#   - no rust-toolchain.toml in <root>              -> nothing to install, returns 0
#   - toolchain installed (checked without network) -> returns 0 in milliseconds
#   - toolchain missing                             -> `rustup toolchain install`, which
#                                                      with no names installs the active
#                                                      toolchain of rust-toolchain.toml
#                                                      with its listed components
#   - no rustup, cargo on PATH (distro cargo)       -> returns 0: nothing rustup could
#                                                      install, and the pin is not ours
#   - no rustup, no cargo                           -> message on stderr, returns 1
#
# `--no-self-update` keeps rustup itself as it is; `--no-update` stops it checking the
# network for a newer build of an installed toolchain (a pinned channel has none).
# RUSTUP_AUTO_INSTALL=0 makes the presence check fail instead of installing.

ensure_pinned_rust_toolchain() {
  local root="${1:-}"
  [[ -n "$root" ]] || root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  [[ -f "$root/rust-toolchain.toml" || -f "$root/rust-toolchain" ]] || return 0

  if ! command -v rustup >/dev/null 2>&1; then
    command -v cargo >/dev/null 2>&1 && return 0
    echo "✗ Neither rustup nor cargo is on PATH, and $root/rust-toolchain.toml pins a Rust toolchain." >&2
    echo "  Install rustup (https://rustup.rs, or \`brew install rustup\`), then retry." >&2
    return 1
  fi

  (cd "$root" && RUSTUP_AUTO_INSTALL=0 rustup show active-toolchain >/dev/null 2>&1) && return 0

  echo "==> Installing the Rust toolchain pinned by $root/rust-toolchain.toml" >&2
  if ! (cd "$root" && rustup toolchain install --no-self-update --no-update >&2); then
    echo "✗ \`rustup toolchain install\` failed in $root; the build cannot continue." >&2
    return 1
  fi
}

# Run standalone (`make rust-toolchain`): install the pinned toolchain of this checkout.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  set -euo pipefail
  ensure_pinned_rust_toolchain "${1:-}"
fi
