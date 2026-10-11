//! Blocking helpers over nusb's endpoint API.

use std::time::Duration;

use nusb::transfer::{Buffer, Bulk, Out, TransferError};
use nusb::{Endpoint, ErrorKind, Interface, MaybeFuture};

use crate::{Error, Result};

const INTERFACE: u8 = 0;

/// Linux usbfs historically caps one bulk URB at 16 KiB. Chunks are multiples
/// of the 512-byte packet size, so the device sees the same packet stream as
/// one large transfer.
const MAX_CHUNK: usize = 16 * 1024;

pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long to wait for a cancelled transfer to be handed back. If it never
/// is, the write still fails with a timeout; callers then drop the device,
/// which releases the endpoint, rather than writing to it again.
const CANCEL_WAIT: Duration = Duration::from_secs(5);

/// A claimed bulk OUT endpoint. It keeps its interface (and device) open.
pub type BulkOut = Endpoint<Bulk, Out>;

/// Opens the device and claims interface 0, detaching a kernel driver first
/// on Linux (a plain claim elsewhere).
pub fn claim(info: &nusb::DeviceInfo) -> Result<Interface> {
    let device = info.open().wait().map_err(map_err)?;
    device
        .detach_and_claim_interface(INTERFACE)
        .wait()
        .map_err(map_err)
}

/// Claims the bulk OUT endpoint at `address`. An endpoint can only be claimed
/// once per interface, so callers keep the result.
pub fn bulk_out(iface: &Interface, address: u8) -> Result<BulkOut> {
    iface.endpoint::<Bulk, Out>(address).map_err(map_err)
}

fn map_err(e: nusb::Error) -> Error {
    if e.kind() == ErrorKind::PermissionDenied {
        Error::PermissionDenied
    } else {
        Error::Usb(e)
    }
}

/// Writes `data` to a bulk OUT endpoint, splitting it into URB-sized chunks.
pub fn bulk_write(ep: &mut BulkOut, data: &[u8]) -> Result<()> {
    for chunk in data.chunks(MAX_CHUNK) {
        ep.submit(Buffer::from(chunk));
        let completion = match ep.wait_next_complete(WRITE_TIMEOUT) {
            Some(completion) => completion,
            None => {
                // Timing out doesn't cancel the transfer. Cancel it and wait
                // for it to come back so nothing stays queued on the endpoint.
                ep.cancel_all();
                match ep.wait_next_complete(CANCEL_WAIT).map(|c| c.status) {
                    // It finished in the race with the cancel.
                    Some(Ok(())) => continue,
                    Some(Err(TransferError::Cancelled)) | None => {
                        return Err(Error::Timeout(WRITE_TIMEOUT));
                    }
                    Some(Err(e)) => return Err(Error::Transfer(e)),
                }
            }
        };
        completion.status?;
    }
    Ok(())
}
