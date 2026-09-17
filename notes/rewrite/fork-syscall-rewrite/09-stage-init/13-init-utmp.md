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

记录是完整的数据契约：`UtmpxRecord` 带类型、pid、行、短 id、用户、退出字对与时间戳，`RecordType` 覆盖 init 写的四种（INIT/LOGIN/DEAD/RUN_LVL，数值按 `utmpx.h:57-64`）。**on-disk 编码是自觉的临时态**：C 的 `pututxline` 写二进制 ABI，但 minix-rs 还没有任何 utmp 文件的读者，`UtmpxRecord::encode` 先以稳定文本形式落盘，ABI 布局留给出现共享消费者时收敛（ARCH A-2 残余）——字段契约已经钉住，编码只是表达层。写路径经 `InitHost::append_file`（live ENOSYS），失败是普通 `false` 值，与 C 的 `pututxline == NULL` 同一待遇。runlevel 的空会话短路（C 的 `sessions == NULL`，init.c:1438-1441，等不到可写的 /var）这次有了真测试。

---

## 4. 实现详解

模块 `utmp.rs`（P0-6）。三个台账函数：`utmpx_set_runlevel`（空会话短路，RUN_LVL 的 e_exit 带旧级、e_termination 带新级，init.c:1444-1445）、`logwtmpx`（`~` 行的 reboot/shutdown 历史条目，init.c:1008/1674）、`clear_session_logs`（DEAD 进 utmpx、成功才进 wtmpx 的两步，init.c:652-658）。runcom 与 death 的 `record_reboot`/`record_shutdown` 回调即由调用方接到 `logwtmpx` 上。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_runlevel_maps_all_states` | init.c:1411-1427 |
| `test_runlevel_unknown_falls_to_death` | init.c:1426 |
| `test_session_record_login_vs_dead` | init.c:1379 |
| `test_runlevel_write_skipped_without_sessions` | init.c:1438-1441 `sessions == NULL` |
| `test_runlevel_write_record_fields` | init.c:1442-1445 RUN_LVL 字段 |
| `test_logwtmpx_reboot_goes_to_wtmpx` | init.c:1008 |
| `test_clear_session_logs_writes_dead_then_history` | init.c:652-658 两步写 |
| `test_append_failure_is_a_false_not_a_panic` | init.c:1446-1447 失败为值 |
| `test_line_suffix_id` | init.c:1401-1404 |

### 5.1 测试统计（截至 2026-09-18）

- `cargo test -p minix-init`：143 个通过（全 crate 口径），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/utmp.rs`。

---

## 6. 过渡

账本已定。终章 14 看 init 与外部世界的全部契约。

---

## 7. 参见

- `02-init-state-machine.md` — 状态字符。
- `08-init-session-db.md` — add/del 挂钩点。
- C 源码：`minix3/sbin/init/init.c:647-662,1372-1451`。
