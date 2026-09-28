// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Installed CLI conformance for sandbox policy control-plane behavior.
//!
//! The tests create a portable policy fixture by default. A driver harness may
//! supply its own policy through `OPENSHELL_CONFORMANCE_SMOKE_CREATE_ARGS`, its
//! write grant through `OPENSHELL_CONFORMANCE_POLICY_RW_PATH`, and a workload
//! through `OPENSHELL_CONFORMANCE_SMOKE_COMMAND`. The live-update test requires
//! a runtime that supports live policy replacement; select tests by capability.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use openshell_conformance::OpenShellRunner;
use serde_json::Value;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const CREATE_TIMEOUT: Duration = Duration::from_mins(10);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(90);
const PORTABLE_POLICY: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only: [/usr, /bin, /lib, /lib64, /proc, /dev/urandom, /app, /etc, /var/log]
  read_write: [/sandbox, /tmp, /dev/null]
landlock: { compatibility: best_effort }
network_policies: {}
";

#[derive(Clone, Copy)]
enum Case {
    CreatePolicy,
    History,
    InvalidPolicy,
    LiveUpdate,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::CreatePolicy => "policy-create",
            Self::History => "policy-history",
            Self::InvalidPolicy => "policy-invalid",
            Self::LiveUpdate => "policy-live-update",
        }
    }
}

#[tokio::test]
async fn create_policy_is_loaded() {
    run_case(Case::CreatePolicy).await;
}

#[tokio::test]
async fn initial_policy_has_history() {
    run_case(Case::History).await;
}

#[tokio::test]
async fn invalid_policy_is_rejected() {
    run_case(Case::InvalidPolicy).await;
}

#[tokio::test]
async fn live_policy_update_is_loaded() {
    run_case(Case::LiveUpdate).await;
}

async fn run_case(case: Case) {
    let mut runner = OpenShellRunner::from_env(case.name()).expect("candidate CLI is available");
    let result = execute_case(&mut runner, case).await;
    if let Err(error) = runner.finish(result).await {
        panic!("{} conformance failed:\n{error}", case.name());
    }
}

async fn execute_case(runner: &mut OpenShellRunner, case: Case) -> Result<(), String> {
    runner
        .check_gateway_status()
        .await
        .map_err(|error| error.to_string())?;
    let name = format!("ct-{}-p", runner.id());
    let fixture = if matches!(case, Case::InvalidPolicy) {
        None
    } else {
        Some(policy_fixture(runner.id())?)
    };
    match case {
        Case::CreatePolicy => {
            create(runner, &name, &fixture.as_ref().unwrap().path).await?;
            let current = policy_get(runner, &name, &[]).await?;
            let revision = policy_get(
                runner,
                &name,
                &["--rev", &number(&current, "version")?.to_string()],
            )
            .await?;
            assert_initial_policy(
                &current,
                &revision,
                &name,
                &fixture.as_ref().unwrap().write_grant,
            )?;
        }
        Case::History => {
            create(runner, &name, &fixture.as_ref().unwrap().path).await?;
            let current = policy_get(runner, &name, &[]).await?;
            let version = number(&current, "version")?;
            let revision = policy_get(runner, &name, &["--rev", &version.to_string()]).await?;
            if revision["status"] != "loaded"
                || current["hash"] != revision["hash"]
                || current["policy"] != revision["policy"]
            {
                return Err("initial policy revision differs from the active base policy".into());
            }
            let list = command(
                runner,
                "history",
                &["policy", "list", &name, "--output", "json"],
            )
            .await?;
            let history: Value = list.json().map_err(|error| error.to_string())?;
            let revisions = history["revisions"]
                .as_array()
                .ok_or("policy list has no revisions array")?;
            if !revisions
                .iter()
                .any(|entry| entry["version"] == version && entry["hash"] == current["hash"])
            {
                return Err(format!(
                    "policy history does not contain active revision {version}: {history}"
                ));
            }
        }
        Case::InvalidPolicy => {
            let path = std::env::temp_dir().join(format!(
                "openshell-conformance-invalid-{}.yaml",
                runner.id()
            ));
            fs::write(&path, "version: invalid\nfilesystem_policy: []\n")
                .map_err(|error| error.to_string())?;
            let _remove_file = RemoveFile(path.clone());
            let result = create_command(runner, &name, &path).await?;
            if result.success() {
                runner.track_sandbox(&name);
                return Err("sandbox creation accepted an invalid policy".into());
            }
            if !result.stderr().contains("policy") {
                return Err(
                    result.failure_diagnostic("the invalid policy caused the create failure")
                );
            }
            let get = runner
                .step("get-rejected")
                .description("invalid policy created no sandbox")
                .with_timeout(COMMAND_TIMEOUT)
                .run(&["sandbox", "get", &name, "--output", "json"])
                .await
                .map_err(|error| error.to_string())?;
            if get.success() {
                runner.track_sandbox(&name);
                return Err("sandbox exists after invalid policy was rejected".into());
            }
        }
        Case::LiveUpdate => {
            create(runner, &name, &fixture.as_ref().unwrap().path).await?;
            let before = policy_get(runner, &name, &[]).await?;
            let binary = std::env::var("OPENSHELL_CONFORMANCE_POLICY_UPDATE_BINARY")
                .unwrap_or_else(|_| "/bin/sh".to_string());
            command_with_timeout(
                runner,
                "live-update",
                &[
                    "policy",
                    "update",
                    &name,
                    "--add-endpoint",
                    "api.example.com:443",
                    "--rule-name",
                    "conformance-live-update",
                    "--binary",
                    &binary,
                    "--wait",
                    "--timeout",
                    "60",
                ],
                UPDATE_TIMEOUT,
            )
            .await?;
            let after = policy_get(runner, &name, &[]).await?;
            if number(&after, "version")? <= number(&before, "version")?
                || after["hash"] == before["hash"]
            {
                return Err("live policy update did not install a new revision".into());
            }
            let revision = policy_get(
                runner,
                &name,
                &["--rev", &number(&after, "version")?.to_string()],
            )
            .await?;
            if revision["status"] != "loaded"
                || revision["hash"] != after["hash"]
                || after["policy"]["network_policies"]["conformance-live-update"].is_null()
            {
                return Err(format!(
                    "updated policy was not loaded with the new rule: {after}"
                ));
            }
            let sandbox = command(
                runner,
                "get-after-update",
                &["sandbox", "get", &name, "--output", "json"],
            )
            .await?;
            let sandbox: Value = sandbox.json().map_err(|error| error.to_string())?;
            if sandbox["phase"] != "Ready" {
                return Err(format!(
                    "sandbox is not Ready after live policy update: {sandbox}"
                ));
            }
        }
    }
    Ok(())
}

struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct PolicyFixture {
    path: PathBuf,
    write_grant: String,
    _cleanup: Option<RemoveFile>,
}

fn policy_fixture(id: &str) -> Result<PolicyFixture, String> {
    if let Ok(raw) = std::env::var("OPENSHELL_CONFORMANCE_SMOKE_CREATE_ARGS") {
        let args: Vec<String> = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        let path = args
            .windows(2)
            .find(|pair| pair[0] == "--policy")
            .map(|pair| PathBuf::from(&pair[1]))
            .ok_or("OPENSHELL_CONFORMANCE_SMOKE_CREATE_ARGS must include --policy")?;
        let write_grant = std::env::var("OPENSHELL_CONFORMANCE_POLICY_RW_PATH").map_err(
            |_| "OPENSHELL_CONFORMANCE_POLICY_RW_PATH must name the fixture's write grant",
        )?;
        return Ok(PolicyFixture {
            path,
            write_grant,
            _cleanup: None,
        });
    }

    let path = std::env::temp_dir().join(format!("openshell-conformance-policy-{id}.yaml"));
    fs::write(&path, PORTABLE_POLICY).map_err(|error| error.to_string())?;
    Ok(PolicyFixture {
        path: path.clone(),
        write_grant: "/tmp".into(),
        _cleanup: Some(RemoveFile(path)),
    })
}

fn smoke_command() -> Result<Option<Vec<String>>, String> {
    let Ok(raw) = std::env::var("OPENSHELL_CONFORMANCE_SMOKE_COMMAND") else {
        return Ok(None);
    };
    let command: Vec<String> = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    if command.first().is_none_or(String::is_empty) {
        return Err("OPENSHELL_CONFORMANCE_SMOKE_COMMAND has no executable".into());
    }
    Ok(Some(command))
}

async fn create_command(
    runner: &OpenShellRunner,
    name: &str,
    policy: &Path,
) -> Result<openshell_conformance::CommandResult, String> {
    let policy = policy.to_str().ok_or("policy path is not valid Unicode")?;
    let mut args = vec![
        "sandbox", "create", "--name", name, "--detach", "--policy", policy,
    ];
    let command = smoke_command()?;
    if let Some(command) = &command {
        args.push("--");
        args.extend(command.iter().map(String::as_str));
    }
    runner
        .step("create")
        .description("sandbox creation with explicit policy succeeds")
        .with_timeout(CREATE_TIMEOUT)
        .run(&args)
        .await
        .map_err(|error| error.to_string())
}

async fn create(runner: &mut OpenShellRunner, name: &str, policy: &Path) -> Result<(), String> {
    runner.track_sandbox(name);
    create_command(runner, name, policy)
        .await?
        .require_success()
}

async fn command(
    runner: &OpenShellRunner,
    step: &str,
    args: &[&str],
) -> Result<openshell_conformance::CommandResult, String> {
    command_with_timeout(runner, step, args, COMMAND_TIMEOUT).await
}

async fn command_with_timeout(
    runner: &OpenShellRunner,
    step: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<openshell_conformance::CommandResult, String> {
    let result = runner
        .step(step)
        .description(format!("{step} succeeds"))
        .with_timeout(timeout)
        .run(args)
        .await
        .map_err(|error| error.to_string())?;
    result.require_success()?;
    Ok(result)
}

async fn policy_get(runner: &OpenShellRunner, name: &str, extra: &[&str]) -> Result<Value, String> {
    let mut args = vec!["policy", "get", name, "--base", "--output", "json"];
    args.extend_from_slice(extra);
    command(runner, "policy-get", &args)
        .await?
        .json()
        .map_err(|error| error.to_string())
}

fn number(value: &Value, key: &str) -> Result<u64, String> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("policy response missing numeric {key}: {value}"))
}

fn assert_initial_policy(
    current: &Value,
    revision: &Value,
    name: &str,
    expected_path: &str,
) -> Result<(), String> {
    let grants = current["policy"]["filesystem_policy"]["read_write"]
        .as_array()
        .ok_or("base policy has no filesystem read_write array")?;
    if current["sandbox"] != name
        || current["policy_source"] != "sandbox"
        || revision["status"] != "loaded"
        || current["hash"] != revision["hash"]
        || number(current, "version")? == 0
        || !grants.iter().any(|grant| grant == expected_path)
    {
        return Err(format!(
            "created sandbox did not load its explicit filesystem policy: {current}"
        ));
    }
    Ok(())
}
