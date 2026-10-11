//! Discovers DeepCool LCD coolers and talks to them over USB.
//!
//! Every supported model implements [`Display`], so frontends (CLI, GUI,
//! daemon) never deal with model-specific bytes.

mod lm;
mod usb;

use std::time::Duration;

use thiserror::Error;

pub use lm::LmSeries;

/// Static facts about a supported model.
#[derive(Debug)]
pub struct ModelSpec {
    pub name: &'static str,
    pub vendor_id: u16,
    pub product_id: u16,
    pub width: u32,
    pub height: u32,
}

pub const SUPPORTED: &[ModelSpec] = &[lm::SPEC];

#[derive(Debug, Error)]
pub enum Error {
    #[error("no supported DeepCool display found")]
    NotFound,
    #[error("no supported DeepCool display with serial {0}")]
    SerialNotFound(String),
    #[error(
        "permission denied opening the USB device; install the udev rule \
         (packaging/linux/60-opencreative.rules) and re-plug the cooler"
    )]
    PermissionDenied,
    #[error("USB error: {0}")]
    Usb(#[from] std::io::Error),
    #[error("USB transfer failed: {0}")]
    Transfer(#[from] nusb::transfer::TransferError),
    #[error("USB transfer timed out after {0:?}")]
    Timeout(Duration),
    #[error("frame is {got} bytes of RGBA, expected {expected}")]
    FrameSize { expected: usize, got: usize },
}

pub type Result<T> = std::result::Result<T, Error>;

/// A connected cooler with a screen.
pub trait Display {
    fn spec(&self) -> &'static ModelSpec;
    /// Backlight brightness, 0-100.
    fn set_brightness(&mut self, percent: u8) -> Result<()>;
    /// Toggles the screen on/off.
    fn screen_toggle(&mut self) -> Result<()>;
    /// Shows a full frame of tightly packed RGBA8 pixels at the panel's native size.
    fn send_frame(&mut self, rgba: &[u8]) -> Result<()>;
}

/// A supported device found on the bus but not yet opened.
pub struct Found {
    pub spec: &'static ModelSpec,
    info: nusb::DeviceInfo,
}

impl Found {
    pub fn serial(&self) -> Option<&str> {
        self.info.serial_number()
    }

    pub fn bus(&self) -> u8 {
        self.info.bus_number()
    }

    pub fn address(&self) -> u8 {
        self.info.device_address()
    }

    pub fn open(&self) -> Result<Box<dyn Display>> {
        let iface = usb::claim(&self.info)?;
        // Only one family exists today; dispatch on spec once there are more.
        Ok(Box::new(LmSeries::new(iface)?))
    }
}

/// Lists all supported devices currently plugged in.
pub fn discover() -> Result<Vec<Found>> {
    Ok(nusb::list_devices()?
        .filter_map(|info| {
            SUPPORTED
                .iter()
                .find(|s| s.vendor_id == info.vendor_id() && s.product_id == info.product_id())
                .map(|spec| Found { spec, info })
        })
        .collect())
}

/// Opens the device with the given serial, or the first one found.
pub fn open(serial: Option<&str>) -> Result<Box<dyn Display>> {
    let found = discover()?;
    let pick = match serial {
        Some(want) => found
            .iter()
            .find(|f| f.serial() == Some(want))
            .ok_or_else(|| Error::SerialNotFound(want.to_owned()))?,
        None => found.first().ok_or(Error::NotFound)?,
    };
    pick.open()
}
