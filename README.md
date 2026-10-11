# OpenCreative

[![CI](https://github.com/siam-parvez/opencreative/actions/workflows/ci.yml/badge.svg)](https://github.com/siam-parvez/opencreative/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/siam-parvez/opencreative)](https://github.com/siam-parvez/opencreative/releases)

Open-source, cross-platform control for DeepCool LCD coolers, written in Rust.
An alternative to DeepCool's Windows-only DeepCreative app.

> OpenCreative is an independent project. It is not affiliated with or endorsed
> by DeepCool. "DeepCool" and "DeepCreative" are trademarks of their owners.

## Status

Early. Linux first; Windows and macOS are planned (the USB layer, `nusb`, already
supports all three).

| Device | USB ID | Screen | Status |
|---|---|---|---|
| LM240 / LM360 | `3633:0026` | 320x240 | Supported |

Want your cooler supported? Open an issue with the output of `lsusb` and, ideally,
a USB capture of DeepCreative (see [docs/protocol](docs/protocol/lm-series.md)).

## Features

- System monitor screen: CPU temperature and load gauges, GPU temperature, RAM, clock
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

Run the monitor on login:

```sh
mkdir -p ~/.config/systemd/user
cp packaging/linux/opencreative.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now opencreative
```

## Usage

```sh
opencreative list                      # find connected coolers
opencreative monitor                   # live system monitor (Ctrl+C to stop)
opencreative image wallpaper.gif       # still images or looping GIFs
opencreative image logo.png --fit contain
opencreative solid '#ff8800'
opencreative brightness 60
opencreative screen-toggle
opencreative --rotate 180 monitor      # upside-down pump
opencreative sensors                   # check which temperature sensors were found
opencreative preview monitor -o out.png
```

## Architecture

```
crates/
  opencreative-protocol   pure packet encoding (no I/O), unit-tested against captures
  opencreative-device     USB discovery + transport, `Display` trait per model
  opencreative-sensors    cross-platform sensors via sysinfo
  opencreative-render     tiny-skia drawing: themes, images, GIFs, rotation
  opencreative-core       scene playback + reconnect loop shared by all frontends
  opencreative-cli        the `opencreative` binary
```

New devices implement `Display` in `opencreative-device`; frontends don't change.

## Roadmap

- [ ] Desktop GUI (theme picker, live preview, device settings)
- [ ] Config file + background daemon with IPC so the GUI and CLI share one device
- [ ] Theme engine: user-defined layouts (TOML), custom fonts and backgrounds
- [ ] More sensors: NVIDIA GPU (NVML), fan/pump RPM, network, disk
- [ ] Windows (WinUSB) and macOS builds, CI release artifacts
- [ ] More DeepCool devices: Mystique, LQ, LP, AK/LS/LD digital series

## Credits

- Protocol captures: [daedlock/deepcool-lm](https://github.com/daedlock/deepcool-lm) (MIT)
- Font: [Rajdhani](https://fonts.google.com/specimen/Rajdhani) by Indian Type Foundry (SIL OFL 1.1)

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
The bundled font is under the [SIL Open Font License](assets/fonts/OFL.txt).
