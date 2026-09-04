# 08-init-session-db：会话数据库

> **定位**：`start_session_db`（`minix3/sbin/init/init.c:1021-1033`）、`add_session`（1038-1057）、`del_session`（1062-1075）、`find_session`（1080-1096）。**[ARCH A-1]** Berkeley DB 内存哈希 → `HashMap`。
> **Rust**：`os/commands/sbin/init/src/session_db.rs`。
> **前置依赖**：07（`Session` 节点）。
> **本篇不覆盖（移交）**：utmp 挂钩（见 13，`session_utmpx` 调用点）。

---

## 1. 概念：waitpid 只给 pid，init 需要反向索引

子进程退出时，内核只告诉 init 一个数字：pid。但 init 需要知道的是“这是哪条线路的 getty”，才能决定重启还是移除。会话链表是按线路组织的，挨个遍历找 pid 又慢又丑。数据库就是这张反向索引表：键是 pid，值是会话。`dbopen(NULL, ...)` 的 NULL 很关键——它表示内存表而非文件，进程退出表就消失，无持久化语义。Rust 用 `HashMap` 等价实现，这是全篇最干净的一次架构演进：接口与行为都不变，只是换掉了底层的库依赖。

启停语义也值得注意。`start_session_db` 先关旧表再开新表（读表重建时调用），`add` 在 DB 未开时静默返回（启动顺序的宽容），`find` 在未命中时返回空（调用方决定重启还是忽略）。失败路径都记 emergency 而不崩溃，符合 init“能修就修”的哲学。

### 1.1 小结

DB 等于 pid 到会话的内存哈希。下一章看稳态如何用它回收子进程。

---

## 2. C 源码分析

| 函数 | 行号 | 行为 |
|---|---|---|
| `start_session_db` | 1021-1033 | 关旧表（失败记 emergency）、开内存 HASH 表，失败返回 1 |
| `add_session` | 1038-1057 | DB 未开返回；pid 为键、session 指针为值插入；失败记 emergency；utmp 挂钩 |
| `del_session` | 1062-1075 | 按 pid 删除；失败记 emergency；utmp 挂钩 |
| `find_session` | 1080-1096 | DB 未开或未命中返回 NULL；命中拷出指针 |

键长均为 `sizeof(pid)`，值长为指针尺寸，无持久化、无遍历接口。

---

## 3. Rust 设计决策

`SessionDb` trait 加 `HashMapDb` 真实现与 `FakeDb` 剧本实现，双实现满足 trait 规则并关闭 01 的 OQ-1 关联（DeviceProbe 仍单 impl，见 §5 注）。`open()` 语义对应“关旧开新”，失败返回 `DbError` 而非 exit。utmp 挂钩移交 13。与 Redox 对照：Redox 同样用 HashMap 做进程索引，我们借鉴其所有权模式（值类型存储而非裸指针），避免 C 的指针拷贝。

---

## 4. 实现详解

模块 `session_db.rs`；差异：DBT 键值对改为类型化 HashMap；`memmove` 拷指针改为 `get().cloned()`；空 DB 的静默语义保留为 `Option`。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_open_insert_find` | 1027-1053+1092 |
| `test_find_missing_none` | 1092-1093 |
| `test_remove_deletes` | 1070 |
| `test_reopen_clears` | 1025-1030 |
| `test_null_db_fake` | 1044-1045/1087-1088 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：56 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/session_db.rs`。

---

## 6. 过渡

索引已建。下一站 09 多用户稳态，DB 将在每次子进程退出时被查询。

---

## 7. 参见

- `07-init-session-model.md` — 节点。
- `09-init-multi-user.md` — 查询方。
- `13-init-utmp.md` — utmp 挂钩。
- C 源码：`minix3/sbin/init/init.c:1021-1096`。
