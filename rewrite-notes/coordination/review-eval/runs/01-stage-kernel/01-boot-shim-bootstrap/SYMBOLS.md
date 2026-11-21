# D01 SYMBOLS（R37）

> 覆盖口径：`tools/anchor-resolve.sh --check` 解析 89 个符号锚点，全部 resolved，zero-def=0，multi-def=0；`tools/doc-code-map.sh --check` 列 48 个关联 Rust 文件，48/48 存在。下表按语义层列代表符号，不重复全文锚点。

## 交接载荷

| 符号 | 位置 | 角色 |
|---|---|---|
| `KernelInfo` | `os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo` | 接收侧进程内载荷；10 个字段 |
| `KernelInfo::validate*` | 同文件 | 字段/跨度/来源的共享校验体 |
| `BootReport` | 同文件 | 发送侧借用视图，不是 wire 载荷 |
| `BootHandoffV1` / `BOOT_HANDOFF_V1_VERSION` | `os/libs/minix-boot/src/handoff_v1.rs` | 跨镜像 wire 与版本门 |
| `store_kernel_info` | `os/kernel/src/lib.rs:fn store_kernel_info` | 六个 `.bss` 落地区的单次发布 |
| `BootStorage` | `os/libs/minix-boot/src/boot_storage.rs` | 六类载荷的单所有者；`fill_*` 一次提交 |

## 内存账与来源

| 符号 | 位置 | 角色 |
|---|---|---|
| `MemoryRegion` / `reserved_regions` | `kernel_info.rs` | 可用/保留两表 |
| `SourceRecord` / `audit_source_coverage` | `os/libs/minix-boot/src/memmap_source.rs` | 每个投影字节的来源审计 |
| `FreeRangeSet` | `os/kernel/src/vm_handoff.rs` | 内核侧扣减后的有序区间集合 |
| `classify_regions` / `build_vm_handoff` | `os/kernel/src/vm_handoff.rs` | VM 交接分类与扣除记录 |
| `PlatformDescSource` | `os/libs/minix-boot/src/kernel_info.rs` | DTB/ACPI 平台源窗口 |

## 引导资源与发送侧

| 符号 | 位置 | 角色 |
|---|---|---|
| `BootAlloc` | `os/kernel/src/boot_alloc.rs:struct BootAlloc` | 引导期页表页 bump 分配器；Acquire load / Release store |
| `init_boot_pt_alloc` / `register` | `os/arch/src/arch/pt_alloc.rs` | 页表分配器注册；二次注册 panic |
| `prepare_and_enter_kernel_image` | `os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image` | UEFI 发送侧主链 |
| `pair_and_classify` | `os/boot-shim/src/ebs_pair.rs` | `GetMemoryMap` + `ExitBootServices` 配对 |
| `enter_kernel_image_v1` / `prepare_kernel_image_v1` | `os/kernel/src/lib.rs` | EBS 后建根、补运行期字段、跳独立镜像 |
| `arch_boot_resume_v1` / `resume_additive` | `os/kernel/src/lib.rs` | v1 接收侧 additive 阶段 |

## C 对照

| 符号 | 位置 | 角色 |
|---|---|---|
| `pre_init` / `get_parameters` | `minix3/minix/kernel/arch/i386/pre_init.c` | C 侧数据入口 |
| `add_memmap` / `cut_memmap` | `minix3/minix/kernel/arch/i386/pg_utils.c` | 内存账维护 |
| `overlaps` / `kern_mod` | 同文件、`pre_init.c` | 内核作为第 M+1 个模块的重叠检查 |
| `kinfo_t` | `minix3/minix/include/minix/param.h:struct kinfo` | C 侧全局交接结构 |

## 覆盖声明

- 文档内所有 `os/...:fn|struct|const|static` 与 `minix3/...:func|struct` 锚点均由 `anchor-resolve.sh` 解析；没有手工行号锚点。
- 未解析符号：0。关联代码缺失：0。旧节号不在此表；旧→新映射只在 `rewrite-notes/MIGRATION.md`。
