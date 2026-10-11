# DeepCool LM series USB protocol

Applies to the DeepCool LM240 / LM360 (USB `3633:0026`, product string `LM-Series`).

Status: frame, handshake, brightness and screen toggle are confirmed against
bytes captured from the vendor software (published by the MIT-licensed
[daedlock/deepcool-lm](https://github.com/daedlock/deepcool-lm), independently
cross-checked by [rifaniponk/LM360-custom-driver](https://github.com/rifaniponk/LM360-custom-driver)).
The checksum algorithm and field meanings below were derived by OpenCreative
from those captures and are pinned by unit tests in
`crates/opencreative-protocol/src/lm.rs`.

## Transport

| | |
|---|---|
| Interface | 0, class `0xFF` (vendor specific), no kernel driver |
| OUT endpoint | `0x01`, bulk, 512-byte packets (USB 2.0 high speed) |
| IN endpoint | `0x81`, bulk (responses not yet decoded) |
| Panel | 320 x 240, RGB565 little-endian, row-major from top-left |

## Packet framing

```
AA | len: u16 LE | payload[len] | crc: u16 LE
```

`crc` is **CRC-16/XMODEM** (poly `0x1021`, init `0x0000`, no reflection,
no final XOR) over the payload only. It was identified by brute-forcing CRC-16
parameters against all five captured packets; exactly one variant matched.

## Commands

| Payload | Meaning |
|---|---|
| `09` | Handshake. Sent once after claiming the interface; the vendor app then waits ~500 ms. |
| `06 03 <v> 00` | Set backlight brightness to `<v>`. Captured values: `0x1d` (29) and `0x61` (97). OpenCreative treats `<v>` as a 0-100 percentage. |
| `03 00 00 00` | Toggle the screen on/off ("Zen mode"). |
| `00 01 <len: u32 LE> <packets: u16 LE>` | Frame header. `len` = pixel bytes that follow (153600); `packets` = `len / 512` (300). Raw RGB565 data follows immediately as bulk writes, unframed. |

## Open questions

- What the device sends on `0x81` after the handshake (firmware version? model?).
- Whether brightness accepts values above 100.
- Whether `06 <sub> ...` has other sub-commands (rotation, idle timeout).
- Whether the device needs periodic frames to stay on (the vendor app resends
  continuously; a single frame has been observed to stay visible).

Contributions with USB captures (Wireshark + USBPcap on Windows running the
official DeepCreative app) are very welcome.
