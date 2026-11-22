use anyhow::{Result, anyhow};

#[cfg(windows)]
use windows_sys::Win32::Foundation::GetLastError;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessAffinityMask, SetProcessAffinityMask,
};

pub struct CpuTopology {
    pub total_logical_cores: usize,
    pub p_cores_mask: usize,
    pub e_cores_mask: usize,
}

impl CpuTopology {
    pub fn query() -> Self {
        #[cfg(windows)]
        unsafe {
            let proc = GetCurrentProcess();
            let mut proc_mask: usize = 0;
            let mut sys_mask: usize = 0;
            if GetProcessAffinityMask(proc, &mut proc_mask, &mut sys_mask) != 0 {
                let total = sys_mask.count_ones() as usize;
                // If system has more than 8 cores and is a hybrid architecture,
                // roughly split or assign top half as P-cores and bottom half as E-cores
                let (p_mask, e_mask) = if total >= 12 {
                    let half = total / 2;
                    let p = (1usize << (total - half)) - 1;
                    let e = sys_mask & !p;
                    (p, e)
                } else {
                    (sys_mask, sys_mask)
                };

                return Self {
                    total_logical_cores: total,
                    p_cores_mask: p_mask,
                    e_cores_mask: e_mask,
                };
            }
        }

        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let mask = if num_cpus >= usize::BITS as usize {
            usize::MAX
        } else {
            (1usize << num_cpus) - 1
        };

        let (p_mask, e_mask) = if num_cpus >= 12 && num_cpus < usize::BITS as usize {
            let half = num_cpus / 2;
            let p = (1usize << (num_cpus - half)) - 1;
            let e = mask & !p;
            (p, e)
        } else {
            (mask, mask)
        };

        Self {
            total_logical_cores: num_cpus,
            p_cores_mask: p_mask,
            e_cores_mask: e_mask,
        }
    }

    pub fn parse_affinity_spec(&self, spec: &str) -> Result<usize> {
        let trimmed = spec.trim().to_lowercase();
        match trimmed.as_str() {
            "p-cores" | "pcores" | "p" => Ok(self.p_cores_mask),
            "e-cores" | "ecores" | "e" => Ok(self.e_cores_mask),
            "all" => Ok(if self.total_logical_cores >= usize::BITS as usize {
                usize::MAX
            } else {
                (1usize << self.total_logical_cores) - 1
            }),
            _ => {
                let mut mask = 0usize;
                for part in trimmed.split(',') {
                    let part = part.trim();
                    if let Some((start, end)) = part.split_once('-') {
                        let s: usize = start
                            .parse()
                            .map_err(|_| anyhow!("Invalid core index: {}", start))?;
                        let e: usize = end
                            .parse()
                            .map_err(|_| anyhow!("Invalid core index: {}", end))?;
                        for core in s..=e {
                            if core < usize::BITS as usize {
                                mask |= 1usize << core;
                            }
                        }
                    } else if !part.is_empty() {
                        let core: usize = part
                            .parse()
                            .map_err(|_| anyhow!("Invalid core index: {}", part))?;
                        if core < usize::BITS as usize {
                            mask |= 1usize << core;
                        }
                    }
                }
                if mask == 0 {
                    return Err(anyhow!("Affinity spec resolved to empty mask: '{}'", spec));
                }
                Ok(mask)
            }
        }
    }
}

pub fn apply_process_affinity(affinity_mask: usize) -> Result<()> {
    #[cfg(windows)]
    unsafe {
        let proc = GetCurrentProcess();
        if SetProcessAffinityMask(proc, affinity_mask) == 0 {
            return Err(anyhow!(
                "Failed to set process affinity mask 0x{:X} (Win32 error: {})",
                affinity_mask,
                GetLastError()
            ));
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        for cpu in 0..(usize::BITS as usize) {
            if (affinity_mask & (1usize << cpu)) != 0 {
                libc::CPU_SET(cpu, &mut set);
            }
        }
        let res = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        if res != 0 {
            let err = std::io::Error::last_os_error();
            return Err(anyhow!(
                "Failed to set process affinity mask 0x{:X}: {}",
                affinity_mask,
                err
            ));
        }
        Ok(())
    }

    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        let _ = affinity_mask;
        Ok(())
    }
}
