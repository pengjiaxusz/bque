use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Return platform-appropriate shell invocation arguments for running `script`.
#[cfg(windows)]
fn shell_args(script: &str) -> [&str; 3] {
    ["cmd.exe", "/c", script]
}

#[cfg(not(windows))]
fn shell_args(script: &str) -> [&str; 3] {
    ["sh", "-c", script]
}

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
  "cwd": "E:\\work\\project",
  "frozen_pids": [],
  {identity}
  "heartbeat": null
}}"#
    )
}

fn observer_record(pid: u32, start_time: &str) -> String {
    format!(
        r#"{{
  "pid": {pid},
  "start_time": "{start_time}",
  "command": "cargo run -q -p xtask -- dev",
  "description": "xtask dev server",
  "cwd": "E:\\work\\project\\worktrees\\a",
  "priority": "Idle",
  "cpu_rate": 30,
  "is_observer": true,
  "heartbeat": "{start_time}"
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
    let sh = shell_args("exit 42");
    let output = harness
        .cmd()
        .args(["run", "--desc", "Exit 42 Test", "--"])
        .args(sh)
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
    let sh = shell_args("echo BENCH_OK");
    let output = harness
        .cmd()
        .args([
            "bench",
            "--desc",
            "Bench Test",
            "--freeze-builds=false",
            "--",
        ])
        .args(sh)
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
    let sh = shell_args("echo REENTRANT_BYPASS_OK");

    let output = harness
        .cmd()
        .env("BQUE_ACTIVE_PID", current_pid)
        .args(["run", "--"])
        .args(sh)
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
fn test_status_names_each_task_working_directory() {
    let harness = Harness::new("cwd_detail");
    let me = std::process::id();
    let now = chrono::Utc::now().to_rfc3339();
    harness.write_state("lock_state.json", &lock_record(me, None, &now));
    harness.write_state(
        &format!("waiters/waiter_{}.json", me),
        &waiter_record(me, &now),
    );
    harness.write_state(
        &format!("observers/observer_{}.json", me),
        &observer_record(me, &now),
    );

    let output = harness
        .cmd()
        .arg("status")
        .output()
        .expect("Failed to execute bque status");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("dir  E:\\work\\project"),
        "the lock and waiter must report their working directory; got:\n{stdout}"
    );
    assert!(
        stdout.contains("dir  E:\\work\\project\\worktrees\\a"),
        "the observer must report its worktree; got:\n{stdout}"
    );
    assert!(
        stdout.contains("cmd  cargo run -q -p xtask -- dev"),
        "a task whose command differs from the headline must show it; got:\n{stdout}"
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

    let sh = shell_args("echo should_not_run");
    let run_out = harness
        .cmd()
        .args(["run", "--timeout", "1", "--"])
        .args(sh)
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

#[test]
#[cfg(unix)]
fn test_concurrent_runs_are_serialized() {
    let harness = Harness::new("serialize");
    let log_path = harness.state_path("order.log");
    let script1 = format!(
        "echo first_start >> '{0}'; sleep 0.6; echo first_end >> '{0}'",
        log_path.display()
    );
    let script2 = format!("echo second_start >> '{}'", log_path.display());

    let mut first = harness
        .cmd()
        .args(["run", "--no-governor", "--", "sh", "-c", &script1])
        .spawn()
        .expect("spawn first bque run");

    // Wait briefly so the first run acquires the flock before the second starts.
    std::thread::sleep(std::time::Duration::from_millis(150));

    let second = harness
        .cmd()
        .args([
            "run",
            "--no-governor",
            "--timeout",
            "10",
            "--",
            "sh",
            "-c",
            &script2,
        ])
        .output()
        .expect("run second bque run");

    let first_status = first.wait().expect("wait for first bque run");
    assert!(first_status.success(), "first run failed");
    assert!(second.status.success(), "second run failed");

    let log = std::fs::read_to_string(&log_path).expect("read order log");
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(
        lines,
        vec!["first_start", "first_end", "second_start"],
        "second run must wait until first run releases the lock"
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
    let sh = shell_args("echo BUILD_RAN");
    let output = harness
        .cmd()
        .args(["run", "--timeout", "20", "--"])
        .args(sh)
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

    let sh = shell_args("echo UNBLOCKED");
    let output = harness
        .cmd()
        .args(["run", "--timeout", "20", "--"])
        .args(sh)
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

// ---------------------------------------------------------------------------
// Host resource probing & preflight resource guard
// ---------------------------------------------------------------------------

#[test]
fn test_probe_human_and_json_output() {
    let harness = Harness::new("probe_out");

    let human = harness.cmd().arg("probe").output().expect("run bque probe");
    assert!(
        human.status.success(),
        "bque probe failed: {}",
        String::from_utf8_lossy(&human.stderr)
    );
    let stdout = String::from_utf8_lossy(&human.stdout);
    assert!(stdout.contains("Host Resource Profile"), "got: {stdout}");
    assert!(stdout.contains("Tier:"), "got: {stdout}");
    assert!(stdout.contains("Logical Cores:"), "got: {stdout}");
    assert!(stdout.contains("Recommended Jobs:"), "got: {stdout}");

    let json_out = harness
        .cmd()
        .args(["probe", "--json", "--path"])
        .arg(&harness.dir)
        .output()
        .expect("run bque probe --json --path");
    assert!(
        json_out.status.success(),
        "bque probe --json failed: {}",
        String::from_utf8_lossy(&json_out.stderr)
    );
    let val: serde_json::Value =
        serde_json::from_slice(&json_out.stdout).expect("parse probe json");
    assert!(val["os"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(val["arch"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(val["logical_cores"].as_u64().unwrap_or(0) >= 1);
    assert!(val["total_ram_mb"].as_u64().unwrap_or(0) > 0);
    assert!(val.get("available_ram_mb").is_some());
    assert!(val.get("swap_total_mb").is_some());
    assert!(val.get("swap_available_mb").is_some());
    assert!(val["disk_path"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(val["disk_free_gb"].as_f64().is_some());
    let tier = val["tier"].as_str().unwrap_or("");
    assert!(
        matches!(tier, "light" | "standard" | "heavy"),
        "unexpected tier: {tier}"
    );
    assert!(val["recommended_build_jobs"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn test_probe_nonexistent_path_fails() {
    let harness = Harness::new("probe_missing_path");
    let missing = harness.state_path("does_not_exist_dir");
    let output = harness
        .cmd()
        .args(["probe", "--path"])
        .arg(&missing)
        .output()
        .expect("run bque probe --path missing");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Probe path does not exist"),
        "got: {stderr}"
    );
}

#[test]
fn test_run_preflight_warn_continues() {
    let harness = Harness::new("preflight_warn");
    let sh = shell_args("echo PREFLIGHT_WARN_RAN");
    let output = harness
        .cmd()
        .args([
            "run",
            "--min-cores",
            "999999",
            "--on-low-resource",
            "warn",
            "--",
        ])
        .args(sh)
        .output()
        .expect("run bque with preflight warn");

    assert!(output.status.success(), "warn policy must allow execution");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("PREFLIGHT_WARN_RAN"), "got: {stdout}");
    assert!(
        stderr.contains("Preflight resource check warning"),
        "got: {stderr}"
    );
    assert!(stderr.contains("Host Resource Profile"), "got: {stderr}");
}

#[test]
fn test_run_preflight_deny_aborts_before_execution() {
    let harness = Harness::new("preflight_deny");
    let sh = shell_args("echo SHOULD_NOT_RUN");
    let output = harness
        .cmd()
        .args([
            "run",
            "--min-cores",
            "999999",
            "--min-ram-mb",
            "999999999",
            "--on-low-resource",
            "deny",
            "--",
        ])
        .args(sh)
        .output()
        .expect("run bque with preflight deny");

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains("SHOULD_NOT_RUN"), "got: {stdout}");
    assert!(
        stderr.contains("Preflight resource check failed"),
        "got: {stderr}"
    );
    assert!(stderr.contains("logical cores"), "got: {stderr}");
    assert!(stderr.contains("total RAM"), "got: {stderr}");
    assert!(stderr.contains("Host Resource Profile"), "got: {stderr}");
}

#[test]
fn test_run_preflight_passes_when_thresholds_met() {
    let harness = Harness::new("preflight_pass");
    let sh = shell_args("echo PREFLIGHT_PASS_OK");
    let output = harness
        .cmd()
        .args([
            "run",
            "--min-cores",
            "1",
            "--min-ram-mb",
            "1",
            "--on-low-resource",
            "deny",
            "--",
        ])
        .args(sh)
        .output()
        .expect("run bque with satisfied preflight");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("PREFLIGHT_PASS_OK"), "got: {stdout}");
    assert!(
        !stderr.contains("Preflight resource check"),
        "got: {stderr}"
    );
}
