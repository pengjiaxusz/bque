use crate::cli::{PauseArgs, RunArgs, StatusArgs};
use crate::governor::{ActivityGovernorConfig, GovernorWorker, evaluate_target_cpu_rate};
use crate::job::{CpuTopology, JobObject, apply_process_affinity, apply_process_priority};
use crate::lock::{BUILD_MUTEX_NAME, NamedMutex};
use crate::runner::execute_child_command;
use crate::state::{LockState, StateStorage, WaiterState, is_pid_alive};
use crate::ui::{ColorTheme, LiveProgressReporter, render_full_dashboard};
use anyhow::{Result, anyhow};
use chrono::Utc;
use std::io::Write;
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
        let mut reporter = LiveProgressReporter::new(my_pid, desc.clone());
        let start_wait = std::time::Instant::now();

        // Loop until not paused, no benchmark active, and build mutex acquired
        let guard = loop {
            if timeout > Duration::ZERO && start_wait.elapsed() >= timeout {
                let _ = storage.unregister_waiter(my_pid);
                return Err(anyhow!(
                    "Timed out waiting for lock '{}' after {:?}",
                    BUILD_MUTEX_NAME,
                    timeout
                ));
            }

            // 1. Check if paused or benchmark active
            let is_blocked = {
                let bench = storage.load_bench_state()?;
                let pause = storage.load_pause_state()?;
                bench.is_some() || pause.is_some()
            };

            if is_blocked {
                if let Ok(status) = storage.get_full_status() {
                    reporter.tick_queue(&status, start_wait.elapsed().as_secs());
                }
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }

            // 2. Attempt to acquire build mutex with 500ms timeout chunk
            let chunk_timeout = Duration::from_millis(500);
            match mutex.acquire_with_progress(chunk_timeout, |_| Ok(())) {
                Ok(guard) => {
                    // Double check bench/pause wasn't initiated in the instant of acquisition
                    if storage.load_bench_state()?.is_none()
                        && storage.load_pause_state()?.is_none()
                    {
                        break guard;
                    }
                    drop(guard);
                    if let Ok(status) = storage.get_full_status() {
                        reporter.tick_queue(&status, start_wait.elapsed().as_secs());
                    }
                }
                Err(_) => {
                    if let Ok(status) = storage.get_full_status() {
                        reporter.tick_queue(&status, start_wait.elapsed().as_secs());
                    }
                }
            }
        };

        let _ = storage.unregister_waiter(my_pid);
        reporter.finish_acquired(start_wait.elapsed().as_secs());

        if guard.was_abandoned() {
            println!(
                "{} {} Previous lock owner exited abnormally. Mutex ownership safely recovered.",
                reporter.theme.badge_bque(),
                reporter.theme.bold_yellow("⚠️")
            );
        }

        Some(guard)
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

    let theme = ColorTheme::new();

    // Set process priority class (Idle by default)
    if let Err(e) = apply_process_priority(args.priority) {
        eprintln!(
            "{} Warning: Failed to set priority {:?}: {}",
            theme.badge_bque(),
            args.priority,
            e
        );
    }

    // Set core affinity if requested
    if let Some(spec) = &args.cores {
        let topo = CpuTopology::query();
        match topo.parse_affinity_spec(spec) {
            Ok(mask) => {
                if let Err(e) = apply_process_affinity(mask) {
                    eprintln!(
                        "{} Warning: Failed to set core affinity: {}",
                        theme.badge_bque(),
                        e
                    );
                }
            }
            Err(e) => eprintln!(
                "{} Warning: Failed to parse affinity spec: {}",
                theme.badge_bque(),
                e
            ),
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
                "{} Warning: Job Object creation failed: {}. Continuing without CPU rate throttle.",
                theme.badge_bque(),
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

    crate::update::check_and_print_notice(&theme);

    Ok(exit_code)
}

pub fn handle_status(args: StatusArgs) -> Result<()> {
    let storage = StateStorage::new()?;

    if args.json {
        let status = storage.get_full_status()?;
        let out = serde_json::to_string_pretty(&status)?;
        println!("{}", out);
        return Ok(());
    }

    let theme = ColorTheme::new();

    if args.watch {
        let mut last_count = 0;
        loop {
            let status = storage.get_full_status()?;
            let lines = render_full_dashboard(&status, None, None, &theme);
            let mut stdout = std::io::stdout().lock();
            if theme.enabled && last_count > 0 {
                let _ = write!(stdout, "\x1b[{}A\r", last_count);
            }
            for line in &lines {
                if theme.enabled {
                    let _ = writeln!(stdout, "\x1b[2K{}", line);
                } else {
                    let _ = writeln!(stdout, "{}", line);
                }
            }
            let _ = stdout.flush();
            last_count = lines.len();
            std::thread::sleep(Duration::from_secs(1));
        }
    } else {
        let status = storage.get_full_status()?;
        let lines = render_full_dashboard(&status, None, None, &theme);
        for line in lines {
            println!("{}", line);
        }
        crate::update::check_and_print_notice(&theme);
        Ok(())
    }
}

pub fn handle_pause(args: PauseArgs) -> Result<()> {
    let storage = StateStorage::new()?;
    let state = crate::state::PauseState {
        paused_at: Utc::now(),
        reason: args.reason.clone(),
        paused_by_pid: std::process::id(),
    };
    storage.save_pause_state(&state)?;
    let theme = ColorTheme::new();
    println!(
        "{} ⏸️  Background builds successfully paused.",
        theme.badge_bque()
    );
    println!("  Reason: {}", theme.bold_white(&args.reason));
    println!("  Run '{}' to unpause.", theme.cyan("bque resume"));
    Ok(())
}

pub fn handle_resume() -> Result<()> {
    let storage = StateStorage::new()?;
    storage.clear_pause_state()?;
    let theme = ColorTheme::new();
    println!("{} ▶️  Background builds resumed.", theme.badge_bque());
    Ok(())
}

pub fn handle_clean() -> Result<()> {
    let storage = StateStorage::new()?;
    let cleaned = storage.clean_dead_entries()?;
    let theme = ColorTheme::new();
    println!(
        "{} 🧹 Cleaned {} dead/orphaned state records.",
        theme.badge_bque(),
        theme.bold_cyan(&cleaned.to_string())
    );
    Ok(())
}
