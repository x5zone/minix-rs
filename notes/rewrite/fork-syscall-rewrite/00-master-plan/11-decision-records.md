# 技术决策记录

> 本文档记录 fork 纵向切片实现过程中的关键技术决策及其理由。

---

## D.1 为什么 MProc 放在 PM crate 而不是 minix-types？

**决策**: MProc 结构体定义在 `os/servers/pm/src/mproc/mproc.rs`，而不是 `os/libs/minix-types/src/`。

**理由**:
1. **职责隔离**: MProc 包含大量仅 PM 关心的私有逻辑（信号处理、父子进程树等）
2. **不变量保护**: 状态转换逻辑绑定了 PM 内部复杂逻辑，放在公共库会破坏不变量
3. **微内核原则**: 遵循"知识最小化"原则，其他服务不需要了解 PM 的内部实现

---

## D.2 为什么 Generation 嵌入 Endpoint 而不是单独数组？

**决策**: Generation 作为 Endpoint 结构体的字段，而不是单独的 `generations: [u16; NR_PROCS]` 数组。

**理由**:
1. **唯一 truth**: 避免多处维护同一个值，否则会导致一致性地狱
2. **Minix3 原始设计**: Generation 本来就是 Endpoint 的一部分，不是独立存储
3. **验证简单**: 只需比较 Endpoint 是否相等，无需额外查找

---

## D.3 为什么使用分层设计而不是扁平结构？

**决策**: MProc 使用分层结构（`Identity`, `Resources`, `Signals`, `Guardianship`, `Lifecycle`, `BlockState`, `WaitState`, `TraceState`）而不是 40+ 个字段的扁平结构。

**理由**:
1. **字段归类**: mproc 有 40+ 个字段，扁平结构难以管理
2. **状态机隔离**: 生命周期、阻塞、等待等状态有独立的转换规则
3. **部分复制**: fork 时某些层整体复制，某些层整体清零

---

## D.4 为什么使用 Enum 而不是 Bitflags 表示生命周期？

**决策**: `Lifecycle` 使用 Enum（`Running`, `Zombie`, `Exiting`, `ToldParent`, `TraceZombie`）而不是 Bitflags。

**理由**:
1. **互斥性**: 生命周期状态是互斥的（进程不可能同时是 Running 和 Zombie）
2. **穷尽匹配**: Rust 的 `match` 强制处理所有情况
3. **类型安全**: 不可能出现非法的状态组合

---

## D.5 为什么硬件操作使用 Mock 而不是真实实现？

**决策**: VM 的 MMU/物理内存操作、VFS 的磁盘 I/O、Kernel 的硬件寄存器使用 Mock 实现。

**理由**:
1. **聚焦核心逻辑**: 本阶段目标是验证 fork 状态机和跨服务协调，而非硬件驱动
2. **可测试性**: Mock 允许在宿主机上运行测试，无需真实硬件
3. **渐进实现**: 先验证高层逻辑正确，再逐步替换为真实硬件驱动

**Mock 边界**:
- ✅ 服务间 IPC 调用：真实实现
- ✅ 服务内部逻辑：真实实现
- ❌ 硬件访问：Mock 实现

---

## D.6 为什么 PID 生成器放在 PM 而不是共享库？

**决策**: PID 生成逻辑放在 `os/servers/pm/src/mproc/pid.rs`，作为 PM 的内部模块。

**理由**:
1. **PM 专属**: PID 是 PM 的概念，其他服务不需要生成 PID
2. **冲突检测**: PID 冲突检测需要访问 PM 的进程表
3. **简化接口**: 避免跨 crate 的复杂依赖

---

## D.7 为什么 SUSPEND 机制使用状态标志而不是阻塞调用？

**决策**: PM 调用 VFS fork 后返回 `SUSPEND` 状态码，而不是阻塞等待 VFS 回复。

**理由**:
1. **Minix3 设计**: 遵循 Minix3 的异步 IPC 设计
2. **避免死锁**: 同步阻塞可能导致服务间死锁
3. **调度效率**: 允许内核调度其他进程，提高系统吞吐量
4. **状态可见**: SUSPEND 状态可以被查询和调试
