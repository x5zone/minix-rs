# 06-init-read-ttys：读终端表与会话重建

> **定位**：状态 `'t'`，`read_ttys`（`minix3/sbin/init/init.c:1222-1285`）、`do_setttyent`（1792-1806）。
> **Rust**：`os/commands/sbin/init/src/ttys.rs`。
> **前置依赖**：02（状态字符）、05（runcom 出口）。
> **本篇不覆盖（移交）**：`new_session/free_session` 结构细节（见 07）、会话 DB 实现（见 08）、开机 utmp 记录（见 13）、chroot 路径（见 12）。

---

## 1. 概念：终端表是登录系统的总开关

`/etc/ttys` 是一个四列表格，每行描述一个终端：设备名、跑什么 getty、什么类型、状态标志。`TTY_ON` 表示“允许登录”，`TTY_SECURE` 表示“允许 root 登录”（`minix3/include/ttyent.h:57-58`）。init 的 `read_ttys` 不做别的，就是把这张表翻译成内存里的会话链表：开的行建会话，关的行跳过。翻译前先把旧链表整个销毁——“先清空再重建”在 01 见过，这里是第二次出现。这种粗暴但正确的策略避免了新旧 diff 的复杂性，代价是每次重读都要重建全部会话，而重读恰恰很少发生（只在启动与 SIGHUP 时）。

`do_setttyent` 的 chroot 感知值得一提：如果之前 chroot 过，ttys 路径要拼上新根（`rootdir + /etc/ttys`，`init.c:1800`），否则读宿主的表就错了。路径细节的机制见 12，本篇只确认调用点。

### 1.1 小结

读表即重建链表。下一章看链表节点（session）长什么样。

---

## 2. C 源码分析

| 步骤 | 行号 | 行为 |
|---|---|---|
| 开机记录 | 1229-1247 | 首次进 read_ttys 写 BOOT 记录（SUPPORT_UTMPX，机制见 13） |
| 清旧链表 | 1252-1260 | 有进程的先清日志，逐个 free，会话头置空 |
| 开 DB | 1262-1271 | 失败：chroot 过进 death，否则回 single_user |
| 打开表 | 1273 | `do_setttyent()`（chroot 感知） |
| 逐行建会话 | 1279-1282 | `getttyent` 循环，`new_session` 非空才推进链表尾 |
| 收尾 | 1282-1284 | `endttyent`，返回 multi_user |

`new_session` 内部过滤 `TTY_ON` 熄灭行与空名行（`init.c:1147`，机制见 07），本篇只记“过滤存在”。

---

## 3. Rust 设计决策

自有解析器替代 libc `getttyent`（**[ARCH A-6]**）：`parse_ttys_line` 按空白分词，`#` 开头与空行返回 `None`，少于四列返回 `None`，状态列取第四列起全部词段合并后大小写不敏感匹配：含 `on` 即开（含 `secure` 的行自然含 `on` 语义由调用方按位理解），含 `secure` 即允许 root。`plan()` 把 DB 成败与行数映射为下一状态，与 Redox 的服务表解析思路一致但格式独立。`TtysSource` trait 隔离文件读取，Fake 供单测。

---

## 4. 实现详解

模块 `ttys.rs`；差异：libc 迭代器改为行切片纯函数；全局链表操作改为返回计划数据。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_parse_normal_line` | ttys 四列格式 |
| `test_parse_secure_flag` | TTY_SECURE |
| `test_parse_comment_and_empty_skipped` | getttyent 跳过语义 |
| `test_parse_off_line_still_parsed` | new_session 过滤在 07（本篇只解析） |
| `test_plan_db_failure_goes_single_user` | init.c:1262-1271 |
| `test_plan_db_failure_chrooted_goes_death` | init.c:1266-1267 |
| `test_plan_counts_sessions` | init.c:1279-1284 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：45 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/ttys.rs`。

---

## 6. 过渡

表已读完，链表待建。下一站 07 会话结构，08 会话数据库。

---

## 7. 参见

- `07-init-session-model.md` — new/free/setupargv。
- `08-init-session-db.md` — start/add/del/find。
- `10-init-clean-ttys.md` — 重读路径的兄弟篇。
- C 源码：`minix3/sbin/init/init.c:1222-1285,1792-1806`、`minix3/include/ttyent.h:40,57-58`。
