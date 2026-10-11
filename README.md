# Llama

**Winamp 2.x for Omarchy.** It really whips the llama's ass.

Llama is the classic Winamp window (main, equalizer and playlist) drawn pixel for pixel from real `.wsz` skins. That's any of the ~100,000 in the [Winamp Skin Museum](https://skins.webamp.org/). It plays everything [cliamp](https://github.com/bjarneo/cliamp) can play: local files, Spotify, Tidal, Qobuz, YouTube and YouTube Music, SoundCloud, Mixcloud, Bandcamp, Plex, Jellyfin, Emby, Navidrome, Lyrion, Audiobookshelf, podcasts and ~58,000 radio stations.

It's written in Rust and draws on the CPU into a plain Wayland surface, with no GPU, browser engine, Qt or GTK.

| | |
|---|---|
| Memory (PSS) | ~10 MB |
| First frame | ~12 ms after launch |
| Binary | ~4 MB |
| Idle CPU | ~0.1% |

## How it fits together

```
 Llama window (Rust, Wayland)    ──┐
 bar widget (QML, omarchy-shell) ──┼── cliamp IPC v2 (~/.config/cliamp/cliamp.sock) ── cliamp -d
 llama skin … (CLI)              ──┘
```

Llama is only the interface. cliamp is the engine: it does decoding, every provider, the 10-band EQ, MPRIS media keys and scrobbling. If no cliamp is running, opening Llama starts `cliamp -d`. Closing Llama stops that daemon, the same way closing Winamp stopped the music. A cliamp you started yourself is left running.

## Requirements

- Omarchy with the Quattro shell (plugins and the bar)
- [cliamp](https://github.com/bjarneo/cliamp) 2.0 or newer (Llama talks to it over IPC v2): `sudo pacman -S cliamp`
- Optional: Rust, to build Llama yourself (`sudo pacman -S --needed rustup && rustup default stable`). Without it, the installer downloads a release binary (x86_64 and aarch64).
- Already on Omarchy: Hyprland, `curl`, `jq`, fontconfig, libxkbcommon

## Install

Llama is a program plus an Omarchy bar widget. `omarchy plugin add` only fetches the widget (it never builds anything), so the program is installed with the included script.

**With the Omarchy plugin command** (how the marketplace installs it)

```bash
omarchy plugin add https://github.com/derekross/llama.git
~/.config/omarchy/plugins/derekross.llama/dist/install.sh
omarchy plugin enable derekross.llama
```

The checkout in `~/.config/omarchy/plugins/derekross.llama` *is* the bar widget, so `install.sh` copies nothing there. It builds in `~/.cache/llama/build`, not in the plugin folder: the shell reloads a plugin whenever files change in its folder, and a Rust `target/` is gigabytes of changing files.

**Or from a clone**

```bash
git clone https://github.com/derekross/llama && cd llama
./dist/install.sh
omarchy plugin enable derekross.llama
```

From a clone, `install.sh` copies the widget (`manifest.json` and `shell-plugin/`) into `~/.config/omarchy/plugins/derekross.llama`, so builds in your clone never make the shell reload. If that folder is a git checkout or a link, it's left alone.

Either way, `install.sh` adds:

- `~/.local/bin/llama`
- a launcher entry, `~/.local/share/applications/llama.desktop`
- the bar widget copy, from a clone only
- **only if you say yes:** one marked block at the end of `~/.config/hypr/hyprland.lua`, so the Llama window floats with no border, blur, shadow or transparency. The installer shows the exact block and asks (the default is no). Pass `--hypr-rule` to say yes without being asked, or `--no-hypr-rule` to not be asked. Without a terminal and without `--hypr-rule`, `hyprland.lua` is left alone and the block is printed for you to add yourself.

It records each file it writes, with its SHA-256, in `~/.local/state/llama/install.tsv`, checks every path before it changes anything, and says what it keeps (see [What it touches](#what-it-touches)). If `~/.local/bin/llama` already exists without a record (llama.cpp and ollama users often have one, and Llama 0.1.0 kept no record), it asks before replacing it, or stops in a run without a terminal; `--replace-existing` says yes. The old file is moved to `~/.local/state/llama/backup/` and never deleted.

**The binary.** With Rust installed, `install.sh` builds with `cargo build --release --locked`. Without it, it downloads the release matching the plugin's version from [GitHub Releases](https://github.com/derekross/llama/releases). Each release is built by GitHub Actions from its tag, with a build-provenance attestation. The installer accepts the download only if its size and SHA-256 match the ones pinned in the checkout itself (`dist/release-checksums.tsv`, added after each release once its attestation was verified, with the source commit the attestation names), so the bytes you run are tied to a reviewed commit rather than to whatever the release page holds today. When the GitHub CLI is signed in, it verifies the attestation again. The download is bounded (connect, total and stall timeouts, and never more than the pinned size), and a checkout without a pin for its version refuses to download and says so. Choose explicitly with `install.sh --build` or `install.sh --prebuilt`. To check a download yourself: `gh attestation verify llama-v0.2.0-x86_64-linux.tar.gz --repo derekross/llama`.

`./dist/install.sh --help` lists every option.

## Update

```bash
omarchy plugin update derekross.llama
~/.config/omarchy/plugins/derekross.llama/dist/install.sh
```

From a clone: `git pull && ./dist/install.sh`. Your skins and settings stay. The binary, launcher and widget files from any earlier Llama version are recognised by their recorded or released hashes and replaced; a widget file you edited stays, and the output says so. An older Llama block in `hyprland.lua` is only replaced if you agree again.

## Remove

```bash
~/.config/omarchy/plugins/derekross.llama/dist/uninstall.sh    # or ./dist/uninstall.sh in a clone
omarchy plugin remove derekross.llama                           # if added with omarchy plugin add
```

`uninstall.sh` closes the player, then removes the binary, the launcher entry, the widget copy and the `hyprland.lua` block, but only what it can verify Llama wrote: a `~/.local/bin/llama` without a matching record (someone else's) is neither run nor deleted, an edited widget or launcher file stays, folders keep anything you added, and a git checkout of the plugin is left for `omarchy plugin remove`. The `hyprland.lua` block is taken out only while there is exactly one start marker followed by one end marker; otherwise the file is left alone and the script explains what to fix. The file is otherwise restored byte for byte, including a missing last newline.

Your skins and settings stay. `uninstall.sh --purge` also deletes `~/.local/share/llama` (skins), the settings in `~/.local/state/llama` and `~/.cache/llama` (Museum listings, screenshots and the build folder). Backups Llama made stay even then.

## Use

**The window** works like Winamp:

- Drag the title bar to move it. Double-click the title bar to shade it. Llama remembers where you put it.
- Minimize hides the window while the music keeps playing. Right-click the bar icon (or run `llama`) to bring it back.
- The options button, the clutterbar **O** or a right-click anywhere opens the main menu: play a file or a URL, add a folder, the EQ and playlist, the skin browser, double size, window shade, show on all workspaces, the visualizer and remaining time.
- The clutterbar: **A** shows the window on every workspace, **I** shows the track's details, **D** switches double size, **V** turns the visualizer on and off.
- Click the time to toggle remaining time. Click the visualizer to turn it off.
- The EQ and PL buttons fold the equalizer and playlist in and out. EQ **ON** off bypasses the equalizer; the sliders keep shaping the curve it brings back. **PRESETS** lists cliamp's presets.
- Eject and **L** open your file chooser and play what you pick. In the playlist, **ADD** takes a URL, a folder or files; **REM** removes all, crops or removes the selection; **SEL** selects; **MISC** sorts by title or shows a track's details; **LIST** starts, saves and loads cliamp playlists. The small transport buttons under the playlist work too.
- In the playlist, click to select, <kbd>Ctrl</kbd>-click to add a row, <kbd>Shift</kbd>-click for a range, double-click to play. Drag the corner to resize.

The classic keys work: <kbd>z</kbd> <kbd>x</kbd> <kbd>c</kbd> <kbd>v</kbd> <kbd>b</kbd> for the transport, arrow keys for seek and volume, <kbd>s</kbd> shuffle, <kbd>r</kbd> repeat, <kbd>l</kbd> play file, <kbd>Shift</kbd>+<kbd>L</kbd> add folder, <kbd>Ctrl</kbd>+<kbd>L</kbd> play location, <kbd>Alt</kbd>+<kbd>G</kbd>/<kbd>Alt</kbd>+<kbd>E</kbd> for the EQ and playlist, <kbd>Ctrl</kbd>+<kbd>D</kbd> double size, <kbd>Ctrl</kbd>+<kbd>W</kbd> shade, <kbd>Ctrl</kbd>+<kbd>A</kbd> all workspaces. After a click in the playlist, the arrows, <kbd>PgUp</kbd>/<kbd>PgDn</kbd>, <kbd>Home</kbd>/<kbd>End</kbd> and <kbd>Ctrl</kbd>+<kbd>A</kbd> move and select there, <kbd>Enter</kbd> plays and <kbd>Delete</kbd> removes.

Drop a `.wsz` file anywhere on the window to wear it.

**The bar widget:**

- Left-click opens the panel. It has three tabs:
  - **Now Playing:** transport, seek, volume, shuffle and repeat.
  - **Library:** pick any configured cliamp source, then search it or browse its playlists. Click plays, right-click queues.
  - **Skins:** browse the Museum's favorites or search all ~100k skins. Click one and the running player switches to it immediately.
- Right-click opens the player.
- Middle-click plays or pauses.
- The scroll wheel changes the volume.

**The CLI:**

```bash
llama                          # open the player, or focus the one already open
llama --skin <md5|museum link|file.wsz>
llama --zoom 3                 # physical pixels per skin pixel (default 2)
llama --hide                   # minimize; `llama` or right-clicking the bar icon brings it back
llama --quit
llama skin use|base|current|featured|search|list
```

## What it touches

**Files.** `install.sh` writes `~/.local/bin/llama`, `~/.local/share/applications/llama.desktop`, the widget copy (from a clone) and its record in `~/.local/state/llama/`. With your consent it adds one marked block to `~/.config/hypr/hyprland.lua`; before every change to that file a copy goes to `~/.local/state/llama/backup/` (the first copy is kept for good, plus the newest five). `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME` and `XDG_CONFIG_HOME` are honoured. The player keeps skins in `~/.local/share/llama`, its settings and window position in `~/.local/state/llama/state.json`, and Museum listings and screenshots in `~/.cache/llama`.

**Ownership.** A path is replaced or removed only while it is a regular file (never a link or a folder) whose hash matches Llama's install record, or a file some Llama version shipped (`dist/known-hashes.tsv`, generated from the git history by `dist/gen-known-hashes.sh`). Every replacement checks the bytes it swapped out, after the swap, so an edit made in between is kept as a backup. Everything else stays and is named in the output. `install.sh` and `uninstall.sh` never use sudo.

**Network.** The player talks to the [Winamp Skin Museum](https://skins.webamp.org/): `api.webamp.org` to browse and search skins, and `r2.webampskins.org` to download a skin you pick and, on first run, the base skin. The bar widget's Skins tab shows Museum screenshots, which the shell loads from `r2.webampskins.org`. Nothing else: Llama sends no telemetry, and it controls playback only through cliamp's local socket (`~/.config/cliamp/cliamp.sock`). Music comes from cliamp, which reaches whatever sources you configured in it. The installer reaches the network only for `cargo` (crates.io) or, without Rust, the GitHub release download described above (and `gh attestation verify`, when the GitHub CLI is signed in).

**Processes.** The widget runs `~/.local/bin/llama` and `cliamp`. The player runs `cliamp -d` when no cliamp is running, `hyprctl` to place and focus its window, and `fc-match` to find the playlist font.

`tests/install/` exercises all of this against a sandboxed home on every push.

## Differences from Winamp

- **Window layout.** Hyprland can't snap separate windows together, so the three windows sit in one stacked surface, docked like Winamp's default layout.
- **Preamp and balance.** cliamp has neither, so the preamp slider is fixed at 0 dB and the balance slider is fixed at center.
- **EQ ON button.** cliamp has no EQ bypass, so switching EQ off sets cliamp's curve flat and keeps yours; switching it back on restores it.
- **Visualizer.** cliamp gives 10 spectrum bands. Llama spreads them across Winamp's 19 bars, colored from the skin's `viscolor.txt`.
- **Clutterbar A.** Hyprland floats keep above tiled windows anyway, so A pins the window to every workspace instead of keeping it on top.
- **Not available.** EQ AUTO (cliamp keeps one EQ for all tracks), the oscilloscope visualizer, and shade modes for the EQ and playlist. These controls say so in the title display.

## Development

```bash
./tests/fetch-skins.sh                           # test skins, from the Skin Museum
cargo test                                       # skin engine, UI logic, IPC framing
cargo test --test cliamp_live -- --ignored       # against a real, throwaway cliamp -d
cargo run --example render -- skin.wsz out.png   # render a skin to a PNG
tests/install/run.sh                             # install.sh/uninstall.sh against a sandboxed home
```

CI runs `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` and the installer tests.

**Releasing.** Bump the version in `manifest.json` and `Cargo.toml`, run `./dist/gen-known-hashes.sh > dist/known-hashes.tsv` (the installer's table of every widget file each version ships; CI checks it on every push), commit, then `git tag vX.Y.Z && git push github main vX.Y.Z`. The release workflow checks that the three versions match and the table covers the tag, runs the tests, builds on Ubuntu 22.04 for x86_64 and aarch64, attests the builds and publishes them. Then `./dist/pin-release.sh vX.Y.Z` (needs a signed-in GitHub CLI) downloads the tarballs, verifies their attestations and that they were built from the tag's commit, and pins their hashes in `dist/release-checksums.tsv`; commit and push that, and the no-Rust install for that version works from then on.

## Credits

- The sprite coordinates come from the classic Winamp skin format, as documented by [Webamp](https://github.com/captbaritone/webamp)'s `skinSprites.ts` (MIT, Jordan Eldredge). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), which also lists the Rust crates Llama is built from.
- [OmaAmp](https://github.com/jonnyace/omaamp) by jonnyace showed that Winamp on Omarchy, driven by cliamp, was a good idea. Llama is an independent Rust implementation of that idea.
- Skins and screenshots come from the [Winamp Skin Museum](https://skins.webamp.org/) and belong to their authors, including any skin shown in this repository's screenshots or preview image. No skin is shipped with Llama: the base skin is downloaded on first run.
- Winamp is a trademark of its owners. Llama is not affiliated with them.

MIT licensed.
