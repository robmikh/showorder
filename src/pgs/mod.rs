mod image;
mod parsing;
mod types;

use byteorder::ReadBytesExt;
use nalgebra::SMatrix;
use windows::core::Result;
use windows::Graphics::Imaging::SoftwareBitmap;
use windows::UI::Color;

use self::image::decode_image;
use self::image::ConvertedPaletteEntry;
use self::parsing::PgsDeserializer;
use self::types::{ObjectDef, PaletteDef, PaletteEntry, SegmentHeader, SegmentType};

// This keeps parsing segments until the end of the data,
// and will return the first bitmap it's able to construct.
//
// WARNING: The bare minimum was implemented based on the
//          behavior of a small set of test files. Over time
//          this should more closely follow the spec.
//          Currently likely to break.
pub fn parse_segments(data: &[u8]) -> Result<Option<SoftwareBitmap>> {
    // The mkv spec (https://www.matroska.org/technical/subtitles.html) says
    // the PGS segments can be found within the blocks.
    //
    // From the spec:
    // The specifications for the HDMV presentation graphics subtitle format
    // (short: HDMV PGS) can be found in the document “Blu-ray Disc Read-Only
    // Format; Part 3 — Audio Visual Basic Specifications” in section 9.14
    // “HDMV graphics streams”.
    //
    // The blog post "Presentation Graphic Stream (SUP files) BluRay Subtitle Format" (http://blog.thescorpius.com/index.php/2017/07/15/presentation-graphic-stream-sup-files-bluray-subtitle-format/)
    // describes the PGS segment data. However we don't have the first 10 bytes
    // listed there (magic number, pts, dts).
    let mut reader = std::io::Cursor::new(data);
    let mut last_palette_data: Option<Vec<ConvertedPaletteEntry>> = None;
    while !reader.is_at_end() {
        let segment_header: SegmentHeader = reader.deserialize().unwrap();
        if segment_header.len == 0 {
            if segment_header.ty != SegmentType::EndDisplaySet {
                panic!(
                    "Invalid segment size for segment type ({:?}): {}",
                    segment_header.ty, segment_header.len
                );
            }
            continue;
        }
        let segment_data = reader.ref_bytes(segment_header.len as usize).unwrap();
        let mut segment_data_reader = std::io::Cursor::new(segment_data);

        match segment_header.ty {
            SegmentType::PaletteDef => {
                let (_, palettes) = read_palette_def_segment(&mut segment_data_reader).unwrap();
                let mut converted = Vec::new();
                for entry in palettes {
                    let color = convert_palette_color(&entry);
                    converted.push(color);
                }
                last_palette_data = Some(converted);
            }
            SegmentType::ObjDataDef => {
                let (object_def, color_data_lines) =
                    read_object_def_segment(&mut segment_data_reader).unwrap();
                if let Some(palette_data) = last_palette_data.as_ref() {
                    let bitmap = decode_image(&object_def, &color_data_lines, palette_data)?;
                    return Ok(Some(bitmap));
                } else {
                    println!("Warning! Expected to have encountered a palette definition before an object definition. Skipping segment...");
                }
            }
            _ => {}
        }
    }
    Ok(None)
}

fn read_palette_def_segment(
    reader: &mut std::io::Cursor<&[u8]>,
) -> std::io::Result<(PaletteDef, Vec<PaletteEntry>)> {
    let palette_def: PaletteDef = reader.deserialize()?;
    let mut palettes = Vec::new();
    while !reader.is_at_end() {
        let palette: PaletteEntry = reader.deserialize()?;
        palettes.push(palette);
    }
    Ok((palette_def, palettes))
}

fn convert_palette_color(entry: &PaletteEntry) -> ConvertedPaletteEntry {
    type Matrix3x3 = SMatrix<f32, 3, 3>;
    type Matrix3x1 = SMatrix<f32, 3, 1>;

    static COLOR_CONVERSION_MATRIX: Matrix3x3 = Matrix3x3::new(
        // https://web.archive.org/web/20180421030430/http://www.equasys.de/colorconversion.html
        1.164, 0.000, 1.793, 1.164, -0.213, -0.533, 1.164, 2.112,
        0.000,
        // https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-rdprfx/2e1618ed-60d6-4a64-aa5d-0608884861bb
        //1.0, 0.0, 1.402525, 1.0, -0.343730, -0.714401, 1.0, 1.769905, 0.000013,
    );

    let values = Matrix3x1::new(
        (entry.luminance.wrapping_sub(16)) as f32,
        (entry.color_difference_blue.wrapping_sub(128)) as f32,
        (entry.color_difference_red.wrapping_sub(128)) as f32,
    );

    let rgb_values: Matrix3x1 = COLOR_CONVERSION_MATRIX * values;
    let r = *rgb_values.get((0, 0)).unwrap() as u8;
    let b = *rgb_values.get((1, 0)).unwrap() as u8;
    let g = *rgb_values.get((2, 0)).unwrap() as u8;
    let color = Color {
        A: entry.alpha,
        R: r,
        G: g,
        B: b,
    };
    ConvertedPaletteEntry {
        id: entry.palette_entry_id,
        color,
    }
}

fn read_object_def_segment(
    reader: &mut std::io::Cursor<&[u8]>,
) -> std::io::Result<(ObjectDef, Vec<Vec<(i32, i32)>>)> {
    let object_def: ObjectDef = reader.deserialize()?;
    let mut color_data_lines: Vec<Vec<(i32, i32)>> = Vec::new();
    let mut current_line: Vec<(i32, i32)> = Vec::new();
    while !reader.is_at_end() {
        let encoded_byte = reader.read_u8()?;

        let mut color_and_num: Option<(i32, i32)> = None;
        if encoded_byte == 0 {
            let num_pixel_data = reader.read_u8()?;
            if num_pixel_data == 0 {
                // End the line
                let old_line = current_line;
                current_line = Vec::new();
                color_data_lines.push(old_line);
            } else {
                // Get the first two bits
                let code = num_pixel_data >> 6;
                let num_data = (((num_pixel_data << 2) as u8) >> 2) as u8;
                match code {
                    0 => {
                        color_and_num = Some((0, num_data as i32));
                    }
                    1 => {
                        let second = reader.read_u8()?;
                        let bytes = [num_data, second];
                        color_and_num = Some((0, u16::from_be_bytes(bytes) as i32));
                    }
                    2 => {
                        let color = reader.read_u8()?;
                        color_and_num = Some((color as i32, num_data as i32));
                    }
                    3 => {
                        let second = reader.read_u8()?;
                        let bytes = [num_data, second];
                        let color = reader.read_u8()?;
                        color_and_num = Some((color as i32, u16::from_be_bytes(bytes) as i32));
                    }
                    _ => panic!("Unexpected code: {:X}", code),
                }
            }
        } else {
            color_and_num = Some((encoded_byte as i32, 1));
        }

        if let Some((color, num)) = color_and_num {
            current_line.push((color, num));
        }
    }
    Ok((object_def, color_data_lines))
}
