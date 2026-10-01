#!/usr/bin/env bash
# Pause media, then lock. Every lock path goes through here.
# Flags: --from-rofi (wait for rofi's frame to clear), --reuse-shot (relock.sh).

set -uo pipefail

pidof -q hyprlock && exit 0

command -v playerctl >/dev/null && playerctl pause 2>/dev/null

from_rofi=false reuse_shot=false
while [ $# -gt 0 ]; do
  case $1 in
    --from-rofi) from_rofi=true ;;
    --reuse-shot) reuse_shot=true ;;
    *) break ;;
  esac
  shift
done

# Its frame lingers, so give the compositor time to redraw or grim catches it.
[ "$from_rofi" = true ] && sleep 0.05

# Own shot, since hyprlock's `screenshot` would capture the lock on a relock.
# Runtime dir (0700), not /tmp: it shows whatever was on screen.
shot=${XDG_RUNTIME_DIR:-/tmp}/hyprlock-bg.png
if [ "$reuse_shot" = false ]; then
  # Timeout: a capture that stalls across suspend would leave the machine unlocked.
  (umask 077; timeout 2 grim "$shot" 2>/dev/null) || rm -f "$shot"
fi

hyprlock "$@" >/dev/null 2>&1
rc=$?

# Only on a real unlock: relock.sh kills this hyprlock and its replacement needs the shot.
[ "$rc" -eq 0 ] && rm -f "$shot"
exit "$rc"
