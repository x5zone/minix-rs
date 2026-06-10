/* Kernel linker script — riscv64 (Sv39)
 *
 * Higher-half kernel layout for RISC-V 64-bit.
 * Uses Sv39 canonical high region (different from x86-64/aarch64).
 *
 * Sv39 address space layout (39-bit virtual addresses):
 * - Low:  0x0000_0000_0000_0000 ~ 0x0000_003F_FFFF_FFFF (VPN[2] = 0~255)
 * - High: 0xFFFF_FFC0_0000_0000 ~ 0xFFFF_FFFF_FFFF_FFFF (VPN[2] = 256~511)
 *
 * NOTE: 0xFFFF_C000_0000_0000 and 0xFFFF_FC00_0000_0000 are NOT valid
 *       Sv39 canonical high addresses! Their VPN[2] = 0 (same as low
 *       addresses), which conflicts with identity mapping.
 *       The correct canonical high region starts at
 *       0xFFFF_FFC0_0000_0000 (VPN[2] = 256).
 *
 * See 02-higher-half-kernel.md §4.1 for design rationale.
 */

OUTPUT_ARCH(riscv)

KERN_VIRT_BASE = 0xFFFFFFC000000000;  /* Sv39 canonical high, VPN[2]=256 */
KERN_PHYS_BASE = 0x80200000;         /* QEMU virt DRAM start */

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
