/* Kernel linker script — x86-64
 *
 * Higher-half kernel layout:
 * - VMA starts at KERN_VIRT_BASE (canonical high address)
 * - LMA starts at KERN_PHYS_BASE (low physical address)
 * - boot-shim loads segments by p_paddr (LMA)
 * - All symbol addresses resolve to VMA (high address)
 * - No AT() directives — boot-shim handles LMA placement
 *
 * Corresponds to Minix3 kernel/arch/i386/kernel.lds, but without AT().
 * See 02-higher-half-kernel.md §4.1 for design rationale.
 */

OUTPUT_ARCH(i386:x86-64)

KERN_VIRT_BASE = 0xFFFF800000000000;
KERN_PHYS_BASE = 0x00200000;    /* 2MB, typical physical load address */

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

    /* Exported symbols for KernelInfo */
    PROVIDE(kern_virt_base = KERN_VIRT_BASE);
    PROVIDE(kern_phys_base = KERN_PHYS_BASE);
    PROVIDE(kern_size = . - KERN_VIRT_BASE);

    /* Initial kernel stack (4 pages = 16KB) */
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
