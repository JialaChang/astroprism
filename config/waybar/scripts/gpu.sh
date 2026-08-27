#!/usr/bin/env bash

# Reading runtime_status from sysfs does not wake the GPU, but nvidia-smi does.
# Poll the cheap file first and report a sleeping dGPU without ever pulling it
# out of runtime D3 -- at a 10s interval this module alone otherwise costs the
# ~4.5W the dGPU draws whenever it is awake.

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
