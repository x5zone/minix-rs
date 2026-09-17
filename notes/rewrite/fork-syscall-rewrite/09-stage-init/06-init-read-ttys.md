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

自有解析器替代 libc `getttyent`（**[ARCH A-6]**），但对齐的是 libc 的字段语义，而不是"四列空白分词"的直觉。对着 `minix3/lib/libc/gen/getttyent.c` 读一遍会撞见三个容易被简化掉的细节，每一个都在真实文件里现形：引号是模式开关而非字段前缀（`skip()` 里的 `q ^= QUOTED`，引号可以在字段任何位置出现、开关一次就从这个字段里消失，所以 `"/usr/libexec/getty default"` 是一个字段）；状态 token 是精确匹配（`scmp` 做前缀加边界比较，`ondemand` 不是 `on`，`insecure` 不是 `secure`）；`off` 是显式清除（token 按序生效，先 `on` 后 `off` 则后者赢）。Rust 侧对应三件：`strip_inline_comment` 在引号感知下把行内 `#` 之后截掉，`next_field` 按 `skip()` 的状态机扫出一个字段（引号剥除、`\"` 转义、引号内空格保留），token 循环按序处理 `on`/`off`/`secure` 与 `window=` 带值选项。`TtysLine` 携带 `TtyStatus { on, secure }` 位组与原始 `window` 值；libc 认识而 init 不读的其余 token（`local`、`rtscts` 等）解析后忽略——正如 getttyent 存下 init 从不测试的那些状态位。少于四列的行不拒绝：libc 会交出一个 status 为 0 的条目，随后被 07 的 `build_session` 按 off 行拒收，解析与过滤各归其位。golden 测试直接取 `minix3/etc/ttys` 的真实行（引号 getty 与引号空 getty 两种）。已知边界：`fparseln` 的续行反斜杠与全局转义未实现，Minix3 自带的 ttys 文件不使用这些特性。

---

## 4. 实现详解

模块 `ttys.rs`；差异：libc 的 FILE 迭代器改为行切片纯函数（`parse_ttys_line` 一次处理一行），getttyent 的全局静态表状态消失；字段语义收敛在 `strip_inline_comment` 与 `next_field` 两个私有函数里；全局链表操作改为返回计划数据。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_parse_normal_line` | 四列格式与 TTY_ON |
| `test_parse_secure_flag` | TTY_SECURE |
| `test_parse_comment_and_empty_skipped` | fparseln 跳过语义 |
| `test_statusless_line_parses_as_off` | 无状态 token 的条目 status 为 0 |
| `test_parse_off_line_still_parsed` | new_session 过滤在 07（本篇只解析） |
| `test_quoted_getty_keeps_inner_space` | getttyent.c:184 引号字段（minix3/etc/ttys 实行） |
| `test_quoted_empty_getty_is_off_sample` | `""` 空 getty（minix3/etc/ttys 实行） |
| `test_substring_tokens_do_not_match` | scmp 精确 token 匹配 |
| `test_off_after_on_clears_in_order` | `off` 显式清除 TTY_ON |
| `test_window_option_captured` | vcmp window= 带值选项 |
| `test_escaped_quote_inside_quoted_field` | getttyent.c:188 `\"` 转义 |
| `test_trailing_comment_ignored` | 行内 `#` 注释 |
| `test_plan_db_failure_goes_single_user` | init.c:1262-1271 |
| `test_plan_db_failure_chrooted_goes_death` | init.c:1266-1267 |
| `test_plan_counts_sessions` | init.c:1279-1284 |
| `test_plan_counts_exclude_off_and_gettyless_lines` | new_session 三条件过滤，init.c:1147-1149 |

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
