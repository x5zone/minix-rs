// EVAL FIXTURE — 不属于内核代码，只用于 review-loop 的对抗样本。
// 场景：一个进程槽的 Rust 表示。请审查设计，不要修改代码。

pub struct Proc {
    pub flags: u32,
    pub state: i32,
    pub endpoint: i32,
    pub p_nr: i32,
}

pub trait ProcessManager {
    fn get(&self, nr: usize) -> Option<&Proc>;
    fn set_state(&mut self, nr: usize, state: i32);
}

pub struct ProcTable {
    procs: [Proc; 64],
}

impl ProcessManager for ProcTable {
    fn get(&self, nr: usize) -> Option<&Proc> {
        self.procs.get(nr)
    }

    fn set_state(&mut self, nr: usize, state: i32) {
        self.procs[nr].state = state;
    }
}
