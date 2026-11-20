#!/usr/bin/env bash
# deploy-build.sh — the decisions deploy-proxy.sh makes about building mike, as functions that
# read their input from arguments, so scripts/test-deploy-build can drive them without a build.
#
# A deploy builds through the mbx cache when mbx is installed: every crate is compiled with a
# fixed identity (crates/build_identity.rs, MIKE_BUILD_CACHE=mbx), so a new commit restores the
# crates it did not change instead of recompiling mike-base and everything above it. The commit
# is then written into the staged copy by scripts/lib/stamp_build_identity.py, and the deploy
# refuses a staged binary whose --build-info does not report HEAD.

# deploy_build_info_sha <binary>: the source_sha the binary reports with --build-info, printed
# only when it reports a clean tree. Prints nothing for a missing, failing or dirty binary.
deploy_build_info_sha() {
    local bin="$1" info sha
    [[ -x "${bin}" ]] || return 0
    info="$("${bin}" --build-info 2>/dev/null)" || return 0
    printf '%s' "${info}" | grep -qE '"source_dirty"[[:space:]]*:[[:space:]]*false' || return 0
    sha="$(printf '%s' "${info}" \
        | grep -oE '"source_sha"[[:space:]]*:[[:space:]]*"[0-9a-f]{40}"' \
        | grep -oE '[0-9a-f]{40}' | head -n1)"
    printf '%s' "${sha}"
}

# deploy_build_needed <head> <profile> <recorded_commit> <recorded_profile> <live_sha>
# Returns 1 (no build needed) only when the live binary was deployed from HEAD with this
# profile and reports HEAD itself; else 0.
deploy_build_needed() {
    local head="$1" profile="$2" recorded="$3" recorded_profile="$4" live_sha="$5"
    [[ -n "${head}" ]] || return 0
    [[ "${recorded}" == "${head}" && "${recorded_profile}" == "${profile}" ]] || return 0
    [[ "${live_sha}" == "${head}" ]] || return 0
    return 1
}

# deploy_build_cache <mbx_path>: mbx when an mbx binary was found and MIKE_DEPLOY_BUILD_CACHE is
# not cargo; else cargo.
deploy_build_cache() {
    if [[ -n "$1" && "${MIKE_DEPLOY_BUILD_CACHE:-mbx}" != cargo ]]; then
        echo mbx
    else
        echo cargo
    fi
}
