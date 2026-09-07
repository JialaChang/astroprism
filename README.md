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
  <img src="./Docs/rofi.png" width="48%" alt="rofi launcher">
  <img src="./Docs/music.png" width="48%" alt="waybar mpris popup">
</p>

##  Features

- **Wallpaper-driven theming** — pick a wallpaper, the whole desktop recolors itself via [matugen](https://github.com/InioX/matugen) (Material You), light or dark
- **Custom MPRIS popup** — cover art and playback controls, one click from the bar
- **Workspace overview** — a fullscreen grid of live window thumbnails, `Super+Tab` to jump
- **Screenshot applet** — desktop/window/area/timed shots, screen recording, and scroll capture that stitches a scrolling page into one tall PNG
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
| Lockscreen | hyprlock |
| Wallpaper | awww + `wallset` script |
| Theming | [matugen](https://github.com/InioX/matugen) |
| Display manager | SDDM (`sddm-astronaut-theme`) |
| Input method | fcitx5 + chewing |

## Repo layout

```
config/      → ~/.config/…        (hypr, kitty, nvim, waybar, matugen, rofi, uwsm, wallpapers)
config/hosts/→ per-machine values (see Host profiles below)
local/bin/   → ~/.local/bin/…     (wallset, wallset-backend, gpu-mode, prime-run)
bashrc/zshrc → ~/.bashrc, ~/.zshrc
Packages/    → exported package lists, one dir per host profile
Scripts/     → deploy.sh, sync.sh, pkg.sh
Docs/        → screenshots + manual-setup.md (the /etc bits deploy.sh can't write)
```

Everything matugen writes is generated, not tracked — the templates in `config/matugen/templates/`
are the source of truth, and `deploy.sh` keeps the generated files in place when it redeploys a
config directory.

## Host profiles

The desktop and the laptop want different font sizes, gaps and waybar modules. Rather than keeping a
branch per machine, those values live in `config/hosts/<profile>/` and `deploy.sh` copies them into
place as `host.*` files that the main configs pull in.

Both machines share a hostname, so the profile is stored in `~/.config/astroprism-host` instead of
being detected. Pass it once to set or change it:

```sh
./Scripts/deploy.sh deploy laptop   # writes ~/.config/astroprism-host, then deploys
./Scripts/deploy.sh deploy          # later runs reuse it
```

The profile also picks the package lists (`Packages/<profile>/`), so each machine's `pkg.sh export`
records its own hardware instead of overwriting the other's.

The deployed `host.*` files are `sync.sh`-excluded and gitignored — edit
`config/hosts/<profile>/` instead. To add a machine, copy an existing profile directory and deploy
with its name.

## Installation

> [!WARNING]
> These are personal dotfiles, not a distro. `deploy.sh deploy` **overwrites** existing configs without backing them up — run `deploy.sh backup` first if you want a copy — and the package lists include desktop apps like Discord, Spotify and VS Code. Read the scripts and trim `Packages/<profile>/` before running anything.

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

## Scripts

| Script | What it does |
|---|---|
| `deploy.sh backup` | Backs up every target as `*.backup`; run manually before `deploy` if you want a safety copy |
| `deploy.sh build` | Compiles `mpris-popup` (cargo) and `scrollstitch` (go); each target is stamped with a hash of its sources + build command, so an unchanged tree skips the compiler |
| `deploy.sh deploy [profile]` | `build`, then repo → system: copies all configs into `~/.config`, `~/.local/bin`, dotfiles into `~`, applies the host profile, then `hyprctl reload` |
| `sync.sh` | System → repo: pulls current configs back in (needs `rsync`) and re-exports package lists (run before committing) |
| `pkg.sh export` | Writes explicitly-installed packages to `Packages/<profile>/` (debug pkgs excluded) |
| `pkg.sh install` | `pacman -Syu`, then installs this profile's lists; failures go to `Packages/pkg-failed.txt` |

> Both scripts *replace* whole directories rather than merging: `deploy.sh` does `rm -rf` + copy,
> `sync.sh` uses `rsync --delete` (which also skips build output like `target/`).  
> The exceptions are matugen's generated color files, which `deploy.sh` stashes and puts back, and
> the deployed `host.*` files, which `sync.sh` leaves alone.  
> Build stamps live in `.deploy-stamps/`; delete it to force a rebuild.

## Dynamic theming

The whole desktop is re-colored from the current wallpaper:

```
wallset (rofi picker with previews)
  └─ wallset-backend <image>
       ├─ awww img …                  # animated wallpaper switch
       ├─ matugen image …             # generate colors from the image
       │    └─ templates → hyprland, kitty, rofi, waybar, swaync, btop, starship, GTK 3/4, hyprexpose
       ├─ restart waybar, reload swaync
       └─ remembers wallpaper in ~/.cache/last_wallpaper
```

- Templates live in `config/matugen/templates/`, targets are wired up in `config/matugen/config.toml`.
- `waybar/scripts/theme-toggle.sh` re-runs matugen in light/dark mode on the last wallpaper; state is kept in `~/.cache/theme_mode`.

## Waybar MPRIS popup

Click the mpris module → a popup with cover art and playback controls. Written in Rust
(`config/waybar/scripts/mpris-popup/`) and built by `deploy.sh`; waybar's `on-click` points at
`target/release/mpris-popup`.

## Waybar workspaces

The workspaces module is `ext/workspaces` (the ext-workspace-v1 protocol), not `hyprland/workspaces`:
the latter clicks send Hyprland's legacy `dispatch workspace N` string, which the Lua config provider
rejects as a syntax error. Since that module has no `persistent-workspaces` option, workspaces 1–5 are
kept alive by persistent `workspace_rule`s in `hyprland.lua`.

## Screenshot applet

`Super+F` opens the rofi screenshot menu; `Ctrl+Shift+F` goes straight to an area shot. Shots are saved to `~/Pictures/Screenshot` and copied to the clipboard.

- Desktop / window / area / timed shots via grim + slurp.
- **Scroll capture** — start recording a region, scroll through the content, open the menu again to stop; the recording is piped through ffmpeg as raw RGBA straight into `scrollstitch`, a small Go tool in `config/rofi/applets/bin/scrollstitch/` that overlaps the frames into one tall PNG. Built by `deploy.sh` (falls back to building on first use, needs `go`).
- **Screen recording** — toggle a region recording, saved to `~/Videos/Screenrecord`.

`scrollstitch` reports per frame on stderr which ones it placed, skipped as duplicates, or couldn't
match — visible when the applet is run from a terminal (`screenshot --opt5` alias), dropped on a keybind
launch. Its matcher is covered by `stitch_test.go` (`go test ./...` in the tool's directory).

## Credits

- Parts of this setup adapted from [Noro18/linux-ricing-dotfiles](https://github.com/Noro18/linux-ricing-dotfiles)
- Rofi launchers & applets based on [adi1090x/rofi](https://github.com/adi1090x/rofi)
- [matugen](https://github.com/InioX/matugen) for Material You color generation
- [hyprexpose](https://github.com/ThiagoAVicente/hyprexpose) for the workspace overview
- [LazyVim](https://www.lazyvim.org/) as the Neovim base
- [sddm-astronaut-theme](https://github.com/Keyitdev/sddm-astronaut-theme)
