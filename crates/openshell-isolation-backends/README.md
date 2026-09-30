# Isolation backend composition

Registers the first-party Sandbox Protocol adapters by admitted backend name.
The supervisor service uses this registry without platform or MXC branches.
Unknown names are rejected; selection never falls back to another backend.

Native evidence and explicit-proxy ingress belong to each adapter. Common
protocol authentication and process/session transport remain in
`openshell-sandbox-backend`.
