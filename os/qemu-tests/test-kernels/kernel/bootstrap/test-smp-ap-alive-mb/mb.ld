/* Multiboot 直启测试内核 — x86-64，恒等（低）链接于 1MB。
 * QEMU -kernel（multiboot ELF 路径）按段加载；入口 _start（32 位）。 */

OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

SECTIONS
{
    . = 0x100000;

    .mbh : { KEEP(*(.multiboot_header)) }

    .text : { *(.text .text.*) }

    .rodata : ALIGN(8) { *(.rodata .rodata.*) }

    .data : ALIGN(8) { *(.data .data.* .mb_gdt) }
    mb_load_end = .;

    .bss : ALIGN(8) { *(.bss .bss.* .COMMON) }
    mb_bss_end = .;

    . = ALIGN(16);
    mb_stack_bottom = .;
    . += 0x40000;
    mb_stack_top = .;
    mb_entry = _start;

    /DISCARD/ : { *(.eh_frame*) *(.comment) *(.note*) }
}
