#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

source "$SCRIPT_DIR/host.sh"
source "$SCRIPT_DIR/msg.sh"

STAMP_DIR="$ROOT_DIR/.deploy-stamps"

# back up a file or dir to {target}.backup, overwriting any previous backup
# backup {target}
backup() {
  local target=$1

  if [ -d "$target" ]; then
    msg2 "$(tilde "$target")"
    rm -rf "${target}.backup"
    cp -r "$target" "${target}.backup"
  elif [ -f "$target" ]; then
    msg2 "$(tilde "$target")"
    cp "$target" "${target}.backup"
  else
    msg2 "$(tilde "$target"): not found, skipped"
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
    msg2 "$(tilde "$dest")"

    for rel in "${keep[@]}"; do
      keep_args+=(--exclude "$rel")
    done

    mkdir -p "$dest" &&
      rsync -a --delete \
        --exclude 'target' \
        --exclude '.git' \
        --exclude 'node_modules' \
        "${keep_args[@]}" \
        "$src/" "$dest/" || {
      error "Failed to deploy $(tilde "$dest")"
      return 1
    }
  elif [ -f "$src" ]; then
    msg2 "$(tilde "$dest")"
    mkdir -p "$(dirname "$dest")" && cp "$src" "$dest" || {
      error "Failed to deploy $(tilde "$dest")"
      return 1
    }
  else
    warning "$(tilde "$src") not found, skipped"
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
    warning "$(tilde "$srcdir") not found, skipped"
    return 0
  fi

  if ! hash=$(source_hash "$srcdir" "$out"); then
    error "Cannot hash sources for $label"
    return 1
  fi

  # Fold in the build command, so changing a flag invalidates the stamp too.
  hash=$(printf '%s\0' "$hash" "$@" | sha256sum | cut -d' ' -f1)

  if [ -x "$out" ] && [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$hash" ]; then
    msg2 "$label: unchanged, skipped"
    return 0
  fi

  msg2 "$label: building..."
  if (cd "$srcdir" && "$@"); then
    mkdir -p "$STAMP_DIR"
    printf '%s\n' "$hash" > "$stamp"
  else
    error "Build failed for $label"
    return 1
  fi
}

# build all the programs that need compiling
build_all() {
  local failed=0

  msg "Building programs..."

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
  msg "Backing up configs..."
  backup "$HOME/.config/hypr"
  backup "$HOME/.config/kitty"
  backup "$HOME/.config/nvim"
  backup "$HOME/.config/waybar"
  backup "$HOME/.config/matugen"
  backup "$HOME/.config/rofi"
  backup "$HOME/.config/uwsm"
  backup "$HOME/.config/fastfetch"

  backup "$HOME/.bashrc"
  backup "$HOME/.zshrc"
}

# Settle the profile before anything destructive runs.
# resolve_host_profile {profile from argv, may be empty}
resolve_host_profile() {
  local arg=$1

  if [ -n "$arg" ]; then
    if [ ! -d "$ROOT_DIR/config/hosts/$arg" ]; then
      error "Unknown host profile '$arg'"
      plain "available: $(host_profiles)" >&2
      return 1
    fi
    printf '%s\n' "$arg" >"$HOST_FILE"
    msg "Host profile set to '$arg'"
    return 0
  fi

  local current
  current=$(host_profile)

  if [ -z "$current" ]; then
    error "No host profile set for this machine"
    plain "Pick one and pass it once, it is remembered afterwards:" >&2
    plain "  ./deploy.sh deploy <profile>" >&2
    plain "available: $(host_profiles)" >&2
    return 1
  fi

  if [ ! -d "$ROOT_DIR/config/hosts/$current" ]; then
    error "Host profile '$current' ($(tilde "$HOST_FILE")) no longer exists"
    plain "available: $(host_profiles)" >&2
    return 1
  fi

  msg "Host profile: $current"
}

# Runs after deploy_all.
# Copies files in host to config, a failed copy fails the deploy.
deploy_host() {
  local profile
  profile=$(host_profile)
  local src="$ROOT_DIR/config/hosts/$profile"

  msg2 "host profile '$profile'"
  cp "$src/hypr-host.lua" "$HOME/.config/hypr/host.lua" &&
    cp "$src/waybar-host.jsonc" "$HOME/.config/waybar/host.jsonc" &&
    cp "$src/waybar-host.css" "$HOME/.config/waybar/host.css" &&
    cp "$src/hyprlock-host.conf" "$HOME/.config/hypr/hyprlock-host.conf" &&
    cp "$src/hypridle-host.conf" "$HOME/.config/hypr/hypridle.conf" &&
    cp "$src/mpris-popup-margin" "$HOME/.config/waybar/mpris-popup-margin" || {
    error "Failed to deploy host profile '$profile'"
    return 1
  }
}

# deploy files {src} {dest} [generated paths to keep, relative to dest...]
deploy_all() {
  local failed=0

  msg "Deploying files..."

  # keep host files so a failed deploy can't leave them deleted
  deploy "$ROOT_DIR/config/hypr" "$HOME/.config/hypr" "colors" \
    "host.lua" "hyprlock-host.conf" "hypridle.conf" || failed=1
  deploy "$ROOT_DIR/config/kitty" "$HOME/.config/kitty" "colors" || failed=1
  deploy "$ROOT_DIR/config/nvim" "$HOME/.config/nvim" || failed=1
  deploy "$ROOT_DIR/config/waybar" "$HOME/.config/waybar" "colors.css" "clock.jsonc" \
    "host.jsonc" "host.css" "mpris-popup-margin" || failed=1
  deploy "$ROOT_DIR/config/matugen" "$HOME/.config/matugen" || failed=1
  deploy "$ROOT_DIR/config/rofi" "$HOME/.config/rofi" "colors" || failed=1
  deploy "$ROOT_DIR/config/uwsm" "$HOME/.config/uwsm" "gpu-mode" || failed=1
  deploy "$ROOT_DIR/config/fastfetch" "$HOME/.config/fastfetch" "big.jsonc" "small.jsonc" || failed=1

  deploy "$ROOT_DIR/local/bin/wallset" "$HOME/.local/bin/wallset" || failed=1
  deploy "$ROOT_DIR/local/bin/wallset-backend" "$HOME/.local/bin/wallset-backend" || failed=1
  deploy "$ROOT_DIR/local/bin/prime-run" "$HOME/.local/bin/prime-run" || failed=1
  deploy "$ROOT_DIR/local/bin/gpu-mode" "$HOME/.local/bin/gpu-mode" || failed=1

  deploy "$ROOT_DIR/bashrc" "$HOME/.bashrc" || failed=1
  deploy "$ROOT_DIR/zshrc" "$HOME/.zshrc" || failed=1

  return $failed
}

# Restart daemons so they pick up the deployed configs
restart_daemons() {
  msg "Reloading services..."

  msg2 "Reloading hyprland"
  hyprctl reload >/dev/null

  local d
  for d in hypridle waybar; do
    if pkill -x "$d"; then
      msg2 "Restarting $d"
      setsid -f "$d" >/dev/null 2>&1
    fi
  done
}

case "$1" in
backup)
  backup_all
  msg "Backup finished."
  ;;
build)
  build_all || exit 1
  msg "Build finished."
  ;;
deploy)
  resolve_host_profile "$2" || exit 1
  build_all || exit 1
  deploy_all || exit 1
  deploy_host || exit 1
  restart_daemons
  msg "Deploy finished."
  ;;
*)
  echo "Usage: ./deploy.sh {deploy [profile]|backup|build}"
  echo "    deploy  - build, copy the configs into place and reload;"
  echo "              [profile] sets this machine's profile and is saved"
  echo "    backup  - copy the configs deploy would replace to *.backup"
  echo "    build   - compile the programs whose sources changed"
  ;;
esac
