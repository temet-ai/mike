#!/usr/bin/env bash
# scripts/start-infinity.sh — start the shared Infinity embedding server.
#
# Serves nomic-ai/nomic-embed-text-v1.5 (768-dim) on :7997.
# Idempotent: no-op if the port is already responding to HTTP GET /health.
#
# Shared infrastructure: this same Infinity instance is consumed by
# temet-memory (~/projects/ai/tools/memory) and this proxy. Version pins
# match temet-memory's readiness doc (2026-06-10):
#   - optimum[onnxruntime]==1.23.3
#   - click==8.1.8
#
# Prerequisites:
#   brew install uv        # or pip install uv
#   uv is on PATH          # fast Python installer/runner
#
# Usage:
#   ./scripts/start-infinity.sh          # foreground, Ctrl-C to stop
#   ./scripts/start-infinity.sh &        # background; PID printed
#
# Infinity docs: https://github.com/michaelfeil/infinity

set -euo pipefail

INFINITY_PORT="${INFINITY_PORT:-7997}"
INFINITY_HOST="${INFINITY_HOST:-127.0.0.1}"
MODEL="${INFINITY_MODEL:-nomic-ai/nomic-embed-text-v1.5}"
SERVED_NAME="${INFINITY_SERVED_NAME:-nomic-embed-text-v1.5}"

# ── Liveness check ──────────────────────────────────────────────────
health_url="http://${INFINITY_HOST}:${INFINITY_PORT}/health"
if curl -sf --max-time 2 "$health_url" >/dev/null 2>&1; then
  echo "Infinity already running at ${INFINITY_HOST}:${INFINITY_PORT} — no-op."
  exit 0
fi

echo "Starting Infinity on ${INFINITY_HOST}:${INFINITY_PORT} (model: ${MODEL})"

exec uvx \
  --python 3.12 \
  --with "optimum[onnxruntime]==1.23.3" \
  --with "click==8.1.8" \
  --from "infinity-emb[all]" \
  infinity_emb v2 \
  --model-id "${MODEL}" \
  --served-model-name "${SERVED_NAME}" \
  --host "${INFINITY_HOST}" \
  --port "${INFINITY_PORT}" \
  --vector-disk-cache
