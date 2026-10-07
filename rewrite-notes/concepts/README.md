# Concepts: 系统核心概念

> **位置**: `notes/rewrite/concepts/`  
> **说明**: 存放跨服务、跨层级的系统核心概念文档

---

## 目录结构

```
concepts/
├── README.md          # 本文件：概念总览
├── endpoint.md        # Endpoint 协议：进程标识与版本控制
├── fail-stop.md       # Fail-Stop 语义与 Panic 安全
└── (更多概念文档)      # 后续添加
```

---

## 什么是"核心概念"？

核心概念是指：

1. **跨服务共享**: 被多个服务（VM、PM、VFS、RS 等）共同使用
2. **跨层级可见**: 内核实现，服务使用，用户态间接依赖
3. **架构基石**: 改动会影响整个系统的设计

与具体组件文档的区别：

| 核心概念文档 | 组件文档 |
|-------------|---------|
| endpoint 协议 | VM 的内存分配实现 |
| IPC 消息格式 | PM 的 fork 实现 |
| 引导模块规范 | VFS 的路径解析 |

---

## 当前概念列表

### Endpoint 协议

**文件**: [endpoint.md](./endpoint.md)

**一句话描述**: 带版本号的进程标识符，解决微内核中服务重启后的身份识别问题。

**关键特性**:
- 版本控制（generation）：区分同一槽位的不同时期
- 不可伪造：只有内核能生成有效 endpoint
- 自动过期：服务重启后旧 endpoint 失效

**使用场景**:
- IPC 消息路由
- 服务崩溃检测
- 能力（Capability）系统

---

### Fail-Stop 语义

**文件**: [fail-stop.md](./fail-stop.md)

**一句话描述**: Minix3 核心服务的错误处理策略——检测到错误时立即停止，不执行任何副作用。

**关键特性**:
- 立即终止：直接 `_exit(1)`，不执行清理
- 无副作用：不调用 Drop 或 cleanup 函数
- 系统级安全：核心服务崩溃导致系统重启，避免状态污染

**使用场景**:
- VM、PM 等核心服务的 panic 处理
- Rust 实现中的 `_exit(1)` vs `panic!` 选择
- `MaybeUninit` 防止未初始化内存的非法 Drop

---

## 文档规范

### 文件命名

- 使用小写 + 连字符: `endpoint.md`, `ipc-primitives.md`
- 避免与具体组件同名: 不用 `vm.md`, `pm.md`

### 文档结构

每个概念文档应包含：

```markdown
# 概念名称

## 1. 协议概述
## 2. 设计目标
## 3. 规范定义
## 4. 实现机制
## 5. 使用模式
## 6. 相关概念
## 7. 源码参考
```

### 引用方式

其他文档引用概念时：

```markdown
参见 [Endpoint 协议](../concepts/endpoint.md)
```

---

## 与源码的关系

概念文档对应源码位置：

| 概念文档 | 主要源码 |
|---------|---------|
| endpoint.md | `minix/include/minix/endpoint.h` |
| | `minix/include/minix/com.h` |
| | `os/libs/minix-types/src/types/endpoint.rs` |
| fail-stop.md | `minix/lib/libsys/panic.c` |
| | `minix/servers/rs/manager.c` |
| | `os/servers/vm/src/vmproc/` |

---

*最后更新: 2026-04-13*
