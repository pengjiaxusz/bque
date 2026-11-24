use anyhow::{Result, anyhow};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

/// Base names of the global kernel mutexes. The session-local `Local\` prefix
/// makes them per-remote-session, which is what the queue wants.
pub const BUILD_MUTEX_BASE: &str = "Bque_Global_Build_Lock";
pub const COORD_MUTEX_BASE: &str = "Bque_Coord_Lock";
pub const BENCH_MUTEX_BASE: &str = "Bque_Benchmark_Lock";

/// Namespace override used by tests (and by parallel experiments) so they can
/// hold a private queue instead of the machine-wide one.
pub const MUTEX_NAMESPACE_ENV: &str = "BQUE_MUTEX_NAMESPACE";

#[allow(dead_code)]
pub fn build_mutex_name() -> String {
    scoped_mutex_name(BUILD_MUTEX_BASE)
}

pub fn coord_mutex_name() -> String {
    scoped_mutex_name(COORD_MUTEX_BASE)
}

pub fn bench_mutex_name() -> String {
    scoped_mutex_name(BENCH_MUTEX_BASE)
}

fn scoped_mutex_name(base: &str) -> String {
    match std::env::var(MUTEX_NAMESPACE_ENV) {
        Ok(namespace) if !namespace.is_empty() => format!("Local\\{}_{}", namespace, base),
        _ => format!("Local\\{}", base),
    }
}

pub struct NamedMutexGuard {
    #[cfg(windows)]
    handle: HANDLE,
    #[cfg(unix)]
    file: std::fs::File,
    #[allow(dead_code)]
    name: String,
    was_abandoned: bool,
}

impl NamedMutexGuard {
    #[allow(dead_code)]
    pub fn was_abandoned(&self) -> bool {
        self.was_abandoned
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for NamedMutexGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            if !self.handle.is_null() {
                ReleaseMutex(self.handle);
                CloseHandle(self.handle);
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // Explicitly unlock before closing; keep the lock file on disk to
            // avoid flock inode races between concurrent waiters.
            unsafe {
                libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

pub struct NamedMutex {
    name: String,
}

#[cfg(unix)]
fn unix_lock_file_path(name: &str) -> Result<std::path::PathBuf> {
    let locks_dir = crate::state::storage::resolve_state_dir().join("locks");
    std::fs::create_dir_all(&locks_dir)?;

    let trimmed = name.strip_prefix("Local\\").unwrap_or(name);
    let sanitized: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Ok(locks_dir.join(format!("{}.lock", sanitized)))
}

#[cfg(unix)]
fn open_lock_file(name: &str) -> Result<std::fs::File> {
    let path = unix_lock_file_path(name)?;
    std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| anyhow!("Failed to open lock file '{}': {}", path.display(), e))
}

impl NamedMutex {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    #[cfg(windows)]
    pub fn acquire_with_progress<F>(
        &self,
        total_timeout: Duration,
        mut on_wait_tick: F,
    ) -> Result<NamedMutexGuard>
    where
        F: FnMut(Duration) -> Result<()>,
    {
        let wide_name: Vec<u16> = OsStr::new(&self.name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, wide_name.as_ptr());
            if handle.is_null() {
                return Err(anyhow!(
                    "Failed to create or open mutex '{}' (Win32 error: {})",
                    self.name,
                    GetLastError()
                ));
            }

            let start = Instant::now();
            let chunk_ms =
                if total_timeout > Duration::ZERO && total_timeout < Duration::from_secs(1) {
                    total_timeout.as_millis().max(1) as u32
                } else {
                    1000u32
                };

            loop {
                let wait_res = WaitForSingleObject(handle, chunk_ms);
                match wait_res {
                    WAIT_OBJECT_0 => {
                        return Ok(NamedMutexGuard {
                            handle,
                            name: self.name.clone(),
                            was_abandoned: false,
                        });
                    }
                    WAIT_ABANDONED => {
                        return Ok(NamedMutexGuard {
                            handle,
                            name: self.name.clone(),
                            was_abandoned: true,
                        });
                    }
                    WAIT_TIMEOUT => {
                        let elapsed = start.elapsed();
                        if total_timeout > Duration::ZERO && elapsed >= total_timeout {
                            CloseHandle(handle);
                            return Err(anyhow!(
                                "Timed out waiting for lock '{}' after {:?}",
                                self.name,
                                total_timeout
                            ));
                        }
                        on_wait_tick(elapsed)?;
                    }
                    other => {
                        CloseHandle(handle);
                        return Err(anyhow!(
                            "WaitForSingleObject failed on '{}' with code 0x{:X} (Win32 error: {})",
                            self.name,
                            other,
                            GetLastError()
                        ));
                    }
                }
            }
        }
    }

    #[cfg(unix)]
    pub fn acquire_with_progress<F>(
        &self,
        total_timeout: Duration,
        mut on_wait_tick: F,
    ) -> Result<NamedMutexGuard>
    where
        F: FnMut(Duration) -> Result<()>,
    {
        use std::os::unix::io::AsRawFd;

        let file = open_lock_file(&self.name)?;
        let fd = file.as_raw_fd();
        let start = Instant::now();
        let mut last_tick = Instant::now();
        let poll_interval = Duration::from_millis(25);
        let tick_interval = Duration::from_secs(1);

        loop {
            let res = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if res == 0 {
                return Ok(NamedMutexGuard {
                    file,
                    name: self.name.clone(),
                    was_abandoned: false,
                });
            }

            let err = std::io::Error::last_os_error();
            match err.raw_os_error() {
                Some(libc::EWOULDBLOCK) | Some(libc::EINTR) => {
                    let elapsed = start.elapsed();
                    if total_timeout > Duration::ZERO && elapsed >= total_timeout {
                        return Err(anyhow!(
                            "Timed out waiting for lock '{}' after {:?}",
                            self.name,
                            total_timeout
                        ));
                    }
                    if last_tick.elapsed() >= tick_interval {
                        on_wait_tick(elapsed)?;
                        last_tick = Instant::now();
                    }
                    let sleep_for = if total_timeout > Duration::ZERO {
                        poll_interval.min(total_timeout.saturating_sub(elapsed))
                    } else {
                        poll_interval
                    };
                    if !sleep_for.is_zero() {
                        std::thread::sleep(sleep_for);
                    }
                }
                _ => {
                    return Err(anyhow!("flock failed on '{}': {}", self.name, err));
                }
            }
        }
    }

    #[cfg(all(not(windows), not(unix)))]
    pub fn acquire_with_progress<F>(
        &self,
        _total_timeout: Duration,
        _on_wait_tick: F,
    ) -> Result<NamedMutexGuard>
    where
        F: FnMut(Duration) -> Result<()>,
    {
        Ok(NamedMutexGuard {
            name: self.name.clone(),
            was_abandoned: false,
        })
    }

    #[allow(dead_code)]
    #[cfg(windows)]
    pub fn is_held(&self) -> Result<bool> {
        let wide_name: Vec<u16> = OsStr::new(&self.name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, wide_name.as_ptr());
            if handle.is_null() {
                return Err(anyhow!("Failed to test mutex '{}'", self.name));
            }
            let wait_res = WaitForSingleObject(handle, 0);
            match wait_res {
                WAIT_OBJECT_0 => {
                    ReleaseMutex(handle);
                    CloseHandle(handle);
                    Ok(false)
                }
                WAIT_ABANDONED => {
                    ReleaseMutex(handle);
                    CloseHandle(handle);
                    Ok(false)
                }
                WAIT_TIMEOUT => {
                    CloseHandle(handle);
                    Ok(true)
                }
                _ => {
                    CloseHandle(handle);
                    Ok(false)
                }
            }
        }
    }

    #[allow(dead_code)]
    #[cfg(unix)]
    pub fn is_held(&self) -> Result<bool> {
        use std::os::unix::io::AsRawFd;

        let file = open_lock_file(&self.name)?;
        let fd = file.as_raw_fd();
        let res = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if res == 0 {
            unsafe {
                libc::flock(fd, libc::LOCK_UN);
            }
            Ok(false)
        } else {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
                Ok(true)
            } else {
                Err(anyhow!("Failed to test mutex '{}': {}", self.name, err))
            }
        }
    }

    #[allow(dead_code)]
    #[cfg(all(not(windows), not(unix)))]
    pub fn is_held(&self) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_mutex_excludes_second_acquirer() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let name = format!("Local\\BqueUnitLock_{}_{}", std::process::id(), stamp);
        let m1 = NamedMutex::new(name.clone());
        let m2 = NamedMutex::new(name);

        let guard = m1
            .acquire_with_progress(Duration::from_secs(1), |_| Ok(()))
            .expect("first lock acquisition should succeed");

        #[cfg(unix)]
        {
            assert!(m2.is_held().expect("is_held should succeed"));
            let second = m2.acquire_with_progress(Duration::from_millis(120), |_| Ok(()));
            assert!(
                second.is_err(),
                "second lock acquisition should time out while held"
            );
        }

        drop(guard);

        let guard2 = m2
            .acquire_with_progress(Duration::from_secs(1), |_| Ok(()))
            .expect("lock acquisition should succeed after release");
        drop(guard2);
    }
}
