# Minix-RS Operating System

A Rust rewrite of the Minix3 microkernel operating system.

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│  User Commands (commands/)                                       │
│  cat, ls, sh, init, mkfs, ...                                   │
├──────────────────────────────────────────────────────────────────┤
│  System Servers (servers/)                                       │
│  PM, VM, VFS, RS, DS, Sched, IPC, IS, DevMan, Input, MIB       │
├──────────────────────────────────────────────────────────────────┤
│  Kernel (kernel/)                                                │
│  Process table, scheduler, privilege, VM request, boot alloc    │
├──────────────────────────────────────────────────────────────────┤
│  Hardware Abstraction (arch/)                                    │
│  ┌──────────────────────┐  ┌──────────────────────┐             │
│  │ arch/ (CPU ISA)      │  │ plat/ (Board-level)  │             │
│  │ Paging, Protection,  │  │ InterruptController, │             │
│  │ TrapEntry, Clock,    │  │ EarlyConsole         │             │
│  │ Exception, ProcArch  │  │ (PIC/IOAPIC, GIC,   │             │
│  │                      │  │  PLIC, UART)         │             │
│  └──────────────────────┘  └──────────────────────┘             │
├──────────────────────────────────────────────────────────────────┤
│  Shared Libraries (libs/)                                        │
│  minix-types  minix-boot  minix-elf  minix-ipc  minix-sys ...  │
├──────────────────────────────────────────────────────────────────┤
│  Boot Shim (boot-shim/)                                          │
│  UEFI / OpenSBI firmware → kernel handoff                       │
└──────────────────────────────────────────────────────────────────┘
```

## Directory Layout

| Directory | Crate | Description |
|-----------|-------|-------------|
| `kernel/` | `minix-kernel` | Kernel core: process table, scheduler, privilege, VM request handling |
| `arch/` | `minix-arch` | Hardware abstraction layer: CPU ISA traits + board-level device traits |
| `boot-shim/` | `boot-shim` | Firmware bridge: UEFI/OpenSBI → kernel handoff |
| `libs/minix-types/` | `minix-types` | Cross-service protocol types (Endpoint, Pid, Message, errno) |
| `libs/minix-boot/` | `minix-boot` | Boot protocol types (KernelInfo, BootShim trait) |
| `libs/minix-elf/` | `minix-elf` | Minimal ELF64 parser (zero-allocation, no_std) |
| `libs/minix-ipc/` | `minix-ipc` | IPC protocol stubs (send/receive/notify — not yet functional) |
| `libs/minix-sys/` | `minix-sys` | User-space syscall wrappers (stub) |
| `libs/minix-rt/` | `minix-rt` | User-space runtime support (stub) |
| `libs/minix-mock/` | `minix-mock` | Hardware mock for testing (stub) |
| `servers/` | (12 crates) | User-space system servers (PM, VM, VFS, RS, DS, ...) |
| `commands/` | (14 crates) | User-space commands (cat, ls, sh, init, ...) |
| `qemu-tests/` | (18 crates) | QEMU-based hardware integration test kernels |
| `tests/` | `tests` | Host-based integration tests |

## Dependency Graph (Simplified)

```
commands/* ──→ minix-sys ──→ minix-ipc ──→ minix-types
servers/*  ──→ minix-ipc ──→ minix-types
kernel     ──→ minix-arch ──→ minix-types
           ──→ minix-boot
boot-shim  ──→ minix-boot ──→ minix-types
           ──→ minix-elf
           ──→ minix-arch
           ──→ minix-kernel (binary only, for arch_boot entry)
```

## Key Design Principles

1. **Hardware Abstraction**: All hardware interaction goes through traits. Upper layers depend only on trait interfaces.
2. **CPU ISA vs Board-level**: `arch/` subdirectory holds CPU instruction set mechanisms (paging, protection, exceptions). `plat/` subdirectory holds board-level devices (interrupt controller, UART).
3. **Protocol Types**: `minix-types` is the "protocol" between services — equivalent to Minix3's `include/minix/`.
4. **no_std**: All kernel and library code is `#![no_std]`. Only `#[cfg(test)]` may use `std`.
5. **Microkernel**: Each server has its own private process table. No shared mutable state between servers.

## Build & Test

```bash
# Build kernel for x86_64 UEFI
cargo build -p boot-shim --target x86_64-unknown-uefi --release

# Run QEMU integration tests
cd qemu-tests && bash run_all.sh

# Run host-based tests
cargo test -p minix-kernel
cargo test -p minix-arch
```
