/*
 * x86_64 生产内核镜像的链接脚本（NS8-A）。
 *
 * 布局契约（消费方 = os/boot-shim/src/loader.rs）：
 *   - PT_LOAD 的 LMA（p_paddr）是物理装载地址，boot-shim
 *     load_segments_into_phys_memory 按它拷贝段体并清零 BSS；
 *   - PT_LOAD 的 VMA（p_vaddr）是内核虚拟地址，boot-shim
 *     compute_kernel_layout 取 min(vaddr)/min(paddr) 填
 *     KernelInfo.kern_virt_base / kern_phys_base，内核页表把
 *     [kern_virt_base, +kern_size) 映射到 [kern_phys_base, +kern_size)。
 *
 * 高半段惯例（Linux __START_KERNEL_map / Redox KERNEL_BASE 同形）：
 *   vaddr = KERNEL_HIGH_BASE + paddr，偏移对所有段一致——这是
 *     "一个 kern_virt_base ↔ kern_phys_base 平移映射"成立的前提。
 *   首段 LMA 定在 0x200000（与 test-kernel 载体硬编码的
 *     KernelInfo.kern_phys_base 同值；2MiB 对齐避开实模式/EBDA 区）。
 *
 * 入口：_start（.text.boot 段，见 src/main.rs 的 global_asm）。
 * C 对位：Minix3 C 内核镜像同样由链接脚本定
 *   phys/virtual 双址布局（minix3 kernel Makefile + ld script）；
 *   本脚本是 Rust 侧重表达，非 1:1 直译。
 */

ENTRY(_start)

KERNEL_HIGH_BASE = 0xFFFF800000000000;
KERNEL_PHYS_BASE = 0x200000;

SECTIONS
{
    . = KERNEL_HIGH_BASE + KERNEL_PHYS_BASE;

    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_HIGH_BASE)
    {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_HIGH_BASE)
    {
        /* 内核启动图锚点（src/main.rs KERNEL_ENTRY_ANCHOR）：KEEP 防
           --gc-sections 把无引用静态连同整个内核启动图一起裁掉。 */
        KEEP(*(.rodata.kernel_anchor))
        *(.rodata .rodata.*)
    }

    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_HIGH_BASE)
    {
        /* .ldata = rustc 大数据拆分节（同 .lbss 家族），漏匹配即成孤儿节
           排到 ALIGN 之后、撑破镜像跨距对齐（NK4-A 首亮实证）。 */
        *(.data .data.* .ldata .ldata.*)
        *(.got .got.*)
    }

    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_HIGH_BASE)
    {
        /* SD-7 / P-X86-03（2026-10-06）：入口 asm 的 .bss 清零循环需要段
           边界符号——与 riscv64.ld 同名同形（防御性对齐：UEFI boot-shim
           的 loader 已按 memsz-filesz 清零，但 SD-1 多入口形态下未来直载
           入口不再依赖装载者行为）。 */
        __bss_start = .;
        /* .lbss = rustc 大 BSS 拆分节（PANIC_DIAGNOSTIC_HOOK 等），必须
           归入本节才能落在 2MiB 收口之内。 */
        *(.bss .bss.* .lbss .lbss.*)
        *(COMMON)

        . = ALIGN(16);
        kernel_boot_stack_bottom = .;
        . += 0x10000;    /* 64 KiB 引导栈（_start 立栈用） */
        kernel_boot_stack_top = .;

        __bss_end = .;

        /* 镜像跨距 2MiB 收口（NK4-A 首亮实断）：boot_validate_and_prepare
           按 huge-page 阶梯选映射粒度（1GiB→2MiB，lib.rs 选块），kern_size
           非 2MiB 对齐即 panic；kern_stack_top（镜像尾）同时获得对齐。
           收口属于末段 memsz，boot-shim 的 BSS 清零与 kern_size 计算都以
           末段 memsz 端点为准。 */
        . = ALIGN(0x200000);
    }

    /DISCARD/ :
    {
        *(.eh_frame .eh_frame.*)
        *(.comment)
        *(.note .note.*)
    }
}
