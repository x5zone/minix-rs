# 09-devm-bind-unbind：绑定、解绑与装配

> **定位**：设备生命的就绪态 + 全 stage 装配。本篇回答：RS 握手四步、owner 转发、19 特例、ZOMBIE 守卫，以及 ADD→DEL 全旅程如何跑过同一个入口。
> **源码**：`bind.c`（105 行，全部：`do_bind_device` :7-50、`do_unbind_device` :56-104）。
> **Rust 模块**：`os/servers/devman/src/bind.rs`（`do_bind/do_unbind` + 应答 halves + `Action`）+ `src/server.rs`（`Server` + `handle_other` + `OutAction`）。
> **前置依赖**：05（`check_rs`/`apply_reply`/Result 相位）、07（owner 来源）、08（`put` 级联与配对表）。
> **不覆盖（移交）**：客户端 bind_cb 实现（10/11）、RS 发布流程（12）、传输发送（传输层执行 `OutAction`）。

---

## 1. 概念：绑定是三方握手，不是一句赋值

`state = BOUND` 看似一行，实则是三方合意：**RS 发起**（publish 服务时决定绑谁）、**devman 转发**（查树、找 owner、递话）、**驱动应答**（`bind_cb` 说行）。任何一方摇头都不落地：RS 没发则无事发生（非 RS 发了则 EPERM 不回），树上没此设备则 ENODEV，驱动说不行则保持原态。三步跨两次 IPC（RS→devman→driver→devman→RS），devman 是中间人，不是拍板人——这是本篇与 07/08 最大的不同：07/08 一次调用就终局，09 的一次调用只走到"已转发"，终局在驱动的回话里。

次主线（00 §1.2 旅程）至此闭环：10 注册 → 07 落户 → 06 广播 → 13 起驱动 → 12 发布 → **本篇握手** → BOUND。绑定段路径图见 §1.3。

### 1.1 边界声明

本篇讲握手机制（门/转发/应答/特例）与装配（`Server::handle_other`）。不讲：bind_cb 实现（10/11）、RS 为何此时 publish（12）、消息发送原语（05/传输层）。

### 1.2 绑定段路径图（次主线收官段）

```
RS publish_service（12）
  │ devman_id != 0 → ds 查 label → DEVMAN_BIND(device_id, endpoint)
  ▼
devman handle_other → dispatch → Bind（05）
  │ check_rs：非 RS → Dropped（无声，05 §2.4）
  │ find 缺席 → Reply ENODEV
  ▼
Forward → owner（驱动）：m_type=DEVMAN_BIND（原样转交，endpoint 字 RS 已填）
  │ driver bind_cb（10/11）行 → on_bind_response(Ok)
  │   → state=BOUND + get（+1，08 配对表）→ Reply OK → RS
  │ driver 不行 → on_bind_response(Err) → Reply 原错 → RS（状态不动）
  ▼
UNBIND 对称，差两处（§2.3）：应答 19 容错 + ZOMBIE 守卫
```

---

## 2. C 源码分析

### 2.1 绑定四步：门 → 找 → 转发 → 收尾（bind.c:7-50）

本篇覆盖的 C 符号为 `do_bind_device` / `do_unbind_device` / `_find_dev`（树查找见 04 §2.3；`main` 的 switch 形见 01 §2.4/05 §2.5）。

1. **门**：`src != RS_PROC_NR → EPERM` 不回（:11-19，05 §2.4 已钉死）。
2. **找**：`devman_find_device(DEVICE_ID)`，缺席 → ENODEV（:20-21/44-46——注意缺席也**回复**（与门不同：门不回，找不着回——"无声"只属于越权者，不属于合法但指错的 RS）。
3. **转发**：`m->m_type = DEVMAN_BIND`（改同一条消息！endpoint/DEVICE_ID 字不动——"device ID and endpoint is still set"，:24-25 注释原文）+ `ipc_sendrec(dev->owner, m)`（**同步**等待驱动，:32——与回复的异步 `ipc_send` 对比：转发要等，回复不等）。
4. **收尾三分支**（:33-43）：sendrec 失败 → RESULT=错误码；驱动 RESULT 非 OK → 保留驱动错（状态不动）；驱动 OK → `state=BOUND` + `get`（+1，08 配对表）。

收尾恒发：`m_type=REPLY + ipc_send(RS)`（:47-48——do_bind 不用 `do_reply`，自写两行，05 §2.3 已登记；Rust 统一 `apply_reply`，09 §3.3）。

### 2.2 解绑五步：多两处不对称（bind.c:56-104）

形同绑定，差两处：

1. **19 容错**（:85）: `m->DEVMAN_RESULT != OK && != 19` 才算错——19 即 ENODEV（errno.h:19），注释"device drive deleted device already?"（:86 原文，含拼写 drive）：驱动先删了设备（驱动侧 `devman_del_device`），再也 unbind 不出什么——**视为成功**。这是全 stage 唯一"把错误码当成功"的分支，单测 `unbind_enodev_tolerated_and_forced_ok` 双锁（状态照转 + 回复强制 OK）。
2. **ZOMBIE 守卫**（:90-92）：`if (state != ZOMBIE) state = UNBOUND`——ZOMBIE 的设备 unbind 后**保持 ZOMBIE**（名分已除，不开倒车）。UNBOUND→UNBOUND（空转，无害）。
3. **强制 OK**（:94）：走 else 分支必 `RESULT = OK`（覆盖驱动的 19）。

缺席 → ENODEV 回复（:96-99，注释"perhaps its better to keep the device…"——删了的设备 unbind，C 选择回 ENODEV 而非静默，09 §3.3 论证保留）。

### 2.3 转发语义：原样转交（endpoint 字 RS 预填）

`m_type` 改 BIND/UNBIND，其余字不动——endpoint 是 RS 在 BIND 请求里填好的（12 §2 会展示 publish 填字），devman 是邮差，不拆信。Rust 的 `Forward { owner, bind, device, endpoint }` 四字段即此四要素（owner 查树得，其余三原样传）。

---

## 3. Rust 设计决策

### 3.1 两段式 handler：请求半 + 应答半（跨越 IPC 的函数拆分）

C 的 `do_bind` 内含 `sendrec`（阻塞等驱动），Rust 拆 `do_bind`（门+找+转交动作）与 `on_bind_response`（驱动回话后收尾）。拆分线即 IPC 线： transport 执行 `Forward`（调 `ipc_sendrec`），回话路由回 `on_*_response`。单测无需 IPC（`Ok(())`/`Err(EIO)` 直调）。`Action::Dropped` 把"不回"变成类型（05 §3.3 的测试锁升级：`assert_eq!(do_bind(非RS), Dropped)` 即"无发送"证明）。

### 3.2 应答错误即原文（不包装、不翻译）

驱动错原样返 RS（`Err(e) → Err(e)`），sendrec 传输错同样（C :36/84 同）。devman 不翻译驱动的错误码——中间人不改信（与 §2.3 同款"邮差"原则）。

### 3.3 装配：`Server` 有主状态机（09 §4）

`Server { vtreefs, devices, events_cookie }` + `handle_other` 六分支（dispatch 结果直连 07/08/本篇 halves + 05 原语）。`OutAction` 三变体是传输契约（Reply/Forward/Nothing）。单例未建（main 仍 park，传输阶段接线——P1-6 不变；装配 100% 可测，无需全局）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/
  bind.rs   — Action / do_bind / on_bind_response / do_unbind / on_unbind_response（+4 测试）
  server.rs — OutAction / Server / handle_other / answer_*（+2 测试）
```

### 4.2 引用配对表（08 §4.2 右半边）

| +1 | −1 | 锁 |
|---|---|---|
| on_bind_response Ok → ref+1 | on_unbind_response → put（08） | `bind_ok_holds_ref_until_unbind`（BOUND→UNBOUND 全程不断号） |
| — | unbind 19 照转照放 | `unbind_enodev_tolerated_and_forced_ok` |

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| do_bind 含 sendrec（阻塞） | do_bind + on_bind_response 两段 | 分层（IPC 线拆分，单测无需 IPC） |
| 自写 reply 两处 | 统一 OutAction::Reply（传输调 apply_reply） | 去重（05 §2.3 预告兑现） |
| handler 返回 int（恒 0） | Action/Result | A-7 |
| owner 裸端点（恒有值） | `unwrap_or(source)` 兜底 | 防御默认（注释；覆盖路径恒 Some） |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `non_rs_is_dropped_silently` | 双 Dropped（无发送即证明） | §2.1 门 |
| `missing_is_enodev_reply` | 双 Reply ENODEV | §2.1/§2.2 缺席 |
| `bind_ok_holds_ref_until_unbind` | 转发字段 + BOUND/get + 错不动 + UNBIND/put | §2.1 收尾 + §4.2 |
| `unbind_enodev_tolerated_and_forced_ok` | 19 照转 + 强制 OK + 他错透传 | §2.2 三不对称 |
| `lifecycle_add_bind_unbind_del` | ADD→BIND→UNBIND→DEL 全旅程单入口 | §1.2 路径图可执行版 |
| `unknown_is_nothing` | A-6 码 → Nothing | 05 §2.6 装配侧 |

截至 2026-09-04：`cargo test -p minix-devman` **73 passed / 0 failed**（59 + 07 的 3 + 08 的 4 + 本篇 6：bind 4 + server 2）。`cargo clippy` devman 部分 0 警告。

---

## 6. 过渡

handler 三篇收官：ADD 建（07）、DEL 拆（08）、BIND 交接（本篇）。设备生命全旅程已可跑通（`lifecycle_*` 即证明）。下一站 10/11 到旅程起点换视角——驱动侧如何构造 wire（`serialize_dev`，03 解码的镜像）与 USB 建模（属性从哪来）。读 10 时若忘记 `owner` 谁填的，回看 07 §2.4（`owner = m_source`）。

---

## 7. 参见

- `05-devm-message-contract.md` — 门/回复/分发原语（本篇调的三件套）
- `07-devm-add-device.md` — owner 来源与出生值（本篇转发的依据）
- `08-devm-del-device.md` — 配对表左半边与 ZOMBIE 语义
- `10-libdevman-client.md` — bind_cb 实现（转交的另一端）
- `12-rs-integration.md` — publish 填字（endpoint 来源）
- C 源：`minix3/minix/servers/devman/bind.c`
