#!/usr/bin/env bash
# Author: JialaChang & Claude
# Toggle matugen's light/dark scheme for the wallpaper currently in use.

MODE_FILE="$HOME/.cache/theme_mode"
PREFER_FILE="$HOME/.cache/theme_prefer"
WALLPAPER_FILE="$HOME/.cache/last_wallpaper"

shopt -s nullglob

get_mode() {
    if [ -f "$MODE_FILE" ]; then
        cat "$MODE_FILE"
    else
        echo "dark"
    fi
}

get_prefer() {
    if [ -f "$PREFER_FILE" ]; then
        cat "$PREFER_FILE"
    else
        echo "saturation"
    fi
}

status() {
    local mode
    mode=$(get_mode)
    if [ "$mode" = "light" ]; then
        echo '{"text":"󰖙","tooltip":"Light mode","class":"light"}'
    else
        echo '{"text":"󰖔","tooltip":"Dark mode","class":"dark"}'
    fi
}

toggle() {
    local mode new_mode wallpaper
    mode=$(get_mode)
    [ "$mode" = "dark" ] && new_mode="light" || new_mode="dark"

    if [ ! -f "$WALLPAPER_FILE" ]; then
        notify-send "Theme Toggle" "No wallpaper recorded yet"
        exit 1
    fi
    wallpaper=$(cat "$WALLPAPER_FILE")

    if ! matugen image "$wallpaper" --mode "$new_mode" --prefer "$(get_prefer)"; then
        notify-send "Theme Toggle" "matugen failed, theme not changed"
        exit 1
    fi
    echo "$new_mode" >"$MODE_FILE"

    # Global light/dark preference: GTK4 apps, the desktop portal
    gsettings set org.gnome.desktop.interface color-scheme "prefer-$new_mode"

    pkill waybar
    setsid -f waybar >/dev/null 2>&1
    swaync-client -rs >/dev/null 2>&1 &

    notify-send "Theme Toggle" "Switched to $new_mode mode"
}

case "${1:-status}" in
status) status ;;
toggle) toggle ;;
*)
    echo "Usage: $0 {status|toggle}" >&2
    exit 1
    ;;
esac
