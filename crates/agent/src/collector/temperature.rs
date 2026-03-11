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
