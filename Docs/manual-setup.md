# Manual setup

`Scripts/deploy.sh` only writes under `$HOME`. Everything below lives in `/etc`,
needs root, and is **not** restored by a deploy — redo it by hand on a fresh
install or a new machine.

Both are laptop-specific (ASUS Vivobook K6500ZC: Intel Iris Xe + RTX 3050).
Skip them on a desktop or a single-GPU machine.

## 1. Battery charge threshold

Charging to 100% and sitting on AC ages the cell. Capping at 80% is the single
biggest thing you can do for its lifespan.

`/etc/systemd/system/battery-charge-threshold.service`:

```ini
[Unit]
Description=Set battery charge threshold to 80%
After=multi-user.target
StartLimitBurst=0

[Service]
Type=oneshot
Restart=on-failure
RestartSec=1
ExecStart=/bin/bash -c 'echo 80 > /sys/class/power_supply/BAT0/charge_control_end_threshold'

[Install]
WantedBy=multi-user.target suspend.target hibernate.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now battery-charge-threshold.service
cat /sys/class/power_supply/BAT0/charge_control_end_threshold   # -> 80
```

`suspend.target hibernate.target` in `WantedBy` is not optional: ASUS firmware
resets the cap to 100 on resume, so the unit has to run again each time.

Some models only accept 60/80/100. Write the value by hand first and read it
back before wiring up the unit.

## 2. Keep Xorg off the dGPU

SDDM's greeter runs an X server that autoconfigures onto the NVIDIA card and
then keeps holding it for the whole session, which alone stops the dGPU from
ever reaching runtime D3.

`/etc/X11/xorg.conf.d/20-intel-primary.conf`:

```
Section "ServerFlags"
    Option "AutoAddGPU" "off"
EndSection

Section "Device"
    Identifier "intel"
    Driver     "modesetting"
    BusID      "PCI:0:2:0"
EndSection
```

`AutoAddGPU off` is the load-bearing half — without it X still attaches the
NVIDIA card as a secondary GPU screen even though the primary device is pinned
to the iGPU. `BusID` is **decimal**, so check `lspci` and convert if the iGPU is
not at `00:02.0`.

Verify after a reboot — the Processes block should be empty:

```sh
nvidia-smi
```

If the greeter fails to come up, drop to a TTY (Ctrl+Alt+F2) and delete the
file.

## The rest is in the repo

The userspace half of the GPU work is deployed normally and needs no root:

- `config/uwsm/env-hyprland` — points the compositor at the iGPU and hides the
  NVIDIA EGL vendor and Vulkan ICD, so nothing loads the driver just by probing
- `local/bin/gpu-mode` — `igpu` (default) / `hybrid`, takes effect on re-login
- `local/bin/prime-run` — hands the NVIDIA libraries back to one app
- `config/waybar/scripts/gpu.sh` — reads `runtime_status` from sysfs before
  calling `nvidia-smi`, because querying a sleeping GPU wakes it

Idle draw with all of it in place: ~9.6 W, against ~15 W with the dGPU awake.
