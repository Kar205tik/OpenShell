// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    let result = std::env::args_os()
        .nth(1)
        .ok_or_else(|| "boundary configuration path is required".to_string())
        .and_then(|path| {
            openshell_mxc_isolation_backend::run_boundary(std::path::Path::new(&path))
        });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
