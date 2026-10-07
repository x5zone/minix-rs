# 99-is-global-concepts: 全局概念收口

> **状态**: reviewed（2026-09-15 V1 执行轮成文，取代 pending 骨架；常量
> 归属以 08-stage-is/todo.md §1.2 对账表为底）
> **源码**: `com.h`（is_notify:93、TTY_FKEY_CONTROL:874、FKEY_*:875-877、
> GET_*:316-331、DIAGCTL_CODE_*:412-415、VM_INFO:729、VMIW_*:732-734）、
> `minix3/minix/include/minix/sysinfo.h:SI_PROC_TAB`、`keymap.h`、`glo.h`、`inc.h`
> **Rust 模块**: `minix-types`（wire 常量）+ `os/servers/is/src`（本地常量）
> **前置依赖**: 全部 ｜ **不覆盖（移交）**: 各机制细节（01~10）

## 1. 常量权威位置（§2.4g）

| 常量族 | C 出处 | Rust 权威位置 |
|---|---|---|
| FKEY_MAP/UNMAP/EVENTS、F1~F12/SF1~SF12、两 fkey 载荷 | com.h:874-877、keymap.h、ipc.h:1447-1454 | `minix-types::ipc::tty` |
| GET_*（IS 用 8 项）、SI_*、DIAGCTL_CODE_STACKTRACE、SYS_GETINFO/SYS_DIAGCTL、PM/VFS_GETSYSINFO | com.h:315-345、minix3/minix/include/minix/sysinfo.h:SI_PROC_TAB、callnr.h | `minix-types::ipc::sysinfo` |
| VMIW_STATS/USAGE/REGION、VM_INFO | com.h:729-734 | `minix-types::ipc::vm` |
| NOTIFY_MESSAGE=0x1000、TTY_PROC_SLOT=5 | com.h:90/64 | `is/src/dispatch.rs:20/24` |
| SIGTERM=15 | sys/sys/signal.h:67 | `is/src/sef.rs:16` |
| EDONTREPLY=203 | sys/errno.h:199 | `minix-types` errno.rs:101 |
| LINES=22 / VM_LINES=24 / MORE 标记 | 各 dmp_*.c | 各 dump 模块 |

单一权威核对通过（V1 审查对账）：无跨 crate 重复定义。唯一例外在
IS 之外——`os/servers/vfs/src/misc.rs:const VFS_GETSYSINFO_OFF（L66，工具生成）` 本地重定义 `SI_PROC_TAB`，
已登记 edge E-ISPROD 收敛。

## 2. 错误码

`EDONTREPLY`（203）是回复抑制哨兵，不是错误码（01 §2.5）；`EPERM`
（getsysinfo root 门、TTY UNMAP owner 门）、`EINVAL`（尺寸不匹配）、
`ENOSYS`（未知 getsysinfo 目标、kernel DIAGCTL 未接线现状）。

## 3. 执行模型

单线程事件循环（与 Kernel 的 SMP+BKL 互斥）：`!Send` 合理，无 `Rc`/
`RefCell`，无 alloc（crate 无 `extern crate alloc`）。panic/warn 分界：
transport（收/发）失败 = 事件循环已死 → panic；取数失败 = 本屏作废 →
告警续走（04 §2.6）。

## 4. 排除项（plan §5.4 的补全）

除 plan §5.4 已列各排除项外，V1 Gate A 补两条明确排除（均无运行时语义）：

| 项 | 处理 | 依据 |
|----|------|------|
| `DIAG_BUF_SIZE`（glo.h:6） | 排除：服务于同文件 `diag_buf` 死 extern（同表已排除项），全树无真实消费 | `rg -rn "diag_buf" minix3/minix` 仅声明行 |
| `_SYSTEM`（inc.h:7） | 排除：C 头文件包含协议宏（构建期），无运行时语义 | 同 plan §5.4 对 Makefile 的处理 |

## 5. 跨服务引用

TTY（func_key 通知 + fkey_ctl 对端，02）、VM（do_info，10）、
PM/VFS/RS/DS（do_getsysinfo，06~09）、kernel（do_getinfo/do_diagctl，
04/05）。各对端实现归各 stage；IS 是纯消费侧。

## 6. 边界

- **前置依赖**: 全部
- **不覆盖（移交）**: 各机制细节（01~10）
