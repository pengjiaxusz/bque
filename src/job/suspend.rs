use crate::state::process::{Liveness, ProcessIdentity};
use anyhow::Result;
#[cfg(windows)]
use anyhow::anyhow;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SUSPEND_RESUME};

#[cfg(windows)]
type NtSuspendProcessFn = unsafe extern "system" fn(HANDLE) -> i32;
#[cfg(windows)]
type NtResumeProcessFn = unsafe extern "system" fn(HANDLE) -> i32;

pub struct ProcessFreezer {
    #[cfg(windows)]
    suspend_fn: Option<NtSuspendProcessFn>,
    #[cfg(windows)]
    resume_fn: Option<NtResumeProcessFn>,
}

impl ProcessFreezer {
    pub fn new() -> Self {
        #[cfg(windows)]
        unsafe {
            let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr() as *const u8);
            if ntdll.is_null() {
                return Self {
                    suspend_fn: None,
                    resume_fn: None,
                };
            }

            let suspend_ptr = GetProcAddress(ntdll, c"NtSuspendProcess".as_ptr() as *const u8);
            let resume_ptr = GetProcAddress(ntdll, c"NtResumeProcess".as_ptr() as *const u8);

            Self {
                suspend_fn: suspend_ptr.map(|p| std::mem::transmute(p)),
                resume_fn: resume_ptr.map(|p| std::mem::transmute(p)),
            }
        }

        #[cfg(not(windows))]
        {
            Self {}
        }
    }

    pub fn suspend_process_tree(&self, root_pid: u32) -> Result<Vec<u32>> {
        #[cfg(windows)]
        {
            let suspend_fn = self
                .suspend_fn
                .ok_or_else(|| anyhow!("NtSuspendProcess not available in ntdll"))?;

            let mut pids = vec![root_pid];
            pids.extend(find_child_pids(root_pid));

            let mut suspended = Vec::new();
            for &pid in &pids {
                unsafe {
                    let handle = OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid);
                    if !handle.is_null() {
                        let status = suspend_fn(handle);
                        CloseHandle(handle);
                        if status >= 0 {
                            suspended.push(pid);
                        }
                    }
                }
            }
            Ok(suspended)
        }

        #[cfg(target_os = "linux")]
        {
            if root_pid == 0 || root_pid > i32::MAX as u32 {
                return Ok(Vec::new());
            }
            let mut pids = vec![root_pid];
            pids.extend(find_child_pids_linux(root_pid));

            let mut suspended = Vec::new();
            for &pid in &pids {
                if pid == 0 || pid > i32::MAX as u32 {
                    continue;
                }
                let res = unsafe { libc::kill(pid as libc::pid_t, libc::SIGSTOP) };
                if res == 0 {
                    suspended.push(pid);
                }
            }
            Ok(suspended)
        }

        #[cfg(all(not(windows), not(target_os = "linux")))]
        {
            let _ = root_pid;
            Ok(vec![])
        }
    }

    /// Suspend a process tree and capture `(pid, created_at_ms)` identities so
    /// later recovery never resumes an unrelated process after PID reuse.
    pub fn suspend_process_tree_identities(&self, root_pid: u32) -> Result<Vec<ProcessIdentity>> {
        let pids = self.suspend_process_tree(root_pid)?;
        let mut identities = Vec::with_capacity(pids.len());
        for pid in pids {
            if let Some(id) = ProcessIdentity::capture(pid) {
                identities.push(id);
            }
        }
        Ok(identities)
    }

    pub fn resume_processes(&self, pids: &[u32]) -> Result<u32> {
        #[cfg(windows)]
        {
            let resume_fn = self
                .resume_fn
                .ok_or_else(|| anyhow!("NtResumeProcess not available in ntdll"))?;

            let mut resumed_count = 0;
            for &pid in pids {
                unsafe {
                    let handle = OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid);
                    if !handle.is_null() {
                        let status = resume_fn(handle);
                        CloseHandle(handle);
                        if status >= 0 {
                            resumed_count += 1;
                        }
                    }
                }
            }
            Ok(resumed_count)
        }

        #[cfg(target_os = "linux")]
        {
            let mut resumed_count = 0;
            for &pid in pids {
                if pid == 0 || pid > i32::MAX as u32 {
                    continue;
                }
                let res = unsafe { libc::kill(pid as libc::pid_t, libc::SIGCONT) };
                if res == 0 {
                    resumed_count += 1;
                }
            }
            Ok(resumed_count)
        }

        #[cfg(all(not(windows), not(target_os = "linux")))]
        {
            let _ = pids;
            Ok(0)
        }
    }

    /// Resume only processes whose instance identity still matches, preventing
    /// signals/resumes from targeting a recycled PID.
    pub fn resume_identities(&self, identities: &[ProcessIdentity]) -> Result<u32> {
        let live_pids: Vec<u32> = identities
            .iter()
            .filter(|id| id.classify() == Liveness::Alive)
            .map(|id| id.pid)
            .collect();
        if live_pids.is_empty() {
            return Ok(0);
        }
        self.resume_processes(&live_pids)
    }
}

/// RAII guard that guarantees frozen processes are resumed on every exit path
/// (normal return, error propagation, or panic).
pub struct FreezeGuard {
    identities: Vec<ProcessIdentity>,
    disarmed: bool,
}

impl FreezeGuard {
    pub fn new(identities: Vec<ProcessIdentity>) -> Self {
        Self {
            identities,
            disarmed: false,
        }
    }

    pub fn resume_now(&mut self) -> Result<u32> {
        self.disarmed = true;
        if self.identities.is_empty() {
            return Ok(0);
        }
        let freezer = ProcessFreezer::new();
        freezer.resume_identities(&self.identities)
    }
}

impl Drop for FreezeGuard {
    fn drop(&mut self) {
        if !self.disarmed && !self.identities.is_empty() {
            let freezer = ProcessFreezer::new();
            let _ = freezer.resume_identities(&self.identities);
        }
    }
}

#[cfg(windows)]
fn find_child_pids(parent_pid: u32) -> Vec<u32> {
    let mut children = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() {
            return children;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ParentProcessID == parent_pid {
                    let child_pid = entry.th32ProcessID;
                    children.push(child_pid);
                    children.extend(find_child_pids(child_pid));
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    children
}

#[cfg(target_os = "linux")]
fn find_child_pids_linux(root_pid: u32) -> Vec<u32> {
    use std::collections::{HashMap, VecDeque};

    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if let Some(ppid) = read_ppid_linux(pid) {
            children_by_parent.entry(ppid).or_default().push(pid);
        }
    }

    let mut descendants = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(root_pid);
    while let Some(parent) = queue.pop_front() {
        if let Some(kids) = children_by_parent.get(&parent) {
            for &kid in kids {
                descendants.push(kid);
                queue.push_back(kid);
            }
        }
    }
    descendants
}

#[cfg(target_os = "linux")]
fn read_ppid_linux(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    let close_paren = stat.rfind(')')?;
    let rest = stat[close_paren + 1..].trim_start();
    // After ')': field 3 = state (index 0), field 4 = ppid (index 1).
    rest.split_whitespace().nth(1)?.parse::<u32>().ok()
}
