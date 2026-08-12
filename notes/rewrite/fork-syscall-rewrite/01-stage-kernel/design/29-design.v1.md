# 29-kernel-debug: 设计文档（Design v1）

> **文档**: `29-kernel-debug.md`
> **状态**: v1 快照（2026-08-12）
> **用途**: Gate H 依据 + WONTFIX 文档化

---

## Ch1: 设计决策

### D1: 调度队列 sanity check 不实现（WONTFIX）
- **C**: `runqueues_ok_cpu()` / `runqueues_ok_all()` / `runqueues_ok()` 验证调度队列一致性
- **Rust 64-bit**: 不实现
- **理由**: 调试专用功能；Rust 类型系统（所有权 + Option）已在编译期防止大部分队列错误；运行时断言用 `debug_assert!` 替代

### D2: 进程信息打印不实现（WONTFIX）
- **C**: `print_proc()` / `print_proc_depends()` / `print_proc_recursive()` 打印进程详情
- **Rust 64-bit**: 不实现
- **理由**: 调试专用；Rust `Debug` / `Display` trait 提供更规范的格式化输出；依赖 `printf` 字符串格式不适合 Rust

### D3: IPC 消息跟踪不实现（WONTFIX）
- **C**: `hook_ipc_msgkcall()` / `hook_ipc_msgkresult()` / `hook_ipc_msgrecv()` / `hook_ipc_msgsend()` / `hook_ipc_clear()` — 条件编译 `DEBUG_IPC_HOOK`
- **Rust 64-bit**: 不实现
- **理由**: 调试专用；`log` crate + `tracing` crate 提供更结构化的日志方案；条件编译 hook 增加代码复杂度

### D4: IPC 统计不实现（WONTFIX）
- **C**: `printstats()` / `sortstats()` / `statmsg()` — 条件编译 `DEBUG_IPCSTATS`
- **Rust 64-bit**: 不实现
- **理由**: 调试专用；性能分析应用 `perf` / `flamegraph` 等外部工具

---

## Ch2: Minix3 对齐矩阵

| Minix3 概念 | design 对应 | code 对应 | 一致性 |
|------------|------------|----------|--------|
| `runqueues_ok_cpu` | D1 | （不实现） | ✅ WONTFIX — 调试专用 |
| `runqueues_ok_all` (SMP) | D1 | （不实现） | ✅ WONTFIX |
| `runqueues_ok` | D1 | （不实现） | ✅ WONTFIX |
| `print_proc` | D2 | （不实现） | ✅ WONTFIX — Rust Debug trait 替代 |
| `print_proc_depends` | D2 | （不实现） | ✅ WONTFIX |
| `print_proc_recursive` | D2 | （不实现） | ✅ WONTFIX |
| `printproc` / `printparam` | D2 | （不实现） | ✅ WONTFIX |
| `namematch` | D2 | （不实现） | ✅ WONTFIX |
| `printstats` / `sortstats` / `statmsg` | D4 | （不实现） | ✅ WONTFIX |
| `hook_ipc_msg*` (5 个) | D3 | （不实现） | ✅ WONTFIX |

---

## Ch3: 设计一致性检查

- D1-D4 全部 WONTFIX，理由一致（调试专用 + Rust 有更好替代）
- 跨文档引用: 16-smp.md（调度队列）/ 12-ipc-core.md（IPC hooks）
