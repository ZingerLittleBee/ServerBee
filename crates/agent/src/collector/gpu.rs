#[cfg(feature = "gpu")]
use std::time::{Duration, Instant};

use serverbee_common::types::GpuReport;

#[cfg(feature = "gpu")]
const NVML_RETRY_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(crate) struct GpuCollector {
    #[cfg(feature = "gpu")]
    nvml: Option<nvml_wrapper::Nvml>,
    #[cfg(feature = "gpu")]
    last_init_attempt: Option<Instant>,
}

impl GpuCollector {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn collect(&mut self) -> Option<GpuReport> {
        #[cfg(feature = "gpu")]
        {
            self.collect_nvml()
        }

        #[cfg(not(feature = "gpu"))]
        {
            None
        }
    }

    #[cfg(feature = "gpu")]
    fn collect_nvml(&mut self) -> Option<GpuReport> {
        use nvml_wrapper::Nvml;

        if self.nvml.is_none() {
            if self
                .last_init_attempt
                .is_some_and(|t| t.elapsed() < NVML_RETRY_INTERVAL)
            {
                return None;
            }
            self.last_init_attempt = Some(Instant::now());
            match Nvml::init() {
                Ok(n) => self.nvml = Some(n),
                Err(e) => {
                    tracing::debug!("NVML init failed (no NVIDIA GPU?): {e}");
                    return None;
                }
            }
        }

        let nvml = self.nvml.as_ref()?;
        match read_nvml_report(nvml) {
            Ok(report) => report,
            Err(_) => {
                self.nvml = None;
                None
            }
        }
    }
}

#[cfg(test)]
pub fn get_gpu_report() -> Option<GpuReport> {
    let mut collector = GpuCollector::new();
    collector.collect()
}

#[cfg(feature = "gpu")]
fn read_nvml_report(
    nvml: &nvml_wrapper::Nvml,
) -> Result<Option<GpuReport>, nvml_wrapper::error::NvmlError> {
    use serverbee_common::types::GpuInfo;

    let count = nvml.device_count()?;
    if count == 0 {
        return Ok(None);
    }

    let mut gpus = Vec::with_capacity(count as usize);
    let mut total_usage = 0.0;

    for i in 0..count {
        let device = match nvml.device_by_index(i) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let name = device.name().unwrap_or_else(|_| format!("GPU {i}"));
        let mem_info = device.memory_info().ok();
        let utilization = device.utilization_rates().ok();
        let temperature = device
            .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .ok();

        let mem_total = mem_info.as_ref().map(|m| m.total as i64).unwrap_or(0);
        let mem_used = mem_info.as_ref().map(|m| m.used as i64).unwrap_or(0);
        let util = utilization.map(|u| u.gpu as f64).unwrap_or(0.0);
        let temp = temperature.map(|t| t as f64).unwrap_or(0.0);

        total_usage += util;

        gpus.push(GpuInfo {
            name,
            mem_total,
            mem_used,
            utilization: util,
            temperature: temp,
        });
    }

    if gpus.is_empty() {
        return Ok(None);
    }

    let avg_usage = total_usage / gpus.len() as f64;

    Ok(Some(GpuReport {
        count: gpus.len() as i32,
        average_usage: avg_usage,
        detailed_info: gpus,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn test_get_gpu_report_is_none_without_feature() {
        // Without the `gpu` cargo feature the collector is compiled to always
        // return None, which is the case on CI hosts that have no NVIDIA GPU.
        assert!(get_gpu_report().is_none());
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn test_get_gpu_report_no_panic_and_consistent() {
        // With the feature enabled but (typically) no GPU/NVML available, the
        // call must not panic. If a report is produced, validate invariants.
        match get_gpu_report() {
            None => {}
            Some(report) => {
                assert_eq!(report.count as usize, report.detailed_info.len());
                assert!(report.count > 0);
                assert!(report.average_usage >= 0.0);
                assert!(report.average_usage.is_finite());
                for gpu in &report.detailed_info {
                    assert!(gpu.mem_used <= gpu.mem_total.max(gpu.mem_used));
                    assert!(gpu.utilization >= 0.0);
                }
            }
        }
    }
}
