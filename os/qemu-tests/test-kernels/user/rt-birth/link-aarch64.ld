/* rt-birth link script — freestanding minix-rt user image (aarch64).
 *
 * K12b aarch64 leg. Link base 0x140000000 (5 GiB), same rationale as the
 * other arch scripts: above the bootstrap root's identity window, clear of
 * the reserved shared pages (VM_BOOT_HANDOFF_VA @ 0x1_0000_0000,
 * KERNINFO_USER_VA @ 0x2_0000_0000) and of the loader stack. 5 GiB sits
 * well inside the 48-bit user VA range (TTBR0 covers it with 4 KiB pages).
 *
 * Every output section starts on its own page so the PT_LOAD vaddrs are
 * page-aligned and page-disjoint — `load_vm_elf` maps each segment's pages
 * independently and a shared boundary page comes back AlreadyMapped.
 */
OUTPUT_ARCH(aarch64)
ENTRY(_start)

SECTIONS
{
    . = 0x140000000;

    .text : {
        *(.text .text.*)
    }

    . = ALIGN(4096);
    .rodata : {
        *(.rodata .rodata.*)
    }

    . = ALIGN(4096);
    .data : {
        *(.data .data.*)
        *(.got .got.*)
    }

    . = ALIGN(4096);
    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame .eh_frame.*)
        *(.note .note.*)
        *(.comment)
    }
}
