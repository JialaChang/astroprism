#!/usr/bin/env bash
# hypridle's before_sleep_cmd: lock, then blank the screen so resume stays dark until relock.sh.

set -uo pipefail

loginctl lock-session

# DPMS right after hyprlock starts can hang Hyprland.
sleep 1

hyprctl dispatch 'hl.dsp.dpms({ action = "off" })'
