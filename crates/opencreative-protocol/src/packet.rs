//! Packet framing shared by DeepCool LCD devices.
//!
//! ```text
//! +------+-------------+-----------------+-------------+
//! | 0xAA | len: u16 LE | payload (len B) | crc: u16 LE |
//! +------+-------------+-----------------+-------------+
//! ```
//!
//! The checksum is CRC-16/XMODEM (poly 0x1021, init 0, no reflection) computed
//! over the payload only.

use thiserror::Error;

/// First byte of every packet.
pub const SYNC: u8 = 0xAA;

/// Bytes added around a payload: sync + length + crc.
pub const OVERHEAD: usize = 1 + 2 + 2;

/// CRC-16/XMODEM.
pub fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Wraps a payload in a framed packet.
///
/// # Panics
/// If the payload is longer than `u16::MAX` bytes. Payloads are a few bytes in
/// practice; pixel data is sent raw after a frame header, not through `encode`.
pub fn encode(payload: &[u8]) -> Vec<u8> {
    let len = u16::try_from(payload.len()).expect("payload too large for a packet");
    let mut out = Vec::with_capacity(payload.len() + OVERHEAD);
    out.push(SYNC);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&crc16_xmodem(payload).to_le_bytes());
    out
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("packet too short ({0} bytes)")]
    TooShort(usize),
    #[error("bad sync byte 0x{0:02x}")]
    BadSync(u8),
    #[error("length field says {declared} bytes but {available} are available")]
    Truncated { declared: usize, available: usize },
    #[error("crc mismatch: expected 0x{expected:04x}, got 0x{actual:04x}")]
    BadCrc { expected: u16, actual: u16 },
}

/// Validates a framed packet and returns its payload.
pub fn decode(buf: &[u8]) -> Result<&[u8], DecodeError> {
    if buf.len() < OVERHEAD {
        return Err(DecodeError::TooShort(buf.len()));
    }
    if buf[0] != SYNC {
        return Err(DecodeError::BadSync(buf[0]));
    }
    let len = usize::from(u16::from_le_bytes([buf[1], buf[2]]));
    if buf.len() < len + OVERHEAD {
        return Err(DecodeError::Truncated {
            declared: len,
            available: buf.len() - OVERHEAD,
        });
    }
    let payload = &buf[3..3 + len];
    let actual = u16::from_le_bytes([buf[3 + len], buf[4 + len]]);
    let expected = crc16_xmodem(payload);
    if actual != expected {
        return Err(DecodeError::BadCrc { expected, actual });
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_check_value() {
        // Standard CRC-16/XMODEM check value.
        assert_eq!(crc16_xmodem(b"123456789"), 0x31C3);
    }

    #[test]
    fn roundtrip() {
        let pkt = encode(&[1, 2, 3]);
        assert_eq!(decode(&pkt), Ok(&[1u8, 2, 3][..]));
    }

    #[test]
    fn rejects_corruption() {
        let mut pkt = encode(&[1, 2, 3]);
        pkt[4] ^= 0xFF;
        assert!(matches!(decode(&pkt), Err(DecodeError::BadCrc { .. })));
        assert_eq!(decode(&[0x55, 0, 0, 0, 0]), Err(DecodeError::BadSync(0x55)));
        assert_eq!(decode(&[0xAA]), Err(DecodeError::TooShort(1)));
    }
}
