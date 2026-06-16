# Shared Libraries (libs/)

Cross-service shared libraries for Minix-RS. Each crate has a single, well-defined responsibility.

## Crate Overview

```
libs/
├── minix-types/    Core protocol types (Endpoint, Pid, Message, errno)
├── minix-boot/     Boot protocol types (KernelInfo, BootShim trait)
├── minix-elf/      Minimal ELF64 parser (no_std, zero-allocation)
├── minix-ipc/      IPC protocol stubs (send/receive/notify — STUB)
├── minix-sys/      User-space syscall wrappers (STUB)
├── minix-rt/       User-space runtime support (STUB)
└── minix-mock/     Hardware mock for testing (STUB)
```

## Dependency Graph

```
minix-types ◄── minix-boot
             ◄── minix-ipc
             ◄── minix-arch
             ◄── minix-kernel
             ◄── minix-sys

minix-boot  ◄── boot-shim
            ◄── minix-kernel

minix-elf   ◄── boot-shim

minix-ipc   ◄── minix-sys
            ◄── servers/*

minix-sys   ◄── commands/*
```

## Crate Details

### minix-types — Core Protocol Types

**Equivalent to**: Minix3's `include/minix/`

**Contains**:
- `types/`: Basic protocol types (Endpoint, Pid, UserSlot, KernelSlot, Uid, Gid, VirBytes, PhysBytes, errno, bitmap, boot)
- `ipc/`: IPC message structures (Message, MessageM1..M8) and per-server semantic types (VmForkIn, PmExitIn, etc.)

**Does NOT contain**:
- Service-private data (mproc, vmproc, fproc — these live in their respective server crates)
- IPC runtime (send/receive — this is `minix-ipc`/`minix-sys`)
- Hardware-specific types (page table entries — this is `minix-arch`)

### minix-boot — Boot Protocol Types

**Contains**:
- `KernelInfo`: Shared struct passed from boot-loader to kernel (memory map, kernel location, boot modules)
- `BootShim` trait: Firmware-agnostic boot preparation interface
- `BootPrepareResult`: Data crossing the boot-shim → kernel boundary

**Does NOT contain**:
- Firmware-specific code (UEFI, OpenSBI — this is `boot-shim/`)
- Kernel initialization logic (this is `kernel/`)

### minix-elf — ELF64 Parser

Minimal, zero-allocation ELF64 parser for boot loading. Only parses what the boot-shim needs: program headers and section headers.

### minix-ipc — IPC Protocol (STUB)

**Current state**: Nearly empty shell. Re-exports `minix_types::{Endpoint, Message}` and defines `SyscallNum`, `NotifyType`, `IpcError`, but `send`/`receive`/`notify` are all `todo!()`.

**Planned responsibility**: IPC protocol definitions and kernel syscall wrappers for IPC operations.

**Known issue**: Overlaps with `minix-types/src/ipc/` which already contains complete Message definitions and per-server semantic types. See architecture review for refactoring suggestions.

### minix-sys — User-space Syscalls (STUB)

User-space system call wrappers (fork, exec, waitpid, etc.). Currently all `todo!()`.

### minix-rt — User-space Runtime (STUB)

User-space runtime support (crt0 equivalent, allocator, panic handler). Currently a stub.

### minix-mock — Hardware Mock (STUB)

Mock implementations of hardware traits for testing. Currently a stub.

## Design Principles

1. **minix-types is the protocol**: It defines the "language" that all services speak. Changes here affect the entire system.
2. **No circular dependencies**: The dependency graph is a DAG. `minix-types` has zero internal dependencies.
3. **STUB crates are placeholders**: Crates marked "STUB" are architectural placeholders. They define the intended API but are not yet functional.
