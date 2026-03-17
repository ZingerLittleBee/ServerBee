use chrono::{Datelike, Duration, NaiveDate};

/// Compute per-direction independent delta.
/// If a direction's current value < previous, treat as restart (use raw value).
pub fn compute_delta(prev_in: i64, prev_out: i64, curr_in: i64, curr_out: i64) -> (i64, i64) {
    let delta_in = if curr_in >= prev_in {
        curr_in - prev_in
    } else {
        curr_in
    };
    let delta_out = if curr_out >= prev_out {
        curr_out - prev_out
    } else {
        curr_out
    };
    (delta_in, delta_out)
}

/// Compute billing cycle date range.
/// Returns (start_date_inclusive, end_date_inclusive).
pub fn get_cycle_range(
    billing_cycle: &str,
    billing_start_day: Option<i32>,
    today: NaiveDate,
) -> (NaiveDate, NaiveDate) {
    let anchor = billing_start_day.unwrap_or(1).clamp(1, 28);

    match billing_cycle {
        "quarterly" => get_quarterly_range(anchor, today),
        "yearly" => get_yearly_range(anchor, today),
        _ => get_monthly_range(anchor, today), // "monthly" or unknown
    }
}

fn get_monthly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let (y, m) = (today.year(), today.month());

    let cycle_start = if today.day() as i32 >= anchor {
        NaiveDate::from_ymd_opt(y, m, anchor as u32).unwrap()
    } else {
        // Go to previous month
        let prev = today - Duration::days(today.day() as i64);
        NaiveDate::from_ymd_opt(prev.year(), prev.month(), anchor as u32).unwrap()
    };

    // End = day before next anchor
    let cycle_end = if anchor == 1 {
        // Natural month: end is last day of start's month
        let next_month = if cycle_start.month() == 12 {
            NaiveDate::from_ymd_opt(cycle_start.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(cycle_start.year(), cycle_start.month() + 1, 1).unwrap()
        };
        next_month - Duration::days(1)
    } else {
        let next = add_months(cycle_start, 1);
        next - Duration::days(1)
    };

    (cycle_start, cycle_end)
}

fn get_quarterly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let (y, _m) = (today.year(), today.month());
    let quarter_start_months = [1, 4, 7, 10];

    let mut cycle_start = None;
    for &qm in quarter_start_months.iter().rev() {
        let candidate = NaiveDate::from_ymd_opt(y, qm, anchor as u32);
        if let Some(c) = candidate {
            if c <= today {
                cycle_start = Some(c);
                break;
            }
        }
    }
    let cycle_start = cycle_start
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(y - 1, 10, anchor as u32).unwrap());

    let end = add_months(cycle_start, 3) - Duration::days(1);
    (cycle_start, end)
}

fn get_yearly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(today.year(), 1, anchor as u32).unwrap();
    if start <= today {
        let end = add_months(start, 12) - Duration::days(1);
        (start, end)
    } else {
        let start = NaiveDate::from_ymd_opt(today.year() - 1, 1, anchor as u32).unwrap();
        let end = add_months(start, 12) - Duration::days(1);
        (start, end)
    }
}

fn add_months(date: NaiveDate, months: u32) -> NaiveDate {
    let total_months = date.year() * 12 + date.month() as i32 - 1 + months as i32;
    let y = total_months / 12;
    let m = (total_months % 12) + 1;
    let d = date.day().min(days_in_month(y, m as u32));
    NaiveDate::from_ymd_opt(y, m as u32, d).unwrap()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(
        if month == 12 { year + 1 } else { year },
        if month == 12 { 1 } else { month + 1 },
        1,
    )
    .unwrap()
    .pred_opt()
    .unwrap()
    .day()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_delta_normal() {
        let (d_in, d_out) = compute_delta(100, 200, 150, 250);
        assert_eq!(d_in, 50);
        assert_eq!(d_out, 50);
    }

    #[test]
    fn test_compute_delta_both_restart() {
        let (d_in, d_out) = compute_delta(100_000, 50_000, 500, 300);
        assert_eq!(d_in, 500);
        assert_eq!(d_out, 300);
    }

    #[test]
    fn test_compute_delta_single_direction_restart_in() {
        let (d_in, d_out) = compute_delta(100_000, 50_000, 500, 51_000);
        assert_eq!(d_in, 500);
        assert_eq!(d_out, 1_000);
    }

    #[test]
    fn test_compute_delta_single_direction_restart_out() {
        let (d_in, d_out) = compute_delta(100_000, 50_000, 101_000, 300);
        assert_eq!(d_in, 1_000);
        assert_eq!(d_out, 300);
    }

    #[test]
    fn test_compute_delta_zero() {
        let (d_in, d_out) = compute_delta(100, 200, 100, 200);
        assert_eq!(d_in, 0);
        assert_eq!(d_out, 0);
    }

    #[test]
    fn test_cycle_range_natural_month() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let (start, end) = get_cycle_range("monthly", None, today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }

    #[test]
    fn test_cycle_range_billing_day_15() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let (start, end) = get_cycle_range("monthly", Some(15), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 4, 14).unwrap());
    }

    #[test]
    fn test_cycle_range_billing_day_before_anchor() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let (start, end) = get_cycle_range("monthly", Some(15), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 14).unwrap());
    }

    #[test]
    fn test_cycle_range_quarterly() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 10).unwrap();
        let (start, end) = get_cycle_range("quarterly", Some(1), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 6, 30).unwrap());
    }

    #[test]
    fn test_cycle_range_yearly() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
        let (start, end) = get_cycle_range("yearly", Some(1), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }

    #[test]
    fn test_cycle_range_unknown_falls_back_to_monthly() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let (start, end) = get_cycle_range("unknown", None, today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }
}
