use std::process::Command;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct SystemStatsSnapshot {
    pub cpu_usage_pct: f32,
    pub ram_used_gib: f32,
    pub ram_total_gib: f32,
    pub gpu_usage_pct: Option<f32>,
    pub gpu_mem_used_mib: Option<u32>,
    pub gpu_mem_total_mib: Option<u32>,
}

pub struct SystemStatsSampler {
    latest: Arc<RwLock<SystemStatsSnapshot>>,
}

impl SystemStatsSampler {
    pub fn start() -> Self {
        let latest = Arc::new(RwLock::new(SystemStatsSnapshot::default()));
        let latest_bg = Arc::clone(&latest);

        thread::spawn(move || {
            let mut prev_cpu = read_cpu_times();
            loop {
                thread::sleep(Duration::from_millis(900));

                let cpu_usage = match (prev_cpu, read_cpu_times()) {
                    (Some(prev), Some(now)) => {
                        let total_delta = now.total.saturating_sub(prev.total) as f32;
                        let idle_delta = now.idle.saturating_sub(prev.idle) as f32;
                        prev_cpu = Some(now);
                        if total_delta > 0.0 {
                            ((total_delta - idle_delta) / total_delta * 100.0).clamp(0.0, 100.0)
                        } else {
                            0.0
                        }
                    }
                    (_, next) => {
                        prev_cpu = next;
                        0.0
                    }
                };

                let (ram_used_gib, ram_total_gib) = read_ram_gib().unwrap_or((0.0, 0.0));
                let (gpu_usage_pct, gpu_mem_used_mib, gpu_mem_total_mib) =
                    query_nvidia_smi().unwrap_or((None, None, None));

                let snapshot = SystemStatsSnapshot {
                    cpu_usage_pct: cpu_usage,
                    ram_used_gib,
                    ram_total_gib,
                    gpu_usage_pct,
                    gpu_mem_used_mib,
                    gpu_mem_total_mib,
                };

                if let Ok(mut guard) = latest_bg.write() {
                    *guard = snapshot;
                }
            }
        });

        Self { latest }
    }

    pub fn snapshot(&self) -> SystemStatsSnapshot {
        self.latest.read().map(|s| s.clone()).unwrap_or_default()
    }
}

#[derive(Clone, Copy)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

fn read_cpu_times() -> Option<CpuTimes> {
    let content = std::fs::read_to_string("/proc/stat").ok()?;
    let mut it = content.lines().next()?.split_whitespace();
    if it.next()? != "cpu" {
        return None;
    }
    let user = it.next()?.parse::<u64>().ok()?;
    let nice = it.next()?.parse::<u64>().ok()?;
    let system = it.next()?.parse::<u64>().ok()?;
    let idle = it.next()?.parse::<u64>().ok()?;
    let iowait = it.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let irq = it.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let softirq = it.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let steal = it.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let idle_all = idle + iowait;
    let total = user + nice + system + idle + iowait + irq + softirq + steal;
    Some(CpuTimes {
        total,
        idle: idle_all,
    })
}

fn read_ram_gib() -> Option<(f32, f32)> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kib = None;
    let mut available_kib = None;
    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            total_kib = line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<f32>().ok());
        } else if line.starts_with("MemAvailable:") {
            available_kib = line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<f32>().ok());
        }
    }
    let total = total_kib?;
    let available = available_kib.unwrap_or(0.0);
    let used = (total - available).max(0.0);
    Some((used / 1024.0 / 1024.0, total / 1024.0 / 1024.0))
}

fn query_nvidia_smi() -> Option<(Option<f32>, Option<u32>, Option<u32>)> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let out = String::from_utf8_lossy(&output.stdout);
    let line = out.lines().next()?.trim();
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 3 {
        return None;
    }
    let util = parts[0].parse::<f32>().ok();
    let mem_used = parts[1].parse::<u32>().ok();
    let mem_total = parts[2].parse::<u32>().ok();
    Some((util, mem_used, mem_total))
}
