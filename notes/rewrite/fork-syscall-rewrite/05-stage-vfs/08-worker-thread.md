# 08-worker-thread: worker 线程池

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 — 并发基础：执行模型
> **源码**: `threads.h`、`worker.c` 全文件、`glo.h`（workers/self）
> **Rust 模块**: `os/servers/vfs/src/worker.rs`（WorkerPool/WorkerThread/WorkerState）
> **draft 素材**: `draft/08-worker-thread.md`（素材）

## 核心点

- NR_WTHREADS=9 请求槽；worker 与 fproc 一一关联（w_fp）
- 生命周期：worker_init/worker_start/worker_stop/worker_cleanup/worker_set_proc
- 调度：worker_yield/worker_wait/worker_suspend/worker_resume/worker_signal
- 门控：worker_allow/block_all/worker_may_do_pending/worker_try_activate
- ARCH A-1：mthread 真实线程 → 请求槽状态机（Idle/Busy/WaitingForFs）

## 边界

- 主循环消息分发不覆盖（09）
- 消息内容处理不覆盖（10~31）
