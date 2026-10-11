//! Commands for the DeepCool LM series (LM240 / LM360), USB `3633:0026`.
//!
//! The panel is 320x240, RGB565 little-endian, fed over a vendor-specific bulk
//! endpoint. Byte values come from captures published by the MIT-licensed
//! `daedlock/deepcool-lm` project; the field meanings are our interpretation,
//! and the tests below pin our encoder to those captured bytes.

use crate::packet::encode;

pub const VENDOR_ID: u16 = 0x3633;
pub const PRODUCT_ID: u16 = 0x0026;
pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 240;
pub const BYTES_PER_PIXEL: usize = 2;
pub const FRAME_BYTES: usize = WIDTH as usize * HEIGHT as usize * BYTES_PER_PIXEL;
pub const EP_OUT: u8 = 0x01;
pub const EP_IN: u8 = 0x81;
/// High-speed bulk max packet size; the frame header counts pixel data in these.
pub const USB_PACKET: usize = 512;

/// Brightness values seen in captures were 29 and 97, so the device accepts
/// at least that range. We expose 0..=100 as a percentage.
pub const MAX_BRIGHTNESS: u8 = 100;

/// Command opcodes (first payload byte).
mod op {
    pub const FRAME: u8 = 0x00;
    pub const SCREEN_TOGGLE: u8 = 0x03;
    pub const SETTING: u8 = 0x06;
    pub const HANDSHAKE: u8 = 0x09;
}

/// Sub-command of [`op::SETTING`] that sets backlight brightness.
const SETTING_BRIGHTNESS: u8 = 0x03;

/// Sent once after opening the device, before any frame.
pub fn handshake() -> Vec<u8> {
    encode(&[op::HANDSHAKE])
}

/// Sets backlight brightness, clamped to `0..=MAX_BRIGHTNESS`.
pub fn set_brightness(percent: u8) -> Vec<u8> {
    encode(&[
        op::SETTING,
        SETTING_BRIGHTNESS,
        percent.min(MAX_BRIGHTNESS),
        0x00,
    ])
}

/// Toggles the screen on/off ("Zen mode" in the vendor app).
pub fn screen_toggle() -> Vec<u8> {
    encode(&[op::SCREEN_TOGGLE, 0x00, 0x00, 0x00])
}

/// Header announcing `len` bytes of raw pixel data that follow it.
///
/// Payload: `00 01 | len: u32 LE | packet count: u16 LE`, where the packet
/// count is `len` measured in 512-byte USB packets.
pub fn frame_header(len: usize) -> Vec<u8> {
    let len32 = u32::try_from(len).expect("frame too large");
    let packets = u16::try_from(len.div_ceil(USB_PACKET)).expect("frame too large");
    let mut payload = vec![op::FRAME, 0x01];
    payload.extend_from_slice(&len32.to_le_bytes());
    payload.extend_from_slice(&packets.to_le_bytes());
    encode(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from the vendor software by daedlock/deepcool-lm.
    #[test]
    fn matches_captured_bytes() {
        assert_eq!(
            frame_header(FRAME_BYTES),
            [
                0xaa, 0x08, 0x00, 0x00, 0x01, 0x00, 0x58, 0x02, 0x00, 0x2c, 0x01, 0xbc, 0x11
            ]
        );
        assert_eq!(handshake(), [0xaa, 0x01, 0x00, 0x09, 0x29, 0x91]);
        assert_eq!(
            set_brightness(0x61),
            [0xaa, 0x04, 0x00, 0x06, 0x03, 0x61, 0x00, 0xd2, 0x46]
        );
        assert_eq!(
            set_brightness(0x1d),
            [0xaa, 0x04, 0x00, 0x06, 0x03, 0x1d, 0x00, 0xe6, 0x0b]
        );
        assert_eq!(
            screen_toggle(),
            [0xaa, 0x04, 0x00, 0x03, 0x00, 0x00, 0x00, 0xdc, 0x9b]
        );
    }

    #[test]
    fn brightness_is_clamped() {
        assert_eq!(set_brightness(255), set_brightness(MAX_BRIGHTNESS));
    }
}
