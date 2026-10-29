use anyhow::{Result, anyhow};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
    JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_CPU_RATE_CONTROL_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation, SetInformationJobObject,
};

pub struct JobObject {
    #[cfg(windows)]
    handle: HANDLE,
}

unsafe impl Send for JobObject {}
unsafe impl Sync for JobObject {}

impl JobObject {
    pub fn create(name: Option<&str>) -> Result<Self> {
        #[cfg(windows)]
        unsafe {
            let wide_name: Option<Vec<u16>> = name.map(|n| {
                OsStr::new(n)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect()
            });

            let name_ptr = wide_name.as_ref().map_or(std::ptr::null(), |w| w.as_ptr());

            let handle = CreateJobObjectW(std::ptr::null(), name_ptr);
            if handle.is_null() {
                return Err(anyhow!(
                    "Failed to create Job Object (Win32 error: {})",
                    GetLastError()
                ));
            }

            // By default, kill child process tree if parent job handle closes
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let success = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );

            if success == 0 {
                CloseHandle(handle);
                return Err(anyhow!(
                    "Failed to set default Job Object limits (Win32 error: {})",
                    GetLastError()
                ));
            }

            Ok(Self { handle })
        }

        #[cfg(not(windows))]
        {
            let _ = name;
            Ok(Self {})
        }
    }

    #[allow(dead_code)]
    #[cfg(windows)]
    pub fn raw_handle(&self) -> HANDLE {
        self.handle
    }

    pub fn assign_process(&self, process_handle: isize) -> Result<()> {
        #[cfg(windows)]
        unsafe {
            let res = AssignProcessToJobObject(self.handle, process_handle as HANDLE);
            if res == 0 {
                return Err(anyhow!(
                    "Failed to assign process to Job Object (Win32 error: {})",
                    GetLastError()
                ));
            }
            Ok(())
        }

        #[cfg(not(windows))]
        {
            let _ = process_handle;
            Ok(())
        }
    }

    pub fn set_cpu_rate(&self, rate_percent: u32, hard_cap: bool) -> Result<()> {
        let clamped = rate_percent.clamp(1, 100);
        #[cfg(windows)]
        unsafe {
            let mut flags = JOB_OBJECT_CPU_RATE_CONTROL_ENABLE;
            if hard_cap {
                flags |= JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
            }

            let mut info: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION = std::mem::zeroed();
            info.ControlFlags = flags;
            info.Anonymous.CpuRate = clamped * 100; // 1% = 100, 100% = 10000

            let success = SetInformationJobObject(
                self.handle,
                JobObjectCpuRateControlInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
            );

            if success == 0 {
                return Err(anyhow!(
                    "Failed to set CPU rate to {}% (Win32 error: {})",
                    clamped,
                    GetLastError()
                ));
            }
            Ok(())
        }

        #[cfg(not(windows))]
        {
            let _ = clamped;
            let _ = hard_cap;
            Ok(())
        }
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}
