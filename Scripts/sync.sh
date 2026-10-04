#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

source "$SCRIPT_DIR/msg.sh"

# sync_dir {src} {dest} [extra rsync excludes...]
sync_dir() {
  local src=$1
  local dest=$2
  shift 2
  local extra=()
  local pattern
  for pattern in "$@"; do
    extra+=(--exclude "$pattern")
  done

  if [ -d "$src" ]; then
    mkdir -p "$dest" || return 1
    rsync -a --delete \
      --exclude 'target' \
      --exclude 'node_modules' \
      --exclude '.git' \
      --exclude '__pycache__' \
      --exclude '*.pyc' \
      "${extra[@]}" \
      "$src/" "$dest/" || {
      error "Failed to sync $(tilde "$src")"
      return 1
    }
    msg2 "$(tilde "$src")"
  else
    msg2 "$(tilde "$src"): not found, skipped"
  fi
}

sync_file() {
  local src=$1
  local dest=$2
  if [ -f "$src" ]; then
    cp "$src" "$dest" || {
      error "Failed to sync $(tilde "$src")"
      return 1
    }
    msg2 "$(tilde "$src")"
  else
    msg2 "$(tilde "$src"): not found, skipped"
  fi
}

failed=0

msg "Syncing configs..."

# host.* are deploy_host output; the tracked copies live in config/hosts/<profile>/, 
# syncing them back would overwrite the template so exclude.
sync_dir ~/.config/hypr "$ROOT_DIR/config/hypr" 'host.lua' 'hyprlock-host.conf' 'hypridle.conf' || failed=1
sync_dir ~/.config/kitty "$ROOT_DIR/config/kitty" || failed=1
sync_dir ~/.config/nvim "$ROOT_DIR/config/nvim" || failed=1
sync_dir ~/.config/waybar "$ROOT_DIR/config/waybar" 'host.jsonc' 'host.css' 'mpris-popup-margin' || failed=1
sync_dir ~/.config/matugen "$ROOT_DIR/config/matugen" || failed=1
sync_dir ~/.config/rofi "$ROOT_DIR/config/rofi" || failed=1
sync_dir ~/.config/uwsm "$ROOT_DIR/config/uwsm" || failed=1
sync_dir ~/.config/fastfetch "$ROOT_DIR/config/fastfetch" || failed=1

sync_file ~/.bashrc "$ROOT_DIR/bashrc" || failed=1
sync_file ~/.zshrc "$ROOT_DIR/zshrc" || failed=1

"$ROOT_DIR/Scripts/pkg.sh" export || failed=1

if [ "$failed" -ne 0 ]; then
  error "Sync finished with errors, see the ERROR lines above"
  exit 1
fi
msg "Sync finished."
