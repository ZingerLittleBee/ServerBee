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

#[cfg(test)]
mod tests {
    use super::*;
    use sysinfo::System;

    #[test]
    fn test_usage_in_range() {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        let u = usage(&sys);
        assert!(u >= 0.0, "usage should be non-negative, got {u}");
        // Global CPU usage is an average percentage across cores.
        assert!(u <= 100.0, "usage should be <= 100, got {u}");
    }

    #[test]
    fn test_name_returns_string() {
        let sys = System::new_all();
        // name() never panics; on a real machine it is typically non-empty,
        // but we only assert it returns a valid (possibly empty) String.
        let _ = name(&sys);
    }

    #[test]
    fn test_cores_positive() {
        let sys = System::new_all();
        assert!(cores(&sys) > 0, "host must report at least one core");
    }

    #[test]
    fn test_arch_matches_consts() {
        assert_eq!(arch(), std::env::consts::ARCH.to_string());
        assert!(!arch().is_empty());
    }
}
