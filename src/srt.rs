use std::path::Path;

use crate::text::sanitize_text;

pub fn parse_n_subtitles<P: AsRef<Path>>(path: P, num_subtitles: usize) -> Vec<String> {
    let data = std::fs::read_to_string(path).unwrap();
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
