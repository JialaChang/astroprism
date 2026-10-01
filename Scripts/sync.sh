#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

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
      echo " -> !! Failed to sync $src"
      return 1
    }
    echo " -> synced $src"
  else
    echo " -> $src not found, skipping..."
  fi
}

sync_file() {
  local src=$1
  local dest=$2
  if [ -f "$src" ]; then
    cp "$src" "$dest" || {
      echo " -> !! Failed to sync $src"
      return 1
    }
    echo " -> synced $src"
  else
    echo " -> $src not found, skipping..."
  fi
}

failed=0

echo "==> Syncing configs..."

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

echo "==> Exporting packages..."
"$ROOT_DIR/Scripts/pkg.sh" export || failed=1

if [ "$failed" -ne 0 ]; then
  echo "==> Done with errors, check the !! lines above"
  exit 1
fi
echo "==> Done!"
