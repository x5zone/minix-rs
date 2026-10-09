# Shared Libraries (libs/)

Cross-service shared libraries for Minix-RS. Each crate has a single, well-defined responsibility.

> **Current inventory (`2026-10-10` reconciliation)**: `os/libs/` now holds **21** crates — the
> authoritative list is `os/Cargo.toml` `[workspace.members]`. The crate overview and dependency
> graph below were drafted when this tree was a skeleton and are **outdated**: `minix-ipc` and
> `minix-mock` were never split into separate crates (the IPC runtime — `send`/`receive`/`sendrec`/
> `notify` — lives in `minix-sys`; hardware mocking is the `mock` cargo feature in
> `minix-arch`/`minix-platform`, not a crate). The `minix-sys` and `minix-rt` entries previously
> labelled "STUB / all `todo!()`" are **implemented**. Only the corrections noted here were re-verified
> against the tree; the full refresh of every crate's detailed section is tracked as a follow-up.

## Crate Overview

Protocol / boot / runtime core:

```
libs/
├── minix-types/      Core protocol types (Endpoint, Pid, Message, errno, boot info)
├── minix-boot/       Boot protocol types (KernelInfo, BootShim trait)
├── minix-elf/        Minimal ELF64 parser (no_std, zero-allocation)
├── minix-platform/   Platform/firmware abstraction (incl. `mock` feature for tests)
├── minix-sys/        User-space syscall + IPC wrappers — implemented (send/receive/notify/fork/exec, per-server modules)
├── minix-rt/         User-space runtime — implemented (crt0, SlabAllocator + VM_BRK supplier, panic handler)
├── minix-sef/        User-space service framework (event loop, signal proxy)
└── minix-fs/         Filesystem service framework types
```

Driver / device frameworks (each mirrors a Minix3 driver-face protocol):

```
├── minix-driver-rt/   Driver runtime scaffolding
├── minix-chardriver/  Character device driver face
├── minix-blockdriver/ Block device driver face
├── minix-bdev/        Block-device helpers
├── minix-netdriver/   Network device driver face
├── minix-sockdriver/  Socket-driver wire vocabulary (single source — `sdev`/`sockevent`)
├── minix-audiodriver/ Audio device driver face
├── minix-i2cdriver/   I2C device driver face
├── minix-vtreefs/     Virtual tree filesystem helpers
├── minix-sffs/        Simple filesystem (sffs) library
├── minix-usb/         USB request-block / URB bookkeeping types
├── minix-virtio/      Virtio transport types
└── minix-crypt/       Crypto primitives (AES/SHA self-implemented, no_std)
```

## Dependency Graph

> Historical sketch — `minix-ipc` is not a crate; IPC lives in `minix-sys`. See
> `os/Cargo.toml` for the authoritative membership and dependency edges.

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

### minix-sys — User-space Syscalls and IPC runtime

Implemented. Owns the user-space syscall + IPC wrapper surface — `send`, `receive`, `sendrec`,
`notify` (`lib.rs`), the process calls (`fork`, `exec`, `waitpid`, `kill`, …), and per-server
client modules (`ipc.rs`, `socket.rs`, `rs.rs`, `pm.rs`, `vm.rs`, `devman_client.rs`, …). There
is no separate `minix-ipc` crate; the IPC runtime lives here and the wire message types live in
`minix-types/src/ipc/`.

### minix-rt — User-space Runtime

Implemented. crt0 startup, the global `SlabAllocator`, the panic handler, and the VM_BRK-backed
page supplier (`install_vm_supplier`) that lets a born service grow its heap through the VM
service instead of a fixed pool.

### Hardware mocking

There is no separate `minix-mock` crate. Hardware/firmware mocking is the `mock` cargo feature
(the mock arms in `minix-arch`/`minix-platform`); see `os/kernel/Cargo.toml` and `SD-30` /
`P-ALL-10` in the coordination ledger for the tracking of the default-feature question.

## Design Principles

1. **minix-types is the protocol**: It defines the "language" that all services speak. Changes here affect the entire system.
2. **No circular dependencies**: The dependency graph is a DAG. `minix-types` has zero internal dependencies.
3. **STUB crates are placeholders**: Crates marked "STUB" are architectural placeholders. They define the intended API but are not yet functional.
