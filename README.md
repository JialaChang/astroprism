<h1 align="center">☄ astroprism ☄</h1>

<p align="center">
  Light in, spectrum out — Arch Linux + Hyprland dotfiles that refract a wallpaper into an entire desktop.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Arch_Linux-1793D1?logo=archlinux&logoColor=white" alt="Arch Linux">
  <img src="https://img.shields.io/badge/Hyprland-58E1FF?logo=hyprland&logoColor=black" alt="Hyprland">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="GPL-3.0">
</p>

<p align="center">
  <img src="./Docs/fastfetch.png" width="48%" alt="fastfetch in kitty with the MPRIS popup open">
  <img src="./Docs/wallpaper.png" width="48%" alt="wallset wallpaper picker">
</p>

<p align="center">
  <img src="./Docs/rofi.png" width="48%" alt="rofi screenshot applet">
  <img src="./Docs/script.png" width="48%" alt="deploy.sh deploy output">
</p>

##  Features

- **Wallpaper-driven theming** — pick a wallpaper, the whole desktop recolors itself via [matugen](https://github.com/InioX/matugen) (Material You), light or dark
- **Custom MPRIS popup** — cover art and playback controls, one click from the bar
- **Workspace overview** — a fullscreen grid of live window thumbnails
- **Screenshot applet** — desktop/window/area/timed shots, screen recording, and scroll capture that stitches a scrolling page into one tall PNG
- **Themed lock screen** — hyprlock recolored from the wallpaper, with hypridle handling the idle dim/lock/blank/suspend schedule
- **Battery charge limit** (laptop) — click the bar's battery to cap charging at 80%, reapplied on boot and after every resume
- **Reproducible installs** — exported package lists and deploy/sync scripts get the setup back in a few commands, on either machine

## What's inside

| Part | Choice |
|---|---|
| WM | [Hyprland](https://hypr.land/) — configured in **Lua** (`hyprland.lua`) |
| Bar | Waybar |
| Workspace overview | [hyprexpose](https://github.com/ThiagoAVicente/hyprexpose) |
| Launcher / menus | Rofi (launcher, applets, wallpaper picker) |
| Terminal | Kitty |
| Editor | Neovim (LazyVim & Neovide) |
| Shell | zsh + starship (bash config kept as fallback) |
| Notifications | swaync |
| Lockscreen | hyprlock (matugen-themed) + hypridle for idle locking |
| Wallpaper | awww + `wallset` script |
| Theming | [matugen](https://github.com/InioX/matugen) |
| Display manager | SDDM (`sddm-astronaut-theme`) |
| Input method | fcitx5 + chewing |

## Repo layout

```
config/      → ~/.config/…        (hypr, kitty, nvim, waybar, matugen, rofi, uwsm, fastfetch, wallpapers)
config/hosts/→ per-machine values (one dir per profile — see config/hosts/README.md)
local/bin/   → ~/.local/bin/…     (wallset, wallset-backend, gpu-mode, prime-run — deploy-only)
bashrc/zshrc → ~/.bashrc, ~/.zshrc
Packages/    → exported package lists, one dir per host profile
Scripts/     → deploy.sh, sync.sh, pkg.sh
system/udev/ → /etc/udev/rules.d/… (root, installed by hand — see Docs/manual-setup.md)
Docs/        → screenshots + architecture.md, manual-setup.md
```

Everything matugen writes is generated, not tracked — the templates in `config/matugen/templates/`
are the source of truth, and `deploy.sh` keeps the generated files in place when it redeploys a
config directory.

## Installation

> [!WARNING]
> These are personal dotfiles, not a distro. `deploy.sh deploy` **overwrites** existing configs
> without a backup (run `deploy.sh backup` first for a copy), and the package lists include apps like
> Discord, Spotify and VS Code. Read the scripts and trim `Packages/<profile>/` first.

```sh
# 0. install base system and yay first
sudo pacman -S --needed base-devel git
git clone https://aur.archlinux.org/yay.git /tmp/yay && (cd /tmp/yay && makepkg -si)

# 1. clone this dotfile
git clone git@github.com:JialaChang/astroprism.git
cd astroprism

# 2. deploy configs (builds the Rust/Go helpers first, needs cargo + go)
./Scripts/deploy.sh backup            # optional: saves existing configs as *.backup
./Scripts/deploy.sh deploy desktop    # profile: desktop | laptop

# 3. install everything from this profile's lists
./Scripts/pkg.sh install        # failures are logged to Packages/pkg-failed.txt

# 4. set a wallpaper — this also generates the whole color scheme
wallset
```

> [!IMPORTANT]
> Step 4 is not cosmetic: `hyprland.lua`, waybar's `clock.jsonc` and `hyprlock.conf` all pull in a
> matugen-generated file, and hyprlock will not start without its colors — the screen would not lock.
> Run `hyprlock` once by hand after the first deploy to check it locks and unlocks.

## Docs

- [`Docs/architecture.md`](Docs/architecture.md) — what the scripts do, the theming pipeline, and the
  waybar, screenshot and GPU pieces
- [`Docs/manual-setup.md`](Docs/manual-setup.md) — the `/etc` bits a deploy cannot write: battery
  charge limit, keeping Xorg off the dGPU
- [`config/hosts/README.md`](config/hosts/README.md) — what each per-machine profile sets

## Credits

- Parts of this setup adapted from [Noro18/linux-ricing-dotfiles](https://github.com/Noro18/linux-ricing-dotfiles)
- Rofi launchers & applets based on [adi1090x/rofi](https://github.com/adi1090x/rofi)
- [matugen](https://github.com/InioX/matugen) for Material You color generation
- [hyprexpose](https://github.com/ThiagoAVicente/hyprexpose) for the workspace overview
- [LazyVim](https://www.lazyvim.org/) as the Neovim base
- [sddm-astronaut-theme](https://github.com/Keyitdev/sddm-astronaut-theme)
