use crate::cli::{CleanArgs, LowResourcePolicy, PauseArgs, ProbeArgs, RunArgs, StatusArgs};
use crate::governor::{ActivityGovernorConfig, GovernorWorker, evaluate_target_cpu_rate};
use crate::job::{
    CpuTopology, JobObject, ProcessFreezer, apply_process_affinity, apply_process_priority,
};
use crate::lock::{NamedMutex, coord_mutex_name};
use crate::probe::{evaluate_preflight, format_probe_summary, probe_host};
use crate::runner::{MonitorConfig, execute_child_command, execute_child_command_monitored};
use crate::state::process::ProcessIdentity;
use crate::state::{
    LockState, PauseState, StateStorage, TaskState, WaiterState, is_pid_alive,
    storage::WAITER_HEARTBEAT_INTERVAL,
};
use crate::ui::{ColorTheme, LiveProgressReporter, render_full_dashboard};
use anyhow::{Result, anyhow};
use chrono::Utc;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PREFLIGHT_DENIED_EXIT_CODE: i32 = 2;

pub fn handle_probe(args: ProbeArgs) -> Result<()> {
    let profile = probe_host(args.path.as_deref())?;
    if args.json {
        let out = serde_json::to_string_pretty(&profile)?;
        println!("{}", out);
        return Ok(());
    }

    let theme = ColorTheme::new();
    println!("{}", format_probe_summary(&profile, &theme));
    Ok(())
}

pub fn handle_run(args: RunArgs) -> Result<i32> {
    if args.command.is_empty() {
        return Err(anyhow!("No command specified to run"));
    }

    let theme = ColorTheme::new();

    if args.min_cores.is_some() || args.min_ram_mb.is_some() {
        let profile = probe_host(args.cwd.as_deref())?;
        let violations = evaluate_preflight(&profile, args.min_cores, args.min_ram_mb);
        if !violations.is_empty() {
            let reason = violations.join("; ");
            match args.on_low_resource {
                LowResourcePolicy::Warn => {
                    eprintln!(
                        "{} {} Preflight resource check warning: {}",
                        theme.badge_bque(),
                        theme.bold_yellow("WARN"),
                        reason
                    );
                    eprintln!("{}", format_probe_summary(&profile, &theme));
                    eprintln!(
                        "{} Continuing execution because --on-low-resource=warn.",
                        theme.badge_bque()
                    );
                }
                LowResourcePolicy::Deny => {
                    eprintln!(
                        "{} {} Preflight resource check failed: {}",
                        theme.badge_bque(),
                        theme.bold_red("DENIED"),
                        reason
                    );
                    eprintln!("{}", format_probe_summary(&profile, &theme));
                    eprintln!(
                        "{} Refusing to start command because --on-low-resource=deny.",
                        theme.badge_bque()
                    );
                    return Ok(PREFLIGHT_DENIED_EXIT_CODE);
                }
            }
        }
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

    // Calculate initial CPU rate (excluding active bque tasks so we don't self-throttle)
    let target_rate = if let Some(rate) = args.cpu_rate {
        rate
    } else {
        let config = ActivityGovernorConfig::default();
        let bque_cpu = storage.total_active_cpu_pct();
        crate::governor::activity::evaluate_target_cpu_rate_excluding(&config, bque_cpu)
    };

    if args.observer {
        let obs_state = LockState::record(
            my_pid,
            args.command.join(" "),
            desc.clone(),
            current_dir_str,
            format!("{:?}", args.priority),
            target_rate,
            true,
        );
        storage.register_observer(&obs_state)?;
    } else {
        let waiter = WaiterState::record(
            my_pid,
            args.command.join(" "),
            desc.clone(),
            current_dir_str.clone(),
            args.weight,
            args.scope.clone(),
        );
        storage.register_waiter(&waiter)?;
        let _waiter_guard = WaiterCleanupGuard {
            storage: storage.clone(),
            pid: my_pid,
        };

        let timeout = if args.timeout == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs(args.timeout)
        };

        let coord_name = coord_mutex_name();
        let coord_mutex = NamedMutex::new(coord_name.clone());
        let mut reporter = LiveProgressReporter::new(my_pid, desc.clone());
        let start_wait = Instant::now();
        let mut last_heartbeat = Instant::now();

        loop {
            if timeout > Duration::ZERO && start_wait.elapsed() >= timeout {
                return Err(anyhow!(
                    "Timed out waiting for queue budget after {:?}",
                    timeout
                ));
            }

            let admitted = if let Ok(_coord) =
                coord_mutex.acquire_with_progress(Duration::from_millis(500), |_| Ok(()))
            {
                let _ = storage.reap();

                if last_heartbeat.elapsed() >= WAITER_HEARTBEAT_INTERVAL {
                    let _ = storage.touch_waiter(my_pid);
                    last_heartbeat = Instant::now();
                }

                if storage.can_admit(my_pid, args.weight, args.scope.as_deref())? {
                    let task_state = TaskState::record(
                        my_pid,
                        args.command.join(" "),
                        desc.clone(),
                        current_dir_str.clone(),
                        format!("{:?}", args.priority),
                        target_rate,
                        args.weight,
                        args.scope.clone(),
                    );
                    storage.register_task(&task_state)?;
                    let _ = storage.unregister_waiter(my_pid);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if admitted {
                reporter.finish_acquired(start_wait.elapsed().as_secs());
                break;
            }

            if let Ok(status) = storage.get_full_status() {
                reporter.tick_queue(&status, start_wait.elapsed().as_secs());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    // Remove the active task/observer record on every exit path, including errors and panics.
    let _record_guard = ActiveRecordGuard {
        storage: storage.clone(),
        pid: my_pid,
        observer: args.observer,
    };

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
        job_object.as_ref().map(|job| {
            GovernorWorker::start(
                job.clone(),
                storage.clone(),
                my_pid,
                args.weight,
                args.observer,
                target_rate,
                args.hard_cap,
            )
        })
    } else {
        None
    };

    // Pass environment variable for reentrant calls
    unsafe {
        std::env::set_var("BQUE_ACTIVE_PID", my_pid.to_string());
    }

    let monitor_cfg = MonitorConfig {
        storage: storage.clone(),
        pid: my_pid,
        observer: args.observer,
        yield_after_secs: if args.no_yield {
            None
        } else {
            Some(args.yield_after)
        },
        stall_timeout_secs: args.stall_timeout,
        max_duration_secs: args.max_duration,
    };

    let exit_code = execute_child_command_monitored(
        &args.command,
        args.cwd.as_deref(),
        job_object,
        monitor_cfg,
    )?;

    crate::update::check_and_print_notice(&theme);

    Ok(exit_code)
}

/// Ensures a waiter file is removed if admission times out or errors.
struct WaiterCleanupGuard {
    storage: StateStorage,
    pid: u32,
}

impl Drop for WaiterCleanupGuard {
    fn drop(&mut self) {
        let _ = self.storage.unregister_waiter(self.pid);
    }
}

/// Removes the active task/observer record when the build ends, however it ends.
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
            self.storage.unregister_task(self.pid)
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
    let _ = storage.reap();
    let freezer = ProcessFreezer::new();

    let mut frozen_identities: Vec<ProcessIdentity> = Vec::new();
    for task in storage.list_active_tasks().unwrap_or_default() {
        if let Ok(ids) = freezer.suspend_process_tree_identities(task.pid) {
            frozen_identities.extend(ids);
        }
    }
    if !args.keep_observers {
        for obs in storage.list_observers().unwrap_or_default() {
            if let Ok(ids) = freezer.suspend_process_tree_identities(obs.pid) {
                frozen_identities.extend(ids);
            }
        }
    }

    let frozen_count = frozen_identities.len();
    let state = PauseState {
        paused_at: Utc::now(),
        reason: args.reason.clone(),
        paused_by_pid: std::process::id(),
        frozen_identities,
        keep_observers: args.keep_observers,
    };
    storage.save_pause_state(&state)?;
    let theme = ColorTheme::new();
    println!(
        "{} ⏸️  Background builds successfully paused.",
        theme.badge_bque()
    );
    println!("  Reason: {}", theme.bold_white(&args.reason));
    if frozen_count > 0 {
        println!(
            "  Frozen: {} active process(es)",
            theme.bold_cyan(&frozen_count.to_string())
        );
    }
    println!("  Run '{}' to unpause.", theme.cyan("bque resume"));
    Ok(())
}

pub fn handle_resume() -> Result<()> {
    let storage = StateStorage::new()?;
    let mut resumed_count = 0;
    if let Some(pause) = storage.load_pause_state()?
        && !pause.frozen_identities.is_empty()
    {
        let freezer = ProcessFreezer::new();
        resumed_count = freezer
            .resume_identities(&pause.frozen_identities)
            .unwrap_or(0);
    }
    storage.clear_pause_state()?;
    let theme = ColorTheme::new();
    if resumed_count > 0 {
        println!(
            "{} ▶️  Background builds resumed ({} process(es) unfrozen).",
            theme.badge_bque(),
            theme.bold_green(&resumed_count.to_string())
        );
    } else {
        println!("{} ▶️  Background builds resumed.", theme.badge_bque());
    }
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

#[allow(dead_code)]
fn _keep_evaluate_import() {
    let _ = evaluate_target_cpu_rate;
}
