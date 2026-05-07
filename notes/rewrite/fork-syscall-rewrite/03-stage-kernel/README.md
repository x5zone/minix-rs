# 03-stage-kernel - 内核阶段 (fork 纵向切片)

> 本目录包含 Minix3 内核中与 fork 系统调用相关的详细分析文档。

---

## Fork 流程概览

```
用户进程 fork()
    │
    ▼
PM (Process Manager)
    │
    ├── 分配子进程 slot
    │
    ▼ 调用 SYS_FORK
┌─────────────────────────┐
│     Kernel (本阶段)      │
│  • 参数验证             │
│  • 进程结构复制          │
│  • 端点生成             │
│  • 特权处理             │
│  • 标志设置             │
└─────────────────────────┘
    │
    ▼
VM (02-stage-vm)
    │
    └── 复制地址空间
```

---

## 目录结构

### 总览与暂存

| 文件 | 内容 |
|------|------|
| [00-kernel-overview.md](00-kernel-overview.md) | Kernel 整体架构概览 (fork 视角) |
| [99-global-concepts.md](99-global-concepts.md) | 系统全局概念暂存区（已迁移到 [concepts/](../../concepts/)） |

### 进程结构体 (proc.h)

| 文件 | 内容 | 源码行数 |
|------|------|---------|
| [01-proc-struct-basic.md](01-proc-struct-basic.md) | 基本字段 (p_reg, p_seg, p_nr, p_priv, p_rts_flags) | ~50行 |
| [02-proc-struct-schedule.md](02-proc-struct-schedule.md) | 调度相关字段 (p_priority, p_cpu_time_left, p_scheduler) | ~30行 |
| [03-proc-struct-accounting.md](03-proc-struct-accounting.md) | 统计字段 (p_accounting, p_user_time, p_sys_time) | ~40行 |
| [04-proc-struct-ipc.md](04-proc-struct-ipc.md) | IPC 相关字段 (p_sendmsg, p_delivermsg, p_endpoint) | ~50行 |
| [05-proc-struct-vm.md](05-proc-struct-vm.md) | VM 请求字段 (p_vmrequest) | ~40行 |
| [06-proc-rts-flags.md](06-proc-rts-flags.md) | RTS 标志位定义和操作宏 | ~60行 |
| [07-proc-misc-flags.md](07-proc-misc-flags.md) | MF 标志位定义 | ~30行 |
| [08-proc-macros.md](08-proc-macros.md) | 进程访问宏 (proc_addr, proc_nr, isemptyp) | ~30行 |

### 特权结构体 (priv.h)

| 文件 | 内容 | 源码行数 |
|------|------|---------|
| [09-priv-struct.md](09-priv-struct.md) | 特权结构体定义 (s_flags, s_k_call_mask) | ~60行 |
| [10-priv-macros.md](10-priv-macros.md) | 特权访问宏 (priv, priv_addr, may_send_to) | ~45行 |

### 系统调用框架 (system.c)

| 文件 | 内容 | 源码行数 |
|------|------|---------|
| [11-system-init.md](11-system-init.md) | 系统调用初始化 (call_vec, map 宏) | ~50行 |
| [12-kernel-call.md](12-kernel-call.md) | kernel_call 函数 | ~50行 |
| [13-kernel-call-dispatch.md](13-kernel-call-dispatch.md) | kernel_call_dispatch 函数 | ~40行 |
| [14-kernel-call-finish.md](14-kernel-call-finish.md) | kernel_call_finish 函数 | ~40行 |

### fork 实现 (do_fork.c)

| 文件 | 内容 | 源码行数 |
|------|------|---------|
| [15-do-fork-validate.md](15-do-fork-validate.md) | 参数验证 (isokendpt, isemptyp, RTS_RECEIVING) | ~30行 |
| [16-do-fork-copy.md](16-do-fork-copy.md) | 进程结构复制 (*rpc = *rpp, FPU 状态) | ~40行 |
| [17-do-fork-endpoint.md](17-do-fork-endpoint.md) | 端点生成 (_ENDPOINT, 代数递增) | ~20行 |
| [18-do-fork-init.md](18-do-fork-init.md) | 子进程初始化 (RTS_NO_QUANTUM, 特权处理) | ~30行 |
| [19-do-fork-priv.md](19-do-fork-priv.md) | 特权与标志处理 (RTS_VMINHIBIT, 页表清零) | ~20行 |

### 基础类型和常量

| 文件 | 内容 | 源码行数 |
|------|------|---------|
| [20-endpoint.md](20-endpoint.md) | 端点机制 (_ENDPOINT, _ENDPOINT_G, ANY, NONE, SELF) | ~70行 |
| [21-type.md](21-type.md) | 基本类型定义 (proc_nr_t, sys_id_t, sys_map_t) | ~30行 |
| [22-const.md](22-const.md) | 常量定义 (isokendpt, 位操作宏) | ~50行 |

---

## 阅读顺序

### 推荐顺序

1. **总览** (00): 理解 Kernel 在 fork 中的整体角色
2. **基础类型** (20-22): 理解端点机制和基本类型
3. **进程结构** (01-08): 理解进程结构体的所有字段
4. **特权结构** (09-10): 理解特权管理
5. **系统调用框架** (11-14): 理解系统调用如何进入内核
6. **fork 实现** (15-19): 理解 fork 系统调用的具体实现

### 快速理解 fork

如果只想快速理解 fork，可以按以下顺序阅读：

1. [00-kernel-overview.md](00-kernel-overview.md) - Kernel 整体架构概览
2. [20-endpoint.md](20-endpoint.md) - 端点机制
3. [06-proc-rts-flags.md](06-proc-rts-flags.md) - 进程状态标志
4. [15-do-fork-validate.md](15-do-fork-validate.md) - 参数验证
5. [16-do-fork-copy.md](16-do-fork-copy.md) - 进程结构复制
6. [17-do-fork-endpoint.md](17-do-fork-endpoint.md) - 端点生成
7. [18-do-fork-init.md](18-do-fork-init.md) - 子进程初始化
8. [19-do-fork-priv.md](19-do-fork-priv.md) - 标志处理

---

## 依赖关系

```
总览 (00)
    │
    ▼
基础类型 (20-22)
    │
    ├──→ 进程结构体 (01-08)
    │        │
    │        └──→ 特权结构体 (09-10)
    │                   │
    └───────────────────┴──→ 系统调用框架 (11-14)
                                │
                                └──→ fork 实现 (15-19)

全局概念 (99) ← 跨阶段共享内容
```

---

## 文档约定

- 每个 TODO 标记表示待填充的内容
- 每个小节约覆盖 20-60 行 C 源码
- 代码引用包含文件路径和行号
- Rust 设计决策讨论类型安全和抽象

---

## 参见

- [../02-stage-vm](../02-stage-vm) - VM 阶段文档
