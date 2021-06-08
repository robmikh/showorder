fn main() {
    windows::build!(
        Windows::Foundation::*,
        Windows::UI::Color,
        Windows::Graphics::Imaging::{
            SoftwareBitmap, BitmapPixelFormat
        },
        Windows::Storage::Streams::{
            IBuffer, Buffer,
        },
        Windows::Media::Ocr::{
            OcrEngine, OcrResult,
        },
        Windows::Globalization::{
            Language,
        },
        Windows::Win32::System::WinRT::{
            IBufferByteAccess,
        },
    );
}