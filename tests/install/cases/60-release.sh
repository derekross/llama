#!/usr/bin/env bash
source "$T/lib.sh"

# The download itself needs the network; the stub curl serves files from
# $FAKE_RELEASE_DIR instead, and the pins come from a table in the sandbox.
check() { (cd "$ROOT" && source dist/lib.sh && verify_pinned "$@" >"$HOME/out.txt" 2>&1); }
V="$(jq -r .version "$ROOT/manifest.json")"
ARCH="$(uname -m)"
NAME="llama-v$V-$ARCH-linux"

# make_release: a tarball like the release workflow's, served and pinned.
make_release() {
  mkdir -p "$HOME/pkg/$NAME"
  cp "$BINS/release/llama" "$HOME/pkg/$NAME/llama"
  tar -C "$HOME/pkg" -czf "$FAKE_RELEASE_DIR/$NAME.tar.gz" --owner=0 --group=0 "$NAME"
  printf '# pins\n%s\t%s\t%s\t%s\n' "$(sha256sum "$FAKE_RELEASE_DIR/$NAME.tar.gz" | cut -c1-64)" "$NAME.tar.gz" 0123456789abcdef0123456789abcdef01234567 \
    "$(stat -c %s "$FAKE_RELEASE_DIR/$NAME.tar.gz")" >"$HOME/pins.tsv"
  export LLAMA_RELEASE_CHECKSUMS="$HOME/pins.tsv"
}
prebuilt() { (cd "$ROOT" && ./dist/install.sh --prebuilt "$@" </dev/null >"$HOME/out.txt" 2>&1); echo $?; }

case_ "1. verify_pinned accepts only the pinned size and hash"
fresh; echo "release bytes" >"$HOME/asset.tar.gz"; h=$(sha256sum "$HOME/asset.tar.gz" | cut -c1-64)
printf '# pins\n%s\tllama-v9.9.9-x86_64-linux.tar.gz\tdeadbeef\t%s\n' "$h" "$(stat -c %s "$HOME/asset.tar.gz")" >"$HOME/pins.tsv"
export LLAMA_RELEASE_CHECKSUMS="$HOME/pins.tsv"
expect "matching" check "$HOME/asset.tar.gz" llama-v9.9.9-x86_64-linux.tar.gz && ok
echo "tampered" >>"$HOME/asset.tar.gz"
expect "oversized refused" not check "$HOME/asset.tar.gz" llama-v9.9.9-x86_64-linux.tar.gz && said "isn't the pinned size" && ok
printf 'release bytez\n' >"$HOME/asset.tar.gz"
expect "same size, other bytes refused" not check "$HOME/asset.tar.gz" llama-v9.9.9-x86_64-linux.tar.gz && said "doesn't match the checksum pinned" && ok
expect "unpinned asset refused" not check "$HOME/asset.tar.gz" llama-v9.9.9-aarch64-linux.tar.gz && said "no pinned checksum" && ok

case_ "2. --prebuilt with a pinned release: downloaded (bounded), checked, installed"
fresh; make_release; r=$(prebuilt)
expect "exit 0" [ "$r" = 0 ] &&
expect "installed" same "$BINS/release/llama" "$BIN" &&
expect "https only" logged "--proto =https" &&
expect "size bound" logged "--max-filesize $(stat -c %s "$FAKE_RELEASE_DIR/$NAME.tar.gz")" &&
expect "time bounds" logged "--connect-timeout 20 --max-time 600 --speed-limit 1024 --speed-time 60" &&
expect "right URL" logged "https://github.com/derekross/llama/releases/download/v$V/$NAME.tar.gz" &&
expect "said" said "Checksum matches the one pinned" &&
expect "no attestation check when gh isn't signed in" not_logged "attestation" &&
expect "no build" not_logged "cargo build" && ok

case_ "3. with gh signed in the attestation is checked too; a failed check installs nothing"
fresh; make_release; export FAKE_GH_AUTH=0; r=$(prebuilt)
expect "exit 0" [ "$r" = 0 ] && expect "verified" logged "gh attestation verify" && logged "--repo derekross/llama" && ok
fresh; make_release; export FAKE_GH_AUTH=0 FAKE_GH_ATTEST=1; r=$(prebuilt)
expect "exit 1" [ "$r" = 1 ] && expect "said" said "attestation check failed" && expect "nothing installed" [ ! -e "$BIN" ] && ok

case_ "4. a release that isn't the pinned bytes is refused"
fresh; make_release; printf 'x' >>"$FAKE_RELEASE_DIR/$NAME.tar.gz"; r=$(prebuilt)
expect "exit 1" [ "$r" = 1 ] && expect "curl refused it as too large" said "curl exit 63" && expect "nothing installed" [ ! -e "$BIN" ] && ok
fresh; make_release; printf 'garbage' | dd of="$FAKE_RELEASE_DIR/$NAME.tar.gz" bs=1 seek=40 conv=notrunc 2>/dev/null; r=$(prebuilt)
expect "exit 1" [ "$r" = 1 ] && expect "said" said "doesn't match the checksum pinned" && expect "nothing installed" [ ! -e "$BIN" ] && ok

case_ "5. no pin for this version: refused before downloading"
fresh; : >"$HOME/empty.tsv"; export LLAMA_RELEASE_CHECKSUMS="$HOME/empty.tsv"; r=$(prebuilt)
expect "exit 1" [ "$r" = 1 ] && expect "said" said "has no pinned checksum" && expect "no download" not_logged "curl" && expect "nothing installed" [ ! -e "$BIN" ] && ok

case_ "6. auto mode: builds when cargo is there"
fresh; export CARGO_TARGET_DIR="$HOME/target"
r=$( (cd "$ROOT" && ./dist/install.sh </dev/null >"$HOME/out.txt" 2>&1); echo $?)
expect "exit 0" [ "$r" = 0 ] && expect "built" logged "cargo build --release --locked" && expect "no download" not_logged "curl" && ok

case_ "7. every pinned tarball is well formed and names its tag's commit"
n=0
while IFS=$'\t' read -r h a c z; do
  [[ $h == \#* || -z $a ]] && continue
  n=$((n + 1)); tag="${a#llama-}"; tag="${tag%%-*}"
  expect "hash shape ($a)" grep -qE '^[0-9a-f]{64}$' <<<"$h" &&
  expect "size pinned ($a)" grep -qE '^[0-9]+$' <<<"$z" &&
  { ! (cd "$ROOT" && git rev-parse -q --verify "$tag^{commit}" >/dev/null 2>&1) \
    || expect "commit is the tag's ($a)" [ "$c" = "$(cd "$ROOT" && git rev-parse "$tag^{commit}")" ]; }
done <"$ROOT/dist/release-checksums.tsv"
if (( n )); then ok; else echo "   ok (nothing pinned yet: dist/pin-release.sh runs after a release is published)"; fi

finish
