//! The application icon, decoded once to RGBA.

const PNG: &[u8] = include_bytes!("../../../packaging/icons/open-epomaker-panel.png");

pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn rgba() -> Rgba {
    let decoder = png::Decoder::new(std::io::Cursor::new(PNG));
    let mut reader = decoder.read_info().expect("embedded icon is a valid PNG");
    let mut buf = vec![0; reader.output_buffer_size().expect("icon size")];
    let info = reader.next_frame(&mut buf).expect("embedded icon decodes");
    buf.truncate(info.buffer_size());
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        other => panic!("unexpected icon color type {other:?}"),
    };
    Rgba { width: info.width, height: info.height, pixels }
}
