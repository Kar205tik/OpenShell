// SPDX-FileCopyrightText: Copyright (c) 2025-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Native workload tree lifetime. The child starts suspended until job assignment.
#![allow(unsafe_code)]

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

pub(crate) struct ProcessJob(OwnedHandle);

impl ProcessJob {
    pub(crate) fn new() -> Result<Self, String> {
        // SAFETY: all handles returned by these calls transfer to OwnedHandle;
        // the initialized limits buffer remains live for SetInformationJobObject.
        unsafe {
            let handle = CreateJobObjectW(None, None).map_err(|e| e.to_string())?;
            let job = Self(OwnedHandle::from_raw_handle(handle.0));
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of_val(&limits) as u32,
            )
            .map_err(|e| e.to_string())?;
            Ok(job)
        }
    }

    pub(crate) fn assign_and_resume(&self, child: &tokio::process::Child) -> Result<(), String> {
        let pid = child
            .id()
            .ok_or("MXC workload exited before job assignment")?;
        // SAFETY: the child handle remains open throughout assignment and thread
        // lookup. CREATE_SUSPENDED prevents user code from spawning descendants
        // before job assignment. No breakaway flag is enabled on this job.
        unsafe {
            AssignProcessToJobObject(
                HANDLE(self.0.as_raw_handle()),
                HANDLE(
                    child
                        .raw_handle()
                        .ok_or("MXC workload handle unavailable")?,
                ),
            )
            .map_err(|e| format!("assign MXC workload job: {e}"))?;
            let snapshot =
                CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(|e| e.to_string())?;
            let snapshot = OwnedHandle::from_raw_handle(snapshot.0);
            let mut entry = THREADENTRY32 {
                dwSize: size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            Thread32First(HANDLE(snapshot.as_raw_handle()), &raw mut entry)
                .map_err(|e| e.to_string())?;
            loop {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
                        .map_err(|e| e.to_string())?;
                    let thread = OwnedHandle::from_raw_handle(thread.0);
                    if ResumeThread(HANDLE(thread.as_raw_handle())) == u32::MAX {
                        return Err(std::io::Error::last_os_error().to_string());
                    }
                    return Ok(());
                }
                entry.dwSize = size_of::<THREADENTRY32>() as u32;
                Thread32Next(HANDLE(snapshot.as_raw_handle()), &raw mut entry)
                    .map_err(|e| format!("find suspended MXC workload thread: {e}"))?;
            }
        }
    }

    pub(crate) fn terminate(&self) -> Result<(), String> {
        // SAFETY: this object owns a live job handle, including after its root exits.
        unsafe { TerminateJobObject(HANDLE(self.0.as_raw_handle()), 1) }
            .map_err(|e| format!("terminate MXC workload tree: {e}"))
    }
}
