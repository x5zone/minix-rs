# 99-global-concepts: VFS 全局概念

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（阶段收尾）
> **源码**: `const.h`、`glo.h`、`type.h`、`fs.h`、`proto.h`、`utility.c:142-186`（sys_datacopy_wrapper）、minix 外部头
> **Rust 模块**: `minix-types`、`os/servers/vfs/src/call_table.rs` 常量
> **draft 素材**: `draft/09-globals-const.md` + `draft/99-global-concepts.md`（素材）

## 核心点

- 常量表：NR_FILPS/NR_VNODES/NR_MNTS/NR_WTHREADS/NR_LOCKS/NR_SOCKDEVS、FP_BLOCKED_ON_*、SYMLOOP、CTTY_ENDPT
- 全局状态：fp/susp_count/reviving/sending/verbose/m_in/self/workers/err_code/bsf_lock
- 引用计数模型：filp_count / v_ref_count / v_fs_count 双层不变量（draft/99 素材）
- endpoint/transid 术语、who_p/who_e/call_nr 宏
- sys_datacopy_wrapper：跨文档数据拷贝工具
- 64 位类型映射（A-8）、LOCK_DEBUG cfg（A-9）

## 边界

- 一切机制细节不覆盖（01~31）

## 有意省略表（intentional omissions）

单线程事件循环（ARCH A-1）消灭 mthread 后，一批 C 函数的存在前提消失。本表集中登记"有意不移植"的 C 符号——C 锚点、删除理由与 ARCH 依据，防止后续轮次把它们误报为缺口（首轮 C-10/P3-2 的落地；第二轮 Fix #22/Fix #23 各追加一笔）。新增省略项必须走本表：函数名 + C 锚点 + 为何在 minix-rs 中无存在前提。

### 进程锁族（mthread 互斥的替代物消失）

| C 符号 | C 锚点 | 删除理由 |
|--------|--------|---------|
| `lock_proc` / `unlock_proc` | main.c:528-547 | C 用 mutex_trylock + worker_suspend 保护"可睡眠的进程锁"；单线程事件循环下同一时刻只有一个请求在被处理，无并发对手（ARCH A-1 推论，首轮 C-10 判定） |
| `thread_cleanup` | main.c:557 一带 | 同上——mthread 栈清理随线程消失 |

### 死锁断言族（LOCK_DEBUG 调试设施）

| C 符号 | C 锚点 | 删除理由 |
|--------|--------|---------|
| `check_filp_locks(_by_me)` | filedes.c:26-71 | 多线程死锁调试断言；借用模型（`locked_by`/`soft_locked`）使非法状态不可表达，断言无对象 |
| `check_vnode_locks(_by_me)` | vnode.c:43-83 | 同上 |
| `check_vmnt_locks(_by_me)` | vmnt.c:24-62 | 同上 |
| `unlock_filps` | filedes.c:383 一带 | 批量解锁随软锁模型收编进 `dec_count`/槽释放路径 |

### select 清理族（部分建模，部分省略）

| C 符号 | C 锚点 | 处置 |
|--------|--------|---------|
| `select_forget` | select.c:833 | 有意省略——进程退出时 select 台账的批量遗忘由 `fproc` 槽释放级联承担（free_proc 的 close_fd×256 自然清账） |
| `wipe_select` | select.c:78/127 | 已建模——`reply1_step`/`reply2` 的 ops 清零规则即其语义 |
| `select_timeout_check` | select.c:79/335 | 已建模——`plan_timeout` 三态计划承接到期语义 |
| `select_dump` | select.c（调试转储） | 有意省略——纯调试转储，无行为契约 |

### 第二轮追加判定

| C 符号 | C 锚点 | 处置与理由 |
|--------|--------|-----------|
| `fetch_vmnt_paths` | vmnt.c:246-288 | **C 死代码**：定义 + `proto.h:371` 悬空声明、全树零调用；行为真相是 `fill_statvfs` 直拷 `m_mount_path`（stadir.c:284）。移植死函数即 translate 死代码（Fix #22 判定反转） |
| `panic_hook` | misc.c:989-993 | ARCH A-1 消灭 mthread 后"打印 mthread 栈"无对象（Fix #23 判定） |
| `worker_cleanup`/`worker_init` 的 LU 调用点 | main.c:314/332/352 | 槽位是数据不是线程——清理/重建工人按构造为空操作；谓词 `lu_rollback_needs_workers`/`init_lu_needs_workers` 记录 C 分支（Fix #26） |

### 未省略、仅待接线的边界提醒

以下**不是**省略项，只是执行半挂接线矩阵（P1-2/edge E1）：`dmap_endpt_up`/`smap_endpt_up` 的恢复驱动（`recover_step` 已备）、`free_proc` 重启轮（`REBOOT_SEQUENCE` 已备）、`req_readsuper` 确认往返（`PfsMountPlan` 已备）。判别标准：决策函数已存在、只缺消息回路的，归 P1-2；连决策前提都消失的，才入本表。
