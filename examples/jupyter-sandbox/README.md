# Jupyter Sandbox

Build a Jupyter image, expose its server through a local OpenShell gateway,
then execute a local notebook on a kernel inside the sandbox.

Run these commands from `examples/jupyter-sandbox`. You need a local,
loopback-bound OpenShell gateway backed by Docker, plus Docker, the `openshell`
CLI, and [uv](https://docs.astral.sh/uv/) on the host.

## 1. Build the container

```shell
docker build -t openshell-jupyter-sandbox:local .
uv venv
source .venv/bin/activate
uv pip install jupyter-server==2.20.0 nbconvert==7.17.1
```

## 2. Launch the service

```shell
openshell sandbox create \
  --name jupyter-demo \
  --from openshell-jupyter-sandbox:local \
  --policy policy.yaml \
  --env HOME=/sandbox \
  --expose 8888 \
  --detach --no-tty \
  -- jupyter server \
    --ServerApp.ip=127.0.0.1 \
    --ServerApp.port=8888 \
    --ServerApp.port_retries=0 \
    --ServerApp.open_browser=False \
    --ServerApp.root_dir=/sandbox \
    --ServerApp.terminals_enabled=False \
    --ServerApp.allow_remote_access=True \
    --ServerApp.disable_check_xsrf=True \
    --IdentityProvider.token=''
```

The CLI prints the exposed service URL after the sandbox is ready. Jupyter
starts as the sandbox's main process and listens on its loopback port.

This example disables Jupyter authentication and XSRF checks because Jupyter's
remote kernel client does not authenticate its WebSocket connection. Use it only
with a local, loopback-bound gateway: any local process that can reach the
service URL can run code in the sandbox.

## 3. Execute the notebook on the remote kernel

Set `JUPYTER_GATEWAY_URL` to the service URL printed in step 2:

```shell
export JUPYTER_GATEWAY_URL='http://default--jupyter-demo.openshell.localhost:<gateway-port>/'
export JUPYTER_CONFIG_PATH="$PWD"
jupyter nbconvert --execute --to notebook demo.ipynb
```

The JSON config in this directory selects Jupyter's remote kernel manager.
`demo.nbconvert.ipynb` stays on your computer and contains the output `285`;
its code runs in the sandbox kernel.

When finished, delete the sandbox and its service:

```shell
openshell sandbox delete jupyter-demo
```
