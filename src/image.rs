use windows::{
    core::Result,
    Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap},
    Storage::Streams::Buffer,
    UI::Color,
};

use crate::interop::{as_mut_slice, memory_buffer_as_mut_slice, memory_buffer_as_slice};

pub fn scale_image(src_bitmap: &SoftwareBitmap, scale: f32) -> Result<SoftwareBitmap> {
    let width = src_bitmap.PixelWidth()? as usize;
    let height = src_bitmap.PixelHeight()? as usize;

    let new_width = (width as f32 * scale).ceil() as usize;
    let new_height = (height as f32 * scale).ceil() as usize;

    let format = src_bitmap.BitmapPixelFormat()?;
    assert_eq!(format, BitmapPixelFormat::Bgra8);
    let bytes_per_pixel = 4;
    let bitmap_size = (new_width * new_height * bytes_per_pixel) as u32;
    let buffer = Buffer::Create(bitmap_size)?;
    buffer.SetLength(bitmap_size)?;

    {
        let bitmap_buffer = src_bitmap.LockBuffer(BitmapBufferAccessMode::Read)?;
        let bitmap_ref = bitmap_buffer.CreateReference()?;
        let src_slice = unsafe { memory_buffer_as_slice(&bitmap_ref)? };
        let dest_slice = unsafe { as_mut_slice(&buffer)? };
        for y in 0..new_height {
            for x in 0..new_width {
                let x_src = (x as f32 / scale).floor() as usize;
                let y_src = (y as f32 / scale).floor() as usize;
                let src_index = ((width * y_src) + (x_src % width)) * bytes_per_pixel;
                let dest_index = ((new_width * y) + (x % new_width)) * bytes_per_pixel;
                (&mut dest_slice[dest_index..dest_index + bytes_per_pixel])
                    .copy_from_slice(&src_slice[src_index..src_index + bytes_per_pixel]);
            }
        }
        bitmap_ref.Close()?;
        bitmap_buffer.Close()?;
    }

    let scaled_bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        buffer,
        BitmapPixelFormat::Bgra8,
        new_width as i32,
        new_height as i32,
    )?;
    Ok(scaled_bitmap)
}

pub fn blend_with_color(bitmap: &SoftwareBitmap, color: &Color) -> Result<()> {
    let format = bitmap.BitmapPixelFormat()?;
    assert_eq!(format, BitmapPixelFormat::Bgra8);
    let bytes_per_pixel = 4;

    // We ignore the alpha channel for the background color
    let background_blue = color.B as f32 / 255.0;
    let background_green = color.G as f32 / 255.0;
    let background_red = color.R as f32 / 255.0;

    {
        let bitmap_buffer = bitmap.LockBuffer(BitmapBufferAccessMode::ReadWrite)?;
        let bitmap_ref = bitmap_buffer.CreateReference()?;
        let bytes = unsafe { memory_buffer_as_mut_slice(&bitmap_ref)? };
        for pixel_bytes in bytes.chunks_mut(bytes_per_pixel) {
            let src_blue = pixel_bytes[0] as f32 / 255.0;
            let src_green = pixel_bytes[1] as f32 / 255.0;
            let src_red = pixel_bytes[2] as f32 / 255.0;
            let src_alpha = pixel_bytes[3] as f32 / 255.0;
            let one_minus_src_alpha = 1.0 - src_alpha;
            pixel_bytes[0] =
                (((src_blue * src_alpha) + (background_blue * one_minus_src_alpha)) * 255.0) as u8;
            pixel_bytes[1] = (((src_green * src_alpha) + (background_green * one_minus_src_alpha))
                * 255.0) as u8;
            pixel_bytes[2] =
                (((src_red * src_alpha) + (background_red * one_minus_src_alpha)) * 255.0) as u8;
            pixel_bytes[3] = 255;
        }
    }

    Ok(())
}
