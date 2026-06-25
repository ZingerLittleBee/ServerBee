use sysinfo::Components;

pub fn get_temperature() -> Option<f64> {
    let components = Components::new_with_refreshed_list();
    let temps: Vec<f64> = components
        .iter()
        .filter_map(|c| {
            let t = c.temperature()?;
            if t > 0.0 && !t.is_nan() {
                Some(t as f64)
            } else {
                None
            }
        })
        .collect();
    if temps.is_empty() {
        None
    } else {
        Some(temps.iter().sum::<f64>() / temps.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_temperature_returns_sensible_value_or_none() {
        // Hardware-dependent: many CI hosts expose no thermal components and
        // return None. When a value is present it must be a positive, finite
        // average (the function filters out non-positive and NaN readings).
        match get_temperature() {
            None => {}
            Some(t) => {
                assert!(t > 0.0, "averaged temperature must be positive, got {t}");
                assert!(t.is_finite(), "averaged temperature must be finite, got {t}");
                // Sanity upper bound: real sensors stay well under this.
                assert!(t < 1000.0, "temperature {t} is implausibly high");
            }
        }
    }

    #[test]
    fn test_get_temperature_is_callable_repeatedly() {
        // Ensure no panic / no internal state leakage across calls.
        let _ = get_temperature();
        let _ = get_temperature();
    }
}
