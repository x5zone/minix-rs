# 04-schedproc-table: 进程表与 endpoint 验证

> **状态**: pending（最小骨架，待改写）
> **定位**: 数据结构（进程表 + 验证 + 安全模型）
> **源码**: `minix3/minix/servers/sched/schedproc.h`、`utility.c:29-72`
> **Rust 模块**: `table.rs`、`valid.rs`
> **draft 素材**: `draft/01-sched-struct.md`（素材，验证部分拆分）

## 核心点

- `schedproc[NR_PROCS]` 静态表、`_ENDPOINT_P(endpoint)` slot 提取
- `sched_isokendpt()`（utility.c:29）：EBADEPT/EINVAL/EDEADEPT 三错误码语义
- `sched_isemtyendpt()`（utility.c:46）：空闲 slot 验证
- `accept_message()`（utility.c:61）：PM/RS 白名单安全模型
- 错误码表（`EBADEPT`/`EINVAL`/`EDEADEPT`/`EPERM`/`ENOSYS`/`EBADCPU`）

## 边界

- **前置依赖**: 03
- **不覆盖（移交）**: 字段细节（03）、各 handler 中的使用（06~08）
