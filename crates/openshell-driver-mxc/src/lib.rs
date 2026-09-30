// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! `OpenShell` MXC compute driver.
//!
//! Provisions MXC boundaries and standard host supervisors. Native isolation,
//! process control and ingress live in `openshell-mxc-isolation-backend`.

#![allow(clippy::result_large_err)]

#[cfg(target_os = "windows")]
mod driver;
#[cfg(target_os = "windows")]
mod grpc;
#[cfg(target_os = "windows")]
mod isolation;
#[cfg(target_os = "windows")]
mod mxc;
#[cfg(target_os = "windows")]
mod policy;
// Embedded mapper logic (source of truth; was the `openshell-policy-mapper`
// crate). Windows-only — MXC and the policy mapper are not built for Linux/WSL.
#[cfg(target_os = "windows")]
mod policy_map;
// Real-time ETW → OCSF audit consumer (Plane A). Consumes the OS Sandboxing
// provider MXC drives and emits OCSF through the gateway's tracing sink.
// Windows-only.
#[cfg(target_os = "windows")]
mod etw_consumer;

#[cfg(target_os = "windows")]
pub use driver::{MxcBackend, MxcComputeBackend, MxcComputeConfig};
#[cfg(target_os = "windows")]
pub use grpc::ComputeDriverService;
// Re-export the embedded mapper API so the windows-only example and integration
// test can reach it without making `policy_map` a public module.
#[cfg(target_os = "windows")]
pub use policy::{EmbeddedPolicyMapper, MapCtx, MapError, MappedConfig, PolicyMapper};
#[cfg(target_os = "windows")]
pub use policy_map::{
    DEFAULT_COARSE_MXC_VERSION, DEFAULT_COMMAND, DEFAULT_CONTAINMENT, DEFAULT_MXC_VERSION,
    LossItem, MxcMappingOptions, MxcMappingResult, OPEN_SHELL_SUPERSET_GAPS, SplitPolicyResult,
    build_loss_report, map_to_mxc, render_readme, split_policy,
};

#[cfg(windows)]
mod audit;
#[cfg(windows)]
pub use audit::audit_log_directory;

#[cfg(windows)]
mod integration;
#[cfg(windows)]
pub use integration::MxcIntegration;
