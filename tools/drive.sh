#!/usr/bin/env bash
# Drives the story in a window on a private display, sending synthetic input, and captures a
# frame — with no possibility of reaching the operator's desktop.
#
# Usage:
#   tools/drive.sh                                      # advance once, capture at the end
#   tools/drive.sh --keys Return,click1,q               # keys and clicks, in order
#   tools/drive.sh --out frame.png --size 1280x720
#   tools/drive.sh --read-frames                        # one PNG per presented command
#
# Why `xvfb-run` and not a hardcoded `DISPLAY=:99`
# ------------------------------------------------
# A windowed run is the one thing in this project that can appear on someone's screen, and an
# earlier version of this harness put one there. The cause was environmental: `DISPLAY=:99`
# was set as a *prefix* on a backgrounded command, the prefix did not reach the process, and
# the app connected to the inherited `DISPLAY` — the operator's desktop.
#
# `xvfb-run` cannot fail that way: it allocates a display, sets `DISPLAY` in the child's own
# environment, and tears the server down afterwards. There is no code path in which the app
# sees the operator's display, which is the property worth having, rather than a number that
# is *usually* right.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out="frame.png"
size="1280x720"
keys="Return"
project="examples/hello"
read_frames=false

while [[ $# -gt 0 ]]; do
    case "${1}" in
        --out) out="${2}"; shift 2 ;;
        --size) size="${2}"; shift 2 ;;
        --keys) keys="${2}"; shift 2 ;;
        --project) project="${2}"; shift 2 ;;
        --read-frames) read_frames=true; shift ;;
        -h|--help) sed -n '2,12p' "${BASH_SOURCE[0]}"; exit 0 ;;
        -*) echo "unknown flag: ${1}" >&2; exit 2 ;;
        *) project="${1}"; shift ;;
    esac
done

if ! command -v xvfb-run >/dev/null 2>&1; then
    echo "tools/drive.sh needs xvfb-run (xorg-server-xvfb)" >&2
    exit 1
fi
if ! command -v xdotool >/dev/null 2>&1; then
    echo "tools/drive.sh needs xdotool, to send input" >&2
    exit 1
fi

mkdir -p "$(dirname "${out}")"

# Everything below runs inside `xvfb-run`, so `DISPLAY` is the private server. The script
# asserts that rather than assuming it: if this ever prints `:0`, the run is about to appear
# on someone's screen and must stop.
driver=$(mktemp)
cat > "${driver}" <<'DRIVER'
set -euo pipefail
repo_root="${1}"; shift
out="${1}"; shift
size="${1}"; shift
keys="${1}"; shift
project="${1}"; shift

case "${DISPLAY}" in
    :0|:0.*|"")
        echo "refusing to run: DISPLAY is '${DISPLAY}', which is the operator's desktop" >&2
        exit 3
        ;;
esac

# Unset the Wayland session variables, or none of the above matters.
#
# `winit` prefers Wayland whenever `WAYLAND_DISPLAY` is set, and ignores `DISPLAY` entirely
# when it does. So on a Wayland desktop the app opened its window on the *operator's session*
# no matter what display this script set, `xvfb-run`'s private server went unused, and
# `xdotool` could never deliver a key — the app was not an X11 client at all. One cause
# explains every symptom, including the ones that looked like separate bugs.
#
# This is the environment leaking into the sandbox, so the sandbox closes it here rather than
# asking every caller to remember.
unset WAYLAND_DISPLAY WAYLAND_SOCKET XDG_SESSION_TYPE
printf 'drive: project=%s display=%s size=%s keys=%s\n' "${project}" "${DISPLAY}" "${size}" "${keys}"

log=$(mktemp)
# The story prints each presented command, which is how this script knows a keypress landed
# without being able to see the window.

unset WAYLAND_DISPLAY
unset WAYLAND_SOCKET

VELA_INPUT_TRACE=1 "${repo_root}/target/debug/vela" run "${project}" --size "${size}" >"${log}" 2>&1 &
app=$!
trap 'kill ${app} 2>/dev/null || true' EXIT

# Wait for the window to exist and the first command to be presented.
for _ in $(seq 1 60); do
    if grep -q '^present ' "${log}" 2>/dev/null; then
        break
    fi
    if ! kill -0 "${app}" 2>/dev/null; then
        echo "the app exited before presenting anything:" >&2
        cat "${log}" >&2
        exit 1
    fi
    sleep 0.25
done

if ! grep -q '^present ' "${log}" 2>/dev/null; then
    echo "no command was presented within 15s" >&2
    cat "${log}" >&2
    exit 1
fi

echo "--- the app's own account of itself ---"
head -4 "${log}"
echo "--------------------------------------"

window="$(xdotool search --name 'main' | head -1 || true)"
if [[ -z "${window}" ]]; then
    window="$(xdotool search --name '.*' | head -1 || true)"
fi
printf 'drive: window=%s\n' "${window:-<none>}"

# Did a window reach the operator's desktop? Asked directly of that display rather than
# inferred from ours.
#
# Reachability is asked with `getdisplaygeometry`, not `search`: `search` has no `--display`
# flag *and* exits 1 when it finds nothing, so an earlier version of this probe reported
# "not reachable" for both reasons while looking like evidence that the sandbox held.
if DISPLAY=:0 xdotool getdisplaygeometry >/dev/null 2>&1; then
    found="$(DISPLAY=:0 xdotool search --name 'main' 2>/dev/null | head -1 || true)"
    printf 'drive: :0 IS reachable from here; our window on it: %s\n' "${found:-no}"
else
    printf 'drive: :0 refuses the connection from here (the sandbox holds)\n'
fi

# Focus first, then send real events.
#
# `xdotool key --window` uses `XSendEvent`, which marks the event synthetic and which a toolkit
# is entitled to ignore — winit does. And with no window manager there is nothing to give the
# window focus at all, so a plain keypress goes nowhere: `XGetInputFocus` reports the root.
# `windowfocus` calls `XSetInputFocus` directly, which needs no window manager, and a plain
# `xdotool key` then goes through XTEST as a genuine device event.
if [[ -n "${window}" ]]; then
    xdotool windowfocus --sync "${window}" 2>/dev/null || true
    sleep 0.2
    printf 'drive: focus is now %s\n' "$(xdotool getwindowfocus 2>&1 | head -1)"
fi

# Send the keys, one at a time, waiting for the presentation count to move after each.
before=$(grep -c '^present ' "${log}" || true)
index=0
IFS=',' read -r -a sequence <<<"${keys}"
for key in "${sequence[@]}"; do
    index=$((index + 1))
    # A click has to be *over* the window, and with no window manager the pointer starts
    # wherever the server left it. Moving first is what makes the difference between testing
    # the click handler and testing the pointer's default position.
    if [[ "${key}" == click* ]]; then
        xdotool mousemove --sync 100 100
        xdotool click "${key#click}"
    else
        xdotool key "${key}"
    fi
    for _ in $(seq 1 40); do
        now=$(grep -c '^present ' "${log}" || true)
        if [[ "${now}" -gt "${before}" ]]; then
            break
        fi
        sleep 0.1
    done
    after=$(grep -c '^present ' "${log}" || true)
    printf 'drive: key %d (%s) presented %s -> %s\n' "${index}" "${key}" "${before}" "${after}"
    before="${after}"
done

# The frame is captured by the app itself, offscreen, from the same session that just ran —
# which is why the input above and the picture below are the same run.
"${repo_root}/target/debug/vela" run "${project}" --headless --capture "${out}" --size "${size}" >/dev/null

kill "${app}" 2>/dev/null || true
wait "${app}" 2>/dev/null || true
echo "drive: captured ${out}"
DRIVER
chmod +x "${driver}"

xvfb-run -a -s "-screen 0 ${size}x24" "${driver}" \
    "${repo_root}" "${out}" "${size}" "${keys}" "${project}"
status=$?
rm -f "${driver}"
exit "${status}"
