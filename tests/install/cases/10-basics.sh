#!/usr/bin/env bash
source "$T/lib.sh"

case_ "1. fresh install writes the binary, launcher and plugin copy, and records them"
fresh; printf -- '-- my config\n' >"$H"; cp "$H" "$HOME/h.orig"; r=$(inst)
expect "exit 0" [ "$r" = 0 ] &&
expect "binary" same "$BINS/release/llama" "$BIN" &&
expect "binary executable" [ -x "$BIN" ] &&
expect "launcher" [ -f "$L" ] &&
expect "launcher Exec quoted" grep -qxF "Exec=\"$BIN\"" "$L" &&
expect "plugin manifest" same "$ROOT/manifest.json" "$P/manifest.json" &&
expect "plugin widget" same "$ROOT/shell-plugin/BarWidget.qml" "$P/shell-plugin/BarWidget.qml" &&
expect "record verifies" manifest_verifies &&
expect "record lists binary" manifest_lists "$BIN" &&
expect "record lists launcher" manifest_lists "$L" &&
expect "record lists plugin file" manifest_lists "$P/shell-plugin/Panel.qml" &&
expect "hyprland.lua untouched without consent" same "$HOME/h.orig" "$H" &&
expect "rule printed for manual use" said 'o.window("^llama$", {' &&
expect "said how to opt in" said "--hypr-rule" &&
expect "no binary run" not_logged "llama-binary" &&
expect "no plugin enable/disable" not_logged "omarchy plugin" && ok

case_ "2. re-install of an untouched install: replaced silently, nothing kept, still verifies"
r=$(inst)
expect "exit 0" [ "$r" = 0 ] &&
expect "nothing kept" not said "keeping" &&
expect "no question" not said "isn't recorded" &&
expect "record verifies" manifest_verifies &&
expect "no backups" [ ! -e "$B" ] && ok

case_ "3. uninstall removes exactly what was installed, and quits our own binary first"
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] &&
expect "binary gone" [ ! -e "$BIN" ] &&
expect "launcher gone" [ ! -e "$L" ] &&
expect "plugin gone" [ ! -e "$P" ] &&
expect "record gone" [ ! -e "$M" ] &&
expect "quit asked of our binary" eval '[ -n "${LLAMA_TEST_BINARY:-}" ] || logged "llama-binary --quit"' &&
expect "plugin disabled" logged "omarchy plugin disable derekross.llama" &&
expect "hyprland.lua untouched" same "$HOME/h.orig" "$H" &&
expect "other folders kept" [ -d "$HOME/.config/omarchy/plugins" ] && ok

case_ "4. --help prints usage and changes nothing, for both scripts"
fresh; inst >/dev/null; cp -a "$HOME/.local" "$HOME/local.before"
r=$( (cd "$ROOT" && ./dist/uninstall.sh --help >"$HOME/out.txt" 2>&1); echo $?)
expect "uninstall --help exit 0" [ "$r" = 0 ] && expect "usage shown" said "--purge" &&
expect "nothing removed" [ -f "$BIN" ] && [ -d "$P" ] && [ -f "$L" ] &&
expect "no quit" not_logged "--quit" && ok
r=$( (cd "$ROOT" && ./dist/uninstall.sh -h >"$HOME/out.txt" 2>&1); echo $?)
expect "uninstall -h exit 0" [ "$r" = 0 ] && expect "nothing removed" [ -f "$BIN" ] && ok
rm -rf "$HOME/.local/state"; : >"$FAKE_LOG"
r=$( (cd "$ROOT" && ./dist/install.sh --help >"$HOME/out.txt" 2>&1); echo $?)
expect "install --help exit 0" [ "$r" = 0 ] && expect "usage shown" said "--hypr-rule" &&
expect "no state written" [ ! -e "$HOME/.local/state/llama" ] && expect "no build" not_logged "cargo" && ok

case_ "5. unknown options are rejected with usage by both scripts, before anything happens"
fresh
expect "install" [ "$(inst --bogus)" = 2 ] && expect "usage" said "Unknown option: --bogus" && said "--hypr-rule" &&
expect "nothing installed" [ ! -e "$BIN" ] &&
expect "uninstall" [ "$(uninst --purg)" = 2 ] && expect "usage" said "Unknown option: --purg" &&
expect "uninstall --help after an unknown flag still rejected" [ "$(uninst --bogus --help)" = 2 ] && ok

case_ "6. a home with a space in it: Exec is quoted, uninstall still recognises the launcher"
fresh "with space"; r=$(inst)
expect "exit 0" [ "$r" = 0 ] &&
expect "Exec quoted" grep -qxF "Exec=\"$BIN\"" "$L" &&
expect "binary" same "$BINS/release/llama" "$BIN" && ok
r=$(uninst)
expect "launcher removed" [ ! -e "$L" ] && expect "binary removed" [ ! -e "$BIN" ] && expect "plugin removed" [ ! -e "$P" ] && ok

case_ "7. a stop in the checks leaves nothing behind"
fresh; mkdir -p "$P"; echo mine >"$P/notes.txt"; r=$(inst)
expect "exit 1" [ "$r" = 1 ] &&
expect "said why" said "holds no file Llama wrote" &&
expect "no binary" [ ! -e "$BIN" ] &&
expect "no launcher" [ ! -e "$L" ] &&
expect "no record" [ ! -e "$M" ] &&
expect "folder untouched" [ "$(ls -A "$P")" = notes.txt ] &&
expect "no temp files" [ -z "$(find "$HOME" -name '.llama.*' -o -name '.install.*' | head -1)" ] && ok

case_ "8. the 0.1.0 launcher (unquoted Exec) is recognised and replaced"
fresh; mkdir -p "$(dirname "$L")"
(cd "$ROOT" && source dist/lib.sh && render_launcher_legacy) >"$L"
grep -qxF "Exec=$BIN" "$L" || fail "fixture"
r=$(inst)
expect "exit 0" [ "$r" = 0 ] && expect "replaced" grep -qxF "Exec=\"$BIN\"" "$L" && expect "not kept" not said "Keeping $L" && ok

case_ "9. a launcher that isn't ours is kept by install and uninstall"
fresh; mkdir -p "$(dirname "$L")"; printf '[Desktop Entry]\nName=Mine\nExec=/opt/llama\n' >"$L"; cp "$L" "$HOME/l.orig"
r=$(inst)
expect "exit 0" [ "$r" = 0 ] && expect "kept" same "$HOME/l.orig" "$L" && expect "said" said "Keeping $L" && ok
r=$(uninst)
expect "still kept" same "$HOME/l.orig" "$L" && ok

finish
