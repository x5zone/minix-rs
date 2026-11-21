# 01-boot-shim-bootstrap 大纲 v5（2026-11-21，按“交接载荷”轴重写）

> 新轴：01 = 交接载荷（跨边界的数据：`KernelInfo`、内存账、bootstrap 资源、校验、发送侧）。
> 02 = 内核镜像（链接布局、装载摆放、运行期映射、换栈进入）。
> 本大纲是正文结构契约；旧 v4 仅作事实来源参考。

## 1. 概述

| 节 | 讲什么 | 知识点 | 代码落点 | 教学要点 |
|---|---|---|---|---|
| 1.1 | 内核接手时拿到什么 | 三份数据 + 一条接缝：可用内存清单、内核位置记录、交接结构；页表归 02 | `pre_init.c:get_parameters`、`kernel_info.rs:struct KernelInfo` | 先建立“载荷”视角 |
| 1.2 | 四条不变量 I1–I4 | 值可跨镜像、来源可追溯、所有权明确、校验先于消费 | 全篇 | 每章至少服务一条不变量 |
| 1.3 | 与 Minix3 的对应 | multiboot kinfo → KernelInfo；`pre_init` 接缝；`get_parameters` 数据半 | `param.h:struct kinfo`、`pre_init.c` | C 的数据半 vs 镜像半 |
| 1.4 | 范围与术语 | 接缝声明；栈名表只留 `load_stack_start`/`user_sp`；D1/D2/D4 | 全篇 | 明确不读镜像细节 |

## 2. C 源码分析

| 节 | 讲什么 | 知识点 | 代码落点 |
|---|---|---|---|
| 2.1 | 引导器契约与启动信息 | Multiboot 头/flags/checksum；mmap；modules；boot2 两条命令 | `head.S:multiboot_init`、`multiboot.h`、`boot2.c:command_load_mods` |
| 2.2 | `head.S` 第一跳 | `load_stack_start → pre_init`；三件事 | `head.S:multiboot_init` |
| 2.3 | `get_parameters` | 复制 mbi、链接期常量、命令扫描、param_buf、rootdevname | `pre_init.c:get_parameters` |
| 2.4 | 内存账 | `add_memmap`/`cut_memmap`；取整、夹取、空槽；条目来源 | `pg_utils.c:add_memmap`、`pg_utils.c:cut_memmap` |
| 2.5 | `overlaps` 与 `kern_mod` | 半开区间；内核当第 M+1 个模块 | `pre_init.c:overlaps`、`pre_init.c:get_parameters` |
| 2.6 | 模块装载的检查与失败模式 | `load_mods` 次序、条数断言、重叠 panic | `exec.c:exec_multiboot`、`boot2.c:command_load_mods` |
| 2.7 | ARM32 对照 | 自造 multiboot；闭区间差异 | `earm/head.S`、`earm/pre_init.c` |
| 2.8 | 本章归纳 | C 交付物形状：三份数据 + 接缝 | 全章 |

## 3. Rust 设计决策

| 节 | 讲什么 | 知识点 | 代码落点 |
|---|---|---|---|
| 3.1 | 协议换代 | 固件接口替换 Multiboot；EBS 三件失效服务 | `uefi_helpers.rs:prepare_and_enter_kernel_image` |
| 3.2 | 按值交接 | `KernelInfo` 字段台账；wire 只传值；深拷 | `kernel_info.rs:struct KernelInfo`、`handoff_v1.rs:struct BootHandoffV1` |
| 3.3 | 两张内存表与来源记录 | `memmap`/`reserved_regions`；`SourceRecord`；`RamAuthority` | `kernel_info.rs:struct MemoryRegion`、`memmap_source.rs:struct SourceRecord` |
| 3.4 | bootstrap 区间与分配器换手 | `BumpRegion`；`init`/`resume`；`free_upper_idx` 退场 | `boot_alloc.rs:fn init_boot_pt_alloc`、`boot_alloc.rs:fn resume_boot_pt_alloc` |
| 3.5 | 取件抽象与依赖方向 | `FileLoader`；`minix-boot` 共享类型；单向依赖 | `loader.rs:trait FileLoader`、四份 `Cargo.toml` |
| 3.6 | 五条生产腿与发送侧 | 两条 UEFI、U-Boot shim、riscv64/aarch64 直核；共享 `run_self_boot` | `bootface_common.rs:fn run_self_boot`、`uefi_helpers.rs`、`opensbi_helpers.rs` |
| 3.7 | 校验门链与所有权 | `validate_before_tables`/`validate_requiring_sources`；source 覆盖 | `kernel_info.rs:impl KernelInfo` |

## 4. 实现详解

| 节 | 讲什么 | 知识点 | 代码落点 |
|---|---|---|---|
| 4.1 | 入口与调用链 | 五腿入口分道；接收侧只留指针 | `main.rs:rust_image_main`、`lib.rs:enter_kernel_image_v1` |
| 4.2 | UEFI 九步 | 时间线；新增“产出归属”列，镜像步骤指向 02 | `uefi_helpers.rs:prepare_and_enter_kernel_image` |
| 4.3 | 两种文件来源共用取件 | `KERNEL_PATH`、`MODULE_NAMES`、`read_required` | `loader.rs:fn load_kernel_with_loader_gated` |
| 4.4 | KernelInfo 两条产出入径 | `build_kernel_info`；direct 面/接收面 | `boot-shim/src/lib.rs:fn build_kernel_info` |
| 4.5 | `store_kernel_info` 六个落地区 | 容量、截断、名字池、同址跳过 | `kernel/src/lib.rs:fn store_kernel_info` |
| 4.6 | arena 与分配器换手 | 8 页 arena；`BootAlloc` 续游标 | `lib.rs:fn enter_kernel_image_v1`、`boot_alloc.rs` |
| 4.7 | 与 C 函数落点对照 | `get_parameters`/`add_memmap`/`cut_memmap` → Rust | 全章 |

## 5. 测试：四条不变量各自的证据

| 节 | 讲什么 | 知识点 |
|---|---|---|
| 5.1 | I1 值可跨镜像 | 字段台账、wire round-trip、深拷测试 |
| 5.2 | I2 来源可追溯 | `SourceRecord`、`audit_source_coverage`、负控 |
| 5.3 | I3 所有权明确 | `BootStorage`、`BootReport`、landing 区测试 |
| 5.4 | I4 校验先于消费 | `validate_*` 负测、五腿端到端 |
| 5.5 | 各腿端到端 | sysboot、U-Boot shim、riscv bootface、aarch64 直核 |

## 6. 诊断

| 节 | 讲什么 |
|---|---|
| 6.1 | EBS 分界与裸串口 |
| 6.2 | panic 路径 |
| 6.3 | 不变量被破坏时的症状 |
| 6.4 | 可复制命令 |

## 7. 参见

前置/后续篇、C/Rust 清单、构建入口、符号速查。
