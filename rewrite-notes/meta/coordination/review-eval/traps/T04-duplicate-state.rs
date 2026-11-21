// EVAL FIXTURE — 不属于内核代码，只用于 review-loop 的对抗样本。
// 场景：一个进程槽的状态表示。请审查设计，不要修改代码。

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Empty,
    Ready,
    Running,
}

pub struct Slot {
    pub present: bool,
    pub state: SlotState,
}
