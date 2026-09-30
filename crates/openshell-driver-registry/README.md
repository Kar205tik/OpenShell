# Compute driver registry

Composes first-party compute drivers for the standard gateway. Platform and
feature selection live here, leaving the gateway service portable. MXC's
configuration, construction and audit selection use its `MxcIntegration` through
the service-independent `InProcessDriverIntegration` contract in core.

Unsupported Windows compute-driver registrations continue to return explicit
configuration errors. VM subprocess provisioning remains in this composition
layer until its independent driver packaging is refactored.
