#
# ~/.bashrc
#

# If not running interactively, don't do anything
[[ $- != *i* ]] && return

alias grep='grep --color=auto'
export PATH="$HOME/.local/bin:$PATH"

# Starship
eval "$(starship init bash)"

# fastfetch: small preview on every new terminal, skipped once at Hyprland
# startup where hyprland.lua already ran the big one and set this.
if [[ -z "$FASTFETCH_SKIP" ]]; then
  clear && fastfetch -c ~/.config/fastfetch/small.jsonc
fi
unset FASTFETCH_SKIP

# Editor
export EDITOR=nvim
export VISUAL=nvim

# Alias

alias ls='eza --icons=always --group-directories-first'
alias ll='eza -la --icons=always --group-directories-first --git'
alias lt='eza --tree --level=2 --icons=always --group-directories-first'
alias la='eza -a --icons=always --group-directories-first'

alias ff='clear && fastfetch -c ~/.config/fastfetch/big.jsonc'
alias ffs='clear && fastfetch -c ~/.config/fastfetch/small.jsonc'
alias lgit='lazygit'

alias rofi='rofi -show drun -theme ~/.config/rofi/launchers/type-1/style-7.rasi'
alias powermenu='~/.config/rofi/applets/bin/powermenu.sh'
alias screenshot='~/.config/rofi/applets/bin/screenshot.sh'

# functions, not aliases: alias args always land at the end, after the `&`
nv() { neovide "$@" & disown; }
icat() { command kitty +kitten icat "$@"; }
