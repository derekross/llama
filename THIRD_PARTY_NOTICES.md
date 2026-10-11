# Third-party notices

Llama itself is MIT licensed (see [LICENSE](LICENSE)). It builds on the work
below. Nothing here is a skin: Llama ships no skins.

## Webamp: the skin sprite coordinates

The sprite rectangles in `src/skin/sprites.rs` follow the classic Winamp 2.x
skin format as documented by [Webamp](https://github.com/captbaritone/webamp)
(`skinSprites.ts`). The values are written out for Llama, but they come from
that work, so its license applies to them:

```
The MIT License (MIT)

Copyright (c) [2015] [Jordan Eldredge]

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

(From `LICENSE.txt` in captbaritone/webamp, commit
`fd58eab2d41049c8b5afa853bc9c81c622b5c34e`.)

## Skins and the Winamp Skin Museum

Skins are not part of Llama and are not shipped with it. The base skin
(Winamp 2.91) is downloaded from the
[Winamp Skin Museum](https://skins.webamp.org/) on first run; every other skin
is one you choose, from the Museum or from a file. Skins, the Museum's
screenshots and the skins shown in this repository's screenshots belong to
their authors. The test suite downloads a few Museum skins
(`tests/fetch-skins.sh`) for testing only; they are not in the repository.

## Winamp

Winamp is a trademark of its owners. Llama is an independent project and is
not affiliated with, endorsed by or sponsored by them. "It really whips the
llama's ass" is quoted in tribute.

## Inspiration

[OmaAmp](https://github.com/jonnyace/omaamp) by jonnyace first showed Winamp
on Omarchy driven by cliamp. Llama shares no code with it.

## Rust crates

The `llama` binary statically links the crates in `Cargo.lock` that its
dependencies pull in on Linux (125 crates at 0.1.0). All are under permissive
licenses; each crate's license text is in its source package on crates.io.
Regenerate this summary with
`cargo metadata --format-version 1 --locked --filter-platform x86_64-unknown-linux-gnu`.

| License (as declared) | Crates |
|---|---|
| MIT OR Apache-2.0 (incl. `Apache-2.0 OR MIT`, `MIT/Apache-2.0`) | 56, among them serde, serde_json, anyhow, log, env_logger, libc, image, ureq, bitflags, memmap2 |
| MIT | 22: smithay-client-toolkit, wayland-client, wayland-backend, wayland-protocols (+ -wlr, -misc, -experimental), wayland-scanner, wayland-sys, wayland-cursor, wayland-csd-frame, calloop, calloop-wayland-source, xkbcommon, xcursor, zip, quick-xml, simd-adler32, synstructure, tracing, tracing-core, zmij |
| Apache-2.0 | 4: ab_glyph, ab_glyph_rasterizer, owned_ttf_parser, zopfli |
| Unicode-3.0 | 18: the ICU4X crates (icu_*, zerovec, zerotrie, zerofrom, yoke, litemap, tinystr, writeable, potential_utf) used by URL handling |
| Unlicense OR MIT | 5: memchr, aho-corasick, jiff, jiff-core, byteorder-lite |
| Zlib OR Apache-2.0 OR MIT | 6: bytemuck, bytemuck_derive, miniz_oxide (2 versions), cursor-icon, xkeysym |
| Zlib | 1: zlib-rs |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 2: rustix, linux-raw-sys |
| BSD-3-Clause OR Apache-2.0 | 2: moxcms, pxfm |
| BSD-3-Clause | 1: subtle |
| ISC | 2: rustls-webpki, untrusted |
| Apache-2.0 AND ISC | 1: ring |
| Apache-2.0 OR ISC OR MIT | 1: rustls |
| CDLA-Permissive-2.0 | 2: webpki-roots (2 versions; Mozilla's CA certificate list, used for HTTPS) |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 1: unicode-ident |
| 0BSD OR MIT OR Apache-2.0 | 1: adler2 |

At run time Llama also uses, without linking them: `libxkbcommon` (MIT, the
system library, linked dynamically), fontconfig's `fc-match` to find the
playlist font, `hyprctl`, and [cliamp](https://github.com/bjarneo/cliamp)
(MIT) as its engine.
