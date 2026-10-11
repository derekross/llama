#!/usr/bin/env bash
# Remove what dist/install.sh added. Your skins and settings
# (~/.local/share/llama, ~/.local/state/llama, ~/.cache/llama) stay unless
# you pass --purge.
set -euo pipefail

PURGE=0
[[ ${1:-} == --purge ]] && PURGE=1

BINDIR="$HOME/.local/bin"
APPDIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
PLUGIN="$HOME/.config/omarchy/plugins/derekross.llama"
HYPR="$HOME/.config/hypr/hyprland.lua"
BEGIN="-- >>> llama (managed by llama/dist/install.sh) >>>"
END="-- <<< llama <<<"

"$BINDIR/llama" --quit >/dev/null 2>&1 || true
command -v omarchy >/dev/null && omarchy plugin disable derekross.llama >/dev/null 2>&1 || true

rm -f "$BINDIR/llama" "$APPDIR/llama.desktop"
if [[ -d $PLUGIN && ! -L $PLUGIN && ! -d $PLUGIN/.git ]]; then
  rm -rf "$PLUGIN"
fi

if [[ -f $HYPR ]] && grep -qxF -- "$BEGIN" "$HYPR"; then
  cp -p "$HYPR" "$HYPR.bak.$(date +%s)"
  tmp="$(mktemp)"
  awk -v b="$BEGIN" -v e="$END" '
    $0==b { f=1; if (held && prev!="") print prev; held=0; next }
    f { if ($0==e) f=0; next }
    { if (held) print prev; prev=$0; held=1 }
    END { if (held) print prev }' "$HYPR" >"$tmp"
  cat "$tmp" >"$HYPR"
  rm -f "$tmp"
  echo "removed the Llama window rule from $HYPR (backup alongside it)"
fi

if ((PURGE)); then
  rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/llama" "${XDG_STATE_HOME:-$HOME/.local/state}/llama" "${XDG_CACHE_HOME:-$HOME/.cache}/llama"
  echo "removed Llama's skins, settings and cache"
fi
echo "Llama uninstalled"
