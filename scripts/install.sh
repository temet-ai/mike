#!/usr/bin/env bash
# mike installer — interactive one-shot setup.
#
# Walks through:
#   1. Prerequisite checks (rust, cargo, node, pnpm)
#   2. Rust workspace build (mike + mike-mcp)
#   3. Frontend dep install
#   4. Storage mode choice:
#        - in-memory:  no database (fastest, data lost on restart)
#        - external:   you supply the MIKE_DATABASE_URL for an existing
#                      Postgres you already run (LAN, managed cloud)
#   5. local bootstrap env update in ${PROJECT_ROOT}/.env
#      — only touches the five managed keys (persist_enabled, database_url,
#      proxy_listen_addr, inspect_listen_addr, frontend_port), shows a diff
#      of pending changes, asks before rewriting. Other lines, comments, and
#      hand-edited keys are left alone.
#   6. MCP bridge config snippet for ~/.claude.json (printed only)
#
# Safe to re-run.
#
# Usage:
#   ./scripts/install.sh
#   ./scripts/install.sh --non-interactive in-memory
#   ./scripts/install.sh --non-interactive external   # requires MIKE_DATABASE_URL env var
#   ./scripts/install.sh --release
#   ./scripts/install.sh --migrate-env                # one-shot: read existing
#                                                      # .env and seed the bootstrap env vars

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

# ─── Styling helpers ────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
    BOLD=$'\033[1m'; DIM=$'\033[2m'; RESET=$'\033[0m'
    GREEN=$'\033[32m'; YELLOW=$'\033[33m'; RED=$'\033[31m'; CYAN=$'\033[36m'
else
    BOLD=''; DIM=''; RESET=''; GREEN=''; YELLOW=''; RED=''; CYAN=''
fi

heading() { printf '\n%s==> %s%s\n' "${BOLD}${CYAN}" "$*" "${RESET}"; }
info()    { printf '    %s\n' "$*"; }
ok()      { printf '    %s✓%s %s\n' "${GREEN}" "${RESET}" "$*"; }
warn()    { printf '    %s!%s %s\n' "${YELLOW}" "${RESET}" "$*"; }
fail()    { printf '    %s✗%s %s\n' "${RED}" "${RESET}" "$*" >&2; }
die()     { fail "$*"; exit 1; }

# Normalize the permissive truthy/falsy spellings the Rust binary accepts so
# textual differences (`true` vs `1`) don't trigger a config rewrite.
normalize_bool() {
    local v="${1,,}"
    case "${v}" in
        1|true|yes|on)   printf 'true' ;;
        0|false|no|off)  printf 'false' ;;
        *)               printf '%s' "$1" ;;
    esac
}

# Hide the password in a Postgres URL before printing.
redact_url() {
    local url="$1"
    printf '%s' "$url" | sed -E 's|://([^:/@]+):[^@]*@|://\1:***@|'
}

# Read a scalar env value for KEY from FILE. Recognises uncommented
# `KEY=value` lines only — commented (`# KEY=...`) lines are treated as
# unset. Strips surrounding single or double quotes from the value. Last
# matching line wins. Returns empty when absent or the file is missing.
read_env_scalar() {
    local file="$1" key="$2"
    [[ -f "$file" ]] || return 0
    local line
    line="$(grep -E "^[[:space:]]*${key}=" "$file" | tail -n1 || true)"
    [[ -z "$line" ]] && return 0
    # Strip leading whitespace + key + equals.
    local value="${line#*=}"
    # Drop any trailing inline comment.
    value="${value%%#*}"
    value="${value%"${value##*[![:space:]]}"}"
    # Strip matching outer quotes.
    if [[ "$value" =~ ^\"(.*)\"$ ]] || [[ "$value" =~ ^\'(.*)\'$ ]]; then
        value="${BASH_REMATCH[1]}"
    fi
    printf '%s' "$value"
}

# Render a scalar VALUE as a shell env assignment value for KEY.
env_rhs() {
    local kind="$1" value="$2"
    case "$kind" in
        string)
            # Escape double-quotes and backslashes for shell sourcing.
            local escaped="${value//\\/\\\\}"
            escaped="${escaped//\"/\\\"}"
            printf '"%s"' "$escaped"
            ;;
        bool)
            printf '%s' "$(normalize_bool "$value")"
            ;;
        int)
            printf '%s' "$value"
            ;;
        *)
            die "env_rhs: unknown kind '$kind'"
            ;;
    esac
}

# Upsert KEY=<rendered-value> in FILE. If a matching line already exists
# (commented-out or live), it is replaced with the live form. Otherwise the
# new line is appended. Atomic via temp file + mv.
upsert_env_scalar() {
    local file="$1" key="$2" rendered="$3"
    [[ -f "$file" ]] || { mkdir -p "$(dirname "$file")"; : > "$file"; }

    # awk replaces any commented-or-live line with the live form; if nothing
    # matches, it appends after the last line.
    local tmp
    tmp="$(mktemp "${file}.XXXXXX")"
    awk -v k="$key" -v v="${key}=${rendered}" '
        BEGIN { done = 0 }
        {
            if (!done && $0 ~ "^[[:space:]]*#?[[:space:]]*"k"=") {
                print v; done = 1; next
            }
            print
        }
        END { if (!done) print v }
    ' "$file" > "$tmp"
    mv "$tmp" "$file"
}

# ─── Argument parsing ───────────────────────────────────────────────────────
MODE=""                 # "in-memory" | "external" | ""
INTERACTIVE=1
RELEASE=0
MIGRATE_ENV=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --non-interactive)
            INTERACTIVE=0
            MODE="${2:-}"
            case "${MODE}" in
                in-memory|external) ;;
                *) die "--non-interactive requires 'in-memory' or 'external' as the next argument" ;;
            esac
            shift 2
            ;;
        --release)
            RELEASE=1
            shift
            ;;
        --migrate-env)
            MIGRATE_ENV=1
            shift
            ;;
        -h|--help)
            sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            die "Unknown argument: $1 (try --help)"
            ;;
    esac
done

# ─── Prerequisite checks ────────────────────────────────────────────────────
heading "Checking prerequisites"

# Rustup's installer drops binaries in ~/.cargo/bin but only adds them to PATH
# via a shell-profile line that non-login shells (and some CI runners) skip.
# Prepend the standard location silently when that's the case.
if ! command -v cargo >/dev/null 2>&1 && [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
    info "Added ${HOME}/.cargo/bin to PATH for this run (rustup shell integration not active)"
fi

missing=()
for bin in rustc cargo node pnpm; do
    if ! command -v "${bin}" >/dev/null 2>&1; then
        missing+=("${bin}")
    else
        ok "${bin} — $(${bin} --version 2>&1 | head -n1)"
    fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
    fail "Missing tools: ${missing[*]}"
    info "Install them and re-run:"
    info "  Rust:  https://rustup.rs"
    info "  Node:  https://nodejs.org (v20+)"
    info "  pnpm:  https://pnpm.io/installation"
    info ""
    info "If rustup is already installed, try: source \"\$HOME/.cargo/env\""
    exit 1
fi

PSQL_OK=0
if command -v psql >/dev/null 2>&1; then
    PSQL_OK=1
fi

BOOTSTRAP_FILE="${PROJECT_ROOT}/.env"

# ─── --migrate-env one-shot ────────────────────────────────────────────────
if [[ "${MIGRATE_ENV}" -eq 1 ]]; then
    heading "Bootstrap now lives in .env"
    info "The installer now writes its selected bootstrap values directly to ${BOOTSTRAP_FILE}."
    info "No migration step is needed."
    exit 0
fi

# ─── Storage mode selection ─────────────────────────────────────────────────
if [[ -z "${MODE}" ]]; then
    heading "Pick a storage mode"
    info "  1) ${BOLD}in-memory${RESET} — no database. Fastest, nothing extra to install. Captured traffic kept in a 1000-request ring buffer and lost on restart."
    info "  2) ${BOLD}external${RESET}  — use a PostgreSQL you already run yourself. You supply ${BOLD}MIKE_DATABASE_URL${RESET}."
    echo

    if [[ "${INTERACTIVE}" -eq 0 ]]; then
        die "non-interactive mode requires --non-interactive <in-memory|external>"
    fi

    default_choice=1

    while :; do
        read -r -p "    Choice [${default_choice}]: " choice
        choice="${choice:-${default_choice}}"
        case "${choice}" in
            1|in-memory)  MODE="in-memory"; break ;;
            2|external)   MODE="external"; break ;;
            *) warn "Enter 1 or 2" ;;
        esac
    done
fi

ok "Selected mode: ${MODE}"

# ─── Port customization ─────────────────────────────────────────────────────
# Extract `host:port` segment helpers.
extract_port() {
    local addr="$1" fallback="$2" port="${1##*:}"
    if [[ -z "${addr}" || "${port}" == "${addr}" || ! "${port}" =~ ^[0-9]+$ ]]; then
        printf '%s' "${fallback}"
    else
        printf '%s' "${port}"
    fi
}

extract_host() {
    local addr="$1" host="${1%:*}"
    if [[ -z "${addr}" || "${host}" == "${addr}" ]]; then
        printf '127.0.0.1'
    else
        printf '%s' "${host}"
    fi
}

is_valid_port() {
    [[ "$1" =~ ^[0-9]+$ ]] && (( $1 >= 1 && $1 <= 65535 ))
}

# Seed "current" values from an existing bootstrap file if present.
current_proxy_addr="$(read_env_scalar "${BOOTSTRAP_FILE}" MIKE_LISTEN_ADDR)"
current_inspect_addr="$(read_env_scalar "${BOOTSTRAP_FILE}" MIKE_INSPECT_ADDR)"
current_frontend_port="$(read_env_scalar "${BOOTSTRAP_FILE}" MIKE_FRONTEND_PORT)"

default_proxy_port="$(extract_port "${current_proxy_addr}" 9988)"
default_inspect_port="$(extract_port "${current_inspect_addr}" 9989)"
default_frontend_port="${current_frontend_port:-9990}"

# Preserve non-loopback hosts the operator may have set (e.g. 0.0.0.0).
current_proxy_host="$(extract_host "${current_proxy_addr}")"
current_inspect_host="$(extract_host "${current_inspect_addr}")"

desired_proxy_port="${default_proxy_port}"
desired_inspect_port="${default_inspect_port}"
desired_frontend_port="${default_frontend_port}"

customize_ports=0
if [[ "${INTERACTIVE}" -eq 1 ]]; then
    heading "Ports"
    info "Defaults — proxy: ${default_proxy_port} · inspect: ${default_inspect_port} · frontend: ${default_frontend_port}"
    read -r -p "    Customize ports? [y/N]: " ans
    [[ "${ans}" =~ ^[Yy]$ ]] && customize_ports=1
else
    # Non-interactive honors MIKE_LISTEN_ADDR / MIKE_INSPECT_ADDR /
    # MIKE_FRONTEND_PORT from the invoking environment.
    [[ -n "${MIKE_LISTEN_ADDR:-}" ]] && {
        desired_proxy_port="$(extract_port "${MIKE_LISTEN_ADDR}" "${default_proxy_port}")"
        current_proxy_host="$(extract_host "${MIKE_LISTEN_ADDR}")"
    }
    [[ -n "${MIKE_INSPECT_ADDR:-}" ]] && {
        desired_inspect_port="$(extract_port "${MIKE_INSPECT_ADDR}" "${default_inspect_port}")"
        current_inspect_host="$(extract_host "${MIKE_INSPECT_ADDR}")"
    }
    [[ -n "${MIKE_FRONTEND_PORT:-}" ]] && desired_frontend_port="${MIKE_FRONTEND_PORT}"
fi

if [[ "${customize_ports}" -eq 1 ]]; then
    prompt_port() {
        local label="$1" default="$2" result
        while :; do
            read -r -p "    ${label} [${default}]: " result
            result="${result:-${default}}"
            if is_valid_port "${result}"; then
                printf '%s' "${result}"
                return
            fi
            warn "Not a valid port (1–65535): ${result}"
        done
    }
    desired_proxy_port="$(prompt_port "Proxy port    (claude target)" "${default_proxy_port}")"
    desired_inspect_port="$(prompt_port "Inspect port  (REST + WS API)" "${default_inspect_port}")"
    desired_frontend_port="$(prompt_port "Frontend port (web UI)      " "${default_frontend_port}")"
fi

if ! is_valid_port "${desired_proxy_port}" \
    || ! is_valid_port "${desired_inspect_port}" \
    || ! is_valid_port "${desired_frontend_port}"; then
    die "Port values must be 1–65535 (proxy=${desired_proxy_port} inspect=${desired_inspect_port} frontend=${desired_frontend_port})"
fi

if [[ "${desired_proxy_port}" == "${desired_inspect_port}" \
    || "${desired_proxy_port}" == "${desired_frontend_port}" \
    || "${desired_inspect_port}" == "${desired_frontend_port}" ]]; then
    die "Ports must all differ (proxy=${desired_proxy_port} inspect=${desired_inspect_port} frontend=${desired_frontend_port})"
fi

desired_proxy_addr="${current_proxy_host}:${desired_proxy_port}"
desired_inspect_addr="${current_inspect_host}:${desired_inspect_port}"

# ─── External Postgres URL collection ───────────────────────────────────────
EXTERNAL_URL=""
if [[ "${MODE}" == "external" ]]; then
    heading "External Postgres — bring your own URL"

    if [[ -n "${MIKE_DATABASE_URL:-}" ]]; then
        EXTERNAL_URL="${MIKE_DATABASE_URL}"
        info "Using MIKE_DATABASE_URL from environment: $(redact_url "${EXTERNAL_URL}")"
        if [[ "${INTERACTIVE}" -eq 1 ]]; then
            read -r -p "    Use this URL? [Y/n]: " ans
            if [[ "${ans}" =~ ^[Nn]$ ]]; then
                EXTERNAL_URL=""
            fi
        fi
    fi

    if [[ -z "${EXTERNAL_URL}" ]]; then
        if [[ "${INTERACTIVE}" -eq 0 ]]; then
            die "external mode without MIKE_DATABASE_URL — set the env var before running, e.g. MIKE_DATABASE_URL=postgres://... ./scripts/install.sh --non-interactive external"
        fi
        info "Enter your Postgres connection string."
        info "Format: ${DIM}postgres://USER:PASSWORD@HOST:PORT/DATABASE${RESET}"
        info "Example: ${DIM}postgres://claude:secret@db.example.com:5432/claude_proxy${RESET}"
        echo
        while :; do
            read -r -p "    MIKE_DATABASE_URL: " EXTERNAL_URL
            if [[ -z "${EXTERNAL_URL}" ]]; then
                warn "URL cannot be empty"
                continue
            fi
            if [[ ! "${EXTERNAL_URL}" =~ ^postgres(ql)?:// ]]; then
                warn "URL must start with postgres:// or postgresql://"
                continue
            fi
            break
        done
    fi

    if [[ "${PSQL_OK}" -eq 1 ]]; then
        info "Testing connection with local psql..."
        if PGCONNECT_TIMEOUT=5 psql "${EXTERNAL_URL}" -v ON_ERROR_STOP=1 -Atc "SELECT 1" >/dev/null 2>&1; then
            ok "Connection successful"
        else
            warn "psql could not connect. The URL may be wrong, the DB unreachable, or credentials invalid."
            warn "(start-proxy.sh will also fail-fast on bad credentials — you can still proceed.)"
            if [[ "${INTERACTIVE}" -eq 1 ]]; then
                read -r -p "    Continue anyway? [y/N]: " ans
                [[ "${ans}" =~ ^[Yy]$ ]] || die "Aborted at user request"
            fi
        fi
    else
        info "(skipping connection test — psql not installed; start-proxy.sh will verify on boot)"
    fi
fi

# ─── Build Rust workspace ───────────────────────────────────────────────────
heading "Building Rust workspace"
BUILD_FLAG=""
if [[ "${RELEASE}" -eq 1 ]]; then
    BUILD_FLAG="--release"
    info "Release build requested (--release)"
fi

cargo build ${BUILD_FLAG} --all </dev/null
ok "Rust workspace built"

cargo build ${BUILD_FLAG} --manifest-path crates/mike-mcp/Cargo.toml </dev/null
ok "mike-mcp bridge built"

# ─── Install frontend deps ──────────────────────────────────────────────────
heading "Installing frontend dependencies"
if [[ -d frontend/node_modules ]]; then
    ok "frontend/node_modules exists — skipping pnpm install (re-run with 'cd frontend && pnpm install' if stale)"
else
    (cd frontend && pnpm install) </dev/null
    ok "frontend deps installed"
fi

# ─── bootstrap file update ───────────────────────────────────────────────────
# Compute the desired values for the five managed keys. Any other content in
# the bootstrap file (other keys, comments, blank lines) is preserved untouched.

case "${MODE}" in
    in-memory)
        desired_persist="false"
        desired_url=""
        ;;
    external)
        desired_persist="true"
        desired_url="${EXTERNAL_URL}"
        ;;
esac

current_persist="$(read_env_scalar "${BOOTSTRAP_FILE}" MIKE_PERSIST_ENABLED)"
current_url="$(read_env_scalar "${BOOTSTRAP_FILE}" MIKE_DATABASE_URL)"

# Collect pending changes for a coherent diff. Only keys that actually deviate
# from the current bootstrap file get touched. `conflicts` tracks changes where
# an existing value would be overwritten (vs added fresh); non-interactive
# runs refuse to proceed in that case.
declare -a changes=()
declare -a conflicts=()

# Treat unset keys as their built-in default. The binary defaults
# persist_enabled to `true`, so an in-memory install on a fresh machine
# records the opt-out to `false` as an explicit change.
effective_persist="${current_persist:-true}"
if [[ "$(normalize_bool "${effective_persist}")" != "$(normalize_bool "${desired_persist}")" ]]; then
    changes+=("persist_enabled: ${current_persist:-<unset>} → ${desired_persist}")
    [[ -n "${current_persist}" ]] && conflicts+=("persist_enabled")
fi

url_changed=0
if [[ -n "${desired_url}" ]] && [[ "${current_url}" != "${desired_url}" ]]; then
    url_changed=1
    changes+=("database_url: $(redact_url "${current_url:-<unset>}") → $(redact_url "${desired_url}")")
    [[ -n "${current_url}" ]] && conflicts+=("database_url")
fi

# Ports are only written when they'd change behaviour:
#   - current set and differs → update
#   - current unset and desired == default → skip (no need to pollute config)
#   - current unset and desired != default → write
proxy_addr_changed=0
if [[ "${current_proxy_addr}" != "${desired_proxy_addr}" ]]; then
    if [[ -n "${current_proxy_addr}" || "${desired_proxy_addr}" != "127.0.0.1:9988" ]]; then
        proxy_addr_changed=1
        changes+=("proxy_listen_addr: ${current_proxy_addr:-<unset>} → ${desired_proxy_addr}")
        [[ -n "${current_proxy_addr}" ]] && conflicts+=("proxy_listen_addr")
    fi
fi

inspect_addr_changed=0
if [[ "${current_inspect_addr}" != "${desired_inspect_addr}" ]]; then
    if [[ -n "${current_inspect_addr}" || "${desired_inspect_addr}" != "127.0.0.1:9989" ]]; then
        inspect_addr_changed=1
        changes+=("inspect_listen_addr: ${current_inspect_addr:-<unset>} → ${desired_inspect_addr}")
        [[ -n "${current_inspect_addr}" ]] && conflicts+=("inspect_listen_addr")
    fi
fi

frontend_port_changed=0
if [[ "${current_frontend_port}" != "${desired_frontend_port}" ]]; then
    if [[ -n "${current_frontend_port}" || "${desired_frontend_port}" != "9990" ]]; then
        frontend_port_changed=1
        changes+=("frontend_port: ${current_frontend_port:-<unset>} → ${desired_frontend_port}")
        [[ -n "${current_frontend_port}" ]] && conflicts+=("frontend_port")
    fi
fi

# Resolve the built `mike` binary path once. Users with a shared
# CARGO_TARGET_DIR don't have target/ under PROJECT_ROOT; ask cargo.
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --offline 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' \
    | head -n1)"
[[ -z "${TARGET_DIR}" ]] && TARGET_DIR="${PROJECT_ROOT}/target"
BUILD_PROFILE="debug"; [[ "${RELEASE}" -eq 1 ]] && BUILD_PROFILE="release"
MIKE_BIN="${TARGET_DIR}/${BUILD_PROFILE}/mike"

heading "Configuring ${BOOTSTRAP_FILE}"

if [[ ${#changes[@]} -eq 0 ]]; then
    if [[ -f "${BOOTSTRAP_FILE}" ]]; then
        ok "bootstrap env already matches the selected configuration — no changes needed"
    else
        : > "${BOOTSTRAP_FILE}"
        ok "Prepared ${BOOTSTRAP_FILE}"
    fi
else
    info "Pending changes to ${BOOTSTRAP_FILE}:"
    for c in "${changes[@]}"; do info "  - ${c}"; done

    apply_changes=1
    if [[ "${INTERACTIVE}" -eq 1 ]]; then
        if [[ ${#conflicts[@]} -gt 0 ]]; then
            warn "The listed changes would OVERWRITE existing values in the bootstrap env."
            warn "Keys affected: ${conflicts[*]}"
            read -r -p "    Replace the current values with the new ones? [y/N]: " ans
            [[ "${ans}" =~ ^[Yy]$ ]] || apply_changes=0
        else
            read -r -p "    Apply these changes? [Y/n]: " ans
            [[ "${ans}" =~ ^[Nn]$ ]] && apply_changes=0
        fi
    else
        if [[ ${#conflicts[@]} -gt 0 ]]; then
            fail "bootstrap env has conflicting existing values for: ${conflicts[*]}"
            info "Re-run interactively to approve the change, or edit the file manually and re-run."
            exit 2
        fi
    fi

    if [[ "${apply_changes}" -eq 1 ]]; then
        # Create the bootstrap env file when it does not exist yet so the
        # upserts below have something to edit.
        if [[ ! -f "${BOOTSTRAP_FILE}" ]]; then
            : > "${BOOTSTRAP_FILE}"
        fi
        # Only write MIKE_PERSIST_ENABLED when it actually needs to change.
        if [[ "$(normalize_bool "${effective_persist}")" != "$(normalize_bool "${desired_persist}")" ]]; then
            upsert_env_scalar "${BOOTSTRAP_FILE}" MIKE_PERSIST_ENABLED "$(env_rhs bool "${desired_persist}")"
        fi
        [[ "${url_changed}" -eq 1 ]]           && upsert_env_scalar "${BOOTSTRAP_FILE}" MIKE_DATABASE_URL        "$(env_rhs string "${desired_url}")"
        [[ "${proxy_addr_changed}" -eq 1 ]]    && upsert_env_scalar "${BOOTSTRAP_FILE}" MIKE_LISTEN_ADDR          "$(env_rhs string "${desired_proxy_addr}")"
        [[ "${inspect_addr_changed}" -eq 1 ]]  && upsert_env_scalar "${BOOTSTRAP_FILE}" MIKE_INSPECT_ADDR         "$(env_rhs string "${desired_inspect_addr}")"
        [[ "${frontend_port_changed}" -eq 1 ]] && upsert_env_scalar "${BOOTSTRAP_FILE}" MIKE_FRONTEND_PORT         "$(env_rhs int "${desired_frontend_port}")"
        ok "Updated ${BOOTSTRAP_FILE}"
    else
        warn "Skipping bootstrap env updates — start-proxy.sh may fail if values don't match the selected configuration"
    fi
fi

# ─── MCP bridge hint ────────────────────────────────────────────────────────
heading "MCP bridge registration (optional)"

MCP_BIN="${TARGET_DIR}/${BUILD_PROFILE}/mike-mcp"
if [[ ! -x "${MCP_BIN}" ]]; then
    warn "Expected binary at ${MCP_BIN} but it's missing — the build step should have produced it"
fi

info "To expose the proxy's captured traffic to Claude Code as MCP tools,"
info "add this block to ${BOLD}~/.claude.json${RESET} under mcpServers:"
echo
cat <<EOF
    "mike": {
      "command": "${MCP_BIN}",
      "env": {
        "MIKE_INSPECT_URL": "http://127.0.0.1:${desired_inspect_port}"
      }
    }
EOF
echo

# ─── Final instructions ─────────────────────────────────────────────────────
heading "Done — next steps"
release_arg=""
[[ "${RELEASE}" -eq 1 ]] && release_arg=" --release"
info "Start the proxy stack:"
info "  ${BOLD}./scripts/start-proxy.sh${release_arg}${RESET}"
info ""
info "In another terminal, point Claude Code at the proxy:"
info "  ${BOLD}ANTHROPIC_BASE_URL=http://localhost:${desired_proxy_port} claude${RESET}"
info ""
info "Open the web UI:"
info "  ${BOLD}http://localhost:${desired_frontend_port}${RESET}"
info ""
info "Inspect resolved config at any time:"
info "  ${BOLD}${MCP_BIN%/*}/mike print-config${RESET}"

case "${MODE}" in
    external)
        info ""
        info "Using external Postgres at: $(redact_url "${EXTERNAL_URL}")"
        info "Schema + migrations are applied automatically by the proxy on startup."
        ;;
esac
echo
