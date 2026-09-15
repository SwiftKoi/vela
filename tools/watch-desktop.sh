#!/usr/bin/env bash
# Reports any window that appears on the operator's display while a command runs.
#
# Usage:
#   tools/watch-desktop.sh tools/drive.sh --keys Return
#
# Why this exists
# ---------------
# A window appeared on the operator's desktop during automated runs, twice, and reading our
# own logs could not account for it: every run reported `display=:99`, and `drive.sh` cannot
# even authenticate to `:0`. Reasoning from the inside could not settle it, so this asks the
# *outside*: it snapshots the window list on the real display before the command starts, then
# polls while the command runs and reports every window that is new — with its id, its title,
# and the pid of the process that owns it.
#
# The pid is the part that ends the argument. A window that appears here with a pid we can
# name is a fact about which process drew it, not an inference.
set -uo pipefail

if [[ $# -eq 0 ]]; then
    echo "usage: watch-desktop.sh COMMAND [ARGS...]" >&2
    exit 2
fi

watched="${VELA_WATCH_DISPLAY:-:0}"

# Two ways to get this wrong, both of which *read* as a finding:
#
#  1. `xdotool search` has no `--display`. It reads `DISPLAY`, like every other X client, and
#     the unrecognised flag fails. The first version did this and reported "cannot read :0".
#  2. `xdotool search` also exits 1 when it finds *no windows*, which is not a failure to
#     connect. The second version did this and reported an empty display as unreachable.
#
# So reachability is asked of a command that means it: `getdisplaygeometry` succeeds when the
# connection does, whatever windows exist.
if ! DISPLAY="${watched}" xdotool getdisplaygeometry >/dev/null 2>&1; then
    echo "watch: cannot read ${watched} — nothing can open a window there either" >&2
    exit 1
fi
printf 'watch: watching %s (pid %d)\n' "${watched}" "$$"

snapshot() {
    DISPLAY="${watched}" xdotool search --name '.*' 2>/dev/null | sort
}

before="$(snapshot)"
printf 'watch: %s window(s) already open\n' "$(printf '%s' "${before}" | grep -c . || true)"

"$@" &
watched_pid=$!

seen=" ${before//$'\n'/ } "
found=0
while kill -0 "${watched_pid}" 2>/dev/null; do
    for id in $(snapshot); do
        case "${seen}" in *" ${id} "*) continue ;; esac
        seen="${seen} ${id} "
        found=$((found + 1))

        name="$(DISPLAY="${watched}" xdotool getwindowname "${id}" 2>/dev/null || echo '<unnamed>')"
        owner="$(DISPLAY="${watched}" xdotool getwindowpid "${id}" 2>/dev/null || echo '?')"
        command="?"
        if [[ "${owner}" != "?" && -r "/proc/${owner}/cmdline" ]]; then
            command="$(tr '\0' ' ' <"/proc/${owner}/cmdline" 2>/dev/null | cut -c1-100)"
        fi
        printf 'watch: NEW WINDOW id=%s name=%q pid=%s cmd=%q\n' \
            "${id}" "${name}" "${owner}" "${command}"
        printf 'watch: NEW WINDOW id=%s name=%q pid=%s cmd=%q\n' \
            "${id}" "${name}" "${owner}" "${command}" >>"${VELA_WATCH_LOG:-/dev/null}"
    done
    sleep 0.2
done

wait "${watched_pid}"
status=$?
printf 'watch: finished, %d new window(s) on %s\n' "${found}" "${watched}"
exit "${status}"
