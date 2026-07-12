#!/usr/bin/env bash

data=$(nvidia-smi --query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null) || {
  echo '{"text":"N/A","tooltip":"nvidia-smi unavailable"}'
  exit 0
}

IFS=',' read -r util temp mused mtotal <<<"$data"
util=${util// /} temp=${temp// /} mused=${mused// /} mtotal=${mtotal// /}

printf '{"text":"%s","tooltip":"GPU %s%%\\n%s°C · %s/%s MiB"}\n' \
  "$util" "$util" "$temp" "$mused" "$mtotal"
