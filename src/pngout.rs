//! Minimal PNG writer for 0xRRGGBB frame buffers (nearest-neighbour upscale).

pub fn write_png(path: &std::path::Path, buf: &[u32], width: usize, height: usize, scale: usize) {
    let w = width * scale;
    let h = height * scale;
    let mut data = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let src_row = (y / scale) * width;
        for x in 0..w {
            let px = buf[src_row + x / scale];
            data.push(((px >> 16) & 0xFF) as u8);
            data.push(((px >> 8) & 0xFF) as u8);
            data.push((px & 0xFF) as u8);
        }
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("png header");
    writer.write_image_data(&data).expect("png data");
}
