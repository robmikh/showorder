use bindings::Windows::Graphics::Imaging::BitmapPixelFormat;
use bindings::Windows::Graphics::Imaging::SoftwareBitmap;
use bindings::Windows::Storage::Streams::Buffer;
use bindings::Windows::UI::Color;

use crate::interop::as_mut_slice;

use super::types::ObjectDef;

#[derive(Debug)]
pub struct ConvertedPaletteEntry {
    pub id: u8,
    pub color: Color,
}

impl ConvertedPaletteEntry {
    pub const DEFAULT: Self = Self {
        id: 0,
        // The OCR APIs really hate transparent black for some reason...
        color: Color {
            A: 0,
            R: 0,
            G: 0,
            B: 0,
        },
    };
}

pub fn decode_image(
    object_def: &ObjectDef,
    color_data_lines: &Vec<Vec<(i32, i32)>>,
    palette_data: &Vec<ConvertedPaletteEntry>,
) -> windows::Result<SoftwareBitmap> {
    let width = object_def.width as u32;
    let height = object_def.height as u32;
    let bitmap_size = width * height * 4;
    let bitmap_buffer = Buffer::Create(bitmap_size)?;
    bitmap_buffer.SetLength(bitmap_size)?;
    {
        let slice = unsafe { as_mut_slice(&bitmap_buffer)? };
        let mut pixel_index = 0;
        for line in color_data_lines {
            for (palette_id, num) in line {
                let palette_color = palette_data
                    .iter()
                    .find(|p| p.id as i32 == *palette_id)
                    .unwrap_or(&ConvertedPaletteEntry::DEFAULT);
                let color = palette_color.color;
                for _ in 0..*num as usize {
                    let index = pixel_index * 4;
                    slice[index + 0] = color.B;
                    slice[index + 1] = color.G;
                    slice[index + 2] = color.R;
                    slice[index + 3] = color.A;
                    pixel_index += 1;
                }
            }
        }
    }
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        bitmap_buffer,
        BitmapPixelFormat::Bgra8,
        width as i32,
        height as i32,
    )?;
    Ok(bitmap)
}
