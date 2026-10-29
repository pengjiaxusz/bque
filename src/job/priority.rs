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

    #[cfg(not(windows))]
    {
        let _ = priority;
        Ok(())
    }
}
