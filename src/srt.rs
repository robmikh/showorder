use std::{path::Path, time::Duration};

use crate::text::sanitize_text;

pub fn parse_n_subtitles<P: AsRef<Path>>(path: P, num_subtitles: usize, skip_secs: Option<u64>) -> Vec<String> {
    let path = path.as_ref();
    let raw_data =
        std::fs::read(path).expect(&format!("Could not read from \"{}\"", path.display()));
    let data = String::from_utf8_lossy(&raw_data);
    let data = data.replace("\r\n", "\n");
    let chunks = data.split("\n\n");

    let skip_secs = skip_secs.map(|x| Duration::from_secs(x));

    let mut subtitles = Vec::new();
    for chunk in chunks {
        if !chunk.is_empty() {
            let mut parts = chunk.splitn(3, "\n");
            let _id = parts.next().unwrap();
            let timestamp_text = parts.next().unwrap();
            if let Some(skip_secs) = skip_secs {
                let mut chunks = timestamp_text.split("-->");
                let end_timestamp = chunks.nth(1).unwrap().trim();
                let mut chunks = end_timestamp.split(':');
                let hours: u64 = chunks.next().unwrap().parse().unwrap();
                let minutes: u64 = chunks.next().unwrap().parse().unwrap();
                let last_bit_text = chunks.next().unwrap();
                let mut chunks = last_bit_text.split(',');
                let seconds: u64 = chunks.next().unwrap().parse().unwrap();
                let milliseconds: u64 = chunks.next().unwrap().parse().unwrap();

                let hours_to_secs = hours * 60 * 60;
                let minutes_to_secs = minutes * 60;
                let total_seconds = hours_to_secs + minutes_to_secs + seconds;
                let timestamp = Duration::from_secs(total_seconds) + Duration::from_millis(milliseconds);

                if timestamp < skip_secs {
                    continue;
                }
            }
            let text = parts.next().unwrap().replace("\n", " ");
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
