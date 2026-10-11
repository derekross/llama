#!/usr/bin/env bash
source "$T/lib.sh"

case_ "1. an edited plugin file is kept by re-install and by uninstall; the rest is Llama's"
fresh; inst >/dev/null; echo "// mine" >>"$P/shell-plugin/Panel.qml"; cp "$P/shell-plugin/Panel.qml" "$HOME/edited"
r=$(inst)
expect "exit 0" [ "$r" = 0 ] &&
expect "edit kept" same "$HOME/edited" "$P/shell-plugin/Panel.qml" &&
expect "named" said "keeping shell-plugin/Panel.qml: you changed it" &&
expect "edited file no longer recorded" not manifest_lists "$P/shell-plugin/Panel.qml" &&
expect "others still recorded" manifest_lists "$P/shell-plugin/BarWidget.qml" && ok
echo "notes" >"$P/my-notes.txt"
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] &&
expect "edit kept" same "$HOME/edited" "$P/shell-plugin/Panel.qml" &&
expect "own file kept" [ -f "$P/my-notes.txt" ] &&
expect "ours removed" [ ! -e "$P/shell-plugin/BarWidget.qml" ] && [ ! -e "$P/manifest.json" ] &&
expect "folder kept, said why" said "kept $P" && ok

case_ "2. a folder at the plugin path that isn't Llama's: install stops, uninstall leaves it"
fresh; mkdir -p "$P/shell-plugin"; echo '{"id":"derekross.llama","mine":true}' >"$P/manifest.json"; echo x >"$P/shell-plugin/Mine.qml"
cp -a "$P" "$HOME/p.orig"; r=$(inst)
expect "exit 1" [ "$r" = 1 ] && expect "said" said "holds no file Llama wrote" && expect "untouched" diff -r "$HOME/p.orig" "$P" && ok
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "untouched" diff -r "$HOME/p.orig" "$P" && ok

case_ "3. an empty folder at the plugin path is filled"
fresh; mkdir -p "$P"; r=$(inst)
expect "exit 0" [ "$r" = 0 ] && expect "copied" same "$ROOT/manifest.json" "$P/manifest.json" && ok

case_ "4. a git checkout at the plugin path (omarchy plugin add from elsewhere): never copied over or removed"
fresh; mkdir -p "$P/.git" "$P/shell-plugin"; echo old >"$P/shell-plugin/Panel.qml"; cp -a "$P" "$HOME/p.orig"; r=$(inst)
expect "exit 0" [ "$r" = 0 ] && expect "untouched" diff -r "$HOME/p.orig" "$P" && expect "said" said "it's a git checkout" && ok
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "untouched" diff -r "$HOME/p.orig" "$P" && expect "points to omarchy plugin remove" said "omarchy plugin remove derekross.llama" &&
expect "not disabled behind its back" not_logged "plugin disable" && ok

case_ "5. a symlink at the plugin path (a dev checkout): never copied into or removed"
fresh; mkdir -p "$HOME/dev/shell-plugin"; echo dev >"$HOME/dev/shell-plugin/Panel.qml"; ln -s "$HOME/dev" "$P"; r=$(inst)
expect "exit 0" [ "$r" = 0 ] && expect "still a link" [ -L "$P" ] && expect "target untouched" [ "$(ls -A "$HOME/dev")" = shell-plugin ] && [ "$(cat "$HOME/dev/shell-plugin/Panel.qml")" = dev ] && ok
r=$(uninst)
expect "still a link" [ -L "$P" ] && expect "target untouched" [ "$(cat "$HOME/dev/shell-plugin/Panel.qml")" = dev ] && ok

case_ "6. a link inside the plugin copy is never written through"
fresh; inst >/dev/null; rm -rf "$P/shell-plugin"; mkdir "$HOME/elsewhere"; ln -s "$HOME/elsewhere" "$P/shell-plugin"; r=$(inst)
expect "exit 0" [ "$r" = 0 ] && expect "nothing written through it" [ -z "$(ls -A "$HOME/elsewhere")" ] && expect "said" said "not installing shell-plugin/" && ok
r=$(uninst)
expect "link kept" [ -L "$P/shell-plugin" ] && ok

case_ "7. run from the checkout omarchy plugin add made: builds outside it, copies nothing"
fresh; mkdir -p "$P/.git"
cp -a "$ROOT/dist" "$ROOT/manifest.json" "$ROOT/shell-plugin" "$ROOT/Cargo.toml" "$P/"
find "$P" -type f ! -path '*/.git/*' -printf '%P\n' | sort >"$HOME/p.files"
r=$( (cd "$HOME" && "$P/dist/install.sh" </dev/null >"$HOME/out.txt" 2>&1); echo $?)
expect "exit 0" [ "$r" = 0 ] &&
expect "built into the cache" logged "cargo build --release --locked CARGO_TARGET_DIR=$HOME/.cache/llama/build" &&
expect "no target/ in the plugin" [ ! -e "$P/target" ] &&
expect "plugin files unchanged" diff "$HOME/p.files" <(find "$P" -type f ! -path '*/.git/*' -printf '%P\n' | sort) &&
expect "said" said "this checkout is the plugin" &&
expect "installed" same "$BINS/release/llama" "$BIN" && ok
r=$( (cd "$HOME" && "$P/dist/uninstall.sh" </dev/null >"$HOME/out.txt" 2>&1); echo $?)
expect "uninstall exit 0" [ "$r" = 0 ] && expect "checkout kept" [ -f "$P/manifest.json" ] && expect "binary gone" [ ! -e "$BIN" ] && ok

case_ "8. a plugin copy from an earlier commit (0.1.0, no record) is recognised and updated"
if (cd "$ROOT" && git rev-parse -q --verify eeb7a42 >/dev/null 2>&1); then
  fresh; mkdir -p "$P/shell-plugin"
  for f in manifest.json shell-plugin/BarWidget.qml shell-plugin/Panel.qml shell-plugin/Model.js; do (cd "$ROOT" && git show "eeb7a42:$f") >"$P/$f"; done
  r=$(inst)
  expect "exit 0" [ "$r" = 0 ] && expect "updated" same "$ROOT/shell-plugin/Panel.qml" "$P/shell-plugin/Panel.qml" && expect "nothing kept" not said "keeping" && ok
else
  echo "   ok (skipped: not a git checkout)"
fi

case_ "9. a record line pointing elsewhere is ignored by uninstall"
fresh; inst >/dev/null; echo precious >"$HOME/precious"
printf '%s\t%s\n' "$(sha256sum "$HOME/precious" | cut -c1-64)" "$HOME/precious" >>"$M"
r=$(uninst)
expect "exit 0" [ "$r" = 0 ] && expect "kept" [ -f "$HOME/precious" ] && expect "said" said "ignoring a record line for $HOME/precious" && ok

finish
