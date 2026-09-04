# 11-init-shutdown：假死与关机

> **定位**：`catatonia`（`minix3/sbin/init/init.c:1634-1643`）、`death`（1661-1698），`DEATH_WATCH=10`（init.c:96）。
> **Rust**：`os/commands/sbin/init/src/shutdown.rs`。
> **前置依赖**：02（`clang` 语义）、09（回收语义）。
> **本篇不覆盖（移交）**：utmp 关机记录（见 13）。

---

## 1. 概念：假死是关登录，真死是杀全家

`catatonia` 的名字很形象：装死。收到 SIGTSTP 后它把所有会话标为 SHUTDOWN 然后回多用户，新登录不再接受，旧进程自然退出后由回收路径摘链，系统慢慢变空。这是一种温柔的关停，给用户留足保存时间。

`death` 是真死，分三轮升级：先 SIGHUP（请退出），再 SIGTERM（必须退出），最后 SIGKILL（强制杀死）。每轮给 10 秒（DEATH_WATCH），用闹钟的 `clang` 标志计时，收干净（ECHILD）就提前回单用户。三轮杀不完就认栽，记一条“ps axl advised”让人手工查。全场广播用 `kill(-1)`，ESRCH（一个不剩）直接回单用户。

### 1.1 小结

假死标标记，真死三轮杀。出口都是 single_user。下一章进入系统交互三篇。

---

## 2. C 源码分析

| 函数 | 行号 | 要点 |
|---|---|---|
| `catatonia` | 1634-1643 | 全置 SHUTDOWN，回 multi_user |
| `death` | 1661-1698 | 全置 SHUTDOWN；utmp 关机记录；三轮 kill 加 alarm 等待；ECHILD 提前回；杀不完 warning |

`kill(-1)` ESRCH 直接回（`init.c:1681-1682`）；每轮 `clang=0` 加 `alarm(10)`（`init.c:1684-1685`）；等待循环认领子进程（`init.c:1686-1689`）。

---

## 3. Rust 设计决策

轮次与计时提炼为纯决策，kill 与 alarm 收敛为 trait（Live 待缺口）。与 Redox 的关机序列对照：同样三级升级，但信号集取 Minix 原值。

---

## 4. 实现详解

模块 `shutdown.rs`；差异：闹钟标志复用 02 的 `AlarmFlag`；广播副作用由调用方执行。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_catatonia_marks_all` | init.c:1639-1640 |
| `test_death_sequence_order` | init.c:1667 |
| `test_round_all_dead` | init.c:1691-1692 |
| `test_round_timeout_next` | init.c:1686-1689 |
| `test_stuck_warns` | init.c:1695 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：71 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/shutdown.rs`。

---

## 6. 过渡

关停出口回单用户，闭环完成。下一站 12 sysctl 交互。

---

## 7. 参见

- `02-init-state-machine.md` — clang。
- `09-init-multi-user.md` — 回收。
- C 源码：`minix3/sbin/init/init.c:1634-1698`。
