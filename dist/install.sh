#!/usr/bin/env bash
# Install Llama for the current user:
#
#   ~/.local/bin/llama                          the player and its CLI
#   ~/.local/share/applications/llama.desktop   a launcher entry
#   ~/.config/omarchy/plugins/derekross.llama   the bar widget, copied, unless
#                                               that is an `omarchy plugin add`
#                                               checkout or a link
#   ~/.config/hypr/hyprland.lua                 only if you agree: one marked
#                                               block so the window floats
#                                               without border, blur, shadow
#                                               or transparency
#
#   ./dist/install.sh               build with cargo if Rust is installed,
#                                   otherwise download the release binary
#   ./dist/install.sh --build       always build from source
#   ./dist/install.sh --prebuilt    always download the release binary
#   ./dist/install.sh --no-build    install target/release/llama as built
#   --hypr-rule                     add the Hyprland rule without asking
#   --no-hypr-rule                  leave hyprland.lua alone, don't ask
#   --replace-existing              consent, for a non-interactive run, to
#                                   replace a ~/.local/bin/llama Llama has no
#                                   record of (it is kept as a backup)
#
# Without --hypr-rule, a terminal is asked (default no) and a run without one
# leaves hyprland.lua alone. The release binary is accepted only if it matches
# the checksum pinned in this checkout (dist/release-checksums.tsv).
#
# Run it again after `git pull` / `omarchy plugin update` to update.
# ./dist/uninstall.sh undoes it. The rules it follows are in dist/lib.sh.
set -euo pipefail

usage() { sed -n '2,31p' "$0" | sed 's/^# \{0,1\}//'; }

MODE=auto
HYPR_CHOICE=ask
CONSENT_REPLACE=0
for arg in "$@"; do
  case $arg in
    --build | --prebuilt | --no-build) MODE=$arg ;;
    --hypr-rule) HYPR_CHOICE=yes ;;
    --no-hypr-rule | --no-hypr) HYPR_CHOICE=no ;;
    --replace-existing) CONSENT_REPLACE=1 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; usage >&2; exit 2 ;;
  esac
done

cd "$(dirname "$0")/.."
REPO="$(pwd -P)"
source dist/lib.sh || { echo "dist/lib.sh is missing: run this from a Llama checkout" >&2; exit 1; }
VERSION="$(jq -r .version manifest.json)"
[[ $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "manifest.json has no usable version"
GITHUB_REPO="derekross/llama"

[[ $MODE == auto ]] && { command -v cargo >/dev/null && MODE=--build || MODE=--prebuilt; }

# Installed with `omarchy plugin add`, this checkout *is* the plugin. Build
# outside it: the shell reloads plugins whenever files change in there, and
# target/ grows to gigabytes.
FROM_PLUGIN_CHECKOUT=0
if [[ $REPO == "$(realpath -m -- "$PLUGIN_PATH")" ]]; then
  FROM_PLUGIN_CHECKOUT=1
  export CARGO_TARGET_DIR="$CACHEDIR/build"
fi
TARGET="${CARGO_TARGET_DIR:-$REPO/target}"

command -v cliamp >/dev/null \
  || say "note: cliamp isn't installed; Llama needs it (2.0 or newer) as its engine: sudo pacman -S cliamp, or https://github.com/bjarneo/cliamp"

# ── 1. Build or download ───────────────────────────────────────────────
download_release() {
  local arch name base dir pin size rc
  arch="$(uname -m)"
  [[ $arch == x86_64 || $arch == aarch64 ]] || die "No release build for $arch; install Rust to build Llama (--build)."
  name="llama-v$VERSION-$arch-linux"
  base="https://github.com/$GITHUB_REPO/releases/download/v$VERSION"
  dir="$CACHEDIR/release"
  pin="$(pinned_release "$name.tar.gz")"
  [[ -n $pin ]] \
    || die "This checkout has no pinned checksum for $name.tar.gz (a release is pinned in dist/release-checksums.tsv after it is published). Build from source with --build, or use a newer checkout."
  size="${pin##* }"
  command -v curl >/dev/null || die "curl is needed to download the release."
  [[ -L $CACHEDIR || -L $dir ]] && die "$dir (or $CACHEDIR) is a link; not downloading into it."
  rm -rf -- "${dir:?}" && mkdir -p -- "$dir"
  say "Downloading Llama v$VERSION ($arch, $size bytes)"
  # Bounded: 20 s to connect, 10 min in all, give up below 1 KiB/s for a
  # minute, and never take more than the pinned size (curl stops at once).
  rc=0
  curl -fsSL --proto '=https' --tlsv1.2 \
    --connect-timeout 20 --max-time 600 --speed-limit 1024 --speed-time 60 \
    --max-filesize "$size" -o "$dir/$name.tar.gz" "$base/$name.tar.gz" || rc=$?
  if (( rc )); then
    rm -f -- "$dir/$name.tar.gz"
    die "Download failed (curl exit $rc$( (( rc == 63 )) && printf ': larger than the pinned %s bytes' "$size")); nothing installed."
  fi
  verify_pinned "$dir/$name.tar.gz" "$name.tar.gz" \
    || die "Not installing $name.tar.gz: it isn't the release this checkout was reviewed with."
  note "Checksum matches the one pinned in this checkout (built from $PINNED_COMMIT)"
  if command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then
    gh attestation verify "$dir/$name.tar.gz" --repo "$GITHUB_REPO" >/dev/null \
      || die "Build attestation check failed; not installing it."
    note "Build attestation verified as well (GitHub Actions, $GITHUB_REPO)"
  fi
  # Only the one member we install comes out of the archive.
  tar -xzf "$dir/$name.tar.gz" -C "$dir" --no-same-owner -- "$name/llama" \
    || die "$name.tar.gz has no $name/llama."
  BIN_SRC="$dir/$name"
}

BIN_SRC="$TARGET/release"
case $MODE in
  --build)
    command -v cargo >/dev/null \
      || die "Rust is needed to build Llama: sudo pacman -S --needed rustup && rustup default stable (or run with --prebuilt)."
    say "Building Llama (cargo build --release --locked, into $TARGET)"
    cargo build --release --locked
    ;;
  --prebuilt) download_release ;;
esac
[[ -f $BIN_SRC/llama && ! -L $BIN_SRC/llama ]] || die "No built binary at $BIN_SRC/llama: build first (cargo build --release --locked), or run without --no-build."

# ── 2. Look before touching anything ───────────────────────────────────
prepare_state
load_manifest
load_known
load_shipped
print_paths

STOPS=()
stop() { STOPS+=("$*"); }

BIN_STATE="$(binary_state "$BIN")"
BIN_HASH="$(file_hash "$BIN")"
case $BIN_STATE in
  symlink) stop "$BIN is a symbolic link (to $(readlink -- "$BIN")); Llama doesn't follow links. Move it aside, then run this again." ;;
  other) stop "$BIN exists and isn't a regular file. Move it aside, then run this again." ;;
esac

inspect_launcher
LAUNCHER_HASH="$(file_hash "$LAUNCHER")"

PLUGIN_STATE="$(plugin_dir_state "$PLUGIN_PATH")"
(( FROM_PLUGIN_CHECKOUT )) && PLUGIN_STATE=this
if [[ $PLUGIN_STATE == dir ]] && ! plugin_copy_is_ours "$PLUGIN_PATH"; then
  stop "$PLUGIN_PATH is a folder that holds no file Llama wrote (another plugin, or yours). Move it aside, then run this again."
fi

for d in "$BINDIR" "$APPDIR" "$PLUGINDIR" "$DATADIR" "$CACHEDIR"; do
  if [[ -e $d && ! -d $d ]]; then stop "$d exists and isn't a folder. Move it aside, then run this again."; fi
done

if (( ${#STOPS[@]} )); then
  say "Not installing: nothing was changed."
  for s in "${STOPS[@]}"; do note "$s"; done
  exit 1
fi

hypr_scan

# ── 3. Questions ───────────────────────────────────────────────────────
REPLACE_BIN=0
if [[ $BIN_STATE == unrecorded ]]; then
  if (( CONSENT_REPLACE )); then
    REPLACE_BIN=1
  else
    say "$BIN exists but isn't recorded as installed by Llama ($(describe_file "$BIN"))."
    say "  $(binary_hint "$BIN")."
    say "  If replaced, it is moved to $BACKUPDIR/ and Llama never deletes it."
    if ask "  Replace $BIN?"; then
      REPLACE_BIN=1
    else
      say "Nothing was changed. Run again with --replace-existing to replace it, or move it aside yourself."
      exit 1
    fi
  fi
fi

HYPR_DO=none   # none | add | update
case $HYPR_STATE in
  none | stale)
    want=add; [[ $HYPR_STATE == stale ]] && want=update
    case $HYPR_CHOICE in
      yes) HYPR_DO=$want ;;
      ask)
        if [[ -t 0 ]]; then
          say
          if [[ $want == add ]]; then say "Add this block to the end of $HYPR, so the Llama window floats without border, blur, shadow or transparency?"
          else say "$HYPR has an older Llama block. Replace it with this one?"; fi
          hypr_block | sed 's/^/    /'
          say "  A copy of the file is kept in $BACKUPDIR/. uninstall.sh takes the block out again."
          if ask "  Change $HYPR?"; then HYPR_DO=$want; fi
        fi ;;
    esac ;;
esac

# ── 4. Write ───────────────────────────────────────────────────────────
say "Installing $BIN"
mkdir -p -- "$BINDIR"
case $BIN_STATE in
  missing) replace_owned "$BIN_SRC/llama" "$BIN" 755 "" ;;
  owned) replace_owned "$BIN_SRC/llama" "$BIN" 755 "$BIN_HASH" ;;
  unrecorded)
    backup_file "$BIN" llama
    note "moved the previous $BIN to $BACKUP_DEST"
    replace_owned "$BIN_SRC/llama" "$BIN" 755 "" ;;
esac

LAUNCHER_TMP="$(mktemp)"; TEMPS+=("$LAUNCHER_TMP")
render_launcher >"$LAUNCHER_TMP"
grep -q '^Exec="' "$LAUNCHER_TMP" || die "couldn't write the launcher for $(q "$BIN")"
case $LAUNCHER_STATE in
  missing) say "Installing the launcher entry"; replace_owned "$LAUNCHER_TMP" "$LAUNCHER" 644 "" ;;
  owned) say "Updating the launcher entry"; replace_owned "$LAUNCHER_TMP" "$LAUNCHER" 644 "$LAUNCHER_HASH" ;;
  other) say "Keeping $LAUNCHER: it's a link or not a file, so not Llama's." ;;
  foreign) say "Keeping $LAUNCHER: it isn't the one this script writes (you changed it, or it's another program's)." ;;
esac

PLUGIN_CREATED=0
case $PLUGIN_STATE in
  this) say "Bar widget: this checkout is the plugin (omarchy plugin add); nothing to copy." ;;
  checkout) say "Keeping $PLUGIN_PATH: it's a git checkout (omarchy plugin add); update it with: omarchy plugin update $PLUGIN_ID" ;;
  symlink) say "Keeping $PLUGIN_PATH: it's a link (a development checkout?), so not Llama's copy; not copying over it." ;;
  other) say "Keeping $PLUGIN_PATH: it isn't a folder, so not Llama's; the bar widget isn't installed." ;;
  missing)
    say "Installing the bar widget in $PLUGIN_PATH"
    create_plugin_copy "$PLUGIN_PATH"
    PLUGIN_CREATED=1 ;;
  dir)
    say "Updating the bar widget in $PLUGIN_PATH"
    update_plugin_copy "$PLUGIN_PATH" || true ;;
esac

# ── 5. hyprland.lua ────────────────────────────────────────────────────
print_rule_for_manual_use() {
  note "To float the Llama window yourself, add this to your Hyprland config:"
  hypr_block | sed 's/^/      /'
}
case $HYPR_STATE in
  missing)
    say "$HYPR doesn't exist, so no window rule was added."
    print_rule_for_manual_use ;;
  link)
    say "$HYPR is a link; Llama doesn't edit through links, so no window rule was added."
    print_rule_for_manual_use ;;
  other)
    say "$HYPR isn't a readable regular file; no window rule was added."
    print_rule_for_manual_use ;;
  broken)
    say "Not changing $HYPR: $HYPR_WHY."
    hypr_markers_help ;;
  current) say "The Llama window rule in $HYPR is already current." ;;
  none | stale)
    if [[ $HYPR_DO == none ]]; then
      if [[ $HYPR_STATE == stale ]]; then say "Left the older Llama block in $HYPR as it is."
      else say "Left $HYPR alone: no window rule added."; fi
      if [[ $HYPR_CHOICE == ask && ! -t 0 ]]; then note "(no terminal to ask on; run with --hypr-rule to add it)"; fi
      if [[ $HYPR_STATE == none ]]; then print_rule_for_manual_use; fi
    elif hypr_write "$HYPR_DO"; then
      if [[ $HYPR_DO == add ]]; then say "Added the Llama window rule to $HYPR"
      else say "Updated the Llama window rule in $HYPR"; fi
      note "the previous file is kept as $HYPR_BACKUP"
    fi ;;
esac

say
say "Done. Next:"
if (( PLUGIN_CREATED )) || [[ $PLUGIN_STATE == this ]]; then
  note "omarchy plugin enable $PLUGIN_ID     the bar widget (if it isn't on the bar yet)"
fi
note "llama                                     the player (or right-click the bar icon)"
