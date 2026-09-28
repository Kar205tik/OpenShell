# SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0

"""Launch Jupyter in one sandbox and print its local service URL."""

from __future__ import annotations

import json
import secrets
import struct
import time
import uuid
from contextlib import closing, suppress
from datetime import UTC, datetime
from pathlib import Path
from urllib.error import URLError
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit
from urllib.request import ProxyHandler, Request, build_opener

import websocket
import yaml
from google.protobuf.json_format import ParseDict

from openshell import SandboxClient, ServiceExposure
from openshell._proto import openshell_pb2, sandbox_pb2

EXAMPLE_DIR = Path(__file__).resolve().parent
IMAGE = "openshell-jupyter-sandbox:local"
POLICY = EXAMPLE_DIR / "policy.yaml"
WORKSPACE = "default"
GATEWAY: str | None = None  # None selects the active gateway.
CODE = "print(sum(i * i for i in range(10)))"

_START_JUPYTER = """
set -eu
umask 077
IFS= read -r token
printf '%s' "$token" > /tmp/openshell-jupyter-token
export HOME=/sandbox
export JUPYTER_TOKEN_FILE=/tmp/openshell-jupyter-token
nohup jupyter server \
  --ServerApp.ip=127.0.0.1 \
  --ServerApp.port=8888 \
  --ServerApp.port_retries=0 \
  --ServerApp.open_browser=False \
  --ServerApp.root_dir=/sandbox \
  --ServerApp.terminals_enabled=False \
  </dev/null >/tmp/openshell-jupyter.log 2>&1 &
""".strip()


def sandbox_spec() -> openshell_pb2.SandboxSpec:
    """Load the example policy into the SDK's current sandbox spec."""
    policy_data = yaml.safe_load(POLICY.read_text(encoding="utf-8"))
    policy_data["filesystem"] = policy_data.pop("filesystem_policy")
    return openshell_pb2.SandboxSpec(
        template=openshell_pb2.SandboxTemplate(image=IMAGE),
        policy=ParseDict(policy_data, sandbox_pb2.SandboxPolicy()),
    )


def browser_url(service_url: str, token: str) -> str:
    """Add the Jupyter token to the URL returned by OpenShell."""
    parsed = urlsplit(service_url)
    query = dict(parse_qsl(parsed.query, keep_blank_values=True))
    query["token"] = token
    return urlunsplit(parsed._replace(query=urlencode(query)))


def execute_in_kernel(service_url: str, token: str, code: str) -> None:
    """Submit code through the exposed Jupyter REST and WebSocket APIs."""
    base = service_url.rstrip("/")
    opener = build_opener(ProxyHandler({}))

    def api_request(method: str, path: str, payload: dict | None = None) -> bytes:
        data = json.dumps(payload).encode() if payload is not None else None
        request = Request(
            browser_url(f"{base}{path}", token),
            data=data,
            headers={"Content-Type": "application/json"},
            method=method,
        )
        with opener.open(request, timeout=10) as response:
            return response.read()

    deadline = time.monotonic() + 60
    while True:
        try:
            api_request("GET", "/api/status")
            break
        except (URLError, TimeoutError):
            if time.monotonic() >= deadline:
                raise RuntimeError(
                    "Jupyter did not become ready within 60 seconds"
                ) from None
            time.sleep(1)

    kernel = json.loads(api_request("POST", "/api/kernels", {"name": "python3"}))
    kernel_id = kernel["id"]
    try:
        session_id = uuid.uuid4().hex
        msg_id = uuid.uuid4().hex
        channels = browser_url(f"{base}/api/kernels/{kernel_id}/channels", token)
        parsed = urlsplit(channels)
        query = dict(parse_qsl(parsed.query))
        query["session_id"] = session_id
        socket_url = urlunsplit(
            parsed._replace(
                scheme="wss" if parsed.scheme == "https" else "ws",
                query=urlencode(query),
            )
        )

        request = {
            "channel": "shell",
            "header": {
                "date": datetime.now(UTC).isoformat(),
                "msg_id": msg_id,
                "msg_type": "execute_request",
                "session": session_id,
                "username": "openshell",
                "version": "5.3",
            },
            "parent_header": {},
            "metadata": {},
            "content": {
                "code": code,
                "silent": False,
                "store_history": True,
                "allow_stdin": False,
                "stop_on_error": True,
                "user_expressions": {},
            },
        }

        replied = idle = False
        with closing(
            websocket.create_connection(
                socket_url,
                http_no_proxy=[parsed.hostname or "localhost"],
                suppress_origin=True,
                timeout=30,
            )
        ) as socket:
            socket.send(json.dumps(request))
            while not (replied and idle):
                message = socket.recv()
                if isinstance(message, bytes):
                    # Jupyter's binary WebSocket framing starts with JSON offsets.
                    count = struct.unpack_from("!I", message)[0]
                    offsets = struct.unpack_from(f"!{count}I", message, 4)
                    end = offsets[1] if count > 1 else len(message)
                    message = message[offsets[0] : end]
                event = json.loads(message)
                if event.get("parent_header", {}).get("msg_id") != msg_id:
                    continue
                kind = event.get("header", {}).get("msg_type")
                content = event.get("content", {})
                if kind == "stream":
                    print(content.get("text", ""), end="")
                elif kind in {"execute_result", "display_data"}:
                    print(content.get("data", {}).get("text/plain", ""))
                elif kind == "error":
                    raise RuntimeError("\n".join(content.get("traceback", [])))
                elif kind == "execute_reply":
                    replied = True
                    if content.get("status") == "error":
                        raise RuntimeError(
                            content.get("evalue", "kernel execution failed")
                        )
                elif kind == "status" and content.get("execution_state") == "idle":
                    idle = True
    finally:
        api_request("DELETE", f"/api/kernels/{kernel_id}")


def main() -> None:
    token = secrets.token_urlsafe(32)
    name = f"jupyter-{secrets.token_hex(3)}"

    with SandboxClient.from_active_cluster(cluster=GATEWAY) as client:
        session = client.create_session(
            workspace=WORKSPACE,
            name=name,
            spec=sandbox_spec(),
            service_exposures=[ServiceExposure(service="jupyter", target_port=8888)],
        )
        try:
            client.wait_ready(name, workspace=WORKSPACE)
            started = session.exec(
                ["/bin/sh", "-c", _START_JUPYTER], stdin=f"{token}\n".encode()
            )
            if started.exit_code != 0:
                raise RuntimeError(f"could not start Jupyter: {started.stderr}")

            print(f"Sandbox: {name}")
            print(
                "Open in your browser: "
                f"{browser_url(session.sandbox.service_urls['jupyter'], token)}"
            )

            print("Jupyter kernel output:")
            execute_in_kernel(session.sandbox.service_urls["jupyter"], token, CODE)

            with suppress(EOFError, KeyboardInterrupt):
                input("Press Enter to delete the sandbox (or Ctrl-C to stop)...")
        finally:
            deletion = session.delete(allow_missing=True)
            if deletion.sandbox_id:
                client.wait_deleted(
                    name,
                    workspace=WORKSPACE,
                    expected_sandbox_id=deletion.sandbox_id,
                )


if __name__ == "__main__":
    main()
