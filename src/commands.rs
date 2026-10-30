use crate::cli::{PauseArgs, RunArgs, StatusArgs};
use crate::governor::{ActivityGovernorConfig, GovernorWorker, evaluate_target_cpu_rate};
use crate::job::{CpuTopology, JobObject, apply_process_affinity, apply_process_priority};
use crate::lock::{BUILD_MUTEX_NAME, NamedMutex};
use crate::runner::execute_child_command;
use crate::state::{LockState, StateStorage, WaiterState, is_pid_alive};
use anyhow::{Result, anyhow};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;

pub fn handle_run(args: RunArgs) -> Result<i32> {
    if args.command.is_empty() {
        return Err(anyhow!("No command specified to run"));
    }

    let my_pid = std::process::id();
    let storage = StateStorage::new()?;
    let desc = args.desc.clone().unwrap_or_else(|| args.command.join(" "));

    let current_dir_str = args
        .cwd
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });

    // Check reentrancy: if already running inside an active bque session, pass through directly
    let is_reentrant = std::env::var("BQUE_ACTIVE_PID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|pid| pid == my_pid || is_pid_alive(pid))
        .unwrap_or(false);

    if is_reentrant {
        return execute_child_command(&args.command, args.cwd.as_deref(), None);
    }

    let _lock_guard = if !args.observer {
        // Register as waiter
        let waiter = WaiterState {
            pid: my_pid,
            queued_at: Utc::now(),
            command: args.command.join(" "),
            description: desc.clone(),
            cwd: current_dir_str.clone(),
        };
        storage.register_waiter(&waiter)?;

        let timeout = if args.timeout == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs(args.timeout)
        };

        let mutex = NamedMutex::new(BUILD_MUTEX_NAME);
        let mut last_reported_sec = 0;

        let guard_res = mutex.acquire_with_progress(timeout, |elapsed| {
            // Also wait if paused or benchmark active
            while let Ok(Some(pause)) = storage.load_pause_state() {
                println!(
                    "[bque] Builds are currently paused ('{}'). Waiting...",
                    pause.reason
                );
                std::thread::sleep(Duration::from_secs(2));
            }
            while let Ok(Some(bench)) = storage.load_bench_state() {
                println!(
                    "[bque] Benchmark '{}' is running on PID {}. Waiting...",
                    bench.description, bench.pid
                );
                std::thread::sleep(Duration::from_secs(2));
            }

            let secs = elapsed.as_secs();
            if secs >= last_reported_sec + 5 {
                last_reported_sec = secs;
                if let Ok(status) = storage.get_full_status() {
                    let mut pos = 1;
                    for (i, w) in status.waiters.iter().enumerate() {
                        if w.pid == my_pid {
                            pos = i + 1;
                            break;
                        }
                    }
                    if let Some(active) = status.active_lock {
                        println!(
                            "[bque] Queued at position #{}. Waiting for PID {} ('{}') [elapsed: {}s]...",
                            pos, active.pid, active.description, secs
                        );
                    } else {
                        println!(
                            "[bque] Queued at position #{}. Waiting for lock [elapsed: {}s]...",
                            pos, secs
                        );
                    }
                }
            }
            Ok(())
        });

        let _ = storage.unregister_waiter(my_pid);

        let g = guard_res?;
        if g.was_abandoned() {
            println!(
                "[bque] Notice: Previous lock owner exited abnormally. Mutex ownership safely recovered."
            );
        }

        Some(g)
    } else {
        None
    };

    // Calculate initial CPU rate
    let target_rate = if let Some(rate) = args.cpu_rate {
        rate
    } else {
        let config = ActivityGovernorConfig::default();
        evaluate_target_cpu_rate(&config)
    };

    // Record active state
    let active_state = LockState {
        pid: my_pid,
        start_time: Utc::now(),
        command: args.command.join(" "),
        description: desc.clone(),
        cwd: current_dir_str,
        priority: format!("{:?}", args.priority),
        cpu_rate: target_rate,
        is_observer: args.observer,
    };

    if args.observer {
        storage.register_observer(&active_state)?;
    } else {
        storage.save_lock_state(&active_state)?;
    }

    // Set process priority class (Idle by default)
    if let Err(e) = apply_process_priority(args.priority) {
        eprintln!(
            "[bque] Warning: Failed to set priority {:?}: {}",
            args.priority, e
        );
    }

    // Set core affinity if requested
    if let Some(spec) = &args.cores {
        let topo = CpuTopology::query();
        match topo.parse_affinity_spec(spec) {
            Ok(mask) => {
                if let Err(e) = apply_process_affinity(mask) {
                    eprintln!("[bque] Warning: Failed to set core affinity: {}", e);
                }
            }
            Err(e) => eprintln!("[bque] Warning: Failed to parse affinity spec: {}", e),
        }
    }

    // Initialize Job Object with CPU rate
    let job_object = match JobObject::create(None) {
        Ok(job) => {
            let _ = job.set_cpu_rate(target_rate, args.hard_cap);
            Some(Arc::new(job))
        }
        Err(e) => {
            eprintln!(
                "[bque] Warning: Job Object creation failed: {}. Continuing without CPU rate throttle.",
                e
            );
            None
        }
    };

    // Start dynamic governor if enabled and no fixed rate specified
    let _governor = if !args.no_governor && args.cpu_rate.is_none() {
        job_object
            .as_ref()
            .map(|job| GovernorWorker::start(job.clone(), target_rate, args.hard_cap))
    } else {
        None
    };

    // Pass environment variable for reentrant calls
    unsafe {
        std::env::set_var("BQUE_ACTIVE_PID", my_pid.to_string());
    }

    // Execute wrapped command
    let exit_code =
        execute_child_command(&args.command, args.cwd.as_deref(), job_object.as_deref())?;

    // Cleanup state
    if args.observer {
        let _ = storage.unregister_observer(my_pid);
    } else {
        let _ = storage.clear_lock_state(Some(my_pid));
    }

    Ok(exit_code)
}

pub fn handle_status(args: StatusArgs) -> Result<()> {
    let storage = StateStorage::new()?;
    let status = storage.get_full_status()?;

    if args.json {
        let out = serde_json::to_string_pretty(&status)?;
        println!("{}", out);
        return Ok(());
    }

    println!("============================================================");
    println!("              bque Global Build Queue & Governor            ");
    println!("============================================================");

    // Benchmark mode
    if let Some(bench) = status.active_bench {
        println!("\n[!] ACTIVE BENCHMARK IN PROGRESS");
        println!("  PID:         {}", bench.pid);
        println!("  Command:     {}", bench.command);
        println!("  Description: {}", bench.description);
        println!("  Started At:  {}", bench.start_time.to_rfc3339());
        println!("  Frozen PIDs: {:?}", bench.frozen_pids);
    }

    // Pause mode
    if let Some(pause) = status.pause_state {
        println!("\n[!] BUILDS MANUALLY PAUSED");
        println!("  Reason:      {}", pause.reason);
        println!("  Paused At:   {}", pause.paused_at.to_rfc3339());
        println!("  Paused By:   PID {}", pause.paused_by_pid);
    }

    // Active lock holder
    println!("\n[1] Active Lock Holder:");
    if let Some(lock) = status.active_lock {
        let elapsed = Utc::now() - lock.start_time;
        println!("  PID:         {}", lock.pid);
        println!("  Description: {}", lock.description);
        println!("  Command:     {}", lock.command);
        println!("  Working Dir: {}", lock.cwd);
        println!("  Priority:    {}", lock.priority);
        println!("  CPU Rate:    {}% (soft-cap)", lock.cpu_rate);
        println!("  Running For: {}s", elapsed.num_seconds());
    } else {
        println!("  (No active lock holder. Build queue is idle)");
    }

    // Waiters
    println!("\n[2] Queued Waiters ({}):", status.waiters.len());
    if status.waiters.is_empty() {
        println!("  (Queue is empty)");
    } else {
        for (i, w) in status.waiters.iter().enumerate() {
            let elapsed = Utc::now() - w.queued_at;
            println!(
                "  #{:<2} PID {:<6} [{:>4}s] {} ('{}')",
                i + 1,
                w.pid,
                elapsed.num_seconds(),
                w.description,
                w.cwd
            );
        }
    }

    // Observers
    println!("\n[3] Observers ({}):", status.observers.len());
    if status.observers.is_empty() {
        println!("  (No observers active)");
    } else {
        for obs in status.observers {
            let elapsed = Utc::now() - obs.start_time;
            println!(
                "  PID {:<6} [{:>4}s] {} ('{}')",
                obs.pid,
                elapsed.num_seconds(),
                obs.description,
                obs.command
            );
        }
    }

    println!("============================================================");
    Ok(())
}

pub fn handle_pause(args: PauseArgs) -> Result<()> {
    let storage = StateStorage::new()?;
    let state = crate::state::PauseState {
        paused_at: Utc::now(),
        reason: args.reason.clone(),
        paused_by_pid: std::process::id(),
    };
    storage.save_pause_state(&state)?;
    println!("[bque] Background builds successfully paused.");
    println!("Reason: {}", args.reason);
    println!("Run 'bque resume' to unpause.");
    Ok(())
}

pub fn handle_resume() -> Result<()> {
    let storage = StateStorage::new()?;
    storage.clear_pause_state()?;
    println!("[bque] Background builds resumed.");
    Ok(())
}

pub fn handle_clean() -> Result<()> {
    let storage = StateStorage::new()?;
    let cleaned = storage.clean_dead_entries()?;
    println!("[bque] Cleaned {} dead/orphaned state records.", cleaned);
    Ok(())
}
