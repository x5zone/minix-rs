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

/// GDT slots per TSS descriptor. Long mode (unlike C's i386) uses 16-byte
/// TSS descriptors — two consecutive GDT entries. Selector for CPU `i` =
/// `(GDT_TSS_FIRST_INDEX + i * GDT_SLOTS_PER_TSS) * 8`.
/// [ARCH: long-mode TSS descriptor size] C protect.c builds 8-byte (32-bit)
/// TSS descriptors; the x86-64 port's ISA requires 16 bytes (SDM Vol. 3A
/// §7.2.3 — upper dword holds base[63:32]).
const GDT_SLOTS_PER_TSS: usize = 2;

pub(crate) const KERN_CS_SELECTOR: u16 = (GDT_KERN_CS_INDEX * 8) as u16;
pub(crate) const KERN_DS_SELECTOR: u16 = (GDT_KERN_DS_INDEX * 8) as u16;
pub(crate) const USER_CS_SELECTOR: u16 = ((GDT_USER_CS_INDEX * 8) | 3) as u16;
#[allow(dead_code)] // user data segment selector; not yet wired to all call sites
pub(crate) const USER_DS_SELECTOR: u16 = ((GDT_USER_DS_INDEX * 8) | 3) as u16;

const MAX_CPUS: usize = 8;

const GDT_ENTRIES: usize = GDT_TSS_FIRST_INDEX + MAX_CPUS * GDT_SLOTS_PER_TSS;

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
        // No cpu-id stamp here: init() calls this for every slot in a loop
        // (descriptors are address-stable), and stamping would write the
        // same reserved-top address MAX_CPUS times. The stamp belongs to
        // the assignment points — init() for BSP, set_kernel_stack for APs.

        // iobase = 0x8000 disables I/O permission bitmap per Intel SDM Vol. 3A §7.7:
        // "If the I/O Map Base Address ≥ TSS limit, no I/O permission map exists."
        // C (i386): iobase = sizeof(struct tss_s) = 104 (no I/O bitmap in 32-bit).
        // 64-bit TSS uses 0x8000 for consistency — any value >= TSS limit works.

        // IST1 (NMI) / IST2 (#DF): per-CPU dedicated stacks — the shared IDT's
        // gates for vectors 2/8 carry ist=1/2 (C gate_table_exceptions
        // parity), and a gate with IST≠0 reads TSS.ist[ist-1] at delivery.
        // Without these, the first NMI/#DF would switch to a null stack and
        // triple-fault.
        self.tss[idx].ist[0] = ist_stack_top(0, idx);
        self.tss[idx].ist[1] = ist_stack_top(1, idx);

        let tss_addr = &self.tss[idx] as *const Tss64 as u64;
        let desc = make_tss_desc64(tss_addr, TSS64_SIZE as u16 - 1, 0);

        // BOTH halves go into the GDT image — long-mode TSS descriptors are
        // 16 bytes (two consecutive entries). The pre-S-4 code kept the high
        // dword in a separate array that `load()` never exposed, so `ltr`
        // read base[63:32]=0 and any ring crossing would have died on a
        // garbage TSS (latent: nothing crossed rings before S-4).
        let gdt_idx = GDT_TSS_FIRST_INDEX + idx * GDT_SLOTS_PER_TSS;
        self.gdt[gdt_idx] = desc[0];
        self.gdt[gdt_idx + 1] = desc[1];
    }

    /// Task-register selector for `cpu_id` (long-mode selector math: the
    /// descriptor starts at GDT slot `GDT_TSS_FIRST_INDEX + i*2`).
    fn tss_selector(cpu_id: u32) -> u16 {
        ((GDT_TSS_FIRST_INDEX + cpu_id as usize * GDT_SLOTS_PER_TSS) * 8) as u16
    }
}

/// Stamp the CPU id into the reserved top of a CPU's kernel stack
/// (C: `*((reg_t *)(sp0 + sizeof(reg_t))) = cpu` — protect.c:173-181; read
/// by the assembly trap entry to identify the CPU in use). Called only when
/// a specific CPU's stack is assigned — init() stamps the BSP slot,
/// `set_kernel_stack` stamps the AP slot before its SIPI — never in a loop
/// over slots, which would write one address MAX_CPUS times.
///
/// Skipped under cfg(test): the addresses are mock values there.
fn stamp_cpu_id(usable_top: u64, cpu_id: u32) {
    #[cfg(not(test))]
    unsafe {
        let cpu_id_slot = (usable_top + core::mem::size_of::<u64>() as u64) as *mut u64;
        cpu_id_slot.write_volatile(cpu_id as u64);
    }
    #[cfg(test)]
    let _ = (usable_top, cpu_id);
}

/// Per-CPU IST stacks (S-4 §3.3): static, fixed-address, zero-heap —
/// `IST_STACKS[slot][cpu]`, top = base + size. Slot 0 serves IST1 (NMI),
/// slot 1 serves IST2 (#DF), matching the shared IDT's gate ist fields.
const IST_STACK_SIZE: usize = 0x2000; // 8 KiB per stack
static mut IST_STACKS: [[[u8; IST_STACK_SIZE]; MAX_CPUS]; 2] =
    [[[0; IST_STACK_SIZE]; MAX_CPUS]; 2];

fn ist_stack_top(slot: usize, cpu_id: usize) -> u64 {
    // SAFETY: address-only computation on a static array; no dereference.
    let base = unsafe { core::ptr::addr_of!(IST_STACKS[slot][cpu_id]) as *const u8 as usize };
    (base + IST_STACK_SIZE) as u64
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
            cpu_count: 0,
            boot_cpu_id: cpu_id,
        };

        prot.fill_flat_segments();
        // Build the TSS descriptors for EVERY CPU slot now: the `tss` array
        // lives inside this instance at fixed addresses, so the descriptors
        // never need rebuilding — `set_kernel_stack` only rewrites sp0, and
        // an AP's `init_ap` can then be a pure load (no writes, no races).
        for slot in 0..MAX_CPUS as u32 {
            prot.setup_tss_for_cpu(slot, kernel_stack_top);
        }
        // BSP's sp0 gets the real stack top + the cpu-id stamp; other slots
        // keep the placeholder until their `set_kernel_stack` runs during
        // SMP bring-up (S-5) — which stamps them.
        let bsp_usable = kernel_stack_top.get() - X86_64_STACK_TOP_RESERVED as u64;
        prot.tss[cpu_id as usize].sp0 = bsp_usable;
        stamp_cpu_id(bsp_usable, cpu_id);
        prot.cpu_count = cpu_id + 1;

        prot
    }

    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes) {
        let idx = cpu_id as usize;
        assert!(idx < MAX_CPUS, "cpu_id {} exceeds MAX_CPUS {}", cpu_id, MAX_CPUS);
        let usable_top = stack_top.get() - X86_64_STACK_TOP_RESERVED as u64;
        self.tss[idx].sp0 = usable_top;
        stamp_cpu_id(usable_top, cpu_id);
    }

    fn load(&self) {
        self.load_with_tss(self.boot_cpu_id);
    }

    /// AP-side per-CPU protection bring-up (S-4, D-39 closed).
    ///
    /// Runs ON the AP after the early ladder. The shared instance's GDT
    /// already carries this CPU's 16-byte TSS descriptor (built by `init()`
    /// at a stable address), and the BSP wrote `sp0` via `set_kernel_stack`
    /// before the SIPI — so this is a pure load:
    /// lgdt (per-CPU GDT image, C "no global GDT sync" semantics) →
    /// ltr this CPU's TSS → reload data segments → program the arch GS area
    /// and both GS_BASE MSR variants (the S-8 SYSCALL entry reads its kernel
    /// stack top from `gs:0x0`; §3.3 per-CPU MSR contract).
    ///
    /// The per-CPU SYSCALL MSRs (STAR/LSTAR/SFMASK/EFER.SCE) are written by
    /// the caller through `trap_stub::write_syscall_msrs` — they belong to
    /// the trap layer, not this trait.
    ///
    /// C: tss_init(cpu, stack) + prot_load_selectors() — mpx.S AP path.
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        assert!(
            (cpu_id as usize) < MAX_CPUS,
            "init_ap: cpu_id {} exceeds MAX_CPUS {}",
            cpu_id,
            MAX_CPUS
        );
        assert!(
            self.gdt[GDT_TSS_FIRST_INDEX + cpu_id as usize * GDT_SLOTS_PER_TSS] != 0,
            "init_ap: TSS descriptor for cpu {} not built (init() must run first)",
            cpu_id
        );
        // Load order matters: `load_with_tss`'s segment-reload step does
        // `mov gs, 0` — which (unlike most segments) zeroes the GS BASE on
        // x86-64 — so the GS area programming MUST come after it or the
        // wrmsr'd base is silently clobbered (first L2 run: gs:0x10 read
        // IVT bytes at base 0).
        self.load_with_tss(cpu_id);
        // Arch GS area + both MSR variants. IA32_KERNEL_GS_BASE gets the same
        // pointer: no user mode exists before S-6/S-7, so swapgs has nothing
        // to swap yet — the invariant is documented at the stub.
        crate::x86_64::trap_stub::program_gs(cpu_id, kernel_stack_top);
    }
}

impl X86_64Protection {
    /// Shared load sequence: lgdt + reload CS/DS/ES/SS + ltr(cpu).
    fn load_with_tss(&self, cpu_id: u32) {
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

            // 5. Load Task Register (TR) with the target CPU's TSS selector.
            //    SAFETY: the descriptor for `cpu_id` was built by init() at a
            //    stable address and is present in this GDT image.
            let tr_sel: u16 = Self::tss_selector(cpu_id);
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
        // S-4 consistency fix: set_kernel_stack applies the same reserved-area
        // subtraction as init()/C tss_init — sp0 must point BELOW the reserved
        // top (the old code stored the raw top, contradicting init's layout).
        assert_eq!(
            unsafe { core::ptr::read_unaligned(sp0_ptr) },
            0xA000 - X86_64_STACK_TOP_RESERVED as u64,
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
        // S-4: long-mode TSS descriptors are 16 bytes — BOTH halves live in
        // the GDT image (the pre-S-4 code kept the high dword in a separate
        // array `load()` never exposed, so ltr read base[63:32]=0).
        let prot = X86_64Protection::init(0, VirBytes::new(0x8000));
        for cpu in 0..MAX_CPUS {
            let slot = GDT_TSS_FIRST_INDEX + cpu * GDT_SLOTS_PER_TSS;
            let tss_low = prot.gdt[slot];
            let tss_high = prot.gdt[slot + 1];
            // Access byte in the low descriptor: present + 64-bit TSS type.
            let access = ((tss_low >> 40) & 0xFF) as u8;
            assert_eq!(access & 0x89, 0x89, "TSS cpu {cpu}: present + 64-bit TSS type");
            // The high half carries base[63:32]; the struct lives in this
            // test's image (nonzero high bits on any address ≥ 4 GiB and on
            // higher-half kernels) and must be non-null overall.
            assert_ne!(tss_high | tss_low, 0, "TSS cpu {cpu}: descriptor non-null");
            // Selector math: the TR selector for this CPU must point at the
            // descriptor's low half.
            assert_eq!(
                X86_64Protection::tss_selector(cpu as u32),
                (slot * 8) as u16,
                "TSS cpu {cpu}: selector must address the descriptor slot"
            );
        }
    }

    #[test]
    fn tss_descriptor_high_half_carries_base_above_4gib() {
        // Regression pin for the S-4 layout fix: the descriptor's upper dword
        // (GDT slot +1) must carry base[63:32] of the TSS — a higher-half
        // address has nonzero base[63:32] (0xFFFF_8000), which the old
        // tss_desc_high-side-array layout lost.
        let prot = X86_64Protection::init(0, VirBytes::new(0xFFFF_8000_0020_0000));
        let tss_addr = core::ptr::addr_of!(prot.tss[0]) as u64;
        if tss_addr >> 32 == 0 {
            return; // low-memory test build: high half legitimately 0
        }
        let slot = GDT_TSS_FIRST_INDEX;
        let tss_high = prot.gdt[slot + 1];
        assert_eq!(
            (tss_high & 0xFFFF_FFFF) as u64,
            tss_addr >> 32,
            "descriptor high dword must carry base[63:32]"
        );
    }

    #[test]
    fn init_stamps_bsp_cpu_id_on_stack_top() {
        // C protect.c:173-181 parity — the stamp lives at sp0 + sizeof(reg_t).
        // cfg(test) skips the real write; this pins the sp0 arithmetic.
        let prot = X86_64Protection::init(0, VirBytes::new(0x8000));
        // tss is #[repr(C, packed)] — read through addr_of! (unaligned field).
        let sp0 = unsafe { core::ptr::addr_of!(prot.tss[0].sp0).read_unaligned() };
        assert_eq!(
            sp0,
            0x8000 - X86_64_STACK_TOP_RESERVED as u64,
            "BSP sp0 = stack_top - reserved"
        );
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
