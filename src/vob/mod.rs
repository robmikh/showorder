use std::io::Read;

use byteorder::{BigEndian, ReadBytesExt};
use windows::{
    core::Result,
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Storage::Streams::Buffer,
    UI::Color,
};

use crate::{interop::as_mut_slice, mkv::KnownEncoding};

pub fn parse_idx(data: &[u8]) -> KnownEncoding {
    let idx_string = String::from_utf8_lossy(data);
    //println!("{}", idx_string);
    let lines = idx_string.lines();
    //let first_line = lines.nth(0).unwrap();
    //if first_line != r#"# VobSub index file, v7 (do not modify this line!)"# {
    //    println!("Warning! Expected to see the VobSub v7 line at the beginning of the private data...");
    //}
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
                _ => {
                    //println!("Unknown name: \"{}\"", name);
                }
            }
        }
    }

    let (width, height) = size.expect("Expected size in Vob subtitle track private data");
    let palette = palette.expect("Expected palette in Vob subtitle track private data");

    KnownEncoding::VOB {
        width,
        height,
        palette,
    }
}

pub fn parse_block(data: &[u8], palette: &[Color]) -> Result<Option<SoftwareBitmap>> {
    if let Some((bytes, width, height)) = decode_block(data, palette) {
        let bitmap_size = (width * height * 4) as u32;
        let bitmap_buffer = Buffer::Create(bitmap_size)?;
        bitmap_buffer.SetLength(bitmap_size)?;
        {
            let slice = unsafe { as_mut_slice(&bitmap_buffer)? };
            slice.copy_from_slice(&bytes);
        }
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            bitmap_buffer,
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
        )?;
        Ok(Some(bitmap))
    } else {
        Ok(None)
    }
}

fn parse_two_u12(data: &[u8]) -> (u16, u16) {
    let v1_p1 = (data[0] as u16) << 8;
    let v1_p2 = data[1] as u16;
    let v1 = (v1_p1 | v1_p2) >> 4;
    let v2_p1 = (data[1] as u16) << 8;
    let v2_p2 = data[2] as u16;
    let v2 = ((v2_p1 | v2_p2) << 4) >> 4;
    (v1, v2)
}

fn compute_size(x1: u16, x2: u16, y1: u16, y2: u16) -> (u16, u16) {
    let width = x2 - x1 + 1;
    let height = y2 - y1 + 1;
    (width, height)
}

fn read_four_nibbles<R: Read>(mut reader: R) -> Option<[usize; 4]> {
    let mut data = vec![0u8; 2];
    reader.read_exact(&mut data).ok()?;
    let mut nibble_reader = NibbleReader::new(&data);
    let value0 = nibble_reader.read_u4()?;
    let value1 = nibble_reader.read_u4()?;
    let value2 = nibble_reader.read_u4()?;
    let value3 = nibble_reader.read_u4()?;
    Some([
        value0 as usize,
        value1 as usize,
        value2 as usize,
        value3 as usize,
    ])
}

fn decode_block(block_data: &[u8], palette: &[Color]) -> Option<(Vec<u8>, usize, usize)> {
    let len = block_data.len();
    let mut reader = std::io::Cursor::new(block_data);
    let subtitle_packet_size = reader.read_u16::<BigEndian>().unwrap();
    assert_eq!(len, subtitle_packet_size as usize);

    // http://sam.zoy.org/writings/dvd/subtitles/ and http://dvd.sourceforge.net/spu_notes
    // disagree here, but the zoy source seems to be correct. The size of the data packet includes
    // the bytes we read to determine the size. We subtract that to get the size of the data
    // without the bytes representing the size itself.
    let data_packet_size = reader.read_u16::<BigEndian>().unwrap() as usize;
    let data_packet_data_start = reader.position() as usize;
    let data_packet_data_size = data_packet_size - data_packet_data_start;
    let mut data_packet_data = vec![0u8; data_packet_data_size as usize];
    reader.read_exact(&mut data_packet_data).unwrap();

    // Parse the command sequences
    loop {
        let current_sequence_position = reader.position() as usize;
        // http://sam.zoy.org/writings/dvd/subtitles/ says that each sequence starts
        // with 2 bytes with the date(?) and 2 bytes with the offest to the next
        // sequence.
        let _date_data = reader.read_u16::<BigEndian>().unwrap();
        let next_seq_position = reader.read_u16::<BigEndian>().unwrap() as usize;

        // Ordering isn't gartunteed, so we must defer the parsing
        let mut size = None;
        let mut current_color_palette = None;
        let mut current_alpha_palette = None;
        let mut interlaced_data = None;
        loop {
            let command_type = reader.read_u8().unwrap();
            //println!("{:X}", command_type);
            match command_type {
                0x00 => { /* Start subpicture */ }
                0x01 => { /* Start displaying */ }
                0x02 => { /* Stop displaying */ }
                0x03 => {
                    // Palette information
                    current_color_palette = Some(read_four_nibbles(&mut reader).unwrap());
                }
                0x04 => {
                    // Alpha information
                    current_alpha_palette = Some(read_four_nibbles(&mut reader).unwrap());
                }
                0x05 => {
                    // Screen coordinates
                    let mut data = vec![0u8; 6];
                    reader.read_exact(&mut data).unwrap();

                    // The data is in the form of x1, x2, y1, y2, with
                    // each value being 3 nibbles in size.
                    let (x1, x2) = parse_two_u12(&data[0..3]);
                    let (y1, y2) = parse_two_u12(&data[3..]);
                    let (width, height) = compute_size(x1, x2, y1, y2);

                    size = Some((width as usize, height as usize))
                }
                0x06 => {
                    // Image data location
                    let first_line_position = reader.read_u16::<BigEndian>().unwrap() as usize;
                    let second_line_position = reader.read_u16::<BigEndian>().unwrap() as usize;
                    let first_line_position = first_line_position - data_packet_data_start;
                    let second_line_position = second_line_position - data_packet_data_start;
                    let even_data = &data_packet_data[first_line_position..second_line_position];
                    let odd_data = &data_packet_data[second_line_position..];
                    interlaced_data = Some((even_data, odd_data));
                }
                0xFF => {
                    break;
                }
                _ => {
                    panic!("Unknown command type: 0x{:X}", command_type)
                }
            }
        }

        // Now complete parsing
        if let Some((even_data, odd_data)) = interlaced_data {
            let palette = build_subpalette(
                &palette,
                &current_color_palette.expect("No color palette found!"),
                &current_alpha_palette.expect("No alpha palette found!"),
            );
            let (width, height) = size.expect("No size found!");
            //println!("Size: {} x {}", width, height);
            let even_lines_pixels =
                decode_image(even_data, width, height / 2, &palette);
            let odd_lines_pixels =
                decode_image(odd_data, width, height - height / 2, &palette);
            let bytes =
                interlace_image(&even_lines_pixels, &odd_lines_pixels, width, height);
            return Some((bytes, width, height));
        }

        if current_sequence_position == next_seq_position {
            break;
        }
    }
    None
}

fn build_subpalette(palette: &[Color], color_info: &[usize], alpha_info: &[usize]) -> Vec<Color> {
    let mut subpalette = Vec::new();
    for (i, color_index) in color_info.iter().enumerate() {
        let original_alpha_value = alpha_info[i];
        let color = if original_alpha_value == 0 {
            Color {
                A: 0,
                R: 0,
                G: 0,
                B: 0,
            }
        } else {
            let palette_color = &palette[*color_index];
            let alpha_value = ((16.min(original_alpha_value + 1) as f32 / 16.0) * 255.0) as usize;
            Color {
                A: alpha_value as u8,
                R: palette_color.R,
                G: palette_color.G,
                B: palette_color.B,
            }
        };
        subpalette.push(color);
    }
    subpalette
}

fn interlace_image(even_data: &[u8], odd_data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let bytes_per_pixel = 4;
    let mut bytes = vec![0u8; width * height * bytes_per_pixel];
    // TODO: Somtimes we're an entire row short...
    //assert_eq!(even_data.len() + odd_data.len(), bytes.len());
    assert!(even_data.len() + odd_data.len() <= bytes.len());
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
    let total_pixels = width * height;
    //println!("Decoding image ({} x {}), with {} pixels...", width, height, total_pixels);
    let mut pixels = Vec::new();
    let mut nibble_reader = NibbleReader::new(data);
    loop {
        if pixels.len() == total_pixels {
            break;
        } else if pixels.len() > total_pixels {
            panic!(
                "Too many pixels! {} > {} ({} * {})",
                pixels.len(),
                total_pixels,
                width,
                height
            );
            //println!("  Too many pixels ({}). Bailing...", pixels.len());
            //pixels.resize(total_pixels, Color { A: 0, R: 0, G: 0, B: 0 });
            //break;
        }

        let first_nibble = nibble_reader.read_u4();
        if first_nibble.is_none() {
            break;
        }
        let first_nibble = first_nibble.unwrap();
        let (num_pixels, color) = match first_nibble {
            0xf | 0xe | 0xd | 0xc | 0xb | 0xa | 0x9 | 0x8 | 0x7 | 0x6 | 0x5 | 0x4 => {
                let value = first_nibble;
                let num_pixels = (value >> 2) as usize;
                let color = (value & 0x3) as usize;
                //println!("1 nibble value: num_pixels: {} color: {}", num_pixels, color);
                (num_pixels, color)
            }
            0x3 | 0x2 | 0x1 => {
                let second_nibble = nibble_reader.read_u4().unwrap();
                let value = (first_nibble << 4) | second_nibble;
                let num_pixels = (value >> 2) as usize;
                let color = (value & 0x3) as usize;
                //println!("2 nibble value: num_pixels: {} color: {}", num_pixels, color);
                (num_pixels, color)
            }
            0x0 => {
                let second_nibble = nibble_reader.read_u4().unwrap();
                match second_nibble {
                    0xf | 0xe | 0xd | 0xc | 0xb | 0xa | 0x9 | 0x8 | 0x7 | 0x6 | 0x5 | 0x4 => {
                        let value = (first_nibble << 4) | second_nibble;
                        let third_nibble = nibble_reader.read_u4().unwrap();
                        let value = ((value as u16) << 4) | third_nibble as u16;
                        let num_pixels = (value >> 2) as usize;
                        let color = (value & 0x3) as usize;
                        //println!("3 nibble value: num_pixels: {} color: {}", num_pixels, color);
                        (num_pixels, color)
                    }
                    0x3 | 0x2 | 0x1 => {
                        let value = (first_nibble << 4) | second_nibble;
                        let third_nibble = nibble_reader.read_u4().unwrap();
                        let fourth_nibble = nibble_reader.read_u4().unwrap();
                        let value2 = (third_nibble << 4) | fourth_nibble;
                        let value = (value as u16) << 8 | value2 as u16;
                        let num_pixels = (value >> 2) as usize;
                        let color = (value & 0x3) as usize;
                        //println!("4 nibble value: num_pixels: {} color: {}", num_pixels, color);
                        (num_pixels, color)
                    }
                    0x0 => {
                        let value = (first_nibble << 4) | second_nibble;
                        let third_nibble = nibble_reader.read_u4().unwrap();
                        let fourth_nibble = nibble_reader.read_u4().unwrap();
                        let value2 = (third_nibble << 4) | fourth_nibble;
                        let value = (value as u16) << 8 | value2 as u16;
                        assert_eq!(third_nibble, 0);
                        let color = (value & 0x3) as usize;
                        //nibble_reader.round_to_next_byte();
                        //println!("Fill rest of line with : {}", color);
                        let current_position = pixels.len() % width;
                        let num_pixels = width - current_position;
                        (num_pixels, color)
                    }
                    _ => panic!("Unknown second nibble: {:X}", second_nibble),
                }
            }
            _ => panic!("Unknown first nibble: {:X}", first_nibble),
        };
        for _ in 0..num_pixels {
            let color = palette[3 - color]; // ???
            pixels.push(color);
        }
        if pixels.len() % width == 0 {
            //println!("  Ending line with {} pixels...", pixels.len());
            nibble_reader.round_to_next_byte();
        }
    }

    let mut bytes = Vec::new();
    for color in pixels {
        bytes.push(color.B);
        bytes.push(color.G);
        bytes.push(color.R);
        bytes.push(color.A);
    }
    bytes
}

struct NibbleReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> NibbleReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
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

#[cfg(test)]
mod test {
    use super::*;

    fn test_pos_data(
        data: &[u8],
        x1_expected: u16,
        x2_expected: u16,
        y1_expected: u16,
        y2_expected: u16,
    ) {
        println!("data: {:02X?}", data);
        let (x1, x2) = parse_two_u12(&data[0..3]);
        let (y1, y2) = parse_two_u12(&data[3..]);
        assert_eq!(x1, x1_expected);
        assert_eq!(x2, x2_expected);
        assert_eq!(y1, y1_expected);
        assert_eq!(y2, y2_expected);
        println!(
            "x1: {:03X} x2: {:03X} y1: {:03X}, y2: {:03X}",
            x1, x2, y1, y2
        );
        let (width, height) = compute_size(x1, x2, y1, y2);
        println!("size: {:03X} x {:03X}", width, height);
    }

    #[test]
    fn parse_u12_test() {
        test_pos_data(
            &[0x00u8, 0x02, 0xcf, 0x00, 0x22, 0x3e],
            0x000,
            0x2cf,
            0x002,
            0x23e,
        );
        test_pos_data(
            &[0x0Eu8, 0xA1, 0xE1, 0x1A, 0x01, 0xBB],
            0x0EA,
            0x1E1,
            0x1A0,
            0x1BB,
        );
    }
}
