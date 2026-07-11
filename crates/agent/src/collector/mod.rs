mod cpu;
mod disk;
mod disk_io;
mod gpu;
mod load;
mod memory;
mod network;
mod process;
mod source;
mod temperature;
pub mod virtualization;

use std::time::Instant;

use serverbee_common::types::{SystemInfo, SystemReport};

use source::{MetricsSource, SysinfoSource};

/// Report assembly over a [`MetricsSource`]: rate differencing for the
/// cumulative network counters, the elapsed-window guard, and the
/// temperature/GPU enable gating live here — the source only reads the host.
/// Production code uses the default `SysinfoSource`; tests inject a fake to
/// drive every assembly path with arbitrary readings.
pub struct Collector<S: MetricsSource = SysinfoSource> {
    source: S,
    prev_net_in: u64,
    prev_net_out: u64,
    prev_time: Instant,
    enable_temperature: bool,
    enable_gpu: bool,
}

impl Collector<SysinfoSource> {
    pub fn new(enable_temperature: bool, enable_gpu: bool) -> Self {
        Self::with_source(SysinfoSource::new(), enable_temperature, enable_gpu)
    }

    pub fn system_info(&self) -> SystemInfo {
        self.source.system_info()
    }
}

impl<S: MetricsSource> Collector<S> {
    fn with_source(source: S, enable_temperature: bool, enable_gpu: bool) -> Self {
        let (net_in, net_out) = source.net_total_bytes();
        Self {
            source,
            prev_net_in: net_in,
            prev_net_out: net_out,
            prev_time: Instant::now(),
            enable_temperature,
            enable_gpu,
        }
    }

    pub fn collect(&mut self) -> SystemReport {
        self.source.refresh();
        let elapsed = self.prev_time.elapsed().as_secs_f64();
        self.prev_time = Instant::now();
        self.assemble_report(elapsed)
    }

    /// Fold one refreshed sample into a report. Split from [`Self::collect`]
    /// so tests control the elapsed window directly.
    fn assemble_report(&mut self, elapsed: f64) -> SystemReport {
        // Guard the rate denominator: a sub-second (or clock-skewed) window
        // must not inflate speeds or divide by zero.
        let elapsed = elapsed.max(1.0);

        // Cumulative counters difference into per-second rates; saturating_sub
        // absorbs counter resets (interface re-enumeration, rollover) as a
        // zero-speed sample instead of a negative one.
        let (net_in, net_out) = self.source.net_total_bytes();
        let net_in_speed = (net_in.saturating_sub(self.prev_net_in) as f64 / elapsed) as i64;
        let net_out_speed = (net_out.saturating_sub(self.prev_net_out) as f64 / elapsed) as i64;

        self.prev_net_in = net_in;
        self.prev_net_out = net_out;

        let disk_io = self.source.disk_io(elapsed);

        let temperature = if self.enable_temperature {
            self.source.temperature()
        } else {
            None
        };

        let (load1, load5, load15) = self.source.load_averages();

        SystemReport {
            cpu: self.source.cpu_usage(),
            mem_used: self.source.mem_used(),
            swap_used: self.source.swap_used(),
            disk_used: self.source.disk_used(),
            net_in_speed,
            net_out_speed,
            net_in_transfer: net_in as i64,
            net_out_transfer: net_out as i64,
            load1,
            load5,
            load15,
            tcp_conn: self.source.tcp_connections(),
            udp_conn: self.source.udp_connections(),
            process_count: self.source.process_count(),
            uptime: self.source.uptime(),
            disk_io,
            temperature,
            gpu: if self.enable_gpu {
                self.source.gpu()
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod assembly_tests {
    use super::source::MetricsSource;
    use super::*;
    use serverbee_common::types::{DiskIo, GpuInfo, GpuReport};

    /// Test adapter: every reading is a plain field, so assembly paths can be
    /// driven with arbitrary host states.
    struct FakeSource {
        cpu: f64,
        mem_used: i64,
        swap_used: i64,
        disk_used: i64,
        net_totals: (u64, u64),
        loads: (f64, f64, f64),
        tcp: i32,
        udp: i32,
        processes: i32,
        uptime: u64,
        disk_io: Option<Vec<DiskIo>>,
        temperature: Option<f64>,
        gpu: Option<GpuReport>,
        refreshes: u32,
    }

    impl Default for FakeSource {
        fn default() -> Self {
            Self {
                cpu: 12.5,
                mem_used: 1024,
                swap_used: 256,
                disk_used: 4096,
                net_totals: (10_000, 20_000),
                loads: (0.5, 0.4, 0.3),
                tcp: 7,
                udp: 3,
                processes: 42,
                uptime: 3600,
                disk_io: None,
                temperature: Some(55.0),
                gpu: Some(GpuReport {
                    count: 1,
                    average_usage: 30.0,
                    detailed_info: vec![GpuInfo {
                        name: "FakeGPU".to_string(),
                        mem_total: 8000,
                        mem_used: 2000,
                        utilization: 30.0,
                        temperature: 60.0,
                    }],
                }),
                refreshes: 0,
            }
        }
    }

    impl MetricsSource for FakeSource {
        fn refresh(&mut self) {
            self.refreshes += 1;
        }
        fn cpu_usage(&self) -> f64 {
            self.cpu
        }
        fn mem_used(&self) -> i64 {
            self.mem_used
        }
        fn swap_used(&self) -> i64 {
            self.swap_used
        }
        fn disk_used(&self) -> i64 {
            self.disk_used
        }
        fn net_total_bytes(&self) -> (u64, u64) {
            self.net_totals
        }
        fn load_averages(&self) -> (f64, f64, f64) {
            self.loads
        }
        fn tcp_connections(&self) -> i32 {
            self.tcp
        }
        fn udp_connections(&self) -> i32 {
            self.udp
        }
        fn process_count(&self) -> i32 {
            self.processes
        }
        fn uptime(&self) -> u64 {
            self.uptime
        }
        fn disk_io(&mut self, _elapsed: f64) -> Option<Vec<DiskIo>> {
            self.disk_io.clone()
        }
        fn temperature(&self) -> Option<f64> {
            self.temperature
        }
        fn gpu(&self) -> Option<GpuReport> {
            self.gpu.clone()
        }
    }

    fn make_collector(source: FakeSource) -> Collector<FakeSource> {
        Collector::with_source(source, true, true)
    }

    #[test]
    fn net_speeds_difference_cumulative_counters_over_elapsed() {
        // prev seeded from the constructor reading (10k/20k); the next sample
        // adds 30k/60k over a 10s window -> 3k/6k per second.
        let mut c = make_collector(FakeSource::default());
        c.source.net_totals = (40_000, 80_000);
        let report = c.assemble_report(10.0);
        assert_eq!(report.net_in_speed, 3_000);
        assert_eq!(report.net_out_speed, 6_000);
        assert_eq!(report.net_in_transfer, 40_000);
        assert_eq!(report.net_out_transfer, 80_000);
    }

    #[test]
    fn counter_reset_saturates_to_zero_speed() {
        // A counter that went backwards (interface re-enumeration) must read
        // as a zero-speed sample, never a negative rate.
        let mut c = make_collector(FakeSource::default());
        c.source.net_totals = (500, 700);
        let report = c.assemble_report(5.0);
        assert_eq!(report.net_in_speed, 0);
        assert_eq!(report.net_out_speed, 0);
        // The new (lower) totals still become the baseline for the next window.
        c.source.net_totals = (1_500, 1_700);
        let next = c.assemble_report(1.0);
        assert_eq!(next.net_in_speed, 1_000);
        assert_eq!(next.net_out_speed, 1_000);
    }

    #[test]
    fn sub_second_elapsed_is_clamped_to_one_second() {
        // elapsed 0.0 (or any sub-second window) divides by 1.0: finite,
        // non-inflated speeds.
        let mut c = make_collector(FakeSource::default());
        c.source.net_totals = (10_100, 20_200);
        let report = c.assemble_report(0.0);
        assert_eq!(report.net_in_speed, 100);
        assert_eq!(report.net_out_speed, 200);
    }

    #[test]
    fn disabled_temperature_and_gpu_are_gated_to_none() {
        // The source has readings, but disabled collection must report None —
        // the gate lives in assembly, not in the source.
        let mut c = Collector::with_source(FakeSource::default(), false, false);
        let report = c.assemble_report(1.0);
        assert!(report.temperature.is_none());
        assert!(report.gpu.is_none());
    }

    #[test]
    fn enabled_temperature_and_gpu_pass_through() {
        let mut c = make_collector(FakeSource::default());
        let report = c.assemble_report(1.0);
        assert_eq!(report.temperature, Some(55.0));
        assert_eq!(report.gpu.as_ref().map(|g| g.count), Some(1));
    }

    #[test]
    fn gauges_pass_through_untransformed() {
        let mut c = make_collector(FakeSource::default());
        let report = c.assemble_report(1.0);
        assert!((report.cpu - 12.5).abs() < f64::EPSILON);
        assert_eq!(report.mem_used, 1024);
        assert_eq!(report.swap_used, 256);
        assert_eq!(report.disk_used, 4096);
        assert!((report.load1 - 0.5).abs() < f64::EPSILON);
        assert!((report.load15 - 0.3).abs() < f64::EPSILON);
        assert_eq!(report.tcp_conn, 7);
        assert_eq!(report.udp_conn, 3);
        assert_eq!(report.process_count, 42);
        assert_eq!(report.uptime, 3600);
    }

    #[test]
    fn collect_refreshes_the_source_before_sampling() {
        let mut c = make_collector(FakeSource::default());
        let _ = c.collect();
        let _ = c.collect();
        assert_eq!(c.source.refreshes, 2);
    }
}
