# SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0

"""Launch a Jupyter service in one OpenShell sandbox."""

from __future__ import annotations

import json
import os
import secrets
import time
from contextlib import suppress
from pathlib import Path
from urllib.error import URLError
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit
from urllib.request import ProxyHandler, Request, build_opener

import yaml
from google.protobuf.json_format import ParseDict

from openshell import SandboxClient, ServiceExposure
from openshell._proto import openshell_pb2, sandbox_pb2

EXAMPLE_DIR = Path(__file__).resolve().parent
CONNECTION = EXAMPLE_DIR / ".jupyter-service.json"
IMAGE = "openshell-jupyter-sandbox:local"
POLICY = EXAMPLE_DIR / "policy.yaml"
WORKSPACE = "default"
GATEWAY: str | None = None  # None selects the active gateway.


def sandbox_spec(token: str) -> openshell_pb2.SandboxSpec:
    policy_data = yaml.safe_load(POLICY.read_text(encoding="utf-8"))
    policy_data["filesystem"] = policy_data.pop("filesystem_policy")
    return openshell_pb2.SandboxSpec(
        template=openshell_pb2.SandboxTemplate(image=IMAGE),
        policy=ParseDict(policy_data, sandbox_pb2.SandboxPolicy()),
        environment={"HOME": "/sandbox", "JUPYTER_TOKEN": token},
        command=[
            "jupyter",
            "server",
            "--ServerApp.ip=127.0.0.1",
            "--ServerApp.port=8888",
            "--ServerApp.port_retries=0",
            "--ServerApp.open_browser=False",
            "--ServerApp.root_dir=/sandbox",
            "--ServerApp.terminals_enabled=False",
        ],
    )


def browser_url(service_url: str, token: str) -> str:
    parsed = urlsplit(service_url)
    query = dict(parse_qsl(parsed.query, keep_blank_values=True))
    query["token"] = token
    return urlunsplit(parsed._replace(query=urlencode(query)))


def wait_for_jupyter(service_url: str, token: str) -> None:
    opener = build_opener(ProxyHandler({}))
    url = browser_url(f"{service_url.rstrip('/')}/api/status", token)
    deadline = time.monotonic() + 60
    while True:
        try:
            with opener.open(Request(url), timeout=5):
                return
        except (URLError, TimeoutError):
            if time.monotonic() >= deadline:
                raise RuntimeError(
                    "Jupyter did not become ready within 60 seconds"
                ) from None
            time.sleep(1)


def main() -> None:
    token = secrets.token_urlsafe(32)
    name = f"jupyter-{secrets.token_hex(3)}"

    with SandboxClient.from_active_cluster(cluster=GATEWAY) as client:
        session = client.create_session(
            workspace=WORKSPACE,
            name=name,
            spec=sandbox_spec(token),
            service_exposures=[ServiceExposure(service="jupyter", target_port=8888)],
        )
        connection_created = False
        try:
            client.wait_ready(name, workspace=WORKSPACE)
            service_url = session.sandbox.service_urls["jupyter"]
            wait_for_jupyter(service_url, token)

            fd = os.open(CONNECTION, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            connection_created = True
            with os.fdopen(fd, "w", encoding="utf-8") as output:
                json.dump({"url": service_url.rstrip("/"), "token": token}, output)

            print(f"Sandbox: {name}")
            print(f"Browser: {browser_url(service_url, token)}")
            print("In another terminal, run:")
            print(f"  cd {EXAMPLE_DIR}")
            print("  source .venv/bin/activate")
            print('  export JUPYTER_CONFIG_PATH="$PWD"')
            print("  jupyter nbconvert --execute --to notebook demo.ipynb")
            with suppress(EOFError, KeyboardInterrupt):
                input("Press Enter to delete the sandbox (or Ctrl-C to stop)...")
        finally:
            if connection_created:
                CONNECTION.unlink(missing_ok=True)
            deletion = session.delete(allow_missing=True)
            if deletion.sandbox_id:
                client.wait_deleted(
                    name,
                    workspace=WORKSPACE,
                    expected_sandbox_id=deletion.sandbox_id,
                )


if __name__ == "__main__":
    main()
