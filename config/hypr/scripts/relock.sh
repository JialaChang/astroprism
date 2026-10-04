#!/usr/bin/env bash
# hypridle's after_sleep_cmd: replace the hyprlock that spanned suspend.
# Needs misc:allow_session_lock_restore.

set -uo pipefail

# Screen turns on last, so the swap happens while it is still dark.
screen_on() { hyprctl dispatch 'hl.dsp.dpms({ action = "on" })'; }

pkill -x hyprlock

# lock.sh bails out while the old hyprlock is still up.
for _ in {1..20}; do
  pidof -q hyprlock || break
  sleep 0.1
done
pidof -q hyprlock && pkill -9 -x hyprlock

# Detached: lock.sh lives until unlock. Retried only once the last one is gone,
# so a slow start can't spawn a second hyprlock; no lock at all is the worst case.
for _ in {1..5}; do
  setsid ~/.config/hypr/scripts/lock.sh --reuse-shot >/dev/null 2>&1 &
  pid=$!
  for _ in {1..30}; do
    if pidof -q hyprlock; then
      sleep 0.3 # let it draw a first frame
      screen_on
      exit 0
    fi
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
done

screen_on
exit 1
