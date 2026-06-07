# QEMU Integration Tests

This directory contains end-to-end kernel boot tests that run in QEMU with
UEFI firmware (OVMF / AA64 UEFI / RISC-V UEFI) or OpenSBI.

## Quick Start

```bash
# Install QEMU + UEFI firmware
sudo apt install qemu-system-x86 ovmf                # x86-64
sudo apt install qemu-system-arm qemu-efi-aarch64    # aarch64
sudo apt install qemu-system-misc qemu-efi-riscv64   # riscv64

# Install Rust targets
rustup target add x86_64-unknown-uefi
rustup target add aarch64-unknown-uefi
rustup target add riscv64gc-unknown-none-elf

# Run all tests
./run_all.sh
```

## Test Architecture

```
os/qemu-tests/
├── README.md
├── run_all.sh              # CI entry point
├── run_qemu.sh             # Single-test runner (takes test binary + arch)
├── test-kernels/
│   └── kernel/                 # 对应 03-stage-kernel/ 文档
│       └── bootstrap/          # 对应 01-multiboot-bootstrap.md
│           ├── hello-boot/             # x86_64: full boot chain (UEFI → Paging → serial)
│           ├── test-memmap/            # x86_64: KernelInfo.memmap covers kernel region
│           ├── test-paging-enable/     # x86_64: paging.enable() — CPU survives CR3 switch
│           ├── test-kernel-map/        # x86_64: high-half mapping returns correct data
│           ├── hello-boot-aarch64/     # aarch64: UEFI + PL011 serial
│           └── hello-boot-riscv64/     # riscv64: OpenSBI + SBI console
└── common/                 # (future) shared serial helpers
```

## Test Format

Each test kernel is a standalone binary that boots in QEMU and writes a PASS
marker to the serial console. The exact boot path depends on architecture:

### x86_64 / aarch64 (UEFI)

UEFI application using `efi_main()` entry point:

1. UEFI entry: `efi_main()` → construct `KernelInfo`
2. `Paging::new_from_page()` → zero-fill root page table
3. `paging.map_huge()` → identity mapping + kernel mapping
4. `paging.enable()` → load page table into MMU
5. Write `### TEST_RESULT: PASS <name> ###` to serial
6. Halt

### riscv64 (OpenSBI)

Bare-metal ELF loaded by OpenSBI at 0x80200000:

1. `_start` (assembly) → set up stack → `rust_main()`
2. SBI ecall `CONSOLE_PUTCHAR` → write PASS marker to serial
3. Halt (`wfi`)

The `run_qemu.sh` script captures serial output and checks for the PASS marker.

## Adding a New Test

```bash
cp -r test-kernels/kernel/bootstrap/hello-boot test-kernels/kernel/bootstrap/test-my-thing
# edit test-kernels/kernel/bootstrap/test-my-thing/src/main.rs
# add to run_all.sh
```

## Current Status

| Test | x86-64 | aarch64 | riscv64 | What it verifies |
|------|--------|---------|---------|-----------------|
| hello-boot | ✅ | ✅ (serial) | ✅ (serial) | Full boot chain: UEFI → Paging trait → serial output |
| test-memmap | ✅ | — | — | KernelInfo.memmap covers kernel physical region |
| test-paging-enable | ✅ | — | — | paging.enable() switches CR3 and CPU survives |
| test-kernel-map | ✅ | — | — | High-half mapping: same data via low and high addresses |
