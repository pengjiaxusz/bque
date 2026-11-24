use anyhow::Result;
#[cfg(windows)]
use anyhow::anyhow;
#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(unix)]
use std::sync::atomic::{AtomicI32, Ordering};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
    JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};

pub struct JobObject {
    #[cfg(windows)]
    handle: HANDLE,
    /// On Unix, tracks the child process group ID (PGID) so that dropping the
    /// job cleans up the entire descendant process group and CPU accounting
    /// can sum every process in that group.
    #[cfg(unix)]
    pgid: AtomicI32,
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

        #[cfg(unix)]
        {
            let _ = name;
            Ok(Self {
                pgid: AtomicI32::new(0),
            })
        }

        #[cfg(all(not(windows), not(unix)))]
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

        #[cfg(unix)]
        {
            if process_handle > 0 && process_handle <= i32::MAX as isize {
                self.pgid.store(process_handle as i32, Ordering::SeqCst);
            }
            Ok(())
        }

        #[cfg(all(not(windows), not(unix)))]
        {
            let _ = process_handle;
            Ok(())
        }
    }

    /// Clear the tracked process group once the child has exited cleanly.
    #[cfg(unix)]
    pub fn clear_process(&self) {
        self.pgid.store(0, Ordering::SeqCst);
    }

    /// Cumulative user + kernel CPU time consumed by every process in this job,
    /// expressed in milliseconds.
    pub fn total_cpu_time_ms(&self) -> Option<u64> {
        #[cfg(windows)]
        unsafe {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            let ok = QueryInformationJobObject(
                self.handle,
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut _,
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            );
            if ok == 0 {
                return None;
            }
            // TotalUserTime and TotalKernelTime are in 100-nanosecond intervals.
            let total_100ns =
                (info.TotalUserTime as u64).saturating_add(info.TotalKernelTime as u64);
            Some(total_100ns / 10_000)
        }

        #[cfg(target_os = "linux")]
        {
            let pgid = self.pgid.load(Ordering::SeqCst);
            if pgid <= 0 {
                return None;
            }
            sum_linux_pgid_cpu_time_ms(pgid as u32)
        }

        #[cfg(all(not(windows), not(target_os = "linux")))]
        {
            None
        }
    }

    /// Forcefully terminate the entire managed child process tree.
    pub fn terminate(&self, exit_code: u32) -> Result<()> {
        #[cfg(windows)]
        unsafe {
            let ok = TerminateJobObject(self.handle, exit_code);
            if ok == 0 {
                return Err(anyhow!(
                    "Failed to terminate Job Object (Win32 error: {})",
                    GetLastError()
                ));
            }
            Ok(())
        }

        #[cfg(unix)]
        {
            let _ = exit_code;
            let pgid = self.pgid.load(Ordering::SeqCst);
            if pgid > 0 {
                unsafe {
                    libc::kill(-pgid, libc::SIGTERM);
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
                unsafe {
                    libc::kill(-pgid, libc::SIGKILL);
                }
            }
            Ok(())
        }

        #[cfg(all(not(windows), not(unix)))]
        {
            let _ = exit_code;
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
        #[cfg(unix)]
        {
            let pgid = self.pgid.swap(0, Ordering::SeqCst);
            if pgid > 0 {
                // Terminate the entire process group if the job is dropped before
                // the child exited normally.
                unsafe {
                    libc::kill(-pgid, libc::SIGTERM);
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn sum_linux_pgid_cpu_time_ms(target_pgid: u32) -> Option<u64> {
    let entries = std::fs::read_dir("/proc").ok()?;
    let mut total_ticks: u64 = 0;
    let mut found_any = false;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        if !pid_str.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let stat_path = format!("/proc/{}/stat", pid_str);
        let Ok(content) = std::fs::read_to_string(&stat_path) else {
            continue;
        };
        if let Some((pgid, utime, stime, cutime, cstime)) = parse_linux_stat_cpu(&content)
            && pgid == target_pgid
        {
            found_any = true;
            // Include waited-for children ticks so short-lived compiler invocations
            // spawned inside the process group are never missed between samples.
            total_ticks = total_ticks
                .saturating_add(utime)
                .saturating_add(stime)
                .saturating_add(cutime)
                .saturating_add(cstime);
        }
    }

    if !found_any {
        return None;
    }

    let ticks_per_sec = linux_clock_ticks_per_sec();
    Some(total_ticks.saturating_mul(1_000) / ticks_per_sec)
}

#[cfg(target_os = "linux")]
fn parse_linux_stat_cpu(stat: &str) -> Option<(u32, u64, u64, u64, u64)> {
    let close_paren = stat.rfind(')')?;
    let rest = stat[close_paren + 1..].trim_start();
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After ')': index 0 = state (field 3), index 2 = pgrp (field 5),
    // index 11 = utime (field 14), index 12 = stime (field 15),
    // index 13 = cutime (field 16), index 14 = cstime (field 17).
    if fields.len() <= 14 {
        return None;
    }
    let pgid = fields[2].parse::<u32>().ok()?;
    let utime = fields[11].parse::<u64>().ok()?;
    let stime = fields[12].parse::<u64>().ok()?;
    let cutime = fields[13].parse::<u64>().ok()?;
    let cstime = fields[14].parse::<u64>().ok()?;
    Some((pgid, utime, stime, cutime, cstime))
}

#[cfg(target_os = "linux")]
fn linux_clock_ticks_per_sec() -> u64 {
    use std::sync::OnceLock;
    static CLK_TCK: OnceLock<u64> = OnceLock::new();
    *CLK_TCK.get_or_init(|| {
        let val = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if val > 0 { val as u64 } else { 100 }
    })
}

/// Convert a cumulative CPU time delta into host-wide percentage (0.0..=100.0).
pub fn cpu_delta_to_host_pct(delta_cpu_ms: u64, elapsed_ms: u64, logical_cores: usize) -> f64 {
    if elapsed_ms == 0 || logical_cores == 0 {
        return 0.0;
    }
    let capacity_ms = (elapsed_ms as f64) * (logical_cores as f64);
    ((delta_cpu_ms as f64 / capacity_ms) * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_delta_normalizes_by_logical_cores() {
        // 1000ms of CPU over 1000ms wall time on a 4-core host = 25% host CPU.
        let pct = cpu_delta_to_host_pct(1000, 1000, 4);
        assert!((pct - 25.0).abs() < 0.01);
        assert_eq!(cpu_delta_to_host_pct(0, 1000, 4), 0.0);
        assert_eq!(cpu_delta_to_host_pct(10_000, 1000, 4), 100.0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_stat_parser_extracts_pgid_and_cpu_ticks() {
        let sample =
            "1234 (cargo build) R 1000 1234 1000 0 -1 4194304 10 0 0 0 42 8 15 5 20 0 1 0 999";
        let parsed = parse_linux_stat_cpu(sample).expect("parse sample");
        assert_eq!(parsed, (1234, 42, 8, 15, 5));
    }
}
