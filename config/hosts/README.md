# Host profiles

Both machines share a hostname, so the profile is not detected — it is stored in
`~/.config/astroprism-host` by `./Scripts/deploy.sh deploy <profile>` and reused by later runs.

`deploy_host` in `deploy.sh` copies every file below into place after the main deploy, and a failed
copy fails the whole deploy. The deployed copies are gitignored and excluded from `sync.sh`, so the
files in this directory stay the source of truth — edit them here, never the deployed `host.*`.

| File | Deployed to | Sets |
|---|---|---|
| `hypr-host.lua` | `~/.config/hypr/host.lua` | `gaps_out`, and an `autostart` list of extra commands for this machine |
| `waybar-host.jsonc` | `~/.config/waybar/host.jsonc` | bar font, margin, and the three module lists |
| `waybar-host.css` | `~/.config/waybar/host.css` | per-machine sizes. GTK CSS has no variables and `@import` must come first, so `style.css` imports this at the top and the file owns those values outright instead of overriding them |
| `hyprlock-host.conf` | `~/.config/hypr/hyprlock-host.conf` | lock screen font sizes and widget positions (logical pixels, so they differ with monitor scale) |
| `hypridle-host.conf` | `~/.config/hypr/hypridle.conf` | the whole hypridle config: idle timeouts and what each one runs |
| `mpris-popup-margin` | `~/.config/waybar/mpris-popup-margin` | left offset of the MPRIS popup in pixels, so it lines up under the bar's module |

`hypridle-host.conf` is the one that is renamed on the way out: hypridle reads a single
`hypridle.conf` with no include mechanism, so the whole file is per machine rather than a fragment
pulled into a shared base.

The matching package lists live in `Packages/<profile>/`, so `pkg.sh export` on one machine does not
overwrite the other's.

## Adding a machine

```sh
cp -r config/hosts/laptop config/hosts/<name>
# edit the six files, then:
./Scripts/deploy.sh deploy <name>
mkdir -p Packages/<name> && ./Scripts/pkg.sh export
```

Profile names are directory names — `deploy.sh` rejects one that has no directory here and prints the
available ones.
