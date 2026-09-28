// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use super::SmokeFixture;

// Match the fallible Windows fixture constructor behind the platform boundary.
#[allow(clippy::unnecessary_wraps)]
pub fn smoke_fixture() -> Result<SmokeFixture, String> {
    // Unix runtimes resolve their image-appropriate default shell when the
    // canonical command is empty, so the smoke scenario needs no override.
    Ok(SmokeFixture::new(Vec::new(), None, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_fixture_uses_the_runtime_default_workload() {
        let fixture = smoke_fixture().unwrap();
        assert!(fixture.create_args().is_empty());
        assert!(fixture.command().is_none());
    }
}
