#!/usr/bin/env bash
# Close the swaync panel once the cursor has been outside it for 3 seconds.

set -uo pipefail

exec 9>"${XDG_RUNTIME_DIR:-/tmp}/swaync-autoclose.lock"
flock -n 9 || exit 0

timeout=3000 # ms outside before closing
interval=0.25

now_ms() {
  local t=${EPOCHREALTIME/./}
  echo $((t / 1000))
}

# panel_rect {width} {gap} -> "x1 y1 x2 y2" of the right column; fails once closed
panel_rect() {
  local width=$1 gap=$2

  hyprctl layers -j | jq -er --argjson w "$width" --argjson g "$gap" '
    first(.[].levels[][] | select(.namespace == "swaync-control-center"))
    | "\(.x + .w - $g - $w) \(.y + $g) \(.x + .w - $g) \(.y + .h - $g)"'
}

watch_panel() {
  local width gap left rect x1 y1 x2 y2 cx cy
  width=$(jq '."control-center-width" // 500' ~/.config/swaync/config.json) || return
  gap=$(hyprctl getoption general:gaps_out -j | jq '.css | split(" ")[0] | tonumber') || return
  left=$(now_ms)

  while rect=$(panel_rect "$width" "$gap"); do
    read -r x1 y1 x2 y2 <<<"$rect"
    read -r cx cy < <(hyprctl cursorpos -j | jq -r '"\(.x) \(.y)"')

    if ((cx >= x1 && cx < x2 && cy >= y1 && cy < y2)); then
      left=$(now_ms)
    elif (($(now_ms) - left >= timeout)); then
      swaync-client -cp >/dev/null
      return
    fi
    sleep "$interval"
  done
}

# Resubscribe if swaync restarts.
while true; do
  swaync-client -s | while read -r event; do
    [ "$(jq -r .visible <<<"$event")" = true ] && watch_panel
  done
  sleep 2
done
