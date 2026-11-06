#!/usr/bin/env bash

# Development checkouts deliberately load their own trusted shell configuration.
# Values exported by the caller, including empty DB/test URLs, always win.
mike_source_env_preserving_exported() {
    local env_file="$1" name assignment
    local -a preserved=()
    [[ -f "$env_file" ]] || return 0
    while IFS= read -r name; do
        case "$name" in
            MIKE_*|DATABASE_URL|CARGO_TARGET_DIR)
                preserved+=("$name=${!name}")
                ;;
        esac
    done < <(compgen -e)
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
    for assignment in "${preserved[@]}"; do
        export "$assignment"
    done
}
