use crate::cli::PriorityArg;
use anyhow::{Result, anyhow};

#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    BELOW_NORMAL_PRIORITY_CLASS, GetCurrentProcess, HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS,
    NORMAL_PRIORITY_CLASS, PROCESS_CREATION_FLAGS, SetPriorityClass,
};

pub fn apply_process_priority(priority: PriorityArg) -> Result<()> {
    #[cfg(windows)]
    unsafe {
        let flag: PROCESS_CREATION_FLAGS = match priority {
            PriorityArg::Idle => IDLE_PRIORITY_CLASS,
            PriorityArg::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            PriorityArg::Normal => NORMAL_PRIORITY_CLASS,
            PriorityArg::High => HIGH_PRIORITY_CLASS,
        };

        let current = GetCurrentProcess();
        if SetPriorityClass(current, flag) == 0 {
            return Err(anyhow!("Failed to set process priority to {:?}", priority));
        }
        Ok(())
    }

    #[cfg(unix)]
    {
        let nice_val: libc::c_int = match priority {
            PriorityArg::Idle => 19,
            PriorityArg::BelowNormal => 10,
            PriorityArg::Normal => 0,
            PriorityArg::High => -5,
        };

        let res = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, nice_val) };
        if res != 0 {
            let err = std::io::Error::last_os_error();
            return Err(anyhow!(
                "Failed to set process priority to {:?} (nice {}): {}",
                priority,
                nice_val,
                err
            ));
        }

        #[cfg(target_os = "linux")]
        if matches!(priority, PriorityArg::Idle) {
            // Best-effort idle I/O scheduling class (IOPRIO_CLASS_IDLE = 3,
            // IOPRIO_WHO_PROCESS = 1). Ignore failures in restricted containers.
            const IOPRIO_WHO_PROCESS: libc::c_long = 1;
            const IOPRIO_CLASS_IDLE: libc::c_long = 3;
            const IOPRIO_CLASS_SHIFT: u32 = 13;
            let ioprio = IOPRIO_CLASS_IDLE << IOPRIO_CLASS_SHIFT;
            unsafe {
                libc::syscall(libc::SYS_ioprio_set, IOPRIO_WHO_PROCESS, 0i64, ioprio);
            }
        }

        Ok(())
    }

    #[cfg(all(not(windows), not(unix)))]
    {
        let _ = priority;
        Ok(())
    }
}
