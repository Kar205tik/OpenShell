#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: verify-macos-vm-driver.sh <openshell-driver-vm>" >&2
  exit 2
fi

if [[ $(uname -s) != Darwin ]]; then
  echo "error: macOS is required to verify the VM driver's code signature" >&2
  exit 2
fi

binary=$1
if [[ $binary != /* ]]; then
  binary="$PWD/$binary"
fi
if [[ ! -f $binary || ! -x $binary ]]; then
  echo "error: VM driver is not an executable file: $binary" >&2
  exit 1
fi

# Verify the installed or extracted bytes, without signing or otherwise repairing
# them. A successful codesign display alone does not validate the signature.
/usr/bin/codesign --verify --strict --all-architectures "$binary"

entitlements=$(mktemp "${TMPDIR:-/tmp}/openshell-vm-entitlements.XXXXXXXX")
trap 'rm -f "$entitlements"' EXIT
/usr/bin/codesign --display --entitlements - --xml "$binary" >"$entitlements"

# Dots belong to the entitlement name, not nested dictionary keys. Require a
# boolean true: a string that happens to print "true" is not an entitlement grant.
if ! granted=$(/usr/bin/plutil -extract 'com\.apple\.security\.hypervisor' raw -expect bool "$entitlements") || [[ $granted != true ]]; then
  echo "error: VM driver must have the boolean com.apple.security.hypervisor entitlement set to true" >&2
  exit 1
fi

echo "Verified VM driver signature and Hypervisor entitlement: $binary"
