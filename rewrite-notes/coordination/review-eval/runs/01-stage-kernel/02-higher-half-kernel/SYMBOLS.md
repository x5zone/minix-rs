# D02 SYMBOLS（R5）

> 覆盖口径：`tools/anchor-resolve.sh --check` 解析 66 个符号锚点，全部 resolved，zero-def=0，multi-def=0；`tools/doc-code-map.sh --check` 列 30 个关联 Rust 文件，30/30 存在。下表按语义层列代表符号。

## 链接与摆放

| 符号 | 位置 | 角色 |
|---|---|---|
| `KERNEL_VIRT_BASE` / `KERNEL_PHYS_BASE` / `KERNEL_HIGH_BASE` | `os/kernel-image/{x86_64,aarch64,riscv64}.ld` | 链接脚本的基址常量 |
| `AT(ADDR(sec) - …)` | 同三份脚本 | LMA 平移表达式；J1 的组内一致 |
| `compute_kernel_layout` | `os/boot-shim/src/loader.rs:fn compute_kernel_layout` | ELF `PT_LOAD` → 布局/跨距 |
| `KernelImage` / `ValidatedKernelImage` | `os/libs/minix-elf/src/lib.rs`、`loader.rs` | 解析与两道目标侧校验的类型门 |
| `load_segments_into_phys_memory` | `os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory` | 按 `p_paddr` 写段体、清 BSS |

## 映射与建根

| 符号 | 位置 | 角色 |
|---|---|---|
| `build_bootstrap_root_and_enable` | `os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable` | Step 1–3：恒等、内核高半、开分页 |
| `plan_kernel_mapping` | `os/kernel/src/boot_policy.rs:fn plan_kernel_mapping` | 首选/回退页尺寸的阶梯 |
| `map_huge` / `AlreadyMapped` | `os/arch/src/{x86_64,aarch64,riscv64}/paging.rs` | 大页安装；重复/降级拒绝 |
| `kernel_identity_mmio_regions` | `os/arch/src/lib.rs:fn kernel_identity_mmio_regions` | Step 1 的固定 MMIO 表（x86 为空） |
| `device_window_regions` | `os/arch/src/arch/device_window.rs` | Step 2c 的 `bootstrap=true` 设备窗口 |
| `BOOT_IDENTITY_MAP_END` | `os/arch/src/lib.rs:const BOOT_IDENTITY_MAP_END` | 恒等窗口的 VA 上限 |

## 换栈与进入

| 符号 | 位置 | 角色 |
|---|---|---|
| `HigherHalf` | `os/kernel/src/boot/higher_half.rs:trait HigherHalf` | 换栈 + 跳转的契约；返回 `!` |
| `jump_to_kmain` | 三份 `arch/*/higher_half.rs` | 三架构 asm；x86 `call`、aarch64 `br`、riscv64 `jalr` + `delta` |
| `KernelInfo::kern_stack_top` | `os/libs/minix-boot/src/kernel_info.rs:fn kern_stack_top` | `kern_virt_base + kern_size` 的派生访问器 |
| `kmain_verify` | `os/kernel/src/lib.rs:fn kmain_verify` | `test-higher-half` 的 SP/FP/PC 断言 |
| `arch_boot` / `arch_boot_finish` / `arch_boot_impl` | `os/kernel/src/lib.rs` | 直核入口、校验、策略/Step 1–4 |
| `arch_boot_resume_v1` / `resume_additive` | `os/kernel/src/lib.rs` | v1 接收侧 additive 阶段 |

## C 对照

| 符号 | 位置 | 角色 |
|---|---|---|
| `kernel.lds` 的 `AT()` / `_kern_offset` | `minix3/minix/kernel/arch/i386/kernel.lds` | C 的双地址布局 |
| `pg_identity` / `pg_mapkernel` | `minix3/minix/kernel/arch/i386/pg_utils.c` | 4 GiB 恒等 + 内核高半 4 MB 页 |
| `k_initial_stktop` | `minix3/minix/kernel/arch/i386/mpx.S` | C 的入口栈顶（`.bss`，高半） |
| `head.S` 的 `multiboot_init` | `minix3/minix/kernel/arch/i386/head.S` | 设栈、压参数、`call kmain` |

## 覆盖声明

- 文档内所有 `os/...:fn|struct|const|static` 与 `minix3/...:func|struct` 锚点均由 `anchor-resolve.sh` 解析；没有手工行号锚点。
- 未解析符号：0。关联代码缺失：0。旧节号不在此表；旧→新映射只在 `rewrite-notes/MIGRATION.md`。
