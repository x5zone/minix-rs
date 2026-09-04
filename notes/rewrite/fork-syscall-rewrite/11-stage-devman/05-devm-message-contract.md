# 05-devm-message-contract：消息面契约

> **定位**：devman 的协议面。本篇回答：10 种消息谁定义、字段宏怎么读、grant 拷贝怎么 work、回复怎么发、谁有资格发 BIND、fall-through 到底是不是 bug、5 种没实现的消息怎么办。判定只做一次，后续篇只引用。
> **源码**：`minix3/minix/include/minix/com.h:846-866`（常量 + 字段宏）+ `device.c:213-219`（`do_reply`）+ `device.c:223-250`（ADD 前导：grant 拷贝三错）+ `bind.c:7-19,56-68`（RS-only 门）+ `main.c:46-58`（分发形，01 已走读）。
> **Rust 模块**：`os/servers/devman/src/ipc/`（`message.rs` 字段视图 + `dispatch.rs` 单分派）+ `minix-types/src/types/com.rs`（DEVMAN 数字权威）。
> **前置依赖**：01（message_hook 调用点）、03（wire 结构）。
> **不覆盖（移交）**：各 handler 业务（07/08/09 只调本篇原语）、事件格式（06）、客户端构造（10/11）、RS 发布流程（12）。

---

## 1. 概念：消息是 devman 唯一的写通道

01 §2.1 立过规矩：设备树只读（0444），从不通过文件写通道变更。于是**所有变更都走消息**：驱动注册/删除设备（ADD/DEL，人人可发）、RS 替服务绑/解绑驱动（BIND/UNBIND，仅 RS 可发）。读走文件（06），写走消息——devman 的全部外部行为就是这张表：

| 消息 | 发送方 | 语义 | 回复 |
|---|---|---|---|
| ADD_DEV | 驱动（经 libdevman） | 注册设备，返 dev_id | REPLY + RESULT/DEVICE_ID（07） |
| DEL_DEV | 驱动 | 删除设备 | REPLY + RESULT（08） |
| BIND | **仅 RS** | 绑驱动到设备 | REPLY + RESULT（09，经驱动转发） |
| UNBIND | **仅 RS** | 解绑 | REPLY + RESULT（09，经驱动转发） |
| ADD_BUS/DEL_BUS/ADD_DEVFILE/DEL_DEVFILE/REQUEST | — | 声明未实现（A-6） | 无（忽略） |

### 1.1 同词多义：m4 三个字的两副面孔

C 消息是 64 字节定长（`m_source` + `m_type` + 56 字节 union，`Message` 结构见 minix-types）。DEVMAN 用 `m4` 视图（三个 long 字），字段宏（com.h:859-864）是：

```c
#   define DEVMAN_GRANT_ID       m4_l1
#   define DEVMAN_GRANT_SIZE     m4_l2
#   define DEVMAN_ENDPOINT       m4_l3
#   define DEVMAN_DEVICE_ID      m4_l2   /* 与 GRANT_SIZE 同字 */
#   define DEVMAN_RESULT         m4_l1   /* 与 GRANT_ID 同字 */
```

`DEVICE_ID` 与 `GRANT_SIZE` 是**同一个字**的不同人生：请求阶段它是 grant 大小，回复阶段它是设备号。`RESULT` 与 `GRANT_ID` 同理。读消息必须先问"现在是哪个阶段"——本篇 `message.rs` 的整张相位表（§3.1）就是干这个的，调用点禁止裸读 `m4l*`。

### 1.2 边界声明

本篇定协议（形状、权限、分发、回复原语）。handler 内部不展开：ADD 的树操作（07）、DEL 的引用计数（08）、BIND 的转发握手（09）各归其篇——但三篇都调本篇的 `check_rs`/`apply_reply`/`dispatch`，引用关系见 §4.2。

---

## 2. C 源码分析

### 2.1 常量块：1 基址 + 10 消息 + 5 字段宏（com.h:846-866）

`DEVMAN_BASE 0x1200`；ADD 0 / DEL 1 / ADD_BUS 2 / DEL_BUS 3 / ADD_DEVFILE 4 / DEL_DEVFILE 5 / REQUEST 6 / REPLY 7 / BIND 8 / UNBIND 9（`+n` 连续，无空洞——与 DS 的 +5 空洞不同，com.rs 测试锁全集）。字段宏 5 个（§1.1 表）。数值权威已收口进 `minix-types`（`test_devman_messages` 锁 0x1200~0x1209 + RS_PROC_NR==2）。

### 2.2 grant 拷贝：三错三码（device.c:228-250，ADD 前导）

```c
devinf = malloc(msg->DEVMAN_GRANT_SIZE);
if (devinf == NULL) { res = ENOMEM; do_reply(msg, res); return 0; }

res = sys_safecopyfrom(ep, msg->DEVMAN_GRANT_ID, 0, (vir_bytes) devinf, msg->DEVMAN_GRANT_SIZE);
if (res != OK) { res = EINVAL; free(devinf); do_reply(msg, res); return 0; }
```

三点。第一，**先按对方声明的大小分配**（信任 GRANT_SIZE 做 malloc——grant 机制保证发送方 grants 的内存可读，但大小是对方说的；超大声明即超大分配，C 不设上限——Rust 侧 07 设上限，见 07 §3，A-4 的延续）。第二，safecopy 失败映 **`EINVAL`**（不是 EFAULT——"grant 无效"在 devman 语义里是"请求非法"，不是"地址错"；01 轮 P0-2 的姊妹教训：错误码逐个核对，不许想当然）。第三，错误分支全部 `free + do_reply + return 0`——分配与释放配对（03 §2 wire 解码的输入即此 `devinf`）。

`safecopyfrom(ep, grant, 0, dst, size)` 本身是内核原语（跨地址空间拷贝，grant 即能力）：ep 限定"从谁那拷"，grant 限定"哪块"，offset 0 全拷。这是整个 ADD 流程的信任根——07 的其余部分都建立在"devinf 已在本地"之上。

### 2.3 回复原语：改同一条消息，异步发回（device.c:213-219）

```c
static void do_reply(message *msg, int res)
{
    msg->m_type = DEVMAN_REPLY;
    msg->DEVMAN_RESULT = res;
    ipc_send(msg->m_source, msg);
}
```

改的是**收到的那条消息**（`m_source` 不动，所以知道回给谁；`DEVICE_ID` 字由调用方事先填好——ADD 成功路径在 `do_reply` 前写 `msg->DEVMAN_DEVICE_ID = dev->dev_id`，device.c:268）。发送用 `ipc_send`（**异步**，发完不管——C 从不对回复 `sendrec`；回复的回复不存在）。Rust 的 `apply_reply` 原地改（`m_source` 不动）+ 清零其余字（C 留请求残留字，Rust 清零——残留是信息泄漏面，§3.4 取舍）。

注意 `do_reply` 是 `static`（device.c 私有），bind.c 不用它（自写 `m_type=REPLY + ipc_send(RS)`，bind.c:47-48/101-102——行为同，形式散；Rust 统一调 `apply_reply`，09 会收敛这两处）。

### 2.4 RS-only 门：EPERM 写了，但不回（bind.c:11-19, 60-68）

```c
endpoint_t src = m->m_source;
if (src != RS_PROC_NR) {
    m->DEVMAN_RESULT = EPERM;
    printf("[W] could bind message from somebody else than RS\n");
    return 0;   /* 注意：没有 ipc_send——发送方永远收不到回复 */
}
```

这是全文最值得多看一眼的三行：权限检查本身平淡（`m_source != RS` → EPERM），**不回**才反常——`RESULT` 写了，但函数直接返回，消息石沉大海。发送方（非 RS 的捣乱者）永远等不到回复（若它 `sendrec` 则永远阻塞——自作自受，C 的冷酷正在于此）。Rust 原样保留（`check_rs` 返 `Err(EPERM)` + 调用方**禁发**，§3.3 单测锁"Err 即无发送"——用"无发送"断言而非仅错误码断言）。

不对称全貌：ADD/DEL **无门**（任何进程可发——设备注册是开放操作，驱动又不是特权进程），BIND/UNBIND **RS 门**（绑定关系变更只认 RS）。`RS_PROC_NR == 2`（com.h:61，端点号）。

### 2.5 分发形：switch 无 break（`message_hook`，main.c:46-58，01 §2.4 现象回顾）

01 只记录现象，定性归本篇。证据链（判定 fall-through 是 bug 不是协议，共三环）：

1. **客户端可观察契约**：libdevman 的 `devman_add_device` 成功返回后，设备必须可见、可读、可 bind（10 §3 会展示客户端的成功路径断言）——fall-through 下 ADD 紧跟 DEL，同消，契约即破。除非有消费者依赖"ADD 后立即 DEL"（荒谬），否则行为不可辩护。
2. **回复语义**：ADD 的贯穿产生**两次** `do_reply` 发送——`do_add_device` 末尾回一次（OK + DEVICE_ID，device.c:~275），`do_del_device` 末尾又回一次（device.c:452，同条消息、同一通道）。调用者连续收到两条 REPLY：第一条说"设备建好了，id 是 N"，第二条说"好，删完了"（del 用 ADD 刚写入 `m4_l2` 的 id 找到了它，`_find_dev` 实证链闭合）。bind/unbind 两臂各写一次 EPERM 但都不发送（§2.4）。调用者无法从回复流推断任何一致状态。
3. **错误放大**：一条合法 ADD 附带两次 EPERM（bind/unbind handler 认不出 RS 之外的发送方——驱动根本不是 RS），调用者看到莫名其妙的 EPERM。

修复（[ARCH:A-3]）：按 `m_type` 单分派，恰一 handler（`dispatch.rs`）。三处标注：本节 + 设计快照 + `dispatch.rs` 模块头。三处一致（scan Gate H 记录）。

### 2.6 未实现消息：5 个零引用（A-6）

`rg "DEVMAN_ADD_BUS|…|DEVMAN_REQUEST" minix3/ | grep -v com.h` → 空（plan §7.2 D-3 证据，2026-08-16；本轮重放确认仍空，见 scan V11）。C switch 无对应 case——未知消息自然穿过（无匹配分支，函数返回，**无回复**）。Rust：`dispatch` 映 `Ignored`（不运行、不回复，与 C 逐行一致）+ 常量保留（com.rs 已收，注释标 A-6）。fail-closed 的含义在这里是"不存在的分发目标"，不是"返回错误"——返回错误反而是新行为（调用者以前收不到任何东西，现在收到 ENOSYS 会改变重试逻辑；行为重写的第一原则：不请自来的"改进"是语义漂移）。

---

## 3. Rust 设计决策

### 3.1 相位表：同词不同命（message.rs 模块头大表）

ADD/DEL 请求相（grant 二字）/ BIND 请求相（endpoint + device_id）/ 回复相（result [+device_id]）——调用点只调 `grant_id()`/`device_id()` 等命名视图，禁止裸读 `m4l*`（review 时 grep `m4l` 只应命中 message.rs 一处定义点）。

### 3.2 单分派：match 即证明（dispatch.rs）

C 的正确性依赖"记得写 break"（人记性），Rust 的 `match` 穷尽且无贯穿（编译器记性）——fall-through 类 bug 在类型层面**不可表达**。`dispatch` 全覆盖 `i32`（通配 `_` 收未知）。`devman_message_hook`（hooks.rs；01 的 `main` 注册的三个钩子之一）即 `dispatch(m_type)` + 按臂转交——05 落地时 01 §3.6 预告的"替换为单 handler 分派"已经兑现：各臂在 07-09 落地前为忽略占位（注释指向 owner），`Ignored` 臂永久忽略。01 doc 无需改字（它写的是"05 落地时替换"，现在就是落地；01 scan 的前向引用记录同步闭合，见 05 scan 交叉项）。

### 3.3 EPERM 不回：用测试锁"无发送"（message.rs `check_rs`）

`check_rs` 返 `Err(EPERM)`；09 的调用规约是"Err 即 stamp 后静默返回"。单测断言两层：错误码是 EPERM **且**无发送动作（测试替身 Transport 记录发送列表，空）。"且"字是重点——只断言错误码的测试放过了"误发回复"的回归。

### 3.4 回复清零：残留字是信息泄漏面

C 的 `do_reply` 只改两字，grant id/size 残留仍在消息里发回（发送方本来就知道，无大碍——但"无大碍"不是"应该"）。Rust `apply_reply` 重建 `m_m4`（result 字 + 全零）。行为等价（接收方可读位全同）， hygiene 更好。差异表记"设计决策（等价 hygienic）"。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/ipc/
  message.rs  — 相位视图（grant_id/grant_size/request_endpoint/device_id/result）+ apply_reply + check_rs（+3 测试）
  dispatch.rs — Handler 五变体 + dispatch()（+2 测试）
  minix-types/src/types/com.rs — DEVMAN 数字块 + RS_PROC_NR（+1 测试 test_devman_messages）
```

### 4.2 调用关系（本篇是原语层）

```
02 run → Other → 01 message_hook(桩) → 05 dispatch (05 落地后替换桩内部分发)
07 do_add ─┬─ grant 读数 (message::grant_*)
           └─ apply_reply (成功/ENOMEM/EINVAL/ENODEV)
08 do_del ──── apply_reply
09 do_bind/unbind ─┬─ check_rs (EPERM 不回)
                   ├─ 转发 owner (ipc_sendrec，09)
                   └─ apply_reply → ipc_send(RS)
```

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| switch 贯穿 4 handler | match 单分派 | A-3（bug 修复，三环证据） |
| 未知消息穿过无回复 | Ignored 无回复 | 沿用（fail-closed 即"不新行为"） |
| do_reply 留残留字 | apply_reply 清零 | 设计决策（等价 hygienic） |
| bind.c 自写 reply 两处 | 统一 apply_reply（09 收敛） | 去重（09 执行） |
| EPERM 不回（隐式） | check_rs + "无发送"测试锁 | 显式化（行为同） |
| `long` 字（32 位 Minix） | i64→i32 窄化（值域内保值） | 64 位适配（注释） |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `test_devman_messages`（minix-types） | 0x1200~0x1209 全集 + RS==2 | §2.1 |
| `consts_match_com_h` | devman 侧导入值一致 | §4.1 权威唯一 |
| `field_views_share_words_per_phase` | grant 相/device 相/回复相 + source 不动 | §3.1/§2.3 |
| `rs_gate_permits_only_rs` | RS 过/他者 EPERM | §2.4 |
| `four_messages_route_singly` | 一对一 | A-3 |
| `unknown_is_ignored_without_reply` | 5×A-6 + REPLY + 垃圾值全 Ignored | §2.6 |

截至 2026-09-04：`cargo test -p minix-devman` **59 passed / 0 failed**（45 + 本篇 5：message 3 + dispatch 2；com.rs 另 +1 在 minix-types 139 total）。`cargo clippy` 0 警告（devman 部分）。

---

## 6. 过渡

协议钉死：谁能发什么、字怎么读、错了回什么（或不回）、未知怎么办。下一站 06 给树接上**读**（事件行怎么写进 events 文件、读两次才算拿走一个事件），然后 07 走完全流程：收 ADD → grant 拷入 → wire 解码（03）→ 种树（04）→ 发事件（06）→ 回复（本篇 `apply_reply`）。读 07 时若忘记 `GRANT_SIZE` 谁提供，回看 §2.2 的信任根。

---

## 7. 参见

- `01-devm-init-main.md` — message_hook 调用点（分发形现象）
- `03-devm-structs.md` — wire 结构（grant 拷入后的解码对象）
- `06-event-buf.md` — 事件行格式（ADD 行的另一半）
- `07-devm-add-device.md` — grant 拷贝的调用方 + `apply_reply` 主用户
- `09-devm-bind-unbind.md` — `check_rs` 的调用方 + 转发握手
- `99-devm-global-concepts.md` — 常量收口（本篇 com.rs 块的归宿说明）
- C 源：`minix3/minix/include/minix/com.h:846-866`、`device.c:213-250`、`bind.c:7-19,56-68`、`main.c:46-58`
