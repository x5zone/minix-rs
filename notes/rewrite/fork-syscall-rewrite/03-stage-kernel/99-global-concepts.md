# 99-global-concepts: 跨文档全局概念

> **分类**: Kernel 全局基建
> **源码**: `minix3/minix/kernel/const.h`, `config.h`, `type.h`, `glo.h`, `kernel.h`
> **说明**: 01~21 中会被反复引用的核心常量、类型定义、RTS 标志位完整表和全局变量清单

---

## 1. Endpoint 进程标识与 Generation 机制

> 来源：tmp-08-endpoint.md

### 1.1 Endpoint 是什么

Endpoint 是 Minix3 中进程的通信标识符，用于 IPC 消息传递时标识发送方和接收方。它通过在槽位号上叠加 **generation 号**解决进程槽位重用导致的"错把 B 当成 A"问题。每次槽位被复用时 generation 递增，使得旧 endpoint 值永远无法匹配新进程。

### 1.2 Endpoint 编码布局

```
|  generation (高 bits)  |  process slot (低 15 bits)  |
```

| 常量 | 值 | 含义 |
|------|-----|------|
| `_ENDPOINT_GENERATION_SHIFT` | 15 | generation 字段位移量 |
| `_ENDPOINT_GENERATION_SIZE` | 32768 | generation 字段大小 |
| `_ENDPOINT_MAX_GENERATION` | ≈65535 | generation 最大值（回绕前） |
| `_ENDPOINT_SLOT_TOP` | 31745 | slot 号空间上界 |

### 1.3 特殊 Endpoint 值

| 常量 | 实际值 | 含义 |
|------|--------|------|
| `ANY` | 31744 | 接收时匹配任何发送方 |
| `NONE` | 31743 | 表示"无进程" |
| `SELF` | 31742 | 表示"自身进程" |

IPC 过滤器特殊值：`ANY_USR`（匹配任何用户进程）、`ANY_SYS`（匹配任何系统进程）、`ANY_TSK`（匹配任何内核任务）

### 1.4 Endpoint 行为规则

1. **Endpoint 唯一性**：同一时刻不存在两个具有相同 endpoint 的活跃进程
2. **Generation 单调递增**：槽位每次被复用，generation 至少递增 1
3. **Generation 0 特殊性**：generation 为 0 时 endpoint 等于 slot 号，用于硬编码的内核任务 endpoint
4. **验证三重条件**：`isokendpt_f()` 同时检查槽位号合法、槽位非空、`p_endpoint` 匹配
5. **特殊值不可验证**：`ANY`/`NONE`/`SELF` 不通过 `isokendpt()` 验证

### 1.5 Endpoint 相关函数

| 功能 | 函数 | 位置 |
|------|------|------|
| Endpoint 验证 | `isokendpt_f()` | proc.c:1830 |
| Endpoint 查找 | `endpoint_lookup()` | proc.c:1818 |
| Generation 递增 | fork 时 generation++ | do_fork.c:59-72 |
| Endpoint 初始化 | `proc_init()` 中 `_ENDPOINT(0, p_nr)` | proc.c:133 |
