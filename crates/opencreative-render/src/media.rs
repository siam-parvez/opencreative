//! User-supplied images, animated GIFs and (through ffmpeg) videos, with
//! zoom and crop, played one after another as a playlist.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::imageops::{self, FilterType};
use image::{AnimationDecoder, DynamicImage, ImageFormat, ImageResult, Rgba, RgbaImage};
use tiny_skia::{IntSize, Pixmap};

/// How an image is scaled onto the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Fill the screen, cropping overflow.
    #[default]
    Cover,
    /// Show the whole image, letterboxed in black.
    Contain,
    /// Ignore aspect ratio.
    Stretch,
}

/// A zoomed-in window on the source: `zoom` 1 shows everything the fit
/// allows; 2 shows half the width and height around the focus point `x`,
/// `y` (fractions of the source, 0.5 is the middle).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crop {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            x: 0.5,
            y: 0.5,
        }
    }
}

impl Crop {
    pub const MAX_ZOOM: f32 = 8.0;

    /// The source rectangle `(x, y, w, h)` to show from a `src_w` x `src_h`
    /// image on a `width` x `height` screen. With `cover`, the window has
    /// the screen's shape; otherwise the source's.
    pub fn window(
        self,
        src_w: u32,
        src_h: u32,
        width: u32,
        height: u32,
        cover: bool,
    ) -> (u32, u32, u32, u32) {
        let (sw, sh) = (src_w as f32, src_h as f32);
        let (mut w, mut h) = if cover {
            let target = width as f32 / height as f32;
            if sw / sh > target {
                (sh * target, sh)
            } else {
                (sw, sw / target)
            }
        } else {
            (sw, sh)
        };
        let zoom = self.zoom.clamp(1.0, Self::MAX_ZOOM);
        w = (w / zoom).max(1.0);
        h = (h / zoom).max(1.0);
        let x = (self.x.clamp(0.0, 1.0) * sw - w / 2.0).clamp(0.0, sw - w);
        let y = (self.y.clamp(0.0, 1.0) * sh - h / 2.0).clamp(0.0, sh - h);
        (
            x.round() as u32,
            y.round() as u32,
            (w.round() as u32).clamp(1, src_w),
            (h.round() as u32).clamp(1, src_h),
        )
    }
}

/// Where a playlist item comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Path(std::path::PathBuf),
    /// A web address: a YouTube (or other `yt-dlp`-supported) page, or a
    /// direct link to a video file.
    Url(String),
}

/// One playlist entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub source: Source,
    pub fit: Fit,
    pub crop: Crop,
}

impl Item {
    /// Videos and web addresses play through ffmpeg; images and GIFs are
    /// decoded in-process.
    pub fn is_video(&self) -> bool {
        match &self.source {
            Source::Url(_) => true,
            Source::Path(path) => is_video_path(path),
        }
    }
}

/// File extensions treated as video.
pub const VIDEO_EXTENSIONS: [&str; 7] = ["mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"];

pub fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// One frame of an image or animation, already scaled to the target size.
pub struct Frame {
    pub pixmap: Pixmap,
    /// Zero for still images.
    pub delay: Duration,
}

/// Browsers treat GIF delays under 20 ms as 100 ms; so do we.
const MIN_GIF_DELAY: Duration = Duration::from_millis(20);
const DEFAULT_GIF_DELAY: Duration = Duration::from_millis(100);

/// Loads an image or GIF and scales every frame to `width` x `height`.
pub fn load(path: &Path, width: u32, height: u32, fit: Fit, crop: Crop) -> ImageResult<Vec<Frame>> {
    if ImageFormat::from_path(path).ok() == Some(ImageFormat::Gif) {
        let decoder = GifDecoder::new(BufReader::new(File::open(path)?))?;
        return decoder
            .into_frames()
            .map(|frame| {
                let frame = frame?;
                let (num, den) = frame.delay().numer_denom_ms();
                let delay = Duration::from_millis(u64::from(num / den.max(1)));
                let delay = if delay < MIN_GIF_DELAY {
                    DEFAULT_GIF_DELAY
                } else {
                    delay
                };
                Ok(Frame {
                    pixmap: to_pixmap(&scale(frame.into_buffer(), width, height, fit, crop)),
                    delay,
                })
            })
            .collect();
    }

    let img = image::open(path)?.to_rgba8();
    Ok(vec![Frame {
        pixmap: to_pixmap(&scale(img, width, height, fit, crop)),
        delay: Duration::ZERO,
    }])
}

/// Crops to the zoom window, then fits to `width` x `height`.
pub fn scale(img: RgbaImage, width: u32, height: u32, fit: Fit, crop: Crop) -> RgbaImage {
    scale_with(img, width, height, fit, crop, FilterType::Lanczos3)
}

/// The same with a choice of filter (video uses a cheaper one).
pub fn scale_with(
    img: RgbaImage,
    width: u32,
    height: u32,
    fit: Fit,
    crop: Crop,
    filter: FilterType,
) -> RgbaImage {
    let img = if crop == Crop::default() {
        img
    } else {
        let (x, y, w, h) = crop.window(img.width(), img.height(), width, height, fit == Fit::Cover);
        imageops::crop_imm(&img, x, y, w, h).to_image()
    };
    match fit {
        Fit::Stretch => imageops::resize(&img, width, height, filter),
        Fit::Cover => DynamicImage::ImageRgba8(img)
            .resize_to_fill(width, height, filter)
            .to_rgba8(),
        Fit::Contain => {
            let inner = DynamicImage::ImageRgba8(img)
                .resize(width, height, filter)
                .to_rgba8();
            let mut canvas = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));
            let x = (width - inner.width()) / 2;
            let y = (height - inner.height()) / 2;
            imageops::overlay(&mut canvas, &inner, i64::from(x), i64::from(y));
            canvas
        }
    }
}

/// Flattens transparency onto black and wraps the pixels in an opaque pixmap.
fn to_pixmap(img: &RgbaImage) -> Pixmap {
    let mut data = img.as_raw().clone();
    for px in data.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        for c in &mut px[..3] {
            *c = (u16::from(*c) * a / 255) as u8;
        }
        px[3] = 255;
    }
    let size = IntSize::from_wh(img.width(), img.height()).expect("non-zero size");
    Pixmap::from_vec(data, size).expect("buffer matches size")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contain_letterboxes_wide_image() {
        let img = RgbaImage::from_pixel(40, 10, Rgba([255, 255, 255, 255]));
        let out = scale(img, 20, 20, Fit::Contain, Crop::default());
        assert_eq!(out.dimensions(), (20, 20));
        assert_eq!(out.get_pixel(10, 0), &Rgba([0, 0, 0, 255]));
        assert_eq!(out.get_pixel(10, 10), &Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn zoom_windows_stay_inside_the_source() {
        let crop = Crop {
            zoom: 2.0,
            x: 0.5,
            y: 0.5,
        };
        // A 4:3 window from a 16:9 source, halved and centered.
        assert_eq!(crop.window(1600, 900, 320, 240, true), (500, 225, 600, 450));
        // A focus point at the edge slides the window back inside.
        let corner = Crop {
            x: 1.0,
            y: 0.0,
            ..crop
        };
        assert_eq!(
            corner.window(1600, 900, 320, 240, true),
            (1000, 0, 600, 450)
        );
        // Without cover, the window keeps the source's shape.
        assert_eq!(
            crop.window(1600, 900, 320, 240, false),
            (400, 225, 800, 450)
        );
        assert_eq!(
            Crop::default().window(1600, 900, 320, 240, false),
            (0, 0, 1600, 900)
        );
    }

    #[test]
    fn videos_and_urls_are_told_from_images() {
        let item = |source| Item {
            source,
            fit: Fit::Cover,
            crop: Crop::default(),
        };
        assert!(item(Source::Path("clip.MP4".into())).is_video());
        assert!(!item(Source::Path("cat.gif".into())).is_video());
        assert!(item(Source::Url("https://youtu.be/x".into())).is_video());
    }

    #[test]
    fn transparency_flattens_to_black() {
        let img = RgbaImage::from_pixel(1, 1, Rgba([200, 100, 50, 0]));
        assert_eq!(to_pixmap(&img).data(), &[0, 0, 0, 255]);
    }
}
