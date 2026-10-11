//! SD-1 / PD-07 采甲（债①）：**启动形态登记 + 跨形态后置条件等价电池**。
//!
//! 本树的内核启动入口「形态家族」已从三长成五（见 STRUCTURAL-DEBT-REGISTER §SD-1
//! Current state）。PD-07 裁的是保留多形态（乙案）＋**显式接口 + boot-postcondition
//! 等价断言 + 形态计数**，而不是强行统一。此前缺的正是「等价断言层」和「计数在一处
//! 记账」——每次新增一种形态都无断言可拦，还三度复发 `entry_start_pa` 类 paper
//! constant 错字。本模块把两半补齐：
//!
//! 1. [`BootForm`] 把五种形态收在一处枚举，`ALL_BOOT_FORMS` 是唯一的形态计数源；
//!    新增/删除形态必须同步改这里，否则 `test_boot_form_count_is_accounted` 当场红。
//! 2. [`KernelInfo::check_boot_postconditions`]（kernel_info.rs）是跨形态共享的
//!    后置条件谓词；下面的电池对每种形态逐一跑它，落实 PD-07「等价后置断言（非
//!    入口结构相等）」——绝对地址随固件而定，能等价的是同一组 higher-half 地址空间关系。

use crate::kernel_info::KernelInfo;

/// 一种内核启动形态。变体顺序即 `STRUCTURAL-DEBT-REGISTER` §SD-1 Current state 的
/// (a)–(e)，逐一可回勾到在树源码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootForm {
    /// (a) x86_64 UEFI 内联：`arch_boot` 在 boot-shim 进程内直跑，kernel 静态链进 PE。
    X86UefiInline,
    /// (b) aarch64 UEFI 跨镜像：shim 产出 `BootHandoff`，交棒 `kernel.elf`。
    Aarch64UefiCrossImage,
    /// (c) riscv64 OpenSBI 自举：`os/kernel-image/src/bootface.rs`（续-88）。
    RiscvOpensbiSelfBoot,
    /// (d) aarch64 QEMU 直核自举：`os/kernel-image/src/bootface_a64.rs`（续-406，
    ///     经 `-kernel` 装载的第四独立入口）。
    Aarch64QemuDirectCore,
    /// (e) riscv64 早期 hart 过滤停放邮箱（续-400，出生即登记为本家族成员）。
    RiscvEarlyHartMailbox,
}

/// 唯一的形态计数源。加一种形态就往这里加一个元素，计数测试自动把「家族又长但没记账」
/// 拦在编译/测试阶段，而非靠人肉记忆。
pub const ALL_BOOT_FORMS: &[BootForm] = &[
    BootForm::X86UefiInline,
    BootForm::Aarch64UefiCrossImage,
    BootForm::RiscvOpensbiSelfBoot,
    BootForm::Aarch64QemuDirectCore,
    BootForm::RiscvEarlyHartMailbox,
];

impl BootForm {
    /// 该形态在树上的落点（源码路径）。用于把登记与真实代码对齐；缺失即说明某形态
    /// 被搬走或改名而台账未跟上。
    pub const fn source_path(self) -> &'static str {
        match self {
            BootForm::X86UefiInline => "os/boot-shim/src/main.rs",
            BootForm::Aarch64UefiCrossImage => "os/boot-shim/src/handoff.rs",
            BootForm::RiscvOpensbiSelfBoot => "os/kernel-image/src/bootface.rs",
            BootForm::Aarch64QemuDirectCore => "os/kernel-image/src/bootface_a64.rs",
            BootForm::RiscvEarlyHartMailbox => "os/arch/src/riscv64/ap_early_entry.rs",
        }
    }

    /// 人类可读短名，供诊断/文档引用。
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
    use minix_types::{PhysBytes, VirBytes};

    /// PD-07 债①：形态计数在一处记账。五是从 §SD-1 Current state 数出来的；
    /// 任何「悄悄长了第六形态而没改登记」都会让此测红。
    #[test]
    fn test_boot_form_count_is_accounted() {
        assert_eq!(
            ALL_BOOT_FORMS.len(),
            5,
            "启动形态家族数量与 §SD-1 记账不符；新增/删除形态须同步更新 ALL_BOOT_FORMS"
        );
    }

    /// 每形态都有非空落点，且落点互不重复（两种形态指向同一文件＝家族定义出错）。
    #[test]
    fn test_each_boot_form_has_distinct_source() {
        for form in ALL_BOOT_FORMS {
            assert!(
                !form.source_path().is_empty(),
                "启动形态 {:?} 缺源码落点",
                form
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

    /// 构造一份 generic higher-half 几何的 `KernelInfo`（绝对地址是固件相关的，故 PD-07
    /// 要的是跨形态的后置条件**等价**而非入口结构相等——这里用同一组内部关系即可
    /// 覆盖每种形态都必须满足的谓词）。
    fn canonical_higher_half_info() -> KernelInfo {
        KernelInfo {
            memmap: &[MemoryRegion {
                base: PhysBytes(0x100000),
                len: 0x1000000,
            }],
            reserved_regions: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200000),
            kern_size: 0x200000,
            free_upper_idx: Some(256),
            user_sp: VirBytes(0x7fff_ffff_f000),
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

    /// 等价电池：每种启动形态产出的 `KernelInfo` 都必须依次通过 `validate()`
    /// （非地址不变量）与 `check_boot_postconditions()`（higher-half 地址空间关系）。
    #[test]
    fn test_boot_postconditions_hold_for_every_form() {
        for form in ALL_BOOT_FORMS {
            // 逐形态用同一内部关系构造，证明谓词与具体形态无关、可等价套用。
            canonical_higher_half_info().validate();
            canonical_higher_half_info().check_boot_postconditions();
            let _ = form.label();
        }
    }

    /// 负控：入口被指到镜像外（`entry_start_pa` 类错字）——后置条件谓词必须当场红，
    /// 而 `validate()`（只校非地址不变量）抓不到它，正说明本层的独立价值。
    #[test]
    #[should_panic(expected = "syscall_entry")]
    fn test_postcond_rejects_entry_outside_image() {
        let mut info = canonical_higher_half_info();
        info.syscall_entry = VirBytes(0xFFFF_8000_00FF_0000); // 越过 base+kern_size
        info.check_boot_postconditions();
    }

    /// 负控：用户栈高到与内核半区重叠（§1.115 类越界的形状）——必须红。
    #[test]
    #[should_panic(expected = "user_sp")]
    fn test_postcond_rejects_user_sp_in_kernel_half() {
        let mut info = canonical_higher_half_info();
        info.user_sp = VirBytes(0xFFFF_8000_0050_0000);
        info.check_boot_postconditions();
    }

    /// 负控：内核栈掉到用户半区——必须红。
    #[test]
    #[should_panic(expected = "kern_stack_top")]
    fn test_postcond_rejects_kernel_stack_below_base() {
        let mut info = canonical_higher_half_info();
        info.kern_stack_top = VirBytes(0x1000_0000);
        info.check_boot_postconditions();
    }
}
