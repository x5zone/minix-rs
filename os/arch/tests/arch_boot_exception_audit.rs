//! 双架构 boot 出生契约与 exception 帧判定的宿主静态审计（P-ALL-01 深度
//! 迁移批：boot 9 枚 + exception 8 枚门内测试的宿主可见面）。
//!
//! **boot 的承重契约**：出生 PSR/sstatus 决定「进程第一次被调度回到哪个
//! 特权态、中断开不开」——写错一位就是用户进程以内核特权出生（或反之）。
//! FPU 继承（fork 时父 FP 使能传子）是 aarch64 FPEN 懒陷阱模型的接缝。
//!
//! **exception 的承重契约**：帧尺寸冻结（34 槽系列的 build↔run 契约）与
//! is_user_mode 判定位（arm64 看 SPSR.M[3:0]、riscv64 看 sstatus.SPP bit8）
//! ——判定位写错＝用户/内核源的整个分派树走错腿。

// ── boot：出生 PSR/sstatus ──

#[test]
fn arm64_init_psr_constants_are_frozen() {
    let src = include_str!("../src/arm64/boot.rs");
    // 内核任务：EL1h + F/I/A/D 全屏蔽（C earm/include/archconst.h:12）。
    assert!(
        src.contains("const INIT_TASK_PSR: u64 = 0x0000_03C5;"),
        "INIT_TASK_PSR 漂移（0x3C5 = EL1h | D|A|I|F）——内核任务将以错误特权/屏蔽态出生"
    );
    // 用户进程：EL0t、无屏蔽（C archconst.h:11）。
    assert!(
        src.contains("const INIT_PSR: u64 = 0x0000_0000;"),
        "INIT_PSR 漂移（EL0t 无屏蔽）——用户进程将带内核屏蔽位出生"
    );
}

#[test]
fn arm64_fpu_inheritance_contract_is_frozen() {
    let src = include_str!("../src/arm64/boot.rs");
    // FPEN 懒陷阱模型的接缝：内核任务 FPEN=0（门内 kernel_task 用例）、
    // 全部用户类 FPEN=0b01、fork 子继承父的使能（门内 inherit 用例）。
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
    // 内核任务：SPP=1（sret 回 S 态）；用户进程：SPP=0、SPIE=1（中断开）。
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

// ── exception：帧尺寸与判定位 ──

#[test]
fn exception_frame_size_296_is_frozen_both_archs() {
    let a64 = include_str!("../src/arm64/exception.rs");
    let rv = include_str!("../src/riscv64/exception.rs");
    // 双架构同为 296B＝34 槽系列（与 trap stub 的 34*8 帧契约对齐）。
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
    // 判定式：SPSR_EL1.M[3:0]==0 ⇒ EL0。整行钉（防改成比较别的位）。
    assert!(
        src.contains("(frame.spsr_el1 & 0xF) == 0"),
        "arm64 is_user_mode 判定位漂移（SPSR.M[3:0]）——用户/内核源分派树会走错腿"
    );
    // 门内测试的两点锚（EL0t=0 真、EL1h=5 假）仍在。
    assert!(src.contains("f.spsr_el1 = 0; // M=EL0t"));
    assert!(src.contains("f.spsr_el1 = 5; // M=EL1h"));
}

#[test]
fn riscv64_is_user_mode_reads_spp_bit() {
    let src = include_str!("../src/riscv64/exception.rs");
    // 判定式：sstatus.SPP（bit 8）==0 ⇒ U-mode。
    assert!(
        src.contains("(frame.sstatus & (1 << 8)) == 0"),
        "riscv64 is_user_mode 判定位漂移（SPP bit8）——§续-375 哨兵（S-origin 陷入）也依赖同一位语义"
    );
}

#[test]
fn exception_setters_target_the_documented_slots() {
    let a64 = include_str!("../src/arm64/exception.rs");
    let rv = include_str!("../src/riscv64/exception.rs");
    // arm64：IP→elr_el1、返回值→X0；riscv64：IP→sepc、返回值→a0(regs[10])。
    assert!(a64.contains("fn set_instruction_pointer_writes_elr_el1"));
    assert!(a64.contains("fn set_return_value_writes_x0"));
    assert!(a64.contains("f.elr_el1, 0xCAFE"));
    assert!(a64.contains("f.regs[0], 0x1234"));
    assert!(rv.contains("fn set_instruction_pointer_writes_sepc"));
    assert!(rv.contains("fn set_return_value_writes_a0"));
    assert!(rv.contains("f.sepc, 0xCAFE"));
    assert!(rv.contains("f.regs[10], 0x1234"), "a0=ABI 第 0 参槽（regs[10]）");
}
