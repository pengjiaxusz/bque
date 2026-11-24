use crate::job::{JobObject, cpu_delta_to_host_pct};
use crate::state::{ActivityState, StateStorage};
use anyhow::{Context, Result, anyhow};
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::sync::Once;
#[cfg(unix)]
use std::sync::atomic::AtomicI32;

/// Exit code returned when `--stall-timeout` or `--max-duration` terminates a task.
pub const WATCHDOG_EXIT_CODE: i32 = 124;

/// Host-normalized CPU percentage below which a task is considered low-activity
/// for Yielding purposes.
const YIELD_CPU_THRESHOLD_PCT: f64 = 5.0;
/// Host-normalized CPU percentage below which a task is considered stalled for
/// `--stall-timeout` purposes.
const STALL_CPU_THRESHOLD_PCT: f64 = 0.5;
/// Sampling cadence for the background activity & watchdog thread.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

#[cfg(unix)]
static ACTIVE_CHILD_PGID: AtomicI32 = AtomicI32::new(0);
#[cfg(unix)]
static SIGNAL_HANDLER_INIT: Once = Once::new();

#[cfg(unix)]
fn install_signal_forwarder() {
    SIGNAL_HANDLER_INIT.call_once(|| {
        let _ = ctrlc::set_handler(|| {
            let pgid = ACTIVE_CHILD_PGID.load(Ordering::SeqCst);
            if pgid > 0 {
                unsafe {
                    libc::kill(-pgid, libc::SIGTERM);
                }
            }
        });
    });
}

/// Configuration for the execution-phase activity monitor and watchdog.
pub struct MonitorConfig {
    pub storage: StateStorage,
    pub pid: u32,
    pub observer: bool,
    pub yield_after_secs: Option<u64>,
    pub stall_timeout_secs: Option<u64>,
    pub max_duration_secs: Option<u64>,
}

#[cfg(windows)]
fn resolve_program(program: &str) -> std::path::PathBuf {
    let path = Path::new(program);
    if program.contains('/') || program.contains('\\') {
        if path.is_file() {
            return path.to_path_buf();
        }
        if path.extension().is_none()
            && let Ok(pathext) = std::env::var("PATHEXT")
        {
            for ext in pathext.split(';').filter(|s| !s.is_empty()) {
                let ext_clean = ext.trim_start_matches('.');
                let candidate = path.with_extension(ext_clean);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
        return path.to_path_buf();
    }

    if path.extension().is_some() && path.is_file() {
        return path.to_path_buf();
    }

    let default_exts = [".COM", ".EXE", ".BAT", ".CMD"];
    let pathext_var = std::env::var("PATHEXT").ok();
    let exts: Vec<&str> = match &pathext_var {
        Some(val) => val.split(';').filter(|s| !s.is_empty()).collect(),
        None => default_exts.to_vec(),
    };

    if let Ok(paths) = std::env::var("PATH") {
        for dir in std::env::split_paths(&paths) {
            if path.extension().is_none() {
                for ext in &exts {
                    let ext_clean = ext.trim_start_matches('.');
                    let candidate = dir.join(format!("{}.{}", program, ext_clean));
                    if candidate.is_file() {
                        return candidate;
                    }
                }
            }
            let direct = dir.join(program);
            if direct.is_file() {
                return direct;
            }
        }
    }

    path.to_path_buf()
}

#[cfg(not(windows))]
fn resolve_program(program: &str) -> std::path::PathBuf {
    Path::new(program).to_path_buf()
}

/// Absolute working directory recorded in the queue for display.
pub fn display_cwd(cwd: Option<&Path>) -> String {
    let absolute = match cwd {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir()
            .map(|base| base.join(path))
            .unwrap_or_else(|_| path.to_path_buf()),
        None => std::env::current_dir().unwrap_or_default(),
    };
    absolute.to_string_lossy().to_string()
}

pub fn execute_child_command(
    command_args: &[String],
    cwd: Option<&Path>,
    job_object: Option<&JobObject>,
) -> Result<i32> {
    execute_child_command_internal(command_args, cwd, job_object, None, None)
}

pub fn execute_child_command_monitored(
    command_args: &[String],
    cwd: Option<&Path>,
    job_object: Option<Arc<JobObject>>,
    monitor: MonitorConfig,
) -> Result<i32> {
    let job_ref = job_object.as_deref();
    execute_child_command_internal(
        command_args,
        cwd,
        job_ref,
        job_object.clone(),
        Some(monitor),
    )
}

fn execute_child_command_internal(
    command_args: &[String],
    cwd: Option<&Path>,
    job_object: Option<&JobObject>,
    job_arc: Option<Arc<JobObject>>,
    monitor: Option<MonitorConfig>,
) -> Result<i32> {
    if command_args.is_empty() {
        return Err(anyhow!("Command argument list is empty"));
    }

    let program = &command_args[0];
    let args = &command_args[1..];

    let resolved_program = resolve_program(program);
    let mut cmd = Command::new(&resolved_program);
    cmd.args(args);

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        install_signal_forwarder();
        // Place the child in its own process group so signals, CPU accounting,
        // and cleanup reach every descendant compiler process.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to start child process '{}'", program))?;

    #[cfg(unix)]
    let child_pgid = child.id() as i32;

    #[cfg(unix)]
    ACTIVE_CHILD_PGID.store(child_pgid, Ordering::SeqCst);

    if let Some(job) = job_object {
        #[cfg(windows)]
        {
            let handle = child.as_raw_handle() as isize;
            if let Err(e) = job.assign_process(handle) {
                eprintln!(
                    "[bque] Warning: Failed to assign child process to Job Object: {}",
                    e
                );
            }
        }
        #[cfg(unix)]
        {
            let _ = job.assign_process(child_pgid as isize);
        }
    }

    let stop_monitor = Arc::new(AtomicBool::new(false));
    let killed_by_watchdog = Arc::new(AtomicBool::new(false));

    let monitor_handle = if let (Some(cfg), Some(job)) = (monitor, job_arc) {
        let stop = Arc::clone(&stop_monitor);
        let killed = Arc::clone(&killed_by_watchdog);
        Some(std::thread::spawn(move || {
            run_activity_monitor(cfg, job, stop, killed);
        }))
    } else {
        None
    };

    let status = child
        .wait()
        .with_context(|| format!("Error while waiting for child process '{}'", program))?;

    stop_monitor.store(true, Ordering::SeqCst);
    if let Some(handle) = monitor_handle {
        let _ = handle.join();
    }

    #[cfg(unix)]
    {
        ACTIVE_CHILD_PGID.store(0, Ordering::SeqCst);
        if let Some(job) = job_object {
            job.clear_process();
        }
    }

    if killed_by_watchdog.load(Ordering::SeqCst) {
        return Ok(WATCHDOG_EXIT_CODE);
    }

    #[cfg(unix)]
    let exit_code = {
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .or_else(|| status.signal().map(|sig| 128 + sig))
            .unwrap_or(-1)
    };

    #[cfg(not(unix))]
    let exit_code = status.code().unwrap_or(-1);

    Ok(exit_code)
}

fn run_activity_monitor(
    cfg: MonitorConfig,
    job: Arc<JobObject>,
    stop: Arc<AtomicBool>,
    killed_by_watchdog: Arc<AtomicBool>,
) {
    let logical_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1);
    let started_at = Instant::now();
    let mut last_sample_at = Instant::now();
    let mut last_cpu_ms = job.total_cpu_time_ms().unwrap_or(0);
    let mut low_cpu_ms: u64 = 0;
    let mut stall_cpu_ms: u64 = 0;
    let mut activity_state = ActivityState::Active;

    while !stop.load(Ordering::Relaxed) {
        // Sleep in 250ms slices so shutdown is responsive when the child exits.
        let mut slept = Duration::ZERO;
        while slept < SAMPLE_INTERVAL {
            let step = Duration::from_millis(250).min(SAMPLE_INTERVAL - slept);
            std::thread::sleep(step);
            slept += step;
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if let Some(max_secs) = cfg.max_duration_secs
                && started_at.elapsed() >= Duration::from_secs(max_secs)
            {
                eprintln!(
                    "[bque] Watchdog: max duration ({}s) exceeded; terminating child process tree (exit {}).",
                    max_secs, WATCHDOG_EXIT_CODE
                );
                killed_by_watchdog.store(true, Ordering::SeqCst);
                let _ = job.terminate(WATCHDOG_EXIT_CODE as u32);
                return;
            }
        }

        let now = Instant::now();
        let elapsed_ms = now.duration_since(last_sample_at).as_millis() as u64;
        last_sample_at = now;

        let current_cpu_ms = job.total_cpu_time_ms().unwrap_or(last_cpu_ms);
        let delta_cpu_ms = current_cpu_ms.saturating_sub(last_cpu_ms);
        last_cpu_ms = current_cpu_ms;

        let cpu_pct = cpu_delta_to_host_pct(delta_cpu_ms, elapsed_ms, logical_cores);

        if cpu_pct < YIELD_CPU_THRESHOLD_PCT {
            low_cpu_ms = low_cpu_ms.saturating_add(elapsed_ms);
        } else {
            low_cpu_ms = 0;
        }

        if cpu_pct < STALL_CPU_THRESHOLD_PCT {
            stall_cpu_ms = stall_cpu_ms.saturating_add(elapsed_ms);
        } else {
            stall_cpu_ms = 0;
        }

        let low_cpu_secs = low_cpu_ms / 1_000;
        let stall_secs = stall_cpu_ms / 1_000;

        if !cfg.observer {
            if let Some(yield_after) = cfg.yield_after_secs {
                if low_cpu_secs >= yield_after {
                    activity_state = ActivityState::Yielding;
                } else if cpu_pct >= YIELD_CPU_THRESHOLD_PCT {
                    activity_state = ActivityState::Active;
                }
            }
            let _ = cfg.storage.update_task_activity(
                cfg.pid,
                cpu_pct,
                low_cpu_secs,
                activity_state,
                None,
            );
        } else {
            let _ = cfg.storage.touch_observer(cfg.pid);
        }

        if let Some(stall_limit) = cfg.stall_timeout_secs
            && stall_secs >= stall_limit
        {
            eprintln!(
                "[bque] Watchdog: task stalled at {:.1}% CPU for {}s (limit {}s); terminating child process tree (exit {}).",
                cpu_pct, stall_secs, stall_limit, WATCHDOG_EXIT_CODE
            );
            killed_by_watchdog.store(true, Ordering::SeqCst);
            let _ = job.terminate(WATCHDOG_EXIT_CODE as u32);
            return;
        }
    }
}
