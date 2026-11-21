use crate::state::process::{Liveness, ProcessIdentity, classify, system_time_from_unix_secs};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How often a lock holder refreshes its heartbeat.
pub const LOCK_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// A lock record whose owner cannot be inspected is dropped after this lease.
pub const LOCK_LEASE: Duration = Duration::from_secs(90);
/// How often an observer refreshes its heartbeat.
pub const OBSERVER_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// How often a queued waiter refreshes its heartbeat.
pub const WAITER_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
/// A waiter record with an uninspectable owner is dropped after this lease.
pub const WAITER_LEASE: Duration = Duration::from_secs(300);
/// How often a benchmark refreshes its heartbeat.
pub const BENCH_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// Benchmark records have no interactive owner to re-register them, so their
/// lease only covers the "cannot inspect the process" case.
pub const BENCH_LEASE: Duration = Duration::from_secs(120);

/// Environment override for the state directory. Used by tests and by
/// diagnostics that need to inspect an isolated queue.
pub const STATE_DIR_ENV: &str = "BQUE_STATE_DIR";

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
    /// Instance identity of `pid`; absent in records written by older versions.
    #[serde(default)]
    pub identity: Option<ProcessIdentity>,
    /// Last time the owner proved it was still running.
    #[serde(default)]
    pub heartbeat: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WaiterState {
    pub pid: u32,
    pub queued_at: DateTime<Utc>,
    pub command: String,
    pub description: String,
    pub cwd: String,
    #[serde(default)]
    pub identity: Option<ProcessIdentity>,
    #[serde(default)]
    pub heartbeat: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BenchState {
    pub pid: u32,
    pub start_time: DateTime<Utc>,
    pub command: String,
    pub description: String,
    /// Working directory of the benchmark; absent in records written by older versions.
    #[serde(default)]
    pub cwd: String,
    pub frozen_pids: Vec<u32>,
    #[serde(default)]
    pub identity: Option<ProcessIdentity>,
    #[serde(default)]
    pub heartbeat: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PauseState {
    pub paused_at: DateTime<Utc>,
    pub reason: String,
    pub paused_by_pid: u32,
}

/// What a cleanup pass removed, so callers can report it instead of guessing.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ReapReport {
    #[serde(default)]
    pub lock: bool,
    #[serde(default)]
    pub bench: bool,
    #[serde(default)]
    pub waiters: u32,
    #[serde(default)]
    pub observers: u32,
    #[serde(default)]
    pub pause: bool,
}

impl ReapReport {
    pub fn total(&self) -> u32 {
        u32::from(self.lock)
            + u32::from(self.bench)
            + self.waiters
            + self.observers
            + u32::from(self.pause)
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.lock {
            parts.push("active lock".to_string());
        }
        if self.bench {
            parts.push("benchmark".to_string());
        }
        if self.waiters > 0 {
            parts.push(format!("{} waiter(s)", self.waiters));
        }
        if self.observers > 0 {
            parts.push(format!("{} observer(s)", self.observers));
        }
        if self.pause {
            parts.push("manual pause".to_string());
        }
        if parts.is_empty() {
            "nothing".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FullQueueStatus {
    pub active_lock: Option<LockState>,
    pub active_bench: Option<BenchState>,
    pub pause_state: Option<PauseState>,
    pub waiters: Vec<WaiterState>,
    pub observers: Vec<LockState>,
    /// Stale records dropped while building this snapshot.
    #[serde(default, skip_serializing_if = "ReapReport::is_empty")]
    pub reaped: ReapReport,
}

impl LockState {
    pub fn record(
        pid: u32,
        command: String,
        description: String,
        cwd: String,
        priority: String,
        cpu_rate: u32,
        is_observer: bool,
    ) -> Self {
        Self {
            pid,
            start_time: Utc::now(),
            command,
            description,
            cwd,
            priority,
            cpu_rate,
            is_observer,
            identity: ProcessIdentity::capture(pid),
            heartbeat: Some(Utc::now()),
        }
    }
}

impl WaiterState {
    pub fn record(pid: u32, command: String, description: String, cwd: String) -> Self {
        Self {
            pid,
            queued_at: Utc::now(),
            command,
            description,
            cwd,
            identity: ProcessIdentity::capture(pid),
            heartbeat: Some(Utc::now()),
        }
    }
}

impl BenchState {
    pub fn record(
        pid: u32,
        command: String,
        description: String,
        cwd: String,
        frozen_pids: Vec<u32>,
    ) -> Self {
        Self {
            pid,
            start_time: Utc::now(),
            command,
            description,
            cwd,
            frozen_pids,
            identity: ProcessIdentity::capture(pid),
            heartbeat: Some(Utc::now()),
        }
    }
}

/// Records that carry an owner PID and a heartbeat, so they can be refreshed and
/// aged out through one code path.
trait HeartbeatRecord: Serialize + DeserializeOwned {
    fn record_pid(&self) -> u32;
    fn set_heartbeat(&mut self, at: DateTime<Utc>);
}

impl HeartbeatRecord for LockState {
    fn record_pid(&self) -> u32 {
        self.pid
    }
    fn set_heartbeat(&mut self, at: DateTime<Utc>) {
        self.heartbeat = Some(at);
    }
}

impl HeartbeatRecord for WaiterState {
    fn record_pid(&self) -> u32 {
        self.pid
    }
    fn set_heartbeat(&mut self, at: DateTime<Utc>) {
        self.heartbeat = Some(at);
    }
}

impl HeartbeatRecord for BenchState {
    fn record_pid(&self) -> u32 {
        self.pid
    }
    fn set_heartbeat(&mut self, at: DateTime<Utc>) {
        self.heartbeat = Some(at);
    }
}

#[derive(Clone)]
pub struct StateStorage {
    root_dir: PathBuf,
}

impl StateStorage {
    pub fn new() -> Result<Self> {
        let root = match std::env::var_os(STATE_DIR_ENV) {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => std::env::temp_dir().join("bque"),
        };
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
        atomic_write(&self.lock_file_path(), &to_json(state)?)
    }

    pub fn load_lock_state(&self) -> Result<Option<LockState>> {
        let path = self.lock_file_path();
        match load_json_file::<LockState>(&path) {
            Some(state) if lock_record_is_live(&state) => Ok(Some(state)),
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
            let current = load_json_file::<LockState>(&path).map(|s| s.pid);
            if current != Some(pid) {
                return Ok(());
            }
        }

        let _ = fs::remove_file(&path);
        Ok(())
    }

    /// Refresh the heartbeat of the lock record, but only while it still belongs
    /// to `pid` — a forced cleanup must not be resurrected by a heartbeat.
    pub fn touch_lock_state(&self, pid: u32) -> Result<()> {
        self.touch::<LockState>(self.lock_file_path(), pid)
    }

    pub fn touch_bench_state(&self, pid: u32) -> Result<()> {
        self.touch::<BenchState>(self.bench_file_path(), pid)
    }

    /// Refresh an observer record. Like the lock, the writable path is the PID
    /// file itself, so a forced cleanup cannot be resurrected by a heartbeat.
    pub fn touch_observer(&self, pid: u32) -> Result<()> {
        self.touch::<LockState>(self.observer_file_path(pid), pid)
    }

    pub fn touch_waiter(&self, pid: u32) -> Result<()> {
        self.touch::<WaiterState>(self.waiter_file_path(pid), pid)
    }

    fn touch<T: HeartbeatRecord>(&self, path: PathBuf, pid: u32) -> Result<()> {
        let Some(mut record) = load_json_file::<T>(&path) else {
            return Ok(());
        };
        if record.record_pid() != pid {
            return Ok(());
        }
        record.set_heartbeat(Utc::now());
        atomic_write(&path, &to_json(&record)?)
    }

    pub fn register_waiter(&self, state: &WaiterState) -> Result<()> {
        atomic_write(&self.waiter_file_path(state.pid), &to_json(state)?)
    }

    pub fn unregister_waiter(&self, pid: u32) -> Result<()> {
        let path = self.waiter_file_path(pid);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn list_waiters(&self) -> Result<Vec<WaiterState>> {
        let (mut list, _) = self.scan_waiters()?;
        list.sort_by_key(|w| w.queued_at);
        Ok(list)
    }

    pub fn register_observer(&self, state: &LockState) -> Result<()> {
        atomic_write(&self.observer_file_path(state.pid), &to_json(state)?)
    }

    pub fn unregister_observer(&self, pid: u32) -> Result<()> {
        let path = self.observer_file_path(pid);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn list_observers(&self) -> Result<Vec<LockState>> {
        let (list, _) = self.scan_observers()?;
        Ok(list)
    }

    pub fn save_bench_state(&self, state: &BenchState) -> Result<()> {
        atomic_write(&self.bench_file_path(), &to_json(state)?)
    }

    pub fn load_bench_state(&self) -> Result<Option<BenchState>> {
        let path = self.bench_file_path();
        match load_json_file::<BenchState>(&path) {
            Some(state) if bench_record_is_live(&state) => Ok(Some(state)),
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
        atomic_write(&self.pause_file_path(), &to_json(state)?)
    }

    pub fn load_pause_state(&self) -> Result<Option<PauseState>> {
        Ok(load_json_file(&self.pause_file_path()))
    }

    pub fn clear_pause_state(&self) -> Result<()> {
        let path = self.pause_file_path();
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn get_full_status(&self) -> Result<FullQueueStatus> {
        let reaped = self.reap()?;
        Ok(FullQueueStatus {
            active_lock: self.load_lock_state()?,
            active_bench: self.load_bench_state()?,
            pause_state: self.load_pause_state()?,
            waiters: self.list_waiters()?,
            observers: self.list_observers()?,
            reaped,
        })
    }

    /// Drop every record whose owner is provably gone, however it is proved.
    ///
    /// This is the single healing pass used by `status`, by the queue wait loop
    /// and by `clean`, so a phantom entry can never survive long enough to make
    /// the queue look jammed.
    pub fn reap(&self) -> Result<ReapReport> {
        let mut report = ReapReport::default();

        let lock_path = self.lock_file_path();
        if lock_path.exists() {
            let stale = match load_json_file::<LockState>(&lock_path) {
                Some(state) => !lock_record_is_live(&state),
                None => true,
            };
            if stale {
                let _ = fs::remove_file(&lock_path);
                report.lock = true;
            }
        }

        let bench_path = self.bench_file_path();
        if bench_path.exists() {
            let stale = match load_json_file::<BenchState>(&bench_path) {
                Some(state) => !bench_record_is_live(&state),
                None => true,
            };
            if stale {
                let _ = fs::remove_file(&bench_path);
                report.bench = true;
            }
        }

        report.waiters = self.scan_waiters()?.1;
        report.observers = self.scan_observers()?.1;
        Ok(report)
    }

    /// Emergency recovery: clear every record that can make the queue look busy,
    /// including owners that still look alive. The kernel mutex stays the source
    /// of truth for exclusion, and an abandoned mutex is recovered by the next
    /// waiter, so this cannot corrupt the lock itself.
    pub fn force_clear(&self) -> Result<ReapReport> {
        let mut report = self.reap()?;

        if self.lock_file_path().exists() {
            let _ = fs::remove_file(self.lock_file_path());
            report.lock = true;
        }
        if self.bench_file_path().exists() {
            let _ = fs::remove_file(self.bench_file_path());
            report.bench = true;
        }
        if self.pause_file_path().exists() {
            let _ = fs::remove_file(self.pause_file_path());
            report.pause = true;
        }

        for pid in self.waiter_pids()? {
            let _ = self.unregister_waiter(pid);
            report.waiters += 1;
        }
        for pid in self.observer_pids()? {
            let _ = self.unregister_observer(pid);
            report.observers += 1;
        }
        Ok(report)
    }

    fn waiter_pids(&self) -> Result<Vec<u32>> {
        self.json_pids(&self.root_dir.join("waiters"))
    }

    fn observer_pids(&self) -> Result<Vec<u32>> {
        self.json_pids(&self.root_dir.join("observers"))
    }

    fn json_pids(&self, dir: &Path) -> Result<Vec<u32>> {
        let mut pids = Vec::new();
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "json")
                    && let Some(pid) = pid_from_file_name(&path)
                {
                    pids.push(pid);
                }
            }
        }
        Ok(pids)
    }

    fn scan_waiters(&self) -> Result<(Vec<WaiterState>, u32)> {
        self.scan_records::<WaiterState>(&self.root_dir.join("waiters"), |state| {
            waiter_record_is_live(state)
        })
    }

    fn scan_observers(&self) -> Result<(Vec<LockState>, u32)> {
        self.scan_records::<LockState>(&self.root_dir.join("observers"), |state| {
            observer_record_is_live(state)
        })
    }

    fn scan_records<T: DeserializeOwned>(
        &self,
        dir: &Path,
        is_live: impl Fn(&T) -> bool,
    ) -> Result<(Vec<T>, u32)> {
        let mut list = Vec::new();
        let mut reaped = 0;
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if !path.extension().is_some_and(|ext| ext == "json") {
                    continue;
                }
                match load_json_file::<T>(&path) {
                    Some(record) if is_live(&record) => list.push(record),
                    _ => {
                        let _ = fs::remove_file(&path);
                        reaped += 1;
                    }
                }
            }
        }
        Ok((list, reaped))
    }
}

fn lock_record_is_live(state: &LockState) -> bool {
    record_is_live(
        state.identity,
        state.pid,
        Some(system_time_from_unix_secs(state.start_time.timestamp())),
        state.heartbeat,
        LOCK_LEASE,
    )
}

fn observer_record_is_live(state: &LockState) -> bool {
    record_is_live(
        state.identity,
        state.pid,
        Some(system_time_from_unix_secs(state.start_time.timestamp())),
        state.heartbeat,
        WAITER_LEASE,
    )
}

fn waiter_record_is_live(state: &WaiterState) -> bool {
    record_is_live(
        state.identity,
        state.pid,
        Some(system_time_from_unix_secs(state.queued_at.timestamp())),
        state.heartbeat,
        WAITER_LEASE,
    )
}

fn bench_record_is_live(state: &BenchState) -> bool {
    record_is_live(
        state.identity,
        state.pid,
        Some(system_time_from_unix_secs(state.start_time.timestamp())),
        state.heartbeat,
        BENCH_LEASE,
    )
}

/// Decide whether a record still belongs to a running owner.
///
/// Identity match wins. When the process cannot be inspected at all, the
/// heartbeat lease decides instead, so a record can still age out without ever
/// touching the process.
fn record_is_live(
    identity: Option<ProcessIdentity>,
    pid: u32,
    recorded_at: Option<SystemTime>,
    heartbeat: Option<DateTime<Utc>>,
    lease: Duration,
) -> bool {
    match classify(identity, pid, recorded_at) {
        Liveness::Alive => true,
        Liveness::Dead | Liveness::Reused => false,
        Liveness::Inaccessible => match heartbeat {
            Some(at) => {
                let age = Utc::now().signed_duration_since(at);
                age.to_std().map(|age| age <= lease).unwrap_or(true)
            }
            // Legacy record with no heartbeat: nothing to age it by, keep it.
            None => true,
        },
    }
}

pub fn is_pid_alive(pid: u32) -> bool {
    !matches!(classify(None, pid, None), Liveness::Dead)
}

fn pid_from_file_name(path: &Path) -> Option<u32> {
    let stem = path.file_stem()?.to_str()?;
    stem.rsplit('_').next()?.parse::<u32>().ok()
}

fn to_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dead_pid() -> u32 {
        // A PID that cannot be in use: process IDs are 4-byte multiples of 4 on
        // Windows, and this value is far beyond any real allocation.
        u32::MAX - 3
    }

    #[test]
    fn epoch_helpers_round_trip() {
        let time = system_time_from_unix_secs(1_700_000_000);
        let millis = crate::state::process::epoch_millis(time);
        assert_eq!(millis / 1000, 1_700_000_000);
    }

    #[test]
    fn identity_mismatch_is_reported_as_reused() {
        let me = std::process::id();
        let Some(actual) = ProcessIdentity::capture(me) else {
            return; // unavailable without process query rights
        };
        let forged = ProcessIdentity {
            pid: me,
            created_at_ms: actual.created_at_ms + 60_000,
        };
        assert_eq!(forged.classify(), Liveness::Reused);
        assert_eq!(actual.classify(), Liveness::Alive);
    }

    #[test]
    fn dead_waiter_record_is_not_live() {
        let waiter = WaiterState {
            pid: dead_pid(),
            queued_at: Utc::now(),
            command: "cargo build".into(),
            description: "cargo build".into(),
            cwd: ".".into(),
            identity: None,
            heartbeat: None,
        };
        assert!(!waiter_record_is_live(&waiter));
    }
}
