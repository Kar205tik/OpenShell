# SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0

"""Check that the example submits code to a Jupyter kernel."""

from __future__ import annotations

import json
import struct
from io import BytesIO
from typing import TYPE_CHECKING, Any

import demo

if TYPE_CHECKING:
    from _pytest.capture import CaptureFixture
    from _pytest.monkeypatch import MonkeyPatch


def test_execute_in_kernel(
    monkeypatch: MonkeyPatch, capsys: CaptureFixture[str]
) -> None:
    methods: list[str] = []

    class Opener:
        def open(self, request: Any, *, timeout: int) -> BytesIO:
            assert timeout == 10
            assert "token=test-token" in request.full_url
            methods.append(request.get_method())
            if request.get_method() == "POST":
                return BytesIO(b'{"id":"kernel-id"}')
            return BytesIO(b"{}")

    class Socket:
        def __init__(self) -> None:
            self.messages: list[str | bytes] = []
            self.closed = False

        def send(self, payload: str) -> None:
            request = json.loads(payload)
            assert request["content"]["code"] == "print(285)"
            msg_id = request["header"]["msg_id"]

            def event(kind: str, content: dict[str, Any]) -> str:
                return json.dumps(
                    {
                        "parent_header": {"msg_id": msg_id},
                        "header": {"msg_type": kind},
                        "content": content,
                    }
                )

            stream = event("stream", {"text": "285\n"}).encode()
            self.messages = [
                struct.pack("!II", 1, 8) + stream,
                event("status", {"execution_state": "idle"}),
                event("execute_reply", {"status": "ok"}),
            ]

        def recv(self) -> str | bytes:
            return self.messages.pop(0)

        def close(self) -> None:
            self.closed = True

    socket = Socket()
    monkeypatch.setattr(demo, "build_opener", lambda _proxy: Opener())
    monkeypatch.setattr(
        demo.websocket, "create_connection", lambda *_args, **_kwargs: socket
    )

    demo.execute_in_kernel(
        "http://jupyter.openshell.localhost:17670/", "test-token", "print(285)"
    )

    assert capsys.readouterr().out == "285\n"
    assert methods == ["GET", "POST", "DELETE"]
    assert socket.closed
