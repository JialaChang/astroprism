# Path
export PATH="$HOME/.local/bin:$PATH"

# Completion
autoload -Uz compinit && compinit
zstyle ':completion:*' menu select
zstyle ':completion:*' matcher-list 'm:{a-zA-Z}={A-Za-z}' 'r:|[._-]=* r:|=*' 'l:|=* r:|=*'
zstyle ':completion:*' list-colors "${(s.:.)LS_COLORS}"
zstyle ':completion:*:descriptions' format '%F{yellow}-- %d --%f'
zstyle ':completion:*:messages' format '%F{purple}-- %d --%f'
zstyle ':completion:*:warnings' format '%F{red}-- No matching items --%f'
zstyle ':completion:*' group-name ''
zmodload zsh/complist
bindkey -M menuselect '^[[Z' reverse-menu-complete

# History
HISTFILE=~/.zsh_history
HISTSIZE=100000
SAVEHIST=100000
setopt HIST_IGNORE_ALL_DUPS HIST_IGNORE_SPACE SHARE_HISTORY EXTENDED_HISTORY

# Operation
setopt AUTO_CD
setopt CORRECT
setopt AUTO_PUSHD
setopt PUSHD_IGNORE_DUPS

# Search
autoload -Uz up-line-or-beginning-search down-line-or-beginning-search
zle -N up-line-or-beginning-search
zle -N down-line-or-beginning-search
bindkey '^[[A' up-line-or-beginning-search
bindkey '^[OA' up-line-or-beginning-search
bindkey '^[[B' down-line-or-beginning-search
bindkey '^[OB' down-line-or-beginning-search

# Keys
bindkey '^[[H' beginning-of-line
bindkey '^[OH' beginning-of-line
bindkey '^[[F' end-of-line
bindkey '^[OF' end-of-line
bindkey '^[[3~' delete-char

bindkey '^H' backward-kill-word
bindkey '^[[3;5~' kill-word
bindkey '^[[1;5D' backward-word

# stop word motions at / . -
WORDCHARS=${WORDCHARS//[\/.-]}
# edits the line in $EDITOR: Ctrl+X Ctrl+E
autoload -Uz edit-command-line
zle -N edit-command-line
bindkey '^X^E' edit-command-line

# Fastfetch (skipped at Hyprland startup, which runs the big one)
if [[ -z "$FASTFETCH_SKIP" ]]; then
  clear && fastfetch -c ~/.config/fastfetch/small.jsonc
fi
unset FASTFETCH_SKIP

# Starship
eval "$(starship init zsh)"

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

# Functions
nv() { neovide "$@" & disown; }
icat() { command kitty +kitten icat "$@"; }

# Plugins (pacman)
source /usr/share/zsh/plugins/zsh-autosuggestions/zsh-autosuggestions.zsh
# at line end: → takes one word of the suggestion, Ctrl+→ takes all
suggest-right() {
  if [[ -n $POSTDISPLAY && $CURSOR -eq $#BUFFER ]]; then zle forward-word; else zle forward-char; fi
}
suggest-ctrl-right() {
  if [[ -n $POSTDISPLAY && $CURSOR -eq $#BUFFER ]]; then zle end-of-line; else zle forward-word; fi
}
zle -N suggest-right
zle -N suggest-ctrl-right
ZSH_AUTOSUGGEST_IGNORE_WIDGETS+=(suggest-right suggest-ctrl-right)
bindkey '^[[C' suggest-right
bindkey '^[OC' suggest-right
bindkey '^[[1;5C' suggest-ctrl-right

# must be sourced last
source /usr/share/zsh/plugins/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh
