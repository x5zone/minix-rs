/* rt-birth link script — freestanding minix-rt user image (riscv64).
 *
 * K12b riscv64 leg. Link base 0x140000000 (5 GiB), same rationale as the
 * x86-64 script: above the bootstrap root's identity-mapped DRAM window
 * (QEMU virt DRAM sits at 2-2.25 GiB under Sv39's 1 GiB-leaf identity),
 * clear of the reserved shared pages (VM_BOOT_HANDOFF_VA @ 0x1_0000_0000,
 * KERNINFO_USER_VA @ 0x2_0000_0000) and of the loader stack (the carrier
 * places user_sp near the top of Sv39's 256 GiB user window). 5 GiB is
 * inside Sv39's 39-bit user VA half, so no paging limits are approached.
 *
 * Every output section starts on its own page (`. = ALIGN(4096)`) so the
 * PT_LOAD vaddrs are page-aligned and page-disjoint — the kernel boot
 * loader (`load_vm_elf`) maps each segment's pages independently and a
 * shared boundary page comes back AlreadyMapped.
 */
OUTPUT_ARCH(riscv)
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
        *(.sdata .sdata.*)
    }

    . = ALIGN(4096);
    .sbss : {
        *(.sbss .sbss.*)
    }
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
