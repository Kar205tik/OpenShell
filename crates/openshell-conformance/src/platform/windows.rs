// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use super::SmokeFixture;

pub fn smoke_fixture() -> Result<SmokeFixture, String> {
    let temp_dir = tempfile::Builder::new()
        .prefix("openshell-conformance-smoke-")
        .tempdir()
        .map_err(|error| format!("create Windows smoke fixture directory: {error}"))?;
    let workload_dir = temp_dir.path().join("workload");
    fs::create_dir(&workload_dir)
        .map_err(|error| format!("create Windows smoke workload directory: {error}"))?;

    let workload_policy_path = workload_dir
        .to_str()
        .ok_or_else(|| "Windows smoke workload path is not valid Unicode".to_string())?
        .replace('\\', "/")
        .replace('"', "\\\"");
    let policy_path = temp_dir.path().join("policy.yaml");
    let policy = format!(
        "version: 1\nfilesystem_policy:\n  include_workdir: false\n  read_only: []\n  read_write:\n    - \"{workload_policy_path}\"\n"
    );
    fs::write(&policy_path, policy)
        .map_err(|error| format!("write Windows smoke policy: {error}"))?;

    let ready_path = workload_dir.join("ready.txt");
    let command = vec![
        r"C:\Windows\System32\cmd.exe".to_string(),
        "/c".to_string(),
        format!(
            "echo ready > \"{}\" & ping -t 127.0.0.1 > NUL",
            ready_path.display()
        ),
    ];
    let create_args = vec![
        "--policy".to_string(),
        policy_path.to_string_lossy().into_owned(),
    ];

    Ok(SmokeFixture::new(
        create_args,
        Some(command),
        Some(temp_dir),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_fixture_defines_a_long_running_windows_workload() {
        let fixture = smoke_fixture().unwrap();
        let command = fixture.command().unwrap();
        assert_eq!(command[0], r"C:\Windows\System32\cmd.exe");
        assert!(command[2].contains("ready.txt"));
        assert!(command[2].contains("ping -t 127.0.0.1"));

        let policy_path = &fixture.create_args()[1];
        let policy = fs::read_to_string(policy_path).unwrap();
        assert!(policy.contains("read_write:"));
        assert!(
            command[2].contains(
                std::path::Path::new(policy_path)
                    .parent()
                    .unwrap()
                    .join("workload")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }
}
