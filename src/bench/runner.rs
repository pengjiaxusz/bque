use crate::cli::BenchArgs;
use crate::job::{CpuTopology, ProcessFreezer, apply_process_affinity, apply_process_priority};
use crate::lock::{BENCH_MUTEX_NAME, NamedMutex};
use crate::runner::execute_child_command;
use crate::state::{BenchState, StateStorage};
use anyhow::{Result, anyhow};
use chrono::Utc;
use std::time::Duration;

pub fn run_benchmark(args: BenchArgs) -> Result<i32> {
    if args.command.is_empty() {
        return Err(anyhow!("No benchmark command specified"));
    }

    let storage = StateStorage::new()?;
    let freezer = ProcessFreezer::new();

    let bench_mutex = NamedMutex::new(BENCH_MUTEX_NAME);
    let timeout = if args.timeout == 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(args.timeout)
    };

    println!("[bque bench] Requesting exclusive benchmark lock...");
    let _guard = bench_mutex.acquire_with_progress(timeout, |elapsed| {
        println!(
            "[bque bench] Waiting for previous benchmark to complete [{:?}]...",
            elapsed
        );
        Ok(())
    })?;

    // Suspend background build if active and requested
    let mut frozen_pids = Vec::new();
    let active_build = if args.freeze_builds {
        storage.load_lock_state().ok().flatten()
    } else {
        None
    };

    if let Some(active) = active_build {
        println!(
            "[bque bench] Freezing active background build PID {} ('{}')...",
            active.pid, active.description
        );
        match freezer.suspend_process_tree(active.pid) {
            Ok(pids) => {
                println!(
                    "[bque bench] Successfully suspended {} processes in build tree.",
                    pids.len()
                );
                frozen_pids = pids;
            }
            Err(e) => {
                eprintln!(
                    "[bque bench] Warning: Failed to suspend background build PID {}: {}",
                    active.pid, e
                );
            }
        }
    }

    let desc = args.desc.clone().unwrap_or_else(|| args.command.join(" "));

    let state = BenchState {
        pid: std::process::id(),
        start_time: Utc::now(),
        command: args.command.join(" "),
        description: desc.clone(),
        frozen_pids: frozen_pids.clone(),
    };
    storage.save_bench_state(&state)?;

    // Apply benchmark priority (default: High)
    if let Err(e) = apply_process_priority(args.priority) {
        eprintln!(
            "[bque bench] Warning: Failed to set benchmark priority: {}",
            e
        );
    } else {
        println!(
            "[bque bench] Elevated process priority to {:?}",
            args.priority
        );
    }

    // Apply core affinity if requested (e.g. p-cores)
    if let Some(spec) = &args.cores {
        let topo = CpuTopology::query();
        match topo.parse_affinity_spec(spec) {
            Ok(mask) => {
                if let Err(e) = apply_process_affinity(mask) {
                    eprintln!("[bque bench] Warning: Failed to set core affinity: {}", e);
                } else {
                    println!(
                        "[bque bench] Pinned benchmark to core affinity mask: 0x{:X}",
                        mask
                    );
                }
            }
            Err(e) => eprintln!("[bque bench] Warning: Failed to parse affinity spec: {}", e),
        }
    }

    println!("[bque bench] Running benchmark with clean CPU: {}", desc);
    let exit_code = execute_child_command(&args.command, args.cwd.as_deref(), None)?;

    // Clean up: resume frozen background builds
    if !frozen_pids.is_empty() {
        println!("[bque bench] Resuming frozen background build processes...");
        match freezer.resume_processes(&frozen_pids) {
            Ok(count) => println!("[bque bench] Resumed {} processes.", count),
            Err(e) => eprintln!("[bque bench] Warning: Failed to resume processes: {}", e),
        }
    }

    let _ = storage.clear_bench_state();
    println!(
        "[bque bench] Benchmark finished with exit code {}.",
        exit_code
    );

    Ok(exit_code)
}
