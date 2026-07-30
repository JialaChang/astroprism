#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

STAMP_DIR="$ROOT_DIR/.deploy-stamps"

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

# hash every source file under a dir, ignoring build output
# source_hash {srcdir} {output binary}
source_hash() {
  local srcdir=$1 out=$2

  # Without pipefail a broken find or grep is masked by cut's exit status,
  # and every source dir hashes to the same constant. local - scopes it.
  local -
  set -o pipefail

  # Anything that must not affect the hash goes in the -prune list below.
  find "$srcdir" -type d \( -name target -o -name .git -o -name node_modules \) -prune -o -type f -print0 |
    grep -zvxF "$out" |
    sort -z |
    xargs -0 sha256sum |
    sha256sum |
    cut -d' ' -f1
}

# build {label} {srcdir} {output binary} {build command...}
build() {
  local label=$1 srcdir=$2 out=$3
  shift 3
  local stamp="$STAMP_DIR/$label" hash

  if [ ! -d "$srcdir" ]; then
    echo "    -> $srcdir not found, skipping build..."
    return 0
  fi

  if ! hash=$(source_hash "$srcdir" "$out"); then
    echo "    -> !! Cannot hash sources for $label"
    return 1
  fi

  # Fold in the build command, so changing a flag invalidates the stamp too.
  hash=$(printf '%s\0' "$hash" "$@" | sha256sum | cut -d' ' -f1)

  if [ -x "$out" ] && [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$hash" ]; then
    echo "    -> $label unchanged, skipping build..."
    return 0
  fi

  echo "    -> Building $label..."
  if (cd "$srcdir" && "$@"); then
    mkdir -p "$STAMP_DIR"
    printf '%s\n' "$hash" > "$stamp"
  else
    echo "    -> !! Build failed for $label"
    return 1
  fi
}

# build all the programs that need compiling
build_all() {
  local failed=0

  build scrollstitch \
    "$ROOT_DIR/config/rofi/applets/bin/scrollstitch" \
    "$ROOT_DIR/config/rofi/applets/bin/scrollstitch/scrollstitch" \
    go build -o scrollstitch . || failed=1

  build mpris-popup \
    "$ROOT_DIR/config/waybar/scripts/mpris-popup" \
    "$ROOT_DIR/config/waybar/scripts/mpris-popup/target/release/mpris-popup" \
    cargo build --release || failed=1

  return $failed
}

# backup files
backup_all() {
  backup "$HOME/.config/hypr"
  backup "$HOME/.config/kitty"
  backup "$HOME/.config/nvim"
  backup "$HOME/.config/waybar"
  backup "$HOME/.config/matugen"
  backup "$HOME/.config/rofi"

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
build)
  build_all || exit 1
  echo "==> Build all the programs !"
  ;;
deploy)
  build_all || exit 1
  echo "==> Build all the programs !"
  backup_all
  echo "==> Backup all the files !"
  deploy_all
  hyprctl reload > /dev/null
  echo "==> Deploy all the files !"
  ;;
*)
  echo "Usage: ./deploy.sh [deploy|backup|build]"
  echo "    deploy  - deploy all the settings"
  echo "    backup - backup all your files will be replaced as .backup files"
  echo "    build  - compile the programs whose sources changed"
  ;;
esac
