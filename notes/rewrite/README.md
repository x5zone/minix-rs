# Minix-RS 重构文档索引

本目录包含 Minix3 到 Rust 重构的完整设计文档，按主题分类整理。

---

## 快速导航

### 🎯 项目规划（必读）

| 文档 | 说明 | 关键内容 |
|------|------|----------|
| [project-plan.md](project-plan.md) | **项目完整规划** | 目录结构、crate命名、实施路线图、纵向切片策略 |
| [vertical-slice-strategy.md](vertical-slice-strategy.md) | 纵向切片策略详解 | 为什么按模块重写会失败、切片实施路径 |
| [rewrite-strategy.md](rewrite-strategy.md) | 重写策略：语义冻结 | 两阶段方法、语义等价验证、冻结清单 |

### 🏗️ 架构设计

| 文档 | 说明 | 关键内容 |
|------|------|----------|
| [project-structure.md](project-structure.md) | 项目结构设计 | Workspace布局、Makefile vs xtask、构建系统 |
| [minimal-skeleton.md](minimal-skeleton.md) | 最小可运行骨架 | 第一阶段目标、简化策略、initramfs方案 |
| [modern-hardware-and-rust.md](modern-hardware-and-rust.md) | 现代硬件与Rust重构 | 设计哲学、性能理论、多核设计、IPC原子性 |
| [arch_mapping.md](arch_mapping.md) | 架构机制映射 | 硬件描述→机制抽象、Trait设计、静态分发 |

### 🔍 源码分析

| 文档 | 说明 | 关键内容 |
|------|------|----------|
| [invariant.md](invariant.md) | 内核不变量分析 | 进程状态不变量、IPC不变量、高危路径识别 |
| [misc.md](misc.md) | 异步消息表分析 | C语言"阴险锁"设计、Rust类型安全替代方案 |
| [ipc-sendrec.md](ipc-sendrec.md) | SENDREC原子性 | IPC语义原子性vs进程上下文原子性、信号处理问题 |
| [elf-loader.md](elf-loader.md) | ELF加载与用户态执行 | 从rootfs读取、解析ELF、建立地址空间、切换到用户态 |

### 📋 总览

| 文档 | 说明 | 关键内容 |
|------|------|----------|
| [rewrite.md](rewrite.md) | Rust重构总览 | 核心目标、设计原则、文档索引、类型安全状态机 |

---

## 阅读建议

### 第一次接触本项目？

按以下顺序阅读：

1. **[rewrite.md](rewrite.md)** - 了解整体目标和设计原则
2. **[project-plan.md](project-plan.md)** - 了解项目结构和实施计划
3. **[vertical-slice-strategy.md](vertical-slice-strategy.md)** - 理解为什么选择纵向切片
4. **[rewrite-strategy.md](rewrite-strategy.md)** - 理解语义冻结的重要性

### 准备开始编码？

1. **[project-structure.md](project-structure.md)** - 确认项目结构
2. **[invariant.md](invariant.md)** - 理解内核不变量，避免破坏关键假设
3. **[arch_mapping.md](arch_mapping.md)** - 了解如何用Rust trait抽象硬件

### 深入理解Minix3设计？

1. **[modern-hardware-and-rust.md](modern-hardware-and-rust.md)** - 现代硬件视角
2. **[ipc-sendrec.md](ipc-sendrec.md)** - IPC原子性细节
3. **[misc.md](misc.md)** - 异步消息表的巧妙设计
4. **[elf-loader.md](elf-loader.md)** - 用户态执行机制

---

## 文档关系图

```
                    rewrite.md (总览)
                         │
         ┌───────────────┼───────────────┐
         │               │               │
    project-plan   rewrite-strategy  vertical-slice
         │               │               │
         └───────────────┼───────────────┘
                         │
              project-structure
                         │
         ┌───────────────┼───────────────┐
         │               │               │
   modern-hardware    invariant     minimal-skeleton
         │               │               │
    arch_mapping    ipc-sendrec          │
         │               │               │
        misc          elf-loader         │
```

---

## 相关目录

- `../../os/` - Rust 实现的源代码
- `../../minix3/` - 原始 Minix3 C 代码（参考）
