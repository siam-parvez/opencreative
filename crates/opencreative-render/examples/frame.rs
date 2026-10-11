//! Saves the first screen frame of a video file or web address (YouTube…):
//! `cargo run --example frame -- <file-or-url> out.png [zoom]`.
//! Needs ffmpeg (and yt-dlp for web pages).
use opencreative_render::media::{Crop, Fit, Item, Source};
use opencreative_render::video::Stream;

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("file or url");
    let out = args.next().expect("output path");
    let zoom = args.next().map_or(1.0, |z| z.parse().expect("zoom"));
    let source = if input.starts_with("http") {
        Source::Url(input)
    } else {
        Source::Path(input.into())
    };
    let item = Item {
        source,
        fit: Fit::Cover,
        crop: Crop {
            zoom,
            ..Crop::default()
        },
    };
    let mut stream = Stream::open(&item, 320, 240).unwrap_or_else(|e| panic!("{e}"));
    // Skip the first second, often black.
    let mut frame = None;
    for _ in 0..20 {
        frame = stream.next_frame();
    }
    frame.expect("a frame").save_png(out).unwrap();
}
