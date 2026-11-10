#!/usr/bin/env bash
set -uo pipefail

# Check sysfs first: nvidia-smi wakes a sleeping dGPU (~4.5W).

card=$(grep -l 0x10de /sys/class/drm/card[0-9]*/device/vendor 2>/dev/null | head -1)
if [ -n "$card" ] && [ "$(cat "$(dirname "$card")/power/runtime_status" 2>/dev/null)" = suspended ]; then
  echo '{"text":"󰤄 󰾲","tooltip":"dGPU suspended (D3)"}'
  exit 0
fi

data=$(nvidia-smi --query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null) || {
  echo '{"text":"N/A","tooltip":"nvidia-smi unavailable"}'
  exit 0
}

IFS=',' read -r util temp mused mtotal <<<"$data"
util=${util// /} temp=${temp// /} mused=${mused// /} mtotal=${mtotal// /}

printf '{"text":"%s%% 󰾲","tooltip":"GPU %s%%\\n%s°C · %s/%s MiB"}\n' \
  "$util" "$util" "$temp" "$mused" "$mtotal"
