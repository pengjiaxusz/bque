use crate::job::ProcessFreezer;
use crate::state::process::{Liveness, ProcessIdentity, classify, system_time_from_unix_secs};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Total shared host budget units across all active managed tasks.
pub const BUDGET_TOTAL: u32 = 100;
/// Maximum seconds an earlier waiter may be bypassed by smaller backfill tasks.
pub const BACKFILL_STARVATION_SECS: u64 = 60;

/// How often a lock holder refreshes its heartbeat.
pub const LOCK_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// A lock/task record whose owner cannot be inspected is dropped after this lease.
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

fn default_weight() -> u32 {
    BUDGET_TOTAL
}

/// Live activity state of a managed task.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ActivityState {
    /// Actively consuming its claimed budget.
    #[default]
    Active,
    /// Sustained low CPU; temporarily lending its budget to queued waiters.
    Yielding,
}

impl std::fmt::Display for ActivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivityState::Active => write!(f, "active"),
            ActivityState::Yielding => write!(f, "yielding"),
        }
    }
}

/// Per-task state stored in `tasks/task_<pid>.json`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TaskState {
    pub pid: u32,
    pub start_time: DateTime<Utc>,
    pub command: String,
    pub description: String,
    pub cwd: String,
    pub priority: String,
    pub cpu_rate: u32,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default)]
    pub activity_state: ActivityState,
    #[serde(default)]
    pub cpu_pct: f64,
    #[serde(default)]
    pub low_cpu_secs: u64,
    #[serde(default)]
    pub identity: Option<ProcessIdentity>,
    #[serde(default)]
    pub heartbeat: Option<DateTime<Utc>>,
}

impl TaskState {
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        pid: u32,
        command: String,
        description: String,
        cwd: String,
        priority: String,
        cpu_rate: u32,
        weight: u32,
        scope: Option<String>,
    ) -> Self {
        Self {
            pid,
            start_time: Utc::now(),
            command,
            description,
            cwd,
            priority,
            cpu_rate,
            weight: weight.clamp(1, BUDGET_TOTAL),
            scope,
            activity_state: ActivityState::Active,
            cpu_pct: 0.0,
            low_cpu_secs: 0,
            identity: ProcessIdentity::capture(pid),
            heartbeat: Some(Utc::now()),
        }
    }

    /// Project into the legacy `LockState` shape for backward-compatible JSON.
    pub fn to_legacy_lock_state(&self) -> LockState {
        LockState {
            pid: self.pid,
            start_time: self.start_time,
            command: self.command.clone(),
            description: self.description.clone(),
            cwd: self.cwd.clone(),
            priority: self.priority.clone(),
            cpu_rate: self.cpu_rate,
            is_observer: false,
            identity: self.identity,
            heartbeat: self.heartbeat,
        }
    }
}

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

impl LockState {
    pub fn to_task_state(&self) -> TaskState {
        TaskState {
            pid: self.pid,
            start_time: self.start_time,
            command: self.command.clone(),
            description: self.description.clone(),
            cwd: self.cwd.clone(),
            priority: self.priority.clone(),
            cpu_rate: self.cpu_rate,
            weight: BUDGET_TOTAL,
            scope: None,
            activity_state: ActivityState::Active,
            cpu_pct: 0.0,
            low_cpu_secs: 0,
            identity: self.identity,
            heartbeat: self.heartbeat,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WaiterState {
    pub pid: u32,
    pub queued_at: DateTime<Utc>,
    pub command: String,
    pub description: String,
    pub cwd: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
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
    pub frozen_identities: Vec<ProcessIdentity>,
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
    #[serde(default)]
    pub frozen_identities: Vec<ProcessIdentity>,
    #[serde(default)]
    pub keep_observers: bool,
}

/// What a cleanup pass removed, so callers can report it instead of guessing.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ReapReport {
    #[serde(default)]
    pub lock: bool,
    #[serde(default)]
    pub tasks: u32,
    #[serde(default)]
    pub bench: bool,
    #[serde(default)]
    pub waiters: u32,
    #[serde(default)]
    pub observers: u32,
    #[serde(default)]
    pub pause: bool,
    #[serde(default)]
    pub unfrozen: u32,
}

impl ReapReport {
    pub fn total(&self) -> u32 {
        u32::from(self.lock)
            + self.tasks
            + u32::from(self.bench)
            + self.waiters
            + self.observers
            + u32::from(self.pause)
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0 && self.unfrozen == 0
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.lock {
            parts.push("active lock".to_string());
        }
        if self.tasks > 0 {
            parts.push(format!("{} task(s)", self.tasks));
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
        if self.unfrozen > 0 {
            parts.push(format!("unfroze {} orphan process(es)", self.unfrozen));
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
    /// Legacy single-lock field populated from the first active task for
    /// backward compatibility with existing JSON consumers.
    pub active_lock: Option<LockState>,
    #[serde(default)]
    pub active_tasks: Vec<TaskState>,
    #[serde(default)]
    pub budget_used: u32,
    #[serde(default = "default_weight")]
    pub budget_total: u32,
    #[serde(default)]
    pub oversubscribed: bool,
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
    pub fn record(
        pid: u32,
        command: String,
        description: String,
        cwd: String,
        weight: u32,
        scope: Option<String>,
    ) -> Self {
        Self {
            pid,
            queued_at: Utc::now(),
            command,
            description,
            cwd,
            weight: weight.clamp(1, BUDGET_TOTAL),
            scope,
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
        frozen_identities: Vec<ProcessIdentity>,
    ) -> Self {
        let frozen_pids = frozen_identities.iter().map(|id| id.pid).collect();
        Self {
            pid,
            start_time: Utc::now(),
            command,
            description,
            cwd,
            frozen_pids,
            frozen_identities,
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

impl HeartbeatRecord for TaskState {
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

/// Resolve the state directory used for queue records and lock files.
///
/// Precedence:
/// 1. `BQUE_STATE_DIR` environment override.
/// 2. On Unix, `XDG_RUNTIME_DIR/bque` if set, otherwise `<temp_dir>/bque-<uid>`.
/// 3. On Windows, `<temp_dir>/bque`.
pub fn resolve_state_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(STATE_DIR_ENV)
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }

    #[cfg(unix)]
    {
        if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR")
            && !runtime_dir.is_empty()
        {
            return PathBuf::from(runtime_dir).join("bque");
        }
        let uid = unsafe { libc::geteuid() };
        std::env::temp_dir().join(format!("bque-{}", uid))
    }

    #[cfg(not(unix))]
    {
        std::env::temp_dir().join("bque")
    }
}

impl StateStorage {
    pub fn new() -> Result<Self> {
        let root = resolve_state_dir();
        fs::create_dir_all(&root).context("Failed to create bque state directory")?;
        fs::create_dir_all(root.join("tasks")).context("Failed to create tasks directory")?;
        fs::create_dir_all(root.join("waiters")).context("Failed to create waiters directory")?;
        fs::create_dir_all(root.join("observers"))
            .context("Failed to create observers directory")?;
        Ok(Self { root_dir: root })
    }

    pub fn lock_file_path(&self) -> PathBuf {
        self.root_dir.join("lock_state.json")
    }

    pub fn task_file_path(&self, pid: u32) -> PathBuf {
        self.root_dir
            .join("tasks")
            .join(format!("task_{}.json", pid))
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

    pub fn register_task(&self, state: &TaskState) -> Result<()> {
        atomic_write(&self.task_file_path(state.pid), &to_json(state)?)
    }

    pub fn unregister_task(&self, pid: u32) -> Result<()> {
        let path = self.task_file_path(pid);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    /// Update a live task's activity metrics, yielding state, optional CPU rate,
    /// and heartbeat in one atomic write.
    pub fn update_task_activity(
        &self,
        pid: u32,
        cpu_pct: f64,
        low_cpu_secs: u64,
        activity_state: ActivityState,
        cpu_rate: Option<u32>,
    ) -> Result<()> {
        let path = self.task_file_path(pid);
        let Some(mut record) = load_json_file::<TaskState>(&path) else {
            return Ok(());
        };
        if record.pid != pid {
            return Ok(());
        }
        record.cpu_pct = cpu_pct;
        record.low_cpu_secs = low_cpu_secs;
        record.activity_state = activity_state;
        if let Some(rate) = cpu_rate {
            record.cpu_rate = rate;
        }
        record.heartbeat = Some(Utc::now());
        atomic_write(&path, &to_json(&record)?)
    }

    pub fn update_task_cpu_rate(&self, pid: u32, cpu_rate: u32) -> Result<()> {
        let path = self.task_file_path(pid);
        let Some(mut record) = load_json_file::<TaskState>(&path) else {
            return Ok(());
        };
        if record.pid != pid {
            return Ok(());
        }
        record.cpu_rate = cpu_rate;
        atomic_write(&path, &to_json(&record)?)
    }

    pub fn list_active_tasks(&self) -> Result<Vec<TaskState>> {
        let (mut tasks, _) = self.scan_tasks()?;
        if let Some(legacy) = self.load_lock_state()?
            && !tasks.iter().any(|t| t.pid == legacy.pid)
        {
            tasks.push(legacy.to_task_state());
        }
        tasks.sort_by_key(|t| t.start_time);
        Ok(tasks)
    }

    /// Sum of `weight` across tasks currently in `Active` state (Yielding tasks
    /// lend their budget and contribute 0).
    pub fn effective_budget_used(tasks: &[TaskState]) -> u32 {
        tasks
            .iter()
            .filter(|t| t.activity_state == ActivityState::Active)
            .map(|t| t.weight)
            .sum()
    }

    /// Total host CPU percentage currently consumed by all active managed tasks.
    pub fn total_active_cpu_pct(&self) -> f64 {
        let tasks = self.list_active_tasks().unwrap_or_default();
        tasks
            .iter()
            .map(|t| t.cpu_pct)
            .sum::<f64>()
            .clamp(0.0, 100.0)
    }

    /// Evaluate whether waiter `my_pid` with `(weight, scope)` can be admitted
    /// right now under Budget, Scope, and Bounded Backfill rules.
    pub fn can_admit(&self, my_pid: u32, weight: u32, scope: Option<&str>) -> Result<bool> {
        if self.load_bench_state()?.is_some() || self.load_pause_state()?.is_some() {
            return Ok(false);
        }
        let active_tasks = self.list_active_tasks()?;
        let waiters = self.list_waiters()?;
        Ok(evaluate_admission(
            &active_tasks,
            &waiters,
            my_pid,
            weight,
            scope,
            Utc::now(),
        ))
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

    #[allow(dead_code)]
    pub fn touch_lock_state(&self, pid: u32) -> Result<()> {
        self.touch::<LockState>(self.lock_file_path(), pid)
    }

    pub fn touch_task(&self, pid: u32) -> Result<()> {
        self.touch::<TaskState>(self.task_file_path(pid), pid)
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
            Some(stale) => {
                unfreeze_orphaned_bench(&stale);
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
        let active_tasks = self.list_active_tasks()?;
        let budget_used = Self::effective_budget_used(&active_tasks);
        let active_lock = active_tasks.first().map(TaskState::to_legacy_lock_state);
        Ok(FullQueueStatus {
            active_lock,
            active_tasks,
            budget_used,
            budget_total: BUDGET_TOTAL,
            oversubscribed: budget_used > BUDGET_TOTAL,
            active_bench: self.load_bench_state()?,
            pause_state: self.load_pause_state()?,
            waiters: self.list_waiters()?,
            observers: self.list_observers()?,
            reaped,
        })
    }

    /// Drop every record whose owner is provably gone, however it is proved.
    /// Also unfreezes any processes left suspended by a crashed benchmark.
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
            match load_json_file::<BenchState>(&bench_path) {
                Some(state) if bench_record_is_live(&state) => {}
                Some(stale) => {
                    report.unfrozen += unfreeze_orphaned_bench(&stale);
                    let _ = fs::remove_file(&bench_path);
                    report.bench = true;
                }
                None => {
                    let _ = fs::remove_file(&bench_path);
                    report.bench = true;
                }
            }
        }

        report.tasks = self.scan_tasks()?.1;
        report.waiters = self.scan_waiters()?.1;
        report.observers = self.scan_observers()?.1;
        Ok(report)
    }

    /// Emergency recovery: clear every record that can make the queue look busy,
    /// including owners that still look alive, and resume any frozen processes.
    pub fn force_clear(&self) -> Result<ReapReport> {
        let mut report = self.reap()?;

        if self.lock_file_path().exists() {
            let _ = fs::remove_file(self.lock_file_path());
            report.lock = true;
        }
        if self.bench_file_path().exists() {
            if let Some(bench) = load_json_file::<BenchState>(&self.bench_file_path()) {
                report.unfrozen += unfreeze_orphaned_bench(&bench);
            }
            let _ = fs::remove_file(self.bench_file_path());
            report.bench = true;
        }
        if self.pause_file_path().exists() {
            if let Some(pause) = load_json_file::<PauseState>(&self.pause_file_path())
                && !pause.frozen_identities.is_empty()
            {
                let freezer = ProcessFreezer::new();
                report.unfrozen += freezer
                    .resume_identities(&pause.frozen_identities)
                    .unwrap_or(0);
            }
            let _ = fs::remove_file(self.pause_file_path());
            report.pause = true;
        }

        for pid in self.task_pids()? {
            let _ = self.unregister_task(pid);
            report.tasks += 1;
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

    fn task_pids(&self) -> Result<Vec<u32>> {
        self.json_pids(&self.root_dir.join("tasks"))
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

    fn scan_tasks(&self) -> Result<(Vec<TaskState>, u32)> {
        self.scan_records::<TaskState>(&self.root_dir.join("tasks"), task_record_is_live)
    }

    fn scan_waiters(&self) -> Result<(Vec<WaiterState>, u32)> {
        self.scan_records::<WaiterState>(&self.root_dir.join("waiters"), waiter_record_is_live)
    }

    fn scan_observers(&self) -> Result<(Vec<LockState>, u32)> {
        self.scan_records::<LockState>(&self.root_dir.join("observers"), observer_record_is_live)
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

/// Pure admission logic used by `StateStorage::can_admit` and unit tests.
pub fn evaluate_admission(
    active_tasks: &[TaskState],
    waiters: &[WaiterState],
    my_pid: u32,
    weight: u32,
    scope: Option<&str>,
    now: DateTime<Utc>,
) -> bool {
    // 1. Scope mutual exclusion holds regardless of Budget or Yielding state.
    if let Some(req_scope) = scope
        && active_tasks
            .iter()
            .any(|t| t.scope.as_deref() == Some(req_scope))
    {
        return false;
    }

    // 2. Budget check: Yielding tasks lend their budget (contribute 0).
    let effective_used = StateStorage::effective_budget_used(active_tasks);
    if effective_used.saturating_add(weight) > BUDGET_TOTAL {
        return false;
    }

    // 3. FIFO + bounded backfill: check waiters ahead of us in queue order.
    let mut sorted: Vec<&WaiterState> = waiters.iter().collect();
    sorted.sort_by_key(|w| w.queued_at);

    for earlier in sorted {
        if earlier.pid == my_pid {
            break;
        }
        // Same-scope earlier waiter always takes strict FIFO precedence.
        if scope.is_some() && earlier.scope.as_deref() == scope {
            return false;
        }
        // If an earlier waiter has waited past the starvation cap, block backfill.
        let waited_secs = (now.timestamp() - earlier.queued_at.timestamp()).max(0) as u64;
        if waited_secs >= BACKFILL_STARVATION_SECS {
            return false;
        }
        // If the earlier waiter could also fit right now, let it go first.
        let earlier_scope_busy = earlier
            .scope
            .as_deref()
            .is_some_and(|es| active_tasks.iter().any(|t| t.scope.as_deref() == Some(es)));
        if !earlier_scope_busy && effective_used.saturating_add(earlier.weight) <= BUDGET_TOTAL {
            return false;
        }
    }

    true
}

fn unfreeze_orphaned_bench(stale: &BenchState) -> u32 {
    if stale.frozen_identities.is_empty() {
        return 0;
    }
    let freezer = ProcessFreezer::new();
    freezer
        .resume_identities(&stale.frozen_identities)
        .unwrap_or(0)
}

fn task_record_is_live(state: &TaskState) -> bool {
    record_is_live(
        state.identity,
        state.pid,
        Some(system_time_from_unix_secs(state.start_time.timestamp())),
        state.heartbeat,
        LOCK_LEASE,
    )
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
        u32::MAX - 3
    }

    fn sample_task(pid: u32, weight: u32, scope: Option<&str>, state: ActivityState) -> TaskState {
        TaskState {
            pid,
            start_time: Utc::now(),
            command: "cargo build".into(),
            description: "cargo build".into(),
            cwd: ".".into(),
            priority: "Idle".into(),
            cpu_rate: 100,
            weight,
            scope: scope.map(String::from),
            activity_state: state,
            cpu_pct: 0.0,
            low_cpu_secs: 0,
            identity: None,
            heartbeat: Some(Utc::now()),
        }
    }

    fn sample_waiter(
        pid: u32,
        weight: u32,
        scope: Option<&str>,
        queued_at: DateTime<Utc>,
    ) -> WaiterState {
        WaiterState {
            pid,
            queued_at,
            command: "cargo test".into(),
            description: "cargo test".into(),
            cwd: ".".into(),
            weight,
            scope: scope.map(String::from),
            identity: None,
            heartbeat: Some(queued_at),
        }
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
            return;
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
            weight: 100,
            scope: None,
            identity: None,
            heartbeat: None,
        };
        assert!(!waiter_record_is_live(&waiter));
    }

    #[test]
    fn yielding_task_lends_budget_to_heavy_waiter() {
        let now = Utc::now();
        let active = vec![sample_task(10, 100, None, ActivityState::Yielding)];
        let waiters = vec![sample_waiter(20, 100, None, now)];
        assert!(evaluate_admission(&active, &waiters, 20, 100, None, now));
    }

    #[test]
    fn scope_blocks_even_when_holder_is_yielding() {
        let now = Utc::now();
        let active = vec![sample_task(
            10,
            100,
            Some("repo-a"),
            ActivityState::Yielding,
        )];
        let waiters = vec![sample_waiter(20, 25, Some("repo-a"), now)];
        assert!(!evaluate_admission(
            &active,
            &waiters,
            20,
            25,
            Some("repo-a"),
            now
        ));
    }

    #[test]
    fn bounded_backfill_allows_light_waiter_until_starvation_cap() {
        let now = Utc::now();
        let active = vec![sample_task(10, 50, None, ActivityState::Active)];

        // Head-of-line heavy waiter waited 10s (< 60s): light waiter may backfill.
        let waiters_fresh = vec![
            sample_waiter(20, 100, None, now - chrono::Duration::seconds(10)),
            sample_waiter(30, 25, None, now - chrono::Duration::seconds(2)),
        ];
        assert!(evaluate_admission(
            &active,
            &waiters_fresh,
            30,
            25,
            None,
            now
        ));

        // Head-of-line heavy waiter waited 65s (>= 60s): backfill is blocked.
        let waiters_starving = vec![
            sample_waiter(20, 100, None, now - chrono::Duration::seconds(65)),
            sample_waiter(30, 25, None, now - chrono::Duration::seconds(2)),
        ];
        assert!(!evaluate_admission(
            &active,
            &waiters_starving,
            30,
            25,
            None,
            now
        ));
    }
}
