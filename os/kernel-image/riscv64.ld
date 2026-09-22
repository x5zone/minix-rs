/*
 * riscv64 生产内核镜像的链接脚本（NK4-B P4 M4.2）。
 *
 * 布局契约与 aarch64.ld 完全同形（消费方 = os/boot-shim/src/loader.rs）：
 *   - PT_LOAD 的 LMA（p_paddr）是物理装载地址，boot-shim
 *     load_segments_into_phys_memory 按它拷贝段体并清零 BSS；
 *   - PT_LOAD 的 VMA（p_vaddr）是内核虚拟地址，boot-shim
 *     compute_kernel_layout 取 min(vaddr)/min(paddr) 填
 *     KernelInfo.kern_virt_base / kern_phys_base，内核页表把
 *     [kern_virt_base, +kern_size) 映射到 [kern_phys_base, +kern_size)。
 *
 * 与 aarch64 的唯一实质差别是两个基址（见下），AT() 表达式与段序、KEEP
 * 名单、引导栈预留、2MiB 收口逐条照抄 aarch64.ld——`vaddr - paddr` 对所有
 * 段一致这条前提也因此一致（check-layout.sh 的 L4 断言）。
 *
 * 两个基址都不是新发明，取自仓内既有 riscv64 常量（三处独立来源）：
 *   - KERNEL_VIRT_BASE = 0xFFFFFFC000000000：Sv39 是 39 位虚拟地址再按
 *     bit38 符号扩展到 64 位，canonical high 区起点即 VPN[2] = 256 处
 *     （`os/arch/src/arch/direct_map.rs:133`、宿主测试
 *     `os/kernel/src/lib.rs:4215`、既有内核侧脚本
 *     `os/kernel/src/arch/riscv64/link.ld:21` 同值）。
 *     注意 0xFFFFC00000000000 与 0xFFFFFC0000000000 **不是**合法的 Sv39
 *     canonical high——它们 VPN[2] = 0，与低半地址重合（旧脚本头部
 *     同一文件 6-14 行的警示，这里一并抄下免得后人重踩）。
 *   - KERNEL_PHYS_BASE = 0x80200000：QEMU virt（riscv64）DRAM 从
 *     0x80000000 起，+2MiB 且 2MiB 对齐（`os/kernel/src/lib.rs:4154/4216`，
 *     与 `os/arch/src/riscv64/paging.rs:883` 的测试地址族一致）。
 *     x86_64 的 0x200000 在这里不适用：riscv64 QEMU virt 上它不是 RAM。
 *
 * 入口：_start（.text.boot 段，见 src/main.rs 的 global_asm）。入口态由
 * OpenSBI 以 S-mode 跳入（a0 = hartid、a1 = DTB 物理指针），但把 a1 递进
 * KernelInfo 属尚未接线的 boot-shim→内核交接协议（NK1/OQ-N6，与
 * x86_64/aarch64 两份脚本同一条边界）。
 * C 对位：同 x86_64.ld / aarch64.ld——Minix3 C 内核镜像由 kernel Makefile
 *   + ld 脚本定 phys/virtual 双址布局，本脚本是 Rust 侧重表达，非 1:1 直译。
 */

OUTPUT_ARCH(riscv)
ENTRY(_start)

KERNEL_VIRT_BASE = 0xFFFFFFC000000000;
KERNEL_PHYS_BASE = 0x80200000;

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
        /* .srodata/.sdata2 = riscv 小数据优化里的只读那一半（gp 相对寻址），
           必须归可执行只读段：落到 .bss（NOBITS）会丢掉文件内容。 */
        *(.rodata .rodata.* .srodata .srodata.* .sdata2 .sdata2.*)
    }

    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)
    {
        /* .ldata = rustc 大数据拆分节（同 .lbss 家族），漏匹配即成孤儿节
           排到 ALIGN 之后、撑破镜像跨距对齐（NK4-A 首亮实证）。
           .sdata = riscv 小可写数据拆分节，同样不被 `.data.*` 通配命中。 */
        *(.data .data.* .ldata .ldata.* .sdata .sdata.*)
        *(.got .got.*)
    }

    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)
    {
        /* .lbss = rustc 大 BSS 拆分节（PANIC_DIAGNOSTIC_HOOK 等），
           .sbss = riscv 小可写零初始化拆分节；两者都必须归本节才能落在
           2MiB 收口之内。 */
        *(.bss .bss.* .lbss .lbss.* .sbss .sbss.*)
        *(COMMON)

        . = ALIGN(16);
        kernel_boot_stack_bottom = .;
        . += 0x10000;    /* 64 KiB 引导栈（_start 立栈用） */
        kernel_boot_stack_top = .;

        /* 镜像跨距 2MiB 收口（Sv39 的大页阶梯就是 2MiB 叶子）：
           boot_validate_and_prepare 按 huge-page 阶梯选映射粒度，
           kern_size 非 2MiB 对齐即 panic；kern_stack_top（镜像尾）同时
           获得对齐。收口属于末段 memsz，boot-shim 的 BSS 清零与
           kern_size 计算都以末段 memsz 端点为准。 */
        . = ALIGN(0x200000);
    }

    /DISCARD/ :
    {
        *(.eh_frame .eh_frame.*)
        *(.comment)
        *(.note .note.*)
        *(.riscv.attributes)
    }
}
