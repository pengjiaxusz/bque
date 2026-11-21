use crate::cli::{CleanArgs, PauseArgs, RunArgs, StatusArgs};
use crate::governor::{ActivityGovernorConfig, GovernorWorker, evaluate_target_cpu_rate};
use crate::job::{CpuTopology, JobObject, apply_process_affinity, apply_process_priority};
use crate::lock::{NamedMutex, build_mutex_name};
use crate::runner::execute_child_command;
use crate::state::{
    LockState, PauseState, StateHeartbeat, StateStorage, WaiterState, is_pid_alive,
    storage::WAITER_HEARTBEAT_INTERVAL,
};
use crate::ui::{ColorTheme, LiveProgressReporter, render_full_dashboard};
use anyhow::{Result, anyhow};
use chrono::Utc;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn handle_run(args: RunArgs) -> Result<i32> {
    if args.command.is_empty() {
        return Err(anyhow!("No command specified to run"));
    }

    let my_pid = std::process::id();
    let storage = StateStorage::new()?;
    let desc = args.desc.clone().unwrap_or_else(|| args.command.join(" "));

    let current_dir_str = crate::runner::display_cwd(args.cwd.as_deref());

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
        let waiter = WaiterState::record(
            my_pid,
            args.command.join(" "),
            desc.clone(),
            current_dir_str.clone(),
        );
        storage.register_waiter(&waiter)?;

        let timeout = if args.timeout == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs(args.timeout)
        };

        let lock_name = build_mutex_name();
        let mutex = NamedMutex::new(lock_name.clone());
        let mut reporter = LiveProgressReporter::new(my_pid, desc.clone());
        let start_wait = Instant::now();
        let mut last_heartbeat = Instant::now();

        // Loop until not paused, no benchmark active, and build mutex acquired
        let guard = loop {
            if timeout > Duration::ZERO && start_wait.elapsed() >= timeout {
                let _ = storage.unregister_waiter(my_pid);
                return Err(anyhow!(
                    "Timed out waiting for lock '{}' after {:?}",
                    lock_name,
                    timeout
                ));
            }

            // Heal the queue before deciding whether anything blocks us: a record
            // whose owner exited (or whose PID was recycled) must never keep this
            // process waiting, and must disappear from the dashboard immediately.
            let _ = storage.reap();

            if last_heartbeat.elapsed() >= WAITER_HEARTBEAT_INTERVAL {
                let _ = storage.touch_waiter(my_pid);
                last_heartbeat = Instant::now();
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
    let active_state = LockState::record(
        my_pid,
        args.command.join(" "),
        desc.clone(),
        current_dir_str,
        format!("{:?}", args.priority),
        target_rate,
        args.observer,
    );

    if args.observer {
        storage.register_observer(&active_state)?;
    } else {
        storage.save_lock_state(&active_state)?;
    }

    // Keep proving this record is alive for as long as the child runs, so a
    // stale-looking entry can only ever appear after this process is gone.
    // Declaration order matters: the heartbeat is declared after the record
    // guard, so it stops before the record is removed and cannot resurrect it.
    let _record_guard = ActiveRecordGuard {
        storage: storage.clone(),
        pid: my_pid,
        observer: args.observer,
    };
    let _heartbeat = if args.observer {
        Some(StateHeartbeat::start_observer(storage.clone(), my_pid))
    } else {
        Some(StateHeartbeat::start_lock(storage.clone(), my_pid))
    };

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

    // Execute wrapped command. The record guard removes our state on every exit
    // path, including errors and panics.
    let exit_code =
        execute_child_command(&args.command, args.cwd.as_deref(), job_object.as_deref())?;

    crate::update::check_and_print_notice(&theme);

    Ok(exit_code)
}

/// Removes the active lock/observer record when the build ends, however it ends.
struct ActiveRecordGuard {
    storage: StateStorage,
    pid: u32,
    observer: bool,
}

impl Drop for ActiveRecordGuard {
    fn drop(&mut self) {
        let _ = if self.observer {
            self.storage.unregister_observer(self.pid)
        } else {
            self.storage.clear_lock_state(Some(self.pid))
        };
    }
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
    let state = PauseState {
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

pub fn handle_clean(args: CleanArgs) -> Result<()> {
    let storage = StateStorage::new()?;
    let theme = ColorTheme::new();

    if args.force {
        let report = storage.force_clear()?;
        println!(
            "{} 🧹 Force-cleared queue state: {}.",
            theme.badge_bque(),
            theme.bold_cyan(&report.describe())
        );
        println!(
            "  {} The kernel mutex remains authoritative: a live build keeps running,",
            theme.bold_yellow("⚠️")
        );
        println!("     and the next waiter automatically recovers an abandoned lock.");
        return Ok(());
    }

    let report = storage.reap()?;
    println!(
        "{} 🧹 Cleaned {} dead/orphaned state record(s): {}.",
        theme.badge_bque(),
        theme.bold_cyan(&report.total().to_string()),
        theme.dim(&report.describe())
    );
    Ok(())
}
