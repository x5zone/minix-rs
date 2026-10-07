# 00 — vm-overview：内存语义权威的边界、事件循环与阅读路线

> **状态**: 已完成（2026-09-09 改写；随收尾 campaign 刷新实施现状）
> **定位**: 总览（入口文档）——VM 的角色边界、事件循环骨架、模块分层与 26 篇文档导航
> **源码**: minix3/minix/servers/vm/ 全部（24 个 .c，11,466 行 + glo.h/vm.h/com.h）
> **Rust 模块**: os/servers/vm/src/ 全部（51 个 .rs，约 2.9 万行）
> **前置阅读**: 无；**后继**: 99（常量与全局词汇表）→ 02/03 → 05/06 → 16/17 → 各机制

## 1 概念

### 1.1 微内核里"谁管理内存语义"

Minix3 的内核只保留调度与 IPC 原语。进程地址空间的**语义**——一个进程有哪些虚拟区域、每个虚拟页背后是哪个物理帧、缺页由谁仲裁（CoW/需求页/文件页/共享页）、物理内存怎么分配与回收、文件映射与共享映射采用什么策略——全部由用户态的 **VM 服务器**持有。VM 是全系统唯一能改写进程页表的用户态进程：它直接持有每个进程的页表对象（`vm_pt`），写 PTE 后经直接映射逐条 invlpg（08 §1.8）。

这份权威的代价与 PM 同构：每个语义动作都要跨消息完成。fork 是 VM_FORK 里的表复制与 CoW 预写；exit 是 VM_WILLEXIT/VM_EXIT 两步的资源清算；缺页是内核停住进程、VM 仲裁、内核恢复的三方协议。VM 的全部代码就是这个"内存语义协议"的实现。

### 1.2 三重权威与执行模型

1. **地址空间账本唯一所有者**：`vmproc[]` + 每进程 `vir_region` 区域映射 + `PageFrames` 逐帧引用计数（05/03/02）。
2. **缺页仲裁者**：五优先级主循环里最高优先级的特殊路径——目标进程带 RTS_PAGEFAULT 停等，VM 仲裁后写 PTE、清标志、内核恢复（16）。
3. **分配与缓存策略层**：物理分配三后端（bitmap/buddy/segment-tree，05/06）+ 页缓存 LRU（24）+ 六种 memtype 策略（09–14，12）。

执行模型是**单线程事件循环**（区别于 kernel 的 SMP+BKL）：所有状态在一个线程上流转，Rust 的借用检查器就是并发审计器——`VmContext` 字段显式化、`AssumeSyncCell` 只在跨槽位访问处出现。这条模型同时是 TLB 纪律的根基："VM 只改不在运行的进程的页表"这一结构不变量（16 §3.7）使得写后 invlpg 的逐条失效即已完备。

### 1.3 事件循环骨架

`run()` 每轮 `run_once`（镜像 C main.c:137-176）：receive → 五优先级分发（VFS transid → RS_INIT → VM_PAGEFAULT → ACL 闸 + CALLMAP → 越界 ENOSYS）→ SUSPEND 特判（不回复，等续作）→ 回复编码发送。传输失败有连续计数上限（fail-fast），进程级失败有审计与计数（fail-closed）。骨架细节归 01/15；每条调用臂的语义归各自机制文档。

## 2 源码地图与文档导航

### 2.1 24 个 C 文件 → 26 篇文档

| 域 | C 源文件 | 文档 |
|----|----------|------|
| 骨架与分发 | main.c、glo.h、pagefaults.c | 01/15/16 |
| 账本与页表 | region.c、regionavl、pagetable.c、pt.h | 02/03/07/08 |
| 物理分配 | alloc.c、pb.c、slaballoc.c、util.h | 05/06/09 |
| memtype 策略 | mem_anon/anon_contig/directphys/shared/cache/file.c、memtype.h | 10–14、12 |
| 系统调用域 | fork.c、exit.c、break.c、mmap.c、region.c 辅助 | 17–22 |
| 服务协同 | rs.c、vfs.c | 25/23 |
| 查询与诊断 | utility.c、sanitycheck | 26 |
| 共享词汇 | vm.h、com.h、ipc.h | 99 |

### 2.2 Rust 模块镜像

```
os/servers/vm/src/
├── vm_server.rs      编排（VmServer + VmContext + run_once 五优先级 + 回复发送）
├── main.rs/lib.rs    入口与门面
├── global.rs/boot.rs 启动全局与 handoff 消费
├── ipc/              dispatcher（CALLMAP）+ cache_handlers + encode + transport
├── region/           vir_region + region_map + page_state（账本核心，无上层依赖）
├── phys_mem/         三后端分配器 + stats + parity 测试（05/06）
├── memtype.rs        六 memtype 策略 trait（12）
├── page_cache.rs     页缓存（24）
├── vmproc/           进程状态（table/proc/handle/flags）
├── cow_exec_pf.rs    缺页仲裁与 CoW（16/17）
├── brk/mmap/munmap/exit/fork/map_phys/query/rs  系统调用与服务协同域
├── fdref.rs/vfs_queue.rs/sanity.rs/audit.rs/direct_map.rs  基础设施
└── pagetable/        VM 自身页表（vm_self_map + sim 注入）
```

### 2.3 推荐阅读路线

**99**（常量与全局词汇表）→ **02/03**（vmproc 与区域账本）→ **05/06**（物理分配）→ **16/17**（缺页与 CoW）→ **15**（分发全景）→ **08/24**（页表操作与页缓存）→ **18–22/25/26**（系统调用域与协同/查询）。

## 3 实施现状总览

### 3.1 分发与覆盖

CALLMAP **26/26 注册对齐 C**（`build_callmap` 镜像 main.c:543-575；EXEC_NEWMEM/DMA 四调用双侧同为 ENOSYS，T19 判定）；`NR_VM_CALLS=49` 单源于 minix-types。覆盖率机器口径：371 个 C 符号、文档覆盖 **92.5%**（coverage-extract，2026-09-09）；175 项逐一语义判定见 todo §17.1，REAL-GAP 已清零。

### 3.2 测试基线

三 feature 矩阵 **503/521/503 passed**（2026-09-09，Fix #80 后）；servers/vm clippy 0 警告。修复账目（Fix #63–#81）与判定记录见 todo.md §18/§18.9。

### 3.3 跨阶段依赖（edge 批，通电前置）

minix-sys trap 层（E1）与 SYS_* wrapper（E2）是全部真实 IPC 通电的前置；E-RSWIRE（rproctab 解码 + ACL u64 掩码）、E-VFSWIRE（VFS_VMCALL 定稿）、E-VMTLB（SMP TLB 刷新）按 edge_todo.md 单线程推进。

## 4 实施详解

### 4.1 接缝与注入

内核能力的唯一出口是 `KernelGateway` seam（生产 `TrapKernelGateway` / 测试 `MockGateway`，T9）；页表面可注入 `SimPaging`（T21）——两者使缺页/CoW/页表语义在无真实硬件时即可单测驱动。分配器三后端经 cargo feature 组合切换，parity 由 allocator_tests 对账（T22）。

## 5 测试点

00 为导航文档，无独立机制测试声称；全 crate 基线见 §3.2，逐篇矩阵在各机制文档 §5。

## 6 过渡

前置阅读：无。后继：99（跨机制共享的常量、旗标与全局状态词汇表）。

## 7 参见

- todo.md：§17/§18（V12/V13 轮判定与 Fix 账目）、§18.8（剩余批次）
- edge_todo.md：E1/E2/E5/E-RSWIRE/E-VFSWIRE/E-VMTLB（跨阶段条目唯一入口）
- plan.md：§1.2（启动主线）、§3.3（编号规则）、ARCH 清单（A-1/A-5/A-6/A-11/A-14）
- 01-stage-kernel（内核对端）、04-stage-pm / 05-stage-vfs / 03-stage-rs（协同域）
