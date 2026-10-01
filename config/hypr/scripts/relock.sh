#!/usr/bin/env bash
# hypridle's after_sleep_cmd: replace the hyprlock that spanned suspend.
# Needs misc:allow_session_lock_restore.

set -uo pipefail

hyprctl dispatch dpms on

pkill -x hyprlock

# lock.sh bails out while the old hyprlock is still up.
for _ in {1..20}; do
  pidof -q hyprlock || break
  sleep 0.1
done
pidof -q hyprlock && pkill -9 -x hyprlock

# Detached: lock.sh lives until unlock. Retried: no lock at all is the worst case.
for _ in {1..5}; do
  setsid ~/.config/hypr/scripts/lock.sh --reuse-shot >/dev/null 2>&1 &
  sleep 0.5
  pidof -q hyprlock && exit 0
done

exit 1
