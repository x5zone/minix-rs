/*
 * riscv64-uboot.ld — D-73 U-Boot leg shim link script (physical execution).
 *
 * Unlike the UEFI shims (x86 PE / aarch64 UEFI image, which run before their
 * kernel enables paging with firmware-provided translation), this shim is
 * jumped to by `bootelf -p` with **no MMU** — so it is linked VMA == LMA at a
 * physical address and runs there directly, with its own bootstrap stack.
 *
 * The base address is not an invention: it sits inside the U-Boot 2025.10
 * (256 MiB QEMU virt) free band measured on the running system
 * (bdinfo/lmb, 2026-10-31 probe):
 *   - lmb reserved:   OpenSBI  0x80000000-0x8005ffff
 *                     board    0x8deaa000-0x8deacfff
 *                     U-Boot   0x8dead8d0-0x8fffffff
 *                     (relocaddr 0x8f6cf000, malloc 0x8eeaf000-0x8f6cf000,
 *                      fdt_blob 0x8eead8e0)
 *   - free band:      0x80060000-0x8dea9fff
 *   - boot script regions that must stay clear of this image:
 *                     kernel.elf PT_LOAD target 0x80200000-0x80600000
 *                     bump/module region         0x82000000-0x84000000
 *                     kernel.elf file staging    0x85000000
 *                     module file staging        0x86000000-0x86bfffff
 *                     BootFileTable              0x8c000000
 *                     DTB staging                0x8c000000
 *   - this image:     0x84500000 (+ ~64 KiB text/rodata/data/bss + 64 KiB
 *                     stack) — disjoint from every region above.
 *
 * The ELF file itself is staged by `fatload` at U-Boot's default
 * `loadaddr` (0x80200000) — the file bytes are consumed by `bootelf -p`
 * (copied to these PT_LOAD LMAs) *before* the shim runs, and the kernel
 * image copies over that dead staging area later, so the overlap with
 * 0x80200000 is transient and harmless (probe-verified: the argv probe
 * used exactly this staging/LMA split twice, clean runs).
 */

OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS
{
    . = 0x84500000;

    .text : {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
        *(.got .got.*)
    }

    .bss (NOLOAD) : {
        __bss_start = .;
        *(.bss .bss.*)
        /* .lbss = rustc large BSS split (SHIM_HEAP 8 MiB lands here — same
         * family as kernel-image's .lbss note in riscv64.ld). Missing this
         * match puts the heap outside __bss_start..__bss_end → the entry
         * bss clear misses it → the bump allocator hands out garbage. */
        *(.lbss .lbss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* The bootstrap stack lives inside the shim image: bootelf hands over
     * on U-Boot's own stack with no contract about its size or lifetime. */
    .stack (NOLOAD) : ALIGN(16) {
        . += 64K;
        UBOOT_SHIM_STACK_TOP = .;
    }

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment)
        *(.riscv.attributes)
    }
}
