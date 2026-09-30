# MXC compute driver

Provisions Windows ProcessContainer or IsolationSession boundaries with
`wxc-exec` and launches the standard `openshell-supervisor`. Native runtime code
lives in [openshell-mxc-isolation-backend](../openshell-mxc-isolation-backend/README.md).

## Runtime ownership

| Component | Responsibility |
|---|---|
| MXC driver | Configuration, policy mapping, provision/stop/delete, paired-process monitoring, ETW attribution |
| MXC isolation backend | Native workload process operations, Windows identity, explicit-proxy authentication, confirmation |
| Standard supervisor | Gateway session, policy evaluation, credentials, networking, process supervision and forwarding |
| Shared Sandbox Protocol backend | Authenticated transport, reconnects and backend process handles |

The driver passes gateway-issued launch generation and credentials to the
supervisor. The boundary receives only its server TLS material, public gateway
verification keys and workload bootstrap settings. Gateway readiness comes from
the supervisor session, rather than a driver-owned proxy or forwarding session.

MXC's confirmation is temporarily stubbed successful with TODOs. This is prototype integration, not a claim
that all RFC 0012 enforcement requirements have been proven.

## Configuration

```toml
[openshell.drivers.mxc]
wxc_exec_path = "C:\\mxc\\wxc-exec.exe"
backend = "process_container"
# Defaults resolve these next to the gateway executable.
supervisor_binary_path = "C:\\openshell\\openshell-supervisor.exe"
sandbox_binary_path = "C:\\openshell\\openshell-mxc-isolation-backend.exe"
# Optional host state directory and supervisor-to-gateway endpoint override.
# state_dir = "C:\\Users\\example\\AppData\\Local\\OpenShell\\mxc"
# grpc_endpoint = "https://localhost:17670"
default_configuration_id = "composable"
pc_least_privilege = false
pc_capabilities = []
pc_allow_local_network = true
pc_minimal_env = false
pc_network_allow = false
egress_proxy = true
egress_proxy_addr = "127.0.0.1:18080"
etw_audit = false
debug = false
# Required only when callers supply driver_config JSON.
allow_driver_config = true
```

`wxc_exec_path` must be absolute. The gateway supplies its local endpoint and
configured guest TLS bundle unless `grpc_endpoint` overrides the endpoint.
Governed egress keeps the existing policy mapping and ProcessContainer-only
restriction. The configured proxy address supplies a loopback seed; each sandbox
gets an ephemeral listener. IsolationSession retains its mapper restrictions.

Supply the workload through `sandbox create -- <COMMAND>` or the existing
`mxc.command` driver configuration. An explicit driver configuration wins. An
omitted working directory resolves to the gateway's current directory.
Environment comes from the template and sandbox request, then admitted provider
placeholders and proxy settings are applied inside the boundary.

The old `pc_relay_spawner_path` and `pc_relay_target_port` fields remain accepted
for configuration compatibility. Forwarding now uses the backend's loopback
connector; those fields no longer select the runtime. The standalone relay
binary remains packaged from this crate for existing manual tooling.

## Limits and validation

CPU/memory limits, GPU, agent sockets, interactive exec/PTY, provider files, live
provider-environment changes and restart recovery remain unsupported. UI support
is advertised only for ProcessContainer. Compilation is the current validation
gate; native runtime and qualification scenarios need a supervisor/gateway launch
fixture and have not been executed for this integration.

Existing native policy fixtures and host qualification material remain in
[tests](tests/), [examples](examples/) and [qualification](qualification/).

The `egress_proxy_addr` setting retains its loopback validation and enables governed
egress; its port is a compatibility hint. Each sandbox reserves its own ephemeral
proxy port, so sandboxes cannot collide on a configured fixed port.
