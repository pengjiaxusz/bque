//! Process identity for queue records.
//!
//! A PID is *not* an identity on Windows: the kernel recycles the number as soon
//! as the process object is released. A state record that only remembers `pid`
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

const FILETIME_UNIX_EPOCH_OFFSET_MS: u64 = 11_644_473_600_000;

#[cfg(not(windows))]
fn probe(pid: u32) -> Probe {
    // bque is a Windows-only global lock; elsewhere records fall back to leases.
    let _ = pid;
    Probe::Inaccessible
}
