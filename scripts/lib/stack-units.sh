# stack-units.sh — systemd user units own the mike stack's long-running processes.
#
# WHY: until 2026-09-28 deploy-proxy.sh / start-proxy.sh started the proxy, the
# proxy-forward sidecar, the prompt assistant and vite as detached nohup
# processes that nothing supervised. When one died (the prompt assistant did,
# silently, on 2026-09-27) it stayed dead until someone happened to redeploy.
# The earlier systemd units (mike.service / proxy-forward.service, disabled
# 2026-09-14) failed for a different reason: they were a SECOND owner, execing
# ~/.local/bin binaries that nothing kept current, and fought the scripts for
# the ports. Here the units are the ONLY owner of the processes; the scripts keep
# building, smoke-testing and rolling back, and hand the result to the unit
# (copy the binary into .deploy/, then `systemctl --user restart`).
#
# Units are rendered from scripts/systemd/*.service.in (versioned with the code)
# into ~/.config/systemd/user by install_stack_units. Restart=always with a
# StartLimitBurst cap: a dead service comes back in seconds, a binary that can't
# start (e.g. a migration-ledger mismatch) gives up after 10 tries in 10 minutes
# instead of crash-looping forever.
#
# Opt out (e.g. macOS, or a CI shell without a user systemd): MIKE_STACK_SUPERVISOR=nohup.

# Storybook joined on 2026-10-01: until then it ran in the foreground of whichever
# terminal ran start-proxy.sh and died with that terminal, which also left the
# Storybook MCP and the style guide (the Foundations stories) unreachable.
STACK_UNITS=(mike-stack-proxy mike-stack-forward mike-stack-assistant mike-stack-spa mike-stack-storybook)

stack_units_enabled() {
    [[ "${MIKE_STACK_SUPERVISOR:-systemd}" == "systemd" ]] || return 1
    command -v systemctl >/dev/null 2>&1 || return 1
    systemctl --user show-environment >/dev/null 2>&1 || return 1
}

install_stack_units() {
    local src="${PROJECT_ROOT}/scripts/systemd" dst="${HOME}/.config/systemd/user" changed=false
    mkdir -p "${dst}"
    # Render with the checkout's physical path. Reached through a symlink
    # (~/projects -> /mnt/data/projects here), PROJECT_ROOT differs between
    # callers, and each one rewrote every unit file and reloaded systemd.
    local unit rendered root
    root="$(cd "${PROJECT_ROOT}" && pwd -P)"
    for unit in "${STACK_UNITS[@]}"; do
        # A masked unit is a symlink to /dev/null: writing the rendered unit
        # through it lands in /dev/null and the restart fails with "masked".
        if [[ -L "${dst}/${unit}.service" && "$(readlink "${dst}/${unit}.service")" == /dev/null ]]; then
            systemctl --user unmask "${unit}.service" >/dev/null 2>&1 || rm -f "${dst}/${unit}.service"
            echo "==> ${unit} was masked; unmasked so the deploy can start it"
            changed=true
        fi
        rendered="$(sed -e "s#@PROJECT_ROOT@#${root}#g" -e "s#@PATH@#${PATH}#g" "${src}/${unit}.service.in")"
        if [[ ! -f "${dst}/${unit}.service" ]] || [[ "$(cat "${dst}/${unit}.service")" != "${rendered}" ]]; then
            printf '%s\n' "${rendered}" > "${dst}/${unit}.service"
            changed=true
        fi
    done
    if ${changed}; then
        systemctl --user daemon-reload
        echo "==> Installed/updated stack units in ${dst}"
    fi
    systemctl --user enable "${STACK_UNITS[@]/%/.service}" >/dev/null 2>&1 || true
    retire_job_timers
}

RETIRED_JOB_TIMERS=(mike-learning-promote mike-learning-audit mike-compressor-loop mike-cargo-sweep mike-lessons-commit mike-usage-audit)

# The proxy runs these jobs itself (crates/mike/src/scheduled_jobs), on every
# machine. A timer left installed from before would run a job a second time, so
# any installed copy is disabled and its files removed.
retire_job_timers() {
    command -v systemctl >/dev/null 2>&1 || return 0
    systemctl --user show-environment >/dev/null 2>&1 || return 0
    local dst="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user" name kind removed=false
    for name in "${RETIRED_JOB_TIMERS[@]}"; do
        if [[ ! -e "${dst}/${name}.timer" && ! -e "${dst}/${name}.service" ]]; then
            continue
        fi
        systemctl --user disable --now "${name}.timer" >/dev/null 2>&1 || true
        for kind in timer service; do
            rm -f "${dst}/${name}.${kind}"
        done
        removed=true
        echo "==> Removed ${name}.timer: the proxy now runs that job itself"
    done
    if ${removed}; then
        systemctl --user daemon-reload
    fi
    return 0
}

unit_active() { systemctl --user is-active --quiet "$1.service"; }

unit_main_pid() { systemctl --user show -p MainPID --value "$1.service" 2>/dev/null; }

# Hand a port over to its unit: if something OTHER than the unit's own process
# holds it (a leftover nohup instance from before the units existed), free it.
adopt_port() {
    local unit="$1" port="$2" holder main
    holder="$(lsof -ti ":${port}" -sTCP:LISTEN 2>/dev/null | head -1 || true)"
    [[ -z "${holder}" ]] && return 0
    main="$(unit_main_pid "${unit}")"
    if [[ "${holder}" != "${main}" ]] && ! pstree -p "${main}" 2>/dev/null | grep -q "(${holder})"; then
        echo "==> Port ${port} is held by pid ${holder} outside ${unit} — handing it to the unit"
        free_port "${port}"
    fi
}

unit_restart() {
    local unit="$1"
    systemctl --user reset-failed "${unit}.service" >/dev/null 2>&1 || true
    systemctl --user restart "${unit}.service"
}

unit_start() {
    local unit="$1"
    systemctl --user reset-failed "${unit}.service" >/dev/null 2>&1 || true
    systemctl --user start "${unit}.service"
}

# Every deploy leaves the whole stack supervised and running: a unit that is
# down (or that StartLimitBurst gave up on) is started again, taking its port
# over from any leftover unsupervised process. Units that are already active are
# not touched, so this never interrupts a healthy service.
ensure_stack_units_running() {
    local pair unit port
    for pair in mike-stack-forward:9987 mike-stack-assistant:9985 mike-stack-spa:9990; do
        unit="${pair%%:*}"; port="${pair##*:}"
        unit_active "${unit}" && continue
        adopt_port "${unit}" "${port}"
        echo "==> ${unit} was not running — starting it"
        unit_start "${unit}" || echo "==> ${unit} failed to start — see systemctl --user status ${unit}" >&2
    done
}
