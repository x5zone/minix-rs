# 13-init-utmp：会话日志账本

> **定位**：`session_utmpx`（`minix3/sbin/init/init.c:1372-1381`）、`make_utmpx`（1383-1409）、`get_runlevel`（1411-1427）、`utmpx_set_runlevel`（1429-1451）、`clear_session_logs`（647-662)。**[ARCH A-2]** utmp/utmpx 缺口 defer，语义契约先行。
> **Rust**：`os/commands/sbin/init/src/utmp.rs`。
> **前置依赖**：02（状态字符）、07（会话字段）。
> **本篇不覆盖（移交）**：libc utmp 文件格式实现（无 minix-rs 服务前 defer）。

---

## 1. 概念：给 who 命令看的账本

`who` 显示谁在哪个终端登录，`last` 显示开关机历史，这些信息不是内核算出来的，是 init 一笔笔记的账。会话启动记 LOGIN，退出记 DEAD，状态变迁记 RUN_LVL，开机关机记 BOOT/SHUTDOWN。`get_runlevel` 把七个状态函数翻译成七个字符，账本只认字符不认函数指针。`utmpx_set_runlevel` 在会话表为空时跳过，因为那时 `/var` 还没挂载可写，记了也白记——这种“时机不到不记账”的克制与 01 的宽容一脉相承。

SUPPORT_UTMP 与 SUPPORT_UTMPX 双写是 Minix 构建的现状（Makefile 双开），Rust 侧统一为一套记录数据，未来双通道落地时再分发。

### 1.1 小结

账本等于登录加死亡加变迁。终章 14 看对外契约。

---

## 2. C 源码分析

| 函数 | 行号 | 要点 |
|---|---|---|
| `session_utmpx` | 1372-1381 | getty/窗口/空三选一为名，设备去前缀为行，add 决定 LOGIN/DEAD |
| `make_utmpx` | 1383-1409 | 零化、拷名、类型、行、pid、时间、序号；ut_id 取行尾；pututxline 失败记 warning |
| `get_runlevel` | 1411-1427 | 七分支映射，未知回 DEATH |
| `utmpx_set_runlevel` | 1429-1451 | 会话空跳过；RUN_LVL 记录新旧；失败记 warning |
| `clear_session_logs` | 647-662 | 会话退出清 LOGIN/DEAD 双通道 |

---

## 3. Rust 设计决策

状态到字符是纯函数，记录是数据，落盘是 trait。Live 落盘 defer，Fake 内存记录供单测。与 Redox 的日志账本思路一致但记录字段取 Minix 原义。

---

## 4. 实现详解

模块 `utmp.rs`；差异：函数指针比较改为枚举匹配；`pututxline` 改为 sink 记录。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_runlevel_maps_all_states` | init.c:1411-1427 |
| `test_runlevel_unknown_falls_to_death` | init.c:1426 |
| `test_session_record_login_vs_dead` | init.c:1379 |
| `test_runlevel_skipped_when_no_sessions` | init.c:1439-1440 |
| `test_line_suffix_id` | init.c:1401-1404 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：81 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/utmp.rs`。

---

## 6. 过渡

账本已定。终章 14 看 init 与外部世界的全部契约。

---

## 7. 参见

- `02-init-state-machine.md` — 状态字符。
- `08-init-session-db.md` — add/del 挂钩点。
- C 源码：`minix3/sbin/init/init.c:647-662,1372-1451`。
