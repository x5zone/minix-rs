/*
 * aarch64 生产内核镜像的链接脚本（NK4-B P3 M3.2）。
 *
 * 布局契约与 x86_64.ld 完全同形（消费方 = os/boot-shim/src/loader.rs）：
 *   - PT_LOAD 的 LMA（p_paddr）是物理装载地址，boot-shim
 *     load_segments_into_phys_memory 按它拷贝段体并清零 BSS；
 *   - PT_LOAD 的 VMA（p_vaddr）是内核虚拟地址，boot-shim
 *     compute_kernel_layout 取 min(vaddr)/min(paddr) 填
 *     KernelInfo.kern_virt_base / kern_phys_base，内核页表把
 *     [kern_virt_base, +kern_size) 映射到 [kern_phys_base, +kern_size)。
 *
 * 与 x86_64 的唯一实质差别是物理基址（见下方 KERN_PHYS_BASE 的来由），
 * 因此 vaddr 与 paddr 的平移量不再是 `KERNEL_HIGH_BASE` 本身，而是
 * `KERNEL_VIRT_BASE - KERNEL_PHYS_BASE`；AT() 表达式按此写，保证所有段
 * 的 `vaddr - paddr` 一致——这是"一个 kern_virt_base ↔ kern_phys_base
 * 平移映射"成立的前提（x86_64.ld 头注释的同一条契约）。
 *
 * 两个基址都不是新发明，取自仓内既有 aarch64 内核侧与载体侧常量：
 *   - KERN_VIRT_BASE = 0xFFFF800000000000：`os/arch/src/arm64/paging.rs`
 *     把 TTBR0/TTBR1 钉在同一张 512 项 L0 根表上，T1SZ=16 ⇒ 高半区
 *     （VA >= 0xFFFF000000000000）落在 L0 表项 256..512，内核直接映射
 *     窗口写死在 0xFFFF800000000000 = L0 slot 256（该文件 :792）；
 *   - KERN_PHYS_BASE = 0x40200000：QEMU virt（arm64）DRAM 从
 *     0x40000000 起，+2MiB 且 2MiB 对齐；既有内核侧脚本
 *     `os/kernel/src/arch/aarch64/link.ld` 与六个 aarch64 载体
 *     （hello-boot / test-higher-half / test-kernel-map /
 *     test-paging-enable / test-protection / test-shutdown）同值。
 *     x86_64 的 0x200000 在这里不适用：aarch64 QEMU virt 上它不是 RAM。
 *
 * 入口：_start（.text.boot 段，见 src/main.rs 的 global_asm）。
 * C 对位：同 x86_64.ld——Minix3 C 内核镜像由 kernel Makefile + ld 脚本
 *   定 phys/virtual 双址布局，本脚本是 Rust 侧重表达，非 1:1 直译。
 */

OUTPUT_ARCH(aarch64)
ENTRY(_start)

KERNEL_VIRT_BASE = 0xFFFF800000000000;
KERNEL_PHYS_BASE = 0x40200000;

SECTIONS
{
    . = KERNEL_VIRT_BASE;

    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)
    {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)
    {
        /* 内核启动图锚点（src/main.rs KERNEL_ENTRY_ANCHOR）：KEEP 防
           --gc-sections 把无引用静态连同整个内核启动图一起裁掉。 */
        KEEP(*(.rodata.kernel_anchor))
        *(.rodata .rodata.*)
    }

    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)
    {
        /* .ldata = rustc 大数据拆分节（同 .lbss 家族），漏匹配即成孤儿节
           排到 ALIGN 之后、撑破镜像跨距对齐（NK4-A 首亮实证）。 */
        *(.data .data.* .ldata .ldata.*)
        *(.got .got.*)
    }

    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)
    {
        /* .lbss = rustc 大 BSS 拆分节（PANIC_DIAGNOSTIC_HOOK 等），必须
           归入本节才能落在 2MiB 收口之内。 */
        *(.bss .bss.* .lbss .lbss.*)
        *(COMMON)

        . = ALIGN(16);
        kernel_boot_stack_bottom = .;
        . += 0x10000;    /* 64 KiB 引导栈（_start 立栈用） */
        kernel_boot_stack_top = .;

        /* 镜像跨距 2MiB 收口（NK4-A 首亮实断，arm64 同理）：
           boot_validate_and_prepare 按 huge-page 阶梯选映射粒度
           （1GiB→2MiB），kern_size 非 2MiB 对齐即 panic；
           kern_stack_top（镜像尾）同时获得对齐。
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
