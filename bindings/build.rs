fn main() {
    windows::build!(
        Windows::Foundation::*,
        Windows::UI::Color,
        Windows::Graphics::Imaging::{
            SoftwareBitmap, BitmapPixelFormat, BitmapEncoder, BitmapBuffer, BitmapBufferAccessMode,
        },
        Windows::Storage::{
            StorageFolder, StorageFile, CreationCollisionOption, FileAccessMode, FileIO,
        },
        Windows::Storage::Streams::{
            IBuffer, Buffer, IRandomAccessStream,
        },
        Windows::Media::Ocr::{
            OcrEngine, OcrResult,
        },
        Windows::Globalization::{
            Language,
        },
        Windows::Win32::System::WinRT::{
            IBufferByteAccess, IMemoryBufferByteAccess, RoInitialize,
        },
    );
}
