use std::{convert::TryInto, fs::File, io::Read, path::Path};

use bindings::Windows::{
    Globalization::Language, Graphics::Imaging::SoftwareBitmap, Media::Ocr::OcrEngine,
};
use webm_iterable::{
    matroska_spec::{Block, EbmlSpecification, MatroskaSpec},
    tags::{TagData, TagPosition},
    WebmIterator,
};

use crate::{pgs, text::sanitize_text};

#[derive(Debug, PartialEq, Clone)]
pub enum KnownLanguage {
    English,
    Unknown(String),
}

impl KnownLanguage {
    pub fn from_tag(tag: &str) -> KnownLanguage {
        match tag {
            "en" | "eng" | "en-US" => KnownLanguage::English,
            _ => KnownLanguage::Unknown(tag.to_owned()),
        }
    }

    pub fn create_winrt_language(&self) -> windows::Result<Option<Language>> {
        match self {
            KnownLanguage::English => Ok(Some(Language::CreateLanguage("en-US")?)),
            _ => Ok(None),
        }
    }

    pub fn to_string(&self) -> &str {
        match self {
            KnownLanguage::English => "English",
            KnownLanguage::Unknown(value) => value.as_str(),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum KnownEncoding {
    PGS,
    VOB,
    Unknown(String),
}

impl KnownEncoding {
    pub fn from_tag(tag: &str) -> KnownEncoding {
        match tag {
            "S_HDMV/PGS" => KnownEncoding::PGS,
            "S_VOBSUB" => KnownEncoding::VOB,
            _ => KnownEncoding::Unknown(tag.to_owned()),
        }
    }

    pub fn to_string(&self) -> &str {
        match self {
            KnownEncoding::PGS => "S_HDMV/PGS",
            KnownEncoding::VOB => "S_VOBSUB",
            KnownEncoding::Unknown(value) => value.as_str(),
        }
    }
}

#[derive(Clone)]
pub struct TrackInfo {
    pub track_number: u64,
    pub encoding: KnownEncoding,
    pub language: KnownLanguage,
}

pub struct MkvFile<R: Read> {
    mkv_iter: WebmIterator<R>,
    track_infos: Vec<TrackInfo>,
}

impl<R: Read> MkvFile<R> {
    pub fn new(source: R) -> Self {
        let mut mkv_iter = WebmIterator::new(source, &[MatroskaSpec::TrackEntry]);
        let mut track_infos = Vec::new();
        // Read until we hit a Tracks tag. Technically this isn't
        // correct, as tracks can be described at any time. However,
        // the files we care about won't do that.
        for tag in &mut mkv_iter {
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
                                    let mut language: Option<String> = None;
                                    let mut encoding: Option<String> = None;
                                    for (id, data) in children {
                                        if let Some((mkv_tag, _)) = MatroskaSpec::get_tag(*id) {
                                            match mkv_tag {
                                                MatroskaSpec::TrackNumber => {
                                                    if let TagData::UnsignedInt(value) = &data {
                                                        track_number = Some(*value);
                                                    }
                                                }
                                                MatroskaSpec::Language => {
                                                    // If language has a value, it must have been
                                                    // from an IETF tag. That means we should ignore
                                                    // this tag.
                                                    if language.is_none() {
                                                        if let TagData::Utf8(value) = &data {
                                                            language = Some(value.clone());
                                                        }
                                                    }
                                                }
                                                MatroskaSpec::LanguageIETF => {
                                                    if let TagData::Utf8(value) = &data {
                                                        language = Some(value.clone());
                                                    }
                                                }
                                                MatroskaSpec::CodecId => {
                                                    if let TagData::Utf8(value) = &data {
                                                        encoding = Some(value.clone());
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    if let Some(track_number) = track_number {
                                        if let Some(language) = language {
                                            let language = KnownLanguage::from_tag(&language);
                                            if let Some(encoding) = encoding {
                                                let encoding = KnownEncoding::from_tag(&encoding);
                                                let track_info = TrackInfo {
                                                    track_number,
                                                    encoding,
                                                    language,
                                                };
                                                track_infos.push(track_info);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    MatroskaSpec::Tracks => {
                        if !track_infos.is_empty() {
                            break;
                        }
                    }
                    _ => {
                        // Skip the tag
                    }
                }
            }
        }

        Self {
            mkv_iter,
            track_infos,
        }
    }

    pub fn tracks(&self) -> &Vec<TrackInfo> {
        &self.track_infos
    }

    pub fn subtitle_iter(
        self,
        language: KnownLanguage,
    ) -> windows::Result<Option<SubtitleIterator<R>>> {
        // Find a suitable track
        let mut track = None;
        for track_info in &self.track_infos {
            if track_info.language == language {
                track = Some(track_info.clone());
            }
        }
        if let Some(track) = track {
            self.subtitle_iter_from_track_info(track)
        } else {
            Ok(None)
        }
    }

    pub fn subtitle_iter_from_track_number(
        self,
        track_number: u64,
    ) -> windows::Result<Option<SubtitleIterator<R>>> {
        // Find a suitable track
        let mut track = None;
        for track_info in &self.track_infos {
            if track_info.track_number == track_number {
                track = Some(track_info.clone());
            }
        }
        if let Some(track) = track {
            self.subtitle_iter_from_track_info(track)
        } else {
            Ok(None)
        }
    }

    fn subtitle_iter_from_track_info(
        self,
        track_info: TrackInfo,
    ) -> windows::Result<Option<SubtitleIterator<R>>> {
        let track_number = track_info.track_number;
        match track_info.encoding {
            KnownEncoding::PGS => {
                let subtitle_iter = SubtitleIterator {
                    track_info,
                    block_iter: BlockIterator::from_webm(track_number, self.mkv_iter),
                };
                Ok(Some(subtitle_iter))
            }
            _ => Ok(None),
        }
    }

    pub fn block_iter(
        self,
        language: KnownLanguage,
    ) -> windows::Result<Option<BlockIterator<R>>> {
        // Find a suitable track
        let mut track = None;
        for track_info in &self.track_infos {
            if track_info.language == language {
                track = Some(track_info.clone());
            }
        }
        if let Some(track) = track {
            Ok(Some(self.block_iter_from_track_info(track)?))
        } else {
            Ok(None)
        }
    }

    fn block_iter_from_track_info(
        self,
        track_info: TrackInfo,
    ) -> windows::Result<BlockIterator<R>> {
        let track_number = track_info.track_number;
        Ok(BlockIterator::from_webm(track_number, self.mkv_iter))
    }
}

pub struct BlockIterator<R: Read> {
    track_number: u64,
    mkv_iter: WebmIterator<R>,
}

impl<R: Read> BlockIterator<R> {
    pub fn from_webm(track_number: u64, mkv_iter: WebmIterator<R>) -> Self {
        Self {
            track_number,
            mkv_iter,
        }
    }
}

impl<R: Read> Iterator for BlockIterator<R> {
    type Item = Block;

    fn next(&mut self) -> Option<Self::Item> {
        for tag in &mut self.mkv_iter {
            let tag = tag.as_ref().unwrap();
            if let Some(spec_tag) = &tag.spec_tag {
                match spec_tag {
                    MatroskaSpec::Block | MatroskaSpec::SimpleBlock => {
                        if let TagPosition::FullTag(_id, tag) = tag.tag.clone() {
                            let block: Block = tag.try_into().unwrap();
                            if block.track == self.track_number {
                                return Some(block)
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

pub struct SubtitleIterator<R: Read> {
    track_info: TrackInfo,
    block_iter: BlockIterator<R>,
}

impl<R: Read> Iterator for SubtitleIterator<R> {
    type Item = SoftwareBitmap;

    fn next(&mut self) -> Option<Self::Item> {
        for block in &mut self.block_iter {
            assert_eq!(block.track, self.track_info.track_number);
            let bitmap = decode_bitmap(&block, &self.track_info).unwrap();
            if bitmap.is_some() {
                return bitmap;
            }
        }
        None
    }
}

pub fn decode_bitmap(block: &Block, track_info: &TrackInfo) -> windows::Result<Option<SoftwareBitmap>> {
    // We don't handle lacing
    assert_eq!(block.lacing, None);

    let bitmap = match track_info.encoding {
        KnownEncoding::PGS => {
            pgs::parse_segments(&block.payload)?
        }
        _ => None,
    };
    Ok(bitmap)
}

pub fn load_first_n_english_subtitles<P: AsRef<Path>>(
    path: P,
    num_subtitles: usize,
    track_number: Option<u64>,
) -> windows::Result<Option<Vec<String>>> {
    load_first_n_subtitles(path, num_subtitles, track_number, KnownLanguage::English)
}

pub fn load_first_n_subtitles<P: AsRef<Path>>(
    path: P,
    num_subtitles: usize,
    track_number: Option<u64>,
    language: KnownLanguage,
) -> windows::Result<Option<Vec<String>>> {
    let winrt_language = language.create_winrt_language()?.unwrap();

    let file = File::open(&path).unwrap();
    let file = MkvFile::new(file);
    let iter = if let Some(track_number) = track_number {
        file.subtitle_iter_from_track_number(track_number)?
    } else {
        file.subtitle_iter(language)?
    };

    let engine = OcrEngine::TryCreateFromLanguage(winrt_language)?;
    if let Some(mut iter) = iter {
        let subtitles = get_first_n_subtitles(&mut iter, &engine, num_subtitles)?;
        Ok(Some(subtitles))
    } else {
        Ok(None)
    }
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

#[cfg(test)]
mod tests {
    use std::{fs::File, path::Path};

    use super::{KnownLanguage, MkvFile};

    #[test]
    fn experiment() -> windows::Result<()> {
        let path = r#"output/title_t00.mkv"#;
        let file = File::open(path).unwrap();
        let mkv = MkvFile::new(file);
        let block_iter = mkv.block_iter(KnownLanguage::English)?.unwrap();
        let mut path = Path::new("output/vob/something").to_owned();
        for (i, block) in block_iter.enumerate() {
            path.set_file_name(&format!("{}.bin", i));
            std::fs::write(&path, block.payload).unwrap();
        }
        Ok(())
    }
}