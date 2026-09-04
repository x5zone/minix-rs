# 10-init-clean-ttys：重读终端表

> **定位**：状态 `'T'`，`clean_ttys`（`minix3/sbin/init/init.c:1569-1629`，SIGHUP 触发）。
> **Rust**：`os/commands/sbin/init/src/clean_ttys.rs`。
> **前置依赖**：07（会话与标志）、09（SIGHUP 回收语义）。
> **本篇不覆盖（移交）**：`new_session/setupargv` 机制（见 07）。

---

## 1. 概念：用存在性标记做 diff

重读终端表要回答三个问题：哪些行还在，哪些是新来的，哪些消失了。C 的解法是存在性标记：先把所有会话的 PRESENT 位清零，再逐行读表，读到的置位，读不到的最后统一关停。新行直接建会话，已有行更新序号与开关状态，下线的发 SIGHUP 让 getty 优雅退出。源码自嘲这是 n 平方算法（`init.c:1567` 注释），但重读很少发生，简单正确压倒渐进复杂度。

序号变化只记一条 warning 不做别的，因为 utmp 索引变了会影响已登录会话的显示，但不值得为此重启 getty。开关关闭（TTY_ON 熄灭或 getty 为空）与整行消失同等处理：置 SHUTDOWN、有进程则发 SIGHUP，真正的摘链发生在 09 的回收路径，本篇只标记不释放。

### 1.1 小结

清标记、逐行对、收尾关停。回 multi_user。下一章看关停双态。

---

## 2. C 源码分析

| 段 | 行号 | 行为 |
|---|---|---|
| 清标记 | 1577-1578 | 全部清 PRESENT |
| 逐行匹配 | 1583-1617 | 命中更新序号/开关/SIGHUP；未命中建新会话 |
| 收尾关停 | 1621-1626 | 仍无 PRESENT 的置 SHUTDOWN并发 SIGHUP |
| 返回 | 1628 | 进 multi_user |

序号变化 warning（`init.c:1593-1596`）；解析失败 warning 加关停（`init.c:1606-1612`）。

---

## 3. Rust 设计决策

`diff_line(known, in_file, on)` 纯函数返回四动作之一。与 Redox 的配置热重载思路一致（标记加收敛），但触发源是 SIGHUP 而非 inotify。

---

## 4. 实现详解

模块 `clean_ttys.rs`；差异：链表遍历改为动作枚举，kill 副作用由调用方执行。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_known_on_keeps` | init.c:1605 |
| `test_known_off_shutdowns` | init.c:1598-1603 |
| `test_unknown_creates` | init.c:1616 |
| `test_missing_retires` | init.c:1621-1626 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：66 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/clean_ttys.rs`。

---

## 6. 过渡

重读出口回稳态。下一站 11 关停双态。

---

## 7. 参见

- `09-init-multi-user.md` — 回收摘链。
- C 源码：`minix3/sbin/init/init.c:1569-1629`。
