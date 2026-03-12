use serverbee_common::types::GpuReport;

/// Collect GPU metrics. Returns None if no GPU is detected or NVML is unavailable.
pub fn get_gpu_report() -> Option<GpuReport> {
    #[cfg(feature = "gpu")]
    {
        get_gpu_report_nvml()
    }

    #[cfg(not(feature = "gpu"))]
    {
        None
    }
}

#[cfg(feature = "gpu")]
fn get_gpu_report_nvml() -> Option<GpuReport> {
    use nvml_wrapper::Nvml;
    use serverbee_common::types::GpuInfo;

    let nvml = match Nvml::init() {
        Ok(n) => n,
        Err(e) => {
            tracing::debug!("NVML init failed (no NVIDIA GPU?): {e}");
            return None;
        }
    };

    let count = match nvml.device_count() {
        Ok(c) if c > 0 => c,
        _ => return None,
    };

    let mut gpus = Vec::new();
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
        return None;
    }

    let avg_usage = total_usage / gpus.len() as f64;

    Some(GpuReport {
        count: gpus.len() as i32,
        average_usage: avg_usage,
        detailed_info: gpus,
    })
}
