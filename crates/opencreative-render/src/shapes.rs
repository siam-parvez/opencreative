//! Path helpers shared by themes and the icon.

use tiny_skia::{Path, PathBuilder};

/// A `w` x `h` rectangle at (`x`, `y`) with corner radius `r`.
pub(crate) fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let (x1, y1) = (x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x1 - r, y);
    pb.quad_to(x1, y, x1, y + r);
    pb.line_to(x1, y1 - r);
    pb.quad_to(x1, y1, x1 - r, y1);
    pb.line_to(x + r, y1);
    pb.quad_to(x, y1, x, y1 - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}
