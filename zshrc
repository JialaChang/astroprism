# oh-my-zsh
export ZSH="$HOME/.oh-my-zsh"
ZSH_THEME=""

plugins=(
  git
  zsh-syntax-highlighting
  zsh-autosuggestions
)

source $ZSH/oh-my-zsh.sh

# Starship
eval "$(starship init zsh)"

# Path
export PATH="$HOME/.local/bin:$PATH"

# History record setting
HISTSIZE=10000
SAVEHIST=10000
setopt HIST_IGNORE_DUPS
setopt HIST_IGNORE_SPACE

# Editor
export EDITOR='nvim'
export VISUAL='nvim'

# Alias
alias nv='neovide &disown'
alias ls='eza --icons=always --group-directories-first'
alias ll='eza -la --icons=always --group-directories-first --git'
alias lt='eza --tree --level=2 --icons=always --group-directories-first'
alias la='eza -a --icons=always --group-directories-first'

alias ff='clear && fastfetch'
alias lgit='lazygit'

alias spotify='spotify &disown'
alias discord='discord &> /dev/null &disown'

alias rofi='rofi -show drun -theme ~/.config/rofi/launchers/type-1/style-7.rasi'
alias powermenu='~/.config/rofi/applets/bin/powermenu.sh'
alias screenshot='~/.config/rofi/applets/bin/screenshot.sh'
