use windows::{
    core::{Interface, Result},
    Foundation::IMemoryBufferReference,
    Storage::Streams::Buffer,
    Win32::System::WinRT::{IBufferByteAccess, IMemoryBufferByteAccess},
};

pub unsafe fn as_mut_slice(buffer: &Buffer) -> Result<&mut [u8]> {
    let interop = buffer.cast::<IBufferByteAccess>()?;
    let len = buffer.Length()?;

    let data = interop.Buffer()?;
    Ok(std::slice::from_raw_parts_mut(data, len as _))
}

pub unsafe fn memory_buffer_as_slice(buffer: &IMemoryBufferReference) -> Result<&[u8]> {
    let interop = buffer.cast::<IMemoryBufferByteAccess>()?;
    let mut data = std::ptr::null_mut();
    let mut len = 0;

    interop.GetBuffer(&mut data, &mut len)?;
    Ok(std::slice::from_raw_parts(data, len as _))
}

pub unsafe fn memory_buffer_as_mut_slice(buffer: &IMemoryBufferReference) -> Result<&mut [u8]> {
    let interop = buffer.cast::<IMemoryBufferByteAccess>()?;
    let mut data = std::ptr::null_mut();
    let mut len = 0;

    interop.GetBuffer(&mut data, &mut len)?;
    Ok(std::slice::from_raw_parts_mut(data, len as _))
}
