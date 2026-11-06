# scripts/shell/cc.zsh — captured, restartable Claude Code launchers (`cc`, `ccdirect`).
#
# Source it from ~/.zshrc:
#   _temet_cc_lib="$HOME/projects/temet/temet-proxy/scripts/shell/cc.zsh"
#   [ -r "$_temet_cc_lib" ] && source "$_temet_cc_lib"
#
# `cc` starts Claude Code through the proxy-forward sidecar on :9987 so mike captures
# every request and the Remote Control channel, and appends --remote-control so mike can
# restart the session later by pushing into that channel. `ccdirect` keeps inference
# captured but lets the Remote Control channel bypass the sidecar: the A/B test for
# "is the sidecar breaking mobile-app replies".
#
# Per launch: --no-remote-control opts out (stripped before claude sees it); passing
# --remote-control [name] yourself is respected as is. When the sidecar is down the
# launch is UNCAPTURED, says so on stderr, and does not add --remote-control because
# nothing would capture the channel.
#
# The same sidecar-up condition also appends --dangerously-load-development-channels
# server:mike, so mike-mcp's background poll can push mailbox mail through the MCP
# channel (issue #352) instead of relying only on the Remote Control stream. This
# triggers Claude Code's one-time-per-launch "Loading development channels" confirmation
# dialog (Enter accepts the preselected "local development" option). --no-channels opts
# out (stripped before claude sees it); passing --dangerously-load-development-channels
# yourself is respected as is.
#
# `_claude_trust_here`, if defined in the caller's shell, runs first so the folder is
# trusted before claude starts.

_temet_proxy_repo="${TEMET_PROXY_REPO:-$HOME/projects/temet/temet-proxy}"
_temet_proxy_ca="$_temet_proxy_repo/.deploy/proxy-forward-ca.pem"

# The sidecar CA is wherever `mike forward ca-path` says (the one place mike, `mike claude`,
# `mike spawn` and `mike doctor` all look); the checkout's .deploy copy is the fallback for a
# machine without an installed mike, or one too old to have `mike forward`.
_temet_proxy_ca_path() {
  local ca
  if (( $+commands[mike] )) && ca="$(command mike forward ca-path 2>/dev/null)" && [ -f "$ca" ]; then
    print -r -- "$ca"
  else
    print -r -- "$_temet_proxy_ca"
  fi
}
_temet_proxy_up() {
  local ca
  ca="$(_temet_proxy_ca_path)"
  [ -f "$ca" ] && curl -s -o /dev/null -m 2 http://127.0.0.1:9987/
}

# Splits argv into _cc_rc_args (without --no-remote-control / --no-channels) and four
# flags: _cc_rc_has_flag / _cc_rc_no_flag for --remote-control, _cc_ch_has_flag /
# _cc_ch_no_flag for --dangerously-load-development-channels (0 or 1 each).
_cc_strip_no_remote_control() {
  _cc_rc_args=()
  _cc_rc_has_flag=0
  _cc_rc_no_flag=0
  _cc_ch_has_flag=0
  _cc_ch_no_flag=0
  local arg
  for arg in "$@"; do
    if [ "$arg" = "--remote-control" ]; then
      _cc_rc_has_flag=1
    fi
    if [ "$arg" = "--no-remote-control" ]; then
      _cc_rc_no_flag=1
      continue
    fi
    if [ "$arg" = "--dangerously-load-development-channels" ]; then
      _cc_ch_has_flag=1
    fi
    if [ "$arg" = "--no-channels" ]; then
      _cc_ch_no_flag=1
      continue
    fi
    _cc_rc_args+=("$arg")
  done
}

# $1 = launcher name for messages, $2 = NO_PROXY list, $3 = "ca" to pass the sidecar CA
# to node (cc) or "noca" (ccdirect), remaining args = claude argv.
_cc_launch() {
  local name="$1" no_proxy="$2" ca_mode="$3"
  shift 3
  (( $+functions[_claude_trust_here] )) && _claude_trust_here
  _cc_strip_no_remote_control "$@"
  if _temet_proxy_up; then
    if [ "$_cc_rc_has_flag" -eq 0 ] && [ "$_cc_rc_no_flag" -eq 0 ]; then
      _cc_rc_args+=(--remote-control)
    fi
    if [ "$_cc_ch_has_flag" -eq 0 ] && [ "$_cc_ch_no_flag" -eq 0 ]; then
      _cc_rc_args+=(--dangerously-load-development-channels server:mike)
    fi
    if [ "$ca_mode" = "ca" ]; then
      HTTPS_PROXY=http://127.0.0.1:9987 NO_PROXY="$no_proxy" NODE_EXTRA_CA_CERTS="$(_temet_proxy_ca_path)" \
        command claude --dangerously-skip-permissions --chrome "${_cc_rc_args[@]}"
    else
      HTTPS_PROXY=http://127.0.0.1:9987 NO_PROXY="$no_proxy" \
        command claude --dangerously-skip-permissions --chrome "${_cc_rc_args[@]}"
    fi
  else
    echo "$name: temet-proxy sidecar not reachable on :9987 — starting UNCAPTURED (run ./scripts/start-proxy.sh in temet-proxy first)" >&2
    command claude --dangerously-skip-permissions --chrome "${_cc_rc_args[@]}"
  fi
}

cc()       { _cc_launch cc       "localhost,127.0.0.1"                       ca   "$@"; }
ccdirect() { _cc_launch ccdirect "localhost,127.0.0.1,claudeusercontent.com" noca "$@"; }
