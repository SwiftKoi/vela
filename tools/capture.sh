#!/usr/bin/env bash
# Renders a frame of a project to a PNG.
#
# Usage:
#   tools/capture.sh examples/hello                          # -> frame.png
#   tools/capture.sh --out frame.png --size 1920x1080 examples/hello
#   tools/capture.sh --frame 3 examples/hello                # a specific command
#   tools/capture.sh --ascii                                 # exact pixels, no viewer
#
# How this differs from a screenshot scraper, and why
# ---------------------------------------------------
# It does not screenshot anything. The application renders into a texture and writes its own
# framebuffer; this script only drives it. That is the same trick as the Godot harness in
# RoguelikeAlegacy (`game/scripts/screenshot.sh`), where `--headless` uses a dummy renderer
# and cannot produce an image, so the app is launched display-backed under Xvfb and saves its
# own viewport.
#
# Vela does not even need the Xvfb half of it. Verified on this machine:
#
#   * `import -window root` against a Vulkan swapchain under Xvfb captures *black*, because
#     presentation goes through DRI3/Present and never reaches Xvfb's framebuffer. Scraping
#     X is a dead end, not a shortcut.
#   * wgpu rendering to a texture, reading it back, and writing a PNG works with **no
#     display at all** — the adapter is the real GPU either way (`Intel UHD`, Vulkan).
#
# So this runs in CI, on a laptop, and in a container identically, which is what the layout
# and frame goldens require. No Xvfb, no compositor, no window.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out="frame.png"
size="1280x720"
mode="png"
project="examples/hello"
frame_args=()

while [[ $# -gt 0 ]]; do
    case "${1}" in
        --out) out="${2}"; shift 2 ;;
        --size) size="${2}"; shift 2 ;;
        --ascii) mode="ascii"; shift ;;
        --frame) frame_args+=(--frame "${2}"); shift 2 ;;
        --project) project="${2}"; shift 2 ;;
        -h|--help) sed -n '2,9p' "${BASH_SOURCE[0]}"; exit 0 ;;
        -*) echo "unknown flag: ${1}" >&2; exit 2 ;;
        *) project="${1}"; shift ;;
    esac
done

# `--ascii` prints the frame as text, which is what a geometry bug needs: exact pixels with
# no viewer and no image encoding in between. It renders a synthetic scene, because the point
# is the rasteriser rather than the story.
if [[ "${mode}" == "ascii" ]]; then
    exec cargo run -q -p vela-render --example capture -- --ascii "${size}"
fi

mkdir -p "$(dirname "${out}")"
cargo run -q -p vela-cli -- run "${project}" --headless \
    --capture "${out}" --size "${size}" "${frame_args[@]}"
echo "capture: ${out}"
