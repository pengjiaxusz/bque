use std::process::Command;

#[test]
fn test_cli_help() {
    let exe = env!("CARGO_BIN_EXE_bque");
    let output = Command::new(exe)
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
    let exe = env!("CARGO_BIN_EXE_bque");
    let output = Command::new(exe)
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
    let exe = env!("CARGO_BIN_EXE_bque");
    let output = Command::new(exe)
        .args(["status", "--json"])
        .output()
        .expect("Failed to execute bque status --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse JSON status");
    assert!(val.get("waiters").is_some());
    assert!(val.get("observers").is_some());
}

#[test]
fn test_pause_and_resume_cycle() {
    let exe = env!("CARGO_BIN_EXE_bque");
    // Pause
    let p_out = Command::new(exe)
        .args(["pause", "--reason", "Integration Test"])
        .output()
        .expect("Failed to pause");
    assert!(p_out.status.success());

    // Check status
    let s_out = Command::new(exe)
        .args(["status", "--json"])
        .output()
        .expect("Failed to get status");
    let s_val: serde_json::Value = serde_json::from_slice(&s_out.stdout).unwrap();
    assert!(s_val.get("pause_state").unwrap().is_object());

    // Resume
    let r_out = Command::new(exe)
        .arg("resume")
        .output()
        .expect("Failed to resume");
    assert!(r_out.status.success());

    // Check status after resume
    let s2_out = Command::new(exe)
        .args(["status", "--json"])
        .output()
        .expect("Failed to get status");
    let s2_val: serde_json::Value = serde_json::from_slice(&s2_out.stdout).unwrap();
    assert!(s2_val.get("pause_state").unwrap().is_null());
}

#[test]
fn test_benchmark_clean_execution() {
    let exe = env!("CARGO_BIN_EXE_bque");
    let output = Command::new(exe)
        .env_remove("BQUE_ACTIVE_PID")
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
    let exe = env!("CARGO_BIN_EXE_bque");
    let current_pid = std::process::id().to_string();

    let output = Command::new(exe)
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
    let exe = env!("CARGO_BIN_EXE_bque");
    let temp_dir = std::env::temp_dir().join(format!("bque_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let cmd_file = temp_dir.join("bquetestcmd.cmd");
    std::fs::write(&cmd_file, "@echo PATHEXT_RESOLUTION_OK\r\n").expect("write cmd file");

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{};{}", temp_dir.display(), current_path);

    let output = Command::new(exe)
        .env("PATH", new_path)
        .args(["run", "--", "bquetestcmd"])
        .output()
        .expect("Failed to run bquetestcmd");

    let _ = std::fs::remove_file(cmd_file);
    let _ = std::fs::remove_dir(temp_dir);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PATHEXT_RESOLUTION_OK"));
}

#[test]
fn test_status_dashboard_rendering() {
    let exe = env!("CARGO_BIN_EXE_bque");
    let output = Command::new(exe)
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
    let exe = env!("CARGO_BIN_EXE_bque");
    let p_out = Command::new(exe)
        .args(["pause", "--reason", "Wait Notification Test"])
        .output()
        .expect("pause failed");
    assert!(p_out.status.success());

    let run_out = Command::new(exe)
        .env_remove("BQUE_ACTIVE_PID")
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

    let r_out = Command::new(exe)
        .arg("resume")
        .output()
        .expect("resume failed");
    assert!(r_out.status.success());

    let stdout = String::from_utf8_lossy(&run_out.stdout);
    let stderr = String::from_utf8_lossy(&run_out.stderr);
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("Blocked by Pause 'Wait Notification Test'")
            || combined.contains("Timed out waiting")
    );
}
