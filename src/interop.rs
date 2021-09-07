use bindings::Windows::{Storage::Streams::Buffer, Win32::System::WinRT::IBufferByteAccess};
use windows::Interface;

pub unsafe fn as_mut_slice(buffer: &Buffer) -> windows::Result<&mut [u8]> {
    let interop = buffer.cast::<IBufferByteAccess>()?;
    let mut data = std::ptr::null_mut();
    let len = buffer.Length()?;

    interop.Buffer(&mut data).ok()?;
    Ok(std::slice::from_raw_parts_mut(data, len as _))
}