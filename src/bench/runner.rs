use crate::cli::BenchArgs;
use crate::job::{CpuTopology, ProcessFreezer, apply_process_affinity, apply_process_priority};
use crate::lock::{BENCH_MUTEX_NAME, NamedMutex};
use crate::runner::execute_child_command;
use crate::state::{BenchState, StateStorage};
use crate::ui::{ColorTheme, LiveProgressReporter};
use anyhow::{Result, anyhow};
use chrono::Utc;
use std::time::Duration;

pub fn run_benchmark(args: BenchArgs) -> Result<i32> {
    if args.command.is_empty() {
        return Err(anyhow!("No benchmark command specified"));
    }

    let my_pid = std::process::id();

    // Check reentrancy: if already running inside an active bque session, pass through directly
    let is_reentrant = std::env::var("BQUE_ACTIVE_PID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|pid| pid == my_pid || crate::state::is_pid_alive(pid))
        .unwrap_or(false);

    if is_reentrant {
        return execute_child_command(&args.command, args.cwd.as_deref(), None);
    }

    let storage = StateStorage::new()?;
    let freezer = ProcessFreezer::new();
    let theme = ColorTheme::new();
    let desc = args.desc.clone().unwrap_or_else(|| args.command.join(" "));

    let bench_mutex = NamedMutex::new(BENCH_MUTEX_NAME);
    let timeout = if args.timeout == 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(args.timeout)
    };

    let mut reporter = LiveProgressReporter::new(my_pid, desc.clone());
    let start_wait = std::time::Instant::now();

    let _guard = bench_mutex.acquire_with_progress(timeout, |elapsed| {
        if let Ok(status) = storage.get_full_status() {
            reporter.tick_bench(&status, elapsed.as_secs());
        }
        Ok(())
    })?;
    reporter.finish_bench_acquired(start_wait.elapsed().as_secs());

    // Suspend background build if active and requested
    let mut frozen_pids = Vec::new();
    let active_build = if args.freeze_builds {
        storage.load_lock_state().ok().flatten()
    } else {
        None
    };

    if let Some(active) = active_build {
        println!(
            "{} ⏸️  Freezing active background build PID {} ('{}')...",
            theme.badge_bench(),
            theme.bold_cyan(&active.pid.to_string()),
            theme.white(&active.description)
        );
        match freezer.suspend_process_tree(active.pid) {
            Ok(pids) => {
                println!(
                    "{} Successfully suspended {} processes in build tree.",
                    theme.badge_bench(),
                    theme.bold_green(&pids.len().to_string())
                );
                frozen_pids = pids;
            }
            Err(e) => {
                eprintln!(
                    "{} Warning: Failed to suspend background build PID {}: {}",
                    theme.badge_bench(),
                    active.pid,
                    e
                );
            }
        }
    }

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
            "{} Warning: Failed to set benchmark priority: {}",
            theme.badge_bench(),
            e
        );
    } else {
        println!(
            "{} Elevated process priority to {}",
            theme.badge_bench(),
            theme.bold_cyan(&format!("{:?}", args.priority))
        );
    }

    // Apply core affinity if requested (e.g. p-cores)
    if let Some(spec) = &args.cores {
        let topo = CpuTopology::query();
        match topo.parse_affinity_spec(spec) {
            Ok(mask) => {
                if let Err(e) = apply_process_affinity(mask) {
                    eprintln!(
                        "{} Warning: Failed to set core affinity: {}",
                        theme.badge_bench(),
                        e
                    );
                } else {
                    println!(
                        "{} Pinned benchmark to core affinity mask: 0x{:X}",
                        theme.badge_bench(),
                        mask
                    );
                }
            }
            Err(e) => eprintln!(
                "{} Warning: Failed to parse affinity spec: {}",
                theme.badge_bench(),
                e
            ),
        }
    }

    println!(
        "{} 🧪 Running benchmark with clean CPU: {}",
        theme.badge_bench(),
        theme.bold_white(&desc)
    );

    // Pass environment variable for reentrant calls in benchmark
    unsafe {
        std::env::set_var("BQUE_ACTIVE_PID", my_pid.to_string());
    }
    let exit_code = execute_child_command(&args.command, args.cwd.as_deref(), None)?;

    // Clean up: resume frozen background builds
    if !frozen_pids.is_empty() {
        println!(
            "{} Resuming frozen background build processes...",
            theme.badge_bench()
        );
        match freezer.resume_processes(&frozen_pids) {
            Ok(count) => println!(
                "{} Resumed {} processes.",
                theme.badge_bench(),
                theme.bold_green(&count.to_string())
            ),
            Err(e) => eprintln!(
                "{} Warning: Failed to resume processes: {}",
                theme.badge_bench(),
                e
            ),
        }
    }

    let _ = storage.clear_bench_state();
    println!(
        "{} Benchmark finished with exit code {}.",
        theme.badge_bench(),
        if exit_code == 0 {
            theme.bold_green(&exit_code.to_string())
        } else {
            theme.bold_red(&exit_code.to_string())
        }
    );

    Ok(exit_code)
}
