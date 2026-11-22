use crate::job::JobObject;
use anyhow::{Context, Result, anyhow};
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::sync::Once;
#[cfg(unix)]
use std::sync::atomic::{AtomicI32, Ordering};

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
///
/// A relative `--cwd` is still handed to the child unchanged (that is the
/// caller's intent), but the dashboard has to name a real place: a queued job
/// that only shows `..\sibling` cannot be traced back to its worktree.
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
        // Place the child in its own process group so signals and cleanup
        // reach every descendant compiler process.
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

    let status = child
        .wait()
        .with_context(|| format!("Error while waiting for child process '{}'", program))?;

    #[cfg(unix)]
    {
        ACTIVE_CHILD_PGID.store(0, Ordering::SeqCst);
        if let Some(job) = job_object {
            job.clear_process();
        }
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
