//! Blocking helpers over nusb's async transfers.

use std::io::ErrorKind;
use std::time::Duration;

use futures_lite::FutureExt;
use futures_lite::future::block_on;
use nusb::Interface;

use crate::{Error, Result};

const INTERFACE: u8 = 0;

/// Linux usbfs historically caps one bulk URB at 16 KiB. Chunks are multiples
/// of the 512-byte packet size, so the device sees the same packet stream as
/// one large transfer.
const MAX_CHUNK: usize = 16 * 1024;

pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

pub fn claim(info: &nusb::DeviceInfo) -> Result<Interface> {
    let device = info.open().map_err(map_open_err)?;
    let iface = device
        .detach_and_claim_interface(INTERFACE)
        .map_err(map_open_err)?;
    Ok(iface)
}

fn map_open_err(e: std::io::Error) -> Error {
    if e.kind() == ErrorKind::PermissionDenied {
        Error::PermissionDenied
    } else {
        Error::Usb(e)
    }
}

/// Writes `data` to a bulk OUT endpoint, splitting it into URB-sized chunks.
pub fn bulk_write(iface: &Interface, endpoint: u8, data: &[u8]) -> Result<()> {
    for chunk in data.chunks(MAX_CHUNK) {
        let transfer = async {
            let done = iface.bulk_out(endpoint, chunk.to_vec()).await;
            Some(done.into_result())
        };
        let timeout = async {
            async_io::Timer::after(WRITE_TIMEOUT).await;
            None
        };
        // Dropping the losing transfer future cancels it.
        match block_on(transfer.or(timeout)) {
            Some(result) => {
                result?;
            }
            None => return Err(Error::Timeout(WRITE_TIMEOUT)),
        }
    }
    Ok(())
}
