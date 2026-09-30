// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use super::{LocalStream, Path, io};
use std::os::windows::io::AsRawHandle as _;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PWSTR};

pub(super) struct Listener {
    name: String,
    pending: tokio::sync::Mutex<NamedPipeServer>,
}

struct LocalAllocation(*mut core::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: the security APIs return LocalAlloc-owned allocations.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0)));
        }
    }
}

fn pipe_name(path: &Path) -> String {
    use sha2::{Digest as _, Sha256};
    // A path is a stable local endpoint identifier, never a network destination.
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    format!(r"\\.\pipe\openshell-{:x}", digest)
}

fn create(name: &str, first: bool) -> io::Result<NamedPipeServer> {
    // SAFETY: token and aligned buffer live through the query; all returned
    // allocations and the token handle are released on every path.
    let descriptor = unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token)
            .map_err(io::Error::other)?;
        let mut needed = 0;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &raw mut needed);
        if (needed as usize) < size_of::<TOKEN_USER>() {
            let _ = CloseHandle(token);
            return Err(io::Error::other("invalid token information size"));
        }
        let mut buffer = vec![0_u64; (needed as usize).div_ceil(size_of::<u64>())];
        let result = GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &raw mut needed,
        );
        let _ = CloseHandle(token);
        result.map_err(io::Error::other)?;
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut sid = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &raw mut sid).map_err(io::Error::other)?;
        let _sid_guard = LocalAllocation(sid.0.cast());
        let sid = sid.to_string().map_err(io::Error::other)?;
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{sid})"));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, 1, &raw mut descriptor, None)
            .map_err(io::Error::other)?;
        descriptor
    };
    let _descriptor_guard = LocalAllocation(descriptor.0);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    // SAFETY: attributes and its security descriptor remain alive for creation.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&raw const attributes).cast_mut().cast())
    }
}

pub(super) fn bind(path: &Path, _shared: bool) -> io::Result<Listener> {
    let name = pipe_name(path);
    let pending = create(&name, true)?;
    Ok(Listener {
        name,
        pending: tokio::sync::Mutex::new(pending),
    })
}

pub(super) async fn accept(listener: &Listener) -> io::Result<LocalStream> {
    let mut pending = listener.pending.lock().await;
    pending.connect().await?;
    let replacement = create(&listener.name, false)?;
    let stream = std::mem::replace(&mut *pending, replacement);
    let mut pid = 0;
    // SAFETY: Tokio owns a connected, live named-pipe handle.
    unsafe { GetNamedPipeClientProcessId(HANDLE(stream.as_raw_handle()), &raw mut pid) }
        .map_err(io::Error::other)?;
    Ok(LocalStream {
        inner: Box::new(stream),
        peer_pid: Some(pid),
    })
}

pub(super) async fn connect(path: &Path) -> io::Result<LocalStream> {
    let name = pipe_name(path);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let stream = loop {
        match ClientOptions::new().open(&name) {
            Ok(stream) => break stream,
            Err(error)
                if error.raw_os_error() == Some(231) && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Err(error) => return Err(error),
        }
    };
    let mut pid = 0;
    // SAFETY: Tokio owns a connected, live named-pipe handle.
    unsafe { GetNamedPipeServerProcessId(HANDLE(stream.as_raw_handle()), &raw mut pid) }
        .map_err(io::Error::other)?;
    Ok(LocalStream {
        inner: Box::new(stream),
        peer_pid: Some(pid),
    })
}

pub(super) fn prepare_readiness_path(path: &Path) -> miette::Result<()> {
    if !path.is_absolute() {
        return Err(miette::miette!("readiness path must be absolute"));
    }
    Ok(())
}
pub(super) fn check_readiness(path: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(pipe_name(path))
        .map(drop)
}

pub(super) fn remove_endpoint(_path: &Path) -> io::Result<()> {
    Ok(())
}
