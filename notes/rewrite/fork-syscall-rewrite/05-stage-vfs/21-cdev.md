# 21 — cdev：终端改道、开合对话、挂起读写与取消换码

本文讲清字符设备层如何在“改道→查表→开合→读写→取消→回复”的六段对话中，以控制终端为改道开关，以访问位为开合语言，以挂起为读写常态，以换码为取消与复活的翻译——块设备谈完就走，字符设备谈到一半会睡着，睡着了还可能被信号叫醒。

前置阅读：`19-device-map.md`（dmap 查表语义与恢复 verdict）、`15-open-close.md`（`FileType` 三路分派）、`02-fproc-struct.md`（`fp_tty/fp_flags` 归属）。

> 本章不讲什么：
> - `asynsend3` 的传输执行—— 内核侧（本篇只给发送顺序）
> - `worker_wait/suspend/revive` 的调度执行—— `08-worker-thread.md` / `09-main-loop.md`
> - `req_newnode` 的 PFS 落子执行—— `12-request-wrappers.md`
> - `select_cdev_reply1/2` 的多路复用执行—— `23-select.md`
> - 授权（grant）的内核侧实现—— `99-global-concepts.md`（本篇只给方向）
> - 终端行规程与会话语义—— TTY 驱动侧（本篇只给归属判定）

---

## 1 概念

### 1.1 为什么字符读写会睡着

块设备的对话是同步往返（发→等→回，线程钉死），字符设备的对话是挂起等待（发→睡→被叫醒，线程让出）。分叉的根因是设备的时间观：块设备承诺“一定回”（盘不转也报错），字符设备承诺“有了再叫”（终端没输入就等着）。`cdev.c:1-9` 的头注释把分叉写在第一段：读写与选择可挂起，开与关不可——开关注的是建连（对端必须在），读写关注的是数据（数据可以等）。

挂起的代价是取消：睡着的进程可能被信号打断，打断要通知驱动（“别等了，人走了”），通知本身又要等驱动确认。取消因此是“为等待而等待”的二阶等待——本篇最绕的逻辑全在这里。

### 1.2 终端改道：魔术设备的替身

`/dev/tty` 不是设备，是代词——指“本进程的控制终端”。打开它时 VFS 先查本进程有没有控制终端：有则换成真设备号继续，无则拒绝。改道幂等（调两次结果一样）但“不这么用”——调用点只调一次，幂等是性质不是用法。

改道的 Nemesis 是越界：换出来的号仍要验 major 范围。信任但验证——代词解析完了，仍要过正常的表查询，不走后门。

### 1.3 克隆：打开成功换号的替身术

有些设备打开成功会换号：驱动回一个带 `CLONED` 位的新 minor，VFS 把 fd 指向的 vnode 换成新号对应的节点（PFS 上现落一个临时节点）。替身术的本质是“对象在打开瞬间才诞生”——打开前只有类（主设备号），打开后才有实例（次设备号）。伪终端（pty）是典型：打开 `/dev/ptmx` 分得一个专属 pty，之后读写走新号，旧号只是引子。

换号失败要关新号（`cdev_clone` 失败调 `cdev_close`）——替身没立住，本尊的打开也不算数。旧结换新（解锁旧 vnode、引用新 vnode）是引用计数的常规操作，归 05 的管辖。

### 1.4 控制终端的归属三条件

打开字符设备可能顺手认主：驱动回 `CTTY` 位，本设备就成了进程的控制终端。但认主有三道门：会话首领、无主状态、本次没说不（`O_NOCTTY` 缺席且无人先占）。三条件是“或”关系——任一不满足即强制 `NOCTTY`：“不是头儿不认，已有主不认，说了不认不认，名花有主不认”。

`dmap_seen_tty` 是性能短路：没驱动授过终端，就跳过全表扫描。短路的正确性在于单调——一旦有过授终端，标志永真，扫描永不跳过；没授过，扫描必空，跳过无害。

### 1.5 授权方向的再交叉

19 已讲过 ioctl 的交叉（读出配写），此处读写同理：读配 `CPF_WRITE`（授权驱动写用户缓冲），写配 `CPF_READ`（授权驱动读用户缓冲）。同一交叉出现两次不是重复，是同一知识在两个调用点的各自表达——19 管 ioctl，本篇管读写，各自建模，语义同源。

### 1.6 取消与复活的换码

取消把 `EAGAIN` 换成 `EINTR`，复活把 `EINTR` 换成 `EAGAIN`——一去一回，恰好互逆。换码的根因是两拨人用词不同：驱动说“没数据”（`EAGAIN`），进程侧说“被打断”（`EINTR`）；去程（取消）站在进程侧翻译，回程（复活）站在驱动侧翻译。`cdev.c:470` 的 TODO 诚实标注“或已过时”——换码可能是历史包袱，但包袱在消除前仍是契约，契约必须建模。

### 1.7 与其他 OS 的字符设备对照

- **Linux** 以 `tty_ldisc`（线路规程）+ `cdev`（字符设备开关表）+ `file_operations` 三层实现同构对话：`cdev_map` 的改道对应 `tty_open` 的 `/dev/tty→real tty` 重定向，克隆对应 pty 的 `ptmx→pts` 配对分配，`NOCTTY` 三条件对应 `tty_init_termios` 的会话检查。
- **Redox** 的终端 scheme 以 `TermScheme` 状态机（前台进程组 + 会话）对应控制终端归属；`O_NOCTTY` 语义一致；Redox 以 `EventQueue` 挂起读写对应 `suspend`，以方案取消包对应 `cdev_cancel`。
- **seL4** 无字符抽象，串口是持有 MMIO 能力的用户驱动，直达即 IPC；VFS 的改道/克隆/归属三知识在 seL4 中由各驱动客户端自理——集中建模 vs 分散自理，仍是状态服务器与无状态内核的分水岭。

### 1.8 小结

字符对话是六段（改道→查表→开合→读写→取消→回复），三组开关（有无终端/查表三合一/三验全过）决定走向，一对换码（去程 `EAGAIN→EINTR`、回程反之）翻译两拨人的用词。记住“改道替身、开合建连、读写可睡、取消换码”，就记住了本篇全部。

---

## 2 C 源码分析

### 2.1 `cdev_map` 改道机（`cdev.c:35-56`）

CTTY 整除改道（`44-51`：无终端 `NO_DEV`，有则代入重取 major）→ 越界 `NO_DEV`（`53`）→ 原样返回（`55`）。幂等声明在注释（`33`），调用点单次使用。

### 2.2 `cdev_get` 查表机（`cdev.c:62-90`）

改道（`72-73`，`NO_DEV` 即空）→ 查 dmap 行（`76`）→ 驱动存活（`79`，`NONE` 即空）→ 端点复核（`81-85`，坏端点打印即空）→ 回写 minor（`88`）返行（`89`）。

### 2.3 `cdev_clone` 替身机（`cdev.c:96-139`）

fd 有效断言（`103`）→ 新号拼合（`106`，`makedev` 见 18）→ PFS 落子（`109-110`，`RWX_MODES|I_CHAR_SPECIAL` 见 `minix3/minix/include/minix/const.h:110,116`，失败关新号 `112-114`）→ 取空 vnode（`117-121`，失败放子关号，`118` 的“is this right?”诚实保留）→ 锁新结（`122`）→ 解旧引用旧（`124-126`，fd 非空断言）→ 填新结九字段（`128-135`：端点/空挂载/`NO_DEV`/inode/模式/新号/双计数 1）→ 换指向（`136`）。

### 2.4 `cdev_opcl` 开合机（`cdev.c:148-248`）

双断言（`164-165`：操作二值、开必有 fd）→ 查表门（`168-169`，失败 `ENXIO`）→ CTTY 短路（`177`，不扰驱动——setsid 后开 tty 不留悬丝）→ NOCTTY 三条件（`185-191`）→ 组包（`194-205`：清零、操作号（`CDEV_OPEN/CDEV_CLOSE` 见 `com.h:926-927`）、minor、id=who、开则拼访问位 `R/W→CDEV_R_BIT/CDEV_W_BIT` 见 `minix3/minix/include/minix/com.h:940-941`、`NOCTTY→CDEV_NOCTTY` 见 `com.h:942`）→ 异步发（`208-209`，`AMF_NOREPLY` 见 `minix3/minix/include/minix/ipc.h:2760`，失败 panic）→ 挂线程等复（`212-218`）→ 取状态（`221`）→ 开成功解效应（`223-244`：`CDEV_CLONED` 换号 `231-235`，`CDEV_CTTY` 授终端 `238-241`，掩码见 `com.h:955-956`）→ 回结果（`247`）。

### 2.5 `cdev_open/close` 薄包装（`cdev.c:253-268`）

开传 fd 与 flags（`257`），关传 -1 与 0（`267`，注释写明关可无 fd：关的是设备不是描述符）。

### 2.6 `cdev_io` 读写机（`cdev.c:279-342`）

三断言（`289`：`CDEV_READ/CDEV_WRITE/CDEV_IOCTL` 三值，见 `com.h:928-930`）→ 查表门（`292-293`，失败 `EIO`——与开合的 `ENXIO` 不同：读写时表丢是运行时事故）→ `TIOCSCTTY` 授终端（`300-303`，`TTY_MAJOR=4/PTY_MAJOR=9` 见 `minix3/minix/include/minix/dmap.h:25,30`，FIXME 硬编码诚实保留）→ 授权（`306-312`：读写交叉配向，控走 19 解码；无效 panic）→ 组包（`315-329`：操作号、minor、控则 request+user、读写则 pos+count、id=proc、grant、flags、`O_NONBLOCK→CDEV_NONBLOCK` 见 `com.h:946`）→ 异步发（`332-333`，失败 panic）→ 登记挂起（`336-339`：设备/端点/授权三存，授权备撤销）→ `SUSPEND`（`341`）。

### 2.7 `cdev_select` 旁路机（`cdev.c:349-375`）

三断言（`358-361`：非空、有界、非 CTTY）→ 直查 dmap 行（`362`，**无改道**——调用者已改道且 `fp` 可能错位，`346-347` 注释）→ 组包（`365-368`：`CDEV_SELECT` 见 `com.h:932`、minor、ops）→ 异步发（`371-372`，失败 panic）→ `OK`（`374`，发送即成功，回复走 23）。

### 2.8 `cdev_cancel` 取消机（`cdev.c:380-419`）

查表门（`389-390`，失败 `EIO`）→ 组包（`393-396`：`CDEV_CANCEL` 见 `com.h:931`、minor、id=端点）→ 异步发（`399-400`，失败 panic）→ 挂线程等复（`403-409`）→ 有权即撤销（`412-413`）→ 取状态（`416`）→ `EAGAIN` 换 `EINTR`（`418`，注释“注意错误码”，换码语义见 §1.6）。

### 2.9 `cdev_reply` 三路机（`cdev.c:428-508`）

知名门（`484-488`，未知打印丢弃）→ 三路分发（`490-507`：通用 `CDEV_REPLY` 见 `com.h:935`、选择两回复 `SEL1/SEL2` 见 `com.h:936-937` 归 23、未知打印）。通用五路（`438-474`）：`SUSPEND` 丢弃（`438-442`）→ 坏端点丢弃（`444-448`）→ 工人投递（`451-455`，未阻塞断言）→ 协议错打印（`456-463`）→ 复活（`464-474`，`EINTR→EAGAIN` 换码，TODO 过时注记诚实保留）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `cdev.c` 的挂起循环，而是吸收 Linux/Redox 的终端模型后做取舍。以下决策对应 `.design/21-design.v1.md` D1-D7。

### D1 改道纯函数

- **C**：`cdev_map` 内嵌 `fp_tty` 读取与越界门（`cdev.c:35-56`）。
- **Rust**：`tty_redirect(dev, is_ctty, tty: Option<u64>, major_valid)` + `TtySource` trait（`FixedTty` 有答 vs `NoTty` 无答）+ `tty_redirect_for` 泛型包装（`os/servers/vfs/src/cdev.rs:84,40,99`）。
- **为什么**：三值知识纯函数化；`None` 即 `NO_DEV`。调用点传入终端，函数不碰 proc 表。

### D2 查表门收敛

- **C**：`cdev_get` 四步直线（`cdev.c:62-90`）。
- **Rust**：`resolve_gate(mapped, driver, endpoint_ok) -> Option<GatePass{driver, minor}>`（`os/servers/vfs/src/cdev.rs:123`）。
- **为什么**：三合一失败皆空（调用点 open 报 `ENXIO`、io 报 `EIO`——差异在调用点）；收敛后门只有过/不过一比特。

### D3 访问位拼合

- **C**：`R/W→CDEV_R/W_BIT` 与 `NOCTTY→CDEV_NOCTTY` 散在 opcl（`cdev.c:199-202`）。
- **Rust**：`access_bits(read, write, noctty) -> u8`（`os/servers/vfs/src/cdev.rs:133`）。
- **为什么**：位运算纯知识；三位一次拼合。位值 sync 树可验证，不编造。

### D4 NOCTTY 三条件

- **C**：三或 + 全表扫描（`cdev.c:185-191`）。
- **Rust**：`noctty_force(is_leader, has_tty, requested, seen_elsewhere) -> bool`（`os/servers/vfs/src/cdev.rs:152`）。
- **为什么**：扫描是调用点的事；规则是四元或式。`seen_tty` 短路以注释声明，不入判定。

### D5 打开后效应

- **C**：`CLONED` 换号 + `CTTY` 授终端（`cdev.c:231-241`）。
- **Rust**：`OpenEffects{clone_minor, grant_tty}` + `open_effects(status, dev)` 掩码解码（`os/servers/vfs/src/cdev.rs:164,177`）。
- **为什么**：状态字位包一次解码；克隆落子归 12，授终端归 02，调用点分办。

### D6 授权方向复用语义

- **C**：读配 `CPF_WRITE`、写配 `CPF_READ`（`cdev.c:306-308`）。
- **Rust**：`grant_dir(is_read) -> u32`（`os/servers/vfs/src/cdev.rs:193`），语义复用 19 交叉。
- **为什么**：同一知识不定义两遍；ioctl 走 19 解码，读写走本函数（方向二值更简单）。

### D7 回复分类与双向换码

- **C**：通用五路（`cdev.c:438-474`）+ 去程换码（`418`）+ 回程换码（`473`）。
- **Rust**：`ReplyClass` 五值 + `classify_reply` + `cancel_map`/`revive_map` 双向换码（`os/servers/vfs/src/cdev.rs:203,225,249`）。
- **为什么**：丢/投/活三类知识纯函数化；换码配对显式；TODO 过时注记诚实保留为文档注记。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | 挂起/等待皆 verdict，执行留 08/09 | `cdev.rs:247,260` + 本文档 D7 + 21 正文 §1.1 |
| 授权执行归内核（grant 只解码） | 方向/尺寸纯函数，执行留 minix-sys | `cdev.rs:193` + 本文档 D6 + 21 正文 §1.5 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── cdev.rs               — 本篇：改道/查表/拼合/效应/回复判定
├── device_map.rs         — CTTY_MAJOR + CPF_* 复用（19，不重复常量）
├── mount.rs              — NO_DEV 语义对照（18，哨兵即 None）
└── open.rs               — FileType 三路对照（15，分派同源）
```

> 设计决策：§3 D1（改道纯函数）/ D5（打开后效应）/ D7（回复分类）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `cdev_map` | `cdev.c:35-56` | `cdev.rs:84,40,99` | 改道 + trait 双实现 |
| `cdev_get` | `cdev.c:62-90` | `cdev.rs:123 resolve_gate` | 三合一门 |
| 访问位拼合 | `cdev.c:199-202` | `cdev.rs:133 access_bits` | 三位一次 |
| NOCTTY 规则 | `cdev.c:185-191` | `cdev.rs:152 noctty_force` | 四元或式 |
| 打开效应 | `cdev.c:231-241` | `cdev.rs:164,177` | 掩码解码 |
| 授权方向 | `cdev.c:306-308` | `cdev.rs:193 grant_dir` | 交叉复用 |
| 回复分类 | `cdev.c:438-474` | `cdev.rs:203,225` | 五值 |
| 换码对 | `cdev.c:418,473` | `cdev.rs:247 + revive 内` | 双向互逆 |
| 选择旁路 | `cdev.c:346-361` | `cdev.rs:258 select_bypass` | 无改道断言 |
| 错误族 | `cdev.c` 全文件 | `cdev.rs:269,286 CdevError::to_errno` | 5 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 改道幂等 | 纯函数无状态 | 调两次同果 | `cdev.c:33` |
| 查表三合一 | `resolve_gate` | 单门过不过 | `cdev.c:62-90` |
| 换码配对 | 去回互逆测试 | `cancel/revive` 对测 | `cdev.c:418,473` |
| 克隆掩码正交 | `&~(CLONED\|CTTY)` | 号与位分离 | `cdev.c:232` |
| 选择无改道 | `select_bypass` | CTTY 拒绝 | `cdev.c:346-361` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **264 passed / 0 failed**（既有 256 + 本篇新增 8；`minix-types` 独立）。
> 本章直接影响 8 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_tty_redirect_matrix` | `cdev.c:35-56` | 改道三值 + trait 双实现 | `cdev.rs:301` |
| `test_resolve_gate` | `cdev.c:62-90` | 三合一门四样本 | `cdev.rs:325` |
| `test_access_bits_combination` | `cdev.c:185-202` | 拼合 + NOCTTY 真值表 | `cdev.rs:337` |
| `test_open_effects_decode` | `cdev.c:231-241` | 掩码解码四样本 | `cdev.rs:352` |
| `test_grant_direction_cross` | `cdev.c:306-308` | 交叉复用 | `cdev.rs:371` |
| `test_reply_classes` | `cdev.c:418,438-474` | 五路 + 双向换码 | `cdev.rs:378` |
| `test_select_bypass_rule` | `cdev.c:346-361` | 旁路断言 | `cdev.rs:413` |
| `test_errno_map_covers_cdev_c` | `cdev.c` 全文件 | 5 变体→errno 全映射 | `cdev.rs:421` |

测试策略：改道以三值 + trait 双实现覆盖；查表以合取四样本覆盖；拼合以位组合 + 真值表覆盖；效应以掩码四样本覆盖；回复以级联优先序 + 换码对覆盖；错误以 5 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**264 passed / 0 failed**
- 本节列出与本模块直接相关的 8 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/cdev.rs`

---

## 6 过渡

本篇在 19（查到人）之后、22（套接字对话）之前，是“字符对话”的归属层：19 只管查到驱动，本篇管查到之后怎么谈（改道/开合/读写/取消/回复）；没有本篇，终端只是个号码，打不开也读不出。

```
19-device-map: get_dmap_by_major → 驱动端点（查到谁）
   │
   └─► 本篇：tty 改道 → resolve 查表 → opcl 开合 → io 读写 → cancel 取消 → reply 分类
          │                      │                   │
          ├─► 23-select：cdev_select 发送与 SEL 回复的执行
          ├─► 09-main-loop：IS_CDEV_RS 分流与 CDEV worker 唤醒
          └─► 22-sdev：套接字对话（同门的另一支，驱动皆 suspend 模型）
```

阅读顺序提示：若关心“select 如何旁路改道”，下一站 `23-select.md`（`select_cdev_reply1/2` 的执行）；若关心“套接字的同门对话”，下一站 `22-sdev.md`（`sdev_*` 全族）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/cdev.c:1-508`（`cdev_map/cdev_get/cdev_clone/cdev_opcl/cdev_open/cdev_close/cdev_io/cdev_select/cdev_cancel/cdev_generic_reply/cdev_reply`）、`minix3/minix/include/minix/com.h:919-956`（`CDEV_*` 操作码与标志位）、`minix3/minix/include/minix/dmap.h:25,30`（`TTY_MAJOR/PTY_MAJOR`）、`minix3/minix/servers/vfs/fproc.h:95`（`FP_SESLDR`）、`minix3/minix/include/minix/const.h:110,116`（`I_CHAR_SPECIAL/RWX_MODES`）、`minix3/sys/sys/fcntl.h:104`（`O_NOCTTY`）
- 阶段文档：`19-device-map.md`（查表与恢复 verdict）、`15-open-close.md`（`FileType` 三路）、`02-fproc-struct.md`（`fp_tty` 归属）、`23-select.md`（选择执行）、`09-main-loop.md`（回复分流）
- Rust 实现：`os/servers/vfs/src/cdev.rs:1`（本篇判定层）、`os/servers/vfs/src/device_map.rs:1`（查表层）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（用户缓冲授权语义）
