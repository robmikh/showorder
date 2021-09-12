use bindings::Windows::{
    Foundation::IMemoryBufferReference,
    Storage::Streams::Buffer,
    Win32::System::WinRT::{IBufferByteAccess, IMemoryBufferByteAccess},
};
use windows::Interface;

pub unsafe fn as_mut_slice(buffer: &Buffer) -> windows::Result<&mut [u8]> {
    let interop = buffer.cast::<IBufferByteAccess>()?;
    let mut data = std::ptr::null_mut();
    let len = buffer.Length()?;

    interop.Buffer(&mut data).ok()?;
    Ok(std::slice::from_raw_parts_mut(data, len as _))
}

pub unsafe fn memory_buffer_as_slice(buffer: &IMemoryBufferReference) -> windows::Result<&[u8]> {
    let interop = buffer.cast::<IMemoryBufferByteAccess>()?;
    let mut data = std::ptr::null_mut();
    let mut len = 0;

    interop.GetBuffer(&mut data, &mut len).ok()?;
    Ok(std::slice::from_raw_parts(data, len as _))
}
