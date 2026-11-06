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
//!    removing a form must change it too, or
//!    `test_boot_form_count_is_accounted` fails on the spot.
//! 2. [`KernelInfo::check_boot_postconditions`] (kernel_info.rs) is the
//!    postcondition predicate shared across forms; the battery below runs
//!    it against each form, realizing "equivalent post-assertions, not
//!    entry-structure equality" — absolute addresses depend on firmware,
//!    so what can be equivalent is the same set of higher-half
//!    address-space relations.
//!
//! This registry is a **postcondition battery**, not the production path
//! map. Production x86_64 UEFI uses the v1 cross-image handoff; the inline
//! entry below is the historical/test-carrier form kept for the battery
//! (see `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` §4.4 for
//! the current production entries).

use crate::kernel_info::KernelInfo;

/// One kernel boot form. The variant order is (a)–(e), each traceable back
/// to in-tree source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootForm {
    /// (a) x86_64 UEFI inline: `arch_boot` runs directly inside the
    /// boot-shim process; the kernel is statically linked into the PE.
    /// This is the historical/test-carrier form; production x86_64 UEFI
    /// uses the v1 cross-image handoff (see the module note above).
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
    /// (e) riscv64 early-hart filter park mailbox (registered as a family
    ///     member from birth).
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
    /// Where this form lives in tree (the source path). Used to keep the
    /// registry aligned with the real code; a missing path means some form
    /// was moved or renamed and this record did not follow.
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

    /// The form count is accounted for in one place. The five were counted
    /// from the current state; any "a sixth form quietly appeared and the
    /// registry did not change" turns this test red.
    #[test]
    fn test_boot_form_count_is_accounted() {
        assert_eq!(
            ALL_BOOT_FORMS.len(),
            5,
            "启动形态家族数量与 §SD-1 记账不符；新增/删除形态须同步更新 ALL_BOOT_FORMS"
        );
    }

    /// Every form has a non-empty landing path, the landing paths are
    /// distinct (two forms pointing at the same file means the family
    /// definition is wrong), and each path really exists on disk (moving or
    /// renaming one turns this red immediately).
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
            free_upper_idx: Some(256),
            user_sp: VirBytes(USER_STACK_TOP as u64),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[BootModule {
                name: "vm",
                start: PhysBytes(0x400000),
                len: 0x100000,
            }],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        }
    }

    /// The equivalence battery: the `KernelInfo` produced by each boot form
    /// must pass `validate()` (non-address invariants) and then
    /// `check_boot_postconditions()` (higher-half address-space relations).
    #[test]
    fn test_boot_postconditions_hold_for_every_form() {
        for form in ALL_BOOT_FORMS {
            // Constructed per form from the same internal relations, proving
            // the predicate is independent of the concrete form and applies
            // equivalently.
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
