//! Total CPU usage, sampled between two calls.

use crate::HostError;

pub struct CpuMeter {
    last: Option<(u64, u64)>,
}

impl Default for CpuMeter {
    fn default() -> Self {
        CpuMeter::new()
    }
}

impl CpuMeter {
    pub fn new() -> CpuMeter {
        let mut m = CpuMeter { last: None };
        let _ = m.usage();
        m
    }

    /// Busy fraction (0.0..=1.0) since the previous call.
    pub fn usage(&mut self) -> Result<f32, HostError> {
        let (busy, total) = imp::times()?;
        let r = match self.last {
            Some((b0, t0)) if total > t0 => (busy.saturating_sub(b0)) as f32 / (total - t0) as f32,
            _ => 0.0,
        };
        self.last = Some((busy, total));
        Ok(r.clamp(0.0, 1.0))
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    pub fn times() -> Result<(u64, u64), HostError> {
        let stat = std::fs::read_to_string("/proc/stat").map_err(|e| HostError::Other(format!("/proc/stat: {e}")))?;
        let line = stat.lines().next().unwrap_or_default();
        let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
        if v.len() < 4 {
            return Err(HostError::Unsupported("unexpected /proc/stat"));
        }
        let total: u64 = v.iter().take(8).sum();
        let idle = v[3] + v.get(4).copied().unwrap_or(0);
        Ok((total - idle, total))
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;
    pub fn times() -> Result<(u64, u64), HostError> {
        let z = || FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut idle, mut kernel, mut user) = (z(), z(), z());
        // SAFETY: GetSystemTimes writes three FILETIME structs we own.
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return Err(HostError::Unsupported("GetSystemTimes failed"));
        }
        let v = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
        // kernel time includes idle time
        let total = v(kernel) + v(user);
        Ok((total - v(idle), total))
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::*;
    pub fn times() -> Result<(u64, u64), HostError> {
        Err(HostError::Unsupported("CPU usage"))
    }
}
