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

# deploy {src} {dest} [generated paths to keep, relative to dest...]
deploy() {
  local src=$1
  local dest=$2
  shift 2
  local keep=("$@")
  local stash rel

  if [ -d "$src" ]; then
    echo "    -> Deploying dir $src to $dest..."

    # stash matugen-generated files so rm -rf does not eat them
    stash=$(mktemp -d)
    for rel in "${keep[@]}"; do
      if [ -e "$dest/$rel" ]; then
        echo "    -> Keeping generated $rel..."
        mkdir -p "$stash/$(dirname "$rel")"
        cp -r "$dest/$rel" "$stash/$rel"
      fi
    done

    rm -rf "$dest"
    cp -r "$src" "$dest"

    for rel in "${keep[@]}"; do
      if [ -e "$stash/$rel" ]; then
        rm -rf "${dest:?}/$rel"
        mkdir -p "$dest/$(dirname "$rel")"
        cp -r "$stash/$rel" "$dest/$rel"
      fi
    done
    rm -rf "$stash"
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

  backup "$HOME/.local/bin/wallset"
  backup "$HOME/.local/bin/wallset-backend"

  backup "$HOME/.bashrc"
  backup "$HOME/.zshrc"
}

# deploy files {src} {dest}
deploy_all() {
  deploy "$ROOT_DIR/config/hypr" "$HOME/.config/hypr" "colors"
  deploy "$ROOT_DIR/config/kitty" "$HOME/.config/kitty" "colors"
  deploy "$ROOT_DIR/config/nvim" "$HOME/.config/nvim"
  deploy "$ROOT_DIR/config/waybar" "$HOME/.config/waybar" "colors.css"
  deploy "$ROOT_DIR/config/matugen" "$HOME/.config/matugen"
  deploy "$ROOT_DIR/config/rofi" "$HOME/.config/rofi" "colors"

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
  backup_all
  echo "==> Backup all the files !"
  deploy_all
  hyprctl reload > /dev/null
  echo "==> Deploy all the files !"
  ;;
*)
  echo "Usage: ./deploy.sh [deploy|backup]"
  echo "    deploy  - deploy all the settings"
  echo "    backup - backup all your files will be replaced as .backup files"
  ;;
esac
