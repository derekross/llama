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

## Install

You need `cliamp` and a Rust toolchain.

```bash
git clone https://github.com/derekross/llama && cd llama
./dist/install.sh
omarchy plugin enable derekross.llama
```

`install.sh` adds the following:

- `~/.local/bin/llama`
- a launcher entry
- the bar widget, copied into `~/.config/omarchy/plugins/derekross.llama`
- one marked block in `~/.config/hypr/hyprland.lua`, so the window floats with no border, blur, shadow or transparency (a backup of the file is kept)

`./dist/uninstall.sh` removes all of it. Add `--purge` to also remove your skins and settings.

## Use

**The window** works like Winamp:

- Drag the title bar to move it. Double-click the title bar to shade it.
- The EQ and PL buttons fold the equalizer and playlist in and out.
- The clutterbar **D** button switches to double size.
- Click the time to toggle remaining time. Click the visualizer to turn it off.
- In the playlist, double-click a track to play it, press Delete to remove it, and drag the corner to resize.
- ADD and eject accept a file, a folder or a URL. LIST loads and saves cliamp playlists.

The classic keys work: <kbd>z</kbd> <kbd>x</kbd> <kbd>c</kbd> <kbd>v</kbd> <kbd>b</kbd>, arrow keys for seek and volume, <kbd>s</kbd> shuffle, <kbd>r</kbd> repeat, <kbd>l</kbd> add, <kbd>g</kbd>/<kbd>e</kbd> for EQ/playlist, <kbd>Ctrl</kbd>+<kbd>D</kbd> double size, <kbd>Ctrl</kbd>+<kbd>W</kbd> shade.

Drop a `.wsz` file anywhere on the window to wear it.

**The bar widget:**

- Left-click opens the panel. It has three tabs:
  - **Now Playing:** transport, seek, volume, shuffle and repeat.
  - **Library:** pick any configured cliamp source, then search it or browse its playlists. Click plays, right-click queues.
  - **Skins:** browse the Museum's favorites or search all ~100k skins. Click one and the running player switches to it immediately.
- Middle-click opens the player.
- Right-click plays or pauses.
- The scroll wheel changes the volume.

**The CLI:**

```bash
llama                          # open the player, or focus the one already open
llama --skin <md5|museum link|file.wsz>
llama --zoom 3                 # physical pixels per skin pixel (default 2)
llama --quit
llama skin use|base|current|featured|search|list
```

## Differences from Winamp

- **Window layout.** Hyprland can't snap separate windows together, so the three windows sit in one stacked surface, docked like Winamp's default layout.
- **Preamp and balance.** cliamp has neither, so the preamp slider is fixed at 0 dB and the balance slider is fixed at center.
- **EQ ON button.** cliamp has no EQ bypass. Switching EQ off sets the curve flat, and switching it back on restores your curve.
- **Visualizer.** cliamp gives 10 spectrum bands. Llama spreads them across Winamp's 19 bars, colored from the skin's `viscolor.txt`.

## Development

```bash
./tests/fetch-skins.sh                           # test skins, from the Skin Museum
cargo test                                       # skin engine, UI logic, IPC framing
cargo test --test cliamp_live -- --ignored       # against a real, throwaway cliamp -d
cargo run --example render -- skin.wsz out.png   # render a skin to a PNG
```

## Credits

- The sprite coordinates come from the classic Winamp skin format, as documented by [Webamp](https://github.com/captbaritone/webamp)'s `skinSprites.ts`.
- [OmaAmp](https://github.com/jonnyace/omaamp) by jonnyace showed that Winamp on Omarchy, driven by cliamp, was a good idea. Llama is an independent Rust implementation of that idea.
- Skins and screenshots come from the [Winamp Skin Museum](https://skins.webamp.org/) and belong to their authors. The base skin is downloaded on first run and not shipped with Llama.
- Winamp is a trademark of its owners. Llama is not affiliated with them.

MIT licensed.
