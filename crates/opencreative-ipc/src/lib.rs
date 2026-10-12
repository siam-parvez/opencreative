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

use opencreative_config::{Config, SceneConfig, ScreenTheme};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{Client, Server, Stopper};

#[cfg(not(unix))]
mod unsupported;
#[cfg(not(unix))]
pub use unsupported::{Client, Server, Stopper};

/// Bumped on any incompatible change to [`Request`] or [`Response`].
pub const PROTOCOL_VERSION: u32 = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Hello {
        version: u32,
    },
    Status,
    SetScene {
        scene: SceneConfig,
    },
    SetBrightness {
        percent: u8,
    },
    SetRotation {
        degrees: u16,
    },
    SetScreen {
        on: bool,
    },
    SetZen {
        on: bool,
    },
    /// The frame on the screen, unless it's still frame `after` (0 for
    /// any), so previews show exactly what the cooler shows.
    Frame {
        after: u64,
    },
    /// Shows or hides the background service's top-bar icon.
    SetTray {
        on: bool,
    },
    /// Dark or light screens.
    SetScreenTheme {
        theme: ScreenTheme,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Hello {
        version: u32,
    },
    Status(Status),
    /// `None` when nothing newer is on the screen.
    Frame {
        frame: Option<Frame>,
    },
    Error {
        message: String,
    },
}

/// A screen frame as the panel shows it: panel orientation, 16-bit color.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    /// Degrees the content was rotated clockwise to fit the panel.
    pub rotation: u16,
    /// RGB565 little-endian pixels, base64.
    rgb565: String,
}

impl Frame {
    pub fn new(seq: u64, width: u32, height: u32, rotation: u16, rgb565: &[u8]) -> Self {
        use base64::Engine as _;
        Self {
            seq,
            width,
            height,
            rotation,
            rgb565: base64::engine::general_purpose::STANDARD.encode(rgb565),
        }
    }

    /// The RGB565 little-endian pixels, if they decode to the right size.
    pub fn rgb565(&self) -> Option<Vec<u8>> {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.rgb565)
            .ok()?;
        (bytes.len() == self.width as usize * self.height as usize * 2).then_some(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// Name of the connected device, if any.
    pub device: Option<String>,
    pub screen_on: bool,
    pub zen: bool,
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
    fn frames_survive_the_wire() {
        let pixels: Vec<u8> = (0..4 * 3 * 2).map(|i| i as u8).collect();
        let frame = Frame::new(7, 4, 3, 90, &pixels);
        let line = serde_json::to_string(&Response::Frame { frame: Some(frame) }).unwrap();
        let Response::Frame { frame: Some(back) } = serde_json::from_str(&line).unwrap() else {
            panic!("{line}");
        };
        assert_eq!(
            (back.seq, back.width, back.height, back.rotation),
            (7, 4, 3, 90)
        );
        assert_eq!(back.rgb565(), Some(pixels));
        // Pixels that don't match the size are refused.
        assert_eq!(Frame::new(1, 5, 5, 0, &[0; 4]).rgb565(), None);
    }

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
            zen: false,
            config: Config::default(),
        });
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&json).unwrap(), status);
    }
}
