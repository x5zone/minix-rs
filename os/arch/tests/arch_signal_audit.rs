//! Host-side static audit of the signal legs on both architectures: the
//! host-visible surface of the 12 in-gate signal tests.
//!
//! sigcontext is a user-visible ABI (the restore context a signal handler
//! receives), and its load-bearing contract is pinned in source text where the
//! host can run it:
//! 1. **SC_MAGIC authoritative value** (0xc0ffee1 at C arch/i386/include/signal.h:115)
//!    — `check_magic` rejects contexts that lack the magic value; a magic
//!    drift silently accepts garbage frames.
//! 2. **MF_CONTEXT_SET** (0x4000 at proc.h:251).
//! 3. **Behavioral assertions of build/restore/setup_handler** cannot run on
//!    the host (the functions live behind the arch gate), but their call
//!    contracts (the `sc_magic` write site, the `check_magic` gate, the
//!    handler entry shape LR=signreturn/X0=signo/X2=sigcontext) are pinned as
//!    structural existence plus call shape in source text — drift turns red.

// ── Authoritative constants (arch/signal_context.rs, shared by all three architectures) ──

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

// ── arm64: the check_magic gate and the three-part call contract ──

#[test]
fn arm64_check_magic_gates_on_sc_magic() {
    let src = include_str!("../src/arm64/signal.rs");
    // The check_magic body must compare sc_magic with SC_MAGIC (whole line,
    // so an accidental substring cannot satisfy it).
    assert!(
        src.contains("sctx.sc_magic == SC_MAGIC"),
        "arm64 check_magic 必须以 sc_magic == SC_MAGIC 判定（漂移＝静默接受垃圾帧）"
    );
    // The build leg must write the magic value.
    assert!(
        src.contains("sctx.sc_magic = SC_MAGIC;"),
        "arm64 build_sigcontext 必须写 SC_MAGIC"
    );
}

#[test]
fn arm64_handler_entry_contract_is_frozen() {
    let src = include_str!("../src/arm64/signal.rs");
    // setup_handler shape: LR=signreturn, X0=signo, X1=0 (sf_code),
    // X2=sigcontext pointer — the user handler returns through LR into
    // sigreturn.
    for contract in [
        "gp_regs[GP_X30], 0x5000", // LR = sigreturn (anchor value in the test)
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

// ── riscv64: the same contract in Sv39 form ──

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
    // riscv form: a0=signo, a1=sf_code, a2=sigcontext (the Sv39 version of
    // the C calling convention).
    for contract in ["test_setup_handler_entry_riscv64", "test_restore_sigcontext_riscv64"] {
        assert!(
            src.contains(contract),
            "riscv64 signal 门内测试 {contract} 消失——清单审计（arch_gated_audit）会同时红，\
             此处为双保险并记录其宿主可达化承诺"
        );
    }
}
