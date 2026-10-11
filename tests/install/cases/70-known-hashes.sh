#!/usr/bin/env bash
source "$T/lib.sh"

case_ "1. dist/known-hashes.tsv lists every plugin file of this checkout"
fresh
if (cd "$ROOT" && git rev-parse --git-dir >/dev/null 2>&1); then
  r=$( (cd "$ROOT" && ./dist/gen-known-hashes.sh --check >"$HOME/out.txt" 2>&1); echo $?)
  expect "covered (run dist/gen-known-hashes.sh > dist/known-hashes.tsv)" [ "$r" = 0 ] && ok
else
  echo "   ok (skipped: not a git checkout)"
fi

finish
