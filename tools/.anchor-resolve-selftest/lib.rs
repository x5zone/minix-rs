pub const MAX_TICKS: u64 = 100;

pub struct Clock {
    pub ticks: u64,
}

impl Clock {
    pub fn reset(&mut self) {
        self.ticks = 0;
    }
}

pub struct Timer;

impl Timer {
    pub fn reset(&mut self) {
    }
}

pub fn clock_init() {
    let _ = MAX_TICKS;
}
