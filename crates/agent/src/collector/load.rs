use sysinfo::System;

pub fn load1() -> f64 {
    System::load_average().one
}

pub fn load5() -> f64 {
    System::load_average().five
}

pub fn load15() -> f64 {
    System::load_average().fifteen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_averages_non_negative_and_finite() {
        // On platforms without load average support these may be 0.0;
        // assert only that they are non-negative and finite.
        for v in [load1(), load5(), load15()] {
            assert!(v >= 0.0, "load average must be non-negative, got {v}");
            assert!(v.is_finite(), "load average must be finite, got {v}");
        }
    }
}
