use crate::ui::ColorTheme;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(windows)]
const BYTES_PER_MB: u64 = 1024 * 1024;
const BYTES_PER_GB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceTier {
    Light,
    Standard,
    Heavy,
}

impl std::fmt::Display for ResourceTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceTier::Light => write!(f, "light"),
            ResourceTier::Standard => write!(f, "standard"),
            ResourceTier::Heavy => write!(f, "heavy"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostProbe {
    pub os: String,
    pub arch: String,
    pub logical_cores: usize,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub swap_total_mb: u64,
    pub swap_available_mb: u64,
    pub disk_path: String,
    pub disk_free_gb: f64,
    pub tier: ResourceTier,
    pub recommended_build_jobs: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemorySnapshot {
    total_ram_mb: u64,
    available_ram_mb: u64,
    swap_total_mb: u64,
    swap_available_mb: u64,
}

/// Classify a host into `light`, `standard`, or `heavy` based on CPU, RAM, and swap.
pub fn classify_tier(logical_cores: usize, total_ram_mb: u64, swap_total_mb: u64) -> ResourceTier {
    if logical_cores <= 4 || (total_ram_mb < 10_240 && swap_total_mb == 0) {
        ResourceTier::Light
    } else if logical_cores >= 12 && total_ram_mb >= 24_576 {
        ResourceTier::Heavy
    } else {
        ResourceTier::Standard
    }
}

/// Compute a conservative recommended parallel build job count (`-j`).
pub fn recommend_build_jobs(logical_cores: usize, total_ram_mb: u64, tier: ResourceTier) -> usize {
    let cores = logical_cores.max(1);
    let mem_cap = (total_ram_mb / 2048).max(1) as usize;
    let core_target = match tier {
        ResourceTier::Light => (cores / 2).clamp(1, 2),
        ResourceTier::Standard => cores.saturating_sub(2).max(2).min(cores),
        ResourceTier::Heavy => cores.saturating_sub(2).max(4).min(cores),
    };
    core_target.min(mem_cap).max(1)
}

/// Collect a complete cross-platform host resource profile.
pub fn probe_host(path: Option<&Path>) -> Result<HostProbe> {
    let logical_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mem = query_memory()?;
    let (disk_path, disk_free_gb) = query_disk_free_gb(path)?;
    let tier = classify_tier(logical_cores, mem.total_ram_mb, mem.swap_total_mb);
    let recommended_build_jobs = recommend_build_jobs(logical_cores, mem.total_ram_mb, tier);

    Ok(HostProbe {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        logical_cores,
        total_ram_mb: mem.total_ram_mb,
        available_ram_mb: mem.available_ram_mb,
        swap_total_mb: mem.swap_total_mb,
        swap_available_mb: mem.swap_available_mb,
        disk_path,
        disk_free_gb,
        tier,
        recommended_build_jobs,
    })
}

/// Evaluate preflight thresholds against a probed host profile.
/// Returns a list of human-readable violation descriptions (empty when all checks pass).
pub fn evaluate_preflight(
    probe: &HostProbe,
    min_cores: Option<usize>,
    min_ram_mb: Option<u64>,
) -> Vec<String> {
    let mut violations = Vec::new();
    if let Some(req_cores) = min_cores
        && probe.logical_cores < req_cores
    {
        violations.push(format!(
            "logical cores {} < required {}",
            probe.logical_cores, req_cores
        ));
    }
    if let Some(req_ram) = min_ram_mb
        && probe.total_ram_mb < req_ram
    {
        violations.push(format!(
            "total RAM {} MB < required {} MB",
            probe.total_ram_mb, req_ram
        ));
    }
    violations
}

/// Format a concise human-readable summary of the host resource profile.
pub fn format_probe_summary(probe: &HostProbe, theme: &ColorTheme) -> String {
    let tier_label = match probe.tier {
        ResourceTier::Light => theme.bold_yellow("light"),
        ResourceTier::Standard => theme.bold_cyan("standard"),
        ResourceTier::Heavy => theme.bold_green("heavy"),
    };

    let swap_desc = if probe.swap_total_mb == 0 {
        "none (0 MB)".to_string()
    } else {
        format!(
            "{} MB total ({} MB available)",
            probe.swap_total_mb, probe.swap_available_mb
        )
    };

    format!(
        "{} Host Resource Profile ({}/{})\n\
         \x20 Tier:             {}\n\
         \x20 Logical Cores:    {}\n\
         \x20 Memory (RAM):     {} MB total ({} MB available)\n\
         \x20 Swap:             {}\n\
         \x20 Disk Free:        {:.2} GB ({})\n\
         \x20 Recommended Jobs: {}",
        theme.badge_bque(),
        probe.os,
        probe.arch,
        tier_label,
        probe.logical_cores,
        probe.total_ram_mb,
        probe.available_ram_mb,
        swap_desc,
        probe.disk_free_gb,
        probe.disk_path,
        probe.recommended_build_jobs,
    )
}

#[cfg(any(target_os = "linux", test))]
fn parse_proc_meminfo(content: &str) -> Result<MemorySnapshot> {
    let mut mem_total_kb: Option<u64> = None;
    let mut mem_available_kb: Option<u64> = None;
    let mut mem_free_kb: Option<u64> = None;
    let mut buffers_kb: Option<u64> = None;
    let mut cached_kb: Option<u64> = None;
    let mut swap_total_kb: Option<u64> = None;
    let mut swap_free_kb: Option<u64> = None;

    for line in content.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let value_kb = rest
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<u64>().ok());
        let Some(kb) = value_kb else {
            continue;
        };
        match key.trim() {
            "MemTotal" => mem_total_kb = Some(kb),
            "MemAvailable" => mem_available_kb = Some(kb),
            "MemFree" => mem_free_kb = Some(kb),
            "Buffers" => buffers_kb = Some(kb),
            "Cached" => cached_kb = Some(kb),
            "SwapTotal" => swap_total_kb = Some(kb),
            "SwapFree" => swap_free_kb = Some(kb),
            _ => {}
        }
    }

    let total_kb = mem_total_kb.ok_or_else(|| anyhow!("MemTotal not found in /proc/meminfo"))?;
    let available_kb = mem_available_kb.unwrap_or_else(|| {
        mem_free_kb.unwrap_or(0) + buffers_kb.unwrap_or(0) + cached_kb.unwrap_or(0)
    });
    let swap_total = swap_total_kb.unwrap_or(0);
    let swap_free = swap_free_kb.unwrap_or(0).min(swap_total);

    Ok(MemorySnapshot {
        total_ram_mb: total_kb / 1024,
        available_ram_mb: available_kb / 1024,
        swap_total_mb: swap_total / 1024,
        swap_available_mb: swap_free / 1024,
    })
}

#[cfg(target_os = "linux")]
fn query_memory() -> Result<MemorySnapshot> {
    let content =
        std::fs::read_to_string("/proc/meminfo").context("Failed to read /proc/meminfo")?;
    parse_proc_meminfo(&content)
}

#[cfg(windows)]
fn query_memory() -> Result<MemorySnapshot> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;

    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok == 0 {
        return Err(anyhow!(
            "GlobalMemoryStatusEx failed (Win32 error: {})",
            unsafe { GetLastError() }
        ));
    }

    let total_ram_mb = status.ullTotalPhys / BYTES_PER_MB;
    let available_ram_mb = status.ullAvailPhys / BYTES_PER_MB;
    let swap_total_bytes = status.ullTotalPageFile.saturating_sub(status.ullTotalPhys);
    let swap_avail_bytes = status
        .ullAvailPageFile
        .saturating_sub(status.ullAvailPhys)
        .min(swap_total_bytes);

    Ok(MemorySnapshot {
        total_ram_mb,
        available_ram_mb,
        swap_total_mb: swap_total_bytes / BYTES_PER_MB,
        swap_available_mb: swap_avail_bytes / BYTES_PER_MB,
    })
}

#[cfg(all(not(target_os = "linux"), not(windows)))]
fn query_memory() -> Result<MemorySnapshot> {
    Ok(MemorySnapshot {
        total_ram_mb: 0,
        available_ram_mb: 0,
        swap_total_mb: 0,
        swap_available_mb: 0,
    })
}

fn resolve_probe_dir(path: Option<&Path>) -> Result<(PathBuf, String)> {
    let raw = match path {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir().context("Failed to determine current working directory")?,
    };

    if !raw.exists() {
        return Err(anyhow!("Probe path does not exist: {}", raw.display()));
    }

    let canonical = std::fs::canonicalize(&raw).unwrap_or(raw);
    let query_dir = if canonical.is_file() {
        canonical
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| canonical.clone())
    } else {
        canonical
    };

    let display = strip_windows_verbatim_prefix(&query_dir.to_string_lossy());
    Ok((query_dir, display))
}

fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        path.to_string()
    }
}

fn round_gb(bytes: u128) -> f64 {
    let gb = bytes as f64 / BYTES_PER_GB;
    (gb * 100.0).round() / 100.0
}

#[cfg(unix)]
fn query_disk_free_gb(path: Option<&Path>) -> Result<(String, f64)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let (query_dir, display) = resolve_probe_dir(path)?;
    let c_path = CString::new(query_dir.as_os_str().as_bytes())
        .with_context(|| format!("Path contains interior NUL byte: {}", display))?;

    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let res = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if res != 0 {
        return Err(anyhow!(
            "statvfs failed for '{}': {}",
            display,
            std::io::Error::last_os_error()
        ));
    }

    #[allow(clippy::unnecessary_cast)]
    let block_size = if stat.f_frsize > 0 {
        stat.f_frsize as u128
    } else {
        stat.f_bsize as u128
    };
    #[allow(clippy::unnecessary_cast)]
    let free_bytes = (stat.f_bavail as u128).saturating_mul(block_size);

    Ok((display, round_gb(free_bytes)))
}

#[cfg(windows)]
fn query_disk_free_gb(path: Option<&Path>) -> Result<(String, f64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let (query_dir, display) = resolve_probe_dir(path)?;
    let wide: Vec<u16> = query_dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_bytes_available: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free_bytes: u64 = 0;

    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_bytes_available,
            &mut total_bytes,
            &mut total_free_bytes,
        )
    };
    if ok == 0 {
        return Err(anyhow!(
            "GetDiskFreeSpaceExW failed for '{}' (Win32 error: {})",
            display,
            unsafe { GetLastError() }
        ));
    }

    Ok((display, round_gb(u128::from(free_bytes_available))))
}

#[cfg(all(not(unix), not(windows)))]
fn query_disk_free_gb(path: Option<&Path>) -> Result<(String, f64)> {
    let (_, display) = resolve_probe_dir(path)?;
    Ok((display, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_tier_light_on_low_cores() {
        assert_eq!(classify_tier(2, 16_384, 4_096), ResourceTier::Light);
        assert_eq!(classify_tier(4, 32_768, 8_192), ResourceTier::Light);
    }

    #[test]
    fn test_classify_tier_light_on_low_ram_without_swap() {
        assert_eq!(classify_tier(8, 8_192, 0), ResourceTier::Light);
        assert_eq!(classify_tier(8, 10_239, 0), ResourceTier::Light);
        // With swap present and >4 cores, it moves up to standard.
        assert_eq!(classify_tier(8, 8_192, 2_048), ResourceTier::Standard);
    }

    #[test]
    fn test_classify_tier_standard_and_heavy() {
        assert_eq!(classify_tier(8, 16_384, 0), ResourceTier::Standard);
        assert_eq!(classify_tier(12, 16_384, 0), ResourceTier::Standard);
        assert_eq!(classify_tier(10, 32_768, 0), ResourceTier::Standard);
        assert_eq!(classify_tier(12, 24_576, 0), ResourceTier::Heavy);
        assert_eq!(classify_tier(24, 65_536, 8_192), ResourceTier::Heavy);
    }

    #[test]
    fn test_recommend_build_jobs() {
        assert_eq!(recommend_build_jobs(2, 4_096, ResourceTier::Light), 1);
        assert_eq!(recommend_build_jobs(4, 8_192, ResourceTier::Light), 2);
        assert_eq!(recommend_build_jobs(8, 16_384, ResourceTier::Standard), 6);
        // Low RAM caps jobs below core count.
        assert_eq!(recommend_build_jobs(8, 4_096, ResourceTier::Standard), 2);
        assert_eq!(recommend_build_jobs(16, 32_768, ResourceTier::Heavy), 14);
    }

    #[test]
    fn test_parse_proc_meminfo() {
        let sample = "\
MemTotal:       16384000 kB
MemFree:         2048000 kB
MemAvailable:    8192000 kB
Buffers:          512000 kB
Cached:          4096000 kB
SwapTotal:       4096000 kB
SwapFree:        3072000 kB
";
        let snap = parse_proc_meminfo(sample).expect("parse meminfo");
        assert_eq!(snap.total_ram_mb, 16000);
        assert_eq!(snap.available_ram_mb, 8000);
        assert_eq!(snap.swap_total_mb, 4000);
        assert_eq!(snap.swap_available_mb, 3000);
    }

    #[test]
    fn test_parse_proc_meminfo_fallback_without_available() {
        let sample = "\
MemTotal:        8192000 kB
MemFree:         1024000 kB
Buffers:         1024000 kB
Cached:          2048000 kB
SwapTotal:             0 kB
SwapFree:              0 kB
";
        let snap = parse_proc_meminfo(sample).expect("parse meminfo");
        assert_eq!(snap.total_ram_mb, 8000);
        assert_eq!(snap.available_ram_mb, 4000);
        assert_eq!(snap.swap_total_mb, 0);
        assert_eq!(snap.swap_available_mb, 0);
    }

    #[test]
    fn test_evaluate_preflight() {
        let probe = HostProbe {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            logical_cores: 4,
            total_ram_mb: 8_192,
            available_ram_mb: 6_000,
            swap_total_mb: 0,
            swap_available_mb: 0,
            disk_path: "/tmp".to_string(),
            disk_free_gb: 25.5,
            tier: ResourceTier::Light,
            recommended_build_jobs: 2,
        };

        assert!(evaluate_preflight(&probe, None, None).is_empty());
        assert!(evaluate_preflight(&probe, Some(4), Some(8_192)).is_empty());

        let violations = evaluate_preflight(&probe, Some(8), Some(16_384));
        assert_eq!(violations.len(), 2);
        assert!(violations[0].contains("logical cores 4 < required 8"));
        assert!(violations[1].contains("total RAM 8192 MB < required 16384 MB"));
    }

    #[test]
    fn test_strip_windows_verbatim_prefix() {
        assert_eq!(strip_windows_verbatim_prefix(r"\\?\C:\work"), r"C:\work");
        assert_eq!(strip_windows_verbatim_prefix("/tmp/repo"), "/tmp/repo");
    }

    #[test]
    fn test_probe_host_live() {
        let probe = probe_host(None).expect("live host probe");
        assert!(probe.logical_cores >= 1);
        assert!(probe.total_ram_mb > 0);
        assert!(probe.recommended_build_jobs >= 1);
        assert!(!probe.disk_path.is_empty());
    }
}
