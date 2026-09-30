// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

#![cfg(windows)]
use openshell_core::proto::{NetworkBinary, NetworkEndpoint, NetworkPolicyRule};
use openshell_supervisor_network::opa::{NetworkInput, OpaEngine};
use std::path::{Path, PathBuf};
const TEST_POLICY: &str =
    include_str!("../../openshell-supervisor-network/data/sandbox-policy.rego");

#[allow(unsafe_code)]
fn set_windows_directory_case_sensitive(path: &Path, enabled: bool) {
    use std::mem::size_of;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_CASE_SENSITIVE_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
        FileCaseSensitiveInfo, SetFileInformationByHandle,
    };

    let handle = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES.0 | FILE_WRITE_ATTRIBUTES.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
        .expect("open temporary directory for case-sensitivity update");
    let info = FILE_CASE_SENSITIVE_INFO {
        Flags: u32::from(enabled),
    };
    let info_size = u32::try_from(size_of::<FILE_CASE_SENSITIVE_INFO>())
        .expect("case-sensitivity info size fits in u32");
    // SAFETY: `handle` and `info` remain alive for the call, and the buffer
    // has the exact type and size required by `FileCaseSensitiveInfo`.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(handle.as_raw_handle()),
            FileCaseSensitiveInfo,
            (&raw const info).cast(),
            info_size,
        )
    }
    .expect("set temporary directory case sensitivity");
}

#[test]
fn windows_binary_matching_denies_case_mismatch_under_sensitive_ancestor() {
    let root = tempfile::tempdir().expect("create temporary directory");
    set_windows_directory_case_sensitive(root.path(), true);
    let containing_directory = root.path().join("TrustedTools");
    std::fs::create_dir(&containing_directory).expect("create binary directory");
    set_windows_directory_case_sensitive(&containing_directory, false);
    let runtime_binary = containing_directory.join("curl.exe");
    std::fs::write(&runtime_binary, b"test executable").expect("create test executable");

    assert!(
        openshell_core::path_identity::network_binary_path_evidence(&runtime_binary)
            .ascii_case_folded
            .is_empty(),
        "a sensitive ancestor must retain exact path matching"
    );

    let policy_binary = runtime_binary
        .to_string_lossy()
        .replace("TrustedTools", "trustedtools");
    assert_ne!(policy_binary, runtime_binary.to_string_lossy());
    let mut proto = openshell_policy::restrictive_default_policy();
    proto.network_policies.insert(
        "windows_sensitive_ancestor".to_string(),
        NetworkPolicyRule {
            name: "windows_sensitive_ancestor".to_string(),
            endpoints: vec![NetworkEndpoint {
                host: "example.com".to_string(),
                port: 443,
                ..Default::default()
            }],
            binaries: vec![NetworkBinary {
                path: policy_binary,
            }],
        },
    );
    let engine = OpaEngine::from_proto(&proto).expect("load Windows policy");
    let decision = engine
        .evaluate_network(&NetworkInput {
            host: "example.com".to_string(),
            port: 443,
            binary_path: runtime_binary,
            binary_sha256: "unused".to_string(),
            ancestors: Vec::new(),
            cmdline_paths: Vec::new(),
        })
        .expect("evaluate Windows policy");

    assert!(
        !decision.allowed,
        "case-distinct paths under a sensitive ancestor must not share grants"
    );
}

#[test]
fn from_proto_matches_windows_equivalent_binary_path() {
    let mut proto = openshell_policy::restrictive_default_policy();
    proto.network_policies.insert(
        "windows_binary".to_string(),
        NetworkPolicyRule {
            name: "windows_binary".to_string(),
            endpoints: vec![NetworkEndpoint {
                host: "example.com".to_string(),
                port: 443,
                ..Default::default()
            }],
            binaries: vec![NetworkBinary {
                path: r"C:\WINDOWS\SYSTEM32\CURL.EXE".to_string(),
            }],
        },
    );
    let engine = OpaEngine::from_proto(&proto).expect("Failed to create engine from proto");

    let equivalent = NetworkInput {
        host: "example.com".into(),
        port: 443,
        binary_path: PathBuf::from("c:/windows/system32/curl.exe"),
        binary_sha256: "unused".into(),
        ancestors: vec![],
        cmdline_paths: vec![],
    };
    let decision = engine.evaluate_network(&equivalent).unwrap();
    assert!(
        decision.allowed,
        "Windows-equivalent binary path should be allowed: {}",
        decision.reason
    );

    let different_binary = NetworkInput {
        binary_path: PathBuf::from("c:/windows/system32/powershell.exe"),
        ..equivalent
    };
    let decision = engine.evaluate_network(&different_binary).unwrap();
    assert!(
        !decision.allowed,
        "normalization must not allow a different binary"
    );
}

#[test]
fn from_strings_matches_windows_equivalent_binary_path() {
    let engine = OpaEngine::from_strings(
        TEST_POLICY,
        r#"
network_policies:
  windows_binary:
    endpoints:
      - { host: example.com, port: 443 }
    binaries:
      - { path: 'C:\WINDOWS\SYSTEM32\CURL.EXE' }
"#,
    )
    .expect("Failed to create engine from YAML");
    let input = NetworkInput {
        host: "example.com".into(),
        port: 443,
        binary_path: PathBuf::from("c:/windows/system32/curl.exe"),
        binary_sha256: "unused".into(),
        ancestors: vec![],
        cmdline_paths: vec![],
    };

    let decision = engine.evaluate_network(&input).unwrap();
    assert!(
        decision.allowed,
        "Windows-equivalent YAML binary path should be allowed: {}",
        decision.reason
    );
}
