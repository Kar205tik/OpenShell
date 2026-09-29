// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceState};
use opentelemetry_sdk::trace::{InMemorySpanExporterBuilder, Sampler, SdkTracerProvider};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use tracing_subscriber::layer::SubscriberExt as _;

fn parent(sampled: bool, remote: bool) -> Context {
    Context::new().with_remote_span_context(SpanContext::new(
        TraceId::from(42),
        SpanId::from(7),
        if sampled {
            TraceFlags::SAMPLED
        } else {
            TraceFlags::default()
        },
        remote,
        TraceState::from_key_value([("vendor", "preserved")]).unwrap(),
    ))
}

fn decide(
    sampler: &RequestSampler,
    parent: Option<&Context>,
    path: Option<&str>,
) -> SamplingResult {
    let attributes = path
        .map(|path| KeyValue::new("path", path.to_owned()))
        .into_iter()
        .collect::<Vec<_>>();
    sampler.should_sample(
        parent,
        TraceId::from(42),
        "test",
        &SpanKind::Server,
        &attributes,
        &[],
    )
}

#[test]
fn request_selection_preserves_sdk_policy_and_remote_parent() {
    let selected_parent = parent(true, true);
    let dropped_parent = parent(false, true);
    let path = Some("/openshell.v1.OpenShell/CreateSandbox");
    let sampler = RequestSampler::new(Box::new(Sampler::ParentBased(Box::new(Sampler::AlwaysOn))));
    assert_eq!(
        decide(&sampler, None, path).decision,
        SamplingDecision::RecordAndSample
    );
    let selected = decide(&sampler, Some(&selected_parent), path);
    assert_eq!(selected.decision, SamplingDecision::RecordAndSample);
    assert_eq!(selected.trace_state.get("vendor"), Some("preserved"));
    assert_eq!(
        decide(&sampler, Some(&dropped_parent), path).decision,
        SamplingDecision::Drop
    );
    for (policy, expected) in [
        (Sampler::AlwaysOff, SamplingDecision::Drop),
        (Sampler::TraceIdRatioBased(0.0), SamplingDecision::Drop),
        (
            Sampler::TraceIdRatioBased(1.0),
            SamplingDecision::RecordAndSample,
        ),
    ] {
        assert_eq!(
            decide(&RequestSampler::new(Box::new(policy)), None, path).decision,
            expected
        );
    }
}

#[test]
fn background_requests_override_sampled_parents() {
    let sampler = RequestSampler::new(Box::new(Sampler::AlwaysOn));
    let remote_parent = parent(true, true);
    for path in [
        "/health",
        "/healthz",
        "/readyz",
        "/metrics",
        "/grpc.health.v1.Health/Watch",
        "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo",
        "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo",
        "/openshell.v1.OpenShell/GetSandboxConfig",
        "/openshell.v1.OpenShell/GetSandboxProviderEnvironment",
        "/openshell.v1.OpenShell/ReportPolicyStatus",
        "/openshell.v1.OpenShell/ReportEndpointStatus",
        "/openshell.v1.OpenShell/ReportProviderReadiness",
        "/openshell.v1.OpenShell/ReportSandboxConfiguration",
        "/openshell.v1.OpenShell/PushSandboxLogs",
        "/openshell.v1.OpenShell/ConnectSupervisor",
    ] {
        let result = decide(&sampler, Some(&remote_parent), Some(path));
        assert_eq!(result.decision, SamplingDecision::Drop, "{path}");
        assert_eq!(result.trace_state.get("vendor"), Some("preserved"));
    }
    for path in [
        "/openshell.v1.OpenShell/UpdateConfig",
        "/openshell.v1.OpenShell/RelayStream",
        "/unknown",
        "/health-check",
    ] {
        assert_eq!(
            decide(&sampler, None, Some(path)).decision,
            SamplingDecision::RecordAndSample,
            "{path}"
        );
    }
}

#[test]
fn children_inherit_local_selection_without_resampling() {
    let sampler = RequestSampler::new(Box::new(Sampler::AlwaysOn));
    assert_eq!(
        decide(&sampler, None, None).decision,
        SamplingDecision::Drop
    );
    for (sampled, remote, expected) in [
        (true, false, SamplingDecision::RecordAndSample),
        (false, false, SamplingDecision::Drop),
        (true, true, SamplingDecision::Drop),
        (false, true, SamplingDecision::Drop),
    ] {
        assert_eq!(
            decide(&sampler, Some(&parent(sampled, remote)), None).decision,
            expected
        );
    }
    let off = RequestSampler::new(Box::new(Sampler::AlwaysOff));
    assert_eq!(
        decide(&off, Some(&parent(true, false)), None).decision,
        SamplingDecision::RecordAndSample
    );
}

#[test]
fn gateway_and_driver_export_only_selected_trees_and_keep_log_events() {
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::Layer as _;

    #[derive(Clone)]
    struct Events(Arc<Mutex<usize>>);
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Events {
        fn on_event(&self, _: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
            *self.0.lock().unwrap() += 1;
        }
    }

    let gateway_exporter = InMemorySpanExporterBuilder::new().build();
    let driver_exporter = InMemorySpanExporterBuilder::new().build();
    let gateway = SdkTracerProvider::builder()
        .with_simple_exporter(gateway_exporter.clone())
        .with_sampler(RequestSampler::new(Box::new(Sampler::AlwaysOn)))
        .build();
    let driver = SdkTracerProvider::builder()
        .with_simple_exporter(driver_exporter.clone())
        .with_sampler(RequestSampler::new(Box::new(Sampler::AlwaysOn)))
        .build();
    let descriptor =
        openshell_otel::ComputeDriverTracing::new("openshell-driver-test", "test_driver");
    let events = Arc::new(Mutex::new(0));
    let subscriber = tracing_subscriber::registry()
        .with(Events(events.clone()))
        .with(super::super::layer(&gateway, Some(descriptor)))
        .with(
            descriptor
                .in_process_layer(&driver)
                .with_filter(tracing_subscriber::filter::LevelFilter::TRACE),
        );
    let _scope = super::super::test_exporter::install_scoped(subscriber);

    let request = tracing::info_span!(
        "selected",
        otel.kind = "server",
        path = "/openshell.v1.OpenShell/CreateSandbox"
    );
    request.in_scope(|| {
        tracing::info!("request log remains visible");
        drop(tracing::info_span!("request.store"));
        let driver =
            tracing::info_span!(target: "test_driver", "request.driver", otel.kind = "server");
        driver
            .in_scope(|| drop(tracing::info_span!(target: "test_driver", "request.driver.child")));
    });
    let request_context = request.context();
    // An explicitly parented task can run after the request handler's scope.
    drop(tracing::info_span!(parent: &request, "request.deferred"));
    drop(request);
    for path in [None, Some("/openshell.v1.OpenShell/GetSandboxConfig")] {
        let background = path.map_or_else(
            || tracing::info_span!("reconcile"),
            |path| tracing::info_span!("poll", otel.kind = "server", path),
        );
        // A sampled remote context must not revive either excluded entry.
        background.set_parent(parent(true, true)).unwrap();
        background.in_scope(|| {
            tracing::warn!("background failure remains visible");
            assert!(!background.context().span().span_context().is_sampled());
            drop(tracing::info_span!("background.store"));
            let driver = tracing::info_span!(target: "test_driver", "background.driver");
            driver.in_scope(|| {
                drop(tracing::info_span!(target: "test_driver", "background.driver.child"));
            });
        });
    }
    drop(tracing::info_span!(target: "test_driver", "driver.background.root"));
    gateway.force_flush().unwrap();
    driver.force_flush().unwrap();
    let gateway_spans = gateway_exporter.get_finished_spans().unwrap();
    let driver_spans = driver_exporter.get_finished_spans().unwrap();
    assert_eq!(gateway_spans.len(), 3, "{gateway_spans:?}");
    assert_eq!(driver_spans.len(), 2, "{driver_spans:?}");
    assert_eq!(*events.lock().unwrap(), 3);
    for span in gateway_spans.iter().chain(&driver_spans) {
        assert_eq!(
            span.span_context.trace_id(),
            request_context.span().span_context().trace_id()
        );
        assert!(span.name == "selected" || span.name.starts_with("request."));
    }
    for name in ["request.store", "request.driver", "request.deferred"] {
        let child = gateway_spans
            .iter()
            .chain(&driver_spans)
            .find(|span| span.name == name)
            .unwrap();
        assert_eq!(
            child.parent_span_id,
            request_context.span().span_context().span_id()
        );
    }
}
