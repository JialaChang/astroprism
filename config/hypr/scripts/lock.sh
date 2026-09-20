#!/usr/bin/env bash
# Pause media, then lock. Every lock path goes through here.

set -uo pipefail

pidof -q hyprlock && exit 0

command -v playerctl >/dev/null && playerctl pause 2>/dev/null

# --from-rofi: rofi has already exited by now, but its frame lingers, so give
# the compositor time to redraw or the screenshot backend catches it.
if [ "${1:-}" = --from-rofi ]; then
  shift
  sleep 0.05
fi

exec hyprlock "$@"
