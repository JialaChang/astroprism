#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

source "$SCRIPT_DIR/host.sh"

STAMP_DIR="$ROOT_DIR/.deploy-stamps"

# back up a file or dir to {target}.backup, overwriting any previous backup
# backup {target}
backup() {
  local target=$1

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
  local rel keep_args=()

  if [ -d "$src" ]; then
    echo "    -> Deploying dir $src to $dest..."

    for rel in "${keep[@]}"; do
      keep_args+=(--exclude "$rel")
    done

    mkdir -p "$dest" || return 1
    rsync -a --delete \
      --exclude 'target' \
      --exclude '.git' \
      --exclude 'node_modules' \
      "${keep_args[@]}" \
      "$src/" "$dest/" || return 1
  elif [ -f "$src" ]; then
    echo "    -> Deploying file $src to $dest..."
    mkdir -p "$(dirname "$dest")" || return 1
    cp "$src" "$dest" || return 1
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
    "$ROOT_DIR/config/waybar/scripts/mpris-popup/mpris-popup" \
    bash -c 'cargo build --release && cp target/release/mpris-popup mpris-popup' || failed=1

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
  backup "$HOME/.config/uwsm"

  backup "$HOME/.bashrc"
  backup "$HOME/.zshrc"
}

# Settle the profile before anything destructive runs.
# resolve_host_profile {profile from argv, may be empty}
resolve_host_profile() {
  local arg=$1

  if [ -n "$arg" ]; then
    if [ ! -d "$ROOT_DIR/config/hosts/$arg" ]; then
      echo "==> Unknown host profile '$arg'"
      echo "    available: $(host_profiles)"
      return 1
    fi
    printf '%s\n' "$arg" >"$HOST_FILE"
    echo "==> Host profile set to '$arg'"
    return 0
  fi

  local current
  current=$(host_profile)

  if [ -z "$current" ]; then
    echo "==> No host profile set for this machine."
    echo "    Pick one and pass it once, it is remembered afterwards:"
    echo "      ./deploy.sh deploy <profile>"
    echo "    available: $(host_profiles)"
    return 1
  fi

  if [ ! -d "$ROOT_DIR/config/hosts/$current" ]; then
    echo "==> Host profile '$current' ($HOST_FILE) no longer exists"
    echo "    available: $(host_profiles)"
    return 1
  fi

  echo "==> Host profile: $current"
}

# Runs after deploy_all, which rm -rf's the dests.
# Copies files in host to config, a failed copy fails the deploy.
deploy_host() {
  local profile
  profile=$(host_profile)
  local src="$ROOT_DIR/config/hosts/$profile"

  echo "    -> Deploying host profile '$profile'..."
  cp "$src/hypr-host.lua" "$HOME/.config/hypr/host.lua" &&
    cp "$src/waybar-host.jsonc" "$HOME/.config/waybar/host.jsonc" &&
    cp "$src/waybar-host.css" "$HOME/.config/waybar/host.css" &&
    cp "$src/mpris-popup-margin" "$HOME/.config/waybar/mpris-popup-margin" || {
    echo "    -> !! Failed to deploy host profile '$profile'"
    return 1
  }
}

# deploy files {src} {dest} [generated paths to keep, relative to dest...]
deploy_all() {
  local failed=0

  deploy "$ROOT_DIR/config/hypr" "$HOME/.config/hypr" "colors" || failed=1
  deploy "$ROOT_DIR/config/kitty" "$HOME/.config/kitty" "colors" || failed=1
  deploy "$ROOT_DIR/config/nvim" "$HOME/.config/nvim" || failed=1
  deploy "$ROOT_DIR/config/waybar" "$HOME/.config/waybar" "colors.css" "clock.jsonc" || failed=1
  deploy "$ROOT_DIR/config/matugen" "$HOME/.config/matugen" || failed=1
  deploy "$ROOT_DIR/config/rofi" "$HOME/.config/rofi" "colors" || failed=1
  deploy "$ROOT_DIR/config/uwsm" "$HOME/.config/uwsm" "gpu-mode" || failed=1

  deploy "$ROOT_DIR/local/bin/wallset" "$HOME/.local/bin/wallset" || failed=1
  deploy "$ROOT_DIR/local/bin/wallset-backend" "$HOME/.local/bin/wallset-backend" || failed=1
  deploy "$ROOT_DIR/local/bin/prime-run" "$HOME/.local/bin/prime-run" || failed=1
  deploy "$ROOT_DIR/local/bin/gpu-mode" "$HOME/.local/bin/gpu-mode" || failed=1

  deploy "$ROOT_DIR/bashrc" "$HOME/.bashrc" || failed=1
  deploy "$ROOT_DIR/zshrc" "$HOME/.zshrc" || failed=1

  return $failed
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
  resolve_host_profile "$2" || exit 1
  build_all || exit 1
  echo "==> Build all the programs !"
  deploy_all || exit 1
  deploy_host || exit 1
  hyprctl reload > /dev/null
  echo "==> Deploy all the files !"
  ;;
*)
  echo "Usage: ./deploy.sh [deploy [profile]|backup|build]"
  echo "    deploy  - deploy all the settings; pass a profile once to set this"
  echo "              machine's host profile (stored in ~/.config/astroprism-host)"
  echo "    backup - backup all your files will be replaced as .backup files"
  echo "    build  - compile the programs whose sources changed"
  ;;
esac
