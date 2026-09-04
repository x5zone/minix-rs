# 10 — DS 订阅与通知：登记兴趣、唤醒订阅者、取走更新

> **分类**: 数据面 handler / 订阅机制（次主线核心）
> **源码**: `minix3/minix/servers/ds/store.c:456-581`（subscribe/check）、`186-227`（match/update）
> **说明**: DS 的"推送"半：订阅者登记"我关心什么"，条目变化时被唤醒，再用 check 把更新取走。本文讲清登记、匹配、唤醒、取阅四段，以及次主线全景图。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要实现订阅流的读者。前置知识：04 的订阅表操作，05 的权限判定，07/09 的"变了调通知环"。正则表达式知道是"模式匹配"即可，细节现场讲。

### 1.2 本章不讲什么

- 正则引擎的完整实现——A-2 未决，本篇只定 trait 和默认的字面引擎（§1.5）。
- 客户端 `ds_subscribe` / `ds_check` 的包装——那是 12 的事。
- 发布和删除本身——那是 07/09 的事（本篇只讲它们调通知环的那一下）。

### 1.3 次主线全景：一条数据从发布到被取消订阅

```
发布者 ds_publish (07)                    订阅者
  │ ① 新条目落表                           │
  │ ② update_subscribers(+,1)：            │ ③ 早已 ds_subscribe 登记
  │    逐订阅：类型相交？门过？            │    (pattern + 类型掩码)
  │    正则中？→ 置位 + ipc_notify ───────▶│ ④ 收到 notify
  │                                        │ ⑤ ds_check：取首置位 → 拷键名
  │                                        │    回类型+发布者 → 清位 (本篇后半)
  │                                        │ ⑥ ds_retrieve 拉数据 (08)
  │ 删除者 ds_delete (09)                  │
  │ ⑦ update_subscribers(+,0)：            │
  │    清位 + ipc_notify ─────────────────▶│ ⑧ 再 check 得 ENOENT（无更新）
```

推送 + 拉取两段式：环只负责"喊人"（置位 + notify），数据永远是订阅者自己来拉（check 取名 + retrieve 取值）。环不搬数据——"喊"和"搬"分离，环才保持 O(订阅数) 的轻量。

### 1.4 登记：订阅的六步

`do_subscribe`（`:456-534`）：主人（无名→`ESRCH`，注意是 ESRCH 不是 EPERM——没名字连"谁"都不知道，搜无此人）→ 旧席（有旧订：没带 `OVERWRITE`→`EEXIST`，带了→释旧席 `:476`）→ 取席（满→`EAGAIN`，`:480`—— rendezvous 满了是"稍后再试"，不是"没内存"，码不同）→ 取键并加锚（`:487-490`，`^pattern$` ——子串不中，整名才中；调用者通常以为是"包含匹配"，锚定是为它好）→ 编译（`:493-498`，失败 `EINVAL`）→ 写席（`:505-508`，flags=在用+类型掩码（没写类型臂则全类型，`:501-503`）、owner、清位图）→ 可选即时扫（`INITIAL`，`:511-528`：全表找已中项，置位，有中则 notify）。

### 1.5 匹配：两道 gate（`check_sub_match`，`:186-193`）

```
check_auth(条目, 订阅者, PRIV_SUBSCRIBE)   ← 门：条目设了订阅门，非主人免谈
  && regexec(订阅正则, 条目键) == 0          ← 式：键名合模式
```

字面模式（无元字符）在 C 的 `^…$` 锚定下 ≡ 精确相等——默认的 `LiteralMatcher` 判的正是这个，结论与 C 逐字节一致。含元字符的模式需要完整引擎（A-2）：`needs_full_engine` 能检出它们，检出即 `BadPattern`（`EINVAL`）——**判不了就拒，不断言自己会**。引擎到了插进同一个 trait，表结构不动（03 D7 的源文栏就是干这个的）。

### 1.6 唤醒环（`update_subscribers`，`:198-224`）

逐订阅：跳过空席（`:208`）→ 跳过类型不交（`:210`，订阅掩码 ∩ 条目类型，空则无缘）→ 解析订阅者端点（`:213`）→ 不匹配跳过（`:214`）→ 置/清位（`:217-221`，`nr = 条目序号`）→ `ipc_notify`（`:222`）。

**一个诚实的偏离**：`:213` 的 `ds_getprocep` 在名字无标签时 `panic`——死订阅者能拖垮整个注册中心。Rust 跳过该席（位不动，不唤醒），计数进 `skipped_stale`。可用性高于崩溃，本文和代码注释两处声明。这是对 bug 的修复，不是对语义的改（语义是"唤醒活人"，不是"死人来了大家一起死"）。

### 1.7 取阅（`do_check`，`:536-578`）

主人（无名→`ESRCH`）→ 找自己的订阅（无→`ESRCH`，`:549`）→ 扫首置位（无→`ENOENT`，`:557`——"订了但没更新"和"没订"是两种空，码不同）→ 拷键名经 key grant（`:561-563`，失败不消费：清位在成功之后，`:575`）→ 回填类型掩码 + 发布者端点（`:570-572`，写请求栏，02 §1.3）→ 清位（`:575`）→ `OK`。

"首置位优先"（从 0 往上第一个）= 更新按条目序号从小到大消费——确定性顺序，不饿死。

### 1.8 小结

登六步（主/旧/席/键/式/扫），配两 gate（门+式），环逐席（空过/型过/名解/式判/置清/喊），阅取首位（拷→填→清）。下一站 11——镜像外借：`getsysinfo`。

---

## 2 C 源码分析

### 2.1 登记（`do_subscribe`，`:456-534`）

主人（`:466-468`）→ 旧席与覆盖（`:470-477`）→ 取席（`:480-481`）→ 锚定取键（`:487-491`，`regex[80+2]` 缓冲：`^` + 键 + `$`）→ 编译（`:493-498`，`REG_EXTENDED`，败则清零回 `EINVAL`）→ 类型掩码（`:501-503`，0 则全掩码）→ 写席清图（`:505-508`）→ 即时扫（`:511-528`，命中置位，有中则 `ipc_notify(src)`）→ `OK`。

注意取席在取键**之前**（`:480` vs `:488`）：满席拒优先于坏键拒——顺序是语义，Rust `plan_subscribe` 同序。

### 2.2 取阅（`do_check`，`:536-578`）

主人（`:544-546`）→ 找订（`:549-550`）→ 扫首位（`:553-558`）→ 拷键（`:561-563`）→ 回填（`:570-572`，`flags = 条目flags & MASK`，`owner = ds_getprocep(条目owner)`——**panic 点**）→ 清位（`:575`）→ `OK`。

### 2.3 匹配与唤醒（`:186-227`）

`check_sub_match`（`:186-193`）：门 + 式，两 gate。`update_subscribers`（`:198-224`）：序号（`:204`，指针减法得下标）→ 逐席六步（见 §1.6）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 匹配变 trait | `regex_t` + `regexec` 直调 | `PatternMatcher` trait + `LiteralMatcher` + `needs_full_engine`（`subscribe.rs`） | 完整引擎（A-2）未到；trait 让"字面精确"先落地且结论与 C 一致，引擎到了只加实现不改表 |
| D2 | 喊搬分离 | 置位 + `ipc_notify` 内联 | `notify.rs` 回端点数组 + 统计，发送归传输 | 置位是状态（可测），发送是 IO（不可测）——分开，前者全测 |
| D3 | 死席跳过 | `panic`（`:213`、`:137` 经由） | `skipped_stale` 计数（`notify.rs`） | §1.6：可用性高于崩溃；偏离声明两处（本文 + 代码） |
| D4 | 阅分判清两步 | 查 + 拷 + 清一锅 | `plan_check`（verdict）+ `apply_check`（清位）（`check.rs`） | 拷失败不消费（`:575` 在拷后）——判和清分开，调用方"拷成了才清"才写得出来 |
| D5 | 回填显式化 | 写 `m_ds_req` 栏（`:570-572`） | `CheckHit::reply_type` / `reply_owner`（`check.rs`） | 02 D3 同理：请求栏装回信是最易误读处，命名函数防误读；`reply_owner` 回 `Option`（panic 点转交调用方） |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
├── subscribe.rs — 本篇前半：SubscribeReject / PatternMatcher / LiteralMatcher /
│                   needs_full_engine / SubscribeArgs / plan_subscribe /
│                   apply_subscribe / entry_matches
├── notify.rs    — 本篇中段：SweepStats / apply_update / initial_scan
└── check.rs     — 本篇后半：CheckReject / CheckHit / plan_check / apply_check
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 登记五拒 | `store.c:466-498` | `subscribe.rs`（`SubscribeReject`） | 源/存/满/键/式→errno |
| 登记 verdict | `store.c:466-508` | `subscribe.rs`（`plan_subscribe`） | 六步（C 序，取席先于取键） |
| 登记落席 | `store.c:505-508` | `subscribe.rs`（`apply_subscribe`） | 先释旧席，再写旗主式图 |
| 匹配两 gate | `store.c:186-193` | `subscribe.rs`（`entry_matches`） | 门 + 型预筛 + 式 |
| 唤醒环 | `store.c:198-224` | `notify.rs`（`apply_update`） | 逐席六步，回端点数组 |
| 即时扫 | `store.c:511-528` | `notify.rs`（`initial_scan`） | 有中则唤源一次 |
| 取阅两拒+一空 | `store.c:544-558` | `check.rs`（`CheckReject`） | 源/订/更新 |
| 取阅 verdict | `store.c:544-575` | `check.rs`（`plan_check`/`apply_check`） | 首位优先，拷成后清 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 取席先于取键 | verdict 顺序 | `store.c:480` vs `:488` |
| 空掩码即全类型 | `mask_bits.is_empty()` 分支 | `store.c:501-503` |
| 拷败不消费 | 清位独立函数，调用方拷后调 | `store.c:575` 在拷后 |
| 首位优先 | `find` 自 0 起 | `store.c:553-558` |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 18 个测试（subscribe 9 + notify 5 + check 4）。

| 测试组 | 代表测试 | 覆盖 C 位置 | 行为 |
|--------|----------|-------------|------|
| 登记 | `test_first_subscribe_takes_seat_zero` | `:480` | 首订占 0 席 |
| 登记 | `test_second_subscribe_without_overwrite_refuses` | `:473` | 无修饰重订 `EEXIST` |
| 登记 | `test_overwrite_frees_old_seat_first` | `:476` | 带修饰释旧占新 |
| 登记 | `test_empty_mask_means_all_types` | `:501-503` | 空掩码即全类型 |
| 匹配 | `test_literal_engine_matches_exactly` | `:190-193` | 字面精确等价 |
| 匹配 | `test_meta_patterns_need_full_engine` | A-2 | 元字符可检出 |
| 唤醒 | `test_publish_wakes_matching_subscriber` | `:198-224` | 中者置位+唤醒 |
| 唤醒 | `test_delete_clears_bit_and_still_wakes` | `:217-222` | 清位仍唤醒 |
| 唤醒 | `test_stale_owner_skips_without_panic` | `:213` 偏离 | 死席跳过计数 |
| 唤醒 | `test_initial_scan_*`（2） | `:511-528` | 有中唤源 / 无中静默 |
| 取阅 | `test_takes_lowest_set_bit_first` | `:553-558` | 首位优先 |
| 取阅 | `test_consume_advances_to_next` | `:575` | 消费推进 |
| 取阅 | `test_stranger_without_subscription_is_esrch` | `:549` | 无订 `ESRCH` |

---

## 6 过渡

订阅流讲完了：登、配、喊、阅四段，次主线收口。下一站 11——镜像外借：别的服务器怎么一次性读走整张表。

## 7 参见

- C 源：`minix3/minix/servers/ds/store.c:456-581,186-227`
- 阶段文档：`09-ds-delete.md`（上一站）、`11-ds-getsysinfo.md`（下一站）、`07-ds-publish.md`（环的发布侧调用点）
- Rust 实现：`os/servers/ds/src/subscribe.rs`、`os/servers/ds/src/notify.rs`、`os/servers/ds/src/check.rs`
