use std::{convert::TryInto, fs::File, io::Read, path::Path};

use bindings::Windows::{Globalization::Language, Graphics::Imaging::SoftwareBitmap, Media::Ocr::OcrEngine, UI::Color};
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
    VOB {
        width: u32,
        height: u32,
        palette: Vec<Color>,
    },
    Unknown(String),
}

impl KnownEncoding {
    pub fn from_tag_and_data(tag: &str, data: Option<&[u8]>) -> KnownEncoding {
        match tag {
            "S_HDMV/PGS" => KnownEncoding::PGS,
            "S_VOBSUB" => {
                if let Some(data) = data {
                    let idx_string = String::from_utf8_lossy(data);
                    let mut lines = idx_string.lines();
                    let first_line = lines.nth(0).unwrap();
                    if first_line != r#"# VobSub index file, v7 (do not modify this line!)"# {
                        println!("Warning! Expected to see the VobSub v7 line at the beginning of the private data...");
                    }
                    let mut size = None;
                    let mut palette = None;
                    for line in lines {
                        // Skip comments
                        if line.starts_with("#") {
                            continue;
                        }

                        // Split the line on the first ':'
                        if let Some((name, value)) = line.split_once(':') {
                            let value = value.trim();
                            match name {
                                "size" => {
                                    let (width_str, height_str) = value.split_once('x').unwrap();
                                    let width = u32::from_str_radix(width_str, 10).unwrap();
                                    let height = u32::from_str_radix(height_str, 10).unwrap();
                                    size = Some((width, height));
                                }
                                "palette" => {
                                    let mut colors = Vec::new();
                                    let color_strs = value.split(", ");
                                    for color_str in color_strs {
                                        assert_eq!(color_str.len(), 6);
                                        // Not sure what the format is, assuming RGB for now
                                        let r_str = &color_str[0..2];
                                        let g_str = &color_str[2..4];
                                        let b_str = &color_str[4..6];

                                        let r = u8::from_str_radix(r_str, 16).unwrap();
                                        let g = u8::from_str_radix(g_str, 16).unwrap();
                                        let b = u8::from_str_radix(b_str, 16).unwrap();

                                        let color = Color {
                                            A: 255,
                                            R: r,
                                            G: g,
                                            B: b,
                                        };
                                        colors.push(color);
                                    }
                                    palette = Some(colors);
                                }
                                _ => {}
                            }
                        }
                    }

                    let (width, height) = size.expect("Expected size in Vob subtitle track private data");
                    let palette = palette.expect("Expected palette in Vob subtitle track private data");

                    KnownEncoding::VOB {
                        width,
                        height,
                        palette
                    }
                } else {
                    panic!("Expected private data for VOB subtitles!");
                }
            },
            _ => KnownEncoding::Unknown(tag.to_owned()),
        }
    }

    pub fn to_string(&self) -> &str {
        match self {
            KnownEncoding::PGS => "S_HDMV/PGS",
            KnownEncoding::VOB{..} => "S_VOBSUB",
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
                                    let mut private_data: Option<&[u8]> = None;
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
                                                MatroskaSpec::CodecPrivate => {
                                                    // VOB subtitles will have the idx file in the
                                                    // private data according to the mkv spec.
                                                    if let TagData::Binary(value) = &data {
                                                        private_data = Some(value);
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
                                                let encoding = KnownEncoding::from_tag_and_data(&encoding, private_data);
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
    use std::{fs::File, io::{Cursor, Read}, path::Path};

    use bindings::Windows::UI::Color;
    use byteorder::{BigEndian, ReadBytesExt};

    use crate::mkv::KnownEncoding;

    use super::{KnownLanguage, MkvFile};

    fn parse_two_u12(data: &[u8]) -> (u16, u16) {
        let v1_p1 = (data[0] as u16) << 8;
        let v1_p2 = (data[1] >> 4) as u16;
        let v1 = v1_p1 | v1_p2;
        let v2_p1 = (((data[1] << 4) >> 4) as u16) << 8;
        let v2_p2 = data[2] as u16;
        let v2 = v2_p1 | v2_p2;
        (v1, v2)
    }

    fn compute_size(x1: u16, x2: u16, y1: u16, y2: u16) -> (u16, u16) {
        let width = x2 - x1 + 1;
        let height = y2 - y1 + 1;
        (width, height)
    }

    #[test]
    fn parse_u12_test() {
        let dummy_data = [ 0x00u8, 0x02, 0xcf, 0x00, 0x22, 0x3e];
        let (x1, x2) = parse_two_u12(&dummy_data[0..3]);
        let (y1, y2) = parse_two_u12(&dummy_data[3..]);
        assert_eq!(x1, 0x000);
        assert_eq!(x2, 0x2cf);
        assert_eq!(y1, 0x002);
        assert_eq!(y2, 0x23e);
        println!("x1: {:X} x2: {:X} y1: {:X}, y2: {:X}", x1, x2, y1, y2);
        let (width, height) = compute_size(x1, x2, y1, y2);
        println!("size: {:X} x {:X}", width, height);
    }

    #[test]
    fn experiment() -> windows::Result<()> {
        let path = r#"output/title_t00.mkv"#;
        let file = File::open(path).unwrap();
        let mkv = MkvFile::new(file);
        let mut track = None;
        for track_info in mkv.tracks() {
            if track_info.language == KnownLanguage::English {
                track = Some(track_info.clone())
            }
        }
        let track = track.unwrap();
        let (width, height, palette) = match &track.encoding {
            KnownEncoding::VOB { width, height, palette } => {
                (width, height, palette)
            },
            _ => panic!()
        };
        println!("Size: {} x {}", width, height);
        let block_iter = mkv.block_iter_from_track_info(track.clone())?;
        let mut path = Path::new("output/vob/something").to_owned();
        for (i, block) in block_iter.enumerate() {
            let len = block.payload.len();
            let mut reader = std::io::Cursor::new(block.payload);
            let subtitle_packet_size = reader.read_u16::<BigEndian>().unwrap();
            assert_eq!(len, subtitle_packet_size as usize);
            //println!("Length: {:X}", len);
            //println!("Subtitle packet size: {:X}", subtitle_packet_size);

            // http://sam.zoy.org/writings/dvd/subtitles/ and http://dvd.sourceforge.net/spu_notes
            // disagree here, but the zoy source seems to be correct. The size of the data packet includes
            // the bytes we read to determine the size. We subtract that to get the size of the data
            // without the bytes representing the size itself.
            let data_packet_size = reader.read_u16::<BigEndian>().unwrap() as usize;
            let data_packet_data_start = reader.position() as usize;
            let data_packet_data_size = data_packet_size - data_packet_data_start;
            //println!("Data packet size: {:X}", data_packet_size);
            //println!("Data packet data start: {:X}", data_packet_data_start);
            //println!("Data packet data size: {:X}", data_packet_data_size);
            let mut data_packet_data = vec![0u8; data_packet_data_size as usize];
            reader.read_exact(&mut data_packet_data).unwrap();

            // Parse the command sequences
            loop {
                let current_sequence_position = reader.position() as usize;
                // http://sam.zoy.org/writings/dvd/subtitles/ says that each sequence starts
                // with 2 bytes with the date(?) and 2 bytes with the offest to the next
                // sequence.
                let date_data = reader.read_u16::<BigEndian>().unwrap();
                let next_seq_position = reader.read_u16::<BigEndian>().unwrap() as usize;

                let mut size = None;
                loop {
                    let command_type = reader.read_u8().unwrap();
                    //println!("{:X}", command_type);
                    match command_type {
                        0x00 => { /* Start subpicture */}
                        0x01 => { /* Start displaying */ }
                        0x02 => { /* Stop displaying */ }
                        0x03 => {
                            // Palette information
                            let data = reader.read_u16::<BigEndian>().unwrap(); 
                        }
                        0x04 => {
                            // Alpha information
                            let data = reader.read_u16::<BigEndian>().unwrap(); 
                        }
                        0x05 => { 
                            // Screen coordinates
                            let mut data = vec![0u8; 6];
                            reader.read_exact(&mut data).unwrap();

                            // The data is in the form of x1, x2, y1, y2, with
                            // each value being 3 nibbles in size.
                            //println!("{:02X?}", &data);
                            let (x1, x2) = parse_two_u12(&data[0..3]);
                            let (y1, y2) = parse_two_u12(&data[3..]);
                            //println!("x1: {:03X} x2: {:03X} y1: {:03X}, y2: {:03X}", x1, x2, y1, y2);
                            let (width, height) = compute_size(x1, x2, y1, y2);
                            //println!("size: {:03X} x {:03X}", width, height);

                            size = Some((width as usize, height as usize))
                        }
                        0x06 => { 
                            // Image data (?)
                            let first_line_position = reader.read_u16::<BigEndian>().unwrap() as usize;
                            let second_line_position = reader.read_u16::<BigEndian>().unwrap() as usize;
                            let first_line_position = first_line_position - data_packet_data_start;
                            let second_line_position = second_line_position - data_packet_data_start;
                            //println!("First line position: {:X}", first_line_position);
                            //println!("Second line position: {:X}", second_line_position);
                            let even_data = &data_packet_data[first_line_position..second_line_position];
                            let odd_data = &data_packet_data[second_line_position..];
                            //println!("Even data: {:X?}", even_data);
                            //println!("Odd data: {:X?}", odd_data);
                            {
                                let temp_palette = [
                                    Color { A: 255, R: 255, G: 255, B: 255},
                                    Color { A: 255, R: 0, G: 0, B: 0},
                                    Color { A: 255, R: 255, G: 0, B: 0},
                                    Color { A: 255, R: 0, G: 255, B: 0},
                                    Color { A: 255, R: 0, G: 0, B: 255},
                                ];
                                let (width, height) = size.unwrap();
                                let even_lines_pixels = decode_image(even_data, width, height / 2, &temp_palette);
                                let odd_lines_pixels = decode_image(odd_data, width, height - height / 2, &temp_palette);
                                let bytes = interlace_image(&even_lines_pixels, &odd_lines_pixels, width, height);
                                path.set_file_name(&format!("{}size{}x{}.bin", i, width, height));
                                std::fs::write(&path, &bytes).unwrap();
                            }
                        }
                        0xFF => {
                            break;
                        }
                        _ => { 
                            println!("Position: {:X}", reader.position());
                            println!("Payload: {:X?}", &reader.get_ref()[(reader.position() - 1) as usize ..]);
                            println!("Payload: {:X?}", &reader.get_ref());
                            panic!("Unknown command type: 0x{:X}", command_type) 
                        }
                    }
                }

                if current_sequence_position == next_seq_position {
                    break;
                }
            }
        }
        Ok(())
    }

    fn interlace_image(even_data: &[u8], odd_data: &[u8], width: usize, height: usize) -> Vec<u8> {
        let bytes_per_pixel = 4;
        let mut bytes = vec![0u8; width * height * bytes_per_pixel];
        assert_eq!(even_data.len() + odd_data.len(), bytes.len());
        let stride = width * bytes_per_pixel;
        for (i, line) in even_data.chunks(stride).enumerate() {
            let interlaced_index = (i * 2) * stride;
            (&mut bytes[interlaced_index..interlaced_index + stride]).copy_from_slice(line);
        }
        for (i, line) in odd_data.chunks(stride).enumerate() {
            let mut interlaced_index = ((i * 2) + 1) * stride;
            // TODO: Find the source of my counting bug
            if interlaced_index == bytes.len() {
                interlaced_index = interlaced_index - stride;
            }
            (&mut bytes[interlaced_index..interlaced_index + stride]).copy_from_slice(line);
        }
        bytes
    }

    fn decode_image(data: &[u8], width: usize, height: usize, palette: &[Color]) -> Vec<u8> {
        let mut pixels = Vec::new();
        let mut nibble_reader = NibbleReader::new(data);
        loop {
            if pixels.len() == width * height {
                break;
            } else if pixels.len() > width * height {
                panic!("Too many pixels!");
            }

            let first_nibble = nibble_reader.read_u4();
            if first_nibble.is_none() {
                break;
            }
            let first_nibble = first_nibble.unwrap();
            match first_nibble {
                0xf | 0xe | 0xd | 0xc | 0xb | 0xa | 0x9 | 0x8 | 0x7 | 0x6 | 0x5 | 0x4 => { 
                    let value = first_nibble;
                    let num_pixels = value >> 2;
                    let color = value & 0x3;
                    //println!("1 nibble value: num_pixels: {} color: {}", num_pixels, color);
                    for _ in 0..num_pixels {
                        let color = palette[color as usize];
                        pixels.push(color);
                    }
                }
                0x3 | 0x2 | 0x1 => {
                    let second_nibble = nibble_reader.read_u4().unwrap();
                    let value = (first_nibble << 4) | second_nibble;
                    let num_pixels = value >> 2;
                    let color = value & 0x3;
                    //println!("2 nibble value: num_pixels: {} color: {}", num_pixels, color);
                    for _ in 0..num_pixels {
                        let color = palette[color as usize];
                        pixels.push(color);
                    }
                }
                0x0 => {
                    let second_nibble = nibble_reader.read_u4().unwrap();
                    match second_nibble {
                        0xf | 0xe | 0xd | 0xc | 0xb | 0xa | 0x9 | 0x8 | 0x7 | 0x6 | 0x5 | 0x4 => {
                            let value = (first_nibble << 4) | second_nibble;
                            let third_nibble = nibble_reader.read_u4().unwrap();
                            let value = ((value as u16) << 4) | third_nibble as u16;
                            let num_pixels = value >> 2;
                            let color = value & 0x3;
                            //println!("3 nibble value: num_pixels: {} color: {}", num_pixels, color);
                            for _ in 0..num_pixels {
                                let color = palette[color as usize];
                                pixels.push(color);
                            }
                        }
                        0x3 | 0x2 | 0x1 => {
                            let value = (first_nibble << 4) | second_nibble;
                            let third_nibble = nibble_reader.read_u4().unwrap();
                            let fourth_nibble = nibble_reader.read_u4().unwrap();
                            let value2 = (third_nibble << 4) | fourth_nibble;
                            let value = (value as u16) << 8 | value2 as u16;
                            let num_pixels = value >> 2;
                            let color = value & 0x3;
                            //println!("4 nibble value: num_pixels: {} color: {}", num_pixels, color);
                            for _ in 0..num_pixels {
                                let color = palette[color as usize];
                                pixels.push(color);
                            }
                        }
                        0x0 => {
                            let value = (first_nibble << 4) | second_nibble;
                            let third_nibble = nibble_reader.read_u4().unwrap();
                            let fourth_nibble = nibble_reader.read_u4().unwrap();
                            let value2 = (third_nibble << 4) | fourth_nibble;
                            let value = (value as u16) << 8 | value2 as u16;
                            assert_eq!(third_nibble, 0);
                            let color = value & 0x3;
                            nibble_reader.round_to_next_byte();
                            //println!("Fill rest of line with : {}", color);
                            let current_position = pixels.len() % width;
                            let num_pixels = width - current_position;
                            for _ in 0..num_pixels {
                                let color = palette[color as usize];
                                pixels.push(color);
                            }
                            
                        }
                        _ => panic!("Unknown second nibble: {:X}", second_nibble)
                    }
                    
                }
                _ => panic!("Unknown first nibble: {:X}", first_nibble)
            }
        }

        let mut bytes = Vec::new();
        for color in pixels {
            bytes.push(color.B);
            bytes.push(color.G);
            bytes.push(color.R);
            bytes.push(color.A);
        }
        //let expected = width * height * 4;
        //while bytes.len() < expected {
        //    bytes.push(0);
        //}
        bytes
    }

    struct NibbleReader<'a> {
        data: &'a [u8],
        pos: usize,
    }

    impl<'a> NibbleReader<'a> {
        pub fn new(data: &'a [u8]) -> Self {
            Self {
                data,
                pos: 0,
            }
        }

        pub fn read_u4(&mut self) -> Option<u8> {
            let pos = self.pos; 
            let byte_index = pos / 2;
            if byte_index >= self.data.len() {
                return None;
            }
            self.pos += 1;
            let byte = self.data[byte_index];
            if pos % 2 == 0 {
                Some(byte >> 4)
            } else {
                Some((byte << 4) >> 4)
            }
        }

        pub fn round_to_next_byte(&mut self) {
            if self.pos % 2 != 0 {
                self.pos += 1;
            }
        }
    }
}