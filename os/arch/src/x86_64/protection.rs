//! x86-64 protection mechanism implementation
//!
//! Implements `ProtectionArch` for x86-64, managing GDT (Global Descriptor
//! Table) and TSS (Task State Segment) structures.
//!
//! # x86-64 specific: GDT/TSS in long mode
//!
//! See `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md`
//! §1.7 ("x86 为什么还保留 GDT") for the architectural rationale of why
//! GDT/TSS are still required in 64-bit long mode: the TSS descriptor
//! must be referenced via a GDT entry (an ISA constraint), so GDT cannot
//! be eliminated entirely. This is INTENTIONALLY hidden from OS code via
//! the `ProtectionArch` trait — the OS calls `set_kernel_stack()` and
//! `load()`, never `lgdt` / `ltr` / TSS descriptor writes directly.
//!
//! # 64-bit long mode specifics
//!
//! - Code/data segment descriptors are flat (base=0, limit=full address
//!   space); segment-based memory isolation is replaced by paging
//! - LDT removed — not used in 64-bit mode
//! - TSS is 64-bit format (no general-purpose/segment register save area,
//!   adds IST1-7 for Interrupt Stack Table)
//! - SYSENTER removed — only SYSCALL/SYSRET in 64-bit mode
//!
//! # How the OS-level concerns map to x86-64 mechanisms
//!
//! | OS concern (trait method) | x86-64 mechanism (this file's impl) |
//! |---------------------------|--------------------------------------|
//! | `init` (establish protection) | Clear GDT, fill segment descriptors (DPL), create TSS |
//! | `set_kernel_stack` (switch kernel stack) | Write TSS.sp0 |
//! | `load` (make protection effective) | `lgdt` (GDTR) + `ltr` (TR) + reload segment registers |
//! | `init_ap` (AP startup) | Per-CPU TSS/GDT entry, load selectors (no global GDT sync) |

use crate::protection::{ProtectionArch, Privilege};
use minix_types::VirBytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X86PrivilegeLevel(u8);

impl X86PrivilegeLevel {
    pub(crate) const RING0: Self = Self(0);
    pub(crate) const RING3: Self = Self(3);

    #[allow(dead_code)] // accessor; not yet wired to all call sites
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
#[allow(dead_code)] // user data segment selector; not yet wired to all call sites
pub(crate) const USER_DS_SELECTOR: u16 = ((GDT_USER_DS_INDEX * 8) | 3) as u16;

const MAX_CPUS: usize = 8;

const GDT_ENTRIES: usize = GDT_TSS_FIRST_INDEX + MAX_CPUS;

const TSS64_SIZE: usize = 104;

// Number of bytes reserved at the top of each kernel stack for per-CPU
// metadata. C reserves 2 * sizeof(reg_t): one slot for the currently
// scheduled process pointer and one slot for the CPU id
// (protect.c:173-181; archconst.h:146-149). For x86-64 reg_t is 64-bit,
// so the reserved area is 16 bytes.
const X86_64_STACK_TOP_RESERVED: usize = 2 * core::mem::size_of::<u64>();

// Segment descriptor access byte encoding (Intel SDM Vol. 3A §3.4.5)
// Bits: Present(7) | DPL(6:5) | S(4) | Type(3:0)
const KERN_CS_ACCESS: u8 = 0x9A; // Present|DPL0|Code|Read
const KERN_DS_ACCESS: u8 = 0x92; // Present|DPL0|Data|Write
const USER_CS_ACCESS: u8 = 0xFA; // Present|DPL3|Code|Read
const USER_DS_ACCESS: u8 = 0xF2; // Present|DPL3|Data|Write

// TSS descriptor access byte (Intel SDM Vol. 3A §7.2.3)
// Bits: Present(7) | DPL(6:5) | 0(4) | Type=1001(3:0) = 64-bit TSS
const TSS64_ACCESS: u8 = 0x89; // Present|DPL0|64-bit TSS

// Granularity byte encoding (Intel SDM Vol. 3A §3.4.5)
// Only the upper 4 bits of byte 6 (G/D/B/L/AVL); Limit[19:16] is
// taken from the `limit` parameter in make_seg_desc().
// Bit layout: G(3) | D/B(2) | L(1) | AVL(0)
const KERN_CS_GRANULARITY: u8 = 0xA; // G=1|L=1 (64-bit code, page granularity)
const DS_GRANULARITY: u8 = 0xC;      // G=1|B=1 (32-bit expand-up data, page granularity)
const USER_CS_GRANULARITY: u8 = 0xA; // G=1|L=1 (64-bit code, page granularity)

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

/// Construct a 64-bit flat segment descriptor.
///
/// `base` is u32 per Intel SDM: segment descriptor base is 32-bit even in
/// long mode (64-bit code segments ignore base, always treating it as 0).
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
    let access = TSS64_ACCESS | ((dpl & 0x3) << 5);
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
    /// CPU ID passed to `init()`. `load()` uses this to select the correct
    /// TSS descriptor for the boot CPU instead of hardcoding BSP (cpu 0).
    boot_cpu_id: u32,
}

// SAFETY: X86_64Protection is Send because:
// - All fields are plain data (u64 arrays, u32) with no interior mutability.
// - During boot, init() is called on BSP only (single-writer).
// - During SMP bringup, init_ap() is called per-CPU under BKL protection.
// - After initialization, the struct is only read (load() reads GDT/TSS).
//   set_kernel_stack() mutates TSS.sp0 but is called under BKL.
// X86_64Protection is Sync because:
// - All mutations (init, set_kernel_stack) are protected by BKL.
// - Reads (load) are safe to race with each other.
unsafe impl Send for X86_64Protection {}
unsafe impl Sync for X86_64Protection {}

impl X86_64Protection {
    fn fill_flat_segments(&mut self) {
        self.gdt[GDT_NULL_INDEX] = 0;

        self.gdt[GDT_KERN_CS_INDEX] = make_seg_desc(
            0, 0xFFFFF,
            KERN_CS_ACCESS,
            KERN_CS_GRANULARITY,
        );

        self.gdt[GDT_KERN_DS_INDEX] = make_seg_desc(
            0, 0xFFFFF,
            KERN_DS_ACCESS,
            DS_GRANULARITY,
        );

        self.gdt[GDT_USER_CS_INDEX] = make_seg_desc(
            0, 0xFFFFF,
            USER_CS_ACCESS,
            USER_CS_GRANULARITY,
        );

        self.gdt[GDT_USER_DS_INDEX] = make_seg_desc(
            0, 0xFFFFF,
            USER_DS_ACCESS,
            DS_GRANULARITY,
        );
    }

    fn setup_tss_for_cpu(&mut self, cpu_id: u32, kernel_stack_top: VirBytes) {
        let idx = cpu_id as usize;
        assert!(idx < MAX_CPUS, "cpu_id {} exceeds MAX_CPUS {}", cpu_id, MAX_CPUS);

        self.tss[idx] = Tss64::zeroed();

        // C: tss_init() reserves the top 2 * sizeof(reg_t) bytes of the kernel
        // stack for the currently-scheduled process pointer and the CPU id
        // (protect.c:173-181). sp0 points to the first usable word below that
        // reserved area.
        let usable_top = kernel_stack_top.get() - X86_64_STACK_TOP_RESERVED as u64;
        self.tss[idx].sp0 = usable_top;

        // Store the CPU id at the top of the reserved area, matching C:
        // *((reg_t *)(sp0 + sizeof(reg_t))) = cpu
        // This is read by the assembly trap entry to determine which CPU's
        // stack is in use.
        // SAFETY: kernel_stack_top is a valid, aligned virtual address at the
        // top of the boot CPU's stack. We write only within the reserved area
        // and run single-threaded during boot before concurrent access is
        // possible. Skipped in unit tests because the addresses are mock values.
        #[cfg(not(test))]
        unsafe {
            let cpu_id_slot = (usable_top + core::mem::size_of::<u64>() as u64) as *mut u64;
            cpu_id_slot.write(cpu_id as u64);
        }

        // iobase = 0x8000 disables I/O permission bitmap per Intel SDM Vol. 3A §7.7:
        // "If the I/O Map Base Address ≥ TSS limit, no I/O permission map exists."
        // C (i386): iobase = sizeof(struct tss_s) = 104 (no I/O bitmap in 32-bit).
        // 64-bit TSS uses 0x8000 for consistency — any value >= TSS limit works.

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
            boot_cpu_id: cpu_id,
        };

        prot.fill_flat_segments();
        prot.setup_tss_for_cpu(cpu_id, kernel_stack_top);
        prot.cpu_count = cpu_id + 1;

        prot
    }

    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes) {
        let idx = cpu_id as usize;
        assert!(idx < MAX_CPUS, "cpu_id {} exceeds MAX_CPUS {}", cpu_id, MAX_CPUS);
        self.tss[idx].sp0 = stack_top.get();
    }

    fn load(&self) {
        let gdtr = DescTablePtr {
            limit: (core::mem::size_of_val(&self.gdt) - 1) as u16,
            base: self.gdt.as_ptr() as u64,
        };

        unsafe {
            // 1. Load GDTR — makes the new GDT active.
            //    SAFETY: gdtr points to a valid DescTablePtr on the stack,
            //    and self.gdt contains a valid GDT initialized by init().
            core::arch::asm!(
                "lgdt [{gdtr}]",
                gdtr = in(reg) &gdtr,
                options(readonly, nostack, preserves_flags)
            );

            // 2. Far jump to reload CS with the kernel code segment selector.
            //    This is required after lgdt — the CPU continues using the
            //    old CS descriptor until explicitly reloaded.
            //    SAFETY: KERN_CS_SELECTOR points to a valid code segment
            //    descriptor in the new GDT.
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

            // 3. Reload data segment registers (DS, ES, SS).
            //    SAFETY: KERN_DS_SELECTOR points to a valid data segment
            //    descriptor in the new GDT.
            let sel_ds: u16 = KERN_DS_SELECTOR;
            core::arch::asm!(
                "mov ds, {0:x}",
                "mov es, {0:x}",
                "mov ss, {0:x}",
                in(reg) sel_ds,
                options(nostack, preserves_flags),
            );

            // 4. Clear FS/GS (unused in 64-bit kernel).
            core::arch::asm!(
                "xor {0:e}, {0:e}",
                "mov fs, {0:x}",
                "mov gs, {0:x}",
                out(reg) _,
                options(nostack, preserves_flags)
            );

            // 5. Load Task Register (TR) with the boot CPU's TSS selector.
            //    SAFETY: GDT_TSS_FIRST_INDEX + boot_cpu_id points to a valid
            //    64-bit TSS descriptor set up by setup_tss_for_cpu().
            let tr_sel: u16 = ((GDT_TSS_FIRST_INDEX + self.boot_cpu_id as usize) * 8) as u16;
            core::arch::asm!(
                "ltr {0:x}",
                in(reg) tr_sel,
                options(nostack, preserves_flags)
            );

            // Note: C loads LDTR via lldt(LDT_SELECTOR), but 64-bit mode
            // does not use LDT. The LDT descriptor is filled in the GDT
            // for 32-bit compatibility but never loaded on 64-bit.
            // C: x86_lldt(LDT_SELECTOR) — protect.c:313
        }
    }

    // AP initialization is not yet implemented for x86-64. The trait requires
    // the method, but SMP bringup is out of scope for the current milestone.
    // When called, panic immediately instead of silently doing nothing —
    // an AP with an unloaded TSS would triple-fault on its first exception.
    // C: tss_init(cpu, stack) + prot_load_selectors() — called from mpx.S
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        panic!(
            "init_ap({}) not implemented for x86-64; cannot set up TSS/selector for stack_top={:?}",
            cpu_id, kernel_stack_top
        );
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

    #[test]
    fn gdt_descriptors_have_correct_dpl() {
        // Verify that kernel segments have DPL=0 and user segments have DPL=3.
        // Access byte bits[6:5] = DPL. KERN_CS_ACCESS=0x9A → DPL=0,
        // USER_CS_ACCESS=0xFA → DPL=3.
        let kern_cs_dpl = (KERN_CS_ACCESS >> 5) & 0x3;
        let kern_ds_dpl = (KERN_DS_ACCESS >> 5) & 0x3;
        let user_cs_dpl = (USER_CS_ACCESS >> 5) & 0x3;
        let user_ds_dpl = (USER_DS_ACCESS >> 5) & 0x3;

        assert_eq!(kern_cs_dpl, 0, "Kernel CS DPL must be 0 (Ring 0)");
        assert_eq!(kern_ds_dpl, 0, "Kernel DS DPL must be 0 (Ring 0)");
        assert_eq!(user_cs_dpl, 3, "User CS DPL must be 3 (Ring 3)");
        assert_eq!(user_ds_dpl, 3, "User DS DPL must be 3 (Ring 3)");
    }

    #[test]
    fn gdt_descriptors_are_flat_mode() {
        // Verify flat mode: code segments have L bit set (64-bit),
        // data segments have B bit set (32-bit expand-up).
        // Granularity constants use 4-bit layout: G(3) | D/B(2) | L(1) | AVL(0)
        let kern_cs_l_bit = (KERN_CS_GRANULARITY >> 1) & 1;
        let kern_cs_g_bit = (KERN_CS_GRANULARITY >> 3) & 1;
        let user_cs_l_bit = (USER_CS_GRANULARITY >> 1) & 1;
        let user_cs_g_bit = (USER_CS_GRANULARITY >> 3) & 1;
        let kern_ds_b_bit = (DS_GRANULARITY >> 2) & 1;
        let kern_ds_g_bit = (DS_GRANULARITY >> 3) & 1;
        let user_ds_b_bit = (DS_GRANULARITY >> 2) & 1;
        let user_ds_g_bit = (DS_GRANULARITY >> 3) & 1;

        assert_eq!(kern_cs_l_bit, 1, "Kernel CS must be 64-bit (L=1)");
        assert_eq!(kern_cs_g_bit, 1, "Kernel CS must have page granularity (G=1)");
        assert_eq!(user_cs_l_bit, 1, "User CS must be 64-bit (L=1)");
        assert_eq!(user_cs_g_bit, 1, "User CS must have page granularity (G=1)");
        assert_eq!(kern_ds_b_bit, 1, "Kernel DS must be 32-bit expand-up (B=1)");
        assert_eq!(kern_ds_g_bit, 1, "Kernel DS must have page granularity (G=1)");
        assert_eq!(user_ds_b_bit, 1, "User DS must be 32-bit expand-up (B=1)");
        assert_eq!(user_ds_g_bit, 1, "User DS must have page granularity (G=1)");
    }

    #[test]
    fn set_kernel_stack_updates_sp0() {
        // Verify that set_kernel_stack writes to the correct TSS entry.
        // Use addr_of! + read_unaligned because Tss64 is #[repr(C, packed)].
        // init() subtracts X86_64_STACK_TOP_RESERVED from the supplied top.
        let stack_top = 0x8000u64;
        let mut prot = X86_64Protection::init(0, VirBytes::new(stack_top));
        let sp0_ptr = core::ptr::addr_of!(prot.tss[0].sp0);
        assert_eq!(
            unsafe { core::ptr::read_unaligned(sp0_ptr) },
            stack_top - X86_64_STACK_TOP_RESERVED as u64,
            "Initial sp0 should be below the reserved area"
        );

        prot.set_kernel_stack(0, VirBytes::new(0xA000));
        assert_eq!(
            unsafe { core::ptr::read_unaligned(sp0_ptr) },
            0xA000,
            "sp0 should be updated to the caller-supplied usable top"
        );
    }

    #[test]
    #[should_panic(expected = "cpu_id")]
    fn set_kernel_stack_panics_on_invalid_cpu_id() {
        let mut prot = X86_64Protection::init(0, VirBytes::new(0x8000));
        prot.set_kernel_stack(MAX_CPUS as u32, VirBytes::new(0xA000));
    }

    #[test]
    fn tss_descriptor_is_64bit() {
        // TSS64_ACCESS=0x89: bit 4=0 (system segment), bits[3:0]=1001 (64-bit TSS)
        let type_field = TSS64_ACCESS & 0xF;
        let s_bit = (TSS64_ACCESS >> 4) & 1;
        assert_eq!(s_bit, 0, "TSS is a system descriptor (S=0)");
        assert_eq!(type_field, 0x9, "Type must be 1001 (64-bit TSS available)");
    }

    #[test]
    fn init_fills_gdt_correctly() {
        let prot = X86_64Protection::init(0, VirBytes::new(0x8000));

        // Null descriptor
        assert_eq!(prot.gdt[GDT_NULL_INDEX], 0, "GDT[0] must be null");

        // Kernel CS: DPL=0, code/read, 64-bit (L=1), present
        let kern_cs = prot.gdt[GDT_KERN_CS_INDEX];
        let kern_cs_access = ((kern_cs >> 40) & 0xFF) as u8;
        assert_eq!(kern_cs_access, KERN_CS_ACCESS, "Kernel CS access byte");
        assert_ne!(kern_cs & (1 << 53), 0, "Kernel CS must have L bit set");

        // Kernel DS: DPL=0, data/write, present
        let kern_ds = prot.gdt[GDT_KERN_DS_INDEX];
        let kern_ds_access = ((kern_ds >> 40) & 0xFF) as u8;
        assert_eq!(kern_ds_access, KERN_DS_ACCESS, "Kernel DS access byte");

        // User CS: DPL=3, code/read, 64-bit (L=1), present
        let user_cs = prot.gdt[GDT_USER_CS_INDEX];
        let user_cs_access = ((user_cs >> 40) & 0xFF) as u8;
        assert_eq!(user_cs_access, USER_CS_ACCESS, "User CS access byte");

        // User DS: DPL=3, data/write, present
        let user_ds = prot.gdt[GDT_USER_DS_INDEX];
        let user_ds_access = ((user_ds >> 40) & 0xFF) as u8;
        assert_eq!(user_ds_access, USER_DS_ACCESS, "User DS access byte");
    }

    #[test]
    fn init_sets_tss_sp0_below_reserved_area() {
        let stack_top = 0xABCD_1234u64;
        let prot = X86_64Protection::init(0, VirBytes::new(stack_top));
        let sp0_ptr = core::ptr::addr_of!(prot.tss[0].sp0);
        assert_eq!(
            unsafe { core::ptr::read_unaligned(sp0_ptr) },
            stack_top - X86_64_STACK_TOP_RESERVED as u64,
            "init() must set TSS.sp0 below the X86_64_STACK_TOP_RESERVED area"
        );
    }

    #[test]
    fn init_sets_cpu_count() {
        let prot = X86_64Protection::init(3, VirBytes::new(0x8000));
        assert_eq!(prot.cpu_count, 4, "cpu_count = cpu_id + 1");
    }

    #[test]
    fn init_creates_tss_descriptor_in_gdt() {
        let prot = X86_64Protection::init(0, VirBytes::new(0x8000));
        // TSS descriptor occupies GDT entries 5 (low) and tss_desc_high[0] (high)
        let tss_low = prot.gdt[GDT_TSS_FIRST_INDEX];
        let tss_high = prot.tss_desc_high[0];
        // Access byte in low descriptor must have present bit and TSS type
        let access = ((tss_low >> 40) & 0xFF) as u8;
        assert_eq!(access & 0x89, 0x89, "TSS descriptor: present + 64-bit TSS type");
        // High descriptor contains upper 32 bits of TSS address
        assert_ne!(tss_high | tss_low, 0, "TSS descriptor must be non-null");
    }

    #[test]
    fn gdt_null_entry_is_zero() {
        let prot = X86_64Protection::init(0, VirBytes::new(0x8000));
        assert_eq!(prot.gdt[0], 0, "GDT null entry must be zero");
    }

    #[test]
    fn tss_iobase_disables_io_bitmap() {
        let prot = X86_64Protection::init(0, VirBytes::new(0x8000));
        let iobase_ptr = core::ptr::addr_of!(prot.tss[0].iobase);
        assert_eq!(
            unsafe { core::ptr::read_unaligned(iobase_ptr) },
            0x8000,
            "iobase must be 0x8000 to disable I/O bitmap"
        );
    }
}
