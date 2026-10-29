use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Foundation::FILETIME;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::GetSystemTimes;

/// Configuration for the system activity and CPU contention governor.
#[derive(Debug, Clone)]
pub struct ActivityGovernorConfig {
    /// Target CPU rate during heavy contention (percentage 1-100)
    pub heavy_contention_rate: u32,
    /// Target CPU rate during moderate load (percentage 1-100)
    pub moderate_contention_rate: u32,
    /// Target CPU rate when system is idle (percentage 1-100)
    pub idle_rate: u32,
    /// Threshold for moderate contention (0.0 - 1.0, default 0.25 / 25%)
    pub moderate_threshold: f64,
    /// Threshold for heavy contention (0.0 - 1.0, default 0.60 / 60%)
    pub heavy_threshold: f64,
}

impl Default for ActivityGovernorConfig {
    fn default() -> Self {
        Self {
            heavy_contention_rate: 35,
            moderate_contention_rate: 65,
            idle_rate: 100,
            moderate_threshold: 0.25,
            heavy_threshold: 0.60,
        }
    }
}

#[cfg(windows)]
fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

/// Samples the overall system CPU utilization over a brief measurement window.
pub fn sample_system_cpu_load(sample_duration: Duration) -> Option<f64> {
    #[cfg(windows)]
    unsafe {
        let mut idle1: FILETIME = std::mem::zeroed();
        let mut kernel1: FILETIME = std::mem::zeroed();
        let mut user1: FILETIME = std::mem::zeroed();

        if GetSystemTimes(&mut idle1, &mut kernel1, &mut user1) == 0 {
            return None;
        }

        std::thread::sleep(sample_duration);

        let mut idle2: FILETIME = std::mem::zeroed();
        let mut kernel2: FILETIME = std::mem::zeroed();
        let mut user2: FILETIME = std::mem::zeroed();

        if GetSystemTimes(&mut idle2, &mut kernel2, &mut user2) == 0 {
            return None;
        }

        let idle_delta = filetime_to_u64(idle2).saturating_sub(filetime_to_u64(idle1));
        let kernel_delta = filetime_to_u64(kernel2).saturating_sub(filetime_to_u64(kernel1));
        let user_delta = filetime_to_u64(user2).saturating_sub(filetime_to_u64(user1));

        let total_delta = kernel_delta + user_delta;
        if total_delta == 0 {
            return None;
        }

        let busy_delta = total_delta.saturating_sub(idle_delta);
        let usage = (busy_delta as f64) / (total_delta as f64);
        Some(usage.clamp(0.0, 1.0))
    }
    #[cfg(not(windows))]
    {
        let _ = sample_duration;
        None
    }
}

/// Dynamically evaluates the recommended CPU rate limit based on real-time system contention.
pub fn evaluate_target_cpu_rate(config: &ActivityGovernorConfig) -> u32 {
    if let Some(load) = sample_system_cpu_load(Duration::from_millis(150)) {
        if load >= config.heavy_threshold {
            return config.heavy_contention_rate;
        } else if load >= config.moderate_threshold {
            return config.moderate_contention_rate;
        } else {
            return config.idle_rate;
        }
    }
    config.idle_rate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governor_defaults() {
        let config = ActivityGovernorConfig::default();
        assert_eq!(config.idle_rate, 100);
        assert_eq!(config.moderate_contention_rate, 65);
        assert_eq!(config.heavy_contention_rate, 35);
        assert!(config.moderate_threshold < config.heavy_threshold);
    }

    #[test]
    fn test_cpu_sampling_returns_value() {
        if cfg!(windows) {
            let load = sample_system_cpu_load(Duration::from_millis(50));
            assert!(load.is_some());
            let val = load.unwrap();
            assert!((0.0..=1.0).contains(&val));
        }
    }
}
