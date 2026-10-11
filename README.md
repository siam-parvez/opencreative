# OpenCreative

[![CI](https://github.com/siam-parvez/opencreative/actions/workflows/ci.yml/badge.svg)](https://github.com/siam-parvez/opencreative/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/siam-parvez/opencreative)](https://github.com/siam-parvez/opencreative/releases)

Open-source, cross-platform control for DeepCool LCD coolers, written in Rust.
An alternative to DeepCool's Windows-only DeepCreative app.

> OpenCreative is an independent project. It is not affiliated with or endorsed
> by DeepCool. "DeepCool" and "DeepCreative" are trademarks of their owners.

<p>
  <img src="docs/screenshots/dashboard.png" alt="OpenCreative dashboard" width="49%">
  <img src="docs/screenshots/device.png" alt="OpenCreative cooler screen settings" width="49%">
</p>

## Status

Early. Linux first; Windows and macOS are planned (the USB layer, `nusb`, already
supports all three).

| Device | USB ID | Screen | Status |
|---|---|---|---|
| LM240 / LM360 | `3633:0026` | 320x240 | Supported |

Want your cooler supported? Open an issue with the output of `lsusb` and, ideally,
a USB capture of DeepCreative (see [docs/protocol](docs/protocol/lm-series.md)).

## Features

- Panels screen: one large and two small readings of your choice (CPU/GPU
  temperature, load, frequency, power, RAM), like the vendor app's digital
  mode, in a choice of skins: **Lines** (interlocking lines over a tile grid)
  and **HUD** (neon frames with live network and usage charts)
- Gauges screen: CPU temperature and load gauges, GPU temperature, RAM, clock
- Images (PNG, JPEG, WebP, BMP) and animated GIFs, with cover / contain / stretch fit
- Solid colors, brightness 0-100, screen on/off
- Rotation (0/90/180/270) for any pump orientation
- Survives cooler replug and suspend/resume
- `preview` renders screens to PNG without hardware

## Install (Linux)

Prebuilt binaries for Linux (x86_64, ARM64), Windows and macOS are attached to
each [release](https://github.com/siam-parvez/opencreative/releases).

To build from source you need a C toolchain (for Rust's linker) and Rust.

```sh
sudo apt install build-essential          # Debian/Ubuntu
curl https://sh.rustup.rs -sSf | sh       # if Rust isn't installed
cargo install --path crates/opencreative-cli
```

Allow your user to access the cooler without root:

```sh
sudo cp packaging/linux/60-opencreative.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

Run the daemon on login, so the screen is always driven and your choices
persist across reboots:

```sh
mkdir -p ~/.config/systemd/user
cp packaging/linux/opencreative.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now opencreative
```

## Usage

```sh
opencreative list                      # find connected coolers
opencreative panels                    # live readings in panels (the default screen)
opencreative panels --primary gpu-temp --secondary gpu-load --secondary ram-usage
opencreative panels --skin hud          # neon skin with live charts
opencreative monitor                   # live readings as gauges
opencreative image wallpaper.gif       # still images or looping GIFs
opencreative image logo.png --fit contain
opencreative solid '#ff8800'
opencreative brightness 60
opencreative zen on                    # calm screen: a dim clock; `zen off` returns
opencreative screen off                # backlight off; `screen on` restores it
opencreative --rotate 180 monitor      # upside-down pump
opencreative sensors                   # check which temperature sensors were found
opencreative preview monitor -o out.png
opencreative status                    # what the daemon is showing
```

### Desktop app

```sh
cargo install --path crates/opencreative-gui
opencreative-gui
```

Laid out like DeepCool's app, with a light theme (dark available):

- **Dashboard**: CPU and GPU cards with load gauges, frequency, temperature and
  power; memory, storage (read/write and per-disk usage) and network speeds;
  a two-minute history chart.
- **Devices**: coolers found on USB, with a live thumbnail of each screen.
- **Cooler screen**: brightness, Zen mode (a calm clock screen), rotation, and
  a pixel-exact live preview of what the panel shows. *Digital* mode picks a skin from a gallery of live thumbnails
  (Lines, HUD, Gauges) and which readings to show (with recommended
  combinations); *Multimedia* mode shows an image or
  GIF (Browse… or drop a file on the window) or a color. Changes apply
  immediately.
- **Settings**: start with the computer (on by default; the background service
  keeps the screen running from login), install the USB rule (asks for your
  password), add OpenCreative to your app menu, Claude usage, and the theme.

### Claude usage (optional)

With [OpenCUT](https://github.com/siam-parvez/OpenCUT)'s `ocutd` running,
turn on **Settings → Integrations → Claude usage** to see your Claude plan's
5-hour and weekly usage on the dashboard, and to pick them as cooler-screen
readings (`claude_5h`, `claude_week`, or the "Claude usage" preset).
OpenCreative only reads OpenCUT's snapshot file
(`$XDG_RUNTIME_DIR/opencut/usage.json`); it never contacts Anthropic or reads
credentials. Not affiliated with Anthropic.

The app talks to the daemon; if none is running it starts one inside the
window, so the screen only keeps updating while the window is open — Settings
hands it over to a background service.

### Daemon

`opencreative daemon` owns the cooler and keeps the chosen screen on it,
reconnecting after replugs and suspend/resume. While it runs, the commands
above are sent to it instead of opening the USB device (only one program can
hold it at a time), and every change is saved to
`~/.config/opencreative/config.toml`:

```toml
rotation = 0
brightness = 60

[scene]
type = "monitor"        # or "image" (path, fit) / "solid" (color = "#ff8800")
interval_secs = 1.0
```

Without a daemon, commands drive the device directly; `--no-daemon` forces
that. The daemon is Linux/macOS only for now (Unix socket at
`$XDG_RUNTIME_DIR/opencreative.sock`); on Windows commands always run directly.

## Architecture

```
crates/
  opencreative-protocol   pure packet encoding (no I/O), unit-tested against captures
  opencreative-device     USB discovery + transport, `Display` trait per model
  opencreative-sensors    cross-platform sensors via sysinfo
  opencreative-render     tiny-skia drawing: themes, images, GIFs, rotation
  opencreative-core       scene playback, reconnect loop, daemon engine
  opencreative-config     TOML settings
  opencreative-ipc        JSON-lines protocol between the daemon and its clients
  opencreative-daemon     the daemon (used by `opencreative daemon` and the GUI)
  opencreative-cli        the `opencreative` binary (CLI + `daemon`)
  opencreative-gui        the `opencreative-gui` desktop app (egui)
```

New devices implement `Display` in `opencreative-device`; frontends don't change.

## Roadmap

- [x] Desktop GUI MVP (screen picker, live preview, device settings) — see [plan](docs/design/gui.md)
- [x] GUI polish: file picker, sensor history, service page, app menu entry + icon
- [ ] Tray icon (needs GTK3 dev libraries on Linux; the background service covers most of the need)
- [x] Config file + background daemon with IPC so the GUI and CLI share one device
- [ ] Daemon on Windows (named pipe)
- [ ] Theme engine: user-defined layouts (TOML), custom fonts and backgrounds
- [ ] More sensors: NVIDIA GPU (NVML), fan/pump RPM, network, disk
- [ ] Windows (WinUSB) and macOS builds, CI release artifacts
- [ ] More DeepCool devices: Mystique, LQ, LP, AK/LS/LD digital series

## Credits

- Protocol captures: [daedlock/deepcool-lm](https://github.com/daedlock/deepcool-lm) (MIT)
- Fonts: [Rajdhani](https://fonts.google.com/specimen/Rajdhani) by Indian Type Foundry, [Orbitron](https://github.com/theleagueof/orbitron) by The League of Moveable Type, and [IBM Plex Sans](https://github.com/IBM/plex) by IBM (SIL OFL 1.1)

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Bundled fonts are under the SIL Open Font License: [Rajdhani](assets/fonts/OFL-Rajdhani.txt) and [Orbitron](assets/fonts/OFL-Orbitron.txt) (screen), [IBM Plex Sans](assets/fonts/OFL-IBMPlexSans.txt) (app).
