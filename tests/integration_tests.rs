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
