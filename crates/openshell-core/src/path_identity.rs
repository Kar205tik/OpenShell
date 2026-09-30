// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Platform path matching and case-sensitivity evidence.

use std::path::Path;
/// Return the stable representation used only for network-policy path matching.
///
/// Windows paths accept either path separator. Namespace paths retain their exact
/// spelling, and other platforms retain exact matching. This lexical operation
/// never probes the filesystem, so it is also safe for policy-authored paths.
pub fn network_binary_match_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    #[cfg(target_os = "windows")]
    {
        windows_network_binary_match_path(&path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.into_owned()
    }
}

#[cfg(any(windows, test))]
fn windows_network_binary_match_path(path: &str) -> String {
    if is_windows_namespace_path(path) {
        return path.to_owned();
    }
    path.replace('\\', "/")
}

#[cfg(any(windows, test))]
fn is_windows_namespace_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && matches!(bytes[0], b'/' | b'\\') && matches!(bytes[1], b'/' | b'\\')
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct NetworkBinaryPathEvidence {
    pub path: String,
    /// Present only when trusted runtime evidence confirms that every parent
    /// directory uses case-insensitive lookup.
    pub ascii_case_folded: String,
}

pub fn network_binary_path_evidence(path: &Path) -> NetworkBinaryPathEvidence {
    let normalized = network_binary_match_path(path);
    #[cfg(target_os = "windows")]
    let ascii_case_folded = windows_path_components_case_insensitive(path)
        .filter(|case_insensitive| *case_insensitive)
        .map_or_else(String::new, |_| normalized.to_ascii_lowercase());
    #[cfg(not(target_os = "windows"))]
    let ascii_case_folded = String::new();

    NetworkBinaryPathEvidence {
        path: normalized,
        ascii_case_folded,
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn windows_directory_case_sensitive(path: &Path) -> Option<bool> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_CASE_SENSITIVE_INFO, FileCaseSensitiveInfo, GetFileInformationByHandleEx,
    };

    let handle = open_windows_directory_for_attributes(path)?;
    let mut info = FILE_CASE_SENSITIVE_INFO::default();
    let info_size = u32::try_from(size_of::<FILE_CASE_SENSITIVE_INFO>()).ok()?;
    // SAFETY: `handle` stays alive for the call and `info` is the exact buffer
    // type and size required by `FileCaseSensitiveInfo`.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(handle.as_raw_handle()),
            FileCaseSensitiveInfo,
            (&raw mut info).cast(),
            info_size,
        )
    }
    .ok()?;
    Some(info.Flags & 1 != 0)
}

#[cfg(target_os = "windows")]
fn windows_path_components_case_insensitive(path: &Path) -> Option<bool> {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if text.contains(['*', '?'])
        || is_windows_namespace_path(&text)
        || bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'/' | b'\\')
    {
        return None;
    }

    let parent = path.parent()?;
    for directory in parent.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        if windows_directory_case_sensitive(directory)? {
            return Some(false);
        }
    }
    Some(true)
}

#[cfg(target_os = "windows")]
fn open_windows_directory_for_attributes(path: &Path) -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    };

    std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_binary_match_path_normalizes_case_and_separators() {
        let normalized = windows_network_binary_match_path(r"C:\WINDOWS\SYSTEM32\CURL.EXE");
        assert_eq!(normalized, "C:/WINDOWS/SYSTEM32/CURL.EXE");
        assert_ne!(
            normalized,
            windows_network_binary_match_path(r"C:\Windows\System32\powershell.exe")
        );
    }

    #[test]
    fn windows_binary_match_path_preserves_namespace_paths_without_probing() {
        for namespace_path in [
            r"\\?\C:\Windows\System32\CURL.EXE",
            r"\\.\PhysicalDrive0",
            r"\\server\share\tool.exe",
        ] {
            assert_eq!(
                windows_network_binary_match_path(namespace_path),
                namespace_path
            );
        }
    }
}
