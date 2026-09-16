/* rt-birth link script — freestanding minix-rt user image (x86-64).
 *
 * Link base 0x140000000 (5 GiB), NOT the classic 0x400000: the minix-rs
 * bootstrap root keeps a 1 GiB-leaf identity map for the first 4 GiB
 * (arch_boot_impl Step 1), so any 4 KiB PT_LOAD mapped below 4 GiB hits
 * `PageTableError::AlreadyMapped` at the PDPT level. User images link
 * above the identity window; 5 GiB also stays clear of the reserved
 * shared pages (VM_BOOT_HANDOFF_VA @ 0x1_0000_0000, KERNINFO_USER_VA @
 * 0x2_0000_0000) and of the loader stack at user_sp - 64 KiB.
 *
 * Every output section starts on its own page (`. = ALIGN(4096)`) so the
 * PT_LOAD vaddrs are page-aligned and page-disjoint — the kernel boot
 * loader (`load_vm_elf`) maps each segment's pages independently and a
 * shared boundary page comes back AlreadyMapped.
 */
OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

SECTIONS
{
    /* 0x140000000 == 5 GiB (ld numbers take no digit separators). */
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
