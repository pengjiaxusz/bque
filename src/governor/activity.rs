use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Foundation::FILETIME;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::GetSystemTimes;

/// Configuration for the system activity and CPU contention governor.
#[derive(Debug, Clone)]
pub struct ActivityGovernorConfig {
    /// Target CPU rate during heavy external contention (percentage 1-100)
    pub heavy_contention_rate: u32,
    /// Target CPU rate during moderate external load (percentage 1-100)
    pub moderate_contention_rate: u32,
    /// Target CPU rate when external system load is idle (percentage 1-100)
    pub idle_rate: u32,
    /// Threshold for moderate external contention (0.0 - 1.0, default 0.25 / 25%)
    pub moderate_threshold: f64,
    /// Threshold for heavy external contention (0.0 - 1.0, default 0.60 / 60%)
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

#[cfg(target_os = "linux")]
fn read_linux_cpu_times() -> Option<(u64, u64)> {
    let content = std::fs::read_to_string("/proc/stat").ok()?;
    let cpu_line = content.lines().next()?;
    let mut parts = cpu_line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }

    let values: Vec<u64> = parts.filter_map(|v| v.parse::<u64>().ok()).collect();
    if values.len() < 4 {
        return None;
    }

    let idle = values[3].saturating_add(values.get(4).copied().unwrap_or(0));
    let total: u64 = values.iter().copied().sum();
    Some((idle, total))
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

    #[cfg(target_os = "linux")]
    {
        let (idle1, total1) = read_linux_cpu_times()?;
        std::thread::sleep(sample_duration);
        let (idle2, total2) = read_linux_cpu_times()?;

        let total_delta = total2.saturating_sub(total1);
        if total_delta == 0 {
            return None;
        }
        let idle_delta = idle2.saturating_sub(idle1);
        let busy_delta = total_delta.saturating_sub(idle_delta);
        let usage = (busy_delta as f64) / (total_delta as f64);
        Some(usage.clamp(0.0, 1.0))
    }

    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        let _ = sample_duration;
        None
    }
}

/// Evaluate the recommended host CPU budget rate given total host load and the
/// CPU fraction already consumed by active managed bque tasks.
pub fn compute_rate_from_external_load(
    config: &ActivityGovernorConfig,
    total_system_load: f64,
    bque_cpu_pct: f64,
) -> u32 {
    let bque_load = (bque_cpu_pct / 100.0).clamp(0.0, 1.0);
    let external_load = (total_system_load - bque_load).clamp(0.0, 1.0);
    if external_load >= config.heavy_threshold {
        config.heavy_contention_rate
    } else if external_load >= config.moderate_threshold {
        config.moderate_contention_rate
    } else {
        config.idle_rate
    }
}

/// Split `host_rate` proportionally when oversubscription (`total_active_weight > 100`)
/// is in effect.
pub fn apply_oversubscription_share(
    host_rate: u32,
    my_weight: u32,
    total_active_weight: u32,
) -> u32 {
    if total_active_weight <= crate::state::BUDGET_TOTAL || total_active_weight == 0 {
        return host_rate.clamp(1, 100);
    }
    let scaled = (host_rate as u64)
        .saturating_mul(my_weight as u64)
        .saturating_div(total_active_weight as u64) as u32;
    scaled.clamp(10, 100)
}

/// Dynamically evaluates the recommended CPU rate limit based on real-time
/// external system contention (excluding `bque_cpu_pct`).
pub fn evaluate_target_cpu_rate_excluding(
    config: &ActivityGovernorConfig,
    bque_cpu_pct: f64,
) -> u32 {
    if let Some(load) = sample_system_cpu_load(Duration::from_millis(150)) {
        return compute_rate_from_external_load(config, load, bque_cpu_pct);
    }
    config.idle_rate
}

pub fn evaluate_target_cpu_rate(config: &ActivityGovernorConfig) -> u32 {
    evaluate_target_cpu_rate_excluding(config, 0.0)
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
    fn test_external_load_subtracts_bque_self_cpu() {
        let config = ActivityGovernorConfig::default();
        // Host is at 85% CPU, all 85% from bque itself -> external is 0% -> stay at 100%.
        assert_eq!(compute_rate_from_external_load(&config, 0.85, 85.0), 100);
        // Host is at 85% CPU, 20% from bque -> external is 65% -> throttle to 35%.
        assert_eq!(compute_rate_from_external_load(&config, 0.85, 20.0), 35);
    }

    #[test]
    fn test_oversubscription_splits_cpu_rate_proportionally() {
        assert_eq!(apply_oversubscription_share(100, 100, 100), 100);
        assert_eq!(apply_oversubscription_share(100, 100, 200), 50);
        assert_eq!(apply_oversubscription_share(65, 100, 200), 32);
    }

    #[test]
    fn test_cpu_sampling_returns_value() {
        if cfg!(any(windows, target_os = "linux")) {
            let load = sample_system_cpu_load(Duration::from_millis(50));
            assert!(load.is_some());
            let val = load.unwrap();
            assert!((0.0..=1.0).contains(&val));
        }
    }
}
