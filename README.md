# OpenCreative

[![CI](https://github.com/siam-parvez/opencreative/actions/workflows/ci.yml/badge.svg)](https://github.com/siam-parvez/opencreative/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/siam-parvez/opencreative)](https://github.com/siam-parvez/opencreative/releases)

Control the screen on your DeepCool LCD cooler from Linux: live system
readings, images, GIFs, videos, YouTube and music bars. A free, open-source
alternative to DeepCool's Windows-only DeepCreative app.

> OpenCreative is an independent project. It is not affiliated with or endorsed
> by DeepCool. "DeepCool" and "DeepCreative" are trademarks of their owners.

<p>
  <img src="docs/screenshots/devices.png" alt="OpenCreative devices page: the cooler pictured with its live screen" width="100%">
</p>
<p>
  <img src="docs/screenshots/dashboard.png" alt="OpenCreative dashboard" width="49%">
  <img src="docs/screenshots/device.png" alt="OpenCreative cooler screen settings" width="49%">
</p>

## Install

1. Download the file for your system from the
   [latest release](https://github.com/siam-parvez/opencreative/releases/latest):

   | Your system | Download |
   |---|---|
   | Ubuntu, Debian, Mint, Pop!_OS | `opencreative_…_amd64.deb` |
   | Fedora, openSUSE | `opencreative-…x86_64.rpm` |
   | Anything else | `OpenCreative-…-x86_64.AppImage` |

   On an ARM computer, pick the `arm64` / `aarch64` file instead.

2. Open it in your software center and click **Install**, or in a terminal:

   ```sh
   sudo apt install ./opencreative_*_amd64.deb     # Ubuntu, Debian
   sudo dnf install ./opencreative-*.x86_64.rpm    # Fedora
   ```

3. Open **OpenCreative** from your app menu.

That's it. The cooler is found automatically, and its screen keeps running in
the background from then on, with the window closed and after restarts.

**AppImage:** make it executable (right-click → Properties → allow running as
a program) and open it. In the app's **Settings**, turn on **Allow access to
the cooler** (asks for your password once) and **Show in the app menu**.

**Updating:** install the new file the same way. OpenCreative restarts on the
new version by itself and keeps your settings.

## What it does

- **Digital mode**: live readings on the cooler, one large and two small, of
  your choice (CPU or GPU temperature, load, frequency, power, RAM), in three
  looks: **Lines**, **HUD** (with live charts) and **Gauges**. One-click
  presets cover the usual combinations.
- **Multimedia mode**: playlists of **Images**, **GIFs**, **Videos** or
  **YouTube** links, each kind on its own tab, or a plain **Color**. Drop files
  on the window or use Browse. Drag the preview to move a picture and scroll to
  zoom. Select several items with Ctrl or Shift and press Delete.
- **Music mode**: bars that move with whatever your speakers or headphones are
  playing, rising or mirrored, in one color or a rainbow by pitch.
- **Display**: brightness, rotation for any pump orientation, and Zen mode (a
  calm clock).
- **Your look**: dark or light theme, and a choice of app fonts (Inter,
  Geist, Manrope, IBM Plex Sans).
- **Dashboard**: this computer's CPU, GPU, memory, storage and network at a
  glance, with a two-minute history.
- The preview in the app shows exactly what's on the cooler, frame for frame.

<p>
  <img src="docs/screenshots/skin-lines.png" alt="Lines skin: three readings framed by thin lines over a tile grid" width="32%">
  <img src="docs/screenshots/skin-hud.png" alt="HUD skin: neon frames with network and usage charts" width="32%">
  <img src="docs/screenshots/skin-gauges.png" alt="Gauges skin: CPU temperature and load gauges" width="32%">
</p>

### Optional extras

- **YouTube** needs a small helper, yt-dlp. OpenCreative downloads it for you
  the first time you add a YouTube link. Videos play through ffmpeg, which the
  packages install for you.
- **CPU power readings** are off by default: they need the CPU's energy
  counters, which are a known security side channel
  ([CVE-2020-8694](https://nvd.nist.gov/vuln/detail/CVE-2020-8694)). Turn on
  **Settings → CPU and PC power** if you want them. "PC power" only shows a
  number on computers that can measure more than the CPU.
- **Claude usage**: if you use [Claude Code](https://claude.com/claude-code),
  **Settings → Integrations → Claude usage** shows your 5-hour and weekly
  limits on the dashboard and as cooler readings. It reuses the login Claude
  Code already keeps on this computer, sends it only to Anthropic's usage
  endpoint, and never stores it. Not affiliated with Anthropic.

## Supported coolers

| Cooler | USB ID | Screen |
|---|---|---|
| DeepCool LM240 / LM360 | `3633:0026` | 320 × 240 |

Have another DeepCool cooler with a screen?
[Adding a device](docs/adding-a-device.md) explains what we need to support it.

## Troubleshooting

- **"No cooler connected"**: unplug the cooler's USB cable and plug it back
  in. If it still isn't found, use **Settings → Allow access to the cooler**.
- **YouTube says it needs yt-dlp**: click **Download yt-dlp** on the YouTube
  tab.
- **The music bars say NO AUDIO**: OpenCreative listens through PipeWire or
  PulseAudio; check that sound is playing through your normal output.
- **Anything else**:
  [open an issue](https://github.com/siam-parvez/opencreative/issues) with
  what you see and the output of `opencreative status`.

## Uninstall

- From a `.deb` or `.rpm`: remove **OpenCreative** in your software center, or
  run `sudo apt remove opencreative` / `sudo dnf remove opencreative`.
- AppImage: right-click OpenCreative in the app menu → **Uninstall
  OpenCreative**, or use **Settings → Uninstall** in the app.

<details>
<summary><b>Command line</b></summary>

Everything except the music bars is also available as `opencreative`
commands. While the background service runs, commands go to it and are
remembered.

```sh
opencreative list                      # find connected coolers
opencreative panels                    # live readings in panels (the default screen)
opencreative panels --primary gpu-temp --secondary gpu-load --secondary ram-usage
opencreative panels --skin hud         # neon skin with live charts
opencreative monitor                   # live readings as gauges
opencreative media wallpaper.gif       # images, GIFs, videos
opencreative media a.png b.gif clip.mp4 --seconds 15   # a playlist
opencreative media cat.jpg --zoom 2 --focus 0.3,0.4    # zoom in on a point
opencreative media "https://www.youtube.com/watch?v=…"
opencreative solid '#ff8800'
opencreative brightness 60
opencreative zen on                    # calm clock; `zen off` returns
opencreative screen off                # backlight off; `screen on` restores it
opencreative --rotate 180 monitor      # upside-down pump
opencreative sensors                   # which sensors were found
opencreative preview monitor -o out.png   # render to a PNG, no cooler needed
opencreative status                    # what the background service is showing
```

Settings live in `~/.config/opencreative/config.toml`, for example:

```toml
rotation = 0
brightness = 60

[scene]
type = "music"          # or "panels", "monitor", "media", "solid"
style = "mirror"        # "bars" or "mirror"
color = "#22d3ee"
rainbow = false
bars = 24
```

</details>

<details>
<summary><b>Build from source</b></summary>

You need Rust and a C toolchain:

```sh
sudo apt install build-essential          # Debian/Ubuntu
curl https://sh.rustup.rs -sSf | sh       # if Rust isn't installed
cargo install --path crates/opencreative-cli
cargo install --path crates/opencreative-gui
opencreative-gui
```

Then, in the app's **Settings**, turn on **Allow access to the cooler**,
**Show in the app menu** and **Start with the computer**.

The workspace:

```
crates/
  opencreative-protocol   packet encoding (no I/O), tested against captures
  opencreative-device     USB discovery and transport, a `Display` per model
  opencreative-sensors    system readings
  opencreative-audio      what's playing, as a spectrum (music bars)
  opencreative-render     drawing: skins, images, GIFs, videos, music bars
  opencreative-core       scene playback, reconnecting, the daemon engine
  opencreative-config     TOML settings
  opencreative-ipc        JSON-lines protocol between the daemon and its clients
  opencreative-daemon     the background service
  opencreative-cli        the `opencreative` command
  opencreative-gui        the desktop app (egui)
```

New coolers implement `Display` in `opencreative-device`; nothing else changes.

</details>

## Roadmap

- [x] Desktop app, background service, Linux packages
- [x] Images, GIFs, videos, YouTube and music bars
- [ ] RGB control for motherboard and RAM lighting, synced to music (via OpenRGB)
- [ ] Custom screen layouts, fonts and backgrounds
- [ ] More DeepCool coolers: Mystique, LQ, LP, AK/LS/LD digital series
- [ ] Windows and macOS (maybe, later)

## Credits

- Icons: [Lucide](https://lucide.dev) (ISC, `assets/icons/LICENSE-lucide.txt`).
- Device pictures in `assets/devices/` are DeepCool's product images, used to
  identify the hardware; they remain DeepCool's property.
- Protocol captures: [daedlock/deepcool-lm](https://github.com/daedlock/deepcool-lm) (MIT)
- Fonts (SIL OFL 1.1): [Rajdhani](https://fonts.google.com/specimen/Rajdhani) by Indian Type Foundry and [Orbitron](https://github.com/theleagueof/orbitron) by The League of Moveable Type on the cooler screen; [Inter](https://github.com/rsms/inter), [Geist](https://github.com/vercel/geist-font), [Manrope](https://github.com/googlefonts/manrope) and [IBM Plex Sans](https://github.com/IBM/plex) in the app.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Bundled fonts are under the SIL Open Font License: [Rajdhani](assets/fonts/OFL-Rajdhani.txt) and [Orbitron](assets/fonts/OFL-Orbitron.txt) (screen); [Inter](assets/fonts/OFL-Inter.txt), [Geist](assets/fonts/OFL-Geist.txt), [Manrope](assets/fonts/OFL-Manrope.txt) and [IBM Plex Sans](assets/fonts/OFL-IBMPlexSans.txt) (app).
