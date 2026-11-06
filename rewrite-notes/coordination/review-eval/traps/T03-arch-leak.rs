// EVAL FIXTURE — 不属于内核代码，只用于 review-loop 的对抗样本。
// 场景：一个声称位于共享层的模块。请审查设计，不要修改代码。

pub fn init_tss(cpu: usize) {
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) cpu);
    }
}

pub fn flush_tlb() {
    unsafe {
        core::arch::asm!("invlpg [0]");
    }
}
