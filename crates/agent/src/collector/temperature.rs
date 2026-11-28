use sysinfo::Components;

pub fn get_temperature_from_components(components: &Components) -> Option<f64> {
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for component in components.iter() {
        if let Some(t) = component.temperature()
            && t > 0.0
            && !t.is_nan()
        {
            sum += t as f64;
            count += 1;
        }
    }
    if count == 0 {
        None
    } else {
        Some(sum / count as f64)
    }
}

#[cfg(test)]
pub fn get_temperature() -> Option<f64> {
    let components = Components::new_with_refreshed_list();
    get_temperature_from_components(&components)
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
                assert!(
                    t.is_finite(),
                    "averaged temperature must be finite, got {t}"
                );
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
