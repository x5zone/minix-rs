# 23-ipc-filter Outline v1

> 本文件是 23-ipc-filter.md 的结构契约（Gate H.6 依据），基于 C 源码独立推导。
> 非持久化 ground truth；每轮 review 重新评估，保留历史版本不覆盖。

---

## Ch1: 概念（IPC 过滤的三层模型）

### 教学目标
- 从 CPU 视角回答"内核如何决定一条 IPC 消息能否送达"
- 引入三层过滤模型，每层回答不同的问题
- 为后续 Ch3 设计决策提供概念基础

### 知识点覆盖
1. **过滤的三层模型**（统一抽象先行）
   - L1 粗粒度：`s_ipc_to` 位图（系统调用过滤 + IPC 目标过滤）
   - L2 细粒度：`s_ipcf` 过滤链（按 m_source / m_type 过滤）
   - L3 状态：IPC_STATUS（在 RECEIVE 时报告过滤结果）
2. **过滤时机**：send / notify / asyncsend / receive（receive 时反向过滤）
3. **对称与不对称**：`may_send_to` vs `may_asynsend_to`（后者允许 self-send）
4. **CHECK_IPC 不存在的设计理由**：与 `CHECK_IO_PORT/CHECK_IRQ/CHECK_MEM` 对比
5. **本章不讲什么**：syscall 层的 do_privctl 入口见 17-syscall-process.md；权限字段定义见 22-privilege.md；IPC 原语语义见 12-ipc-core.md

### 核心概念清单（按引入顺序）
| 顺序 | 概念 | 依赖 | 教学要点 |
|------|------|------|---------|
| 1 | IPC 过滤 | 无 | 为什么需要过滤（最小特权原则） |
| 2 | 三层模型 | IPC 过滤 | 每层解决不同粒度问题 |
| 3 | s_ipc_to 位图 | 三层模型 | 粗粒度，按 sys_id 索引 |
| 4 | s_k_call_mask | 三层模型 | 粗粒度，按 call_nr 索引 |
| 5 | s_ipcf 过滤链 | 三层模型 | 细粒度，按 m_source/m_type 匹配 |
| 6 | 黑/白名单 | s_ipcf | 反向逻辑（blacklist 默认 allow） |
| 7 | IPC_STATUS | 三层模型 | 状态报告机制 |

---

## Ch2: C 源码分析

### 知识点覆盖
1. **文件清单与职责**：
   - `const.h:19-26` — `get_sys_bit/set_sys_bit/unset_sys_bit` 宏
   - `priv.h:35,38,46,84,86,87` — `s_ipc_to`, `s_k_call_mask`, `s_ipcf`, `nr_to_id`, `may_send_to`, `may_asynsend_to`
   - `ipc.h:14-22` — `WILLRECEIVE`, `CANRECEIVE` 宏（用 `s_ipcf`）
   - `ipc.h:40-48` — `IPC_STATUS_ADD/ADD_CALL/ADD_FLAGS` 宏
   - `include/minix/ipc_filter.h` — `IPCF_MATCH_M_SOURCE/M_TYPE`, `struct ipc_filter_el_s`, `ANY_USR/SYS/TSK`
   - `kernel/ipc_filter.h` — `IPCF_NONE/BLACKLIST/WHITELIST`, `IPCF_POOL_*` 宏, `struct ipc_filter_s`
   - `system.c:111` — `GET_BIT(priv(caller)->s_k_call_mask, call_nr)` 检查
   - `system.c:803-874` — `allow_ipc_filtered_msg()` 函数
2. **核心函数行为契约**（5 个，列在 Ch2 末尾，Gate B 依据）
3. **过滤链遍历算法**：blacklist/whitelist 的反向逻辑
4. **IPCF_EL_MATCH 算法**：m_source + m_type 双重匹配（含 ANY_USR/SYS/TSK 特殊端点）

---

## Ch3: Rust 设计决策

### 决策表（≥ 7 个 hypothesis-driven 决策）
| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | 位图操作 | 宏 vs 内联函数 | 内联函数 | Rust 类型安全 + 编译期内联 |
| D2 | 过滤函数位置 | 内联 vs 独立函数 | 独立函数 | 可测试性 + 单一职责 |
| D3 | s_k_call_mask 类型 | `[u32; 2]` vs `u64` | `u64` | 58 syscall 足够；对齐 22-privilege IpcMask |
| D4 | 过滤失败返回 | EPERM vs panic | EPERM | 对齐 C |
| D5 | 过滤池空闲槽 | type==IPCF_NONE vs Option | Option | Rust "illegal states unrepresentable" |
| D6 | 过滤链 next | 裸指针 vs Option<usize> | Option<usize> | 避免 unsafe + 索引安全 |
| D7 | IPCF_MATCH_M_SOURCE/M_TYPE | bitflags | bitflags | 类型安全 + 可组合 |
| D8 | filter_type enum | int vs enum | enum | 穷尽匹配 + 编译器检查 |
| D9 | IPC_STATUS | 不实现 vs 实现 | DEFERRED | 当前无 RECEIVE 路径使用，标 DEFERRED |

---

## Ch4: 实现要点

### 知识点覆盖
1. **IPC 过滤函数**：`ipc_filter_check`, `kcall_filter_check`
2. **位图原语**：`set_sys_bit`, `unset_sys_bit`, `get_sys_bit`
3. **IPC 过滤池**：`IpcFilterPool`, `IpcFilterSlot`, `IpcFilterElement`
4. **与 syscall.rs 集成**：`kernel_call_dispatch` 中的 kcall_filter_check
5. **未实现的 C 函数**：`allow_ipc_filtered_msg()`, `allow_ipc_filtered_memreq()`, `IPCF_EL_MATCH` 宏链

### 已知缺口
- `allow_ipc_filtered_msg()` 未实现（接收路径细粒度过滤）
- `may_asynsend_to()` 不对称语义未在 Rust 显式表达（当前用 `may_send_to` 替代）
- IPC_STATUS 机制未实现

---

## Ch5: 测试

### 测试覆盖
- L1 对偶：ipc_filter_check / kcall_filter_check 与 C 行为一致
- L2 契约：IpcFilterPool allocate/free 契约
- 边界：call_nr >= 64 / sys_id >= 64 / 空池 / 重复 free

---

## Ch6: 已知缺口与限制

| 缺口 | C 位置 | Rust 状态 | 优先级 |
|------|--------|----------|--------|
| allow_ipc_filtered_msg | system.c:803-874 | 未实现 | P1 |
| may_asynsend_to 不对称 | priv.h:87 | 未实现 | P1 |
| IPC_STATUS 机制 | ipc.h:40-48 | 未实现 | P2 |
| IPCF_EL_MATCH 任意端点 | ipc_filter.h:24-39 | 未实现 | P2 |

---

## Ch7: 参见

- [22-privilege.md](../22-privilege.md) — s_ipc_to / s_k_call_mask / s_ipcf 字段定义
- [12-ipc-core.md](../12-ipc-core.md) — send/receive/notify 调用过滤
- [17-syscall-process.md](../17-syscall-process.md) — do_privctl 运行时权限控制入口
- [13-syscall-dispatch.md](../13-syscall-dispatch.md) — kernel_call_dispatch 过滤接入点

---

## 覆盖矩阵

| Ch | 知识点数 | 教学要点 | C 引用 | Rust 引用 |
|----|---------|---------|--------|----------|
| 1 | 7 | ✅ | 0 | 0 |
| 2 | 4 | ✅ | 8 file:line | 0 |
| 3 | 9 决策 | ✅ | 0 | 0 |
| 4 | 5 | ✅ | 5 file:line | 5 file:line |
| 5 | 3 | ✅ | 0 | 9 test fn |
| 6 | 4 | ✅ | 4 file:line | 4 status |
| 7 | 4 | ✅ | 0 | 0 |
