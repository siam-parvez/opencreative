//! Videos and web videos, decoded by `ffmpeg` into raw frames already
//! cropped and scaled for the screen. Web pages (YouTube and the like) are
//! downloaded by `yt-dlp` and piped into ffmpeg as they play, so yt-dlp
//! deals with the site and ffmpeg never touches the network.
//!
//! Both tools are found on `PATH` or in OpenCreative's own tools folder
//! (`~/.local/share/opencreative/bin`), where the app can put `yt-dlp`.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use thiserror::Error;
use tiny_skia::{IntSize, Pixmap};

use crate::media::{Crop, Fit, Item, Source};

/// Frames per second sent to the screen. The panel takes about this many
/// full frames per second over USB.
pub const FPS: u32 = 20;
/// Decoded frames buffered ahead of the screen.
const BUFFER: usize = 4;
/// `yt-dlp` format: at most 480p (the screen is 320x240), no AV1 (slow to
/// decode), falling back to whatever single file is offered.
const YTDLP_FORMAT: &str = "bv*[height<=480][vcodec!^=av01]/b[height<=480]/bv*/b";

#[derive(Debug, Error)]
pub enum VideoError {
    #[error("playing videos needs ffmpeg: install it with `sudo apt install ffmpeg`")]
    NoFfmpeg,
    #[error("couldn't run {tool}: {source}")]
    Spawn {
        tool: &'static str,
        source: std::io::Error,
    },
    #[error(
        "YouTube and other web links need yt-dlp: use \"Download yt-dlp\" on the \
         Cooler screen page"
    )]
    NoYtdlp,
    /// With yt-dlp's own explanation when it gave one.
    #[error("{}", match reason {
        Some(r) => format!("the site wouldn't send the video: {r}"),
        None => "the video has no frames, or the site wouldn't send it".to_owned(),
    })]
    NoFrames { reason: Option<String> },
}

/// Whether `url` is a web page (YouTube and the like) rather than a direct
/// link to a video file, so it needs yt-dlp.
pub fn needs_ytdlp(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    !crate::media::is_video_path(Path::new(path))
}

/// Where OpenCreative keeps tools it downloads itself.
pub fn tools_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("opencreative").join("bin"))
}

/// A helper program: on `PATH`, else in [`tools_dir`].
pub fn tool(name: &str) -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join(name));
    on_path
        .chain(tools_dir().map(|d| d.join(name)))
        .find(|p| p.is_file())
}

/// A running decode, yielding screen-sized frames until the video ends.
pub struct Stream {
    /// ffmpeg, and yt-dlp feeding it for web pages.
    children: Vec<Child>,
    frames: Receiver<Pixmap>,
    /// yt-dlp's last error line, if it reported one.
    complaint: Arc<Mutex<Option<String>>>,
}

impl Stream {
    /// Starts decoding `item` for a `width` x `height` screen.
    pub fn open(item: &Item, width: u32, height: u32) -> Result<Self, VideoError> {
        let ffmpeg = tool("ffmpeg").ok_or(VideoError::NoFfmpeg)?;
        let mut children = Vec::new();
        let complaint = Arc::new(Mutex::new(None));
        let (input, stdin) = match &item.source {
            Source::Path(path) => (path.display().to_string(), Stdio::null()),
            Source::Url(url) => match tool("yt-dlp") {
                Some(ytdlp) => {
                    let mut download = Command::new(ytdlp)
                        .args([
                            "--no-playlist",
                            "--no-warnings",
                            "--quiet",
                            "-f",
                            YTDLP_FORMAT,
                        ])
                        .args(["-o", "-", url])
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                        .map_err(|source| VideoError::Spawn {
                            tool: "yt-dlp",
                            source,
                        })?;
                    let pipe = Stdio::from(download.stdout.take().expect("piped"));
                    let stderr = download.stderr.take().expect("piped");
                    let sink = Arc::clone(&complaint);
                    // Keep the reason a site refused, for the error message.
                    let _ = thread::Builder::new()
                        .name("yt-dlp-errors".into())
                        .spawn(move || {
                            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                                if let Some(msg) = line.strip_prefix("ERROR: ") {
                                    *sink.lock().expect("lock") = Some(msg.trim().to_owned());
                                }
                            }
                        });
                    children.push(download);
                    ("pipe:0".to_owned(), pipe)
                }
                None if needs_ytdlp(url) => return Err(VideoError::NoYtdlp),
                // A direct link to a video file: ffmpeg reads it itself.
                None => (url.clone(), Stdio::null()),
            },
        };
        let filter = filter(item.fit, item.crop, (width, height));
        let mut child = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-i",
                &input,
            ])
            .args(["-an", "-vf", &filter])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| VideoError::Spawn {
                tool: "ffmpeg",
                source,
            })?;
        let mut stdout = child.stdout.take().expect("piped");
        let (tx, frames) = mpsc::sync_channel(BUFFER);
        let size = IntSize::from_wh(width, height).expect("non-zero size");
        thread::Builder::new()
            .name("video-decode".into())
            .spawn(move || {
                let mut buf = vec![0u8; (width * height * 4) as usize];
                while stdout.read_exact(&mut buf).is_ok() {
                    let Some(pixmap) = Pixmap::from_vec(buf.clone(), size) else {
                        break;
                    };
                    // The screen has gone away: stop decoding.
                    if tx.send(pixmap).is_err() {
                        break;
                    }
                }
            })
            .map_err(|source| VideoError::Spawn {
                tool: "decoder thread",
                source,
            })?;
        children.push(child);
        Ok(Self {
            children,
            frames,
            complaint,
        })
    }

    /// Why no frames came: yt-dlp's complaint, if it made one.
    pub fn failure(&self) -> VideoError {
        VideoError::NoFrames {
            reason: self.complaint.lock().expect("lock").clone(),
        }
    }

    /// The next frame, waiting for the decoder; `None` once the video ends.
    pub fn next_frame(&mut self) -> Option<Pixmap> {
        self.frames.recv_timeout(Duration::from_secs(20)).ok()
    }

    /// How long each frame stays on screen.
    pub fn frame_time() -> Duration {
        Duration::from_secs(1) / FPS
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The ffmpeg filter: frame rate, the zoom window, then the fit. The window
/// is worked out by ffmpeg from the video's own size (`iw`, `ih`), the same
/// way [`Crop::window`] does it for images.
fn filter(fit: Fit, crop: Crop, (w, h): (u32, u32)) -> String {
    let zoom = crop.zoom.clamp(1.0, Crop::MAX_ZOOM);
    let aspect = w as f32 / h as f32;
    let (cw, ch) = if fit == Fit::Cover {
        (
            format!("min(iw\\,ih*{aspect})/{zoom}"),
            format!("min(ih\\,iw/{aspect})/{zoom}"),
        )
    } else {
        (format!("iw/{zoom}"), format!("ih/{zoom}"))
    };
    let (fx, fy) = (crop.x.clamp(0.0, 1.0), crop.y.clamp(0.0, 1.0));
    let x = format!("clip({fx}*iw-ow/2\\,0\\,iw-ow)");
    let y = format!("clip({fy}*ih-oh/2\\,0\\,ih-oh)");
    let scaled = match fit {
        Fit::Cover | Fit::Stretch => format!("scale={w}:{h}"),
        Fit::Contain => format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:black"
        ),
    };
    format!("fps={FPS},crop={cw}:{ch}:{x}:{y},{scaled},setsar=1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_pages_need_ytdlp_but_file_links_dont() {
        assert!(needs_ytdlp("https://www.youtube.com/watch?v=jNQXAC9IVRw"));
        assert!(needs_ytdlp("https://youtu.be/jNQXAC9IVRw"));
        assert!(!needs_ytdlp("https://example.com/clip.mp4?token=1"));
        assert!(!needs_ytdlp("https://example.com/a/b.WEBM"));
    }

    #[test]
    fn filters_crop_then_fit() {
        let cover = filter(Fit::Cover, Crop::default(), (320, 240));
        assert!(
            cover.starts_with("fps=20,crop=min(iw\\,ih*1.3333334)/1:"),
            "{cover}"
        );
        assert!(cover.ends_with(",scale=320:240,setsar=1"), "{cover}");
        let contain = filter(
            Fit::Contain,
            Crop {
                zoom: 2.0,
                x: 0.25,
                y: 0.5,
            },
            (320, 240),
        );
        assert!(
            contain.contains("crop=iw/2:ih/2:clip(0.25*iw-ow/2"),
            "{contain}"
        );
        assert!(contain.contains("pad=320:240"), "{contain}");
    }
}
