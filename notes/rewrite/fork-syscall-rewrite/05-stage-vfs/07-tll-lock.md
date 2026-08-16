# 07-tll-lock: 三级锁（Three-Level Lock）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 — 并发基础：锁原语
> **源码**: `tll.h`、`tll.c` 全文件
> **Rust 模块**: （未实现）锁模块
> **draft 素材**: `draft/07-tll-lock.md`（素材）

## 核心点

- TLL_READ / TLL_READSER / TLL_WRITE 三级锁语义（读写并发、串行读、独占）
- tll_init/tll_lock/tll_unlock/tll_downgrade/tll_upgrade
- 锁等待队列与 pending 语义：tll_haspendinglock/tll_append
- tll_islocked/tll_locked_by_me（调试/校验）
- 被 vnode/vmnt/filp 锁族复用（VNODE_*/VMNT_* 映射）

## 边界

- 具体锁使用方不覆盖（04/05/06/14）
- LOCK_DEBUG 编译宏归 99（A-9）
