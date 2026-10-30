//! Host-shape pin test for riscv64 instruction-cache synchronization.
//!
//! `os/arch/src/riscv64/cache_sync.rs` is gated wholesale behind
//! `#[cfg(target_arch = "riscv64")]`, so the host never compiles it; this
//! file pins the load-bearing shape with `include_str!`:
//!
//! 1. the local hart's synchronization action must be `FENCE.I` (a local
//!    hart's earlier writes become visible to its own later instruction
//!    fetches — the loader scenario is exactly "this hart writes, the same
//!    hart fetches afterwards");
//! 2. this file must **not** grow an SBI remote-fence ecall on its own:
//!    `remote_fence_i` across harts is a hard prerequisite of the batch that
//!    removes the main-core clamp (in this tree processes run only on the
//!    BSP, so a local fence is complete for the current execution model); its
//!    implementation needs a hart mask from the kernel topology and is not
//!    held at the arch layer. Half-wiring it (passing a bogus mask) would
//!    reach M-mode firmware and be worse than not doing it at all. That
//!    binding is part of the file documentation; this pin freezes it together
//!    with the code shape.
//!
//! The real-hardware judgment boundary matches `arm64_cache_sync_pin.rs`:
//! QEMU does not model this class of cache mismatch, so a green gate only
//! proves "nothing was written wrong".

/// Full source text of the file (comments and docs included).
fn src() -> &'static str {
    include_str!("../src/riscv64/cache_sync.rs")
}

/// Code skeleton with comments stripped (same as `arm64_cache_sync_pin.rs::code`).
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
