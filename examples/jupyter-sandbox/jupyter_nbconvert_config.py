# SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0

"""Make nbconvert use the Jupyter service started by launch.py."""

import json
import os
from pathlib import Path
from unittest.mock import patch

import websocket
from jupyter_server.gateway.managers import GatewayKernelClient, GatewayKernelManager

connection_file = Path(".jupyter-service.json")
if not connection_file.is_file():
    raise RuntimeError("Start the Jupyter sandbox with `python launch.py` first")
connection = json.loads(connection_file.read_text(encoding="utf-8"))
os.environ["JUPYTER_GATEWAY_URL"] = connection["url"]
os.environ["JUPYTER_GATEWAY_AUTH_TOKEN"] = connection["token"]


class AuthenticatedGatewayKernelClient(GatewayKernelClient):
    """Send the Jupyter token on the gateway WebSocket connection."""

    async def start_channels(self, *args, **kwargs):
        original = websocket.create_connection

        def authenticated(*connection_args, **connection_kwargs):
            connection_kwargs["header"] = {
                "Authorization": f"token {connection['token']}"
            }
            return original(*connection_args, **connection_kwargs)

        # GatewayKernelClient forwards the token on REST but not WebSocket.
        with patch.object(websocket, "create_connection", authenticated):
            return await super().start_channels(*args, **kwargs)


class AuthenticatedGatewayKernelManager(GatewayKernelManager):
    client_factory = AuthenticatedGatewayKernelClient


c = get_config()  # noqa: F821 - provided by Jupyter's config loader
c.ExecutePreprocessor.kernel_manager_class = AuthenticatedGatewayKernelManager
