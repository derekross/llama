#!/usr/bin/env bash
# Install Llama for the current user:
#
#   ~/.local/bin/llama                       the player and its CLI
#   ~/.local/share/applications/llama.desktop  a launcher entry
#   ~/.config/omarchy/plugins/derekross.llama  the bar widget (copied)
#   ~/.config/hypr/hyprland.lua              one marked rule block: Llama
#                                            floats, without border, blur,
#                                            shadow or transparency
#
#   ./dist/install.sh             build (release) and install
#   ./dist/install.sh --no-build  install target/release/llama as built
#   ./dist/install.sh --no-hypr   leave hyprland.lua alone
#
# Run it again after `git pull` to update. ./dist/uninstall.sh undoes it.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
BUILD=1
HYPR_RULE=1
for arg in "$@"; do
  case $arg in
    --no-build) BUILD=0 ;;
    --no-hypr) HYPR_RULE=0 ;;
    -h | --help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown option: $arg (see --help)" >&2; exit 2 ;;
  esac
done

BINDIR="$HOME/.local/bin"
APPDIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
PLUGIN="$HOME/.config/omarchy/plugins/derekross.llama"
HYPR="$HOME/.config/hypr/hyprland.lua"
BEGIN="-- >>> llama (managed by llama/dist/install.sh) >>>"
END="-- <<< llama <<<"

command -v cliamp >/dev/null || echo "note: cliamp is not installed; Llama needs it as its engine (pacman -S cliamp or https://github.com/bjarneo/cliamp)" >&2

if ((BUILD)); then
  command -v cargo >/dev/null || { echo "cargo is not installed (pacman -S rust)" >&2; exit 1; }
  cargo build --release --locked 2>/dev/null || cargo build --release
fi
[[ -x target/release/llama ]] || { echo "target/release/llama is missing: build first" >&2; exit 1; }

install -Dm755 target/release/llama "$BINDIR/llama"
echo "installed $BINDIR/llama"

install -Dm644 /dev/stdin "$APPDIR/llama.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Llama
GenericName=Music Player
Comment=Winamp 2.x for Omarchy, playing through cliamp
Exec=$BINDIR/llama
Icon=multimedia-audio-player
Terminal=false
Categories=Audio;AudioVideo;Player;
Keywords=winamp;music;cliamp;skins;
StartupWMClass=llama
EOF
echo "installed $APPDIR/llama.desktop"

# The bar widget: a plain copy, so builds in this checkout never make the
# shell reload. A git checkout from `omarchy plugin add` is left alone.
if [[ -d $PLUGIN/.git ]]; then
  echo "$PLUGIN is a git checkout (omarchy plugin add); not copying over it"
elif [[ -L $PLUGIN ]]; then
  echo "$PLUGIN is a symlink; not touching it"
else
  mkdir -p "$PLUGIN/shell-plugin"
  install -m644 manifest.json "$PLUGIN/manifest.json"
  install -m644 shell-plugin/*.qml shell-plugin/*.js "$PLUGIN/shell-plugin/"
  echo "installed the bar widget in $PLUGIN"
fi

if ((HYPR_RULE)) && [[ -f $HYPR ]]; then
  block="$BEGIN
o.window(\"^llama\$\", {
  float = true,
  border_size = 0,
  rounding = 0,
  no_blur = true,
  no_shadow = true,
  tag = \"-default-opacity\",
  opacity = \"1 1\",
})
$END"
  current="$(awk -v b="$BEGIN" -v e="$END" '$0==b{f=1} f{print} $0==e{f=0}' "$HYPR")"
  if [[ $current == "$block" ]]; then
    echo "Hyprland rule already current"
  else
    cp -p "$HYPR" "$HYPR.bak.$(date +%s)"
    tmp="$(mktemp)"
    awk -v b="$BEGIN" -v e="$END" '
    $0==b { f=1; if (held && prev!="") print prev; held=0; next }
    f { if ($0==e) f=0; next }
    { if (held) print prev; prev=$0; held=1 }
    END { if (held) print prev }' "$HYPR" >"$tmp"
    printf '\n%s\n' "$block" >>"$tmp"
    cat "$tmp" >"$HYPR"
    rm -f "$tmp"
    echo "added the Llama window rule to $HYPR (backup alongside it)"
  fi
fi

cat <<EOF

Done. Next:
  omarchy plugin enable derekross.llama     the bar widget
  llama                                     the player (or middle-click the bar icon)
EOF
