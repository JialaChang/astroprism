#!/usr/bin/env bash
# Caps lock warning for hyprlock. Prints nothing when caps lock is off.
# Reads the LED sysfs entries, which works without a Wayland connection.

set -uo pipefail

for led in /sys/class/leds/*::capslock/brightness; do
  [ -r "$led" ] || continue
  if [ "$(<"$led")" != 0 ]; then
    printf '󰪛  Caps Lock\n'
    exit 0
  fi
done
