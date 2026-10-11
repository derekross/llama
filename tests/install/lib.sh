# Sourced by every case. A case is a sequence of `case_ "…"` headings, each
# followed by `expect` checks and a final `ok`.
set -u
: "${ROOT:?}" "${T:?}" "${BINS:?}" "${SANDBOX:?}"
FAILS=0

# A new, empty home inside the sandbox; every XDG folder points into it.
# fresh "with space" gives the home a name with a space in it.
fresh() {
  local base="$SANDBOX/home"
  [[ ${1:-} == "with space" ]] && base="$SANDBOX/my home"
  export HOME; HOME="$(mktemp -d "$base.XXXXXX")"
  case $HOME in "$SANDBOX"/*) ;; *) echo "refusing: HOME $HOME isn't in the sandbox" >&2; exit 1 ;; esac
  export XDG_CONFIG_HOME="$HOME/.config" XDG_DATA_HOME="$HOME/.local/share" \
    XDG_CACHE_HOME="$HOME/.cache" XDG_STATE_HOME="$HOME/.local/state" XDG_RUNTIME_DIR="$HOME/run"
  unset CARGO_TARGET_DIR LLAMA_RELEASE_CHECKSUMS
  export FAKE_LOG="$HOME/calls.log" FAKE_RELEASE_DIR="$HOME/release-server" FAKE_GH_AUTH=1 FAKE_GH_ATTEST=0
  mkdir -p "$HOME/.config/omarchy/plugins" "$HOME/.config/hypr" "$HOME/.local/bin" "$XDG_RUNTIME_DIR" "$FAKE_RELEASE_DIR"
  : >"$FAKE_LOG"
  P="$HOME/.config/omarchy/plugins/derekross.llama"
  L="$HOME/.local/share/applications/llama.desktop"
  M="$HOME/.local/state/llama/install.tsv"
  B="$HOME/.local/state/llama/backup"
  H="$HOME/.config/hypr/hyprland.lua"
  BIN="$HOME/.local/bin/llama"
}
inst() { (cd "$ROOT" && CARGO_TARGET_DIR="$BINS" ./dist/install.sh --no-build "$@" </dev/null >"$HOME/out.txt" 2>&1); echo $?; }
uninst() { (cd "$ROOT" && ./dist/uninstall.sh "$@" </dev/null >"$HOME/out.txt" 2>&1); echo $?; }
# On a terminal: tty_run "<answers>" <script> <args…>; answers are typed in.
have_tty() { command -v script >/dev/null; }
tty_run() {
  local answers=$1; shift
  local cmd; printf -v cmd '%q ' "$@"
  (cd "$ROOT" && printf '%b' "$answers" | CARGO_TARGET_DIR="$BINS" script -qec "$cmd" /dev/null >"$HOME/out.txt" 2>&1); echo $?
}
out() { cat "$HOME/out.txt"; }
said() { grep -qF -- "$1" "$HOME/out.txt"; }
logged() { grep -qF -- "$1" "$FAKE_LOG"; }
not_logged() { ! grep -qF -- "$1" "$FAKE_LOG"; }
manifest_lists() { [[ -f $M ]] && awk -F'\t' -v p="$1" '$2 == p { found = 1 } END { exit !found }' "$M"; }
manifest_verifies() { [[ -f $M ]] && awk -F'\t' '$1 != "backup" { print $1 "  " $2 }' "$M" | (cd / && sha256sum -c --quiet --strict >/dev/null 2>&1); }
same() { cmp -s -- "$1" "$2"; }
# The Hyprland block, as install.sh writes it.
block() { (cd "$ROOT" && source dist/lib.sh && hypr_block); }
has_block() { grep -qxF -- "-- >>> llama (managed by llama/dist/install.sh) >>>" "$H"; }

CASE=""
case_() { CASE=$1; echo "$1"; }
ok() { echo "   ok"; }
fail() { echo "   FAIL: $*"; sed 's/^/     | /' "$HOME/out.txt" | head -40; echo "     | calls: $(tr '\n' ';' <"$FAKE_LOG" | cut -c1-300)"; FAILS=$((FAILS + 1)); }
# expect "<what>" <command...>: runs the command; a failure names it.
expect() { local what=$1; shift; if "$@"; then return 0; fi; fail "$what"; return 1; }
not() { ! "$@"; }
finish() { exit $(( FAILS > 0 )); }
