#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

sync_dir() {
  local src=$1
  local dest=$2
  if [ -d "$src" ]; then
    rm -rf "$dest"
    cp -r "$src" "$dest"
    echo " -> synced $src"
  else
    echo " -> $src not found, skipping..."
  fi
}

sync_file() {
  local src=$1
  local dest=$2
  if [ -f "$src" ]; then
    cp "$src" "$dest"
    echo " -> synced $src"
  else
    echo " -> $src not found, skipping..."
  fi
}

echo "==> Syncing configs..."

sync_dir ~/.config/hypr "$ROOT_DIR/config/hypr"
sync_dir ~/.config/kitty "$ROOT_DIR/config/kitty"
sync_dir ~/.config/nvim "$ROOT_DIR/config/nvim"
sync_dir ~/.config/waybar "$ROOT_DIR/config/waybar"
sync_dir ~/.config/matugen "$ROOT_DIR/config/matugen"
sync_dir ~/.config/rofi "$ROOT_DIR/config/rofi"
sync_dir ~/.config/uwsm "$ROOT_DIR/config/uwsm"

sync_file ~/.config/starship.toml "$ROOT_DIR/config/starship.toml"

sync_file ~/.local/bin/wallset "$ROOT_DIR/local/bin/wallset"
sync_file ~/.local/bin/wallset-backend "$ROOT_DIR/local/bin/wallset-backend"
sync_file ~/.local/bin/prime-run "$ROOT_DIR/local/bin/prime-run"
sync_file ~/.local/bin/gpu-mode "$ROOT_DIR/local/bin/gpu-mode"

sync_file ~/.bashrc "$ROOT_DIR/bashrc"
sync_file ~/.zshrc "$ROOT_DIR/zshrc"

echo "==> Exporting packages..."
"$ROOT_DIR/Scripts/pkg.sh" export

echo "==> Done!"
