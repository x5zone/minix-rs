# 09-init-multi-user：多用户稳态

> **定位**：状态 `'m'`，`multi_user`（`minix3/sbin/init/init.c:1528-1564`）、`start_getty`（1321-1370）、`start_window_system`（1290-1316）、`setctty`（669-689）、`collect_child`（1460-1497）。
> **Rust**：`os/commands/sbin/init/src/multi_user.rs`。
> **前置依赖**：06/07/08（表、节点、索引）。
> **本篇不覆盖（移交）**：utmp 会话记录（见 13）、chroot 细节（见 12）。

---

## 1. 概念：启动全部，然后睡觉

多用户是 init 的稳态，也是它一生中待得最久的地方。进入时做一件事：遍历会话链表，所有空闲线路全部启动 getty。然后做第二件事：睡觉。睡觉的方式是 `waitpid(-1)` 阻塞等待任意子进程退出，醒来后查 DB 知道是谁，重启它，再睡回去。getty 正常退出（用户注销）就重启，SHUTDOWN 的就移除，重启失败就请求重读终端表。三种常量防止抖动：5 秒内重复崩溃睡 30 秒，窗口系统启动后等 3 秒让它先画完。

`setctty` 是子进程的成人礼：新建会话、等 DTR 电平稳定、打开终端、设为控制终端。失败直接退出，让父进程的回收逻辑接管。这种“子进程只管出生，父进程管超生”的分工贯穿 init。

### 1.1 小结

稳态等于全启动加回收循环。下一章看 SIGHUP 如何触发重读。

---

## 2. C 源码分析

| 函数 | 行号 | 要点 |
|---|---|---|
| `multi_user` | 1528-1564 | 安全级别升 1（机制见 12）；全启动；waitpid 主循环；返回请求状态 |
| `start_getty` | 1321-1370 | fork 失败返回 -1；chroot；防抖动；窗口；解屏蔽；exec 失败 `_exit(8)` |
| `start_window_system` | 1290-1316 | fork 后子进程 setsid、exec 窗口，失败 `_exit(6)` |
| `setctty` | 669-689 | 子进程 setsid、纳秒级 DTR 等待、open、login_tty |
| `collect_child` | 1460-1497 | 未知 pid 忽略；SHUTDOWN 摘链；重启失败请求 clean_ttys |

时间常量：`GETTY_SPACING=5`、`GETTY_SLEEP=30`、`WINDOW_WAIT=3`（`init.c:92-94`）。

---

## 3. Rust 设计决策

防抖动与回收分类提炼为纯函数，进程操作收敛为 trait。与 Redox 的守护进程重启退避思路一致，但退避参数取 Minix 原值。

---

## 4. 实现详解

模块 `multi_user.rs`；差异：`time()`/`gettimeofday` 改为传入秒数；全局链表操作改为动作枚举。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_spacing_triggers_sleep` | init.c:1350-1355 |
| `test_spacing_no_sleep` | 同上 |
| `test_collect_restarts` | init.c:1487-1495 |
| `test_collect_removes_shutdown` | init.c:1476-1485 |
| `test_collect_unknown_ignores` | init.c:1469-1470 |
| `test_collect_spawn_failure_requests_clean` | init.c:1487-1491 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：62 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/multi_user.rs`。

---

## 6. 过渡

稳态的三种出口：SIGHUP 进 clean_ttys（见 10），SIGTSTP/SIGTERM 进关停（见 11），getty 崩溃原地重启（本篇）。

---

## 7. 参见

- `10-init-clean-ttys.md` — 重读。
- `11-init-shutdown.md` — 关停。
- C 源码：`minix3/sbin/init/init.c:669-689,1290-1370,1460-1564`。
