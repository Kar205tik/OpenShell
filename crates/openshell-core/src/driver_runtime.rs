// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Small runtime integration contracts shared by compute drivers and services.

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// A connected byte stream. Authentication and transport setup belong to its owner.
trait DriverStream: AsyncRead + AsyncWrite + Send {}
impl<T: AsyncRead + AsyncWrite + Send> DriverStream for T {}

/// A stream and the resources that must remain alive until it is dropped.
pub struct DriverConnection {
    stream: Pin<Box<dyn DriverStream>>,
    _lifetime: Box<dyn Send>,
}

impl DriverConnection {
    pub fn new(
        stream: impl AsyncRead + AsyncWrite + Send + 'static,
        lifetime: impl Send + 'static,
    ) -> Self {
        Self {
            stream: Box::pin(stream),
            _lifetime: Box::new(lifetime),
        }
    }
}

impl AsyncRead for DriverConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.stream.as_mut().poll_read(cx, buf)
    }
}

impl AsyncWrite for DriverConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.stream.as_mut().poll_write(cx, buf)
    }
    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        self.stream.as_mut().poll_write_vectored(cx, bufs)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.stream.as_mut().poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.stream.as_mut().poll_shutdown(cx)
    }
}

/// Optional forwarding capability for runtimes that provide their own transport.
#[async_trait::async_trait]
pub trait ComputeDriverForwardSink: Send + Sync {
    /// Open an authenticated connection and attach its cleanup lifetime.
    async fn open_dynamic_forward(
        &self,
        sandbox_id: &str,
        target_port: u16,
    ) -> Result<DriverConnection, String>;
}
