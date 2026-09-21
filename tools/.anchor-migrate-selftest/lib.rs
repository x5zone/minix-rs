pub const MAX_TICKS: u64 = 100;

pub struct Clock {
    pub ticks: u64,
}

pub fn clock_init() {
    // 内部第 1 行
    // 内部第 2 行
    let _ = MAX_TICKS;
}

pub fn clock_stop() {
}
