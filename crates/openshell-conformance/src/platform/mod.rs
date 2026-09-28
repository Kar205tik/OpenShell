// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Target-specific fixtures used by portable conformance scenarios.

use tempfile::TempDir;

#[cfg(unix)]
mod unix;
#[cfg(any(test, target_os = "windows"))]
mod windows;

#[cfg(unix)]
pub use unix::smoke_fixture;
#[cfg(target_os = "windows")]
pub use windows::smoke_fixture;

#[cfg(not(any(unix, target_os = "windows")))]
compile_error!("OpenShell conformance does not support this target platform");

/// Platform-specific inputs needed to keep a smoke-test sandbox alive.
pub struct SmokeFixture {
    create_args: Vec<String>,
    command: Option<Vec<String>>,
    // Keep generated policy and workload paths alive until the scenario ends.
    _temp_dir: Option<TempDir>,
}

impl SmokeFixture {
    fn new(
        create_args: Vec<String>,
        command: Option<Vec<String>>,
        temp_dir: Option<TempDir>,
    ) -> Self {
        Self {
            create_args,
            command,
            _temp_dir: temp_dir,
        }
    }

    pub fn create_args(&self) -> &[String] {
        &self.create_args
    }

    pub fn command(&self) -> Option<&[String]> {
        self.command.as_deref()
    }
}
