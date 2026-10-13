//! riscv64 指令缓存同步（SD-20）的宿主形状钉住测试。
//!
//! `os/arch/src/riscv64/cache_sync.rs` 被 `#[cfg(target_arch = "riscv64")]`
//! 整模块门住，宿主不编译它；本文件用 `include_str!` 钉住承重形状：
//!
//! 1. 本 hart 的同步动作必须是 `FENCE.I`（本地 hart 的先前写入对后续取指
//!    可见——装载器场景正好是「本 hart 写、本 hart 随后取」）；
//! 2. 本文件**不得**自行长出 SBI 远端栅栏 ecall：跨 hart 的
//!    `remote_fence_i` 是撤主核钳制批次的硬前置（本树进程只落 BSP，
//!    本地栅栏对当前执行模型是完备的），其实现需要内核拓扑提供 hart 掩码，
//!    不在 arch 层自持。半接（乱传掩码）会打进 M 态固件，比不做更糟。
//!    该绑定是文件文档的一部分，本钉把它连同代码形状一起冻住。
//!
//! 真机判读边界同 `arm64_cache_sync_pin.rs`：QEMU 不建模此类缓存失配，
//! 门禁跑绿只证明「没写坏」。

/// 原始文件全文（含注释与文档）。
fn src() -> &'static str {
    include_str!("../src/riscv64/cache_sync.rs")
}

/// 剥注释后的代码骨架（同 `arm64_cache_sync_pin.rs::code`）。
fn code() -> String {
    let mut out = String::new();
    for line in src().lines() {
        let cut = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        out.push_str(cut);
        out.push(' ');
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn local_fence_i_is_the_synchronization_action() {
    let c = code();
    assert!(c.contains(r#"asm!("fence.i")"#), "本地同步动作不是 FENCE.I");
}

#[test]
fn no_sbi_remote_fence_in_body_until_unclamp_batch() {
    let c = code();
    assert!(
        !c.contains("ecall"),
        "契约体里出现了 SBI ecall——跨 hart 的 remote_fence_i 属撤主核钳制批次的\
         硬前置（需内核拓扑提供 hart 掩码），不得在 arch 层半接"
    );
    assert!(
        !c.contains("remote_fence_i(") || !c.contains("asm!(\"ecall\""),
        "远端栅栏调用形态越界"
    );
}

#[test]
fn smp_binding_is_documented_in_file() {
    let s = src();
    assert!(
        s.contains("remote_fence_i"),
        "文件未记录远端栅栏的归属（SMP 撤钳批次）——绑定说明丢失会让下轮\
         把它当成漏实现重做"
    );
    assert!(
        s.contains("clamp_cpu_to_bsp") && s.contains("SMP"),
        "文件未写明「当前只有 BSP 执行装载镜像」的判据（clamp_cpu_to_bsp/SMP），\
         这是本地栅栏完备性的前提"
    );
}
