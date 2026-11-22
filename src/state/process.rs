//! Process identity for queue records.
//!
//! A PID is *not* an identity on Windows or Linux: the kernel recycles the
//! number once the process is reaped. A state record that only remembers `pid`
//! therefore has two failure modes, and both of them are silent:
//!
//! * the owner exits and its PID is handed to an unrelated process, so the dead
//!   record looks alive forever and never gets reaped;
//! * the owner exits and the record is only reaped when somebody happens to read
//!   it, so a phantom waiter or lock stays visible for minutes.
//!
//! The fix is to pin every record to one process *instance* by storing the
//! process creation timestamp next to the PID, and to fall back to a heartbeat
//! lease when the process cannot be inspected at all (access denied).

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Identity of the process that wrote a state record: PID plus the exact moment
/// the process was created. Two processes sharing a PID never share creation
/// time, so the pair survives PID recycling.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// Process creation time, Unix epoch milliseconds.
    pub created_at_ms: u64,
}

/// What the OS says about a recorded process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Liveness {
    /// Same process instance that wrote the record.
    Alive,
    /// No such process (or it has already exited).
    Dead,
    /// A process exists but it is a different instance: the PID was recycled.
    Reused,
    /// The process exists but cannot be inspected (access denied).
    Inaccessible,
}

impl Liveness {
    /// `true` when the record can no longer belong to a running owner.
    pub fn is_stale(self) -> bool {
        matches!(self, Liveness::Dead | Liveness::Reused)
    }
}

impl ProcessIdentity {
    /// Capture the identity of a live process. `None` when the process is gone
    /// or cannot be inspected.
    pub fn capture(pid: u32) -> Option<Self> {
        match probe(pid) {
            Probe::Born(created_at_ms) => Some(Self { pid, created_at_ms }),
            Probe::Gone | Probe::Inaccessible => None,
        }
    }

    pub fn classify(&self) -> Liveness {
        classify(Some(*self), self.pid, None)
    }

    /// Wall-clock instant the process started, for display purposes.
    pub fn started_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.created_at_ms)
    }
}

/// Classify a recorded process.
///
/// `recorded_at` is when the record was written. It is only used for legacy
/// records that predate [`ProcessIdentity`]: a process created *after* the
/// record was written cannot be the owner, which is enough to spot a recycled
/// PID without any extra field.
pub fn classify(
    identity: Option<ProcessIdentity>,
    pid: u32,
    recorded_at: Option<SystemTime>,
) -> Liveness {
    match probe(pid) {
        Probe::Gone => Liveness::Dead,
        Probe::Inaccessible => Liveness::Inaccessible,
        Probe::Born(created_at_ms) => {
            if let Some(identity) = identity {
                return if identity.created_at_ms == created_at_ms {
                    Liveness::Alive
                } else {
                    Liveness::Reused
                };
            }
            if let Some(recorded_at) = recorded_at {
                // A fresh process created after the record was written is a
                // recycled PID, never the original owner.
                if created_at_ms > epoch_millis(recorded_at).saturating_add(PID_REUSE_GRACE_MS) {
                    return Liveness::Reused;
                }
            }
            Liveness::Alive
        }
    }
}

/// Slack when comparing a legacy record timestamp with a process creation time.
/// They come from different clocks, so a couple of seconds of skew is normal.
const PID_REUSE_GRACE_MS: u64 = 5_000;

pub fn epoch_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn system_time_from_unix_secs(secs: i64) -> SystemTime {
    if secs >= 0 {
        UNIX_EPOCH + Duration::from_secs(secs as u64)
    } else {
        UNIX_EPOCH - Duration::from_millis((-secs as u64) * 1_000)
    }
}

enum Probe {
    /// Process exists, with its creation time in Unix epoch milliseconds.
    Born(u64),
    Gone,
    /// Process may exist but could not be queried (access denied).
    Inaccessible,
}

#[cfg(windows)]
fn probe(pid: u32) -> Probe {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, FILETIME, GetLastError,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return if GetLastError() == ERROR_ACCESS_DENIED {
                Probe::Inaccessible
            } else {
                Probe::Gone
            };
        }

        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        CloseHandle(handle);

        if ok == 0 {
            return Probe::Inaccessible;
        }
        Probe::Born(filetime_to_epoch_millis(created))
    }
}

#[cfg(windows)]
fn filetime_to_epoch_millis(ft: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    // FILETIME counts 100ns intervals since 1601-01-01 UTC.
    (ticks / 10_000).saturating_sub(FILETIME_UNIX_EPOCH_OFFSET_MS)
}

#[cfg(windows)]
const FILETIME_UNIX_EPOCH_OFFSET_MS: u64 = 11_644_473_600_000;

#[cfg(target_os = "linux")]
fn probe(pid: u32) -> Probe {
    use std::io::ErrorKind;

    // Guard against invalid or negative pid_t conversions (kill(-pid, 0) targets a pgrp).
    if pid == 0 || pid > i32::MAX as u32 {
        return Probe::Gone;
    }

    let stat_path = format!("/proc/{}/stat", pid);
    let content = match std::fs::read_to_string(&stat_path) {
        Ok(c) => c,
        Err(e) if e.kind() == ErrorKind::NotFound => return Probe::Gone,
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            return probe_kill_fallback(pid);
        }
        Err(_) => return probe_kill_fallback(pid),
    };

    // Field 2 (comm) is wrapped in parentheses and may contain spaces or ')'.
    // Everything after the last ')' starts at field 3 (process state).
    let Some(close_paren) = content.rfind(')') else {
        return Probe::Inaccessible;
    };
    let rest = content[close_paren + 1..].trim_start();
    let mut fields = rest.split_whitespace();

    // Field 3: state ('Z' = zombie, 'X'/'x' = dead).
    let Some(state) = fields.next() else {
        return Probe::Inaccessible;
    };
    if matches!(state, "Z" | "X" | "x") {
        return Probe::Gone;
    }

    // Field 22 (starttime in clock ticks since boot) is index 18 after consuming field 3.
    let Some(starttime_str) = fields.nth(18) else {
        return Probe::Inaccessible;
    };
    let Ok(starttime_ticks) = starttime_str.parse::<u64>() else {
        return Probe::Inaccessible;
    };

    let Some(btime_secs) = boot_time_secs() else {
        return Probe::Inaccessible;
    };
    let ticks_per_sec = clock_ticks_per_sec();
    let offset_ms = starttime_ticks.saturating_mul(1_000) / ticks_per_sec;
    let created_at_ms = btime_secs.saturating_mul(1_000).saturating_add(offset_ms);
    Probe::Born(created_at_ms)
}

#[cfg(target_os = "linux")]
fn probe_kill_fallback(pid: u32) -> Probe {
    let res = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if res == 0 {
        return Probe::Inaccessible;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EPERM) => Probe::Inaccessible,
        _ => Probe::Gone,
    }
}

#[cfg(target_os = "linux")]
fn boot_time_secs() -> Option<u64> {
    use std::sync::OnceLock;
    static BTIME: OnceLock<Option<u64>> = OnceLock::new();
    *BTIME.get_or_init(|| {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        for line in stat.lines() {
            if let Some(rest) = line.strip_prefix("btime ") {
                return rest.trim().parse::<u64>().ok();
            }
        }
        None
    })
}

#[cfg(target_os = "linux")]
fn clock_ticks_per_sec() -> u64 {
    use std::sync::OnceLock;
    static CLK_TCK: OnceLock<u64> = OnceLock::new();
    *CLK_TCK.get_or_init(|| {
        let val = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if val > 0 { val as u64 } else { 100 }
    })
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn probe(pid: u32) -> Probe {
    let _ = pid;
    Probe::Inaccessible
}
