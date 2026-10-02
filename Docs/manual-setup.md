# Manual setup

`Scripts/deploy.sh` only writes under `$HOME`. These two live in `/etc`, need
root, and no deploy restores them — redo them on a fresh install. Both apply to
the laptop only: one needs a battery, the other a hybrid Intel + NVIDIA setup.

## 1. Battery charge limit

Charging to 100% and sitting on AC ages the cell. Capping at 80% is the single
biggest thing you can do for its lifespan.

```sh
sudo cp system/udev/90-charge-limit.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --action=add -s power_supply
cat /sys/class/power_supply/BAT0/charge_control_end_threshold   # -> 80
```

The rule caps the charge at 80 and gives that sysfs file to the `wheel` group,
so clicking waybar's battery can flip 80/100 without root. The firmware resets
the cap on resume, so `charge-limit.sh restore` writes it back from the laptop
profile's `autostart` and hypridle's `after_sleep_cmd`.

Some models only accept 60/80/100, and the rule matches `BAT0`. Write the value
by hand first and read it back.

> [!WARNING]
> Remove the old `battery-charge-threshold.service` if you have it — it forces 80
> on every resume and fights the toggle:
>
> ```sh
> sudo systemctl disable --now battery-charge-threshold.service
> sudo rm /etc/systemd/system/battery-charge-threshold.service
> ```

## 2. Keep Xorg off the dGPU

SDDM's greeter runs an X server that autoconfigures onto the NVIDIA card and
holds it for the whole session, which alone stops the dGPU from ever reaching
runtime D3.

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

`AutoAddGPU off` is the load-bearing half — without it X attaches the NVIDIA card
as a secondary GPU screen even with the primary device pinned to the iGPU.
`BusID` is **decimal**, so check `lspci` and convert if the iGPU is not at
`00:02.0`.

Reboot and check that `nvidia-smi` shows an empty Processes block. If the greeter
does not come up, drop to a TTY (Ctrl+Alt+F2) and delete the file.
