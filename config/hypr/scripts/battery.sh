#!/usr/bin/env bash
# Battery indicator for hyprlock. Prints nothing on machines with no battery.

set -uo pipefail

bat=$(echo /sys/class/power_supply/BAT* 2>/dev/null | cut -d' ' -f1)
[ -r "$bat/capacity" ] || exit 0

capacity=$(<"$bat/capacity")

charging=false
for ac in /sys/class/power_supply/{AC,ADP,ACAD}*; do
  if [ -r "$ac/online" ] && [ "$(<"$ac/online")" = 1 ]; then
    charging=true
    break
  fi
done

if [ "$charging" = true ]; then
  icon='󰂄'
elif [ "$capacity" -ge 90 ]; then
  icon='󰁹'
elif [ "$capacity" -ge 70 ]; then
  icon='󰂁'
elif [ "$capacity" -ge 50 ]; then
  icon='󰁿'
elif [ "$capacity" -ge 30 ]; then
  icon='󰁽'
elif [ "$capacity" -ge 15 ]; then
  icon='󰁻'
else
  icon='󰁺'
fi

printf '%s  %s%%\n' "$icon" "$capacity"
