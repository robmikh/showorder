use std::{convert::TryInto, fs::File, io::Read, path::Path};

use bindings::Windows::{
    Globalization::Language, Graphics::Imaging::SoftwareBitmap, Media::Ocr::OcrEngine,
};
use webm_iterable::{
    matroska_spec::{Block, BlockLacing, MatroskaSpec, MatroskaTag},
    tags::{DataTag, DataTagType, EbmlTag, TagSpec},
    WebmIterator,
};

use crate::{pgs::parse_segments, text::sanitize_text};

struct SubtitleIterator<'a> {
    track_number: u64,
    mkv_iter: WebmIterator<'a>,
}

impl<'a> SubtitleIterator<'a> {
    pub fn new(source: &'a mut dyn Read, language: &str) -> windows::Result<Option<Self>> {
        let spec = MatroskaSpec {};
        let mut mkv_iter = WebmIterator::new(source, &[MatroskaTag::TrackEntry]);

        if let Some(track_number) =
            find_subtitle_track_number_for_language(&mut mkv_iter, &spec, language)
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

impl<'a> Iterator for SubtitleIterator<'a> {
    type Item = SoftwareBitmap;

    fn next(&mut self) -> Option<Self::Item> {
        for tag in &mut self.mkv_iter {
            let tag = tag.as_ref().unwrap();
            match &tag.spec_type {
                MatroskaTag::Block | MatroskaTag::SimpleBlock => {
                    if let EbmlTag::FullTag(tag) = tag.tag.clone() {
                        let block: Block = tag.data_type.try_into().unwrap();
                        if block.track == self.track_number {
                            // We don't handle lacing
                            assert_eq!(block.lacing, BlockLacing::None);
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
        None
    }
}

pub fn load_first_n_english_subtitles<P: AsRef<Path>>(
    path: P,
    num_subtitles: usize,
) -> windows::Result<Option<Vec<String>>> {
    let mut file = File::open(&path).unwrap();
    let iter = SubtitleIterator::new(&mut file, "eng")?;

    let engine = OcrEngine::TryCreateFromLanguage(Language::CreateLanguage("en-US")?)?;
    if let Some(mut iter) = iter {
        let subtitles = get_first_n_subtitles(&mut iter, &engine, num_subtitles)?;
        Ok(Some(subtitles))
    } else {
        Ok(None)
    }
}

fn find_subtitle_track_number_for_language(
    iter: &mut WebmIterator,
    spec: &MatroskaSpec,
    language: &str,
) -> Option<u64> {
    for tag in iter {
        let tag = tag.as_ref().unwrap();
        match &tag.spec_type {
            MatroskaTag::TrackEntry => {
                if let EbmlTag::FullTag(data) = &tag.tag {
                    if let DataTagType::Master(children) = &data.data_type {
                        let is_subtitle_track = |tag: &DataTag| {
                            if spec.get_tag(tag.id) == MatroskaTag::TrackType {
                                if let DataTagType::UnsignedInt(value) = tag.data_type {
                                    return value == 0x11;
                                }
                            }
                            false
                        };

                        if children.iter().any(is_subtitle_track) {
                            if let Some(track_number) = children
                                .iter()
                                .find(|c| spec.get_tag(c.id) == MatroskaTag::TrackNumber)
                            {
                                if let DataTagType::UnsignedInt(track_number) =
                                    track_number.data_type
                                {
                                    if let Some(tag) = children
                                        .iter()
                                        .find(|c| spec.get_tag(c.id) == MatroskaTag::Language)
                                    {
                                        if let DataTagType::Utf8(value) = &tag.data_type {
                                            if value == language {
                                                return Some(track_number);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn get_first_n_subtitles(
    iter: &mut SubtitleIterator,
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
