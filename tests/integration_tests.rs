use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Isolates each test in its own queue: a private state directory and a private
/// kernel mutex namespace, so a test can hold, block and jam a queue without
/// touching the real machine-wide one.
struct Harness {
    dir: PathBuf,
    namespace: String,
}

impl Harness {
    fn new(tag: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("bque_it_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("waiters")).expect("create waiters dir");
        std::fs::create_dir_all(dir.join("observers")).expect("create observers dir");

        Self {
            dir,
            namespace: format!("BqueTest_{}_{}_{}", tag, std::process::id(), stamp),
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bque"));
        cmd.env("BQUE_STATE_DIR", &self.dir)
            .env("BQUE_MUTEX_NAMESPACE", &self.namespace)
            .env_remove("BQUE_ACTIVE_PID");
        cmd
    }

    fn state_path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    fn write_state(&self, relative: &str, json: &str) -> PathBuf {
        let path = self.state_path(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create state parent");
        }
        std::fs::write(&path, json).expect("write state record");
        path
    }

    fn status_json(&self) -> serde_json::Value {
        let output = self
            .cmd()
            .args(["status", "--json"])
            .output()
            .expect("run bque status --json");
        assert!(
            output.status.success(),
            "status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("parse status json")
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A PID that cannot belong to a running process, used to fake a dead owner.
const DEAD_PID: u32 = u32::MAX - 4;

fn lock_record(pid: u32, identity: Option<(u32, u64)>, start_time: &str) -> String {
    let identity = match identity {
        Some((pid, created_at_ms)) => {
            format!(
                r#""identity": {{ "pid": {}, "created_at_ms": {} }},"#,
                pid, created_at_ms
            )
        }
        None => String::new(),
    };
    format!(
        r#"{{
  "pid": {pid},
  "start_time": "{start_time}",
  "command": "cargo build --release",
  "description": "cargo build --release",
  "cwd": "E:\\work\\project",
  "priority": "Idle",
  "cpu_rate": 100,
  "is_observer": false,
  {identity}
  "heartbeat": null
}}"#
    )
}

fn bench_record(pid: u32, identity: Option<(u32, u64)>, start_time: &str) -> String {
    let identity = match identity {
        Some((pid, created_at_ms)) => {
            format!(
                r#""identity": {{ "pid": {}, "created_at_ms": {} }},"#,
                pid, created_at_ms
            )
        }
        None => String::new(),
    };
    format!(
        r#"{{
  "pid": {pid},
  "start_time": "{start_time}",
  "command": "cargo bench",
  "description": "cargo bench",
  "frozen_pids": [],
  {identity}
  "heartbeat": null
}}"#
    )
}

fn waiter_record(pid: u32, queued_at: &str) -> String {
    format!(
        r#"{{
  "pid": {pid},
  "queued_at": "{queued_at}",
  "command": "cargo clippy --workspace --all-targets -- -D warnings",
  "description": "cargo clippy --workspace --all-targets -- -D warnings",
  "cwd": "E:\\work\\project"
}}"#
    )
}

const LONG_AGO: &str = "2020-01-01T00:00:00Z";

#[test]
fn test_cli_help() {
    let harness = Harness::new("help");
    let output = harness
        .cmd()
        .arg("--help")
        .output()
        .expect("Failed to run bque --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bque serializes builds"));
    assert!(stdout.contains("bench"));
}

#[test]
fn test_exit_code_propagation() {
    let harness = Harness::new("exit_code");
    let output = harness
        .cmd()
        .args([
            "run",
            "--desc",
            "Exit 42 Test",
            "--",
            "cmd.exe",
            "/c",
            "exit 42",
        ])
        .output()
        .expect("Failed to execute bque run");
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn test_status_json_output() {
    let harness = Harness::new("status_json");
    let val = harness.status_json();
    assert!(val.get("waiters").is_some());
    assert!(val.get("observers").is_some());
    assert!(val["waiters"].as_array().unwrap().is_empty());
}

#[test]
fn test_pause_and_resume_cycle() {
    let harness = Harness::new("pause_cycle");
    let p_out = harness
        .cmd()
        .args(["pause", "--reason", "Integration Test"])
        .output()
        .expect("Failed to pause");
    assert!(p_out.status.success());
    assert!(harness.status_json()["pause_state"].is_object());

    let r_out = harness
        .cmd()
        .arg("resume")
        .output()
        .expect("Failed to resume");
    assert!(r_out.status.success());
    assert!(harness.status_json()["pause_state"].is_null());
}

#[test]
fn test_benchmark_clean_execution() {
    let harness = Harness::new("bench_run");
    let output = harness
        .cmd()
        .args([
            "bench",
            "--desc",
            "Bench Test",
            "--freeze-builds=false",
            "--",
            "cmd.exe",
            "/c",
            "echo BENCH_OK",
        ])
        .output()
        .expect("Failed to run bench");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BENCH_OK"));
    assert!(stdout.contains("Benchmark finished with exit code 0"));
}

#[test]
fn test_reentrancy_bypass() {
    let harness = Harness::new("reentrancy");
    let current_pid = std::process::id().to_string();

    let output = harness
        .cmd()
        .env("BQUE_ACTIVE_PID", current_pid)
        .args(["run", "--", "cmd.exe", "/c", "echo REENTRANT_BYPASS_OK"])
        .output()
        .expect("Failed to run reentrant bque");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REENTRANT_BYPASS_OK"));
}

#[test]
#[cfg(windows)]
fn test_pathext_cmd_resolution() {
    let harness = Harness::new("pathext");
    let temp_dir = harness.state_path("bin");
    let _ = std::fs::create_dir_all(&temp_dir);
    let cmd_file = temp_dir.join("bquetestcmd.cmd");
    std::fs::write(&cmd_file, "@echo PATHEXT_RESOLUTION_OK\r\n").expect("write cmd file");

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{};{}", temp_dir.display(), current_path);

    let output = harness
        .cmd()
        .env("PATH", new_path)
        .args(["run", "--", "bquetestcmd"])
        .output()
        .expect("Failed to run bquetestcmd");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PATHEXT_RESOLUTION_OK"));
}

#[test]
fn test_status_dashboard_rendering() {
    let harness = Harness::new("dashboard");
    let output = harness
        .cmd()
        .arg("status")
        .output()
        .expect("Failed to execute bque status");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Global Build Queue & Governor"));
    assert!(
        stdout.contains("WAITERS")
            || stdout.contains("BUILD QUEUE")
            || stdout.contains("BUILDS MANUALLY PAUSED")
    );
}

#[test]
fn test_blocked_by_pause_notification() {
    let harness = Harness::new("pause_notify");
    let p_out = harness
        .cmd()
        .args(["pause", "--reason", "Wait Notification Test"])
        .output()
        .expect("pause failed");
    assert!(p_out.status.success());

    let run_out = harness
        .cmd()
        .args([
            "run",
            "--timeout",
            "1",
            "--",
            "cmd.exe",
            "/c",
            "echo should_not_run",
        ])
        .output()
        .expect("run failed");

    let r_out = harness.cmd().arg("resume").output().expect("resume failed");
    assert!(r_out.status.success());

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&run_out.stdout),
        String::from_utf8_lossy(&run_out.stderr)
    );
    assert!(
        combined.contains("Blocked by Pause 'Wait Notification Test'")
            || combined.contains("Timed out waiting")
    );
}

#[test]
fn test_update_check_command() {
    let harness = Harness::new("update_check");
    let output = harness
        .cmd()
        .args(["update", "--check"])
        .output()
        .expect("Failed to execute bque update --check");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Checking for updates")
            || combined.contains("up to date")
            || combined.contains("version is available")
            || combined.contains("Failed to check")
    );
}

// ---------------------------------------------------------------------------
// Stale-queue recovery: a record whose owner is gone, or whose PID was handed to
// an unrelated process, must never make the queue look busy.
// ---------------------------------------------------------------------------

#[test]
fn test_dead_waiter_is_reaped_and_disappears_from_status() {
    let harness = Harness::new("stale_waiter");
    let path = harness.write_state(
        &format!("waiters/waiter_{}.json", DEAD_PID),
        &waiter_record(DEAD_PID, LONG_AGO),
    );

    let val = harness.status_json();
    assert!(
        val["waiters"].as_array().unwrap().is_empty(),
        "a waiter whose process is gone must not be reported"
    );
    assert_eq!(val["reaped"]["waiters"], 1);
    assert!(!path.exists(), "the stale waiter record must be deleted");
}

#[test]
fn test_recycled_pid_does_not_look_like_an_active_lock() {
    let harness = Harness::new("recycled_lock");
    // A live PID (ours) that predates the record: the original owner is gone and
    // the number was recycled, which only the creation time can reveal.
    let me = std::process::id();
    let path = harness.write_state("lock_state.json", &lock_record(me, Some((me, 1)), LONG_AGO));

    let val = harness.status_json();
    assert!(
        val["active_lock"].is_null(),
        "a recycled PID must not be reported as the lock holder"
    );
    assert_eq!(val["reaped"]["lock"], true);
    assert!(!path.exists(), "the recycled lock record must be deleted");
}

#[test]
fn test_recycled_pid_benchmark_does_not_block_builds() {
    let harness = Harness::new("recycled_bench");
    let me = std::process::id();
    harness.write_state(
        "bench_state.json",
        &bench_record(me, Some((me, 1)), LONG_AGO),
    );

    let started = SystemTime::now();
    let output = harness
        .cmd()
        .args([
            "run",
            "--timeout",
            "20",
            "--",
            "cmd.exe",
            "/c",
            "echo BUILD_RAN",
        ])
        .output()
        .expect("run failed");
    let elapsed = started.elapsed().unwrap_or_default();

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("BUILD_RAN"),
        "the build must run instead of queueing behind a phantom benchmark; got: {combined}"
    );
    assert!(
        elapsed.as_secs() < 15,
        "the build should not wait for the phantom benchmark (waited {elapsed:?})"
    );
    assert!(!harness.state_path("bench_state.json").exists());
}

#[test]
fn test_dead_lock_holder_does_not_block_new_builds() {
    let harness = Harness::new("dead_lock");
    harness.write_state("lock_state.json", &lock_record(DEAD_PID, None, LONG_AGO));
    harness.write_state(
        &format!("waiters/waiter_{}.json", DEAD_PID),
        &waiter_record(DEAD_PID, LONG_AGO),
    );

    let val = harness.status_json();
    assert!(val["active_lock"].is_null());
    assert!(val["waiters"].as_array().unwrap().is_empty());

    let output = harness
        .cmd()
        .args([
            "run",
            "--timeout",
            "20",
            "--",
            "cmd.exe",
            "/c",
            "echo UNBLOCKED",
        ])
        .output()
        .expect("run failed");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("UNBLOCKED"), "got: {combined}");
}

#[test]
fn test_legacy_records_without_identity_still_work() {
    let harness = Harness::new("legacy_schema");
    // Records written by older versions carry no identity and no heartbeat.
    let me = std::process::id();
    let now = chrono::Utc::now().to_rfc3339();
    harness.write_state("lock_state.json", &lock_record(me, None, &now));
    harness.write_state(
        &format!("waiters/waiter_{}.json", me),
        &waiter_record(me, &now),
    );

    let val = harness.status_json();
    assert_eq!(val["active_lock"]["pid"], me);
    assert_eq!(val["waiters"].as_array().unwrap().len(), 1);
}

#[test]
fn test_clean_force_unblocks_everything() {
    let harness = Harness::new("clean_force");
    let me = std::process::id();
    harness.write_state(
        "bench_state.json",
        &bench_record(me, Some((me, 1)), LONG_AGO),
    );
    harness.write_state(
        "pause_state.json",
        r#"{
  "paused_at": "2020-01-01T00:00:00Z",
  "reason": "forgotten pause",
  "paused_by_pid": 4242
}"#,
    );

    let output = harness
        .cmd()
        .args(["clean", "--force"])
        .output()
        .expect("clean --force failed");
    assert!(output.status.success());

    let val = harness.status_json();
    assert!(val["pause_state"].is_null());
    assert!(val["active_bench"].is_null());
}

#[test]
fn test_clean_reports_reaped_records() {
    let harness = Harness::new("clean_report");
    harness.write_state(
        &format!("waiters/waiter_{}.json", DEAD_PID),
        &waiter_record(DEAD_PID, LONG_AGO),
    );

    let output = harness.cmd().arg("clean").output().expect("clean failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 waiter(s)"), "got: {stdout}");
}
