/* Kernel linker script — aarch64
 *
 * Higher-half kernel layout for ARM64.
 * Same structure as x86-64, different base addresses.
 *
 * See 02-higher-half-kernel.md §4.1 for design rationale.
 */

OUTPUT_ARCH(aarch64)

KERN_VIRT_BASE = 0xFFFF800000000000;
KERN_PHYS_BASE = 0x40200000;    /* QEMU virt RAM start + 2MB (2MB-aligned for huge pages) */

SECTIONS
{
    . = KERN_VIRT_BASE;

    .text : {
        *(.text .text.*)
    }

    .rodata : ALIGN(4096) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4096) {
        *(.data .data.*)
    }

    .bss : ALIGN(4096) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    PROVIDE(kern_virt_base = KERN_VIRT_BASE);
    PROVIDE(kern_phys_base = KERN_PHYS_BASE);
    PROVIDE(kern_size = . - KERN_VIRT_BASE);

    . = ALIGN(4096);
    . += 0x4000;
    PROVIDE(k_initial_stktop = .);

    _end = .;

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note.*)
    }
}
