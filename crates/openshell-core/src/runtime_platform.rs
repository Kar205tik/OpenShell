// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Platform support for portable service runtimes.

use miette::{IntoDiagnostic as _, Result, WrapErr as _};
use std::io::Write as _;

pub struct PreparedNetworkProxyTlsDir {
    pub path: std::path::PathBuf,
    _temporary: Option<tempfile::TempDir>,
}

pub fn prepare_network_proxy_tls_dir(
    requested: Option<std::path::PathBuf>,
) -> Result<PreparedNetworkProxyTlsDir> {
    let Some(requested) = requested else {
        let mut builder = tempfile::Builder::new();
        builder.prefix("openshell-supervisor-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let temporary = builder
            .tempdir()
            .into_diagnostic()
            .wrap_err("create private network-proxy TLS directory")?;
        crate::paths::set_dir_owner_only(temporary.path())?;
        return Ok(PreparedNetworkProxyTlsDir {
            path: temporary.path().to_path_buf(),
            _temporary: Some(temporary),
        });
    };

    match std::fs::symlink_metadata(&requested) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(miette::miette!(
                "network-proxy TLS directory must not be a symlink: {}",
                requested.display()
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(miette::miette!(
                "network-proxy TLS path is not a directory: {}",
                requested.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;

                let mut builder = std::fs::DirBuilder::new();
                builder.mode(0o700);
                builder
                    .create(&requested)
                    .into_diagnostic()
                    .wrap_err_with(|| {
                        format!(
                            "create private network-proxy TLS directory {}",
                            requested.display()
                        )
                    })?;
            }
            #[cfg(not(unix))]
            crate::paths::create_dir_restricted(&requested).wrap_err_with(|| {
                format!(
                    "create private network-proxy TLS directory {}",
                    requested.display()
                )
            })?;
        }
        Err(error) => return Err(error).into_diagnostic(),
    }

    let path = requested
        .canonicalize()
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "resolve network-proxy TLS directory {}",
                requested.display()
            )
        })?;
    validate_network_proxy_tls_dir(&path)?;
    Ok(PreparedNetworkProxyTlsDir {
        path,
        _temporary: None,
    })
}

#[cfg(unix)]
pub fn validate_network_proxy_tls_dir(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt as _;

    let effective_uid = nix::unistd::geteuid().as_raw();
    for (index, component) in path.ancestors().enumerate() {
        let metadata = std::fs::metadata(component)
            .into_diagnostic()
            .wrap_err_with(|| format!("inspect TLS directory component {}", component.display()))?;
        let mode = metadata.mode();
        if !metadata.is_dir() {
            return Err(miette::miette!(
                "TLS directory component is not a directory: {}",
                component.display()
            ));
        }
        if metadata.uid() != 0 && metadata.uid() != effective_uid {
            return Err(miette::miette!(
                "TLS directory component is owned by an untrusted user: {}",
                component.display()
            ));
        }
        if index == 0 {
            if metadata.uid() != effective_uid || mode & 0o022 != 0 {
                return Err(miette::miette!(
                    "network-proxy TLS directory must be owned by the current user and not group- or world-writable: {}",
                    component.display()
                ));
            }
        } else if mode & 0o022 != 0 && mode & 0o1000 == 0 {
            return Err(miette::miette!(
                "TLS directory has an untrusted writable ancestor: {}",
                component.display()
            ));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn validate_network_proxy_tls_dir(path: &std::path::Path) -> Result<()> {
    if !path.is_dir() || crate::paths::is_file_permissions_too_open(path) {
        return Err(miette::miette!(
            "network-proxy TLS path is not a directory: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub async fn wait_for_control_shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigterm = signal(SignalKind::terminate()).expect("install control SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("install control SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }
}

#[cfg(not(unix))]
pub async fn wait_for_control_shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

pub fn persist_main_exit_marker(path: &std::path::Path, exit_code: i32) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("completion marker has no parent: {}", path.display()),
            )
        })?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("completion marker has no file name: {}", path.display()),
        )
    })?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    writeln!(file, "exit_code={exit_code}")?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
#[allow(unsafe_code)]
pub fn arm_parent_liveness(raw_fd: Option<i32>) -> Result<()> {
    use std::io::Read as _;
    use std::os::fd::{FromRawFd as _, OwnedFd};

    let Some(raw_fd) = raw_fd else {
        return Ok(());
    };
    if raw_fd <= 2 {
        return Err(miette::miette!("parent liveness descriptor is invalid"));
    }
    nix::fcntl::fcntl(raw_fd, nix::fcntl::FcntlArg::F_GETFD)
        .map_err(|error| miette::miette!("parent liveness descriptor is not open: {error}"))?;
    // SAFETY: the driver transfers this inherited descriptor to the
    // supervisor exactly once through the private command line.
    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    std::thread::Builder::new()
        .name("supervisor-parent-liveness".to_string())
        .spawn(move || {
            let mut stream = std::fs::File::from(fd);
            let mut byte = [0_u8; 1];
            loop {
                match stream.read(&mut byte) {
                    Ok(0) | Err(_) => std::process::exit(1),
                    Ok(_) => {}
                }
            }
        })
        .map(|_| ())
        .into_diagnostic()
}

#[cfg(not(unix))]
pub fn arm_parent_liveness(raw_fd: Option<i32>) -> Result<()> {
    if raw_fd.is_some() {
        return Err(miette::miette!(
            "parent liveness descriptors are unsupported on this platform"
        ));
    }
    Ok(())
}

/// Keep each native supervisor's logs separate while preserving container defaults.
pub fn supervisor_log_directory() -> std::path::PathBuf {
    #[cfg(unix)]
    {
        std::path::PathBuf::from("/var/log")
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("OpenShell")
            .join("supervisor-logs")
            .join(std::process::id().to_string())
    }
}
