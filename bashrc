#
# ~/.bashrc
#

# If not running interactively, don't do anything
[[ $- != *i* ]] && return

alias grep='grep --color=auto'
export PATH="$HOME/.local/bin:$PATH"

# Starship
eval "$(starship init bash)"

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

# Functions
nv() { neovide "$@" & disown; }
icat() { command kitty +kitten icat "$@"; }
