#!/usr/bin/env bash
source "$T/lib.sh"

seed() { printf '%b' "$1" >"$H"; cp "$H" "$HOME/h.orig"; }

case_ "1. --hypr-rule adds the block at the end, after a backup; uninstall restores the file byte for byte"
fresh; seed '-- mine\nlocal x = 1\n'; r=$(inst --hypr-rule)
expect "exit 0" [ "$r" = 0 ] &&
expect "block added" has_block &&
expect "exact block at the end" diff <(tail -n 11 "$H") <(block) &&
expect "original kept above" diff <(head -n 2 "$H") "$HOME/h.orig" &&
expect "first backup" same "$HOME/h.orig" "$B/hyprland.lua.first" &&
expect "said" said "Added the Llama window rule" && ok
r=$(inst --hypr-rule)
expect "idempotent" said "already current" && expect "one block" [ "$(grep -c '^-- >>> llama' "$H")" = 1 ] && ok
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "byte for byte" same "$HOME/h.orig" "$H" && ok

case_ "2. no trailing newline: the round trip keeps the missing last newline"
fresh; seed '-- mine\nlocal x = 1'; r=$(inst --hypr-rule)
expect "exit 0" [ "$r" = 0 ] && expect "block added" has_block && ok
r=$(uninst)
expect "byte for byte" same "$HOME/h.orig" "$H" && ok

case_ "3. other shapes round-trip too: empty file, blank lines at the end, one line"
for s in '' '\n' 'a\n\n' 'a\n\n\n' 'x'; do
  fresh; seed "$s"; inst --hypr-rule >/dev/null; has_block || fail "no block for '$s'"; uninst >/dev/null
  expect "round trip of '$s'" same "$HOME/h.orig" "$H" || break
done && ok

case_ "4. lines added after the block survive uninstall"
fresh; seed 'top\n'; inst --hypr-rule >/dev/null; printf 'after\n' >>"$H"; r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "result" [ "$(cat "$H")" = "$(printf 'top\nafter')" ] && ok

case_ "5. no terminal and no flag: hyprland.lua is left alone and the rule printed"
fresh; seed 'top\n'; r=$(inst)
expect "exit 0" [ "$r" = 0 ] && expect "untouched" same "$HOME/h.orig" "$H" && expect "said" said "no terminal to ask on" && expect "no backup" [ ! -e "$B" ] && ok
fresh; seed 'top\n'; r=$(inst --no-hypr-rule)
expect "untouched with --no-hypr-rule" same "$HOME/h.orig" "$H" && expect "no question mention" not said "no terminal" && ok

case_ "6. on a terminal the exact block is shown; the default (enter) is no, y adds it"
if have_tty; then
  fresh; seed 'top\n'
  r=$(tty_run '\n' ./dist/install.sh --no-build)
  expect "exit 0" [ "$r" = 0 ] && expect "block shown" said "o.window(\"^llama\$\", {" && expect "asked" said "Change $H?" &&
  expect "default no" same "$HOME/h.orig" "$H" && ok
  r=$(tty_run 'n\n' ./dist/install.sh --no-build)
  expect "n: untouched" same "$HOME/h.orig" "$H" && ok
  r=$(tty_run 'y\n' ./dist/install.sh --no-build)
  expect "y: added" has_block && ok
else
  echo "   ok (skipped: no 'script' command)"
fi

case_ "7. END marker missing: install and uninstall refuse to touch the file, and say why"
fresh; seed 'top\n-- >>> llama (managed by llama/dist/install.sh) >>>\no.window("^llama$", {})\nlocal keep = "everything below"\nmore()\n'
r=$(inst --hypr-rule)
expect "install exit 0" [ "$r" = 0 ] && expect "untouched" same "$HOME/h.orig" "$H" && expect "why" said "has no end marker" && ok
r=$(uninst)
expect "uninstall exit 0" [ "$r" = 0 ] && expect "untouched" same "$HOME/h.orig" "$H" && expect "why" said "has no end marker" &&
expect "summary" said "except the block in $H" && ok

case_ "8. other broken markers: two blocks, END before BEGIN, END alone"
for s in '-- >>> llama (managed by llama/dist/install.sh) >>>\n-- <<< llama <<<\n-- >>> llama (managed by llama/dist/install.sh) >>>\n-- <<< llama <<<\n' \
         '-- <<< llama <<<\nx\n-- >>> llama (managed by llama/dist/install.sh) >>>\n' \
         'x\n-- <<< llama <<<\n'; do
  fresh; seed "$s"; inst --hypr-rule >/dev/null
  expect "install left it: $(printf '%b' "$s" | head -c 30)…" same "$HOME/h.orig" "$H" || break
  uninst >/dev/null
  expect "untouched: $(printf '%b' "$s" | head -c 30)…" same "$HOME/h.orig" "$H" || break
done && ok

case_ "9. an older Llama block is updated only with consent"
fresh; seed 'top\n\n-- >>> llama (managed by llama/dist/install.sh) >>>\no.window("^llama$", { float = true })\n-- <<< llama <<<\nbottom\n'
r=$(inst)
expect "kept without consent" same "$HOME/h.orig" "$H" && expect "said" said "older Llama block" && ok
r=$(inst --hypr-rule)
expect "updated" diff <(sed -n '3,13p' "$H") <(block) && expect "around kept" [ "$(head -1 "$H")" = top ] && [ "$(tail -1 "$H")" = bottom ] && ok

case_ "10. hyprland.lua missing: said so, rule printed, nothing created"
fresh; r=$(inst --hypr-rule)
expect "exit 0" [ "$r" = 0 ] && expect "said" said "$H doesn't exist" && expect "rule printed" said 'no_shadow = true,' && expect "not created" [ ! -e "$H" ] && ok

case_ "11. hyprland.lua as a link (dotfiles): never edited through"
fresh; printf 'top\n' >"$HOME/dotfile.lua"; ln -s "$HOME/dotfile.lua" "$H"; r=$(inst --hypr-rule)
expect "exit 0" [ "$r" = 0 ] && expect "target untouched" [ "$(cat "$HOME/dotfile.lua")" = top ] && expect "still a link" [ -L "$H" ] && expect "said" said "is a link" && ok

case_ "12. backups have unique names and don't pile up"
fresh; seed 'top\n'
for i in 1 2 3 4 5 6 7 8; do inst --hypr-rule >/dev/null; uninst >/dev/null; done
n="$(find "$B" -name 'hyprland.lua.2*' | wc -l)"
expect "at most 5 timestamped ($n)" [ "$n" -le 5 ] && expect "first kept" same "$HOME/h.orig" "$B/hyprland.lua.first" &&
expect "file back as it was" same "$HOME/h.orig" "$H" && ok

case_ "13. the file's permissions are kept"
fresh; seed 'top\n'; chmod 600 "$H"; inst --hypr-rule >/dev/null
expect "mode kept" [ "$(stat -c %a "$H")" = 600 ] && ok

finish
