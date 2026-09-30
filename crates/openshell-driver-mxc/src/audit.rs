// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! MXC audit opt-in and native output location.

/// Supply the gateway audit sink location only when explicitly enabled.
pub fn audit_log_directory() -> Option<std::path::PathBuf> {
    let requested = std::env::var("OPENSHELL_OCSF_JSON").ok();
    ocsf_jsonl_requested(requested.as_deref()).then(ocsf_log_dir)
}

/// Whether this gateway explicitly requested the Windows/MXC JSONL sink.
/// Unknown values fail closed so a typo cannot unexpectedly retain audit data.
fn ocsf_jsonl_requested(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "on" | "yes"
        )
    })
}

/// Resolve the directory for the OCSF JSONL audit file.
///
/// Precedence: `OPENSHELL_OCSF_LOG_DIR` (harness / operator override) then
/// `%PROGRAMDATA%\OpenShell\logs`.
fn ocsf_log_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("OPENSHELL_OCSF_LOG_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return std::path::PathBuf::from(trimmed);
        }
    }
    if let Ok(pd) = std::env::var("ProgramData") {
        return std::path::PathBuf::from(pd).join("OpenShell").join("logs");
    }
    std::env::temp_dir().join("openshell").join("logs")
}

#[cfg(test)]
mod tests {
    use super::ocsf_jsonl_requested;
    #[test]
    fn gateway_ocsf_jsonl_requires_explicit_opt_in() {
        assert!(!ocsf_jsonl_requested(None));
        assert!(!ocsf_jsonl_requested(Some("")));
        assert!(!ocsf_jsonl_requested(Some("enabled")));
        for value in ["0", "false", "FALSE", " off ", "no"] {
            assert!(!ocsf_jsonl_requested(Some(value)));
        }

        for value in ["1", "true", "TRUE", " on ", "yes"] {
            assert!(
                ocsf_jsonl_requested(Some(value)),
                "expected {value:?} to opt in"
            );
        }
    }
}
