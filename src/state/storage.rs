use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LockState {
    pub pid: u32,
    pub start_time: DateTime<Utc>,
    pub command: String,
    pub description: String,
    pub cwd: String,
    pub priority: String,
    pub cpu_rate: u32,
    pub is_observer: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WaiterState {
    pub pid: u32,
    pub queued_at: DateTime<Utc>,
    pub command: String,
    pub description: String,
    pub cwd: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BenchState {
    pub pid: u32,
    pub start_time: DateTime<Utc>,
    pub command: String,
    pub description: String,
    pub frozen_pids: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PauseState {
    pub paused_at: DateTime<Utc>,
    pub reason: String,
    pub paused_by_pid: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FullQueueStatus {
    pub active_lock: Option<LockState>,
    pub active_bench: Option<BenchState>,
    pub pause_state: Option<PauseState>,
    pub waiters: Vec<WaiterState>,
    pub observers: Vec<LockState>,
}

pub struct StateStorage {
    root_dir: PathBuf,
}

impl StateStorage {
    pub fn new() -> Result<Self> {
        let root = std::env::temp_dir().join("bque");
        fs::create_dir_all(&root).context("Failed to create bque state directory")?;
        fs::create_dir_all(root.join("waiters")).context("Failed to create waiters directory")?;
        fs::create_dir_all(root.join("observers"))
            .context("Failed to create observers directory")?;
        Ok(Self { root_dir: root })
    }

    pub fn lock_file_path(&self) -> PathBuf {
        self.root_dir.join("lock_state.json")
    }

    pub fn bench_file_path(&self) -> PathBuf {
        self.root_dir.join("bench_state.json")
    }

    pub fn pause_file_path(&self) -> PathBuf {
        self.root_dir.join("pause_state.json")
    }

    pub fn waiter_file_path(&self, pid: u32) -> PathBuf {
        self.root_dir
            .join("waiters")
            .join(format!("waiter_{}.json", pid))
    }

    pub fn observer_file_path(&self, pid: u32) -> PathBuf {
        self.root_dir
            .join("observers")
            .join(format!("observer_{}.json", pid))
    }

    pub fn save_lock_state(&self, state: &LockState) -> Result<()> {
        let content = serde_json::to_string_pretty(state)?;
        atomic_write(&self.lock_file_path(), &content)?;
        Ok(())
    }

    pub fn load_lock_state(&self) -> Result<Option<LockState>> {
        let path = self.lock_file_path();
        let state: Option<LockState> = load_json_file(&path);
        match state {
            Some(s) if is_pid_alive(s.pid) => Ok(Some(s)),
            Some(_) => {
                let _ = fs::remove_file(&path);
                Ok(None)
            }
            None => Ok(None),
        }
    }

    pub fn clear_lock_state(&self, expected_pid: Option<u32>) -> Result<()> {
        let path = self.lock_file_path();
        if !path.exists() {
            return Ok(());
        }

        if let Some(pid) = expected_pid {
            let current = self.load_lock_state()?.map(|s| s.pid);
            if current != Some(pid) {
                return Ok(());
            }
        }

        let _ = fs::remove_file(&path);
        Ok(())
    }

    pub fn register_waiter(&self, state: &WaiterState) -> Result<()> {
        let path = self.waiter_file_path(state.pid);
        let content = serde_json::to_string_pretty(state)?;
        atomic_write(&path, &content)?;
        Ok(())
    }

    pub fn unregister_waiter(&self, pid: u32) -> Result<()> {
        let path = self.waiter_file_path(pid);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn list_waiters(&self) -> Result<Vec<WaiterState>> {
        let dir = self.root_dir.join("waiters");
        let mut list = Vec::new();
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if !path.extension().is_some_and(|ext| ext == "json") {
                    continue;
                }
                if let Some(waiter) = load_json_file::<WaiterState>(&path) {
                    if is_pid_alive(waiter.pid) {
                        list.push(waiter);
                    } else {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
        list.sort_by_key(|w| w.queued_at);
        Ok(list)
    }

    pub fn register_observer(&self, state: &LockState) -> Result<()> {
        let path = self.observer_file_path(state.pid);
        let content = serde_json::to_string_pretty(state)?;
        atomic_write(&path, &content)?;
        Ok(())
    }

    pub fn unregister_observer(&self, pid: u32) -> Result<()> {
        let path = self.observer_file_path(pid);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn list_observers(&self) -> Result<Vec<LockState>> {
        let dir = self.root_dir.join("observers");
        let mut list = Vec::new();
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if !path.extension().is_some_and(|ext| ext == "json") {
                    continue;
                }
                if let Some(obs) = load_json_file::<LockState>(&path) {
                    if is_pid_alive(obs.pid) {
                        list.push(obs);
                    } else {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
        Ok(list)
    }

    pub fn save_bench_state(&self, state: &BenchState) -> Result<()> {
        let content = serde_json::to_string_pretty(state)?;
        atomic_write(&self.bench_file_path(), &content)?;
        Ok(())
    }

    pub fn load_bench_state(&self) -> Result<Option<BenchState>> {
        let path = self.bench_file_path();
        let state: Option<BenchState> = load_json_file(&path);
        match state {
            Some(s) if is_pid_alive(s.pid) => Ok(Some(s)),
            Some(_) => {
                let _ = fs::remove_file(&path);
                Ok(None)
            }
            None => Ok(None),
        }
    }

    pub fn clear_bench_state(&self) -> Result<()> {
        let path = self.bench_file_path();
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn save_pause_state(&self, state: &PauseState) -> Result<()> {
        let content = serde_json::to_string_pretty(state)?;
        atomic_write(&self.pause_file_path(), &content)?;
        Ok(())
    }

    pub fn load_pause_state(&self) -> Result<Option<PauseState>> {
        let path = self.pause_file_path();
        Ok(load_json_file(&path))
    }

    pub fn clear_pause_state(&self) -> Result<()> {
        let path = self.pause_file_path();
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn get_full_status(&self) -> Result<FullQueueStatus> {
        self.clean_dead_entries()?;
        Ok(FullQueueStatus {
            active_lock: self.load_lock_state()?,
            active_bench: self.load_bench_state()?,
            pause_state: self.load_pause_state()?,
            waiters: self.list_waiters()?,
            observers: self.list_observers()?,
        })
    }

    pub fn clean_dead_entries(&self) -> Result<u32> {
        let mut cleaned = 0;
        let lock_path = self.lock_file_path();
        if lock_path.exists() {
            let should_clean = match load_json_file::<LockState>(&lock_path) {
                Some(state) => !is_pid_alive(state.pid),
                None => true,
            };
            if should_clean {
                let _ = fs::remove_file(&lock_path);
                cleaned += 1;
            }
        }

        let bench_path = self.bench_file_path();
        if bench_path.exists() {
            let should_clean = match load_json_file::<BenchState>(&bench_path) {
                Some(state) => !is_pid_alive(state.pid),
                None => true,
            };
            if should_clean {
                let _ = fs::remove_file(&bench_path);
                cleaned += 1;
            }
        }

        for waiter in self.list_waiters()? {
            if !is_pid_alive(waiter.pid) {
                let _ = self.unregister_waiter(waiter.pid);
                cleaned += 1;
            }
        }

        for obs in self.list_observers()? {
            if !is_pid_alive(obs.pid) {
                let _ = self.unregister_observer(obs.pid);
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }
}

pub fn is_pid_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, FALSE, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
            if handle.is_null() {
                return false;
            }
            let mut exit_code: u32 = 0;
            let success = GetExitCodeProcess(handle, &mut exit_code);
            CloseHandle(handle);
            success != 0 && exit_code == STILL_ACTIVE as u32
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        true
    }
}

fn load_json_file<T: DeserializeOwned>(path: &Path) -> Option<T> {
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
