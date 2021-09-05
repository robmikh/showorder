use std::path::Path;

use crate::text::sanitize_text;

pub fn parse_n_subtitles<P: AsRef<Path>>(path: P, num_subtitles: usize) -> Vec<String> {
    let path = path.as_ref();
    let raw_data =
        std::fs::read(path).expect(&format!("Could not read from \"{}\"", path.display()));
    let data = String::from_utf8_lossy(&raw_data);
    let data = data.replace("\r\n", "\n");
    let chunks = data.split("\n\n");

    let mut subtitles = Vec::new();
    for chunk in chunks {
        if !chunk.is_empty() {
            let mut parts = chunk.splitn(3, "\n");
            let text = parts.nth(2).unwrap().replace("\n", " ");
            let text = sanitize_text(&text);
            if !text.is_empty() {
                subtitles.push(text);
                if subtitles.len() >= num_subtitles {
                    break;
                }
            }
        }
    }
    subtitles
}
