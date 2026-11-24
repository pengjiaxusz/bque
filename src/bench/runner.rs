use crate::cli::BenchArgs;
use crate::job::{
    CpuTopology, FreezeGuard, ProcessFreezer, apply_process_affinity, apply_process_priority,
};
use crate::lock::{NamedMutex, bench_mutex_name};
use crate::runner::{display_cwd, execute_child_command};
use crate::state::process::ProcessIdentity;
use crate::state::{BenchState, StateHeartbeat, StateStorage};
use crate::ui::{ColorTheme, LiveProgressReporter};
use anyhow::{Result, anyhow};
use std::time::Duration;

struct BenchRecordGuard {
    storage: StateStorage,
}

impl Drop for BenchRecordGuard {
    fn drop(&mut self) {
        let _ = self.storage.clear_bench_state();
    }
}

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

    let bench_mutex = NamedMutex::new(bench_mutex_name());
    let timeout = if args.timeout == 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(args.timeout)
    };

    let mut reporter = LiveProgressReporter::new(my_pid, desc.clone());
    let start_wait = std::time::Instant::now();

    let _guard = bench_mutex.acquire_with_progress(timeout, |elapsed| {
        let _ = storage.reap();
        if let Ok(status) = storage.get_full_status() {
            reporter.tick_bench(&status, elapsed.as_secs());
        }
        Ok(())
    })?;
    reporter.finish_bench_acquired(start_wait.elapsed().as_secs());

    // Suspend all active managed tasks and observers when requested.
    let mut frozen_identities: Vec<ProcessIdentity> = Vec::new();
    if args.freeze_builds {
        let active_tasks = storage.list_active_tasks().unwrap_or_default();
        let observers = storage.list_observers().unwrap_or_default();

        for task in &active_tasks {
            println!(
                "{} ⏸️  Freezing active task PID {} ('{}')...",
                theme.badge_bench(),
                theme.bold_cyan(&task.pid.to_string()),
                theme.white(&task.description)
            );
            match freezer.suspend_process_tree_identities(task.pid) {
                Ok(ids) => frozen_identities.extend(ids),
                Err(e) => eprintln!(
                    "{} Warning: Failed to suspend active task PID {}: {}",
                    theme.badge_bench(),
                    task.pid,
                    e
                ),
            }
        }

        for obs in &observers {
            match freezer.suspend_process_tree_identities(obs.pid) {
                Ok(ids) => frozen_identities.extend(ids),
                Err(e) => eprintln!(
                    "{} Warning: Failed to suspend observer PID {}: {}",
                    theme.badge_bench(),
                    obs.pid,
                    e
                ),
            }
        }

        if !frozen_identities.is_empty() {
            println!(
                "{} Successfully suspended {} process(es) across background trees.",
                theme.badge_bench(),
                theme.bold_green(&frozen_identities.len().to_string())
            );
        }
    }

    // Declare RAII guards before saving state / spawning child so every exit
    // path (error, panic, or normal return) resumes frozen trees and clears state.
    let mut freeze_guard = FreezeGuard::new(frozen_identities.clone());
    let state = BenchState::record(
        my_pid,
        args.command.join(" "),
        desc.clone(),
        display_cwd(args.cwd.as_deref()),
        frozen_identities,
    );
    storage.save_bench_state(&state)?;
    let _bench_record_guard = BenchRecordGuard {
        storage: storage.clone(),
    };
    let _heartbeat = StateHeartbeat::start_bench(storage.clone(), state.pid);

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

    unsafe {
        std::env::set_var("BQUE_ACTIVE_PID", my_pid.to_string());
    }
    let exit_code = execute_child_command(&args.command, args.cwd.as_deref(), None)?;

    match freeze_guard.resume_now() {
        Ok(count) if count > 0 => println!(
            "{} Resumed {} frozen background process(es).",
            theme.badge_bench(),
            theme.bold_green(&count.to_string())
        ),
        Ok(_) => {}
        Err(e) => eprintln!(
            "{} Warning: Failed to resume processes: {}",
            theme.badge_bench(),
            e
        ),
    }

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
