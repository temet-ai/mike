# shellcheck shell=bash
# Decisions of scripts/deploy-ha, kept here so scripts/test-ha-deploy can drive them
# without ha.

# ha_swap_outcome: reads the text of a swap.log written by scripts/ha-swap on stdin and
# prints its outcome. ha-swap ends its log with one of these lines:
#   OK        the new build runs
#   ROLLBACK  the new build did not come up; the previous binaries are back
#   FAILED    nothing was swapped (a missing binary or a failed backup)
# Anything else, an empty log included, is PENDING: ha-swap is still running.
ha_swap_outcome() {
  local last
  last="$(awk 'NF { line = $0 } END { print line }')"
  case "$last" in
    OK|ROLLBACK|FAILED) printf '%s\n' "$last" ;;
    *) echo PENDING ;;
  esac
}

# ha_version_matches <version line> <commit>: true when `mike --version` output names the
# build of <commit> ("mike 0.1.0 (3fa90345, built ...)"). The binary prints eight hex digits.
ha_version_matches() {
  local short="${2:0:8}"
  [ ${#short} -eq 8 ] || return 1
  case "$1" in
    *"($short"*","*) return 0 ;;
    *) return 1 ;;
  esac
}
