// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! MXC-owned native boundary and supervisor adapter for the `IsolationBackend` contract.
use openshell_isolation_interface::contract::{
    BackendError, BoundaryConfirmation, NetworkMediationSource,
};
use openshell_sandbox_backend::{
    RuntimeAdapter,
    boundary_protocol::{BoundaryConfig, SandboxRuntimeDescriptor},
};
use std::sync::Arc;

pub const BACKEND_NAME: &str = "mxc";

/// Protected host configuration. Proxy credentials are deliberately omitted from Debug.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MxcRuntimeDescriptor {
    pub runtime: SandboxRuntimeDescriptor,
    pub proxy_addr: std::net::SocketAddr,
    pub proxy_authorization: String,
}

/// Protected native helper configuration.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MxcBoundaryConfig {
    pub runtime: BoundaryConfig,
    pub direct_proxy_url: Option<String>,
}
impl std::ops::Deref for MxcBoundaryConfig {
    type Target = BoundaryConfig;
    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

#[derive(Debug)]
pub struct MxcRuntimeAdapter;
impl RuntimeAdapter for MxcRuntimeAdapter {
    fn backend_name(&self) -> &str {
        BACKEND_NAME
    }
    fn decode_descriptor(&self, payload: &[u8]) -> Result<SandboxRuntimeDescriptor, BackendError> {
        let descriptor: MxcRuntimeDescriptor =
            serde_json::from_slice(payload).map_err(|e| BackendError::Descriptor(e.to_string()))?;
        Ok(descriptor.runtime)
    }
    fn validate_confirmation(
        &self,
        _confirmation: &BoundaryConfirmation,
    ) -> Result<(), BackendError> {
        // TODO(mxc-confirmation): validate native filesystem, egress, attribution,
        // token and controller-loss evidence before accepting confirmation.
        tracing::warn!("MXC enforcement confirmation is UNVERIFIED (temporary TODO)");
        Ok(())
    }
    fn network_mediation_source(
        &self,
        payload: &[u8],
    ) -> Result<Option<Arc<dyn NetworkMediationSource>>, BackendError> {
        native_source(payload).map(Some)
    }
}

#[cfg(windows)]
mod boundary;
#[cfg(windows)]
mod ingress;
#[cfg(windows)]
mod windows_process;
#[cfg(windows)]
fn native_source(payload: &[u8]) -> Result<Arc<dyn NetworkMediationSource>, BackendError> {
    ingress::source(payload)
}
#[cfg(not(windows))]
fn native_source(_payload: &[u8]) -> Result<Arc<dyn NetworkMediationSource>, BackendError> {
    Err(BackendError::Unavailable("MXC requires Windows".into()))
}

/// Start the MXC workload-side protocol service.
#[cfg(windows)]
pub use boundary::run_boundary;
#[cfg(not(windows))]
pub fn run_boundary(_path: &std::path::Path) -> Result<(), String> {
    Err("MXC requires Windows".into())
}

#[cfg(windows)]
mod process_job;
