#!/usr/bin/env bash
# Now-playing line for hyprlock. Prints nothing when nothing is playing.

set -uo pipefail

command -v playerctl >/dev/null || exit 0

status=$(playerctl status 2>/dev/null) || exit 0
[ "$status" = Playing ] || [ "$status" = Paused ] || exit 0

title=$(playerctl metadata --format '{{title}}' 2>/dev/null)
artist=$(playerctl metadata --format '{{artist}}' 2>/dev/null)

[ -n "$title" ] || exit 0

# Truncated so a long title can't run into the battery in the other corner.
trim() {
  local s=$1 max=$2
  if (( ${#s} > max )); then printf '%s…' "${s:0:max-1}"; else printf '%s' "$s"; fi
}

if [ "$status" = Playing ]; then
  icon=' '
else
  icon=' '
fi

if [ -n "$artist" ]; then
  printf '%s  %s — %s\n' "$icon" "$(trim "$title" 32)" "$(trim "$artist" 20)"
else
  printf '%s  %s\n' "$icon" "$(trim "$title" 48)"
fi
