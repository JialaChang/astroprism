#!/bin/bash

HOST_FILE="$HOME/.config/astroprism-host"

# Which config/hosts/ profile this machine uses
host_profile() {
  [ -f "$HOST_FILE" ] && cat "$HOST_FILE"
}

host_profiles() {
  ls "$ROOT_DIR/config/hosts" | tr '\n' ' '
}
