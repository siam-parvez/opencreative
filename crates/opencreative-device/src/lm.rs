//! DeepCool LM series (LM240 / LM360).

use std::thread;
use std::time::Duration;

use nusb::Interface;
use opencreative_protocol::{lm, pixel};

use crate::{Display, Error, ModelSpec, Result, usb};

pub const SPEC: ModelSpec = ModelSpec {
    name: "DeepCool LM Series",
    vendor_id: lm::VENDOR_ID,
    product_id: lm::PRODUCT_ID,
    width: lm::WIDTH,
    height: lm::HEIGHT,
};

/// The vendor app waits after the handshake before streaming frames.
const HANDSHAKE_SETTLE: Duration = Duration::from_millis(500);

const RGBA_FRAME_BYTES: usize = lm::WIDTH as usize * lm::HEIGHT as usize * 4;

pub struct LmSeries {
    ep: usb::BulkOut,
    header: Vec<u8>,
}

impl LmSeries {
    pub(crate) fn new(iface: Interface) -> Result<Self> {
        // The endpoint holds the interface open, so `iface` can be dropped.
        let mut ep = usb::bulk_out(&iface, lm::EP_OUT)?;
        usb::bulk_write(&mut ep, &lm::handshake())?;
        thread::sleep(HANDSHAKE_SETTLE);
        Ok(Self {
            ep,
            header: lm::frame_header(lm::FRAME_BYTES),
        })
    }
}

impl Display for LmSeries {
    fn spec(&self) -> &'static ModelSpec {
        &SPEC
    }

    fn set_brightness(&mut self, percent: u8) -> Result<()> {
        usb::bulk_write(&mut self.ep, &lm::set_brightness(percent))
    }

    fn screen_toggle(&mut self) -> Result<()> {
        usb::bulk_write(&mut self.ep, &lm::screen_toggle())
    }

    fn send_frame(&mut self, rgba: &[u8]) -> Result<()> {
        if rgba.len() != RGBA_FRAME_BYTES {
            return Err(Error::FrameSize {
                expected: RGBA_FRAME_BYTES,
                got: rgba.len(),
            });
        }
        usb::bulk_write(&mut self.ep, &self.header)?;
        usb::bulk_write(&mut self.ep, &pixel::rgba_to_rgb565_le(rgba))
    }
}
