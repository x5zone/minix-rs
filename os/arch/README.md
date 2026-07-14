# minix-arch: Hardware Abstraction Layer

CPU ISA mechanism abstractions and board-level device abstractions for Minix-RS.

## Architecture

```
minix-arch/
├── arch/           # CPU ISA mechanism traits (architecture-independent)
│   ├── paging          Page table management (Paging trait)
│   ├── paging_ext      ASID/huge-page extensions (PagingWithId, HugePages)
│   ├── pt_alloc        Page table page allocator callback
│   ├── direct_map      Identity-mapped physical memory access
│   ├── protection      CPU protection structures (GDT/IDT/TSS, VBAR, stvec)
│   ├── trap_entry      Trap/syscall entry configuration
│   ├── exception       Exception handling (faults, page faults)
│   ├── exception_dispatcher  OS-level exception routing policy
│   ├── irq_manager     IRQ hook management (OS policy, not hardware)
│   ├── clock           Hardware timer (PIT/LAPIC, Generic Timer, mtime)
│   ├── arch_init       Architecture-specific early initialization
│   ├── boot            Process CPU context: CpuContextArch trait, boot proc, ELF load
│   └── post_init       Post-boot init: ptproc, freepdes, memory init
│
├── plat/           # Board-level device traits (SoC/motherboard-specific)
│   ├── early_console   Minimal serial output (COM1, PL011, SBI ecall)
│   └── interrupt       Interrupt controller (PIC/IOAPIC, GICv3, PLIC)
│
├── x86_64/         # x86-64 implementations
├── arm64/          # AArch64 implementations
└── riscv64/        # RISC-V 64-bit implementations
```

## CPU ISA vs Board-level: Design Rationale

### CPU ISA Mechanisms (`arch/`)

These are defined by the CPU instruction set architecture and are the same
regardless of which SoC or motherboard is used:

| Trait | What it abstracts | x86-64 | AArch64 | RISC-V |
|-------|-------------------|--------|---------|--------|
| `Paging` | Page table management | 4-level PML4 | 4-level TTBR0/1 | Sv39 |
| `ProtectionArch` | Protection structures | GDT/IDT/TSS | VBAR_EL1 | stvec |
| `TrapEntryArch` | Trap/syscall entry | LSTAR MSR | Exception vector | stvec direct |
| `ClockArch` | CPU-local timer | LAPIC Timer | Generic Timer | mtime |
| `ExceptionArch` | Exception handling | IDT vectors | ESR_EL1 | scause |
| `ArchProcReset` | Process register init | iret frame | SPSR/ELR | sstatus/sepc |

### Board-level Devices (`plat/`)

These vary per SoC/motherboard and are NOT part of the CPU ISA:

| Trait | What it abstracts | Typical implementations |
|-------|-------------------|------------------------|
| `InterruptController` | External interrupt routing | PIC/IOAPIC (x86), GICv2/GICv3 (ARM), PLIC (RISC-V) |
| `EarlyConsole` | Serial output | COM1 (x86), PL011 (ARM), SBI ecall (RISC-V) |

**Key insight**: The same CPU ISA (e.g., AArch64) can be paired with different
interrupt controllers (GICv2 vs GICv3) or different UARTs (PL011 vs NS16550).
Separating `arch/` from `plat/` allows swapping board-level devices without
touching CPU ISA code.

## Known Issues & TODO

1. **P1: `irq_manager` is OS policy, not hardware** — `arch/irq_manager.rs`
   manages IRQ hooks and routing policy. This belongs in `kernel/`, not `arch/`.
   It should depend on `InterruptController` (from `plat/`), not be part of it.

2. **P1: `plat/` should become a separate crate** — Currently `arch/` and `plat/`
   are in the same crate. A future refactor should split `minix-plat` out of
   `minix-arch`, so that user-space servers can depend on `minix-arch` traits
   (e.g., `ArchProcReset`) without pulling in board-level device code.

3. **P2: `CurrentXxx` type aliases use `#[cfg(target_arch)]`** — The current
   approach uses `cfg(target_arch)` to select concrete types. This works for
   single-architecture builds but doesn't support multi-arch runtime selection.
   A future board-description approach could replace this.

## Trait Design Principles

1. **Distributed definition**: Each feature module defines its own traits.
2. **Centralized implementation**: All traits are implemented within this crate.
3. **Architecture-independent**: OS code depends only on traits, not hardware.
4. **Mechanism, not policy**: Traits describe WHAT the hardware does, not HOW
   the OS uses it. For example, `Paging::map()` is mechanism; "which virtual
   address to map for the kernel" is policy (belongs in `kernel/`).
