# Jupyter Sandbox

Build a Jupyter image, launch it as an OpenShell service, then execute a local
notebook on a kernel inside the sandbox.

Run these commands from `examples/jupyter-sandbox`. The example needs a working
local OpenShell gateway, Docker, Python 3.11 or later, and
[uv](https://docs.astral.sh/uv/).

## 1. Build the container

```shell
docker build -t openshell-jupyter-sandbox:local .
uv venv
source .venv/bin/activate
uv pip install -e ../.. pyyaml jupyter-server==2.20.0 nbconvert==7.17.1 websocket-client
```

## 2. Launch the service

```shell
python launch.py
```

The sandbox starts Jupyter as its main command and exposes port 8888 as the
`jupyter` service. The launcher prints a token-authenticated browser URL and
waits. Keep it running while you execute the notebook. Press Enter or Ctrl-C
when done to delete the sandbox and service.

## 3. Execute the notebook on the remote kernel

In a second terminal, activate the same virtual environment and run:

```shell
cd examples/jupyter-sandbox
source .venv/bin/activate
export JUPYTER_CONFIG_PATH="$PWD"
jupyter nbconvert --execute --to notebook demo.ipynb
```

Open `demo.nbconvert.ipynb` locally to see the cell output, `285`. The notebook
file and executed result stay on your computer; the kernel runs in the sandbox.

The launcher writes a mode-restricted `.jupyter-service.json` with the service
URL and token. `jupyter_nbconvert_config.py` uses it to direct nbconvert's
kernel manager to the service and authenticate the WebSocket connection. Both
files must be used from this directory, and `JUPYTER_CONFIG_PATH` tells
nbconvert where to find the config. The launcher removes the connection
file when it exits. Treat the printed URL as a credential.
