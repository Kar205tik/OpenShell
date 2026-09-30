// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! First-party isolation backend composition, independent of supervisor service logic.
use openshell_isolation_interface::contract::BackendError;
use openshell_sandbox_backend::{NativeRuntimeAdapter, RuntimeAdapter};
use std::sync::Arc;

pub fn adapter(name: &str) -> Result<Arc<dyn RuntimeAdapter>, BackendError> {
    let adapters: [Arc<dyn RuntimeAdapter>; 2] = [
        Arc::new(NativeRuntimeAdapter),
        Arc::new(openshell_mxc_isolation_backend::MxcRuntimeAdapter),
    ];
    adapters
        .into_iter()
        .find(|adapter| adapter.backend_name() == name)
        .ok_or_else(|| BackendError::Descriptor(format!("unregistered isolation backend {name:?}")))
}
