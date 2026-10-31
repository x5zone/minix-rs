//! Source pin: riscv64's user-return entry stack is per-hart only.
//!
//! The SMP bug this pins: `KERNEL_TRAP_STACK_BASE` was one scalar shared by
//! every hart. An AP's `init_ap` wrote its own stack top there, the BSP's next
//! `restore_to_user` re-anchored `sscratch` from that scalar, and the
//! stack-based `current_cpu_id()` then attributed the BSP's first kernel call
//! to the AP's idle process. The fix is the trait hook
//! `TrapReturnArch::prepare_user_return` writing the calling hart's own
//! `sscratch`; no shared symbol may reappear in the return path.
//!
//! These are text pins, not behaviour tests: the real CSR write cannot run on
//! the host. The behaviour half is covered by the QEMU `-smp 1/2/4` ATF runs.

use std::fs;
use std::path::PathBuf;

fn arch_src(rel: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(rel))
        .unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

#[test]
fn trap_return_has_no_shared_stack_cell() {
    let src = arch_src("src/riscv64/trap_return.rs");
    assert!(
        !src.contains("KERNEL_TRAP_STACK_BASE"),
        "riscv64 trap return must not read the shared KERNEL_TRAP_STACK_BASE cell"
    );
    assert!(
        !src.contains("csrw sscratch"),
        "riscv64 trap return must not write sscratch itself; prepare_user_return owns that"
    );
    assert!(
        src.contains("fn prepare_user_return"),
        "riscv64 must implement the per-CPU prepare_user_return hook"
    );
}

#[test]
fn protection_keeps_entry_stack_per_hart() {
    let src = arch_src("src/riscv64/protection.rs");
    assert!(
        !src.contains("KERNEL_TRAP_STACK_BASE"),
        "protection must not keep a shared trap-stack scalar"
    );
    assert!(
        src.contains("pub(crate) fn set_entry_stack_base"),
        "the per-hart CSR write helper must stay the single entry-stack writer"
    );
    // The AP path writes the calling hart's own CSR; no cpu-indexed global
    // is needed, and `_cpu_id` documents that the hart is the index.
    let init_ap: String = src
        .split("fn init_ap(")
        .nth(1)
        .expect("riscv64 protection must define init_ap")
        .chars()
        .take(600)
        .collect();
    assert!(
        init_ap.contains("set_entry_stack_base(kernel_stack_top)"),
        "init_ap must install the AP's own sscratch"
    );
    assert!(
        !init_ap.contains("static") && !init_ap.contains("Atomic"),
        "init_ap must not introduce shared state for the entry stack"
    );
}

#[test]
fn trap_return_trait_documents_the_prepare_contract() {
    let src = arch_src("src/arch/trap_return.rs");
    assert!(
        src.contains("fn prepare_user_return"),
        "TrapReturnArch must expose prepare_user_return"
    );
    assert!(
        src.contains("shared scalar") && src.contains("shared symbol"),
        "the trait contract must forbid shared entry-stack state"
    );
}
