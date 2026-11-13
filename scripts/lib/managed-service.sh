#!/usr/bin/env bash
# managed-service.sh — find and drive the background service `mike service install` writes.
#
# On a machine where mike runs as an installed service (launchd agents on macOS, systemd user
# units on Linux) the supervisor owns the proxy process, so a deploy has to replace the binary
# the service file names and restart that service, not start a process of its own. These
# functions read only the environment (HOME, XDG_CONFIG_HOME, XDG_STATE_HOME,
# MANAGED_SERVICE_PLATFORM) and the service definition, so a test can point them at a
# temporary HOME. Only managed_service_pid, managed_service_restart and managed_service_stop call
# launchctl or systemctl, and only managed_service_program, managed_service_log and the plist
# branch call plutil. The cutover functions further down read the globals deploy-proxy.sh sets.
#
# Directory and file names mirror crates/mike/src/service/manager.rs (Manager::from_process),
# the marker mirrors crates/mike/src/service/files.rs (MARKER, is_ours), and the file shapes
# mirror launchd.rs and systemd.rs in the same folder.

MANAGED_SERVICE_MARKER="Managed by mike service install"
# files::is_ours looks for the marker only in the first lines of the file.
MANAGED_SERVICE_MARKER_SCAN_LINES=12

# launchd on macOS, systemd elsewhere. MANAGED_SERVICE_PLATFORM overrides, for tests.
managed_service_platform() {
    if [[ -n "${MANAGED_SERVICE_PLATFORM:-}" ]]; then
        printf '%s\n' "${MANAGED_SERVICE_PLATFORM}"
    elif [[ "$(uname -s)" == "Darwin" ]]; then
        echo launchd
    else
        echo systemd
    fi
}

# $1 = proxy | forward.
managed_service_identifier() {
    case "$(managed_service_platform):$1" in
        launchd:proxy)   echo ai.temet.mike.proxy ;;
        launchd:forward) echo ai.temet.mike.forward ;;
        launchd:assistant) echo ai.temet.mike.assistant ;;
        systemd:proxy)   echo mike.service ;;
        systemd:forward) echo mike-forward.service ;;
        *) return 1 ;;
    esac
}

# The definition file. An XDG_CONFIG_HOME that is not an absolute path is ignored, as in mike.
managed_service_file() {
    local id dir
    id="$(managed_service_identifier "$1")" || return 1
    if [[ "$(managed_service_platform)" == "launchd" ]]; then
        printf '%s\n' "${HOME}/Library/LaunchAgents/${id}.plist"
    else
        case "${XDG_CONFIG_HOME:-}" in
            /*) dir="${XDG_CONFIG_HOME}" ;;
            *)  dir="${HOME}/.config" ;;
        esac
        printf '%s\n' "${dir}/systemd/user/${id}"
    fi
}

# 0 when the proxy's definition file exists and carries the marker mike writes.
managed_service_installed() {
    local file
    file="$(managed_service_file proxy)" || return 1
    [[ -f "${file}" ]] || return 1
    head -n "${MANAGED_SERVICE_MARKER_SCAN_LINES}" "${file}" 2>/dev/null \
        | grep -qF "${MANAGED_SERVICE_MARKER}"
}

# The text of the first <string> that follows <key>$1</key> inside the plist on stdin, with the
# XML escapes launchd.rs applies undone. For ProgramArguments that is the first array element.
_managed_service_plist_string() {
    awk -v key="$1" '
        function unescape(s) {
            gsub(/&lt;/, "<", s); gsub(/&gt;/, ">", s); gsub(/&amp;/, "\\&", s)
            return s
        }
        index($0, "<key>" key "</key>") { want = 1 }
        want && match($0, /<string>[^<]*<\/string>/) {
            print unescape(substr($0, RSTART + 8, RLENGTH - 17))
            exit
        }
        want && /<key>/ && !index($0, "<key>" key "</key>") { want = 0 }
    '
}

# The executable the proxy service runs.
managed_service_program() {
    local file value=""
    file="$(managed_service_file proxy)" || return 1
    [[ -f "${file}" ]] || return 1
    if [[ "$(managed_service_platform)" == "launchd" ]]; then
        if command -v plutil >/dev/null 2>&1; then
            value="$(plutil -extract ProgramArguments.0 raw -o - "${file}" 2>/dev/null || true)"
        fi
        [[ -n "${value}" ]] || value="$(_managed_service_plist_string ProgramArguments <"${file}")"
    else
        # First word of ExecStart=, which systemd.rs double-quotes when it holds whitespace.
        value="$(awk '
            /^ExecStart=/ {
                line = substr($0, 11)
                if (substr(line, 1, 1) == "\"") {
                    line = substr(line, 2); out = ""
                    while (length(line) > 0) {
                        c = substr(line, 1, 1)
                        if (c == "\\") { out = out substr(line, 2, 1); line = substr(line, 3); continue }
                        if (c == "\"") break
                        out = out c; line = substr(line, 2)
                    }
                    print out
                } else {
                    sub(/[ \t].*$/, "", line); print line
                }
                exit
            }
        ' "${file}")"
        value="${value//%%/%}"
        value="${value//\$\$/\$}"
    fi
    [[ -n "${value}" ]] || return 1
    printf '%s\n' "${value}"
}

# Where the proxy service's output goes: a file path, or the journalctl command that shows it.
managed_service_log() {
    local file value=""
    file="$(managed_service_file proxy)" || return 1
    if [[ "$(managed_service_platform)" == "launchd" ]]; then
        if [[ -f "${file}" ]]; then
            if command -v plutil >/dev/null 2>&1; then
                value="$(plutil -extract StandardOutPath raw -o - "${file}" 2>/dev/null || true)"
            fi
            [[ -n "${value}" ]] || value="$(_managed_service_plist_string StandardOutPath <"${file}")"
        fi
        case "${XDG_STATE_HOME:-}" in
            /*) : "${value:=${XDG_STATE_HOME}/mike/logs/proxy.log}" ;;
            *)  : "${value:=${HOME}/.local/state/mike/logs/proxy.log}" ;;
        esac
        printf '%s\n' "${value}"
    else
        # The unit appends to a file (StandardOutput=append:<path>); say so when it does.
        [[ -f "${file}" ]] && value="$(sed -n 's/^StandardOutput=append://p' "${file}" | head -n1)"
        value="${value//%%/%}"
        if [[ -n "${value}" ]]; then
            printf '%s\n' "${value}"
        else
            echo "journalctl --user -u $(managed_service_identifier proxy)"
        fi
    fi
}

# The process id of a service ($1 = proxy, the default, forward or assistant), or nothing when it
# is not running.
managed_service_pid() {
    local kind="${1:-proxy}" pid=""
    if [[ "$(managed_service_platform)" == "launchd" ]]; then
        pid="$(launchctl print "gui/$(id -u)/$(managed_service_identifier "${kind}")" 2>/dev/null \
            | awk '$1 == "pid" && $2 == "=" { print $3; exit }' || true)"
    else
        pid="$(systemctl --user show -p MainPID --value "$(managed_service_identifier "${kind}")" 2>/dev/null || true)"
    fi
    [[ "${pid}" =~ ^[1-9][0-9]*$ ]] && printf '%s\n' "${pid}"
    return 0
}

# Restart one service, or start it when scripts/stop-proxy.sh has unloaded it. launchd refuses a
# bootstrap for a moment after a bootout, so that call is retried.
managed_service_restart() {
    local kind="$1" id tries
    id="$(managed_service_identifier "${kind}")" || return 1
    if [[ "$(managed_service_platform)" != "launchd" ]]; then
        systemctl --user restart "${id}"
        return
    fi
    if launchctl print "gui/$(id -u)/${id}" >/dev/null 2>&1; then
        launchctl kickstart -k "gui/$(id -u)/${id}"
        return
    fi
    for tries in 1 2 3 4 5; do
        launchctl bootstrap "gui/$(id -u)" "$(managed_service_file "${kind}")" && return 0
        (( tries < 5 )) && sleep 1
    done
    return 1
}

# Stop one service so that its supervisor does not bring it back. It stays stopped until
# managed_service_restart or the next login.
managed_service_stop() {
    local kind="$1" id
    id="$(managed_service_identifier "${kind}")" || return 1
    if [[ "$(managed_service_platform)" != "launchd" ]]; then
        systemctl --user stop "${id}"
    elif launchctl print "gui/$(id -u)/${id}" >/dev/null 2>&1; then
        launchctl bootout "gui/$(id -u)/${id}"
    fi
}

# For a script that binds mike's own ports: stop here while the installed service runs. Its
# supervisor would restart anything the script killed and take the installation's owner lock
# first. $1 = what the script does, for the message.
refuse_while_service_runs() {
    managed_service_installed || return 0
    [[ -n "$(managed_service_pid)" ]] || return 0
    echo "$1 cannot run beside the installed service $(managed_service_identifier proxy) (pid $(managed_service_pid))." >&2
    echo "  launchd or systemd would restart anything this script stopped. Nothing was touched." >&2
    echo "  Run ./scripts/stop-proxy.sh first, and ./scripts/start-proxy.sh to bring the service back." >&2
    exit 1
}

# Write the launchd agent that keeps the prompt assistant running; launchd has no development
# stack units to do it. $1 = checkout, $2 = port, $3 = inspect address. Reloads only when the
# definition changed.
install_assistant_agent() {
    local root="$1" port="$2" inspect="$3" file rendered
    file="$(managed_service_file assistant)"
    rendered="$(sed -e "s#@PROJECT_ROOT@#${root}#g" -e "s#@PATH@#${PATH}#g" \
        -e "s#@PORT@#${port}#g" -e "s#@INSPECT_ADDR@#${inspect}#g" \
        "${root}/scripts/launchd/ai.temet.mike.assistant.plist.in")"
    [[ -f "${file}" && "$(cat "${file}")" == "${rendered}" ]] && return 0
    managed_service_stop assistant || true
    printf '%s\n' "${rendered}" >"${file}"
}

# The files whose change means the sidecar's code changed. `mike forward serve` is this code,
# linked into the mike binary.
SIDECAR_SOURCES=(crates/proxy-forward crates/mike/src/forward.rs Cargo.lock)

# Wait until the sidecar runs as a process other than $1 and answers on its port.
_wait_for_sidecar() {
    local old="$1" pid deadline=$(( SECONDS + 30 ))
    while (( SECONDS < deadline )); do
        pid="$(managed_service_pid forward)"
        if [[ -n "${pid}" && "${pid}" != "${old}" ]] \
            && curl --silent --output /dev/null --max-time 1 "http://127.0.0.1:${MIKE_FORWARD_PORT:-9987}/"; then
            return 0
        fi
        sleep 0.2
    done
    return 1
}

# Keep the sidecar running and on current code without restarting it for nothing: a restart
# breaks every stream open through it, so it happens only when the sidecar's own code changed
# since the commit recorded at its last restart, or when it is down. Run this once the proxy is
# healthy, so the sidecar has somewhere to send traffic. Without a record the commit of the
# previous binary (PREV_BIN) stands in. 0 = up to date, 1 = it did not come back.
sync_service_sidecar() {
    local record="${DEPLOY_DIR}/service-forward.commit" head base="" old
    head="$(git -C "${PROJECT_ROOT}" rev-parse HEAD)"
    old="$(managed_service_pid forward)"
    [[ -f "${record}" ]] && base="$(cat "${record}")"
    [[ -n "${base}" || ! -x "${PREV_BIN}" ]] \
        || base="$("${PREV_BIN}" --version 2>/dev/null | sed -nE 's/.*\(([0-9a-f]{7,}).*/\1/p')"
    if [[ -n "${old}" ]]; then
        if ! git -C "${PROJECT_ROOT}" cat-file -e "${base:-none}^{commit}" 2>/dev/null; then
            echo "==> Cannot tell which build the sidecar runs. 'mike service restart' puts it on this one." >&2
            return 0
        fi
        if git -C "${PROJECT_ROOT}" diff --quiet "${base}" "${head}" -- "${SIDECAR_SOURCES[@]}"; then
            echo "==> The sidecar's code has not changed since ${base:0:9} — leaving it running"
            printf '%s\n' "${head}" >"${record}"
            return 0
        fi
        echo "==> The sidecar's code changed since ${base:0:9} — restarting it (open streams through it will retry)"
    else
        echo "==> The sidecar is not running — starting it"
    fi
    if managed_service_restart forward && _wait_for_sidecar "${old}"; then
        printf '%s\n' "${head}" >"${record}"
        return 0
    fi
    echo "==> The sidecar did not come back. 'mike service status' shows its state." >&2
    return 1
}

# The installed service serves the web UI embedded in its binary, which is compiled from
# frontend/build. This script does not rebuild that folder, so say when it predates the newest
# frontend commit: the deploy would put an older web UI live.
warn_if_web_ui_is_stale() {
    local built changed
    built="$(date -r "${PROJECT_ROOT}/frontend/build/index.html" +%s 2>/dev/null || echo 0)"
    changed="$(git -C "${PROJECT_ROOT}" log -1 --format=%ct -- frontend/src frontend/static \
        ':(exclude,glob)frontend/src/**/*.stories.*' ':(exclude,glob)frontend/src/**/*.test.*' 2>/dev/null || true)"
    if (( built < ${changed:-0} )); then
        echo "==> WARNING: frontend/build is older than the newest frontend commit, so this deploy" >&2
        echo "    carries an older web UI. Run 'make spa' and deploy again to bring it up to date." >&2
    fi
}

# Put binary $1 where the service runs it. The copy is made beside it and renamed over it, so
# the running process keeps its own inode (cp over a running binary SIGKILLs it on macOS) and
# SERVICE_BIN is never missing. 0 = replaced, 1 = nothing changed.
install_service_binary() {
    local tmp="${SERVICE_BIN}.deploy.$$"
    if cp "$1" "${tmp}" && chmod +x "${tmp}" && mv -f "${tmp}" "${SERVICE_BIN}"; then
        return 0
    fi
    rm -f "${tmp}"
    return 1
}

# Restart the proxy service and wait for a process with a new boot generation to answer, for
# SERVICE_WAIT_SECONDS. The smoke boot has already applied any migration, so a build that has
# not answered by then is broken and every extra second is outage. $1 = label. A failed restart
# command is a failure too.
restart_service_and_wait() {
    local before
    before="$(health_boot_generation "${INSPECT_ADDR}")"
    managed_service_restart proxy || {
        echo "==> Restarting ${SERVICE_ID} failed ('mike service status' shows its state)" >&2
        return 1
    }
    WAIT_MAX_SECONDS="${SERVICE_WAIT_SECONDS:-60}" wait_new_generation "${INSPECT_ADDR}" "${before}" "$1"
}

# Cutover under an installed service (see the header of deploy-proxy.sh). Always exits: 0 when
# the new build answers as a new process, 1 otherwise. Frees no port, kills no pid, starts no
# process; the supervisor does the starting.
cutover_via_service() {
    local service_log
    service_log="$(managed_service_log || true)"
    warn_if_web_ui_is_stale
    echo "==> Cutover: replacing ${SERVICE_BIN} and restarting ${SERVICE_ID}"
    if ! install_service_binary "${STAGED_BIN}"; then
        echo "==> Could not write ${SERVICE_BIN} — the service was not touched." >&2
        exit 1
    fi
    CUTOVER_DONE="true"   # the live binary is replaced; the abort notice would be wrong from here
    if restart_service_and_wait "new proxy"; then
        verify_live_build_sha "${INSPECT_ADDR}"
        # Record what is now live so the NEXT deploy's smart-skip and .prev are trustworthy.
        rm -f "${DEPLOYED_BIN}"
        cp "${STAGED_BIN}" "${DEPLOYED_BIN}"
        git -C "${PROJECT_ROOT}" rev-parse HEAD >"${DEPLOY_DIR}/mike.deployed.commit" 2>/dev/null || true
        printf '%s\n' "${BUILD_PROFILE:-release}" >"${DEPLOY_DIR}/mike.deployed.profile"
        rm -f "${PID_FILE}"   # the supervisor owns the pid; a pid file of ours would only mislead
        refresh_mike_cli || true
        sync_service_sidecar || exit 1
        echo
        echo "==> Cutover complete. The new proxy is live and healthy under ${SERVICE_ID}."
        echo "    proxy bind:   http://${MIKE_ADDR}/v1/"
        echo "    inspect bind: http://${INSPECT_ADDR}/api/"
        echo "    service log:  ${service_log}"
        echo "    'mike service status' shows both services."
        exit 0
    fi

    echo "==> The new proxy did not come up under ${SERVICE_ID} — service log: ${service_log}" >&2
    if [[ -f "${service_log}" ]]; then tail -n 30 "${service_log}" >&2 || true; fi
    if [[ -x "${PREV_BIN}" ]]; then
        echo "==> Rolling back to the previous binary (${PREV_BIN})" >&2
        echo "    NOTE: rollback is only safe if this deploy's migrations were additive." >&2
        if install_service_binary "${PREV_BIN}" && restart_service_and_wait "rolled-back proxy"; then
            rm -f "${DEPLOYED_BIN}"
            cp "${PREV_BIN}" "${DEPLOYED_BIN}"
            echo "==> Rollback succeeded — the previous binary is live under ${SERVICE_ID}." >&2
            echo "    The new build failed its health check. Service log: ${service_log}" >&2
            exit 1
        fi
        echo "==> ROLLBACK ALSO FAILED. The proxy service is DOWN; ${SERVICE_ID} keeps retrying ${SERVICE_BIN}." >&2
        echo "    Inspect the service log (${service_log}) and ${SMOKE_LOG}." >&2
        exit 1
    fi
    echo "==> No rollback binary available. The proxy service is DOWN; ${SERVICE_ID} keeps retrying ${SERVICE_BIN}." >&2
    echo "    Inspect the service log (${service_log}) and ${SMOKE_LOG}." >&2
    exit 1
}

# instance.boot_generation from an /api/health body on stdin. It changes with every process
# start, so it tells a new process from the one that was answering before. Prints nothing when
# the body does not carry one. Newlines count as whitespace, so a pretty-printed body parses.
parse_boot_generation() {
    tr '\n' ' ' \
        | grep -oE '"boot_generation"[[:space:]]*:[[:space:]]*"[^"]+"' \
        | head -n1 \
        | sed -E 's/.*:[[:space:]]*"([^"]+)"/\1/' \
        || true
}

# The boot generation the inspect API at $1 (host:port) reports, or nothing when it does not answer.
health_boot_generation() {
    local body
    body="$(curl --silent --fail --max-time 2 "http://$1/api/health" 2>/dev/null || true)"
    printf '%s' "${body}" | parse_boot_generation
}

# Wait until the inspect API at $1 answers with a boot generation that differs from $2 (empty
# when nothing was answering before), for at most WAIT_MAX_SECONDS. $3 = label for messages.
# A process id is no handle once a supervisor restarts things, and any 200 on the port could be
# the process that was already there, so a changed boot generation is the only proof here that
# the process we started is the one answering.
wait_new_generation() {
    local addr="$1" old="$2" label="$3" now deadline
    deadline=$(( SECONDS + ${WAIT_MAX_SECONDS:-300} ))
    while :; do
        now="$(health_boot_generation "${addr}")"
        if [[ -n "${now}" && "${now}" != "${old}" ]]; then
            return 0
        fi
        if (( SECONDS >= deadline )); then
            echo "==> ${label} did not answer as a new process within ${WAIT_MAX_SECONDS:-300}s" >&2
            return 1
        fi
        sleep 0.2
    done
}
