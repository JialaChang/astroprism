#!/bin/bash

pacman_succ=0
pacman_fail=0
aur_succ=0
aur_fail=0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

source "$SCRIPT_DIR/host.sh"
source "$SCRIPT_DIR/msg.sh"

# Package lists are per-machine
PROFILE=$(host_profile)
if [ -z "$PROFILE" ]; then
  error "No host profile set, run: ./deploy.sh deploy <profile>"
  plain "available: $(host_profiles)" >&2
  exit 1
fi

PKG_DIR="$ROOT_DIR/Packages/$PROFILE"
PACMAN_TXT="$PKG_DIR/pkg-pacman.txt"
AUR_TXT="$PKG_DIR/pkg-aur.txt"
FAILED_TXT="$ROOT_DIR/Packages/pkg-failed.txt"

export_packages() {
  msg "Exporting package list for '$PROFILE'..."
  mkdir -p "$PKG_DIR"
  # exclude debug packages
  comm -23 <(pacman -Qeq | sort) <(pacman -Qmq | sort) | grep -v '\-debug$' >"$PACMAN_TXT"
  pacman -Qmq | grep -v '\-debug$' >"$AUR_TXT"
  echo "# Exported on $(date +%Y-%m-%d_%H:%M)" >>"$PACMAN_TXT"
  echo "# Exported on $(date +%Y-%m-%d_%H:%M)" >>"$AUR_TXT"
  msg2 "$(tilde "$PACMAN_TXT")"
  msg2 "$(tilde "$AUR_TXT")"
  msg "Export finished."
}

install_packages() {
  # pre-flight checks
  if [ ! -f "$PACMAN_TXT" ] || [ ! -f "$AUR_TXT" ]; then
    error "No package list for '$PROFILE', run './pkg.sh export' first"
    exit 1
  fi

  if ! command -v yay &>/dev/null; then
    error "yay not found, install it first"
    exit 1
  fi

  # cache sudo
  sudo -v

  # update system first
  msg "Updating system..."
  # avoid a partial upgrade
  if ! sudo pacman -Syu --noconfirm; then
    error "System update failed, aborting"
    exit 1
  fi

  # clear previous failed log
  >"$FAILED_TXT"

  echo ""
  msg "Installing pacman packages..."
  # read lists via fd 3 so pacman/yay/sudo can't eat lines from stdin
  while IFS= read -r pkg <&3; do
    [[ "$pkg" =~ ^#|^$ ]] && continue
    msg2 "$pkg"
    if sudo pacman -S --needed --noconfirm "$pkg"; then
      ((pacman_succ++))
    else
      ((pacman_fail++))
      echo "[pacman] $pkg" >>"$FAILED_TXT"
    fi
  done 3<"$PACMAN_TXT"

  echo ""
  msg "Installing AUR packages..."
  while IFS= read -r pkg <&3; do
    [[ "$pkg" =~ ^#|^$ ]] && continue
    msg2 "$pkg"
    if yay -S --needed --noconfirm "$pkg"; then
      ((aur_succ++))
    else
      ((aur_fail++))
      echo "[aur] $pkg" >>"$FAILED_TXT"
    fi
  done 3<"$AUR_TXT"

  echo ""
  msg "Install finished."
  plain "pacman : $pacman_succ success, $pacman_fail failed"
  plain "aur    : $aur_succ success, $aur_fail failed"
  plain "total  : $((pacman_succ + aur_succ)) success, $((pacman_fail + aur_fail)) failed"

  if [ -s "$FAILED_TXT" ]; then
    echo ""
    warning "Failed packages logged to $(tilde "$FAILED_TXT")"
    echo "# Logged on $(date +%Y-%m-%d_%H:%M)" >>"$FAILED_TXT"
    cat "$FAILED_TXT" >&2
  fi
}

case "$1" in
export)
  export_packages
  ;;
install)
  install_packages
  ;;
*)
  echo "Usage: ./pkg.sh {export|install}"
  echo "    export  - write the explicitly installed packages to Packages/<profile>/"
  echo "    install - update the system, then install this profile's package lists"
  ;;
esac
