#!/bin/sh
_command=${0##*/}
if [ "${MIKE_FLEET_BROWSER_GUARD:-1}" != 0 ]; then
case "$_command:${1-}" in
    xdg-open:*|open:*|gio:open|gio:launch)
        printf '%s\n' 'mike fleet: desktop launch blocked by MIKE_FLEET_BROWSER_GUARD; return the link to the operator' >&2
        exit 126
        ;;
esac
fi
_guard=${0%/*}
_remaining=${PATH-}
_clean=
_separator=
_kept=0
while :; do
    _part=${_remaining%%:*}
    _marker=
    if [ -f "$_part/.mike-browser-guard" ]; then
        IFS= read -r _marker < "$_part/.mike-browser-guard"
    fi
    if [ "$_part" != "$_guard" ] && [ "$_marker" != mike-fleet-browser-guard-v1 ]; then
        _clean=$_clean$_separator$_part
        _separator=:
        _kept=1
    fi
    case $_remaining in
        *:*) _remaining=${_remaining#*:} ;;
        *) break ;;
    esac
done
if [ "$_kept" = 0 ]; then
    printf '%s\n' "mike fleet: original $_command is unavailable" >&2
    exit 127
fi
_real=$(PATH="$_clean" command -v "$_command") || {
    printf '%s\n' "mike fleet: original $_command is unavailable" >&2
    exit 127
}
exec "$_real" "$@"
