//! Local IPC between the daemon, which owns the USB device, and its clients
//! (CLI, GUI).
//!
//! Each message is one JSON object per line, and every request gets exactly
//! one response:
//!
//! ```text
//! -> {"cmd":"hello","version":1}
//! <- {"result":"hello","version":1}
//! -> {"cmd":"set_brightness","percent":60}
//! <- {"result":"ok"}
//! ```
//!
//! Unix sockets only for now; on other platforms there is never a daemon and
//! clients use the device directly.

use std::io;
use std::path::PathBuf;

use opencreative_config::{Config, SceneConfig};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{Client, Server};

#[cfg(not(unix))]
mod unsupported;
#[cfg(not(unix))]
pub use unsupported::{Client, Server};

/// Bumped on any incompatible change to [`Request`] or [`Response`].
pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Hello { version: u32 },
    Status,
    SetScene { scene: SceneConfig },
    SetBrightness { percent: u8 },
    SetRotation { degrees: u16 },
    SetScreen { on: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Hello { version: u32 },
    Status(Status),
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// Name of the connected device, if any.
    pub device: Option<String>,
    pub screen_on: bool,
    pub config: Config,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("IPC I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("malformed IPC message: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "daemon speaks protocol v{daemon}, this client v{PROTOCOL_VERSION}; restart the daemon"
    )]
    Version { daemon: u32 },
    #[error("unexpected response from daemon: {0:?}")]
    Unexpected(Box<Response>),
    #[error("daemon closed the connection")]
    Closed,
    #[error("another daemon is already listening on {0}")]
    AlreadyRunning(PathBuf),
    #[error("the daemon is not supported on this platform yet")]
    Unsupported,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Socket location: the per-user runtime directory where there is one
/// (`$XDG_RUNTIME_DIR` on Linux), else the temp directory.
pub fn default_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("opencreative.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_stable() {
        let req = Request::SetBrightness { percent: 60 };
        assert_eq!(
            serde_json::to_string(&req).unwrap(),
            r#"{"cmd":"set_brightness","percent":60}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::Ok).unwrap(),
            r#"{"result":"ok"}"#
        );
        let status = Response::Status(Status {
            device: Some("LM".into()),
            screen_on: true,
            config: Config::default(),
        });
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&json).unwrap(), status);
    }
}
