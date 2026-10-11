#!/usr/bin/env bash
# Downloads the skins the unit tests load into tests/skins/. They belong to
# their authors, so they come from the Winamp Skin Museum instead of this repo.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p skins
while read -r md5 name; do
  out="skins/$name.wsz"
  if [[ -f $out ]] && [[ $(md5sum "$out" | cut -c1-32) == "$md5" ]]; then
    continue
  fi
  curl -fsSL "https://r2.webampskins.org/skins/$md5.wsz" -o "$out.part"
  if [[ $(md5sum "$out.part" | cut -c1-32) != "$md5" ]]; then
    rm -f "$out.part"
    echo "checksum mismatch for $name" >&2
    exit 1
  fi
  mv "$out.part" "$out"
  echo "fetched $name"
done <<'LIST'
2f245e741b6bd4e1a122a28489f0c7b7 base-2.91
4308a2fc648033bf5fe7c4d56a5c8823 Green-Dimension-V2
88dbd4e043795c98625462a908a2d965 MacOSXAqua1-5
11f6cf0624da77b1bbe1d48aaef470c2 TopazAmp1-2
0985438a88963f6803311ed99b17f7f4 Vizor1-01
46ab165b574d97e2e51b22cb47a7661c XMMS-Turquoise
LIST
