use windows::{
    core::Result,
    Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap},
    Storage::Streams::Buffer,
};

use crate::interop::{as_mut_slice, memory_buffer_as_slice};

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
