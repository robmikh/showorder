use std::{convert::TryInto, fs::File, io::Read, path::Path};

use bindings::Windows::{
    Globalization::Language, Graphics::Imaging::SoftwareBitmap, Media::Ocr::OcrEngine,
};
use webm_iterable::{
    matroska_spec::{Block, EbmlSpecification, MatroskaSpec},
    tags::{TagData, TagPosition},
    WebmIterator,
};

use crate::{pgs::parse_segments, text::sanitize_text};

struct SubtitleIterator<R: Read> {
    track_number: u64,
    mkv_iter: WebmIterator<R>,
}

impl<R: Read> SubtitleIterator<R> {
    pub fn new(source: R, language: &str, language_ietf: &str) -> windows::Result<Option<Self>> {
        let mut mkv_iter = WebmIterator::new(source, &[MatroskaSpec::TrackEntry]);

        if let Some(track_number) =
            find_subtitle_track_number_for_language(&mut mkv_iter, language, language_ietf)
        {
            Ok(Some(Self {
                track_number,
                mkv_iter,
            }))
        } else {
            Ok(None)
        }
    }
}

impl<R: Read> Iterator for SubtitleIterator<R> {
    type Item = SoftwareBitmap;

    fn next(&mut self) -> Option<Self::Item> {
        for tag in &mut self.mkv_iter {
            let tag = tag.as_ref().unwrap();
            if let Some(spec_tag) = &tag.spec_tag {
                match spec_tag {
                    MatroskaSpec::Block | MatroskaSpec::SimpleBlock => {
                        if let TagPosition::FullTag(_id, tag) = tag.tag.clone() {
                            let block: Block = tag.try_into().unwrap();
                            if block.track == self.track_number {
                                // We don't handle lacing
                                assert_eq!(block.lacing, None);
                                // Decode our bitmap
                                let bitmap = parse_segments(&block.payload).unwrap();
                                if let Some(bitmap) = bitmap {
                                    return Some(bitmap);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }
}

pub fn load_first_n_english_subtitles<P: AsRef<Path>>(
    path: P,
    num_subtitles: usize,
) -> windows::Result<Option<Vec<String>>> {
    let mut file = File::open(&path).unwrap();
    let iter = SubtitleIterator::new(&mut file, "eng", "en")?;

    let engine = OcrEngine::TryCreateFromLanguage(Language::CreateLanguage("en-US")?)?;
    if let Some(mut iter) = iter {
        let subtitles = get_first_n_subtitles(&mut iter, &engine, num_subtitles)?;
        Ok(Some(subtitles))
    } else {
        Ok(None)
    }
}

fn find_subtitle_track_number_for_language<R: Read>(
    iter: &mut WebmIterator<R>,
    language: &str,
    language_ietf: &str,
) -> Option<u64> {
    for tag in iter {
        let tag = tag.as_ref().unwrap();
        if let Some(spec_tag) = &tag.spec_tag {
            match spec_tag {
                MatroskaSpec::TrackEntry => {
                    if let TagPosition::FullTag(_id, data) = &tag.tag {
                        if let TagData::Master(children) = data {
                            let is_subtitle_track = |tag: &(u64, TagData)| {
                                if MatroskaSpec::get_tag_id(&MatroskaSpec::TrackType) == tag.0 {
                                    if let TagData::UnsignedInt(value) = tag.1 {
                                        return value == 0x11;
                                    }
                                }
                                false
                            };

                            if children.iter().any(is_subtitle_track) {
                                let mut track_number: Option<u64> = None;
                                let mut language_matches = false;
                                let mut pgs_track = false;
                                for (id, data) in children {
                                    if let Some((mkv_tag, _)) = MatroskaSpec::get_tag(*id) {
                                        match mkv_tag {
                                            MatroskaSpec::TrackNumber => {
                                                if let TagData::UnsignedInt(value) = &data {
                                                    track_number = Some(*value);
                                                }
                                            }
                                            MatroskaSpec::Language => {
                                                if let TagData::Utf8(value) = &data {
                                                    if value == language {
                                                        language_matches = true;
                                                    }
                                                }
                                            }
                                            MatroskaSpec::LanguageIETF => {
                                                if let TagData::Utf8(value) = &data {
                                                    if value == language_ietf {
                                                        language_matches = true;
                                                    }
                                                }
                                            }
                                            MatroskaSpec::CodecId => {
                                                if let TagData::Utf8(value) = &data {
                                                    if value == "S_HDMV/PGS" {
                                                        pgs_track = true;
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }

                                if language_matches && pgs_track {
                                    return track_number;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn get_first_n_subtitles<R: Read>(
    iter: &mut SubtitleIterator<R>,
    engine: &OcrEngine,
    num_subtitles: usize,
) -> windows::Result<Vec<String>> {
    let mut subtitles = Vec::new();
    for bitmap in iter {
        let text = process_bitmap(&bitmap, engine)?;
        if let Some(text) = text {
            subtitles.push(text.to_string());
            if subtitles.len() >= num_subtitles {
                break;
            }
        }
    }
    Ok(subtitles)
}

fn process_bitmap(bitmap: &SoftwareBitmap, engine: &OcrEngine) -> windows::Result<Option<String>> {
    // Decode our bitmap
    let result = engine.RecognizeAsync(bitmap)?.get()?;
    let text = result.Text()?.to_string();
    let text = text.trim();

    // Skip empty subtitles
    if !text.is_empty() {
        let text = sanitize_text(&text);
        if !text.is_empty() {
            return Ok(Some(text));
        }
    }
    Ok(None)
}
