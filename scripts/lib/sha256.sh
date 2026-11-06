#!/usr/bin/env bash
# sha256 [file ...] — print the SHA-256 hex digest of each file, or of stdin when
# no file is given, as `<hex>` (stdin) or `<hex>  <file>` (files), one per line.
# Uses whichever of sha256sum, shasum or openssl the machine has, so a script
# hashes the same way on Linux and macOS whatever its PATH holds. Source it:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/sha256.sh"
# Returns 127 when none of the three tools exists.

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        if (($#)); then sha256sum "$@"; else sha256sum | awk '{print $1}'; fi
    elif command -v shasum >/dev/null 2>&1; then
        if (($#)); then shasum -a 256 "$@"; else shasum -a 256 | awk '{print $1}'; fi
    elif command -v openssl >/dev/null 2>&1; then
        if (($#)); then
            local f
            for f in "$@"; do
                printf '%s  %s\n' "$(openssl dgst -sha256 -r "$f" | awk '{print $1}')" "$f"
            done
        else
            openssl dgst -sha256 -r | awk '{print $1}'
        fi
    else
        echo "sha256: no sha256sum, shasum or openssl on PATH" >&2
        return 127
    fi
}
