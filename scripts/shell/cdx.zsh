# scripts/shell/cdx.zsh — `cx` and `cdx`: start OpenAI Codex through mike so its Responses
# traffic is captured, the way `cc` does for Claude Code.
#
# `cx` adds --dangerously-bypass-approvals-and-sandbox (Codex's equivalent of cc's
# --dangerously-skip-permissions); `cdx` starts Codex with its own approval settings.
# Remove any older `alias cx=…` from ~/.zshrc: an alias wins over this function.
#
# Source it from ~/.zshrc beside cc.zsh:
#   _temet_cdx_lib="$HOME/projects/temet/temet-proxy/scripts/shell/cdx.zsh"
#   [ -r "$_temet_cdx_lib" ] && source "$_temet_cdx_lib"
#
# `cdx` points Codex's model provider at mike's /codex route on the proxy bind
# (`-c openai_base_url=http://127.0.0.1:9988/codex`). mike relays each request to the
# upstream the credentials belong to (ChatGPT login → chatgpt.com/backend-api/codex,
# API key → api.openai.com/v1), records the original and forwarded bodies, and registers
# the Codex session on the workspace roster. mike answers Codex's Responses WebSocket
# upgrade with 426, so Codex falls back to HTTP/SSE, which mike captures.
#
# When mike's inspect API does not answer, the launch is UNCAPTURED and says so on stderr.
# `cx` does not clash with scripts/cx (the cargo wrapper), which is always run by its path.
# Docs: docs/CODEX_CAPTURE.md.

_temet_cdx_proxy="${MIKE_CDX_PROXY:-http://127.0.0.1:9988}"
_temet_cdx_health="${MIKE_CDX_HEALTH:-http://127.0.0.1:9989/api/health}"

_temet_cdx_up() {
  curl -s -o /dev/null -m 2 -f "$_temet_cdx_health"
}

# $1 = launcher name for messages, remaining args = codex argv.
_cdx_launch() {
  local name="$1"
  shift
  if _temet_cdx_up; then
    command codex -c "openai_base_url=\"$_temet_cdx_proxy/codex\"" "$@"
  else
    echo "$name: mike not reachable at $_temet_cdx_health — starting Codex UNCAPTURED (run ./scripts/start-proxy.sh in temet-proxy first)" >&2
    command codex "$@"
  fi
}

(( ${+aliases[cx]} )) && unalias cx
cdx() { _cdx_launch cdx "$@"; }
cx()  { _cdx_launch cx --dangerously-bypass-approvals-and-sandbox "$@"; }
