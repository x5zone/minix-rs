# 架构演进

> **创建**: 2026-10-07（目录迁移当天）建档，同日补全本索引。本区的定位来自目录迁移方案里六方的一致表述：
> **重写稳定之后才启用的探索区**，用来放「架构应该变成什么样」的思考，而不是「现在的行为是什么」。
>
> 三条使用约束：
>
> 1. **不能当实现依据**。当前 13 篇全是从两个来源原样迁来的存量思考（旧 notes 伞目录的再设计分区 11 篇，
>    以及重写树里再设计子目录的 2 篇），彼此之间**存在互斥方案**，也尚未与 `minix3/` 的 C 源核对过。
>    正在推进的重写事实一律以 `rewrite-notes/` 为准。
> 2. **允许互斥方案并存，但一个方向一个子目录**。`architecture/`（架构级改进、跨层污染、分布式一致性）、
>    `ipc/`（Endpoint 与 IPC 协议）、`fork/`（fork 语义）、`vm/`（服务是否内进内核地址空间，暂未放文件）。
>    开新方向时新建子目录，并且先把同主题的已有分析收敛成一处结论，避免同一问题多稿并存。
> 3. **裁决状态要看台账**。尚待拍板的方向集中在
>    `rewrite-notes/coordination/PENDING-DECISIONS-3ARCH-PARITY.md` 与三架构对齐台账；
>    本区文档里的结论句不代表已被采纳。

本目录记录 MINIX3 架构改进的探索性思考，聚焦于现代操作系统设计方向。

---

## 核心目标

探索现代操作系统架构优化方向，包括：
- 现代多核硬件优化
- 微内核性能改进
- 新抽象模型实验
- 智能调度策略

---

## 文档索引

### `architecture/` — 架构级改进

| 文档 | 主题 | 关键内容 |
|------|------|----------|
| [architecture-changes.md](architecture/architecture-changes.md) | 架构变更探索 | Trusted Core Servers 内核态化、LPE Core 策略、AI 驱动调度 |
| [improve_minix.md](architecture/improve_minix.md) | 架构改进思考（初稿） | 现代硬件挑战、架构改进方向 |
| [improve_minix_refactored.md](architecture/improve_minix_refactored.md) | 架构改进思考（结构优化版） | 同上，重新组织后的版本 |
| [semantic-modules.md](architecture/semantic-modules.md) | 语义模块设计 | 模块依赖图、调度核心、IPC 模块抽象 |
| [microkernel-cohesion-design.md](architecture/microkernel-cohesion-design.md) | 微内核内聚性 | 服务边界与内聚判据 |
| [microkernel-closure-design.md](architecture/microkernel-closure-design.md) | 架构闭包 | 依赖闭包与分层约束 |
| [rs-cross-layer-pollution.md](architecture/rs-cross-layer-pollution.md) | 跨层污染 | 来自重写树再设计子目录的一篇 |
| [tocutou-and-distributed-consistency.md](architecture/tocutou-and-distributed-consistency.md) | 分布式一致性探索 | 同上 |

### `ipc/` — Endpoint 与进程间通信

| 文档 | 主题 | 关键内容 |
|------|------|----------|
| [endpoint_redesign.md](ipc/endpoint_redesign.md) | Endpoint 协议重设计 | endpoint 语义与生命周期 |
| [ipc-improve.md](ipc/ipc-improve.md) | IPC 设计改进 | BKL 反思、L4 对比、Cache 污染分析 |

### `fork/` — fork 语义

| 文档 | 主题 | 关键内容 |
|------|------|----------|
| [fork-redesign.md](fork/fork-redesign.md) | fork 语义重设计 | 进程复制的边界与资源继承 |

### `vm/` — 预留

「VM 是否内进内核地址空间」与「保持用户态服务 + 继续优化 IPC 路径」是两条互斥路线，尚未裁决，
落点与前置说明见 [vm/README.md](vm/README.md)。

### 两份逐字节相同的备份

`improve_minix.md.backup` 与 `improve_minix_refactored.md.backup` 和正本 `cmp` 逐字节相同（已实测）。
是否合并是早先待办里的旧条目，目录迁移只搬不删，留给独立任务处理。

> **拆分史（保留原文是为了可追溯）**：`improve_minix.md` 与 `improve_minix_refactored.md` 早先被拆成三份，
> 分别落到本区的 `architecture/architecture-changes.md`、重写区的
> `rewrite-notes/misc/modern-hardware-and-rust.md`、学习区的 `study-notes/learning-path.md`；
> 两个原始文件作为参考保留。

---

## 研究方向

### 1. 现代硬件适配

| 方向 | 挑战 | 探索思路 |
|------|------|----------|
| **多核调度** | Cache 一致性开销 | Per-CPU 运行队列、NUMA 感知调度 |
| **大内存支持** | 64 位地址空间 | 页表结构优化、大页支持 |
| **高速设备** | NVMe、RDMA | 用户态驱动、零拷贝 IPC |

### 2. 微内核性能优化

| 问题 | 原因 | 改进方向 |
|------|------|----------|
| **IPC 开销** | 用户态-内核态切换 | 快速路径优化、批量 IPC |
| **Cache 污染** | 频繁上下文切换 | 调度器感知 Cache、Core Pinning |
| **信号处理** | SENDREC 非原子性 | 信号与 IPC 语义统一 |

### 3. 架构抽象

```
┌─────────────────────────────────────────────────────────────┐
│                    语义模块依赖图                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                      ┌─────────┐                            │
│                      │  proc   │  进程控制块（核心数据）     │
│                      └────┬────┘                            │
│                           │                                 │
│           ┌───────────────┼───────────────┐                 │
│           │               │               │                 │
│           ▼               ▼               ▼                 │
│    ┌───────────┐   ┌───────────┐   ┌───────────┐           │
│    │ scheduler │   │    IPC    │   │ interrupt │           │
│    └───────────┘   └───────────┘   └───────────┘           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 4. 智能调度

- **Cache 感知调度**：调度器作为 Cache 所有权管理者
- **负载预测**：基于历史数据的调度决策
- **能耗优化**：动态电压频率调整（DVFS）与调度协同

---

## 设计哲学

### 从 Fast Kernel 到 Invisible Kernel

| 传统目标 | 现代目标 |
|----------|----------|
| 最小化内核代码路径 | 最小化内核对应用的影响 |
| 快速系统调用 | 零系统调用（用户态直接访问） |
| 紧凑数据结构 | Cache 友好数据结构 |

### 去除历史包袱

- 段寄存器（x86 历史遗留）
- 16 位兼容代码
- 实模式启动流程
- 静态资源分配（改为动态）

---

## 参考资料

- [seL4 Microkernel](https://sel4.systems/)
- [Fuchsia Zircon](https://fuchsia.dev/fuchsia-src/reference/kernel)
- [Redox OS](https://www.redox-os.org/)
- [L4 Microkernel Family](https://l4microkernel.org/)
