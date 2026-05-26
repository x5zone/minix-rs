//! x86-64 protection mechanism implementation
//!
//! Implements `ProtectionArch` for x86-64, managing GDT (Global Descriptor
//! Table) and TSS (Task State Segment) structures.
//!
//! # 64-bit long mode changes from 32-bit (see 04-protection.md §3.4)
//!
//! - Code/data segment descriptors are flat (base=0, limit=full address space)
//! - LDT removed — not used in 64-bit mode
//! - TSS is 64-bit format (no general-purpose/segment registers, adds IST1-7)
//! - SYSENTER removed — only SYSCALL/SYSRET in 64-bit mode

use crate::protection::{ProtectionArch, Privilege};
use minix_types::VirBytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X86PrivilegeLevel(u8);

impl X86PrivilegeLevel {
    pub const RING0: Self = Self(0);
    pub const RING3: Self = Self(3);

    pub(crate) const fn get(self) -> u8 {
        self.0
    }
}

const GDT_NULL_INDEX: usize = 0;
const GDT_KERN_CS_INDEX: usize = 1;
const GDT_KERN_DS_INDEX: usize = 2;
const GDT_USER_CS_INDEX: usize = 3;
const GDT_USER_DS_INDEX: usize = 4;
const GDT_TSS_FIRST_INDEX: usize = 5;

pub(crate) const KERN_CS_SELECTOR: u16 = (GDT_KERN_CS_INDEX * 8) as u16;
pub(crate) const KERN_DS_SELECTOR: u16 = (GDT_KERN_DS_INDEX * 8) as u16;
pub(crate) const USER_CS_SELECTOR: u16 = ((GDT_USER_CS_INDEX * 8) | 3) as u16;
pub(crate) const USER_DS_SELECTOR: u16 = ((GDT_USER_DS_INDEX * 8) | 3) as u16;

const MAX_CPUS: usize = 8;

const GDT_ENTRIES: usize = GDT_TSS_FIRST_INDEX + MAX_CPUS;

const TSS64_SIZE: usize = 104;

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct Tss64 {
    _reserved0: u32,
    sp0: u64,
    sp1: u64,
    sp2: u64,
    _reserved1: u64,
    ist: [u64; 7],
    _reserved2: u64,
    _reserved3: u16,
    iobase: u16,
}

impl Tss64 {
    const fn zeroed() -> Self {
        Self {
            _reserved0: 0,
            sp0: 0,
            sp1: 0,
            sp2: 0,
            _reserved1: 0,
            ist: [0; 7],
            _reserved2: 0,
            _reserved3: 0,
            iobase: 0x8000,
        }
    }
}

#[repr(C, packed)]
struct DescTablePtr {
    limit: u16,
    base: u64,
}

fn make_seg_desc(base: u32, limit: u32, access: u8, granularity: u8) -> u64 {
    (limit as u64 & 0xFFFF)
        | ((base as u64 & 0xFFFF) << 16)
        | (((base >> 16) as u64 & 0xFF) << 32)
        | ((access as u64) << 40)
        | ((granularity as u64 & 0x0F) << 52)
        | (((limit >> 16) as u64 & 0x0F) << 48)
        | (((base >> 24) as u64 & 0xFF) << 56)
}

fn make_tss_desc64(tss_addr: u64, limit: u16, dpl: u8) -> [u64; 2] {
    let access = 0x89 | ((dpl & 0x3) << 5);
    let low = (limit as u64 & 0xFFFF)
        | ((tss_addr & 0xFFFF) << 16)
        | (((tss_addr >> 16) & 0xFF) << 32)
        | ((access as u64) << 40)
        | (((tss_addr >> 24) & 0xFF) << 56);
    let high = (tss_addr >> 32) & 0xFFFFFFFF;
    [low, high]
}

pub struct X86_64Protection {
    gdt: [u64; GDT_ENTRIES],
    tss: [Tss64; MAX_CPUS],
    tss_desc_high: [u64; MAX_CPUS],
    cpu_count: u32,
}

impl X86_64Protection {
    fn fill_flat_segments(&mut self) {
        self.gdt[GDT_NULL_INDEX] = 0;

        self.gdt[GDT_KERN_CS_INDEX] = make_seg_desc(
            0, 0xFFFFF,
            0x9A,
            0xA,
        );

        self.gdt[GDT_KERN_DS_INDEX] = make_seg_desc(
            0, 0xFFFFF,
            0x92,
            0xC,
        );

        self.gdt[GDT_USER_CS_INDEX] = make_seg_desc(
            0, 0xFFFFF,
            0xFA,
            0xA,
        );

        self.gdt[GDT_USER_DS_INDEX] = make_seg_desc(
            0, 0xFFFFF,
            0xF2,
            0xC,
        );
    }

    fn setup_tss_for_cpu(&mut self, cpu_id: u32, kernel_stack_top: VirBytes) {
        let idx = cpu_id as usize;
        if idx >= MAX_CPUS {
            return;
        }

        self.tss[idx] = Tss64::zeroed();
        self.tss[idx].sp0 = kernel_stack_top.get();
        self.tss[idx].iobase = (TSS64_SIZE + 8) as u16;

        let tss_addr = &self.tss[idx] as *const Tss64 as u64;
        let desc = make_tss_desc64(tss_addr, TSS64_SIZE as u16 - 1, 0);

        let gdt_idx = GDT_TSS_FIRST_INDEX + idx;
        self.gdt[gdt_idx] = desc[0];
        self.tss_desc_high[idx] = desc[1];
    }
}

impl ProtectionArch for X86_64Protection {
    type PrivilegeLevel = X86PrivilegeLevel;

    const KERNEL_PRIVILEGE: X86PrivilegeLevel = X86PrivilegeLevel::RING0;
    const USER_PRIVILEGE: X86PrivilegeLevel = X86PrivilegeLevel::RING3;

    fn to_privilege(level: X86PrivilegeLevel) -> Privilege {
        match level {
            X86PrivilegeLevel::RING0 => Privilege::Kernel,
            _ => Privilege::User,
        }
    }

    fn from_privilege(privilege: Privilege) -> X86PrivilegeLevel {
        match privilege {
            Privilege::Kernel => X86PrivilegeLevel::RING0,
            Privilege::User => X86PrivilegeLevel::RING3,
        }
    }

    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        let mut prot = Self {
            gdt: [0u64; GDT_ENTRIES],
            tss: [Tss64::zeroed(); MAX_CPUS],
            tss_desc_high: [0u64; MAX_CPUS],
            cpu_count: 0,
        };

        prot.fill_flat_segments();
        prot.setup_tss_for_cpu(cpu_id, kernel_stack_top);
        prot.cpu_count = cpu_id + 1;

        prot
    }

    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes) {
        let idx = cpu_id as usize;
        if idx < MAX_CPUS {
            self.tss[idx].sp0 = stack_top.get();
        }
    }

    fn load(&self) {
        let gdtr = DescTablePtr {
            limit: (core::mem::size_of_val(&self.gdt) - 1) as u16,
            base: self.gdt.as_ptr() as u64,
        };

        unsafe {
            core::arch::asm!(
                "lgdt [{gdtr}]",
                gdtr = in(reg) &gdtr,
                options(readonly, nostack, preserves_flags)
            );

            let _tmp_cs: u64;
            core::arch::asm!(
                "mov {cs}, {sel}",
                "push {cs}",
                "lea {addr}, [2f + rip]",
                "push {addr}",
                "retfq",
                "2:",
                sel = const KERN_CS_SELECTOR as u64,
                cs = out(reg) _tmp_cs,
                addr = out(reg) _,
                options(nostack),
            );

            let sel_ds: u16 = KERN_DS_SELECTOR;
            core::arch::asm!(
                "mov ds, {0:x}",
                "mov es, {0:x}",
                "mov ss, {0:x}",
                in(reg) sel_ds,
                options(nostack, preserves_flags),
            );

            core::arch::asm!(
                "xor {0:e}, {0:e}",
                "mov fs, {0:x}",
                "mov gs, {0:x}",
                out(reg) _,
                options(nostack, preserves_flags)
            );

            let tr_sel: u16 = (GDT_TSS_FIRST_INDEX * 8) as u16;
            core::arch::asm!(
                "ltr {0:x}",
                in(reg) tr_sel,
                options(nostack, preserves_flags)
            );
        }
    }

    // TODO: implement AP initialization — per-CPU TSS setup + selector loading
    // C: tss_init(cpu, stack) + prot_load_selectors() — called from mpx.S
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        let _ = (cpu_id, kernel_stack_top);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tss64_size_is_104_bytes() {
        assert_eq!(
            core::mem::size_of::<Tss64>(),
            104,
            "Tss64 must be 104 bytes per Intel SDM Vol. 3A §7.7"
        );
    }

    #[test]
    fn tss64_offsets_correct() {
        assert_eq!(
            core::mem::offset_of!(Tss64, sp0),
            4,
            "sp0 at offset 4"
        );
        assert_eq!(
            core::mem::offset_of!(Tss64, ist),
            36,
            "IST array at offset 36"
        );
        assert_eq!(
            core::mem::offset_of!(Tss64, iobase),
            102,
            "iobase at offset 102"
        );
    }

    #[test]
    fn segment_selectors_correct() {
        assert_eq!(KERN_CS_SELECTOR, 0x08);
        assert_eq!(KERN_DS_SELECTOR, 0x10);
        assert_eq!(USER_CS_SELECTOR, 0x1B);
        assert_eq!(USER_DS_SELECTOR, 0x23);
    }

    #[test]
    fn privilege_level_roundtrip() {
        assert_eq!(
            X86_64Protection::to_privilege(X86PrivilegeLevel::RING0),
            crate::protection::Privilege::Kernel
        );
        assert_eq!(
            X86_64Protection::to_privilege(X86PrivilegeLevel::RING3),
            crate::protection::Privilege::User
        );
        assert_eq!(
            X86_64Protection::from_privilege(crate::protection::Privilege::Kernel),
            X86PrivilegeLevel::RING0
        );
        assert_eq!(
            X86_64Protection::from_privilege(crate::protection::Privilege::User),
            X86PrivilegeLevel::RING3
        );
    }
}
