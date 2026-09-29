// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Select request trace trees without removing their context from logging or
//! propagation. Dropped SDK spans still carry an unsampled parent context.

use opentelemetry::trace::{Link, SpanKind, TraceContextExt, TraceId};
use opentelemetry::{Context, KeyValue, Value};
use opentelemetry_sdk::trace::{SamplingDecision, SamplingResult, ShouldSample};

#[derive(Debug, Clone)]
pub(super) struct RequestSampler {
    request_sampler: Box<dyn ShouldSample>,
}

impl RequestSampler {
    pub(super) fn new(request_sampler: Box<dyn ShouldSample>) -> Self {
        Self { request_sampler }
    }
}

impl ShouldSample for RequestSampler {
    fn should_sample(
        &self,
        parent_context: Option<&Context>,
        trace_id: TraceId,
        name: &str,
        span_kind: &SpanKind,
        attributes: &[KeyValue],
        links: &[Link],
    ) -> SamplingResult {
        // Only the HTTP/gRPC request boundary supplies a server span with a
        // path. Driver server spans have no path and inherit their parent.
        let request_path = attributes.iter().find_map(|attribute| {
            match (span_kind, attribute.key.as_str(), &attribute.value) {
                (SpanKind::Server, "path", Value::String(path)) => Some(path.as_str()),
                _ => None,
            }
        });
        if let Some(path) = request_path
            && !is_background_request(path)
        {
            return self.request_sampler.should_sample(
                parent_context,
                trace_id,
                name,
                span_kind,
                attributes,
                links,
            );
        }

        let parent = parent_context.map(TraceContextExt::span);
        let parent_span_context = parent
            .as_ref()
            .map(opentelemetry::trace::SpanRef::span_context);
        // Do not delegate descendants to AlwaysOn: it would resurrect children
        // of a dropped poll or reconciliation root. An excluded request must
        // also override a sampled incoming parent before its handler executes.
        let selected = request_path.is_none()
            && parent_span_context.is_some_and(|context| {
                context.is_valid() && !context.is_remote() && context.is_sampled()
            });
        SamplingResult {
            decision: if selected {
                SamplingDecision::RecordAndSample
            } else {
                SamplingDecision::Drop
            },
            attributes: Vec::new(),
            trace_state: parent_span_context
                .map(|context| context.trace_state().clone())
                .unwrap_or_default(),
        }
    }
}

/// These routes perform periodic observation or runtime housekeeping. Selection
/// is by operation, so manual calls to the same endpoint are excluded too.
/// Relay streams stay eligible because they can carry operator-triggered work.
fn is_background_request(path: &str) -> bool {
    matches!(
        path,
        "/health"
            | "/healthz"
            | "/readyz"
            | "/metrics"
            | "/openshell.v1.OpenShell/GetSandboxConfig"
            | "/openshell.v1.OpenShell/GetSandboxProviderEnvironment"
            | "/openshell.v1.OpenShell/ReportPolicyStatus"
            | "/openshell.v1.OpenShell/ReportEndpointStatus"
            | "/openshell.v1.OpenShell/ReportProviderReadiness"
            | "/openshell.v1.OpenShell/ReportSandboxConfiguration"
            | "/openshell.v1.OpenShell/PushSandboxLogs"
            | "/openshell.v1.OpenShell/ConnectSupervisor"
    ) || path.starts_with("/grpc.health.v1.Health/")
        || path.starts_with("/grpc.reflection.v1.ServerReflection/")
        || path.starts_with("/grpc.reflection.v1alpha.ServerReflection/")
}

#[cfg(test)]
mod tests;
