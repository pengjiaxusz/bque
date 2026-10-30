use crate::job::JobObject;
use anyhow::{Context, Result, anyhow};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::{Command, Stdio};

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

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to start child process '{}'", program))?;

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
    }

    let status = child
        .wait()
        .with_context(|| format!("Error while waiting for child process '{}'", program))?;

    let exit_code = status.code().unwrap_or(-1);
    Ok(exit_code)
}
