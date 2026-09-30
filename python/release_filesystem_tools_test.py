# SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import runpy
import subprocess
import textwrap
from pathlib import Path

import pytest


@pytest.mark.parametrize("prefix", ["opt/homebrew", "usr/local", "private/tools"])
@pytest.mark.parametrize("service_path", [None, "/usr/bin:/bin", "", "operator"])
def test_homebrew_service_finds_its_filesystem_tools_without_shell_setup(
    tmp_path: Path, prefix: str, service_path: str | None
) -> None:
    repo_root = Path(__file__).resolve().parents[1]
    release = runpy.run_path(str(repo_root / "tasks/scripts/release.py"))
    formula = release["render_homebrew_formula"](
        release_tag="v0.1.2",
        cli_sha256="a" * 64,
        gateway_sha256="b" * 64,
        driver_vm_sha256="c" * 64,
        prover_sha256="d" * 64,
    )
    assert 'depends_on "e2fsprogs"' in formula
    wrapper = formula.split(".write <<~SH\n", 1)[1].split("\n    SH", 1)[0]
    # Exercise the generated service environment without installing packages or
    # starting a gateway. Homebrew supplies these Ruby interpolation values.
    wrapper = textwrap.dedent(wrapper.split("      docker_tls_dir=", 1)[0])
    tools = tmp_path / prefix / "opt/e2fsprogs"
    (tools / "sbin").mkdir(parents=True)
    (tools / "bin").mkdir()
    operator_bin = tmp_path / "operator/bin"
    operator_bin.mkdir(parents=True)
    selected_bin = operator_bin if service_path == "operator" else tools / "sbin"
    # Unique fixture names keep a runner's preinstalled e2fsprogs from satisfying
    # lookup. A conflicting operator copy still proves inherited PATH wins.
    for name in ("mke2fs", "debugfs", "e2fsck"):
        command = f"openshell-test-{name}"
        for directory in (tools / "sbin", operator_bin):
            tool = directory / command
            tool.write_text(f"#!/bin/sh\necho '{name} 1.47.4'\n", encoding="utf-8")
            tool.chmod(0o755)
        wrapper += f"command -v {command}\n{command} -V\n"
    wrapper = wrapper.replace('#{Formula["e2fsprogs"].opt_sbin}', str(tools / "sbin"))
    wrapper = wrapper.replace('#{Formula["e2fsprogs"].opt_bin}', str(tools / "bin"))
    wrapper = wrapper.replace("#{var}", str(tmp_path / "var"))
    environment = {"HOME": str(tmp_path)}
    if service_path is not None:
        environment["PATH"] = (
            str(operator_bin) if service_path == "operator" else service_path
        )
    completed = subprocess.run(
        ["/bin/sh", "-c", wrapper],
        env=environment,
        text=True,
        capture_output=True,
        check=True,
    )
    for name in ("mke2fs", "debugfs", "e2fsck"):
        assert str(selected_bin / f"openshell-test-{name}") in completed.stdout
        assert f"{name} 1.47.4" in completed.stdout
