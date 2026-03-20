//! System-level metrics using sysinfo

use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

#[cfg(feature = "system-metrics")]
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

#[allow(dead_code)]
pub struct SystemMetrics {
    pub cpu_usage_percent: Gauge,
    pub memory_used_bytes: Gauge,
    pub memory_total_bytes: Gauge,
    pub disk_used_bytes: Gauge,
    pub disk_total_bytes: Gauge,

    #[cfg(feature = "system-metrics")]
    sys: System,
    #[cfg(feature = "system-metrics")]
    last_cpu_refresh: std::time::Instant,
}

#[allow(dead_code)]
impl SystemMetrics {
    #[cfg(feature = "system-metrics")]
    pub fn new(registry: &mut Registry) -> Self {
        let cpu_usage_percent = Gauge::default();
        registry.register(
            "pgqt_system_cpu_usage_percent",
            "CPU usage percentage (0-100)",
            cpu_usage_percent.clone(),
        );

        let memory_used_bytes = Gauge::default();
        registry.register(
            "pgqt_system_memory_used_bytes",
            "Memory used in bytes",
            memory_used_bytes.clone(),
        );

        let memory_total_bytes = Gauge::default();
        registry.register(
            "pgqt_system_memory_total_bytes",
            "Total memory in bytes",
            memory_total_bytes.clone(),
        );

        let disk_used_bytes = Gauge::default();
        registry.register(
            "pgqt_system_disk_used_bytes",
            "Disk space used by database in bytes",
            disk_used_bytes.clone(),
        );

        let disk_total_bytes = Gauge::default();
        registry.register(
            "pgqt_system_disk_total_bytes",
            "Total disk space in bytes",
            disk_total_bytes.clone(),
        );

        #[cfg(feature = "system-metrics")]
        let sys = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );

        Self {
            cpu_usage_percent,
            memory_used_bytes,
            memory_total_bytes,
            disk_used_bytes,
            disk_total_bytes,
            #[cfg(feature = "system-metrics")]
            sys,
            #[cfg(feature = "system-metrics")]
            last_cpu_refresh: std::time::Instant::now(),
        }
    }

    /// Refresh system metrics
    /// Call this periodically (e.g., every 15 seconds)
    #[cfg(feature = "system-metrics")]
    pub fn refresh(&mut self, db_path: &str) {
        // Refresh CPU - only every 200ms minimum for accurate readings
        if self.last_cpu_refresh.elapsed().as_millis() > 200 {
            self.sys.refresh_cpu();
            self.last_cpu_refresh = std::time::Instant::now();
        }

        // Calculate average CPU usage across all cores
        let cpu_usage: f32 = self.sys.cpus().iter().map(|c| c.cpu_usage()).sum();
        let avg_cpu = cpu_usage / self.sys.cpus().len() as f32;
        self.cpu_usage_percent
            .set(((avg_cpu * 1_000_000.0) as i64).into()); // Store as fixed-point

        // Refresh memory
        self.sys.refresh_memory();
        self.memory_used_bytes.set(self.sys.used_memory() as i64);
        self.memory_total_bytes.set(self.sys.total_memory() as i64);

        // Disk usage for database file
        if let Ok(metadata) = std::fs::metadata(db_path) {
            self.disk_used_bytes.set(metadata.len() as i64);
        }

        // Total disk space (approximation using available + used)
        let total_disk = self.sys.used_memory() + self.sys.available_memory(); // Rough approximation
        self.disk_total_bytes.set(total_disk as i64);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_metrics_new() {
        let mut registry = Registry::default();
        let _metrics = SystemMetrics::new(&mut registry);
    }
}
