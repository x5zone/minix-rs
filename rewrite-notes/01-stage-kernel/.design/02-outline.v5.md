# 02-higher-half-kernel 大纲 v5（2026-11-21，按“内核镜像”轴重写）

> 新轴：02 = 内核镜像（链接布局 → 装载摆放 → 运行期映射 → 成对进入）。
> 本大纲是正文结构契约；旧 v4 仅作事实来源参考。

## 1. 概述

| 节 | 讲什么 | 知识点 | 代码落点 | 教学要点 |
|---|---|---|---|---|
| 1.1 | 镜像的四个时刻 | 链接/摆放/映射/进入 | 全篇 | 四个时刻各有断言 |
| 1.2 | 四条不变量 J1–J4 | 平移一致、跨距与粒度、映射先于进入、换栈成对不返回 | 全篇 | 每章服务一条 |
| 1.3 | 范围与接缝 | 载荷归 01；本文只讲镜像 | 01 §1 | 不重复 wire 字段 |

## 2. C 源码分析

| 节 | 讲什么 | 知识点 | 代码落点 |
|---|---|---|---|
| 2.1 | `kernel.lds` | 双地址、`AT()`、位置计数器 | `kernel.lds`、`pg_utils.c:pg_mapkernel` |
| 2.2 | 两段式布局 | unpaged 段与高半段；改名名单 | `Makefile.inc`、`kernel.lds` |
| 2.3 | `head.S` 第二跳 | `mov %esp`+`call`；相对/绝对寻址 | `head.S:multiboot_init` |
| 2.4 | 建表五步与位表 | `pg_clear/identity/mapkernel/load/enable`；PWT/PCD | `pg_utils.c`、`pg_utils.c:pg_mapkernel` |
| 2.5 | `k_initial_stktop` 落段 | 不在 unpaged 名单 → 高半 | `mpx.S`、`Makefile.inc` |
| 2.6 | 本章归纳 | C 的“布局-摆放-映射-进入”四段 | 全章 |

## 3. Rust 设计决策

| 节 | 讲什么 | 知识点 | 代码落点 |
|---|---|---|---|
| 3.1 | 链接脚本三种形态 | 按“谁按装载地址摆放”分 | `os/kernel-image/*.ld` |
| 3.2 | ELF 解析器自建 | 五道校验；12 变体 | `minix-elf/src/lib.rs:struct Elf64Phdr` |
| 3.3 | 栈顶从符号到派生值 | `kern_stack_top = virt_base + size` | `kernel_info.rs:fn kern_stack_top` |
| 3.4 | 大页能力进扩展 trait | `HugePages: Paging`；首选/回退 | `paging_ext.rs:trait HugePages` |
| 3.5 | 跨距收口 | 2 MiB 对齐；L5/L9；栈在末段 | `os/kernel-image/*.ld` |
| 3.6 | 自举根与映射建立 | Step1 恒等/设备窗口；Step2 内核；Step2b/2c；enable | `kernel/src/lib.rs:fn build_bootstrap_root_and_enable` |
| 3.7 | 换栈与跳转 trait | `HigherHalf::jump_to_kmain`；三份 asm | `boot/higher_half.rs:trait HigherHalf` |
| 3.8 | 接收侧两条路径 | `resume_additive`/`arch_boot_finish`；additive 不重建根 | `kernel/src/lib.rs:fn resume_additive` |

## 4. 实现详解

| 节 | 讲什么 | 知识点 | 代码落点 |
|---|---|---|---|
| 4.1 | 链接脚本落地 | 三份脚本；ASSERT；L0–L9 输入 | `os/kernel-image/*.ld` |
| 4.2 | ELF 解析与摆放 | `KernelImage`/`ValidatedKernelImage`；两版装载器 | `boot-shim/src/loader.rs` |
| 4.3 | 自举根建立与开启 | 恒等窗口、内核映射、设备窗口 | `kernel/src/lib.rs:fn arch_boot_impl`、`arch_boot_finish` |
| 4.4 | 三架构成对跳转 | 指令序列、对齐、PC/delta | 三份 `arch/*/higher_half.rs` |
| 4.5 | 从装载到 kmain | 四腿路径；接收/直核合流 | `lib.rs:fn enter_kernel_image_v1`、`kernel-image/src/bootface_common.rs` |
| 4.6 | 与 C 对照 | `kernel.lds`/`head.S`/`pg_utils.c` 落点 | 全章 |

## 5. 测试

| 节 | 讲什么 | 知识点 |
|---|---|---|
| 5.1 | 宿主：解析器/装载器/建表 | minix-elf 27、loader 32、policy 测试 |
| 5.2 | 布局契约 L0–L9 | 唯一权威清单（从 01 迁来） |
| 5.3 | 真机：test-higher-half | SP/PC/FP、三架构 |
| 5.4 | 机制 → 需要证明什么 | J1–J4 反转核对 |

## 6. 诊断

症状→怀疑对象→确认命令；布局/映射/跳转三类。

## 7. 参见

前置/后续篇、C/Rust 清单、构建入口。
