// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    MxcRuntimeDescriptor,
    windows_process::{WorkloadProxyTcpConnection, resolve_tcp_peer_identity},
};
use openshell_isolation_interface::contract::*;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::io::{
    AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadBuf,
};

pub(crate) fn source(payload: &[u8]) -> Result<Arc<dyn NetworkMediationSource>, BackendError> {
    let descriptor: MxcRuntimeDescriptor =
        serde_json::from_slice(payload).map_err(|e| BackendError::Descriptor(e.to_string()))?;
    if !descriptor.proxy_addr.ip().is_loopback()
        || descriptor.proxy_addr.port() == 0
        || descriptor.proxy_authorization.is_empty()
    {
        return Err(BackendError::Descriptor(
            "MXC proxy requires a loopback address and authentication".into(),
        ));
    }
    let listener = std::net::TcpListener::bind(descriptor.proxy_addr).map_err(unavailable)?;
    listener.set_nonblocking(true).map_err(unavailable)?;
    let listener = tokio::net::TcpListener::from_std(listener).map_err(unavailable)?;
    let authorization = Arc::new(descriptor.proxy_authorization);
    let (sender, receiver) = tokio::sync::mpsc::channel(128);
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = sender.closed() => return,
                _ = connections.join_next(), if !connections.is_empty() => {},
                accepted = listener.accept(), if connections.len() < 128 => {
                    let (stream, workload_addr) = match accepted {
                        Ok(accepted) => accepted,
                        Err(error) => { let _ = sender.send(Err(unavailable(error))).await; return; }
                    };
                    let authorization = authorization.clone();
                    let sender = sender.clone();
                    connections.spawn(async move {
                        if let Ok(connection) = authenticate_connection(stream, workload_addr, &authorization).await {
                            let _ = sender.send(Ok(connection)).await;
                        }
                    });
                }
            }
        }
    });
    Ok(Arc::new(Ingress {
        receiver: tokio::sync::Mutex::new(receiver),
        task,
    }))
}
fn unavailable(error: impl std::fmt::Display) -> BackendError {
    BackendError::Unavailable(error.to_string())
}
struct Ingress {
    receiver: tokio::sync::Mutex<
        tokio::sync::mpsc::Receiver<Result<PendingProxyConnection, BackendError>>,
    >,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Ingress {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[async_trait::async_trait]
impl NetworkMediationSource for Ingress {
    async fn accept_tcp(&self) -> Result<PendingTcpOpen, BackendError> {
        std::future::pending().await
    }
    async fn accept_dns(&self) -> Result<PendingDnsQuery, BackendError> {
        std::future::pending().await
    }
    async fn accept_proxy(&self) -> Result<PendingProxyConnection, BackendError> {
        self.receiver
            .lock()
            .await
            .recv()
            .await
            .unwrap_or_else(|| Err(unavailable("MXC ingress closed")))
    }
}
async fn authenticate_connection(
    stream: tokio::net::TcpStream,
    workload_addr: std::net::SocketAddr,
    authorization: &str,
) -> Result<PendingProxyConnection, BackendError> {
    openshell_core::net::set_tcp_nodelay_best_effort(&stream);
    let proxy_addr = stream.local_addr().map_err(unavailable)?;
    // Resolve ownership while the accepted socket is alive; hash off the async workers.
    let binary_identity = tokio::task::spawn_blocking(move || {
        resolve_tcp_peer_identity(WorkloadProxyTcpConnection {
            workload: workload_addr,
            proxy: proxy_addr,
        })
        .map_err(|e| ResolveError::Failed(e.to_string()))
        .and_then(|(path, _pid)| {
            use sha2::Digest as _;
            // TODO(mxc-identity): attest the running executable object, including replacement races.
            let mut file =
                std::fs::File::open(&path).map_err(|e| ResolveError::Failed(e.to_string()))?;
            let mut digest = sha2::Sha256::new();
            std::io::copy(&mut file, &mut digest)
                .map_err(|e| ResolveError::Failed(e.to_string()))?;
            let digest = format!("{:x}", digest.finalize()).parse::<Sha256Digest>()?;
            Ok(BinaryIdentity {
                executable: ExecutableIdentity {
                    path,
                    digest: Some(digest),
                },
                ancestors: Vec::new(),
                cmdline_paths: Vec::new(),
            })
        })
    })
    .await
    .map_err(unavailable)?;
    let mut reader = tokio::io::BufReader::new(stream);
    let authenticated = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        authenticate(&mut reader, authorization),
    )
    .await;
    if let Ok(Ok(prefix)) = authenticated {
        return Ok(PendingProxyConnection {
            stream: Box::new(ReplayStream {
                prefix: std::io::Cursor::new(prefix),
                reader,
            }),
            binary_identity,
            workload_addr,
            proxy_addr,
        });
    }
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        reader
            .get_mut()
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nConnection: close\r\n\r\n"),
    )
    .await;
    Err(unavailable("MXC proxy authentication failed"))
}
async fn authenticate(
    reader: &mut tokio::io::BufReader<tokio::net::TcpStream>,
    expected: &str,
) -> std::io::Result<Vec<u8>> {
    let mut prefix = Vec::new();
    let mut authorized = false;
    let mut total = 0;
    loop {
        let mut line = Vec::new();
        (&mut *reader)
            .take((8193 - total) as u64)
            .read_until(b'\n', &mut line)
            .await?;
        total += line.len();
        if total > 8192 || line.is_empty() {
            return Err(std::io::Error::other("invalid proxy headers"));
        }
        if let Some((name, value)) = std::str::from_utf8(&line)
            .ok()
            .and_then(|s| s.split_once(':'))
            && name.eq_ignore_ascii_case("proxy-authorization")
        {
            if authorized || value.trim() != expected {
                return Err(std::io::Error::other("invalid proxy authentication"));
            }
            authorized = true;
        } else {
            prefix.extend_from_slice(&line);
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
    }
    if !authorized {
        return Err(std::io::Error::other("missing proxy authentication"));
    }
    Ok(prefix)
}
struct ReplayStream {
    prefix: std::io::Cursor<Vec<u8>>,
    reader: tokio::io::BufReader<tokio::net::TcpStream>,
}
impl AsyncRead for ReplayStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.prefix.position() < self.prefix.get_ref().len() as u64 {
            Pin::new(&mut self.prefix).poll_read(cx, buf)
        } else {
            Pin::new(&mut self.reader).poll_read(cx, buf)
        }
    }
}
impl AsyncWrite for ReplayStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(self.reader.get_mut()).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(self.reader.get_mut()).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(self.reader.get_mut()).poll_shutdown(cx)
    }
}
