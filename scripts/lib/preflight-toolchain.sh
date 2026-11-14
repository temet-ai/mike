#!/usr/bin/env bash
# preflight-toolchain.sh — ensure the build accelerators the repo's tooling
# opts in to (sccache, lld) are present, and install them if they are not.
#
# `.cargo/config.toml` itself is stock (a fresh clone builds with plain
# `cargo`). Accelerated callers (scripts/cx, Makefile build/test/lint/install
# targets) inject these via `cargo --config` at invocation time — see
# `compute_accelerator_cargo_args` below:
#   - sccache : `build.rustc-wrapper` (all platforms). Missing -> every cargo
#               invocation dies with `error: process didn't start ... sccache:
#               No such file or directory`.
#   - lld     : linker on `[target.*-apple-darwin]` (macOS only). Missing ->
#               the link step fails. CRITICAL: injects ld64.lld (the Mach-O
#               driver), NOT ld.lld (the ELF driver, which fails with
#               `unable to find library -lSystem`). lld ships in the standalone
#               `brew install lld` formula — `llvm` no longer bundles it.
#
# Source this file and call `ensure_build_toolchain`, or run it standalone
# (`scripts/lib/preflight-toolchain.sh`) — that's what `make bootstrap` does.
#
# Behaviour when a tool is missing:
#   - Homebrew present -> auto-install it (this is the "handle it for the user"
#     path). Set MIKE_SKIP_TOOLCHAIN_BOOTSTRAP=1 to warn-and-continue instead
#     (e.g. CI that provisions the toolchain its own way).
#   - Homebrew absent  -> print manual instructions and return non-zero.
#
# Idempotent and fast: when both tools are present it is two existence tests and
# returns 0, so it is safe to call on every build (cx, start-proxy, make).

# Absolute ld64.lld path injected via --config, by arch. Keep in sync with
# compute_accelerator_cargo_args below.
_expected_lld_path() {
  if [[ "$(uname -m)" == "x86_64" ]]; then
    echo "/usr/local/opt/lld/bin/ld64.lld"   # Intel Homebrew prefix
  else
    echo "/opt/homebrew/opt/lld/bin/ld64.lld" # Apple Silicon Homebrew prefix
  fi
}

ensure_build_toolchain() {
  local need=()

  # sccache is opt-in (CX_SCCACHE=1, see compute_accelerator_cargo_args), so only
  # that case and `make bootstrap` (--all) require it; a machine without it builds.
  if [[ "${CX_SCCACHE:-0}" == "1" || "${PREFLIGHT_ALL:-0}" == "1" ]]; then
    command -v sccache >/dev/null 2>&1 || need+=(sccache)
  fi

  # lld is only applied by .cargo/config.toml on apple-darwin targets.
  local lld_path=""
  if [[ "$(uname -s)" == "Darwin" ]]; then
    lld_path="$(_expected_lld_path)"
    [[ -x "$lld_path" ]] || need+=(lld)
  fi

  [[ ${#need[@]} -eq 0 ]] && return 0

  if [[ "${MIKE_SKIP_TOOLCHAIN_BOOTSTRAP:-0}" == "1" ]]; then
    printf '⚠  Missing build accelerators: %s (MIKE_SKIP_TOOLCHAIN_BOOTSTRAP=1 set; continuing — the accelerated build may fail)\n' \
      "${need[*]}" >&2
    return 0
  fi

  if ! command -v brew >/dev/null 2>&1; then
    cat >&2 <<EOF
✗ Missing build accelerators requested by scripts/cx / make: ${need[*]}
  Homebrew was not found, so they cannot be installed automatically.

  Install manually, then retry:
    - sccache:  cargo install sccache   (or your package manager)
    - lld:      install LLVM lld. On macOS this needs ld64.lld (the
                Mach-O driver) at $(_expected_lld_path)

  Or, to build WITHOUT the accelerators, run plain \`cargo\` directly instead
  of scripts/cx / make — .cargo/config.toml itself never requires them.
EOF
    return 1
  fi

  printf '==> Installing missing build accelerators via Homebrew: %s\n' "${need[*]}" >&2
  if ! brew install "${need[@]}"; then
    printf '✗ `brew install %s` failed. Install manually and retry.\n' "${need[*]}" >&2
    return 1
  fi

  # Verify lld landed at the exact path the config references — a brew prefix
  # mismatch would otherwise surface later as a confusing link failure.
  if [[ -n "$lld_path" && ! -x "$lld_path" ]]; then
    local actual; actual="$(brew --prefix lld 2>/dev/null)/bin/ld64.lld"
    cat >&2 <<EOF
✗ lld installed, but ld64.lld is not at the path this script expects:
    expected: $lld_path
    actual:   $actual
  Update _expected_lld_path() in scripts/lib/preflight-toolchain.sh to match.
EOF
    return 1
  fi

  printf '✓ Build accelerators ready.\n' >&2
  return 0
}

# Host cargo target triple for the accelerator rustflags injection below.
# Assumes the host triple == the build triple (no cross-compiling here).
_host_target_triple() {
  local host
  host="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
  if [[ -n "$host" ]]; then
    echo "$host"
  elif [[ "$(uname -s)" == "Darwin" && "$(uname -m)" == "x86_64" ]]; then
    echo "x86_64-apple-darwin"
  elif [[ "$(uname -s)" == "Darwin" ]]; then
    echo "aarch64-apple-darwin"
  else
    echo "$(uname -m)-unknown-linux-gnu"
  fi
}

# Populates the global array ACCELERATOR_CARGO_ARGS with `--config key=value`
# pairs for whichever accelerators this machine actually has (call
# ensure_build_toolchain first to install what's missing). Callers splice
# this into a cargo invocation: cargo "${ACCELERATOR_CARGO_ARGS[@]}" <args>.
# Uses `--config`, never the RUSTFLAGS env var — RUSTFLAGS *replaces* config
# rustflags wholesale and would silently drop the committed clippy-deny flags;
# --config merges. sccache is incompatible with incremental compilation, so
# build.incremental=false rides along whenever it's injected — which is why it
# is opt-in (CX_SCCACHE=1): sccache can only hit on an unchanged crate, and
# crates/mike is one 194k-line crate, so in the edit-build-test loop it turned
# every edit into a full non-incremental recompile (issue temet-ai/mike-private#341). Turn it on for
# a fresh checkout whose dependencies are not built yet; leave it off to edit.
compute_accelerator_cargo_args() {
  ACCELERATOR_CARGO_ARGS=()

  if [[ "${CX_SCCACHE:-0}" == "1" ]] && command -v sccache >/dev/null 2>&1; then
    ACCELERATOR_CARGO_ARGS+=(--config "build.rustc-wrapper=\"sccache\"" --config "build.incremental=false")
  fi

  local triple
  triple="$(_host_target_triple)"
  if [[ "$(uname -s)" == "Darwin" ]]; then
    local lld_path
    lld_path="$(_expected_lld_path)"
    if [[ -x "$lld_path" ]]; then
      ACCELERATOR_CARGO_ARGS+=(--config "target.${triple}.rustflags=[\"-C\",\"link-arg=-fuse-ld=${lld_path}\"]")
    fi
  elif command -v ld.lld >/dev/null 2>&1; then
    ACCELERATOR_CARGO_ARGS+=(--config "target.${triple}.rustflags=[\"-C\",\"link-arg=-fuse-ld=lld\"]")
  fi
}

# mbx_bin_path: prints the mbx binary to use, or nothing when there is none.
# MBX_BIN wins when set (a set but missing MBX_BIN means none); then mbx on
# PATH; then the trial install.
mbx_bin_path() {
  local bin="${MBX_BIN:-}"
  if [[ -n "$bin" ]]; then
    [[ -x "$bin" ]] && printf '%s\n' "$bin"
    return 0
  fi
  bin="$(command -v mbx 2>/dev/null || true)"
  if [[ -z "$bin" && -x /mnt/data/mbx-trial/bin/mbx ]]; then
    bin=/mnt/data/mbx-trial/bin/mbx
  fi
  [[ -n "$bin" ]] && printf '%s\n' "$bin"
  return 0
}

# The cargo subcommands mbx caches. Anything else mbx runs as plain cargo, and
# plain cargo must never write into an mbx build directory (read-only hard
# links into the store), so those are never sent through mbx. nextest builds its
# test binaries through mbx (scripts/cx sets CARGO to mbx) and only runs them.
MBX_CACHED_SUBCOMMANDS=" build check clippy test nextest "
# What scripts/cx sends through mbx without being asked: all of them, except build
# when MIKE_BUILD_IDENTITY=git asks for the commit to be compiled in (make install,
# the release scripts), which plain cargo in target/ does.
MBX_AUTO_SUBCOMMANDS=" build check clippy test nextest "

# cx_cargo_subcommand <cargo args…>: the cargo subcommand, skipping a leading
# +toolchain. Prints nothing when the first word is an option.
cx_cargo_subcommand() {
  local first="${1:-}"
  [[ "$first" == +* ]] && first="${2:-}"
  [[ "$first" == -* ]] && first=""
  printf '%s\n' "$first"
}

# cx_mbx_slot: the per-session build directory name under target/: mbx-<id>,
# from MIKE_SESSION_ID, else CLAUDE_CODE_SESSION_ID, else mbx-default.
cx_mbx_slot() {
  local id="${MIKE_SESSION_ID:-${CLAUDE_CODE_SESSION_ID:-}}"
  id="$(printf '%s' "$id" | tr -cd 'A-Za-z0-9_-' | cut -c1-12)"
  printf 'mbx-%s\n' "${id:-default}"
}

# select_build_cache <repo_root> <named_target_dir> <subcommand>
# Decides whether scripts/cx runs cargo or mbx. Sets CX_CACHE_MODE to mbx or
# cargo and, for mbx, CX_MBX_TARGET to the build directory. Returns non-zero
# with a message on stderr for a combination that would let plain cargo write
# into an mbx build directory.
#   MIKE_BUILD_CACHE=mbx    mbx, into the named directory (any cached subcommand)
#   MIKE_BUILD_CACHE=cargo  plain cargo
#   unset                   mbx for check, clippy, test, nextest and build (not with
#                           MIKE_BUILD_IDENTITY=git) when mbx is installed and no
#                           other build directory was named; else cargo
select_build_cache() {
  local root="$1" named="$2" sub="$3" dir="" in_mbx=0
  CX_CACHE_MODE=cargo
  CX_MBX_TARGET=""
  if [[ -n "$named" ]]; then
    dir="$named"
    [[ "$dir" == /* ]] || dir="$root/$dir"
    while [[ "$dir" == */ && "$dir" != "/" ]]; do dir="${dir%/}"; done
    [[ "$(basename "$dir")" == mbx-* ]] && in_mbx=1
  fi
  case "${MIKE_BUILD_CACHE:-}" in
    mbx)
      if [[ "$MBX_CACHED_SUBCOMMANDS" != *" $sub "* ]]; then
        echo "cx: mbx caches only build, check, clippy, test and nextest; it would run '${sub}' as plain cargo inside an mbx build directory. Run it without MIKE_BUILD_CACHE=mbx." >&2
        return 1
      fi
      CX_CACHE_MODE=mbx
      CX_MBX_TARGET="$named"
      ;;
    cargo | plain) ;;
    "")
      if [[ "$MBX_AUTO_SUBCOMMANDS" == *" $sub "* && -n "$(mbx_bin_path)" ]] &&
        ! [[ "$sub" == build && "${MIKE_BUILD_IDENTITY:-}" == git ]] &&
        { [[ -z "$named" || "$dir" == "$root/target" ]] || (( in_mbx )); }; then
        CX_CACHE_MODE=mbx
        if (( in_mbx )); then CX_MBX_TARGET="$dir"; else CX_MBX_TARGET="$root/target/$(cx_mbx_slot)"; fi
        return 0
      fi
      ;;
    *)
      echo "cx: MIKE_BUILD_CACHE must be mbx, cargo or unset (got '${MIKE_BUILD_CACHE}')" >&2
      return 1
      ;;
  esac
  if [[ "$CX_CACHE_MODE" == cargo ]] && (( in_mbx )); then
    echo "cx: ${named} is an mbx build directory (read-only hard links into the store); plain cargo must not build there. Name another CARGO_TARGET_DIR, or leave it unset." >&2
    return 1
  fi
  return 0
}

# prepare_mbx_build_cache <repo_root> <named_target_dir>
# The mbx path of scripts/cx. Sets MBX_BIN and the
# environment mbx needs, switches sccache off (mbx cannot be combined with it),
# and refuses a build directory that is not named or that is a shared one.
# Returns non-zero with a message on stderr when it cannot continue.
prepare_mbx_build_cache() {
  local root="$1" named="$2"

  if [[ -z "$named" ]]; then
    echo "cx: MIKE_BUILD_CACHE=mbx needs CARGO_TARGET_DIR to name the build directory (for example target/mbx-a)" >&2
    return 1
  fi
  local dir="$named"
  [[ "$dir" == /* ]] || dir="$root/$dir"
  while [[ "$dir" == */ && "$dir" != "/" ]]; do dir="${dir%/}"; done
  if [[ "$dir" == "$root/target" || "$dir" == "$root/target/fleet-check" ]]; then
    echo "cx: MIKE_BUILD_CACHE=mbx refuses ${named}: a build directory used through mbx holds read-only hard links, and this one is built by plain cargo. Name another, such as target/mbx-a" >&2
    return 1
  fi

  local bin
  bin="$(mbx_bin_path)"
  if [[ -z "$bin" ]]; then
    echo "cx: MIKE_BUILD_CACHE=mbx found no mbx: set MBX_BIN, put mbx on PATH, or install it at /mnt/data/mbx-trial/bin/mbx" >&2
    return 1
  fi

  export MBX_BIN="$bin"
  export MBX_CACHE_DIR="${MBX_CACHE_DIR:-$HOME/.cache/mike-mbx}"
  export MBX_TARGET_VIEWS=0 MBX_GC_AUTO=0 MBX_TARGET_SEED=0
  export MBX_LEARNED_INCREMENTAL=0
  # Workspace crates such as mike-base embed their checkout path (query_file! .sql
  # paths), so a recompile in another checkout gives different bytes and every crate
  # above it misses. Sharing the workspace root makes those bytes the same everywhere.
  export MBX_SHARE_WORKSPACE_ROOT=1
  # crates/build_identity.rs gives an mbx build a fixed identity only when it sees
  # this; without it every commit changes crate mike's key and nothing restores.
  export MIKE_BUILD_CACHE=mbx
  export CX_SCCACHE=0
  unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER
  export CARGO_TARGET_DIR="$dir"
}

# Run standalone: `scripts/lib/preflight-toolchain.sh` before every make build
# target (`make preflight`), and with --all to install every accelerator
# including the opt-in sccache (`make bootstrap`).
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  set -euo pipefail
  [[ "${1:-}" == "--all" ]] && PREFLIGHT_ALL=1
  ensure_build_toolchain
fi
