//! Audited Windows FFI boundary. No filesystem/network logic belongs here.
#[cfg(test)]
mod disposition_reference;
#[cfg(feature = "native-tests")]
pub mod lifecycle_probe;
#[cfg(test)]
mod replacement_reference;
use std::{ffi::c_void, mem::size_of};
use windows::{
    Wdk::System::SystemServices::RtlUpcaseUnicodeChar,
    Win32::{
        Foundation::{
            CloseHandle, HANDLE, HLOCAL, LocalFree, STATUS_BUFFER_OVERFLOW, STATUS_INTERNAL_ERROR,
        },
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            },
            GetTokenInformation, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::{
            DELETE, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
            GetVolumeInformationW,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    },
    core::{HSTRING, PWSTR},
};
use winfsp::Result;

pub fn is_ntfs(root: &str) -> Result<bool> {
    let mut name = [0u16; 32];
    // SAFETY: root is a stable NUL-terminated drive root and name is a writable
    // buffer. Optional outputs are unused; the API checks its supplied length.
    unsafe {
        GetVolumeInformationW(
            &HSTRING::from(root),
            None,
            None,
            None,
            None,
            Some(&mut name),
        )?;
    }
    let length = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    Ok(String::from_utf16_lossy(&name[..length]).eq_ignore_ascii_case("NTFS"))
}

struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: this owns the valid handle returned by OpenProcessToken.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: both conversion APIs document LocalFree ownership.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0)));
        }
    }
}

/// NT's one-code-unit uppercase mapping, matching WinFsp's case comparison.
/// Do not substitute Rust's Unicode expansion or a locale-dependent mapping.
pub fn name_key(name: &str) -> Vec<u16> {
    name.encode_utf16()
        .map(|c| {
            // SAFETY: this pure Windows mapping accepts every UTF-16 code unit.
            unsafe { RtlUpcaseUnicodeChar(c) }
        })
        .collect()
}

/// Refuse writable portable namespaces on an incompatible host upcase table.
/// Supplementary scalars are pairs of unchanged surrogate units in this API.
fn portable_mapping_issue() -> Option<u32> {
    for value in 0..=u16::MAX as u32 {
        let Some(c) = char::from_u32(value) else {
            continue;
        };
        let text = c.to_string();
        let Some(key) = s3drive_core::names::collision_key(&text) else {
            continue;
        };
        let Ok(upper) = String::from_utf16(&name_key(&text)) else {
            return Some(value);
        };
        if s3drive_core::names::collision_key(&upper).as_ref() != Some(&key) {
            return Some(value);
        }
    }
    None
}
#[test]
fn portable_policy_covers_native_bmp_upcase_aliases() {
    assert_eq!(
        portable_mapping_issue(),
        None,
        "portable policy must cover host BMP mappings"
    );
}

pub fn portable_mapping_compatible() -> bool {
    portable_mapping_issue().is_none()
}

pub fn owner_descriptor(writable: bool, namespace: bool) -> Result<Vec<u8>> {
    let mut handle = HANDLE::default();
    // SAFETY: output is initialized, process pseudo-handle is valid.
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle)?;
    }
    let token = Token(handle);
    let mut size = 0u32;
    // SAFETY: first call queries size only, without an output buffer.
    unsafe {
        let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut size);
    }
    if !(size_of::<TOKEN_USER>() as u32..=65536).contains(&size) {
        return Err(STATUS_INTERNAL_ERROR.into());
    }
    // usize alignment suffices for TOKEN_USER; allocation includes trailing SID.
    let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    // SAFETY: writable aligned allocation has at least `size` bytes. A successful
    // call initializes TOKEN_USER and its SID, valid until buffer is dropped.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            size,
            &mut size,
        )?;
    }
    let mut sid = PWSTR::null();
    // SAFETY: token query succeeded; SID points into the live buffer.
    unsafe {
        ConvertSidToStringSidW((*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid, &mut sid)?;
    }
    let _sid_allocation = LocalAllocation(sid.0.cast());
    // SAFETY: conversion returns an allocated null-terminated UTF-16 string.
    let sid = unsafe { sid.to_string() }.map_err(|_| STATUS_INTERNAL_ERROR)?;
    // Owner and protected DACL are fixed to the mounting account. Optional
    // write rights add no other SID; callbacks still enforce per-source policy.
    let rights = if writable && namespace {
        format!(
            "0x{:08x}",
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | FILE_GENERIC_EXECUTE.0 | DELETE.0
        )
    } else if writable {
        "FRFWFX".into()
    } else {
        "FRFX".into()
    };
    let sddl = HSTRING::from(format!("O:{sid}G:{sid}D:P(A;;{rights};;;{sid})"));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let mut length = 0;
    // SAFETY: stable NUL-terminated input and valid out parameters. Revision 1
    // requests a self-relative descriptor, so its bytes can be copied safely.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &sddl,
            1,
            &mut descriptor,
            Some(&mut length),
        )?;
    }
    let allocation = LocalAllocation(descriptor.0);
    // SAFETY: API returns exactly `length` initialized bytes; copy before free.
    Ok(unsafe { std::slice::from_raw_parts(allocation.0.cast::<u8>(), length as usize) }.to_vec())
}

pub fn copy_descriptor(bytes: &[u8], output: Option<&mut [c_void]>) -> Result<u64> {
    if let Some(output) = output {
        if output.len() < bytes.len() {
            return Err(STATUS_BUFFER_OVERFLOW.into());
        }
        // SAFETY: WinFsp supplies a byte-sized c_void buffer. The exclusive
        // destination has enough bytes, does not overlap our owned descriptor,
        // and neither pointer escapes. c_void has size 1.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                output.as_mut_ptr().cast::<u8>(),
                bytes.len(),
            );
        }
    }
    Ok(bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_native_case_equivalence_without_unicode_expansion() {
        assert_eq!(name_key("CaFé"), name_key("CAFÉ"));
        assert_ne!(name_key("ß"), name_key("SS"));
        assert_eq!(name_key("a%2Fb"), name_key("A%2fB"));
    }
}
