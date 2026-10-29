use crate::job::JobObject;
use anyhow::{Context, Result, anyhow};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::{Command, Stdio};

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

    let mut cmd = Command::new(program);
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
