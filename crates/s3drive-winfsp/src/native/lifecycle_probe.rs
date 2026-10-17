//! Safe test-only wrappers around the exact native disposition structures.
use std::{fs::File, mem::size_of, os::windows::io::AsRawHandle};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        FILE_DISPOSITION_FLAG_ON_CLOSE, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
        FILE_STANDARD_INFO, FileDispositionInfo, FileDispositionInfoEx, FileStandardInfo,
        GetFileInformationByHandleEx, SetFileInformationByHandle,
    },
};
pub fn pending(file: &File) -> std::io::Result<bool> {
    let mut info = FILE_STANDARD_INFO::default();
    // SAFETY: the borrowed File owns a live handle; info is initialized, writable
    // and exactly the size required for FileStandardInfo throughout the call.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileStandardInfo,
            (&mut info as *mut FILE_STANDARD_INFO).cast(),
            size_of::<FILE_STANDARD_INFO>() as u32,
        )
    }
    .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(info.DeletePending)
}
pub fn set_delete(file: &File, delete: bool) -> std::io::Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: delete };
    // SAFETY: the exact repr(C) input and its borrowed live handle remain valid
    // for this synchronous FileDispositionInfo request; no ownership transfers.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(|e| std::io::Error::other(e.to_string()))
}
pub fn clear_delete_on_close(file: &File) -> windows::core::Result<()> {
    let info = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_ON_CLOSE,
    };
    // SAFETY: the exact extended input and borrowed live handle remain valid;
    // ON_CLOSE without DELETE requests cancellation, not an inferred callback.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfoEx,
            (&info as *const FILE_DISPOSITION_INFO_EX).cast(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    }
}
pub fn set_posix_delete(file: &File) -> std::io::Result<()> {
    use windows::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    let info = FILE_DISPOSITION_INFO_EX {
        Flags: windows::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO_EX_FLAGS(
            FILE_DISPOSITION_FLAG_DELETE.0 | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0,
        ),
    };
    // SAFETY: exact synchronous extended disposition input with borrowed live
    // File ownership and no buffers or references retained by the API.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfoEx,
            (&info as *const FILE_DISPOSITION_INFO_EX).cast(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    }
    .map_err(|e| std::io::Error::other(e.to_string()))
}

/// Exercise the actual directory+DoC Create callback in one NT request. Tests
/// first run the identical request against their owned local NTFS control.
pub fn create_directory_delete_on_close(path: &std::path::Path) -> std::io::Result<File> {
    use std::os::windows::io::FromRawHandle;
    use windows::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_CREATE, FILE_DELETE_ON_CLOSE, FILE_DIRECTORY_FILE,
                FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{OBJ_CASE_INSENSITIVE, UNICODE_STRING},
            Storage::FileSystem::{
                DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
                FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, SYNCHRONIZE,
            },
            System::IO::IO_STATUS_BLOCK,
        },
        core::PWSTR,
    };
    let path = path
        .to_str()
        .ok_or_else(|| std::io::Error::other("Unicode local test path required"))?
        .replace('/', "\\");
    if path.as_bytes().get(1) != Some(&b':')
        || path.as_bytes().get(2) != Some(&b'\\')
        || !path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        || path.contains('\0')
    {
        return Err(std::io::Error::other(
            "absolute local DOS test path required",
        ));
    }
    let mut text = format!("\\??\\{path}")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let max =
        u16::try_from(text.len() * 2).map_err(|_| std::io::Error::other("test path too long"))?;
    let name = UNICODE_STRING {
        Length: max - 2,
        MaximumLength: max,
        Buffer: PWSTR(text.as_mut_ptr()),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        ObjectName: &name,
        Attributes: OBJ_CASE_INSENSITIVE,
        ..Default::default()
    };
    let mut handle = HANDLE::default();
    let mut status = IO_STATUS_BLOCK::default();
    // SAFETY: all NT structures, UTF-16 storage and initialized outputs remain
    // alive for the synchronous call. No EA/security pointers are supplied.
    // SYNCHRONIZE matches NONALERT; DELETE matches DELETE_ON_CLOSE.
    let result = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
            &attributes,
            &mut status,
            None,
            FILE_ATTRIBUTE_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_DELETE_ON_CLOSE | FILE_SYNCHRONOUS_IO_NONALERT,
            None,
            0,
        )
    };
    if result.0 < 0
        || handle.0.is_null()
        || handle == windows::Win32::Foundation::INVALID_HANDLE_VALUE
    {
        return Err(std::io::Error::other(format!(
            "native directory Create failed: {result:?}"
        )));
    }
    // SAFETY: successful NtCreateFile transfers one newly owned synchronous
    // handle to File; no other Rust owner or CloseHandle call exists.
    Ok(unsafe { File::from_raw_handle(handle.0) })
}
