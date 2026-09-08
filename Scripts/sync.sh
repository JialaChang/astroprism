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
    mkdir -p "$dest"
    rsync -a --delete \
      --exclude 'target' \
      --exclude 'node_modules' \
      --exclude '.git' \
      --exclude '__pycache__' \
      --exclude '*.pyc' \
      "${extra[@]}" \
      "$src/" "$dest/"
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

# host.* are deploy_host output; the tracked copies live in config/hosts/<profile>/, 
# syncing them back would overwrite the template so exclude.
sync_dir ~/.config/hypr "$ROOT_DIR/config/hypr" 'host.lua'
sync_dir ~/.config/kitty "$ROOT_DIR/config/kitty"
sync_dir ~/.config/nvim "$ROOT_DIR/config/nvim"
sync_dir ~/.config/waybar "$ROOT_DIR/config/waybar" 'host.jsonc' 'host.css' 'mpris-popup-margin'
sync_dir ~/.config/matugen "$ROOT_DIR/config/matugen"
sync_dir ~/.config/rofi "$ROOT_DIR/config/rofi"
sync_dir ~/.config/uwsm "$ROOT_DIR/config/uwsm"
sync_dir ~/.config/fastfetch "$ROOT_DIR/config/fastfetch"

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
