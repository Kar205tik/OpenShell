<!--
SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
SPDX-License-Identifier: Apache-2.0
-->

# OpenShell conformance scenarios

This crate defines reusable black-box conformance scenarios for the public
`openshell` CLI. It owns command execution, assertions, diagnostics, generated
resource names, and best-effort cleanup. It does not provision a gateway or
inspect compute-driver internals.

The crate is a scenario library, not the primary CI entrypoint:

| Component | Responsibility |
|---|---|
| [`tests/suites/conformance/cli`](../../tests/suites/conformance/README.md) | Wrap exported scenarios as Cargo tests. Conformance CI should run the complete workspace; focused selection is for local diagnosis. |
| `openshell-conformance` | Define portable scenarios and the shared `OpenShellRunner`. |
| [`openshell-conformance-cli`](../openshell-conformance-cli) | Run registered scenarios manually or for compatibility with existing E2E tooling. |

Cargo tests can invoke any exported `Scenario`. The standalone CLI invokes the
scenarios returned by `scenarios()`. A scenario does not need to be registered
with the CLI when it exists only as an independently selectable test capability.
For example, the Cargo suite invokes `SMOKE_CONTROL_PLANE_SCENARIO` and
`SMOKE_EXEC_SCENARIO` separately, while the compatibility CLI retains the
registered aggregate `SMOKE_SCENARIO` under the stable name `smoke`.

Scenarios must exercise public CLI behavior, own only resources created for
their run ID, and remain independent of unrelated gateway state. Split coverage
when a behavior requires an optional runtime capability so environments can run
the largest supported subset without weakening assertions. Driver internals,
platform enforcement, and hardware qualification belong in driver-specific
tests rather than this crate.

When adding a scenario, export it from the library and add the appropriate Cargo
test wrapper under `tests/suites/conformance/cli`. Add it to the standalone CLI
registry only when existing E2E or manual workflows also need that entrypoint.
