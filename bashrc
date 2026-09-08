#
# ~/.bashrc
#

# If not running interactively, don't do anything
[[ $- != *i* ]] && return

alias ls='ls --color=auto'
alias grep='grep --color=auto'
PS1='[\u@\h \W]\$ '
export PATH="$HOME/.local/bin:$PATH"

# oh-my-posh
eval "$(oh-my-posh init bash --config /usr/share/oh-my-posh/themes/catppuccin_frappe.omp.json)"

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
alias nv='neovide &disown'

alias ls='eza --icons=always --group-directories-first'
alias ll='eza -la --icons=always --group-directories-first --git'
alias lt='eza --tree --level=2 --icons=always --group-directories-first'
alias la='eza -a --icons=always --group-directories-first'

alias ff='clear && fastfetch -c ~/.config/fastfetch/big.jsonc'
alias ffs='clear && fastfetch -c ~/.config/fastfetch/small.jsonc'
alias lgit='lazygit'

alias spotify='spotify &disown'
alias discord='discord &> /dev/null & disown'

alias rofi='rofi -show drun -theme ~/.config/rofi/launchers/type-1/style-7.rasi'
alias powermenu='~/.config/rofi/applets/bin/powermenu.sh'
alias screenshot='~/.config/rofi/applets/bin/screenshot.sh'
