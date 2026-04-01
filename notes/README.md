# MINIX3 学习笔记

本目录记录 MINIX3 源码阅读、Rust 重写和重新设计的全过程。

---

## 目录结构

```
notes/
├── study/          # 第一步：源码阅读笔记
│   ├── ipc/        # IPC 模块
│   ├── process/    # 进程管理
│   ├── syscall/    # 系统调用
│   ├── interrupt/  # 中断与时钟
│   ├── boot/       # 启动流程
│   ├── arch/       # 架构相关
│   ├── vm/         # 虚拟内存
│   ├── pm/         # 进程管理器
│   ├── vfs/        # 文件系统
│   └── services/   # 系统服务
│
├── rewrite/        # 第二步：Rust 重写思路
│   └── (重写时的设计思路和想法)
│
├── redesign/       # 第三步：重新设计实验
│   └── (架构改进实验和新想法)
│
├── progress.md     # 阅读进度跟踪
├── roadmap.md      # 学习路线图
└── minix_structure.md  # MINIX3 代码结构
```

---

## 项目阶段

本项目采用渐进式开发策略，分为三个递进阶段：

### 阶段一：Study（源码研读）

**目标**：深入理解 MINIX3 微内核架构与实现机制

**方法论**：
- **机制优先，硬件分离**：抽象操作系统核心机制，而非描述特定硬件细节
- **架构理解优先**：聚焦操作系统架构设计，而非系统启动工程

**研读范围**：
1. **核心机制**（Core Mechanisms）
   - IPC（进程间通信）
   - Process（进程管理）
   - Syscall（系统调用）
   - Interrupt（中断与时钟）

2. **用户态服务**（User-Space Services）
   - PM（进程管理器）
   - VFS（虚拟文件系统）
   - VM（虚拟内存管理器）
   - RS（重生服务器）

3. **硬件抽象层**（Hardware Abstraction Layer）
   - Boot（启动流程）
   - Arch（架构相关）

### 阶段二：Rewrite（Rust 重构）

**目标**：基于 Rust 语言重构 MINIX3，保留原有设计语义

**设计原则**：
- **现代硬件适配**：面向 64 位架构，移除历史遗留机制（如段寄存器）
- **Rust 语言特性**：利用类型系统、所有权模型表达设计意图
- **代码即文档**：通过类型安全消除 C 语言中的隐式假设

**技术要点**：
- 类型安全的状态机建模
- 所有权与生命周期管理
- 显式错误处理（Result/Option）
- 编译期数据竞争检测

### 阶段三：Redesign（架构演进）

**目标**：探索现代操作系统架构优化方向

**研究方向**：
- 现代多核硬件优化
- 微内核性能改进
- 新抽象模型实验
- 智能调度策略

---

## 文件说明

| 文件 | 说明 | 所属阶段 |
|------|------|----------|
| `progress.md` | 源码阅读进度跟踪 | Study |
| `roadmap.md` | 学习路线图 | Study |
| `minix_structure.md` | MINIX3 代码结构 | Study |
| `daily.md` | 日常学习记录 | Study |
| `invariant.md` | 内核不变量分析 | Rewrite |
| `arch_mapping.md` | 架构机制映射 | Rewrite |
| `misc.md` | 异步消息表分析 | Rewrite |
| `improve_minix*.md` | 架构改进思考 | Redesign |

---

## 学习资源

- [MINIX3 官方网站](https://minix3.org/)
- [MINIX3 源码](https://github.com/Stichting-MINIX-Research-Foundation/minix)
- [Operating Systems: Design and Implementation](https://minix3.org/doc/)
