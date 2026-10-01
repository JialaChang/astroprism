# Completion
autoload -Uz compinit && compinit
zstyle ':completion:*' menu select
zstyle ':completion:*' matcher-list 'm:{a-z}={A-Z}'

# History
HISTFILE=~/.zsh_history
HISTSIZE=10000
SAVEHIST=10000
setopt HIST_IGNORE_DUPS HIST_IGNORE_SPACE SHARE_HISTORY EXTENDED_HISTORY

# Up/Down search history by the typed prefix
autoload -Uz up-line-or-beginning-search down-line-or-beginning-search
zle -N up-line-or-beginning-search
zle -N down-line-or-beginning-search
bindkey '^[[A' up-line-or-beginning-search
bindkey '^[OA' up-line-or-beginning-search
bindkey '^[[B' down-line-or-beginning-search
bindkey '^[OB' down-line-or-beginning-search

# fastfetch: small preview on every new terminal, skipped once at Hyprland
# startup where hyprland.lua already ran the big one and set this.
if [[ -z "$FASTFETCH_SKIP" ]]; then
  clear && fastfetch -c ~/.config/fastfetch/small.jsonc
fi
unset FASTFETCH_SKIP

# Starship
eval "$(starship init zsh)"

# Path
export PATH="$HOME/.local/bin:$PATH"

# Editor
export EDITOR='nvim'
export VISUAL='nvim'

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
spotify() { command spotify "$@" & disown; }
discord() { command discord "$@" &> /dev/null & disown; }

# Plugins from pacman; syntax-highlighting must be sourced last
source /usr/share/zsh/plugins/zsh-autosuggestions/zsh-autosuggestions.zsh
source /usr/share/zsh/plugins/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh
