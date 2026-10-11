#!/usr/bin/env bash
source "$T/lib.sh"

case_ "1. without --purge, skins, settings and cache stay"
fresh; inst >/dev/null
mkdir -p "$HOME/.local/share/llama/skins" "$HOME/.cache/llama" ; echo s >"$HOME/.local/share/llama/skins/a.wsz"; echo '{}' >"$HOME/.local/state/llama/state.json"; echo c >"$HOME/.cache/llama/x"
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "skins kept" [ -f "$HOME/.local/share/llama/skins/a.wsz" ] && expect "settings kept" [ -f "$HOME/.local/state/llama/state.json" ] && expect "cache kept" [ -f "$HOME/.cache/llama/x" ] && ok

case_ "2. --purge deletes them, but keeps the backups Llama made"
fresh; printf 'top\n' >"$H"; inst --hypr-rule >/dev/null
mkdir -p "$HOME/.local/share/llama/skins" "$HOME/.cache/llama/build"; echo s >"$HOME/.local/share/llama/skins/a.wsz"; echo '{}' >"$HOME/.local/state/llama/state.json"
r=$(uninst --purge)
expect "exit 0" [ "$r" = 0 ] &&
expect "skins gone" [ ! -e "$HOME/.local/share/llama" ] &&
expect "cache gone" [ ! -e "$HOME/.cache/llama" ] &&
expect "settings gone" [ ! -e "$HOME/.local/state/llama/state.json" ] &&
expect "backups kept" [ -f "$B/hyprland.lua.first" ] &&
expect "everything else removed" [ ! -e "$BIN" ] && [ ! -e "$P" ] && not has_block && ok

case_ "3. --purge doesn't follow a link at a data folder"
fresh; mkdir -p "$HOME/elsewhere" "$HOME/.local/share"; echo keep >"$HOME/elsewhere/a"; ln -s "$HOME/elsewhere" "$HOME/.local/share/llama"
r=$(uninst --purge)
expect "exit 0" [ "$r" = 0 ] && expect "target kept" [ -f "$HOME/elsewhere/a" ] && expect "link kept" [ -L "$HOME/.local/share/llama" ] && expect "said" said "is a link; not followed" && ok

case_ "4. uninstall on a home Llama never touched changes nothing and leaves no state folder"
fresh; printf 'top\n' >"$H"; cp "$H" "$HOME/h.orig"; r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "no state left" [ ! -e "$HOME/.local/state/llama" ] && expect "hypr untouched" same "$HOME/h.orig" "$H" && ok

finish
