# Jupyter Sandbox

Launch a Jupyter Server in one OpenShell sandbox, open its local service URL in
your browser, and submit code to a kernel through the exposed service.

## Prerequisites

- A working local OpenShell gateway and its gateway configuration
- Docker to build the Jupyter image
- Python 3.11 or later and [uv](https://docs.astral.sh/uv/)

## Run

From this directory:

```shell
uv venv
source .venv/bin/activate
uv pip install -e ../.. pyyaml websocket-client
docker build -t openshell-jupyter-sandbox:local .
python demo.py
```

The script creates a sandbox and exposes port 8888 as the `jupyter` service. It
starts a token-authenticated Jupyter Server on the sandbox's loopback address,
then prints a URL to open in your local browser. Jupyter may take a few seconds
to start. Keep the script running while you use it; press Enter or Ctrl-C to
delete the sandbox and its service.

The script also creates a Jupyter kernel through the exposed REST API, sends
this code through the kernel's WebSocket channel, and prints `285`:

```python
print(sum(i * i for i in range(10)))
```

The code runs in a Jupyter kernel inside the sandbox. You can also run code
interactively by opening the printed URL in your browser.

The URL contains a Jupyter token. Treat it as a credential and do not share it.
The example requires a local gateway; it does not configure remote gateway
authentication for browser access.

Edit the constants at the top of `demo.py` to change the image, policy,
workspace, gateway, or command. The image must contain Jupyter Server and
Python, plus the `sandbox` user and group selected by the policy.
