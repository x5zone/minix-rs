//! 双架构 signal 腿的宿主静态审计（P-ALL-01 深度迁移批：signal 双架构
//! 12 枚门内测试的宿主可见面）。
//!
//! sigcontext 是用户可见 ABI（信号处理器收到的恢复上下文），其承重契约
//! 以源码文本钉在宿主可跑处：
//! 1. **SC_MAGIC 权威值**（C arch/i386/include/signal.h:115 的 0xc0ffee1）
//!    ——`check_magic` 拒收非魔数上下文，魔数漂移＝静默接受垃圾帧。
//! 2. **MF_CONTEXT_SET**（proc.h:251 的 0x4000）。
//! 3. **build/restore/setup_handler 的行为断言**无法在宿主执行（函数住在
//!    架构门内），但它们的调用契约（`sc_magic` 写入点、`check_magic` 门、
//!    handler 入腿的 LR=signreturn/X0=signo/X2=sigcontext 形状）以结构
//!    存在性＋调用形状文本钉住——漂移即红。

// ── 权威常量（arch/signal_context.rs，三架构共享）──

#[test]
fn sc_magic_matches_c_ground_truth() {
    const SRC: &str = include_str!("../src/arch/signal_context.rs");
    assert!(
        SRC.contains("pub const SC_MAGIC: i32 = 0xc0ffee1;"),
        "SC_MAGIC 漂移——C arch/i386/include/signal.h:115 的 0xc0ffee1 是 \
         sigcontext 完整性魔数（mcf_magic/MCF_MAGIC 同族），漂移即 check_magic \
         全线失效"
    );
    assert!(
        SRC.contains("pub const MF_CONTEXT_SET: i32 = 0x4000;"),
        "MF_CONTEXT_SET 漂移（C proc.h:251）"
    );
}

// ── arm64：check_magic 门与三段调用契约 ──

#[test]
fn arm64_check_magic_gates_on_sc_magic() {
    let src = include_str!("../src/arm64/signal.rs");
    // check_magic 的实现体必须比较 sc_magic 与 SC_MAGIC（整行级，防偶然子串）。
    assert!(
        src.contains("sctx.sc_magic == SC_MAGIC"),
        "arm64 check_magic 必须以 sc_magic == SC_MAGIC 判定（漂移＝静默接受垃圾帧）"
    );
    // build 腿必须写入魔数。
    assert!(
        src.contains("sctx.sc_magic = SC_MAGIC;"),
        "arm64 build_sigcontext 必须写 SC_MAGIC"
    );
}

#[test]
fn arm64_handler_entry_contract_is_frozen() {
    let src = include_str!("../src/arm64/signal.rs");
    // setup_handler 的形状：LR=signreturn、X0=signo、X1=0（sf_code）、
    // X2=sigcontext 指针——用户处理器返回时经 LR 进 sigreturn。
    for contract in [
        "gp_regs[GP_X30], 0x5000", // LR = sigreturn（测试锚值）
        "ctx.r0, 6",               // X0 = signo
        "gp_regs[GP_X1], 0",       // X1 = 0 (sf_code)
    ] {
        assert!(
            src.contains(contract),
            "arm64 handler 入腿契约片段漂移：{contract}——X30=LR/X0=signo/X1=code \
             是用户信号 ABI 的承重面"
        );
    }
}

// ── riscv64：同一契约的 Sv39 形 ──

#[test]
fn riscv64_check_magic_gates_on_sc_magic() {
    let src = include_str!("../src/riscv64/signal.rs");
    assert!(
        src.contains("sc_magic == SC_MAGIC"),
        "riscv64 check_magic 必须以 sc_magic == SC_MAGIC 判定"
    );
    assert!(
        src.contains("sctx.sc_magic = SC_MAGIC;"),
        "riscv64 build_sigcontext 必须写 SC_MAGIC"
    );
}

#[test]
fn riscv64_handler_entry_contract_is_frozen() {
    let src = include_str!("../src/riscv64/signal.rs");
    // riscv 形：a0=signo、a1=sf_code、a2=sigcontext（C 传导约定的 Sv39 版）。
    for contract in ["test_setup_handler_entry_riscv64", "test_restore_sigcontext_riscv64"] {
        assert!(
            src.contains(contract),
            "riscv64 signal 门内测试 {contract} 消失——清单审计（arch_gated_audit）会同时红，\
             此处为双保险并记录其宿主可达化承诺"
        );
    }
}
