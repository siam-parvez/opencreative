//! User-supplied images and animated GIFs.

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
pub fn load(path: &Path, width: u32, height: u32, fit: Fit) -> ImageResult<Vec<Frame>> {
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
                    pixmap: to_pixmap(&scale(frame.into_buffer(), width, height, fit)),
                    delay,
                })
            })
            .collect();
    }

    let img = image::open(path)?.to_rgba8();
    Ok(vec![Frame {
        pixmap: to_pixmap(&scale(img, width, height, fit)),
        delay: Duration::ZERO,
    }])
}

fn scale(img: RgbaImage, width: u32, height: u32, fit: Fit) -> RgbaImage {
    let filter = FilterType::Lanczos3;
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
        let out = scale(img, 20, 20, Fit::Contain);
        assert_eq!(out.dimensions(), (20, 20));
        assert_eq!(out.get_pixel(10, 0), &Rgba([0, 0, 0, 255]));
        assert_eq!(out.get_pixel(10, 10), &Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn transparency_flattens_to_black() {
        let img = RgbaImage::from_pixel(1, 1, Rgba([200, 100, 50, 0]));
        assert_eq!(to_pixmap(&img).data(), &[0, 0, 0, 255]);
    }
}
