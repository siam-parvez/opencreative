# OpenCreative desktop GUI — plan

Status: draft. Based on two independent proposals (egui-first and iced-first)
written against the codebase; both reached the same architecture.

## Decision: egui (eframe)

- Pure Rust, one binary per OS, no GTK/Qt/WebView runtime — matches the
  existing nusb / tiny-skia / sysinfo stack.
- Immediate mode fits a constantly updating 320x240 preview: upload the latest
  frame into one texture per tick.
- Built-in sliders, color picker and combo boxes, plus `egui_plot` for sensor
  history.
- Costs we accept: non-native look; tray needs `tray-icon` (pulls GTK3 on
  Linux); must use `request_repaint_after` to avoid burning CPU when idle.

iced was the runner-up (nicer theming and Elm-style state); it loses on
preview ergonomics and Linux build deps for this app.

## The real risk: device ownership

Only one process can claim the cooler's USB interface. Today the systemd
`monitor` service holds it forever, so a GUI started next to it gets EBUSY
or races the service after every replug or resume. The GUI therefore depends
on a daemon that owns the device, with everything else as an IPC client.

```
opencreative-gui ─┐
opencreative CLI ─┼─ IPC (local socket, JSON lines) ─▶ opencreative daemon ─▶ USB
                  │                                     (systemd user service)
                  └─ falls back to direct USB if no daemon answers
```

## Components

| Crate | Role |
|---|---|
| `opencreative-core` (done, #9) | `Scene`, `Player`, reconnect loop |
| `opencreative-config` | TOML settings: scene, brightness, rotation, serial, interval; `config_dir()/opencreative/config.toml` |
| `opencreative-ipc` | Versioned JSON-lines protocol over `interprocess` (Unix socket in `$XDG_RUNTIME_DIR`, named pipe on Windows). Requests: `Hello{proto}`, `GetStatus`, `SetScene`, `SetBrightness`, `ScreenToggle`, `SetRotation`, `Subscribe` (status + sensor snapshot + optional preview frame at low rate) |
| `opencreative daemon` | New CLI subcommand; the systemd unit runs it. Owns the device, persists GUI/CLI changes to config, single-instance lock |
| `opencreative-gui` | eframe app; UI thread + one backend worker over channels |

## Screens

1. **Device** — discovered coolers, connection state (connected / reconnecting /
   permission denied with udev instructions), brightness, screen on/off, rotation.
2. **Screens** — gallery of scene types (System Monitor, Image/GIF, Solid) with
   thumbnails from `opencreative-render`; per-scene settings (fit, color, interval).
3. **Live preview** (always visible) — 320x240 at integer scale, nearest
   filtering, with a *device-accurate* toggle that runs the real RGB565 + Bayer
   conversion and expands back to RGB888, so banding/dither look exactly like
   the panel.
4. **Media library** — drag-drop or file picker (`rfd`), GIF frame count and
   duration, recent files.
5. **Sensors** — current readings, history plot, raw sensor table (which label matched).
6. **Service** — daemon status, enable/disable the systemd unit, install the
   udev rule, start on login, tray.

## Milestones (one PR each)

1. ~~Core extraction~~ — done (#9).
2. **Config + IPC + daemon** — `opencreative daemon`; CLI routes over IPC when a
   daemon answers, else direct USB; systemd unit switches to `daemon`; command
   changes apply immediately (wait on the command channel instead of sleeping).
   Add `pixel::rgb565_le_to_rgba` + round-trip test for the accurate preview.
3. **GUI MVP (Linux)** — Device + Screens + live preview, client of the daemon,
   standalone fallback.
4. **Polish** — media library, sensors history, service page, tray, `.desktop`
   file + icon, release artifact.
5. **Windows / macOS** — named-pipe IPC, per-OS autostart (Task Scheduler / Run
   key, LaunchAgent), WinUSB/Zadig detection and guidance, signed builds.

## Known cross-platform risks

- Windows: nusb needs WinUSB bound to the cooler's interface (Zadig or a signed
  INF); "access denied" usually means DeepCreative has it open.
- Windows/macOS sensors: `sysinfo` exposes few or no CPU temperatures without
  vendor drivers; may need LibreHardwareMonitor (Windows) / SMC keys (macOS).
- macOS: if the interface is HID-class, IOHIDFamily owns it; may need a hidapi
  transport behind the `Display` trait.
- Background service differs per OS (systemd / Task Scheduler / LaunchAgent).

## Linux build dependencies (GUI only)

`pkg-config libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev
libxrandr-dev libxi-dev libgl1-mesa-dev` (most are loaded at runtime; end users
need only standard desktop libraries).
