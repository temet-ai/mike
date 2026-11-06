#!/usr/bin/env bash
# The installed product owns Docker provisioning. This wrapper requires an explicit
# config so development work cannot silently replace the primary installation.
set -euo pipefail
if [[ "${1:-}" != --config || -z "${2:-}" ]]; then
  echo 'Usage: scripts/local-db/up.sh --config PRIVATE_CONFIG [--port PORT] [--image pgvector/pgvector@sha256:DIGEST]' >&2
  echo 'Provisioning does not migrate. Start mike --config PRIVATE_CONFIG serve to apply embedded migrations.' >&2
  exit 2
fi
config="$2"
shift 2
exec "${MIKE_BIN:-mike}" --config "$config" database provision --port "${MIKE_LOCAL_DB_PORT:-6666}" "$@"
