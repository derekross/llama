# Shared by install.sh and uninstall.sh. Sourced, not run.
#
# The one rule: Llama replaces or removes a path only if it is a regular file
# (never a symlink, never a folder) whose SHA-256 matches either the line for
# that path in Llama's install record, or a file some Llama version wrote
# (dist/known-hashes.tsv). Every replacement swaps the file out first and
# checks the bytes that came out, so an edit made after the check is kept,
# not lost. Everything else is yours: it stays, and the output says so.
#
# Record: ${XDG_STATE_HOME:-~/.local/state}/llama/install.tsv, one
# "<sha256><TAB><absolute path>" per file Llama wrote, plus
# "backup<TAB><original><TAB><backup>" for files moved aside with your
# consent. Backups of those files under state/llama/backup/ are never deleted.
#
# hyprland.lua is yours, not Llama's: Llama only ever adds or removes its one
# marked block, with your consent, and only while the markers are intact.
#
# What stays imprecise, on purpose (documented in the README): parent
# folders are resolved the way the OS does (a symlinked ~/.local/bin is
# followed; only the final path component is never a link); a file you
# deliberately set back to an older Llama version's exact bytes counts as
# Llama's; when `mv --exchange` isn't available the swap degrades to two
# renames with a microsecond gap; hard links elsewhere keep the old bytes.

[[ -n ${BASH_VERSION:-} ]] || { echo "dist/lib.sh needs bash" >&2; return 1 2>/dev/null || exit 1; }

LLAMA_URL="https://github.com/derekross/llama"

CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CACHE_HOME="${XDG_CACHE_HOME:-$HOME/.cache}"
STATE_HOME="${XDG_STATE_HOME:-$HOME/.local/state}"

BINDIR="$HOME/.local/bin"
BIN="$BINDIR/llama"
APPDIR="$DATA_HOME/applications"               # the desktop spec honours XDG_DATA_HOME
LAUNCHER="$APPDIR/llama.desktop"
PLUGINDIR="$HOME/.config/omarchy/plugins"      # where Omarchy itself looks
HYPR="$CONFIG_HOME/hypr/hyprland.lua"          # so does Hyprland
STATEDIR="$STATE_HOME/llama"                   # as src/store.rs
MANIFEST="$STATEDIR/install.tsv"
BACKUPDIR="$STATEDIR/backup"
DATADIR="$DATA_HOME/llama"
CACHEDIR="$CACHE_HOME/llama"

PLUGIN_ID="$(jq -r .id manifest.json 2>/dev/null || true)"
[[ $PLUGIN_ID =~ ^[a-z0-9][a-z0-9.-]{0,63}$ ]] || { echo "manifest.json has no usable plugin id" >&2; exit 1; }
PLUGIN_PATH="$PLUGINDIR/$PLUGIN_ID"

HYPR_BEGIN="-- >>> llama (managed by llama/dist/install.sh) >>>"
HYPR_END="-- <<< llama <<<"
HYPR_KEEP=5   # timestamped hyprland.lua backups kept, besides the first one

say() { printf '%s\n' "$*"; }
note() { printf '  %s\n' "$*"; }
die() { printf '%s\n' "$*" >&2; exit 1; }

# ── Files ──────────────────────────────────────────────────────────────
path_kind() {
  if [[ -L $1 ]]; then echo symlink
  elif [[ ! -e $1 ]]; then echo missing
  elif [[ -f $1 ]]; then echo file
  elif [[ -d $1 ]]; then echo dir
  else echo other
  fi
}
# SHA-256 of a regular file, or nothing. Never fails the caller.
file_hash() {
  local out
  out="$(sha256sum -- "$1" 2>/dev/null)" || out=
  printf '%s\n' "${out%% *}"
}
hash_stdin() { sha256sum | cut -d' ' -f1; }
describe_file() {
  local size when
  size="$(stat -c %s -- "$1" 2>/dev/null || echo '?')"
  when="$(date -r "$1" '+%Y-%m-%d %H:%M' 2>/dev/null || echo '?')"
  printf '%s bytes, modified %s\n' "$size" "$when"
}
# One-line, shell-safe spelling of a name for messages.
q() { printf '%q' "$1"; }

# Temp files this run made; removed on exit. Never anything of yours.
TEMPS=()
cleanup_temps() { local t; for t in "${TEMPS[@]}"; do [[ -e $t || -L $t ]] && rm -rf -- "$t"; done; return 0; }
trap cleanup_temps EXIT

MV_EXCHANGE=0
mv --help 2>/dev/null | grep -q -- '--exchange' && MV_EXCHANGE=1

# ── The install record ─────────────────────────────────────────────────
declare -A MANIFEST_HASH=()   # absolute path -> sha256
BACKUP_LINES=()               # "original<TAB>backup"
load_manifest() {
  MANIFEST_HASH=(); BACKUP_LINES=()
  [[ -f $MANIFEST && ! -L $MANIFEST ]] || return 0
  local h p extra
  while IFS=$'\t' read -r h p extra || [[ -n $h ]]; do
    if [[ $h == backup ]]; then
      [[ $p == /* && $extra == /* ]] && BACKUP_LINES+=("$p"$'\t'"$extra")
    elif [[ $h =~ ^[0-9a-f]{64}$ && $p == /* && -z $extra ]]; then
      MANIFEST_HASH["$p"]=$h
    else
      note "ignoring an odd line in $MANIFEST: ${h:0:20}…"
    fi
  done <"$MANIFEST"
}
write_manifest() {
  local tmp p b
  if (( ${#MANIFEST_HASH[@]} == 0 && ${#BACKUP_LINES[@]} == 0 )); then
    rm -f -- "$MANIFEST"
    return 0
  fi
  mkdir -p -m 700 -- "$STATEDIR"
  tmp="$(mktemp -- "$STATEDIR/.install.XXXXXX")"
  {
    for p in "${!MANIFEST_HASH[@]}"; do printf '%s\t%s\n' "${MANIFEST_HASH[$p]}" "$p"; done | LC_ALL=C sort -t $'\t' -k2
    for b in "${BACKUP_LINES[@]}"; do printf 'backup\t%s\n' "$b"; done
  } >"$tmp"
  chmod 600 -- "$tmp"
  mv -T -- "$tmp" "$MANIFEST"
}
unrecord() { [[ -n ${MANIFEST_HASH[$1]:-} ]] || return 0; unset 'MANIFEST_HASH[$1]'; write_manifest; }

# The state folder must be a real, writable folder before anything is written.
# Also takes the lock, so two installs (two checkouts, say) can't interleave.
prepare_state() {
  if [[ -L $STATEDIR && ! -d $STATEDIR ]]; then die "$STATEDIR is a dangling link; fix it first."; fi
  [[ -e $STATEDIR && ! -d $STATEDIR ]] && die "$STATEDIR exists and isn't a folder; move it aside first."
  mkdir -p -m 700 -- "$STATEDIR" || die "can't create $STATEDIR"
  [[ -w $STATEDIR ]] || die "$STATEDIR isn't writable."
  exec 9>>"$STATEDIR/.lock"
  flock -n 9 || die "another Llama install or uninstall is running (lock: $STATEDIR/.lock)."
}

# ── What Llama versions wrote ──────────────────────────────────────────
# The launcher names the binary by its absolute path, so it can't be in the
# table; it is rendered here, as this version writes it and as 0.1.0 did.
#
# Exec= quoting, per the Desktop Entry spec: the path is one double-quoted
# argument with " ` $ \ backslash-escaped, then the value's own string
# escaping doubles every backslash, and % is written %%.
desktop_exec_arg() {
  local s=$1
  [[ $s != *$'\n'* && $s != *$'\r'* ]] || die "can't write a launcher for a path with a line break in it: $(q "$s")"
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//\`/\\\`}
  s=${s//\$/\\\$}
  s=${s//\\/\\\\}
  s=${s//%/%%}
  printf '"%s"' "$s"
}
render_launcher() { render_launcher_exec "$(desktop_exec_arg "$BIN")"; }
# What 0.1.0's install.sh wrote (Exec unquoted).
render_launcher_legacy() { render_launcher_exec "$BIN"; }
render_launcher_exec() {
  cat <<EOF
[Desktop Entry]
Type=Application
Name=Llama
GenericName=Music Player
Comment=Winamp 2.x for Omarchy, playing through cliamp
Exec=$1
Icon=multimedia-audio-player
Terminal=false
Categories=Audio;AudioVideo;Player;
Keywords=winamp;music;cliamp;skins;
StartupWMClass=llama
EOF
}

declare -A KNOWN=()   # "sha256<TAB>logical path" -> label
load_known() {
  local h l logical f
  KNOWN=()
  [[ -f dist/known-hashes.tsv ]] || die "dist/known-hashes.tsv is missing; run this from a Llama checkout."
  while IFS=$'\t' read -r h l logical || [[ -n $h ]]; do
    [[ $h == \#* || -z $logical ]] && continue
    KNOWN["$h"$'\t'"$logical"]=$l
  done <dist/known-hashes.tsv
  # This checkout counts too (an install made from it without a record).
  KNOWN["$(file_hash manifest.json)"$'\t'plugin/manifest.json]=checkout
  while IFS= read -r -d '' f; do
    KNOWN["$(file_hash "$f")"$'\t'"plugin/$f"]=checkout
  done < <(find shell-plugin -type f -print0)
  KNOWN["$(render_launcher | hash_stdin)"$'\t'launcher]=checkout
  KNOWN["$(render_launcher_legacy | hash_stdin)"$'\t'launcher]=legacy
}

# The only ownership test: owned_file <absolute path> <logical path>.
owned_file() {
  local p=$1 logical=$2 h
  [[ -f $p && ! -L $p ]] || return 1
  h="$(file_hash "$p")"
  [[ -n $h ]] || return 1
  [[ ${MANIFEST_HASH[$p]:-} == "$h" ]] && return 0
  [[ -n ${KNOWN["$h"$'\t'"$logical"]:-} ]] && return 0
  return 1
}

# ── Backups, replacing, removing ───────────────────────────────────────
# backup_file <path> <name>: moves the file into the backup folder under a
# name nobody else has, records it, and leaves the new path in BACKUP_DEST.
BACKUP_DEST=
unique_backup_name() {
  local name=$1 ts n=0 dest
  ts="$(date +%Y%m%dT%H%M%S)"
  mkdir -p -m 700 -- "$BACKUPDIR"
  while :; do
    dest="$BACKUPDIR/$name.$ts"; (( n )) && dest="$dest.$n"
    ( set -o noclobber; : >"$dest" ) 2>/dev/null && break
    (( n++ )); (( n > 999 )) && die "couldn't find a free backup name in $BACKUPDIR"
  done
  printf '%s\n' "$dest"
}
backup_file() {
  local src=$1 name=$2 dest
  dest="$(unique_backup_name "$name")"
  mv -T -- "$src" "$dest"
  BACKUP_LINES+=("$src"$'\t'"$dest")
  write_manifest
  BACKUP_DEST=$dest
}

# replace_owned <source> <dest> <mode> <expected hash of what's at dest>
# With an empty expected hash, nothing must be at dest (link(2) refuses
# otherwise). With one, the new file is swapped in and the file that came
# out is checked: if it isn't the expected bytes any more (changed after
# the check, or a link that appeared), it is kept as a backup and said so.
replace_owned() {
  local src=$1 dest=$2 mode=$3 expect=$4 dir tmp out
  dir="$(dirname -- "$dest")"
  mkdir -p -- "$dir"
  tmp="$(mktemp -- "$dir/.llama.XXXXXX")"; TEMPS+=("$tmp")
  cp -- "$src" "$tmp"
  chmod "$mode" -- "$tmp"
  if [[ -z $expect ]]; then
    if ! ln -- "$tmp" "$dest" 2>/dev/null; then
      rm -f -- "$tmp"
      note "$dest appeared while installing; not written."
      return 1
    fi
    rm -f -- "$tmp"
  else
    if (( MV_EXCHANGE )) && mv --exchange -T -- "$tmp" "$dest" 2>/dev/null; then
      out=$tmp
    else
      # No atomic exchange: two renames, a moment with nothing at dest.
      out="$(mktemp -u -- "$dir/.llama.XXXXXX")"
      mv -T -- "$dest" "$out" || { rm -f -- "$tmp"; note "couldn't move $dest aside; not replaced."; return 1; }
      mv -T -- "$tmp" "$dest"
    fi
    if [[ -L $out || "$(file_hash "$out")" != "$expect" ]]; then
      backup_file "$out" "$(basename -- "$dest")"
      note "$dest changed after it was checked; what was there is kept in $BACKUP_DEST"
    else
      rm -f -- "$out"
    fi
  fi
  MANIFEST_HASH["$dest"]="$(file_hash "$dest")"
  write_manifest
}

# remove_owned <path> <expected hash>: moves the file aside, checks it, removes it.
remove_owned() {
  local p=$1 expect=$2 out
  out="$(mktemp -u -- "$(dirname -- "$p")/.llama.XXXXXX")"
  mv -T -- "$p" "$out" || { note "couldn't move $p aside; left as is."; return 1; }
  if [[ -L $out || "$(file_hash "$out")" != "$expect" ]]; then
    backup_file "$out" "$(basename -- "$p")"
    note "$p changed after it was checked; kept in $BACKUP_DEST"
  else
    rm -f -- "$out"
  fi
  unset 'MANIFEST_HASH[$p]'
  write_manifest
}

# Remove the folders that held <rel> inside <root>, while they're empty.
prune_dirs() {
  local root=$1 rel=$2
  rel="$(dirname -- "$rel")"
  while [[ $rel != . && $rel != / ]]; do
    rmdir -- "$root/$rel" 2>/dev/null || break
    rel="$(dirname -- "$rel")"
  done
}

# walk <dir>: every entry below it, relative, into WALK. Fails (and says
# why) if the folder can't be read fully; then nothing in it is touched.
WALK=()
walk() {
  local dir=$1 list err
  WALK=()
  list="$(mktemp)"; err="$(mktemp)"; TEMPS+=("$list" "$err")
  if ! find "$dir" -mindepth 1 -printf '%P\0' >"$list" 2>"$err"; then
    note "couldn't read all of $dir: $(head -c 200 -- "$err")"
    return 1
  fi
  mapfile -d '' -t WALK <"$list"
  rm -f -- "$list" "$err"
}

# ── The plugin copy ────────────────────────────────────────────────────
# A plugin folder is Llama's copy only if it is a real folder without .git
# that is empty or holds at least one file Llama wrote. A git checkout is
# `omarchy plugin add`'s, a link is yours (a dev checkout, say).
plugin_dir_state() {
  local k; k="$(path_kind "$1")"
  case $k in
    missing | symlink | other) echo "$k" ;;
    file) echo other ;;
    dir) if [[ -e $1/.git || -L $1/.git ]]; then echo checkout; else echo dir; fi ;;
  esac
}

# The files Llama ships into a plugin copy: SHIPPED[rel] = source path, the
# same layout as the repository (manifest.json + shell-plugin/).
declare -A SHIPPED=() SHIPPED_DIRS=()
load_shipped() {
  local f d
  SHIPPED=([manifest.json]=manifest.json); SHIPPED_DIRS=()
  while IFS= read -r -d '' f; do
    SHIPPED["$f"]=$f
    d="$(dirname -- "$f")"
    while [[ $d != . ]]; do SHIPPED_DIRS["$d"]=1; d="$(dirname -- "$d")"; done
  done < <(find shell-plugin -type f -print0)
}

# plugin_copy_is_ours <dir>: empty, or holds a file Llama wrote.
plugin_copy_is_ours() {
  local dir=$1 rel
  walk "$dir" || return 1
  (( ${#WALK[@]} == 0 )) && return 0
  for rel in "${WALK[@]}"; do
    owned_file "$dir/$rel" "plugin/$rel" && return 0
  done
  return 1
}

# Every folder between <root> and <rel> must be a real folder (or not exist
# yet): a link inside the copy is never followed.
safe_parents() {
  local root=$1 rel=$2 d
  d="$(dirname -- "$rel")"
  while [[ $d != . ]]; do
    case "$(path_kind "$root/$d")" in
      missing | dir) ;;
      *) return 1 ;;
    esac
    d="$(dirname -- "$d")"
  done
}

# Say "keeping …" once per top-level folder of yours, not once per file in it.
declare -A ANNOUNCED=()
keep_note() {
  local rel=$1 why=$2 dir=$3 top=${1%%/*}
  [[ -n ${ANNOUNCED[$top]:-} ]] && return 0
  if [[ $rel != */* && -d $dir/$rel && ! -L $dir/$rel ]]; then ANNOUNCED[$top]=1; note "keeping $(q "$rel")/: $why"; return 0; fi
  note "keeping $(q "$rel"): $why"
}

# update_plugin_copy <dir>: Llama's files replaced or added in place, its
# files that are no longer shipped removed; everything of yours kept.
update_plugin_copy() {
  local dir=$1 rel kind h
  local -A done=() skip=()
  local -a pruned=()
  ANNOUNCED=()
  walk "$dir" || { note "leaving $dir as it is."; return 1; }
  for rel in "${WALK[@]}"; do
    kind="$(path_kind "$dir/$rel")"
    case $kind in
      dir)
        [[ -n ${SHIPPED_DIRS[$rel]:-} ]] && continue
        if [[ -n ${SHIPPED[$rel]:-} ]]; then note "keeping $(q "$rel")/: a folder where Llama ships a file (Llama's isn't installed)"; skip[$rel]=1
        else keep_note "$rel" "not Llama's" "$dir"; fi
        unrecord "$dir/$rel"
        continue ;;
      symlink | other)
        keep_note "$rel" "not Llama's (a link, or not a file)" "$dir"
        [[ -n ${SHIPPED[$rel]:-} ]] && { note "Llama's $(q "$rel") isn't installed."; skip[$rel]=1; }
        unrecord "$dir/$rel"
        continue ;;
    esac
    if owned_file "$dir/$rel" "plugin/$rel"; then
      h="$(file_hash "$dir/$rel")"
      if [[ -n ${SHIPPED[$rel]:-} ]]; then
        replace_owned "${SHIPPED[$rel]}" "$dir/$rel" 644 "$h" && done[$rel]=1
      else
        note "removing $(q "$rel"): Llama no longer ships it"
        remove_owned "$dir/$rel" "$h" && pruned+=("$rel")
      fi
    elif [[ -n ${SHIPPED[$rel]:-} ]]; then
      note "keeping $(q "$rel"): you changed it (Llama's version isn't installed)"
      unrecord "$dir/$rel"
      skip[$rel]=1
    else
      keep_note "$rel" "not a file Llama wrote, or you changed it" "$dir"
    fi
  done
  for rel in "${!SHIPPED[@]}"; do
    [[ -n ${done[$rel]:-} || -n ${skip[$rel]:-} ]] && continue
    if ! safe_parents "$dir" "$rel"; then note "not installing $(q "$rel"): a folder on its way is a link or not a folder."; continue; fi
    replace_owned "${SHIPPED[$rel]}" "$dir/$rel" 644 "" || true
  done
  for rel in "${pruned[@]}"; do prune_dirs "$dir" "$rel"; done
}

# create_plugin_copy <dir>: a fresh copy where nothing was.
create_plugin_copy() {
  local dir=$1 rel
  mkdir -p -m 755 -- "$dir"
  for rel in "${!SHIPPED[@]}"; do replace_owned "${SHIPPED[$rel]}" "$dir/$rel" 644 "" || true; done
}

# remove_plugin_copy <dir>: Llama's files go, yours stay. Returns 0 if the
# folder is gone afterwards.
remove_plugin_copy() {
  local dir=$1 rel kind h
  local -a pruned=()
  ANNOUNCED=()
  walk "$dir" || { note "leaving $dir as it is."; return 1; }
  for rel in "${WALK[@]}"; do
    kind="$(path_kind "$dir/$rel")"
    case $kind in
      dir) [[ -n ${SHIPPED_DIRS[$rel]:-} ]] || keep_note "$rel" "not Llama's" "$dir"; continue ;;
      symlink | other) keep_note "$rel" "not Llama's (a link, or not a file)" "$dir"; continue ;;
    esac
    if owned_file "$dir/$rel" "plugin/$rel"; then
      h="$(file_hash "$dir/$rel")"
      remove_owned "$dir/$rel" "$h" && pruned+=("$rel")
    else
      keep_note "$rel" "not a file Llama wrote, or you changed it" "$dir"
    fi
  done
  for rel in "${pruned[@]}"; do prune_dirs "$dir" "$rel"; done
  if rmdir -- "$dir" 2>/dev/null; then return 0; fi
  note "kept $dir: it still holds entries that aren't Llama's."
  return 1
}

# ── The binary and the launcher ────────────────────────────────────────
# binary_state <path>: missing | symlink | other | owned | unrecorded
binary_state() {
  case "$(path_kind "$1")" in
    missing) echo missing ;;
    symlink) echo symlink ;;
    dir | other) echo other ;;
    file) if owned_file "$1" bin/llama; then echo owned; else echo unrecorded; fi ;;
  esac
}
# A hint for the question, read from the bytes (never by running it): every
# Llama build names its repository in its HTTP user agent.
binary_hint() {
  if LC_ALL=C grep -qaF -- "github.com/derekross/llama" "$1" 2>/dev/null; then
    echo "it looks like an earlier Llama build (0.1.0 kept no record)"
  else
    echo "it doesn't look like Llama: llama.cpp and ollama, for example, install a 'llama' too"
  fi
}

LAUNCHER_STATE=missing
inspect_launcher() {
  case "$(path_kind "$LAUNCHER")" in
    missing) LAUNCHER_STATE=missing ;;
    symlink | dir | other) LAUNCHER_STATE=other ;;
    file) if owned_file "$LAUNCHER" launcher; then LAUNCHER_STATE=owned; else LAUNCHER_STATE=foreign; fi ;;
  esac
}

# ask <question> → 0 for yes. Only a terminal can say yes; the default is no.
ask() {
  local reply
  [[ -t 0 ]] || return 1
  read -r -p "$1 [y/N] " reply </dev/tty || return 1
  [[ $reply =~ ^[Yy] ]]
}

# ── hyprland.lua ───────────────────────────────────────────────────────
# The one block Llama may add, exactly as 0.1.0 wrote it.
hypr_block() {
  printf '%s\n' \
    "$HYPR_BEGIN" \
    'o.window("^llama$", {' \
    '  float = true,' \
    '  border_size = 0,' \
    '  rounding = 0,' \
    '  no_blur = true,' \
    '  no_shadow = true,' \
    '  tag = "-default-opacity",' \
    '  opacity = "1 1",' \
    '})' \
    "$HYPR_END"
}

# hypr_scan: reads hyprland.lua and sets
#   HYPR_STATE  missing | link | other | none | current | stale | broken
#   HYPR_WHY    for broken: what is wrong with the markers
#   HYPR_LINES  the file's lines; HYPR_EOL 1 if it ends with a newline (or
#               is empty), so a rewrite keeps that last byte as it was
#   HYPR_B, HYPR_E  the block's first and last line index
#   HYPR_HASH   the file's hash, to notice a change before writing
HYPR_STATE=missing HYPR_WHY= HYPR_EOL=1 HYPR_B=-1 HYPR_E=-1 HYPR_HASH=
HYPR_LINES=()
hypr_scan() {
  local i nb=0 ne=0 cur
  HYPR_LINES=(); HYPR_B=-1; HYPR_E=-1; HYPR_WHY=; HYPR_EOL=1; HYPR_HASH=
  case "$(path_kind "$HYPR")" in
    missing) HYPR_STATE=missing; return 0 ;;
    symlink) HYPR_STATE=link; return 0 ;;
    dir | other) HYPR_STATE=other; return 0 ;;
  esac
  if [[ ! -r $HYPR ]]; then HYPR_STATE=other; return 0; fi
  if ! tr -d '\000' <"$HYPR" | cmp -s -- - "$HYPR"; then HYPR_STATE=broken; HYPR_WHY="it contains NUL bytes"; return 0; fi
  HYPR_HASH="$(file_hash "$HYPR")"
  mapfile -t HYPR_LINES <"$HYPR"
  [[ -s $HYPR && -n "$(tail -c1 -- "$HYPR")" ]] && HYPR_EOL=0
  for i in "${!HYPR_LINES[@]}"; do
    if [[ ${HYPR_LINES[i]} == "$HYPR_BEGIN" ]]; then (( nb++ )) || true; HYPR_B=$i; fi
    if [[ ${HYPR_LINES[i]} == "$HYPR_END" ]]; then (( ne++ )) || true; HYPR_E=$i; fi
  done
  if (( nb == 0 && ne == 0 )); then HYPR_STATE=none; return 0; fi
  if (( nb == 1 && ne == 1 && HYPR_B < HYPR_E )); then
    cur="$(printf '%s\n' "${HYPR_LINES[@]:HYPR_B:HYPR_E-HYPR_B+1}")"
    if [[ $cur == "$(hypr_block)" ]]; then HYPR_STATE=current; else HYPR_STATE=stale; fi
    return 0
  fi
  HYPR_STATE=broken
  if (( nb > 1 )); then HYPR_WHY="it has $nb Llama start markers"
  elif (( ne > 1 )); then HYPR_WHY="it has $ne Llama end markers"
  elif (( nb == 1 && ne == 0 )); then HYPR_WHY="the Llama start marker (line $((HYPR_B + 1))) has no end marker"
  elif (( nb == 0 )); then HYPR_WHY="it has a Llama end marker (line $((HYPR_E + 1))) but no start marker"
  else HYPR_WHY="the Llama end marker (line $((HYPR_E + 1))) comes before the start marker (line $((HYPR_B + 1)))"
  fi
}
hypr_markers_help() {
  note "Llama only edits its block while there is exactly one line"
  note "  $HYPR_BEGIN"
  note "followed later by exactly one line"
  note "  $HYPR_END"
  note "Fix or remove the markers by hand, then run this again."
}

# emit <eol> <lines...>: the lines joined by newlines, with a final newline
# only if <eol> is 1.
emit() {
  local eol=$1; shift
  (( $# )) || return 0
  (( $# > 1 )) && printf '%s\n' "${@:1:$#-1}"
  printf '%s' "${!#}"
  (( eol )) && printf '\n'
  return 0
}

# hypr_backup: a copy of hyprland.lua before Llama changes it. The first one
# ever (hyprland.lua.first) is kept for good; of the timestamped ones, only
# the newest $HYPR_KEEP. Leaves the copy's path in HYPR_BACKUP.
HYPR_BACKUP=
hypr_backup() {
  local old
  mkdir -p -m 700 -- "$BACKUPDIR"
  if [[ ! -e $BACKUPDIR/hyprland.lua.first && ! -L $BACKUPDIR/hyprland.lua.first ]]; then
    cp -p -- "$HYPR" "$BACKUPDIR/hyprland.lua.first"
  fi
  HYPR_BACKUP="$(unique_backup_name hyprland.lua)"
  cp -p -- "$HYPR" "$HYPR_BACKUP"
  # Names are hyprland.lua.<YYYYmmddTHHMMSS>[.n]: they sort by time.
  while IFS= read -r old; do
    if [[ -n $old ]]; then rm -f -- "$old"; fi
  done < <(find "$BACKUPDIR" -maxdepth 1 -type f -name 'hyprland.lua.2*' -printf '%p\n' | LC_ALL=C sort -V | head -n "-$HYPR_KEEP")
}

# hypr_write add|update|remove: rewrites hyprland.lua in place (same
# permissions), after a backup, only if it is still what hypr_scan read.
hypr_write() {
  local mode=$1 tmp eol=$HYPR_EOL from
  local -a out=() block=()
  mapfile -t block < <(hypr_block)
  case $mode in
    add)
      if (( ${#HYPR_LINES[@]} == 0 )); then out=("${block[@]}"); eol=1
      else out=("${HYPR_LINES[@]}" "" "${block[@]}"); fi ;;
    update)
      out=("${HYPR_LINES[@]:0:HYPR_B}" "${block[@]}" "${HYPR_LINES[@]:HYPR_E+1}") ;;
    remove)
      from=$HYPR_B
      # The blank line install.sh put before the block goes with it.
      (( from > 0 )) && [[ -z ${HYPR_LINES[from-1]} ]] && from=$((from - 1))
      out=("${HYPR_LINES[@]:0:from}" "${HYPR_LINES[@]:HYPR_E+1}") ;;
  esac
  tmp="$(mktemp -- "$(dirname -- "$HYPR")/.llama.XXXXXX")"; TEMPS+=("$tmp")
  emit "$eol" "${out[@]}" >"$tmp"
  chmod --reference="$HYPR" -- "$tmp" 2>/dev/null || true
  if [[ -L $HYPR || "$(file_hash "$HYPR")" != "$HYPR_HASH" ]]; then
    rm -f -- "$tmp"
    note "$HYPR changed while this ran; not touched. Run this again."
    return 1
  fi
  hypr_backup
  mv -T -- "$tmp" "$HYPR"
}

# ── Release binaries ───────────────────────────────────────────────────
# The tarball a no-Rust install downloads is checked against a hash pinned
# in this checkout (dist/release-checksums.tsv), never against a checksum
# file fetched from the same release: a release can be edited, a reviewed
# commit can't. Each pin also names the source commit the build attestation
# vouched for when the pin was made.
RELEASE_CHECKSUMS="${LLAMA_RELEASE_CHECKSUMS:-dist/release-checksums.tsv}"
# pinned_release <asset>: prints "<sha256> <source commit> <size>" or nothing.
pinned_release() {
  local h a c z
  [[ -f $RELEASE_CHECKSUMS ]] || return 0
  while IFS=$'\t' read -r h a c z || [[ -n $h ]]; do
    [[ $h == \#* || -z $a ]] && continue
    [[ $a == "$1" && $h =~ ^[0-9a-f]{64}$ && $z =~ ^[0-9]+$ ]] && { printf '%s %s %s\n' "$h" "$c" "$z"; return 0; }
  done <"$RELEASE_CHECKSUMS"
}
# verify_pinned <file> <asset>: 0 if the file is exactly the pinned bytes
# (size first, so an oversized file is never even hashed).
PINNED_COMMIT=
verify_pinned() {
  local pin h c z
  pin="$(pinned_release "$2")"
  if [[ -z $pin ]]; then
    note "no pinned checksum for $2 in $RELEASE_CHECKSUMS."
    return 1
  fi
  read -r h c z <<<"$pin"
  if [[ "$(stat -c %s -- "$1" 2>/dev/null || echo -1)" != "$z" ]]; then
    note "$2 isn't the pinned size ($z bytes)."
    return 1
  fi
  if [[ "$(file_hash "$1")" != "$h" ]]; then
    note "$2 doesn't match the checksum pinned in this checkout."
    return 1
  fi
  PINNED_COMMIT=$c
}

print_paths() {
  say "Paths:"
  note "binary    $BIN"
  note "launcher  $LAUNCHER"
  note "plugin    $PLUGIN_PATH"
  note "hyprland  $HYPR"
  note "record    $MANIFEST$( [[ -f $MANIFEST ]] || printf ' (none yet)')"
  note "data      $DATADIR, $STATEDIR, $CACHEDIR"
}
