# MXC isolation backend

Owns the Windows implementation of the IsolationBackend contract. The standard
supervisor uses `OpenShellRuntimeBackend` with `MxcRuntimeAdapter`; the adapter
owns descriptor decoding, native confirmation and authenticated proxy ingress.
The shared backend owns Sandbox Protocol authentication, reconnect handling,
process handles and forwarding transport.

The `openshell-mxc-isolation-backend.exe` helper runs inside the MXC boundary. It
accepts a protected bootstrap-file path, authenticates the supervisor, and waits
for `StartAgent` before launching the workload. The gateway JWT and supervisor
credentials stay outside the boundary. The helper consumes its bootstrap file
and TLS private-key file before launching untrusted code.

## Temporary confirmation

Per the current prototype scope, MXC confirmation returns success for the
required properties and outer-fence guarantees. **These checks are not verified.**
The adapter emits an unverified-confirmation warning. `TODO(mxc-confirmation)` marks
the checks to replace with native evidence. The common interface validation is
unchanged; these placeholders are confined to MXC provisioning and its backend.
There is no legacy runtime fallback or additional opt-in switch.

Follow-up work includes native filesystem and network evidence, attribution,
privilege and runtime-exit guarantees. Executable hashing retains the existing
fresh-per-connection behavior; `TODO(mxc-identity)` records the remaining
executable-object attestation work.

## Current operations

- Agent start, output attachment, wait, termination and boundary-local TCP forwarding.
- Suspended child launch and Job Object assignment before execution; root exit and termination end descendants too.
- Backend-attributed, authenticated HTTP/CONNECT ingress into the common network supervisor.
- Existing MXC policy mapping and ETW remain in the compute driver.
- Interactive exec, PTY, provider files and live provider-environment updates return unsupported errors.

Compilation checks do not establish Windows runtime behavior or RFC conformance.
