//! **Boot-form registry + cross-form postcondition equivalence battery**.
//!
//! The kernel boot entry "form family" in this tree has grown from three
//! to five. The decision was to keep the multiple forms and add an
//! **explicit interface + boot-postcondition equivalence assertions + a
//! form count**, rather than forcing unification. What was missing before
//! was exactly the "equivalence assertion layer" and "the count accounted
//! for in one place" — every new form used to arrive with nothing to stop
//! it, and the `entry_start_pa` class of paper-constant typo recurred
//! three times. This module supplies both halves:
//!
//! 1. [`BootForm`] collects the five forms in one enum, and
//!    `ALL_BOOT_FORMS` is the single source for the form count; adding or
//!    removing a variant must change it too, or
//!    `test_boot_form_count_is_accounted` fails on the spot. The count test
//!    guards **this enum only**: a new boot entry path added elsewhere in
//!    the tree is not detected here — this registry is a postcondition
//!    battery and a historical inventory, not the production path map.
//! 2. [`KernelInfo::check_boot_postconditions`] (kernel_info.rs) is the
//!    postcondition predicate shared across forms; the battery below runs
//!    it against one canonical higher-half geometry, realizing "equivalent
//!    post-assertions, not entry-structure equality" — absolute addresses
//!    depend on firmware, so what can be equivalent is the same set of
//!    higher-half address-space relations. No per-form `KernelInfo`
//!    constructor exists, so the battery does not exercise each form's real
//!    entry; it proves the predicate is form-independent.
//!
//! Production x86_64 UEFI uses the v1 cross-image handoff; the inline
//! entry this enum calls `X86UefiInline` is a retired historical form kept
//! for the inventory (see `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
//! §4.4 for the current production entries).

use crate::kernel_info::KernelInfo;

/// One kernel boot form. The variant order is (a)–(e). The retired inline
/// form has no live implementation in tree; its `source_path` points at the
/// carrier binary where the entry used to live, and only the file's
/// existence is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootForm {
    /// (a) x86_64 UEFI inline: `arch_boot` ran directly inside the
    /// boot-shim process; the kernel was statically linked into the PE.
    /// Retired: `os/boot-shim/src` no longer contains an `arch_boot` call
    /// (production x86_64 UEFI uses the v1 cross-image handoff; see the
    /// module note above). The path records where the carrier entry used to
    /// live.
    X86UefiInline,
    /// (b) aarch64 UEFI cross-image: the shim produces a `BootHandoffV1`
    /// arena and hands over to `kernel.elf`.
    Aarch64UefiCrossImage,
    /// (c) riscv64 OpenSBI self-boot: `os/kernel-image/src/bootface.rs`.
    RiscvOpensbiSelfBoot,
    /// (d) aarch64 QEMU direct-core boot:
    ///     `os/kernel-image/src/bootface_a64.rs` (the fourth independent
    ///     entry, loaded via `-kernel`).
    Aarch64QemuDirectCore,
    /// (e) riscv64 AP early-entry stub: secondary harts enter
    ///     `ap_early_entry` and park until a kernel-registered tail is
    ///     installed. This is a secondary-hart path, not a primary boot form;
    ///     it stays in the inventory for the family count.
    RiscvEarlyHartMailbox,
}

/// The single source of the form count. Adding a form adds one element
/// here, and the count test stops "the family grew without being accounted
/// for" at compile/test time instead of relying on human memory.
pub const ALL_BOOT_FORMS: &[BootForm] = &[
    BootForm::X86UefiInline,
    BootForm::Aarch64UefiCrossImage,
    BootForm::RiscvOpensbiSelfBoot,
    BootForm::Aarch64QemuDirectCore,
    BootForm::RiscvEarlyHartMailbox,
];

impl BootForm {
    /// Where this form lives in tree (the source path). For the four live
    /// forms a missing path means the form was moved or renamed without
    /// updating this record; for the retired inline form it only records
    /// the carrier binary's location.
    pub const fn source_path(self) -> &'static str {
        match self {
            BootForm::X86UefiInline => "os/boot-shim/src/main.rs",
            BootForm::Aarch64UefiCrossImage => "os/libs/minix-boot/src/handoff_v1.rs",
            BootForm::RiscvOpensbiSelfBoot => "os/kernel-image/src/bootface.rs",
            BootForm::Aarch64QemuDirectCore => "os/kernel-image/src/bootface_a64.rs",
            BootForm::RiscvEarlyHartMailbox => "os/arch/src/riscv64/ap_early_entry.rs",
        }
    }

    /// Human-readable short name, for diagnostics and documentation
    /// references.
    pub const fn label(self) -> &'static str {
        match self {
            BootForm::X86UefiInline => "x86_64 UEFI inline",
            BootForm::Aarch64UefiCrossImage => "aarch64 UEFI cross-image",
            BootForm::RiscvOpensbiSelfBoot => "riscv64 OpenSBI self-boot",
            BootForm::Aarch64QemuDirectCore => "aarch64 QEMU direct-core",
            BootForm::RiscvEarlyHartMailbox => "riscv64 early-hart mailbox",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_info::{BootModule, MemoryRegion};
    use minix_types::{PhysBytes, USER_STACK_TOP, VirBytes};

    /// The form count is accounted for in one place. This guards the enum's
    /// own membership: adding or removing a variant without updating
    /// `ALL_BOOT_FORMS` turns it red. It does **not** detect a new boot entry
    /// path elsewhere in the tree — the registry is not the production path
    /// map.
    #[test]
    fn test_boot_form_count_is_accounted() {
        assert_eq!(
            ALL_BOOT_FORMS.len(),
            5,
            "启动形态家族数量与 §SD-1 记账不符；新增/删除形态须同步更新 ALL_BOOT_FORMS"
        );
    }

    /// Every form has a non-empty landing path and the landing paths are
    /// distinct. Each path is checked for existence on disk; for the four
    /// live forms that catches a move or rename, while for the retired
    /// inline form it only checks that its old carrier file still exists.
    #[test]
    fn test_each_boot_form_has_distinct_source() {
        // `source_path()` is a repo-root-relative path; this package's
        // manifest directory is os/libs/minix-boot, so the repo root is
        // three levels above the manifest directory.
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for form in ALL_BOOT_FORMS {
            assert!(
                !form.source_path().is_empty(),
                "启动形态 {:?} 缺源码落点",
                form
            );
            assert!(
                repo_root.join(form.source_path()).exists(),
                "启动形态 {:?} 的落点不存在：{}",
                form,
                form.source_path()
            );
        }
        for (i, a) in ALL_BOOT_FORMS.iter().enumerate() {
            for b in &ALL_BOOT_FORMS[i + 1..] {
                assert_ne!(
                    a.source_path(),
                    b.source_path(),
                    "形态 {:?}/{:?} 落点撞车",
                    a,
                    b
                );
            }
        }
    }

    /// Build a `KernelInfo` with generic higher-half geometry (absolute
    /// addresses are firmware-dependent, so what is required is cross-form
    /// postcondition **equivalence**, not entry-structure equality — the
    /// same set of internal relations is enough to cover the predicate
    /// every form must satisfy).
    fn canonical_higher_half_info() -> KernelInfo {
        KernelInfo {
            memmap: &[MemoryRegion {
                base: PhysBytes(0x100000),
                len: 0x1000000,
            }],
            reserved_regions: &[],
            memmap_sources: &[],
            startup_objects: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200000),
            kern_size: 0x200000,
            user_sp: VirBytes(USER_STACK_TOP as u64),
            // 与 builder 派生式一致：kern_stack_top = kern_virt_base + kern_size
            // （D01-R1 D2-04 上界断言后，夹具几何必须满足它自己的构造不变量）。
            kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[BootModule {
                name: "vm",
                start: PhysBytes(0x400000),
                len: 0x100000,
            }],
            platform_sources: &[],
            param_buf: &[],
        }
    }

    /// The equivalence battery: the shared postcondition predicate is checked
    /// against one canonical higher-half geometry, once per registry member.
    /// This proves the predicate is independent of the enum variant; it does
    /// not construct any form's real `KernelInfo` (no per-form constructor
    /// exists), so it does not exercise the forms' entry paths.
    #[test]
    fn test_boot_postconditions_hold_for_every_form() {
        for form in ALL_BOOT_FORMS {
            canonical_higher_half_info().validate();
            canonical_higher_half_info().check_boot_postconditions();
            let _ = form.label();
        }
    }

    /// Negative control: the entry points outside the image (the
    /// `entry_start_pa` class of typo) — the postcondition predicate must
    /// turn red on the spot, while `validate()` (which only checks
    /// non-address invariants) cannot catch it, which is exactly this
    /// layer's independent value.
    #[test]
    #[should_panic(expected = "syscall_entry")]
    fn test_postcond_rejects_entry_outside_image() {
        let mut info = canonical_higher_half_info();
        info.syscall_entry = VirBytes(0xFFFF_8000_00FF_0000); // past base+kern_size
        info.check_boot_postconditions();
    }

    /// Negative control: the user stack reaches up so high that it overlaps
    /// the kernel half (the shape of that class of out-of-range bug) — must
    /// turn red.
    #[test]
    #[should_panic(expected = "user_sp")]
    fn test_postcond_rejects_user_sp_in_kernel_half() {
        let mut info = canonical_higher_half_info();
        info.user_sp = VirBytes(0xFFFF_8000_0050_0000);
        info.check_boot_postconditions();
    }

    /// Negative control: the kernel stack drops into the user half — must
    /// turn red.
    #[test]
    #[should_panic(expected = "kern_stack_top")]
    fn test_postcond_rejects_kernel_stack_below_base() {
        let mut info = canonical_higher_half_info();
        info.kern_stack_top = VirBytes(0x1000_0000);
        info.check_boot_postconditions();
    }
}
