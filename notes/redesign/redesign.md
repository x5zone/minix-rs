# 架构演进

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

| 文档 | 主题 | 关键内容 |
|------|------|----------|
| [architecture-changes.md](architecture-changes.md) | 架构变更探索 | Trusted Core Servers 内核态化、LPE Core 策略、AI 驱动调度 |
| [semantic-modules.md](semantic-modules.md) | 语义模块设计 | 模块依赖图、调度核心、IPC 模块抽象 |
| [ipc-improve.md](ipc-improve.md) | IPC 设计改进 | BKL 反思、L4 对比、Cache 污染分析 |
| [improve_minix.md](improve_minix.md) | 架构改进思考（原始） | 现代硬件挑战、架构改进方向 |
| [improve_minix_refactored.md](improve_minix_refactored.md) | 架构改进思考（重构版） | 同上，结构优化版本 |

> **注意**：`improve_minix.md` 和 `improve_minix_refactored.md` 已被拆分到 `architecture-changes.md`（redesign）、`modern-hardware-and-rust.md`（rewrite）和 `learning-path.md`（study）。原始文件保留作为参考。

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
