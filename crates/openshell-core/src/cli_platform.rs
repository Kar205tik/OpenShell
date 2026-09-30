// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Platform adapters for CLI process startup and desktop integration.

use std::future::Future;

pub fn run_main<F: Future<Output = miette::Result<()>> + 'static>(
    run: fn() -> F,
) -> miette::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::thread::Builder::new()
            .name("openshell-main".to_string())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || run_runtime(run))
            .map_err(|err| miette::miette!("failed to start OpenShell main thread: {err}"))?
            .join()
            .map_err(|_| miette::miette!("OpenShell main thread panicked"))?
    }
    #[cfg(not(target_os = "windows"))]
    {
        run_runtime(run)
    }
}

fn run_runtime<F: Future<Output = miette::Result<()>>>(run: fn() -> F) -> miette::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| miette::miette!("failed to build Tokio runtime: {err}"))?
        .block_on(run())
}

/// Open a URL in the default browser.
pub fn open_browser_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("failed to run `open`: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("failed to run `xdg-open`: {e}"))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
            .map_err(|e| format!("failed to open browser: {e}"))?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        return Err("unsupported platform for browser opening".to_string());
    }

    Ok(())
}
