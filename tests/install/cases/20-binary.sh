#!/usr/bin/env bash
source "$T/lib.sh"

foreign() { printf '#!/bin/sh\necho "FOREIGN-RAN $*" >> "$FAKE_LOG"\n' >"$BIN"; chmod +x "$BIN"; cp "$BIN" "$HOME/foreign"; }

case_ "1. a foreign ~/.local/bin/llama (llama.cpp, ollama): non-interactive install stops before any write"
fresh; foreign; r=$(inst)
expect "exit 1" [ "$r" = 1 ] &&
expect "names it" said "$BIN exists but isn't recorded" &&
expect "says it isn't Llama" said "doesn't look like Llama" &&
expect "names the flag" said "--replace-existing" &&
expect "untouched" same "$HOME/foreign" "$BIN" &&
expect "never run" not_logged "FOREIGN-RAN" &&
expect "nothing else written" [ ! -e "$P" ] && [ ! -e "$L" ] && ok

case_ "2. uninstall leaves a foreign binary alone: not run, not removed"
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] &&
expect "kept" same "$HOME/foreign" "$BIN" &&
expect "never run" not_logged "FOREIGN-RAN" &&
expect "said" said "not run, not removed" && ok

case_ "3. --replace-existing: replaced, the old one kept as a backup and recorded"
r=$(inst --replace-existing)
expect "exit 0" [ "$r" = 0 ] &&
expect "new binary" same "$BINS/release/llama" "$BIN" &&
expect "backup content" same "$HOME/foreign" "$B"/llama.* &&
expect "backup recorded" grep -q "^backup	$BIN	$B/llama\." "$M" &&
expect "said where" said "moved the previous $BIN to $B/llama." &&
expect "never run" not_logged "FOREIGN-RAN" && ok

case_ "4. uninstall then removes ours, keeps the backup and lists it"
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "binary gone" [ ! -e "$BIN" ] &&
expect "backup kept" same "$HOME/foreign" "$B"/llama.* &&
expect "backups listed" said "Backups Llama made are in $B" && ok

case_ "5. an unrecorded earlier Llama build is named as such, and still needs consent"
fresh; cp "$BINS/release/llama" "$BIN"; printf '\n# rebuilt\n' >>"$BIN"; r=$(inst)
expect "exit 1" [ "$r" = 1 ] && expect "hint" said "looks like an earlier Llama build" && ok

case_ "6. the interactive question: n stops, y replaces with a backup"
if have_tty; then
  fresh; foreign
  r=$(tty_run 'n\n' ./dist/install.sh --no-build)
  expect "declined stops" [ "$r" = 1 ] && expect "untouched" same "$HOME/foreign" "$BIN" && ok
  r=$(tty_run 'y\nn\n' ./dist/install.sh --no-build)
  expect "accepted installs" [ "$r" = 0 ] && expect "replaced" same "$BINS/release/llama" "$BIN" &&
  expect "backup" same "$HOME/foreign" "$B"/llama.* && expect "never run" not_logged "FOREIGN-RAN" && ok
else
  echo "   ok (skipped: no 'script' command)"
fi

case_ "7. a symlink or a folder at the binary path stops the install, never followed"
fresh; printf 'target\n' >"$HOME/real"; ln -s "$HOME/real" "$BIN"; r=$(inst)
expect "exit 1" [ "$r" = 1 ] && expect "message" said "is a symbolic link" && expect "target untouched" [ "$(cat "$HOME/real")" = target ] && expect "still a link" [ -L "$BIN" ] && ok
r=$(uninst)
expect "uninstall keeps the link" [ -L "$BIN" ] && expect "said" said "is a link; not Llama's" && ok
fresh; mkdir "$BIN"; r=$(inst)
expect "exit 1" [ "$r" = 1 ] && expect "message" said "isn't a regular file" && ok

case_ "8. a binary changed after install isn't ours any more: uninstall keeps it, doesn't run it"
fresh; inst >/dev/null; printf '#!/bin/sh\necho "FOREIGN-RAN $*" >> "$FAKE_LOG"\n' >"$BIN"; r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "kept" [ -f "$BIN" ] && expect "not run" not_logged "FOREIGN-RAN" && ok

case_ "9. a file that changed after the check is kept as a backup, not overwritten"
fresh; inst >/dev/null
printf 'edited after check\n' >>"$BIN"
(cd "$ROOT" && source dist/lib.sh && prepare_state && load_manifest && replace_owned "$BINS/release/llama" "$BIN" 755 "${MANIFEST_HASH[$BIN]}" >"$HOME/out.txt" 2>&1)
expect "new file in place" same "$BINS/release/llama" "$BIN" &&
expect "old kept" grep -q "edited after check" "$B"/llama.* &&
expect "said" said "changed after it was checked" && ok

case_ "10. --build: cargo build --release --locked, nothing hidden, no unlocked fallback"
fresh; export CARGO_TARGET_DIR="$HOME/target"
r=$( (cd "$ROOT" && ./dist/install.sh --build </dev/null >"$HOME/out.txt" 2>&1); echo $?)
expect "exit 0" [ "$r" = 0 ] && expect "locked build" logged "cargo build --release --locked CARGO_TARGET_DIR=$HOME/target" &&
expect "one build only" [ "$(grep -c '^cargo build' "$FAKE_LOG")" = 1 ] &&
expect "installed" same "$BINS/release/llama" "$BIN" && ok
unset CARGO_TARGET_DIR

case_ "11. --build without cargo names rustup"
fresh; mkdir -p "$HOME/nocargo"; for t in "$T"/stubs/*; do [ "${t##*/}" = cargo ] || ln -s "$t" "$HOME/nocargo/"; done
for t in jq bash sed cat dirname realpath mktemp sha256sum cut sort awk find stat date flock mv cp ln rm mkdir chmod readlink grep head tail tr cmp uname timeout env pwd; do
  p="$(command -v "$t")" && ln -sf "$p" "$HOME/nocargo/$t"; done
r=$( (cd "$ROOT" && PATH="$HOME/nocargo" ./dist/install.sh --build </dev/null >"$HOME/out.txt" 2>&1); echo $?)
expect "exit 1" [ "$r" = 1 ] && expect "rustup hint" said "rustup" && expect "nothing written" [ ! -e "$BIN" ] && ok

finish
