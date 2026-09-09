# 99 — vm-global-concepts：跨机制共享的常量、旗标与全局状态

> **状态**: 已完成（2026-09-09 改写）
> **定位**: 全局概念（跨文档共享的词汇表）——任何机制文档里的魔数，其权威定义与 C 锚点都在这里
> **源码**: minix3/minix/servers/vm/ 的 glo.h、vm.h；minix/include/minix/com.h、ipc.h
> **Rust 模块**: os/libs/minix-types（ipc/vm.rs、types）、os/servers/vm/src（global.rs、mmap.rs、page_cache.rs、phys_mem/types.rs、vm_server.rs）
> **前置阅读**: 00；**后继**: 一切机制文档

## 1 概念

### 1.1 常量的语义半径

机制文档里的每个魔数——调用号为什么从 0xC00 起、PAF_LOWER16MB 为什么决定"分配器扫到哪"、VMSF_ONCE 为什么检查的是条目而非请求——其权威定义都在本章。本章关心每个常量的**语义半径**：它约束哪些行为、消费点在哪、两侧（C/Rust）是否同值同位。

三个代表性的例子。**NR_VM_CALLS=49**（com.h:769）不只是"调用号上限"：CALLMAP 数组长度、越界判定、注册对齐守护测试全部以它为界——minix-rs 曾双源定义（minix-types 与 vm_server 各一份），已单源化为 `minix_types::NR_VM_CALLS`。**VM_PAGEFAULT=VM_RQ_BASE+0xFF**（com.h:773）被刻意放在线性调用号区间之外：它的来源是内核异常路径而非用户消息，独立编码使主循环能在 ACL/越界判定之前就走特殊路径。**VMC_NO_INODE=0**（cache.h）依赖"dev=0 永不合法"这一事实——addcache 以 `dev == NO_DEV` 拒绝，0 才能安全地充当"无 inode"哨兵。

### 1.2 全局状态的显式化

C 的 VM 用文件级全局承载一切：`enable_filemap`、`total_pages`、`num_vm_instances`、`vm_running`、`missing_spares`、`spare_pagequeue`……全局使重入不可分析。Rust 的显式化分三档：**每请求上下文**进 `VmContext`（frames/cache/queue/gateway）；**跨请求恒量**进 `global.rs`（TOTAL_PAGES/VM_INSTANCE_COUNT/KERNEL_LAYOUT）或模块静态量（mmap.rs 的 FILEMAP_ENABLED）；**已被模型消除的**不再存在（missing_spares/spare_pagequeue 随 Direct Map 结构性删除，V12-P2-4）。

## 2 C 源码分析

### 2.1 调用号族（com.h）

| 常量 | 值 | C 锚点 | 语义 |
|------|-----|--------|------|
| VM_RQ_BASE | 0xC00 | com.h:627 | VM 调用号区间基址 |
| NR_VM_CALLS | 49 | com.h:769 | 调用号上限（CALLMAP 长度、越界判定） |
| VM_PAGEFAULT | VM_RQ_BASE+0xFF | com.h:773 | 内核异常路径专用号（线性区间外） |
| VM_EXIT/FORK/BRK/WILLEXIT… | +0..+48 | com.h:637-766 | 26 个注册调用（见 15 篇 CALLMAP 表） |
| VFS transid 位 | 高位标志 | main.c:143 | VFS 回复与 transid 复用 m_type 的编码 |

### 2.2 物理分配与写映射旗标（vm.h）

| 旗标 | 值 | C 锚点 | 语义半径 |
|------|-----|--------|----------|
| PAF_CLEAR | 0x01 | vm.h:22 | 分配后清零（Direct Map 逐页写零） |
| PAF_CONTIG | 0x02 | vm.h:23 | 定义后零消费（两侧连续性都来自原生多页分配，V12-P2-7 判定） |
| PAF_ALIGN64K/ALIGN16K | 0x04/0x20 | vm.h:24/:27 | 对齐预留（alloc_mem 内加页再裁剪，S-1） |
| PAF_LOWER16MB/LOWER1MB | 0x08/0x10 | vm.h:25/:26 | 低端约束 → `max_page_bound` 单点换算（V12-P2-1） |
| WMF_OVERWRITE/WRITEFLAGSONLY/FREE/VERIFY | — | vm.h:56-59 | 写映射策略四分（08 篇逐条对账） |
| MAP_NONE / NO_MEM | — | vm.h:61-62 | C 哨兵值 → Rust `Option`/`Result`（类型化消失） |

### 2.3 区域与缓存旗标（region.h / cache 侧）

VR_WRITABLE/VR_ANON/VR_SHARED（region.h，`VrFlags`）驱动 pr_writable 判定与 fork 共享语义；VMSF_ONCE（cache 侧）标记一次性缓存页——mapcache 检查的是**条目**的 once（mem_cache.c:149）而非请求 flags（24-P0-1）；VMC_NO_INODE=0 是 find/add 的"无 inode"哨兵（其安全性依赖 dev=0 非法）。

## 3 Rust 设计决策

1. **常量权威位置判据**：跨 crate 消费 → minix-types（NR_VM_CALLS/endpoint 编码/SIG*）；crate 内 → 就近模块（MMAP_BASE/VM_MMAPTOP 留 mmap.rs:203-204，glo 全局留 global.rs）。判据 = 是否被第二个 crate 消费。
2. **哨兵类型化**：MAP_NONE/NO_MEM → `Option`/`Result`；NO_INODE → `Option<u64>`；两态 `PageSlot`（V12-P2-4 删除 Reserved 懒占位后 Empty/Mapped 显式区分，P0-1 的回归锚）。
3. **endpoint 代际编码**：`from_generation_slot/slot` 互逆 const fn（minix-types）——跨服务器消息以 endpoint 寻址，`vm_isokendpt` 是"endpoint → 槽位"的唯一校验门。
4. **单源化先例**：NR_VM_CALLS、VM_CACHE 常量族收敛 minix-types；新常量入库前先回答"第二个消费者是谁"。

## 4 实现详解

常量与静态量的定义点：调用号/回复结构（minix-types ipc/vm.rs）、PAF 旗标（phys_mem/types.rs:80-92）、VR 旗标（region/vir_region.rs:27 起）、VMSF_ONCE/VMC_NO_INODE（page_cache.rs:42-48）、glo 三件（global.rs）、FILEMAP_ENABLED（mmap.rs:137）、VM_MMAPBASE 等价（mmap.rs:203-204，ARCH A-6 固定 0x1_0000_0000 窗口）。每个定义点带 C 锚点注释。

## 5 测试点

- `test_callmap_registration_matches_c`：CALLMAP 26 项注册对齐（15 篇守护）
- NR_VM_CALLS 单源一致性（vm_server/dispatcher 两处同引 minix-types）
- PAF 旗标换算对账（max_page_bound 三后端 parity，allocator_tests）
- endpoint 编解码互逆（minix-types）

## 6 过渡

前置：00（导航）。本章是全部机制文档的引用底层——机制文档引用常量/旗标/全局时不重复定义，一律指向本章与各定义点。

## 7 参见

- 00（导航）、15（分发全景）、05/06（物理分配）、24（页缓存）
- todo.md §17/§18（判定与 Fix 账目）
- minix3 源：servers/vm/glo.h、vm.h、minix/include/minix/com.h、ipc.h
