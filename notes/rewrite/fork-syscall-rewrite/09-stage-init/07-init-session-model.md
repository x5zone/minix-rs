# 07-init-session-model：会话结构与生命周期

> **定位**：`session_t`（`minix3/sbin/init/init.c:156-170`）、`SE_*`（161-162）、`new_session`（1142-1180）、`free_session`（1123-1137）、`setupargv`（1185-1217）、`construct_argv`（1101-1118）。
> **Rust**：`os/commands/sbin/init/src/session.rs`。
> **前置依赖**：06（ttys 行来源）。
> **本篇不覆盖（移交）**：DB 增删查（见 08）、getty 启动与回收（见 09）、防抖动时间比较（见 09，`se_started` 字段定义在本篇）。

---

## 1. 概念：会话是登录线路的内存化身

`/etc/ttys` 的每一行在磁盘上是文字，在内存里是一个会话对象。对象里有设备路径（`/dev/` 加终端名）、getty 命令行（已分词的向量，开箱即 exec）、可选的窗口系统命令行、两个标志位（在表中、当关停），以及链表指针。`se_process` 记录当前跑在这条线路上的子进程号，0 表示空闲；`se_started` 记录上次启动时间，用于防抖动（getty 疯狂崩溃时先睡一会儿，见 09）。

`construct_argv` 的分词器极简：空格与制表符分隔，空命令返回空。这种简单是有意的——getty 命令行本来就不该有引号转义这种复杂语法，解析器越简单越不容易错。Rust 侧保留其语义，但用 `Option<Vec<String>>` 替代 NULL 返回，用 RAII 替代 `free_session` 的五处释放。

### 1.1 小结

会话等于设备加两个命令向量加标志。下一章看这些会话如何被数据库索引。

---

## 2. C 源码分析

### 2.1 结构全字段

| 字段 | 行号 | 含义 |
|---|---|---|
| `se_index` | 157 | ttys 行序号 |
| `se_process` | 158 | 控制进程号 |
| `se_started` | 159 | 启动时间（防抖动） |
| `se_flags` | 160-162 | SE_SHUTDOWN/SE_PRESENT |
| `se_device` | 163 | `/dev/`+终端名 |
| `se_getty/argv` | 164-165 | getty 命令与分词向量 |
| `se_window/argv` | 166-167 | 窗口命令与向量 |
| `se_prev/next` | 168-169 | 双向链表 |

### 2.2 new_session 四关

OFF 行、空名、空 getty 直接 NULL（`init.c:1147-1149`）；malloc 失败 NULL；device 拼接失败释放返回 NULL（`init.c:1159-1163`）；`setupargv` 失败释放返回 NULL（`init.c:1165-1168`）。成功置 PRESENT、挂链表尾（`init.c:1170-1179`）。

### 2.3 setupargv 与分词

getty 字符串为 `"getty名 终端名"` 再分词（`init.c:1193-1196`）；失败记 warning 后清理返回 0（`init.c:1197-1202`）；窗口命令可选，失败同样清理（`init.c:1203-1214`）。分词器按空格制表切分（`init.c:1101-1118`）。释放函数按 device→getty→window 顺序释放（`init.c:1123-1137`）。

---

## 3. Rust 设计决策

节点结构体加位标志加纯分词。链表指针不进入节点（由容器管理，避免 C 式侵入式链表的别名风险）。`build_session` 合并 new 与 setupargv 的成功路径，失败返回 `None` 并附原因枚举。与 Redox 的会话管理对照：Redox 用 scheme 路径索引，我们用 pid 索引（见 08），节点本身都是值类型加 RAII。

---

## 4. 实现详解

模块 `session.rs`；差异：malloc 失败路径由分配器统一处理；`strtok` 改为 `split_whitespace`（语义等价，空格制表换行统一切分，单测锁定）。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_split_simple` | init.c:1101-1118 |
| `test_split_empty_none` | init.c:1111-1114 |
| `test_build_rejects_off` | init.c:1147 |
| `test_build_device_prefix` | init.c:1159 |
| `test_flags_bits` | init.c:161-162 |
| `test_window_optional` | init.c:1203-1215 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：51 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/session.rs`。

---

## 6. 过渡

节点已定，索引待建。下一站 08 会话数据库（ARCH A-1）。

---

## 7. 参见

- `06-init-read-ttys.md` — 行来源。
- `08-init-session-db.md` — DB。
- `09-init-multi-user.md` — 启动与防抖动。
- C 源码：`minix3/sbin/init/init.c:156-170,1101-1217`。
