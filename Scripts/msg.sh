#!/bin/bash

# color only on a terminal, checked per stream; NO_COLOR turns it off
if [ -z "${NO_COLOR:-}" ] && [ -t 1 ]; then
  M_GREEN=$'\e[1;32m' M_BLUE=$'\e[1;34m' M_BOLD=$'\e[1m' M_OFF=$'\e[0m'
fi
if [ -z "${NO_COLOR:-}" ] && [ -t 2 ]; then
  E_YELLOW=$'\e[1;33m' E_RED=$'\e[1;31m' E_BOLD=$'\e[1m' E_OFF=$'\e[0m'
fi

# makepkg-style output: msg for a phase, msg2 for its steps
msg() { printf '%s==>%s %s%s\n' "$M_GREEN" "$M_OFF$M_BOLD" "$*" "$M_OFF"; }
msg2() { printf '%s  ->%s %s%s\n' "$M_BLUE" "$M_OFF$M_BOLD" "$*" "$M_OFF"; }
plain() { printf '    %s\n' "$*"; }
warning() { printf '%s==> WARNING:%s %s%s\n' "$E_YELLOW" "$E_OFF$E_BOLD" "$*" "$E_OFF" >&2; }
error() { printf '%s==> ERROR:%s %s%s\n' "$E_RED" "$E_OFF$E_BOLD" "$*" "$E_OFF" >&2; }

# shorten $HOME and the repo root for display
tilde() {
  case $1 in
  "$ROOT_DIR"/*) printf '%s' "${1#"$ROOT_DIR"/}" ;;
  "$HOME"/*) printf '~%s' "${1#"$HOME"}" ;;
  *) printf '%s' "$1" ;;
  esac
}
