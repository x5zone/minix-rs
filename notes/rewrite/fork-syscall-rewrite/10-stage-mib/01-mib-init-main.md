# 01 — 系统信息库服务启动入口与主循环：三类请求、两种拒绝情形、一条回信规则

> **分类**: 服务器入口 / 主循环骨架
> **源码**: `minix3/minix/servers/mib/main.c`（492 行，本篇覆盖 `:277-383` 解码与 `:415-492` 启动循环；`:64-275` 拷贝与鉴权原语移交 06/07，`:36-64` 静态表与 `:384-413` 初始化体移交 04）
> **说明**: 系统信息库服务（MIB，Management Information Base）是启动映像里系统控制调用 sysctl(2) 的归口服务，它的主循环一生只做三件事：接收消息、按消息类型分发到三条路径、按规则决定是否回信。本文讲清这三件事、启动时的两类初始化注册，以及一次系统控制请求进门时要过的六道检查。

---

## 1 概念

### 1.1 目标读者与前置知识

面向第一次读 Minix3 用户态服务的读者。前置知识：C 语言，操作系统里"服务器循环收消息"的基本想法。`sysctl` 是什么不需要预习——把它理解成"按名字读写系统信息"，细节在用到时一句话解释。IPC 原语（`sef_receive_status` / `ipc_sendnb`）的名字会在正文里出现，语义同样现用现讲。

### 1.2 本章不讲什么

- 三封信里每封信的**字段含义**——那是 02 的事（协议面）。
- 名字进门之后**怎么查到数据**——那是 05（查找）、10（分发）、09/13~20（handler）的事。
- 拷贝字节、grant、鉴权缓存怎么做——那是 06/07 的事。
- 静态树长什么样、初始化体里四行 wiring——那是 03/04 的事。

### 1.3 为什么主循环长成这样

系统信息库服务的本质是一个**单线程的按名查询服务器**：它不管理硬件，不调度进程，不碰页表。它的一生就是：等一封消息，看消息头上的调用号，要么按名字查数据并返回，要么代为挂载一棵远端子树，再回一封信。因为逻辑是线性的，主循环不需要线程、不需要锁——同一时刻只处理一封消息。这和目录服务（DS，07-stage-ds/01 篇讲的三步循环）是同一个形状：接收、分发、回复。微内核服务器大多长这样（Redox 的 scheme 服务、seL4 的用户态服务器同理：单线程收消息是默认形状，多线程是例外）——不是 Minix3 的癖好，是"一次只答一封信"这个约束的自然结果。

整个循环可以画成这样：

```
                ┌──────────────────────────────────┐
                │ mib_startup()：注册两类初始化回调 │  启动时一次
                │ 全新启动 → 构建整棵树 / 重启 → 仅重建静态部分│
                └───────────────┬──────────────────┘
                                 ▼
┌─────────┐   ┌───────────────────────────┐   ┌───────────┐
│ 接收消息 │──▶│ 按消息类型分发的三条路径   │──▶│ 决定回信   │
│ receive │   │ 内核通知→跳过 / 三类已知请求→处理 │   │ reply     │
│ 主循环   │   │ /未知编号→看是否为阻塞式调用   │   │ 主循环     │
│ :445    │   │ :449-479                  │   │ :482-487  │
└─────────┘   └───────────────────────────┘   └───────────┘
```

三条路径的第一条值得停一下：**本服务的通知由内核直接告知**。本服务用 `sef_receive_status` 收信，内核顺手告诉它"刚才那是一次通知"（`is_ipc_notify`，`com.h:92`）。目录服务没有这个待遇——它用不带状态的 `sef_receive` 收信，只能拿调用号猜（`is_notify` 宏，`com.h:90-93`，还标着 FIXME）。所以同样的"拒绝通知"，本服务是听判决，目录服务是做推理。这是读两个服务器主循环时最容易忽略的差别：**接收原语决定了分发的第一行怎么写**。

启动时的两类注册（`mib_startup`，`main.c:415-428`）分别是：全新启动回调 `sef_setcb_init_fresh(mib_init)`、重启回调 `sef_setcb_init_restart(mib_init)`。注意两类注册的是**同一个函数体**——和目录服务完全相反的选择。目录服务的重启走通用保持态（表不能丢，丢了注册中心就没意义了）；本服务的重启跑同一套初始化，**运行时动态创建的节点全丢，只剩静态骨架**（`main.c:420-424` 注释说得很直白：只剩静态树也比不跑强）。记住这个对照：**目录服务重启保留全部状态，本服务重启仅保留静态部分**。后文 08（动态节点）和 12（远程挂载）讲的"丢了之后谁负责重建"，答案就藏在这两行注册里。

分发规则（`main`，`main.c:458-479`）按下表走：

| 到达的消息类型 | 触发条件 | 处理动作 | C 位置 |
|------|------|------|--------|
| 内核通知 | `is_ipc_notify(ipc_status)` 为真 | 打印来源后继续等待下一封消息（不回信，连沉默标记都不经过） | `main.c:449-454` |
| 三类已知请求 | 系统控制查询 / 注册远端子树 / 注销远端子树（`com.h:1026-1028`，基址 `0x1800`） | 调用对应的处理函数 | `main.c:459-472` |
| 其他一切 | 未知调用号 | 若是阻塞式调用则返回不支持（ENOSYS），否则返回沉默标记不回信 | `main.c:474-479` |

回信规则只有一条：除非处理结果是沉默标记（EDONTREPLY，数值 203，表示"这次不回信"），否则把结果码填入回复消息的消息类型字段发回去（`main.c:482-484`）。发信失败（`ipc_sendnb`）只打印不终止进程——**收不到消息则终止进程，发不出消息只记录日志**（`:445-446` 对比 `:485-486`）：收不到说明循环本身无法接收，无可挽回；发不出多半是对端已退出，不值得为此终止本服务。轻重分明。

### 1.4 一次系统控制请求进门要过的六道检查

`mib_sysctl`（`main.c:277-379`）是三类请求里最重的一类。把它想成安检六道关卡，每道不过即返回错误：

```
1. 调用方式检查：判断是否为阻塞式调用（SENDREC），若不是则直接返回不回复（EDONTREPLY），连名字都不看
  → 2. 名字长度检查：分量数必须满足 0 < 长度 ≤ 12，否则返回参数错误（EINVAL）
  → 3. 名字取用路径选择：长度 ≤8 则名字已在消息内直接拷贝，长度 >8 则需通过内核拷贝从用户地址取名字
  → 4. 旧数据区配对：若用户提供了旧数据缓冲区地址则打开该缓冲区，长度字段单独有值而地址为零则视为用户未初始化而宽容忽略
  → 5. 新数据区配对：新数据地址与长度必须同时非零才视为提供新数据，半份提供视为未提供（NetBSD 同款宽容规则）
  → 6. 收尾长度与错误码换算：若结果能装下则返回成功，若装不下则返回缓冲区太小（ENOMEM）但同时告知完整长度以便重试，若底层处理已返回错误则原样返回错误并附带已暂存的长度
```

最后这道收尾换算是全文最反直觉的一句，值得现在就记住：**此处返回的 ENOMEM 不是"内存分配失败"，而是"用户提供的接收缓冲区太小"**——数据已经算出来了，只是调用者给的槽装不下，内核能塞多少塞多少，同时把完整长度告诉调用者（"下次带个 64 字节的槽来"），再返回 ENOMEM。NetBSD 传下来的规矩（`main.c:355-360` 注释）。调用者拿 ENOMEM + 完整长度重试一次，第二次必中。把 ENOMEM 读成"分配失败"会在 08/09 里处处误判，先在这里钉住。

### 1.5 小结

启动时注册两类初始化回调（全新启动则构建一切，重启则只重建静态骨架而丢弃运行时动态节点），主循环分三步（接收消息、按消息类型分发、按规则决定是否回信），拒绝两类到达（内核通知直接跳过、未知调用号则根据是否为阻塞调用分别返回不支持或沉默），系统控制请求进门需过上述六道检查。记住这张表，后文 02 至 22 的每一篇都是往主循环分发与六道检查里填充具体内容。

---

## 2 C 源码分析

### 2.1 函数清单（本篇相关的 3 个函数 + 1 个初始化体占位）

| 函数 | 位置 | 输入 | 输出/副作用 | 说明 |
|------|------|------|-------------|------|
| `main` | `main.c:433-492` | 无 | 永不返回；循环收发 | 先调 `mib_startup`，然后死循环；`m_out` 每次清零后复用（`:456`） |
| `mib_startup` | `main.c:415-428` | 无 | 注册两个回调后 `sef_startup()` | 纯注册，不做业务初始化（真正的树初始化在 `mib_init`，见 04） |
| `mib_sysctl` | `main.c:277-379` | 收到的消息、IPC 状态、待填的回复消息 | 返回结果码；`m_out.oldlen` 填长度 | 六道判断 + 调 `mib_dispatch`（10）+ 收尾换算 |
| `mib_init`（占位） | `main.c:384-410` | SEF 类型（忽略） | 四棵子树 wiring + 全树初始化 + 远端表复位 | 本体在 04，本篇只认"两次注册挂的是它" |

`mib_table`（`:46-54`）与 `mib_root`（`:62`）的七顶层节点（kern/vm/net/hw/user/vendor/minix）只在本篇露个名字——它们的定义与初始化宏在 04，节点结构在 03。

### 2.2 分发表逐项核对（`main.c:458-479`）

| 调用号 | 值（`com.h:1022-1030`） | handler | 位置 |
|--------|------------------------|---------|------|
| `MIB_SYSCTL` | `0x1800`（`MIB_BASE+0`） | `mib_sysctl` | `:459-462` |
| `MIB_REGISTER` | `0x1801`（`MIB_BASE+1`） | `mib_register` | `:464-467` |
| `MIB_DEREGISTER` | `0x1802`（`MIB_BASE+2`） | `mib_deregister` | `:469-472` |

三封信、无死号：`NR_MIB_CALLS` 是 3（`com.h:1030`），三个全部分支覆盖。和 DS 的 `DS_SNAPSHOT`（有名无分支，死号）对照：MIB 的调用号空间是满的。

### 2.3 解码六步逐项核对（`mib_sysctl`，`main.c:291-377`）

| # | 步骤 | C 位置 | 行为 |
|---|------|--------|------|
| 1 | 调用形状 | `:292-293` | 非 `SENDREC` 直接 `EDONTREPLY`——连名字都不看 |
| 2 | 取字段 | `:295-300` | `endpt=m_source`；`oldaddr/oldlen/newaddr/newlen/namelen` 取自 `mess_lc_mib_sysctl`（`ipc.h:424-433`：`oldp/oldlen/newp/newlen/namelen/namep/name[8]`） |
| 3 | 长度校验 | `:302-303` | `namelen==0 \|\| namelen>CTL_MAXNAME(12，`sys/sys/sysctl.h:75`)` → `EINVAL` |
| 4 | 取名字 | `:309-315` | `>CTL_SHORTNAME(8，`ipc.h:15`)` 则 `sys_datacopy(namep→&name)`，失败回その错误码；否则 `memcpy` 信内 `name[8]` |
| 5 | 配对 old/new | `:322-340` | old：`oldaddr!=0` 才开（长度无地址被原谅）；new：地址长度双非零才开（半份丢弃，NetBSD 同款） |
| 6 | 组装 call 并收尾 | `:347-377` | `call_endpt/name/namelen/flags=0/reslen=0` → `mib_dispatch` → 收尾换算（见 2.4） |

### 2.4 收尾换算（`main.c:368-377`，NetBSD 语义）

| handler 结果 | 条件 | 回复码 | `m_out.oldlen` |
|-------------|------|--------|----------------|
| `r ≥ 0`（r = 完整长度） | 给了槽且装得下 | `OK` | r |
| `r ≥ 0` | 给了槽但装不下 | `ENOMEM`（部分字节已拷） | r（完整长度，caller 凭它重试） |
| `r ≥ 0` | 没给槽（`oldaddr==0`） | `OK` | r（只嫌长度，不抱怨） |
| `r < 0`（真错误） | — | r 原样 | `call_reslen`（handler 经 `mib_setoldlen` 暂存的，`:147-152`；今天只有建节点撞车 `EEXIST` 带出现有节点） |

### 2.5 boot 位置证据

MIB 在 boot 映像里排在 TTY 之后、VM 之前（`kernel/table.c:60` 登记 `{MIB_PROC_NR, "mib"}`），因为 `init(8)` 自身启动期间就要调 sysctl（`main.c:15-19` 注释：晚启动则 sysctl 无法正确实现，且服务需要超级用户权限）。kernel 启动时对除 VM 外的所有用户进程置 `RTS_VMINHIBIT`（`kernel/main.c:265-267`：VM 建好页表前谁也不许跑），VM 解除抑制后 MIB 才真正可被调度——这条抑制链在 `../01-stage-kernel/09-vm-boot-protocol.md`。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 调用号变枚举 | 裸 `int` 做 `switch`（`main.c:458`） | `MibCall` 枚举 + `from_raw` 回 `Option`（`dispatch.rs:43`） | 非法调用号不可表达：`None` 就是 `default` 分支；调用号空间满（无死号）由 `NR_MIB_CALLS=3` 测试钉住，加第四个号时编译器逼你加变体 |
| D2 | 到达分三类，探测器换成内核状态 | `if notify / switch / default` 散写，notify 用 `is_ipc_notify(status)` | `Incoming::{NotifyRefusal, Dispatch, Unknown}` + `triage(notify, mtype)`（`dispatch.rs:72,88`） | notify 拒绝和未知号拒绝原因是两回事（"没号可派" vs "没路可走"），两个名字让日志和测试能区分；`triage` 收 `bool` 而非自己算 status——MIB 听内核判决，不做 DS 式的号码推理（§1.3） |
| D3 | `default` 变纯函数 | `if SENDREC → ENOSYS else EDONTREPLY` 内联（`:475-478`） | `default_outcome(is_sendrec: bool) -> i32`（`dispatch.rs:105`） | DS 的 default 无条件 `EINVAL`，MIB 的要先问调用形状；一个名字 + 一个测试把"单向发送者没有等回信的槽位"钉住 |
| D4 | 回信规则变纯函数 | `if (r != EDONTREPLY) send` 内联（`:482`） | `should_reply(result: i32) -> bool`（`dispatch.rs:116`） | 与 DS 同款 verdict 层：规则一句话，值得一个名字；循环体以后长什么样都不影响它 |
| D5 | 解码六步只留 verdict，效果全部交出 | 一个函数里混着 `sys_datacopy`、handler 调用和纯判断 | `check_namelen` / `classify_name` / `pair_oldp` / `pair_newp` / `map_sysctl_reply` 五个纯函数（`dispatch.rs:126,153,175,197,217`）；长名字拷贝与 handler 调用移交 06/10 的 trait | C 把"判哪条路"和"走这条路"焊在一起；Rust 只判不走——判是纯逻辑可单测，走要等 IPC/拷贝落地；`classify_name` 对越界输入取 `Copy` 臂而不 panic：判错路最多多拷一次，trap 则全盘皆输 |
| D6 | 启动注册变类型，同体不同命显式化 | 两次 `sef_setcb_*` 挂同一 `mib_init`，差异只在注释（`:419-425`） | `MibInitKind::{Fresh, RestartLossy}`（`sef.rs:24`） | C 的差异藏在注释里，注释会撒谎，类型不会；`RestartLossy` 把"动态叶子全丢、调用者负责重建"写进名字，08/12 的重建义务从这里就能 grep 到 |
| D7 | 收尾换算变纯函数 | 四行 `if/else` 内联（`:368-377`） | `map_sysctl_reply(result, oldaddr, oldlen, reslen) -> (code, out_oldlen)`（`dispatch.rs:217`） | `ENOMEM` 在这里是"信封太小"不是"内存不够"（§1.4）；函数签名把"长度永远照给"变成类型保证：返回二元组里第二个分量无条件是完整长度 |

微内核对照（类比，非移植依据）：单线程收信循环是 MINIX/Redox/seL4 用户态服务器的共同默认形状，本篇的 `triage`/`should_reply` 对应"分类—执行—应答"三段里的第一段和第三段；Redox 的 scheme 命名空间按路径分发请求，MIB 按整数名字分发——都是"名字→处理者"的查表，只是 MIB 的名字是整数数组（`CTL_MAXNAME=12` 个 `int`），这是 NetBSD sysctl 遗产（02 细讲）。对照的结论只有一个：循环骨架不值得发明，保持三段式，把心智花在"六道门"上。

替代方案及否决：把 `main` 循环本身也搬进 Rust（`loop { receive; triage; reply }`）——否决，因为收发原语（`sef_receive_status` / `ipc_sendnb`）依赖 IPC 落地（A-12，`minix-sys` 还是 stub），现在搬只能搬个空壳；verdict 层是纯逻辑，先落地、可先测（与 DS 01 同款 verdict-first 策略）。

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/mib/src/
├── dispatch.rs   — 本篇：MibCall / Incoming / triage / default_outcome /
│                    should_reply / check_namelen / NamePath+classify_name /
│                    OldpPresence+pair_oldp / NewpPresence+pair_newp /
│                    map_sysctl_reply / CTL_MAXNAME / CTL_SHORTNAME
├── sef.rs        — 本篇：MibInitKind（两次注册的两种承诺）
├── main.rs       — 二进制入口（调库 init，占位，IPC 落地后接循环）
├── lib.rs        — 模块导出
└── （同 crate 另有 `tree/`、`io/`、`auth.rs`、`data/`、`query.rs`、
    `describe.rs`、`remote.rs`、`subtree/`、`proc/`——分属 02~22 各篇，
    本篇只拥有 `dispatch.rs` + `sef.rs` 两文件的 verdict 层）
```

调用号基址（`MIB_BASE=0x1800`、`MIB_SYSCTL/REGISTER/DEREGISTER`、`NR_MIB_CALLS=3`）住在 `os/libs/minix-types/src/types/com.rs`（路由半区，A-1 number-first：循环先编译，载荷类型 02 再补；与 DS 的 `DS_RQ_BASE` 块同例）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 三种调用 | `main.c:459-472` | `dispatch.rs:43`（`MibCall`） | `from_raw` 映射已知号，未知回 `None` |
| 到达三类 | `main.c:449-479` | `dispatch.rs:72`（`Incoming`）+ `triage` | notify→跳过，已知→办，未知→`default` |
| `default` 判决 | `main.c:474-479` | `dispatch.rs:105`（`default_outcome`） | SENDREC→`ENOSYS`，否则 `EDONTREPLY` |
| 回信规则 | `main.c:482-487` | `dispatch.rs:116`（`should_reply`） | 仅 `EDONTREPLY`（203）不回 |
| 长度校验 | `main.c:302-303` | `dispatch.rs:126`（`check_namelen`） | 越界→`EINVAL` |
| 取道 | `main.c:309-315` | `dispatch.rs:141,153`（`NamePath`/`classify_name`） | ≤8 信内，>8 内核拷（效果在 06） |
| 配对 | `main.c:322-340` | `dispatch.rs:167,175,189,197` | old 宽容、new 严格 |
| 收尾换算 | `main.c:368-377` | `dispatch.rs:217`（`map_sysctl_reply`） | 溢出→`ENOMEM`+完整长度；错误→原码+staged 长度 |
| 启动种类 | `main.c:419-425` | `sef.rs:24`（`MibInitKind`） | Fresh / RestartLossy（动态叶子不恢复） |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 未知号必拒 | `from_raw` 回 `None` → `Unknown` → `default_outcome` | `main.c:474-479` |
| notify 永不分发 | `triage` 先判 `notify`，连 `mtype` 都不看 | `main.c:449-454` |
| 非 EDONTREPLY 必回 | `should_reply` | `main.c:482-484` |
| 长度永远照给 | `map_sysctl_reply` 第二分量无条件是完整长度 | `main.c:369,376` |
| 重启只保骨架 | `RestartLossy` 名字 + 08/12 重建义务 | `main.c:420-424` |

### 4.4 与 C 六步的差异说明（模式 72 CSSCM）

C 的 `mib_sysctl` 是 6 步一气呵成（含两次效果：长名字拷贝 `:310-312`、handler 调用 `:353`）；Rust 本篇是 5 个 verdict 函数（判）+ 2 个效果移交（走）。差异分类全是**设计决策**（verdict-first：判先行、走等 IPC 落地），无已知缺口；C 无 bug 需要修正（`ENOMEM` 换算、C 的宽容/严格配对原样保留）。

---

## 5 测试要点

> 基线：`cargo test -p minix-mib --lib`（workspace 根在 `os/`），本篇 9 个测试；另 `cargo test -p minix-types --lib com::` 钉调用号。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_call_numbers_cover_all_three` | `com.h:1026-1028,1030` | 三号映射正确；`0x1803`（号外第一号）回 `None` | `dispatch.rs` |
| `test_triage_kernel_says_notify` | `main.c:449-479` | notify 压倒一切；三信分发；野号未知 | `dispatch.rs` |
| `test_default_outcome_asks_call_shape` | `main.c:474-479` | SENDREC→`ENOSYS`，否则 `EDONTREPLY` | `dispatch.rs` |
| `test_should_reply_only_silence_silences` | `main.c:482-487` | 仅 `EDONTREPLY` 不回（含 `ENOMEM` 必回） | `dispatch.rs` |
| `test_check_namelen_bounds` | `main.c:302-303` | 0 拒、1 过、12 过、13 拒 | `dispatch.rs` |
| `test_classify_name_short_long` | `main.c:309-315` | 8 信内、9 内核拷 | `dispatch.rs` |
| `test_pair_oldp_forgiving_newp_strict` | `main.c:322-340` | old 原谅 bare 长度；new 半份丢弃（三组合） | `dispatch.rs` |
| `test_map_sysctl_reply_overflow_becomes_enomem` | `main.c:368-377` | 装下 OK / 装不下 `ENOMEM`+全长 / 无槽 OK+全长 / 出错原码+staged | `dispatch.rs` |
| `test_init_kinds_name_the_promise` | `main.c:419-425` | 两种承诺互异 | `sef.rs` |

测试策略：边界值锁定（0/1/8/9/12/13、`0x1803`）；`ENOMEM` 四行表全覆盖——这是本篇最容易被后人"顺手简化"掉的语义；`0x42` 野号与 DS 同款（跨篇可对照）。

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-mib --lib`：**94 passed**（全 crate；其中本篇 9 个，见上表）
- `cargo test -p minix-types --lib`：**139 passed**（全 crate；`com::` 子集 7 passed，含 `test_mib_messages`）
- 本节上表列出与本篇直接相关的 9 个（子集；02~16 各篇的测试同住一个 crate，总数随阶段推进增长）

---

## 6 过渡

本篇讲的是 MIB 的"骨架"：信怎么进来（`sef_receive_status`）、走哪条路（三岔路）、回不回（`should_reply`）、sysctl 进门六道判断的 verdict。骨架有了，下一站是 02——三封信的信封里到底装了什么（调用号、六种消息结构、`sysctlnode`/`sysctldesc` 交换格式、版本与标志宏），也就是本篇故意没拆的信。

## 7 参见

- C 源：`minix3/minix/servers/mib/main.c:277-379,415-492`、`minix3/minix/include/minix/com.h:90-95,1022-1030`、`minix3/minix/include/minix/ipc.h:15,424-433,1548-1552`、`minix3/sys/sys/sysctl.h:75`、`minix3/minix/kernel/table.c:60`、`minix3/minix/kernel/main.c:265-267`
- 阶段文档：`02-mib-message-contract.md`（下一站，协议面）、`04-mib-static-tree-init.md`（`mib_init` 本体）、`06-mib-copy-io.md`（拷贝效果）、`10-mib-dispatch.md`（handler 调用）、`../07-stage-ds/01-ds-init-main.md`（同形状先例：七封信 vs 三封信、猜 notify vs 听 notify、保资产 vs 保骨架）
- Rust 实现：`os/servers/mib/src/dispatch.rs`、`os/servers/mib/src/sef.rs`、`os/libs/minix-types/src/types/com.rs`（MIB 号段）
- 对端：`../01-stage-kernel/09-vm-boot-protocol.md`（抑制解除链）、`../01-stage-kernel/12-ipc-core.md`（收发原语，落地后接循环）
