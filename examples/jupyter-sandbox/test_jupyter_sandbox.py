# SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0

"""Lifecycle checks for the SDK-backed Jupyter example."""

from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast

import pytest
from jupyter_sandbox import JupyterSandbox, JupyterSandboxError

if TYPE_CHECKING:
    from openshell import SandboxClient


class FakeSession:
    def __init__(self, service_url: str | None) -> None:
        self.sandbox = SimpleNamespace(
            service_urls={"jupyter": service_url} if service_url else {}
        )
        self.deleted = False

    def delete(self, *, allow_missing: bool = False) -> SimpleNamespace:
        assert allow_missing
        self.deleted = True
        return SimpleNamespace(sandbox_id="sandbox-id")


class FakeClient:
    def __init__(self, service_url: str | None) -> None:
        self.session = FakeSession(service_url)
        self.create_args: dict[str, Any] | None = None
        self.waited_for_deletion: dict[str, Any] | None = None

    def create_session(self, **kwargs: Any) -> FakeSession:
        self.create_args = kwargs
        return self.session

    def wait_ready(self, name: str, **kwargs: Any) -> None:
        assert name == "jupyter-test"
        assert kwargs["workspace"] == "default"

    def wait_deleted(self, name: str, **kwargs: Any) -> None:
        assert name == "jupyter-test"
        self.waited_for_deletion = kwargs


def test_exposes_service_and_cleans_up_with_sandbox(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    client = FakeClient("http://jupyter.openshell.localhost:17670/")
    monkeypatch.setattr(JupyterSandbox, "_start_jupyter", lambda _self: None)
    monkeypatch.setattr(JupyterSandbox, "_wait_until_ready", lambda _self: None)

    with JupyterSandbox(
        client=cast("SandboxClient", client),
        name="jupyter-test",
        workspace="default",
        image="jupyter:local",
        policy=Path(__file__).with_name("policy.yaml"),
    ) as sandbox:
        assert sandbox.service_url == "http://jupyter.openshell.localhost:17670"
        assert client.create_args is not None
        exposure = client.create_args["service_exposures"][0]
        assert (exposure.service, exposure.target_port) == ("jupyter", 8888)

    assert client.session.deleted
    assert client.waited_for_deletion is not None
    assert client.waited_for_deletion["expected_sandbox_id"] == "sandbox-id"


def test_missing_service_url_still_deletes_created_sandbox() -> None:
    client = FakeClient(None)

    with (
        pytest.raises(JupyterSandboxError, match="service URL"),
        JupyterSandbox(
            client=cast("SandboxClient", client),
            name="jupyter-test",
            workspace="default",
            image="jupyter:local",
            policy=Path(__file__).with_name("policy.yaml"),
        ),
    ):
        pytest.fail("context should not have started")

    assert client.session.deleted
