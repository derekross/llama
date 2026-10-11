#!/usr/bin/env bash
# Remove what dist/install.sh added: the binary, the launcher entry, the bar
# widget copy and the Hyprland rule block. Your skins and settings stay
# unless you pass --purge.
#
#   ./dist/uninstall.sh           remove Llama, keep skins and settings
#   ./dist/uninstall.sh --purge   also delete ~/.local/share/llama (skins),
#                                 ~/.local/state/llama (settings) and
#                                 ~/.cache/llama (Museum cache, builds)
#
# Only what Llama can prove it wrote is removed (the rule is in dist/lib.sh):
# a ~/.local/bin/llama Llama has no record of (llama.cpp's, say) is neither
# run nor deleted, a plugin or launcher file you changed stays and is named,
# folders keep anything you added, and the hyprland.lua block is taken out
# only while its markers are intact. Backups Llama made stay, even with
# --purge (of the hyprland.lua copies, the first and the newest five).
set -euo pipefail

usage() { sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; }

PURGE=0
for arg in "$@"; do
  case $arg in
    --purge) PURGE=1 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; usage >&2; exit 2 ;;
  esac
done

cd "$(dirname "$0")/.."
source dist/lib.sh || { echo "dist/lib.sh is missing: run this from a Llama checkout" >&2; exit 1; }

prepare_state
load_manifest
load_known
load_shipped
print_paths

# Only paths this script would write itself are taken from the record; a
# record that was edited or restored from elsewhere can't point it at
# anything else.
for p in "${!MANIFEST_HASH[@]}"; do
  case $p in
    "$BIN" | "$LAUNCHER" | "$PLUGIN_PATH"/*) ;;
    *) note "ignoring a record line for $p: not a path this script writes"; unset 'MANIFEST_HASH[$p]' ;;
  esac
done

# ── The binary ─────────────────────────────────────────────────────────
BIN_STATE="$(binary_state "$BIN")"
if [[ $BIN_STATE == owned ]]; then
  # Closing the player also stops the cliamp daemon it started.
  timeout 10 "$BIN" --quit >/dev/null 2>&1 || true
fi

# ── The bar widget ─────────────────────────────────────────────────────
say "Removing the bar widget"
case "$(plugin_dir_state "$PLUGIN_PATH")" in
  missing) note "nothing at $PLUGIN_PATH" ;;
  checkout) note "$PLUGIN_PATH is a checkout (omarchy plugin add); remove it with: omarchy plugin remove $PLUGIN_ID" ;;
  symlink) note "$PLUGIN_PATH is a link; not Llama's, leaving it." ;;
  other) note "$PLUGIN_PATH isn't a folder; not Llama's, leaving it." ;;
  dir)
    if remove_plugin_copy "$PLUGIN_PATH"; then
      note "removed $PLUGIN_PATH"
      if command -v omarchy >/dev/null; then omarchy plugin disable "$PLUGIN_ID" >/dev/null 2>&1 || true; fi
    else
      note "the shell may still list it; disable it with: omarchy plugin disable $PLUGIN_ID"
    fi ;;
esac

# ── The launcher ───────────────────────────────────────────────────────
say "Removing the launcher entry"
inspect_launcher
case $LAUNCHER_STATE in
  missing) note "nothing at $LAUNCHER" ;;
  owned) remove_owned "$LAUNCHER" "$(file_hash "$LAUNCHER")" && note "removed $LAUNCHER" ;;
  foreign) note "$LAUNCHER isn't the one install.sh wrote (you changed it, or it's another program's); leaving it." ;;
  other) note "$LAUNCHER is a link or not a file; not Llama's, leaving it." ;;
esac

# ── The binary ─────────────────────────────────────────────────────────
say "Removing the binary"
case $BIN_STATE in
  missing) note "nothing at $BIN" ;;
  owned) remove_owned "$BIN" "$(file_hash "$BIN")" && note "removed $BIN" ;;
  unrecorded) note "$BIN isn't recorded as installed by Llama ($(describe_file "$BIN")); not run, not removed." ;;
  symlink) note "$BIN is a link; not Llama's, leaving it." ;;
  other) note "$BIN isn't a regular file; leaving it." ;;
esac

# ── hyprland.lua ───────────────────────────────────────────────────────
say "Removing the Hyprland window rule"
HYPR_OK=1
hypr_scan
case $HYPR_STATE in
  missing) note "no $HYPR" ;;
  none) note "no Llama block in $HYPR" ;;
  link) if grep -qxF -- "$HYPR_BEGIN" "$HYPR" 2>/dev/null; then note "$HYPR is a link and has a Llama block; Llama doesn't edit through links: take the block out yourself."; fi ;;
  other) note "$HYPR isn't a readable regular file; leaving it." ;;
  broken)
    HYPR_OK=0
    note "Not changing $HYPR: $HYPR_WHY."
    hypr_markers_help ;;
  current | stale)
    if hypr_write remove; then
      note "removed the Llama block from $HYPR (the previous file is kept as $HYPR_BACKUP)"
    else
      HYPR_OK=0
    fi ;;
esac

# ── The record, and --purge ────────────────────────────────────────────
# Drop record lines for files that are gone (kept files keep their lines
# only while they still match, which they don't if you changed them).
for p in "${!MANIFEST_HASH[@]}"; do
  [[ -f $p && ! -L $p && "$(file_hash "$p")" == "${MANIFEST_HASH[$p]}" ]] || unset 'MANIFEST_HASH[$p]'
done
write_manifest

if (( PURGE )); then
  say "Deleting skins, settings and cache"
  for d in "$DATADIR" "$CACHEDIR"; do
    if [[ -L $d ]]; then note "$d is a link; not followed, not removed."
    elif [[ -d $d ]]; then rm -rf -- "$d"; note "removed $d"
    else note "$d: nothing there"; fi
  done
  # The state folder holds the settings, and the backups Llama made, which
  # stay; so does the install record while it still lists files you kept.
  if [[ -L $STATEDIR ]]; then note "$STATEDIR is a link; not followed, not removed."
  elif [[ -d $STATEDIR ]]; then
    find "$STATEDIR" -mindepth 1 -maxdepth 1 ! -name backup ! -name install.tsv ! -name .lock -exec rm -rf -- {} +
    note "removed the settings in $STATEDIR"
  fi
else
  say
  say "Kept: your skins and settings ($DATADIR, $STATEDIR, $CACHEDIR)."
  say "Run with --purge to delete those too."
fi
if [[ -d $BACKUPDIR ]] && [[ -n "$(ls -A -- "$BACKUPDIR" 2>/dev/null)" ]]; then
  say "Backups Llama made are in $BACKUPDIR (Llama doesn't delete them; of the"
  say "timestamped hyprland.lua copies only the newest $HYPR_KEEP are kept, plus hyprland.lua.first):"
  for f in "$BACKUPDIR"/*; do note "$f"; done
fi
# The lock file goes; the folder too if nothing else is left in it.
rm -f -- "$STATEDIR/.lock"
rmdir -- "$STATEDIR" 2>/dev/null || true
if (( HYPR_OK )); then
  say "Llama removed."
else
  say "Llama removed, except the block in $HYPR: see above."
fi
