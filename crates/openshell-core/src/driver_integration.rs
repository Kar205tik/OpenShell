// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Small composition contract for embedded drivers, independent of gateway internals.
use crate::proto::compute::v1::{WatchSandboxesEvent, compute_driver_server::ComputeDriver};
use std::{path::PathBuf, pin::Pin, sync::Arc};

pub type DriverWatchStream =
    Pin<Box<dyn tokio_stream::Stream<Item = Result<WatchSandboxesEvent, tonic::Status>> + Send>>;
pub type SharedComputeDriver =
    Arc<dyn ComputeDriver<WatchSandboxesStream = DriverWatchStream> + Send + Sync>;

pub struct GatewayConnection {
    pub endpoint: String,
    pub tls: Option<(PathBuf, PathBuf, PathBuf)>,
}

#[async_trait::async_trait]
pub trait InProcessDriverIntegration: Send + Sync + 'static {
    type Config: Default + serde::de::DeserializeOwned + Send;
    const NAME: &'static str;
    const LOCAL_SINGLEPLAYER: bool = false;
    fn validate(config: &Self::Config) -> crate::Result<()>;
    fn audit_log_directory() -> Option<PathBuf> {
        None
    }
    async fn build(
        config: Self::Config,
        gateway: GatewayConnection,
    ) -> crate::Result<SharedComputeDriver>;
}
