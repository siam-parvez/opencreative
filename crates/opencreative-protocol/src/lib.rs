//! Wire protocol for DeepCool LCD coolers.
//!
//! This crate is pure: it only builds and parses bytes. USB transport lives in
//! `opencreative-device`, so everything here can be unit-tested without hardware.
//!
//! See `docs/protocol/lm-series.md` for how the format was derived.

pub mod lm;
pub mod packet;
pub mod pixel;

pub use packet::{DecodeError, crc16_xmodem, decode, encode};
