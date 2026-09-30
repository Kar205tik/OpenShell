// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{LocalStream, Path, io};
use miette::{IntoDiagnostic as _, WrapErr as _};
use std::os::unix::fs::{
    DirBuilderExt as _, FileTypeExt as _, MetadataExt as _, PermissionsExt as _,
};

pub(super) type Listener = tokio::net::UnixListener;

fn runtime_path(path: &Path) -> std::borrow::Cow<'_, Path> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
        if let Some(name) = path.as_os_str().as_bytes().strip_prefix(b"@") {
            let mut bytes = vec![0];
            bytes.extend_from_slice(name);
            return std::borrow::Cow::Owned(std::ffi::OsString::from_vec(bytes).into());
        }
    }
    std::borrow::Cow::Borrowed(path)
}

pub(super) fn bind(path: &Path, shared: bool) -> io::Result<Listener> {
    let runtime = runtime_path(path);
    let abstract_socket = matches!(runtime, std::borrow::Cow::Owned(_));
    if !abstract_socket {
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(if shared { 0o770 } else { 0o700 })
                .create(parent)?;
        }
        match std::fs::symlink_metadata(path) {
            Ok(metadata)
                if metadata.file_type().is_socket()
                    && metadata.uid() == rustix::process::getuid().as_raw() =>
            {
                std::fs::remove_file(path)?;
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "unsafe existing local endpoint",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    let listener = Listener::bind(runtime.as_ref())?;
    if !abstract_socket {
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if shared { 0o660 } else { 0o600 }),
        )?;
    }
    Ok(listener)
}

pub(super) async fn connect(path: &Path) -> io::Result<LocalStream> {
    let stream = tokio::net::UnixStream::connect(runtime_path(path).as_ref()).await?;
    let peer_pid = stream
        .peer_cred()?
        .pid()
        .and_then(|pid| u32::try_from(pid).ok());
    Ok(LocalStream {
        inner: Box::new(stream),
        peer_pid,
    })
}

pub(super) async fn accept(listener: &Listener) -> io::Result<LocalStream> {
    let (stream, _) = listener.accept().await?;
    let peer_pid = stream
        .peer_cred()?
        .pid()
        .and_then(|pid| u32::try_from(pid).ok());
    Ok(LocalStream {
        inner: Box::new(stream),
        peer_pid,
    })
}

pub(super) fn prepare_readiness_path(path: &Path) -> miette::Result<()> {
    use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};

    if !path.is_absolute() {
        return Err(miette::miette!(
            "supervisor readiness socket path must be absolute"
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err_with(|| format!("create readiness directory {}", parent.display()))?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_socket()
                || metadata.uid() != rustix::process::getuid().as_raw()
            {
                return Err(miette::miette!(
                    "refusing unsafe existing readiness path {}",
                    path.display()
                ));
            }
            std::fs::remove_file(path)
                .into_diagnostic()
                .wrap_err_with(|| format!("remove stale readiness socket {}", path.display()))?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .into_diagnostic()
                .wrap_err_with(|| format!("inspect readiness path {}", path.display()));
        }
    }
    Ok(())
}

pub(super) fn check_readiness(path: &Path) -> io::Result<()> {
    std::os::unix::net::UnixStream::connect(path).map(drop)
}

pub(super) fn remove_endpoint(path: &Path) -> io::Result<()> {
    if matches!(runtime_path(path), std::borrow::Cow::Owned(_)) {
        return Ok(());
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_socket()
                && metadata.uid() == rustix::process::getuid().as_raw() =>
        {
            std::fs::remove_file(path)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe existing local endpoint",
        )),
    }
}
