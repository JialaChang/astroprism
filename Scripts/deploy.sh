#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

# funtions
backup() {
  local target=$1
  # local timestamp=$(date +%m-%d_%H.%M)

  if [ -d "$target" ]; then
    echo "    -> Backing up dir $target..."
    rm -rf "${target}.backup"
    cp -r "$target" "${target}.backup"
  elif [ -f "$target" ]; then
    echo "    -> Backing up file $target..."
    cp "$target" "${target}.backup"
  else
    echo "    -> $target not found, skipping backup..."
  fi
}

deploy() {
  local src=$1
  local dest=$2

  if [ -d "$src" ]; then
    echo "    -> Deploying dir $src to $dest..."
    rm -rf "$dest"
    cp -r "$src" "$dest"
  elif [ -f "$src" ]; then
    echo "    -> Deploying file $src to $dest..."
    mkdir -p "$(dirname "$dest")"
    cp "$src" "$dest"
  else
    echo "    -> $src not found, skipping..."
  fi
}

# backup files
backup_all() {
  backup "$HOME/.config/hypr"
  backup "$HOME/.config/kitty"
  backup "$HOME/.config/nvim"
  backup "$HOME/.config/waybar"
  backup "$HOME/.config/matugen"
  backup "$HOME/.config/rofi"

  backup "$HOME/.config/starship.toml"

  backup "$HOME/.local/bin/wallset"
  backup "$HOME/.local/bin/wallset-backend"

  backup "$HOME/.bashrc"
  backup "$HOME/.zshrc"
}

# deploy files {src} {dest}
deploy_all() {
  deploy "$ROOT_DIR/config/hypr" "$HOME/.config/hypr"
  deploy "$ROOT_DIR/config/kitty" "$HOME/.config/kitty"
  deploy "$ROOT_DIR/config/nvim" "$HOME/.config/nvim"
  deploy "$ROOT_DIR/config/waybar" "$HOME/.config/waybar"
  deploy "$ROOT_DIR/config/matugen" "$HOME/.config/matugen"
  deploy "$ROOT_DIR/config/rofi" "$HOME/.config/rofi"
  deploy "$ROOT_DIR/config/wallpapers" "$HOME/.config/wallpapers"

  deploy "$ROOT_DIR/config/starship.toml" "$HOME/.config/starship.toml"

  deploy "$ROOT_DIR/local/bin/wallset" "$HOME/.local/bin/wallset"
  deploy "$ROOT_DIR/local/bin/wallset-backend" "$HOME/.local/bin/wallset-backend"

  deploy "$ROOT_DIR/bashrc" "$HOME/.bashrc"
  deploy "$ROOT_DIR/zshrc" "$HOME/.zshrc"
}

case "$1" in
backup)
  backup_all
  echo "==> Backup all the files !"
  ;;
deploy)
  deploy_all
  hyprctl reload
  echo "==> Deploy all the files !"
  ;;
*)
  echo "Usage: ./deploy.sh [deploy|backup]"
  echo "    deploy  - deploy all the settings"
  echo "    backup - backup all your files will be replaced as .backup files"
  ;;
esac
