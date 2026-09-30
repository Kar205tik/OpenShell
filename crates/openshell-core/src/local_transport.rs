// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Private local control transport. Native socket/pipe details stay below services.

use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[cfg(unix)]
#[path = "local_transport/unix.rs"]
mod native;
#[cfg(windows)]
#[path = "local_transport/windows.rs"]
mod native;

trait Stream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Stream for T {}

pub struct LocalStream {
    inner: Box<dyn Stream>,
    peer_pid: Option<u32>,
}

impl LocalStream {
    pub async fn connect(path: &Path) -> io::Result<Self> {
        native::connect(path).await
    }

    #[must_use]
    pub fn peer_pid(&self) -> Option<u32> {
        self.peer_pid
    }
}

impl AsyncRead for LocalStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_read(cx, buf)
    }
}
impl AsyncWrite for LocalStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_shutdown(cx)
    }
}

pub struct LocalListener(native::Listener);
impl LocalListener {
    /// Bind an owner-only endpoint; `shared` retains the Unix group-access mode.
    pub fn bind(path: &Path, shared: bool) -> io::Result<Self> {
        native::bind(path, shared).map(Self)
    }
    pub async fn accept(&self) -> io::Result<(LocalStream, ())> {
        native::accept(&self.0).await.map(|stream| (stream, ()))
    }
}

/// Validate and prepare a private readiness endpoint.
pub fn prepare_readiness_path(path: &Path) -> miette::Result<()> {
    native::prepare_readiness_path(path)
}
/// Probe the local readiness endpoint without starting an async runtime.
pub fn check_readiness(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "readiness path must be absolute",
        ));
    }
    native::check_readiness(path)
}

/// Remove a filesystem endpoint, if the native transport creates one.
pub fn remove_endpoint(path: &Path) -> io::Result<()> {
    native::remove_endpoint(path)
}
