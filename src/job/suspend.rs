use anyhow::{Result, anyhow};

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

        #[cfg(not(windows))]
        {
            let _ = root_pid;
            Ok(vec![])
        }
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

        #[cfg(not(windows))]
        {
            let _ = pids;
            Ok(0)
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
