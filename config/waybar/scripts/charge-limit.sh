#!/usr/bin/env bash
set -uo pipefail

# Toggle the battery's max charge between 80% and 100%.
# Needs the sysfs file writable by wheel: system/udev/90-charge-limit.rules

SYSFS=$(ls /sys/class/power_supply/*/charge_control_end_threshold 2>/dev/null | head -n1)
STATE_FILE="$HOME/.cache/charge_limit"

[ -w "$SYSFS" ] || {
    [ "${1:-}" = "restore" ] || notify-send "Charge Limit" "$SYSFS is not writable"
    exit 1
}

apply() {
    echo "$1" >"$SYSFS" && echo "$1" >"$STATE_FILE"
}

toggle() {
    local new
    [ "$(cat "$SYSFS")" -lt 100 ] && new=100 || new=80
    if apply "$new"; then
        notify-send "Charge Limit" "Max charge set to $new%"
    else
        notify-send "Charge Limit" "Failed to set max charge"
    fi
}

# Firmware resets the limit on boot and resume, so reapply the saved one.
restore() {
    apply "$(cat "$STATE_FILE" 2>/dev/null || echo 80)"
}

case "${1:-toggle}" in
toggle) toggle ;;
restore) restore ;;
*)
    echo "Usage: $0 {toggle|restore}" >&2
    exit 1
    ;;
esac
