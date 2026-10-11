#!/usr/bin/env bash
# Installer tests: each case runs dist/install.sh / dist/uninstall.sh against
# a fresh temporary HOME (every XDG_* folder inside it), with omarchy,
# omarchy-shell, hyprctl, cliamp, llama, cargo, curl and gh replaced by stubs
# that only log what they were asked. Nothing touches the network, your
# home, or a running cliamp.
#
#   tests/install/run.sh                 with a placeholder binary
#   LLAMA_TEST_BINARY=target/release/llama tests/install/run.sh   with a real one
#   tests/install/run.sh 40              only cases/40-*.sh
#
# Temporary homes go under $TMPDIR (default /tmp) and are removed afterwards.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd -P)"
export ROOT T="$ROOT/tests/install"

SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/llama-install-tests.XXXXXX")"
export SANDBOX
trap 'rm -rf -- "$SANDBOX"' EXIT

# The binary to install: a real one if given, else a placeholder that only
# logs. Every Llama build names its repository, and so does the placeholder.
BINS="$SANDBOX/bins"
mkdir -p "$BINS/release"
if [[ -n ${LLAMA_TEST_BINARY:-} ]]; then
  cp -- "$LLAMA_TEST_BINARY" "$BINS/release/llama"
else
  printf '#!/bin/sh\n# placeholder llama (https://github.com/derekross/llama) %s\necho "llama-binary $*" >> "${FAKE_LOG:-/dev/null}"\n' "$RANDOM" >"$BINS/release/llama"
  chmod +x "$BINS/release/llama"
fi
export BINS

chmod +x "$T"/stubs/*
export PATH="$T/stubs:$PATH"
# Never reach the real thing.
for tool in omarchy omarchy-shell hyprctl cliamp llama cargo curl gh; do
  [[ "$(command -v "$tool")" == "$T/stubs/$tool" ]] || { echo "stub for $tool isn't first on PATH" >&2; exit 1; }
done

total=0 failed=0
for c in "$T"/cases/${1:-}*.sh; do
  [[ -f $c ]] || continue
  out="$(bash "$c" 2>&1)"; rc=$?
  printf '%s\n' "$out"
  n="$(grep -c '^   ok' <<<"$out")"; f="$(grep -c '^   FAIL' <<<"$out")"
  total=$((total + n + f)); failed=$((failed + f))
  (( rc == 0 )) || { echo "   FAIL: $(basename "$c") exited $rc"; failed=$((failed + 1)); }
done
echo
echo "$total checks, $failed failed"
(( failed == 0 ))
