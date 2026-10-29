use anyhow::{Result, anyhow};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::time::{Duration, Instant};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

pub const BUILD_MUTEX_NAME: &str = "Local\\Bque_Global_Build_Lock";
pub const BENCH_MUTEX_NAME: &str = "Local\\Bque_Benchmark_Lock";

pub struct NamedMutexGuard {
    #[cfg(windows)]
    handle: HANDLE,
    #[allow(dead_code)]
    name: String,
    was_abandoned: bool,
}

impl NamedMutexGuard {
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
    }
}

pub struct NamedMutex {
    name: String,
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
            let chunk_ms = 1000u32;

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

    #[cfg(not(windows))]
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
    #[cfg(not(windows))]
    pub fn is_held(&self) -> Result<bool> {
        Ok(false)
    }
}
