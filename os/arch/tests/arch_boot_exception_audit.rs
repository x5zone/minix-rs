//! Host-side static audit of the two-architecture boot birth contract and
//! the exception-frame judgement (the host-visible surface of the 9 boot and
//! 8 exception in-gate tests).
//!
//! **The load-bearing boot contract**: the birth PSR/sstatus decides which
//! privilege state the process is first scheduled back into and whether
//! interrupts are enabled — one wrong bit means a user process is born with
//! kernel privilege (or vice versa). FPU inheritance (the parent's FP
//! enable propagating to the child on fork) is the seam of the aarch64 FPEN
//! lazy-trap model.
//!
//! **The load-bearing exception contract**: the frame size is frozen (the
//! build↔run contract of the 34-slot family) and so is the is_user_mode
//! predicate bit (arm64 reads SPSR.M[3:0], riscv64 reads sstatus.SPP bit 8)
//! — a wrong predicate bit sends the whole user/kernel-origin dispatch tree
//! down the wrong leg.

// ── boot: birth PSR/sstatus ──

#[test]
fn arm64_init_psr_constants_are_frozen() {
    let src = include_str!("../src/arm64/boot.rs");
    // Kernel task: EL1h with F/I/A/D all masked
    // (C earm/include/archconst.h:12).
    assert!(
        src.contains("const INIT_TASK_PSR: u64 = 0x0000_03C5;"),
        "INIT_TASK_PSR 漂移（0x3C5 = EL1h | D|A|I|F）——内核任务将以错误特权/屏蔽态出生"
    );
    // User process: EL0t, nothing masked (C archconst.h:11).
    assert!(
        src.contains("const INIT_PSR: u64 = 0x0000_0000;"),
        "INIT_PSR 漂移（EL0t 无屏蔽）——用户进程将带内核屏蔽位出生"
    );
}

#[test]
fn arm64_fpu_inheritance_contract_is_frozen() {
    let src = include_str!("../src/arm64/boot.rs");
    // Seam of the FPEN lazy-trap model: kernel tasks FPEN=0 (in-gate
    // kernel_task case), all user-class entries FPEN=0b01, and fork
    // children inherit the parent's enable (in-gate inherit case).
    for contract in [
        "\"kernel task must keep FPEN=0\"",
        "\"user process must have FPEN=0b01\"",
        "\"child must inherit parent FP enable\"",
    ] {
        assert!(
            src.contains(contract),
            "arm64 FPU 继承契约断言漂移：{contract}——FPEN 链断＝用户态首条 FP 指令不可恢复"
        );
    }
}

#[test]
fn riscv64_init_sstatus_constants_are_frozen() {
    let src = include_str!("../src/riscv64/boot.rs");
    // Kernel task: SPP=1 (sret returns to S-mode); user process: SPP=0,
    // SPIE=1 (interrupts enabled).
    assert!(
        src.contains("const INIT_TASK_SSTATUS: u64 = 0x0000_0100;"),
        "INIT_TASK_SSTATUS 漂移（SPP=1）——内核任务 sret 后落 U 态"
    );
    assert!(
        src.contains("const INIT_USER_SSTATUS: u64 = 0x0000_0020;"),
        "INIT_USER_SSTATUS 漂移（SPP=0|SPIE=1）——用户进程出生即中断关闭"
    );
}

#[test]
fn riscv64_fpu_inheritance_via_sstatus_copy_is_frozen() {
    let src = include_str!("../src/riscv64/boot.rs");
    assert!(
        src.contains("inherit_fpu_state_copies_sstatus"),
        "riscv64 FPU 继承测试消失——懒浮点的 fork 接缝（FS 位随 sstatus 拷贝）失去门内守卫"
    );
}

// ── exception: frame size and predicate bit ──

#[test]
fn exception_frame_size_296_is_frozen_both_archs() {
    let a64 = include_str!("../src/arm64/exception.rs");
    let rv = include_str!("../src/riscv64/exception.rs");
    // Both architectures are 296B = the 34-slot family (aligned with the
    // trap stub's 34*8 frame contract).
    assert!(
        a64.contains("size_of::<AArch64ExceptionFrame>(), 296"),
        "arm64 异常帧尺寸漂移（296B 契约，build↔run 对齐）"
    );
    assert!(
        rv.contains("size_of::<Riscv64ExceptionFrame>(), 296"),
        "riscv64 异常帧尺寸漂移"
    );
}

#[test]
fn arm64_is_user_mode_reads_spsr_m_nibble() {
    let src = include_str!("../src/arm64/exception.rs");
    // Predicate: SPSR_EL1.M[3:0]==0 ⇒ EL0. Pinned as a whole line (so it
    // cannot be changed to compare some other bit).
    assert!(
        src.contains("(frame.spsr_el1 & 0xF) == 0"),
        "arm64 is_user_mode 判定位漂移（SPSR.M[3:0]）——用户/内核源分派树会走错腿"
    );
    // The two in-gate anchors (EL0t=0 true, EL1h=5 false) are still present.
    assert!(src.contains("f.spsr_el1 = 0; // M=EL0t"));
    assert!(src.contains("f.spsr_el1 = 5; // M=EL1h"));
}

#[test]
fn riscv64_is_user_mode_reads_spp_bit() {
    let src = include_str!("../src/riscv64/exception.rs");
    // Predicate: sstatus.SPP (bit 8) == 0 ⇒ U-mode.
    assert!(
        src.contains("(frame.sstatus & (1 << 8)) == 0"),
        "riscv64 is_user_mode 判定位漂移（SPP bit8）——§续-375 哨兵（S-origin 陷入）也依赖同一位语义"
    );
}

#[test]
fn exception_setters_target_the_documented_slots() {
    let a64 = include_str!("../src/arm64/exception.rs");
    let rv = include_str!("../src/riscv64/exception.rs");
    // arm64: IP→elr_el1, return value→X0; riscv64: IP→sepc,
    // return value→a0(regs[10]).
    assert!(a64.contains("fn set_instruction_pointer_writes_elr_el1"));
    assert!(a64.contains("fn set_return_value_writes_x0"));
    assert!(a64.contains("f.elr_el1, 0xCAFE"));
    assert!(a64.contains("f.regs[0], 0x1234"));
    assert!(rv.contains("fn set_instruction_pointer_writes_sepc"));
    assert!(rv.contains("fn set_return_value_writes_a0"));
    assert!(rv.contains("f.sepc, 0xCAFE"));
    assert!(rv.contains("f.regs[10], 0x1234"), "a0=ABI 第 0 参槽（regs[10]）");
}
