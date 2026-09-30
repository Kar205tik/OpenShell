// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use openshell_core::driver_integration::{
    GatewayConnection, InProcessDriverIntegration, SharedComputeDriver,
};

pub struct MxcIntegration;
#[tonic::async_trait]
impl InProcessDriverIntegration for MxcIntegration {
    type Config = crate::MxcComputeConfig;
    const NAME: &'static str = "mxc";
    const LOCAL_SINGLEPLAYER: bool = true;
    fn validate(config: &Self::Config) -> openshell_core::Result<()> {
        config.validate_configuration()
    }
    fn audit_log_directory() -> Option<std::path::PathBuf> {
        crate::audit_log_directory()
    }
    async fn build(
        config: Self::Config,
        gateway: GatewayConnection,
    ) -> openshell_core::Result<SharedComputeDriver> {
        let endpoint = if config.grpc_endpoint.is_empty() {
            gateway.endpoint
        } else {
            config.grpc_endpoint.clone()
        };
        let backend =
            crate::MxcComputeBackend::new_with_gateway(config, endpoint, gateway.tls, None);
        Ok(std::sync::Arc::new(crate::ComputeDriverService::new(
            backend,
        )))
    }
}
