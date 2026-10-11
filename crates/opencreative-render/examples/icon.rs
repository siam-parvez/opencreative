fn main() {
    let path = std::env::args().nth(1).expect("output path");
    opencreative_render::icon::icon(256).save_png(path).unwrap();
}
