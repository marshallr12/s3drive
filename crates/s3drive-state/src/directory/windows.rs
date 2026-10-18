//! Audited native directory/ACL boundary. No SQLite or S3 operations here.
use crate::Result;
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    mem::size_of,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, Prefix},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE, HLOCAL, LocalFree},
        Security::{
            ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                GetSecurityInfo, SE_FILE_OBJECT,
            },
            DACL_SECURITY_INFORMATION, GetTokenInformation, OWNER_SECURITY_INFORMATION,
            PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, FILE_ALL_ACCESS,
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW,
            GetFileInformationByHandle, GetVolumeInformationByHandleW, READ_CONTROL,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    },
    core::{HRESULT, HSTRING, PWSTR},
};

struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: this owns the successful OpenProcessToken result.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: these pointers are allocated by documented LocalFree APIs.
        if !self.0.is_null() {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0)));
            }
        }
    }
}

fn sid_string(sid: PSID) -> Result<String> {
    let mut text = PWSTR::null();
    // SAFETY: callers supply a valid SID inside a live token/ACL allocation.
    unsafe {
        ConvertSidToStringSidW(sid, &mut text)?;
    }
    let _allocation = LocalAllocation(text.0.cast());
    // SAFETY: the conversion returns allocated, terminated UTF-16.
    Ok(unsafe { text.to_string() }?)
}

fn current_sid() -> Result<String> {
    let mut handle = HANDLE::default();
    // SAFETY: valid process pseudo-handle and initialized writable output.
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle)?;
    }
    let token = Token(handle);
    let mut bytes = 0;
    // SAFETY: this first call asks only for the required size.
    unsafe {
        let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut bytes);
    }
    if !(size_of::<TOKEN_USER>() as u32..=65536).contains(&bytes) {
        return Err("invalid token information size".into());
    }
    let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
    // SAFETY: buffer is sufficiently sized/aligned; success initializes the
    // TOKEN_USER and trailing SID, which stay alive through conversion.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            bytes,
            &mut bytes,
        )?;
        sid_string((*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid)
    }
}

pub(crate) fn hold_parents(path: &Path) -> Result<Vec<File>> {
    match path.components().next() {
        Some(Component::Prefix(p))
            if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) => {}
        _ => {
            return Err(
                "state requires an absolute local drive path; UNC paths are refused".into(),
            );
        }
    }
    path.ancestors()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(open_directory)
        .collect()
}

pub(crate) fn create_directory(path: &Path) -> Result<()> {
    create_directory_mode(path, true)
}

pub(crate) fn create_new_directory(path: &Path) -> Result<()> {
    create_directory_mode(path, false)
}

fn create_directory_mode(path: &Path, allow_existing: bool) -> Result<()> {
    let sid = current_sid()?;
    // Protected DACL, with inheritable full control for this account and SYSTEM.
    let sddl = HSTRING::from(format!(
        "O:{sid}G:{sid}D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)"
    ));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: stable terminated input and valid out parameter; revision 1.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, 1, &mut descriptor, None)?;
    }
    let _allocation = LocalAllocation(descriptor.0);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    // SAFETY: the path, descriptor and attributes live throughout this call.
    match unsafe { CreateDirectoryW(&HSTRING::from(path.as_os_str()), Some(&attributes)) } {
        Ok(()) => Ok(()),
        Err(e) if allow_existing && e.code() == HRESULT::from_win32(ERROR_ALREADY_EXISTS.0) => {
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn open_new_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?)
}
pub(crate) fn open_read_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?)
}

pub(crate) fn same_file(left: &File, right: &File) -> Result<bool> {
    fn identity(file: &File) -> Result<(u32, u32, u32)> {
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: File owns the live handle; information is initialized/writable.
        unsafe {
            GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information)?;
        }
        Ok((
            information.dwVolumeSerialNumber,
            information.nFileIndexHigh,
            information.nFileIndexLow,
        ))
    }
    Ok(identity(left)? == identity(right)?)
}

pub(crate) fn open_directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .access_mode(READ_CONTROL.0 | FILE_READ_ATTRIBUTES.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err("state path must contain only real directories, with no reparse points".into());
    }
    Ok(file)
}

pub(crate) fn open_file(path: &Path, create: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    if create {
        match options.clone().create_new(true).open(path) {
            Ok(file) => return Ok(file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(options.open(path)?)
}

pub(crate) fn verify_local(file: &File, path: &Path) -> Result<()> {
    let root = path.ancestors().last().ok_or("missing state drive root")?;
    // SAFETY: root is terminated, and File keeps its valid handle alive.
    if unsafe { GetDriveTypeW(&HSTRING::from(root.as_os_str())) } != 3 {
        // DRIVE_FIXED
        return Err("state requires a local fixed drive".into());
    }
    let mut name = [0u16; 32];
    // SAFETY: valid live handle, bounded writable buffer, unused outputs null.
    unsafe {
        GetVolumeInformationByHandleW(
            HANDLE(file.as_raw_handle()),
            None,
            None,
            None,
            None,
            Some(&mut name),
        )?;
    }
    let length = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    if !String::from_utf16_lossy(&name[..length]).eq_ignore_ascii_case("NTFS") {
        return Err("state currently requires NTFS on Windows".into());
    }
    Ok(())
}

pub(crate) fn verify_private(file: &File, directory: bool) -> Result<()> {
    let metadata = file.metadata()?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err("state entry has an unexpected type or is a reparse point".into());
    }
    let handle = HANDLE(file.as_raw_handle());
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: file keeps the handle alive; information is writable and initialized.
    unsafe {
        GetFileInformationByHandle(handle, &mut information)?;
    }
    if !directory && information.nNumberOfLinks != 1 {
        return Err("state file hard links are refused".into());
    }
    let mut owner = PSID::default();
    let mut acl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: valid handle and writable output pointers. The returned owner/ACL
    // point into the allocated descriptor retained until validation completes.
    unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            Some(&mut acl),
            None,
            Some(&mut descriptor),
        )
        .ok()?;
    }
    let _allocation = LocalAllocation(descriptor.0);
    let current = current_sid()?;
    // SYSTEM and Administrators are privileged local principals, never an
    // isolation boundary. All ordinary accounts other than the owner are denied.
    let allowed = |sid: &str| sid == current || sid == "S-1-5-18" || sid == "S-1-5-32-544";
    if owner.0.is_null() || !allowed(&sid_string(owner)?) || acl.is_null() {
        return Err("state entry must have a private owner and non-null DACL".into());
    }
    let mut account_full_control = false;
    // SAFETY: GetSecurityInfo returned a valid ACL. GetAce bounds each index;
    // header type/size are checked before reading ACCESS_ALLOWED_ACE/SID fields.
    unsafe {
        for index in 0..u32::from((*acl).AceCount) {
            let mut ace = std::ptr::null_mut();
            windows::Win32::Security::GetAce(acl, index, &mut ace)?;
            let header = &*ace.cast::<ACE_HEADER>();
            if header.AceType != 0 || usize::from(header.AceSize) < size_of::<ACCESS_ALLOWED_ACE>()
            {
                // ACCESS_ALLOWED_ACE_TYPE
                return Err("state DACL contains an unsupported access rule".into());
            }
            let entry = &*ace.cast::<ACCESS_ALLOWED_ACE>();
            let sid = sid_string(PSID(std::ptr::addr_of!(entry.SidStart).cast_mut().cast()))?;
            if !allowed(&sid) {
                return Err("state DACL grants access to another account".into());
            }
            if sid == current && header.AceFlags & 8 == 0 // INHERIT_ONLY_ACE
                && entry.Mask & FILE_ALL_ACCESS.0 == FILE_ALL_ACCESS.0
            {
                account_full_control = true;
            }
        }
    }
    if !account_full_control {
        return Err("state entry must grant this account full control".into());
    }
    Ok(())
}
