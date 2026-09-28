<!--
SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
SPDX-License-Identifier: Apache-2.0
-->

# CLI conformance suite

This workspace verifies that an installed `openshell` CLI and a selected gateway
implement portable, public behavior. Tests treat the CLI as a black box. They do
not inspect driver internals or replace driver-specific qualification.

The suite separates portable contracts by capability. A test environment should
run every test supported by its compute driver instead of treating one broad
smoke test as the portability boundary. In particular:

- `smoke::control_plane` covers status, create, get, list, and delete. It does
  not require `sandbox exec`.
- `smoke::exec` covers `sandbox exec` and runs only where the driver supports
  that operation.
- `lifecycle` and the policy advisor tests have additional runtime requirements
  documented in their source modules.

The scenario implementations live in the
[`openshell-conformance` crate](../../../crates/openshell-conformance/README.md)
so the archive tests and the standalone `openshell-conformance` compatibility
runner share assertions and cleanup. New installed-artifact CI should use this
test workspace and select tests with nextest filters.

Set `OPENSHELL_BIN` to the candidate CLI. The selected gateway must already be
reachable. Runtimes that require an explicit sandbox workload can set
`OPENSHELL_CONFORMANCE_SMOKE_COMMAND` to a JSON string array. Set
`OPENSHELL_CONFORMANCE_SMOKE_CREATE_ARGS` to a JSON string array when the
runtime also needs portable create options such as `--policy`. For example:

```shell
export OPENSHELL_CONFORMANCE_SMOKE_COMMAND='["sleep","infinity"]'
cargo nextest run \
  --manifest-path tests/suites/conformance/Cargo.toml \
  -E 'binary(smoke) & test(=control_plane)'
```

The runner owns only resources named for its generated run ID and attempts to
delete them after success or failure. Tests must not depend on unrelated gateway
state or delete resources they did not create.
