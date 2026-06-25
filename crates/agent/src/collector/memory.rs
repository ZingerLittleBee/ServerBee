use sysinfo::System;

pub fn mem_used(sys: &System) -> i64 {
    sys.used_memory() as i64
}

pub fn mem_total(sys: &System) -> i64 {
    sys.total_memory() as i64
}

pub fn swap_used(sys: &System) -> i64 {
    sys.used_swap() as i64
}

pub fn swap_total(sys: &System) -> i64 {
    sys.total_swap() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys() -> System {
        let mut s = System::new_all();
        s.refresh_memory();
        s
    }

    #[test]
    fn test_mem_used_le_total_and_positive() {
        let s = sys();
        let total = mem_total(&s);
        let used = mem_used(&s);
        assert!(total > 0, "host must report total memory");
        assert!(used >= 0);
        assert!(used <= total, "used {used} must not exceed total {total}");
    }

    #[test]
    fn test_swap_used_le_total() {
        let s = sys();
        let total = swap_total(&s);
        let used = swap_used(&s);
        // Swap may legitimately be zero on a host; assert invariants only.
        assert!(total >= 0);
        assert!(used >= 0);
        assert!(used <= total, "swap used {used} must not exceed total {total}");
    }
}
