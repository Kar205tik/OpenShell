#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERIFY="${ROOT}/tasks/scripts/verify-macos-vm-driver.sh"

if [[ $(uname -s) != Darwin ]]; then
  echo "error: the VM-driver signature regression requires macOS" >&2
  exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/openshell-vm-signature-test.XXXXXXXX")
trap 'rm -rf "$work"' EXIT

# Use a real Mach-O and Apple's signing tools. Mock codesign output would not
# detect a signature invalidated after signing or lost during archive assembly.
printf 'int main(void) { return 0; }\n' | /usr/bin/cc -x c - -o "$work/unsigned"
/usr/bin/codesign --remove-signature "$work/unsigned"

check_archive() {
  local label=$1
  local binary=$2
  local expected=$3
  local diagnostic=${4:-}
  local case_dir="$work/cases/$label"
  mkdir -p "$case_dir/raw" "$case_dir/extracted"
  cp "$binary" "$case_dir/raw/openshell-driver-vm"
  chmod +x "$case_dir/raw/openshell-driver-vm"
  tar -czf "$case_dir/driver.tar.gz" -C "$case_dir/raw" openshell-driver-vm
  tar -xzf "$case_dir/driver.tar.gz" -C "$case_dir/extracted"
  cmp "$binary" "$case_dir/extracted/openshell-driver-vm"

  if bash "$VERIFY" "$case_dir/extracted/openshell-driver-vm" >"$case_dir/result" 2>&1; then
    if [[ $expected != pass ]]; then
      echo "FAIL: $label unexpectedly passed verification" >&2
      exit 1
    fi
  elif [[ $expected == pass ]]; then
    cat "$case_dir/result" >&2
    echo "FAIL: $label was rejected" >&2
    exit 1
  elif ! grep -Fq "$diagnostic" "$case_dir/result"; then
    cat "$case_dir/result" >&2
    echo "FAIL: $label failed without the expected diagnostic" >&2
    exit 1
  fi
  echo "PASS: $label"
}

cp "$work/unsigned" "$work/signed"
/usr/bin/codesign --force --sign - --entitlements "$ROOT/crates/openshell-driver-vm/entitlements.plist" "$work/signed"
check_archive valid "$work/signed" pass
check_archive unsigned "$work/unsigned" fail 'code object is not signed'

cp "$work/unsigned" "$work/no-entitlement"
/usr/bin/codesign --force --sign - "$work/no-entitlement"
check_archive missing-entitlement "$work/no-entitlement" fail 'com.apple.security.hypervisor'

for kind in false string; do
  cp "$ROOT/crates/openshell-driver-vm/entitlements.plist" "$work/$kind.plist"
  if [[ $kind == false ]]; then
    /usr/bin/plutil -replace 'com\.apple\.security\.hypervisor' -bool NO "$work/$kind.plist"
  else
    /usr/bin/plutil -replace 'com\.apple\.security\.hypervisor' -string true "$work/$kind.plist"
  fi
  cp "$work/unsigned" "$work/$kind"
  /usr/bin/codesign --force --sign - --entitlements "$work/$kind.plist" "$work/$kind"
  check_archive "$kind-entitlement" "$work/$kind" fail 'com.apple.security.hypervisor'
done

# Changing a signed byte simulates a packaging tool rewriting the executable
# after signing. Inspecting only its entitlement plist would miss this defect.
cp "$work/signed" "$work/tampered"
printf '\001' | dd of="$work/tampered" bs=1 seek=4096 count=1 conv=notrunc 2>/dev/null
check_archive tampered "$work/tampered" fail 'invalid signature'

echo "macOS VM-driver signature regressions passed"
