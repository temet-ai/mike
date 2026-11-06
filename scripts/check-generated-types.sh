#!/usr/bin/env bash
# check-generated-types.sh — authoritative "generated TS matches the Rust DTOs"
# gate. This is the CI layer of the api-types edge (the boot layer in
# session-up.sh only hash-pins the committed artifact; it cannot run a build).
#
# Regenerates the ts-rs bindings from the current Rust source and fails if the
# committed files under frontend/src/lib/api/generated/ differ — i.e. a DTO
# changed without regenerating. Run in CI / pre-push, and locally after touching
# an annotated DTO.
#
# Needs a cargo build (the ts-export feature). NOT a boot-time check.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

GEN_DIR="frontend/src/lib/api/generated"

echo "==> regenerating ts-rs bindings from Rust DTOs"
cargo test -p mike --features ts-export export_bindings

echo "==> diffing committed ${GEN_DIR} against freshly generated output"
if ! git diff --quiet -- "${GEN_DIR}"; then
  echo "✗ generated TS is OUT OF SYNC with the Rust DTOs." >&2
  echo "  A DTO changed without regenerating. Run:" >&2
  echo "    cargo test -p mike --features ts-export export_bindings" >&2
  echo "    scripts/session-up.sh reconcile   # re-stamp the boot hash-pin" >&2
  echo "  then commit ${GEN_DIR}." >&2
  git --no-pager diff --stat -- "${GEN_DIR}" >&2
  exit 1
fi

echo "✓ generated TS is in sync with the Rust inspect-API DTOs."
