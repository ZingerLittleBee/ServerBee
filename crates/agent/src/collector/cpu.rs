use sysinfo::System;

pub fn usage(sys: &System) -> f64 {
    sys.global_cpu_usage() as f64
}

pub fn name(sys: &System) -> String {
    sys.cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_default()
}

pub fn cores(sys: &System) -> i32 {
    sys.cpus().len() as i32
}

pub fn arch() -> String {
    std::env::consts::ARCH.to_string()
}
