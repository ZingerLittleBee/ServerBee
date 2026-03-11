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
