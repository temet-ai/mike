#!/usr/bin/env bash
# Stop only the explicitly selected installation. There is no implicit delete path.
set -euo pipefail
if [[ "$#" -ne 2 || "${1:-}" != --config || -z "${2:-}" ]]; then
  echo 'Usage: scripts/local-db/down.sh --config PRIVATE_CONFIG' >&2
  echo 'The owned database is stopped; its volume and private credential are preserved.' >&2
  exit 2
fi
exec "${MIKE_BIN:-mike}" --config "$2" database stop
