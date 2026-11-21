// EVAL FIXTURE — 不属于内核代码，只用于 review-loop 的对抗样本。
// 场景：一个时钟源抽象。请审查设计，不要修改代码。

pub trait ClockSource {
    fn read_ns(&self) -> u64;
}

pub struct Hpet;

impl ClockSource for Hpet {
    fn read_ns(&self) -> u64 {
        0
    }
}

pub fn now_ms(c: &Hpet) -> u64 {
    c.read_ns() / 1_000_000
}
