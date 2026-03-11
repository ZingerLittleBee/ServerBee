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
