# Architecture

Installation is in the [README](../README.md); the `/etc` pieces no deploy can write are in
[manual-setup.md](manual-setup.md).

## Scripts

| Script | What it does |
|---|---|
| `deploy.sh backup` | Copies every target to `*.backup`; run it before `deploy` if you want a safety copy |
| `deploy.sh build` | Compiles `mpris-popup` (cargo) and `scrollstitch` (go); each is stamped with a hash of its sources and build command, so an unchanged tree skips the compiler |
| `deploy.sh deploy [profile]` | `build`, then repo → system: configs into `~/.config` and `~/.local/bin`, dotfiles into `~`, the host profile on top, then reload Hyprland and restart hypridle and waybar |
| `sync.sh` | System → repo: pulls `~/.config` and the shell rc files back (needs `rsync`) and re-exports package lists. Run it before committing; `local/bin/` is left out on purpose — see below |
| `pkg.sh export` | Writes explicitly-installed packages to `Packages/<profile>/` (debug pkgs excluded) |
| `pkg.sh install` | `pacman -Syu`, then this profile's lists; failures go to `Packages/pkg-failed.txt` |

> Both directions use `rsync --delete`, so directories are replaced rather than merged (build output
> like `target/` is skipped). Generated colors and the deployed `host.*` files are excluded instead,
> their tracked copies living in [`config/hosts/`](../config/hosts/README.md).  
> `local/bin/` is deploy-only: edit those scripts in the repo, since `sync.sh` never pulls them back
> and the next `deploy` overwrites a live edit.  
> Build stamps live in `.deploy-stamps/`; delete it to force a rebuild.

## Dynamic theming

The whole desktop is re-colored from the current wallpaper:

```
wallset (rofi picker with previews, then a color-strategy row)
  └─ wallset-backend <image> [--prefer strategy]
       ├─ awww img …                  # animated wallpaper switch
       ├─ matugen image … --mode … --prefer …   # generate colors from the image
       │    └─ templates → hyprland, hyprlock, kitty, rofi, waybar, swaync, btop,
       │                   starship, fastfetch, GTK 3/4, hyprexpose
       ├─ restart waybar, reload swaync
       └─ remembers wallpaper in ~/.cache/last_wallpaper
```

- Templates live in `config/matugen/templates/`, wired to their targets in `config/matugen/config.toml`.
- `theme-toggle.sh` re-runs matugen in the other mode on the last wallpaper.
- `~/.cache/theme_mode` and `theme_prefer` hold the mode and the `--prefer` strategy; both scripts
  read and write them, so changing a wallpaper never resets the light/dark choice or the reverse.

## Waybar MPRIS popup

A GTK popup with cover art and playback controls, in Rust under `config/waybar/scripts/mpris-popup/`
(`art`, `mpris`, `ui`). `deploy.sh` builds it and copies the binary next to its sources, so the
deployed config carries no `target/` and waybar's `on-click` points at
`~/.config/waybar/scripts/mpris-popup/mpris-popup`.

State is signal-driven: `PropertiesChanged` from playerctld, plus `Seeked` for the position MPRIS
never notifies on. Signal bursts coalesce into one refresh and seek drags are debounced, because every
`playerctl` call is a fork that would otherwise block the GTK main loop. Cover art is fetched
off-thread with a timeout and a size cap.

## Waybar workspaces

The module is `ext/workspaces` (ext-workspace-v1), not `hyprland/workspaces`: the latter's clicks send
Hyprland's legacy `dispatch workspace N` string, which the Lua config provider rejects as a syntax
error. It has no `persistent-workspaces` option, so workspaces 1–5 are kept alive by persistent
`workspace_rule`s in `hyprland.lua`.

## Screenshot applet

A rofi menu; shots land in `~/Pictures/Screenshot` and on the clipboard.

- Desktop / window / area / timed shots via grim + slurp.
- **Scroll capture** — record a region, scroll, open the menu again to stop. ffmpeg pipes the
  recording in as raw RGBA to `scrollstitch`, a Go tool in `config/rofi/applets/bin/scrollstitch/`
  that overlaps the frames into one tall PNG.
- **Screen recording** — toggle a region recording, saved to `~/Videos/Screenrecord`.

`scrollstitch` reports on stderr which frames it placed and which it dropped — visible from a terminal
(`screenshot --opt5`), lost on a keybind launch. Its matcher is covered by `stitch_test.go`
(`go test ./...` in the tool's directory).

## GPU power

On the laptop the dGPU stays in runtime D3 unless something asks for it. The userspace half needs no
root:

- `config/uwsm/env-hyprland` — points the compositor at the iGPU and hides the NVIDIA EGL vendor and
  Vulkan ICD, so nothing loads the driver just by probing
- `local/bin/gpu-mode` — `igpu` (default) / `hybrid`, takes effect on re-login
- `local/bin/prime-run` — hands the NVIDIA libraries back to one app
- `config/waybar/scripts/gpu.sh` — reads `runtime_status` from sysfs before calling `nvidia-smi`,
  because querying a sleeping GPU wakes it

Idle draw: ~9.6 W, against ~15 W with the dGPU awake. The half that needs root — keeping SDDM's X
server off the card — is in [manual-setup.md](manual-setup.md).
