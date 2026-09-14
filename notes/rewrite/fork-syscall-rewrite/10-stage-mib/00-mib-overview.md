# 00-mib-overview: MIB 整体架构概览

> **状态**: reviewed（2026-09-15，随 todo.md P3-1 成文）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/mib/`（8 个 .c，4990 行）+ boot 证据（`kernel/table.c:58-62`、`kernel/main.c:265-267`）
> **Rust 模块**: `os/servers/mib/` 全部
> **draft 素材**: `draft/README.md`（占位）

## 1 概念：为什么需要一个"管着一切却几乎不拥有数据"的服务

每个操作系统都会被问同一类问题：现在有几个进程在跑、内存还剩多少、内核时钟每秒跳几拍、这个驱动允许谁打开它。答案分散在系统各处——进程表在内核和 PM，内存统计在 VM，驱动策略在各自的服务里。问题在于，提问的人不应该知道答案住在哪：`ps(1)` 不该为了画一行表格去跟四个服务分别握手，`sysctl(8)` 更不该为每个子系统学一套私话。

Minix3 的答案是把"提问"统一成一个协议、把"回答"组织成一棵树。这棵树就是 MIB（Management Information Base）：节点按名字分层（`hw.ncpu`、`kern.boottime`），每个节点声明自己的类型（整数、字符串、结构……）和访问规则（谁可读、谁可写），数据本身可以就地存放（立即值）、可以指向服务内部的变量、也可以由一个函数现场计算。这套信息模型直接取自 NetBSD（`SYSCTL_VERSION` 钉在 VERS_1，`sys/sys/sysctl.h:1382-1450` 的 `sysctlnode`/`sysctldesc` 交换格式原样沿用），用户态的 `sysctl(3)`、`ps(1)`、`top(1)` 不需要知道底下是 NetBSD 还是 Minix。

有意思的是 Minix3 把这棵树的"根服务器"放在了用户态。它不拥有那些数据——真正拥有数据的是 kernel、PM、VFS、VM 和各个驱动。MIB 做三件事：**自己能答的照直答**（静态子树里的立即值和函数节点）、**别人答的把问题转发过去**（远程子树，见 §4）、**谁都不答的诚实回错**（`kern.c` 的 40+ 个 "not yet supported" 槽位，A-9 排除契约）。这个"路由器而非仓库"的定位决定了它的两个结构性特征：它需要超级用户权限（要替提问者去查别的服务的特权信息，`plan.md §1.1`），它也是全系统唯一必须理解"子树其实住在另一个进程里"这件事的服务。

## 2 主线：MIB 的一生（启动时序）

MIB 在 boot 镜像里登记得很早——`kernel/table.c:60` 的 `{MIB_PROC_NR, "mib"}` 紧跟 TTY 之后、先于 VM。排这么早不是随意：`init(8)` 在自己的启动过程中就要调用 `sysctl(2)`（`init` 的 `main.c:9-16` 注释），树若不在，init 的第一步就得失败。启动是一条严格的直线：

```
boot_image 登记（table.c:60：MIB_PROC_NR=7）
  → kernel/main.c:265-267 对全部用户进程置 RTS_VMINHIBIT（VM 建页表前禁止调度）
  → VM 建页表、解除抑制，MIB 第一次获得被调度的资格
  ▼
main()（main.c:433）
  ├─ mib_startup()（:415-431）
  │    ├─ sef_setcb_init_fresh(mib_init)      ← 冷启动：建七棵顶层子树
  │    └─ sef_setcb_init_restart(mib_init)    ← 重启：动态节点全丢，静态树重建（"只有静态树也比不跑强"，:415 注释）
  ▼
mib_init()（main.c:384-413）
  ├─ mib_kern_init / mib_vm_init / mib_hw_init / mib_minix_init   ← 四棵子树接线（13~15 篇）
  ├─ mib_tree_init()（tree.c:1476-1536）                          ← 全树递归：父链、计数、版本
  └─ mib_remote_init()（remote.c:40-49）                          ← 远程端点表清零
  ▼
主循环（main.c:443-488）
  sef_receive_status(ANY) → notify 一律拒绝
  → MIB_SYSCTL（→ mib_sysctl，六道解码门 → mib_dispatch）
  → MIB_REGISTER / MIB_DEREGISTER（远程子树挂载/卸载）
  → 野号码：SENDREC 来的回 ENOSYS，其余静默（EDONTREPLY）
```

Rust 侧的对应物：`os/servers/mib/src/dispatch.rs` 已把三信分派、两道拒绝、回信规则全部写成可独立测试的判定函数；循环本体与传输接缝的落地条目见 `todo.md` P1-1。

## 3 次主线：一次 sysctl(2) 调用的旅程

MIB 的全部日常工作，本质是把一个名字（如 `{CTL_KERN, KERN_BOOTTIME}`）解析成一份数据。把这条路径走一遍，22 篇文档就在正确的位置各就各位：

```
用户态 sysctl(3)/__sysctl（libc，不重写——A-10 外部契约）
  ├─ CTL_USER 子树 libc 本地处理（sysctl.c:80-86），其余进 MIB
  └─ _syscall(MIB_PROC_NR, MIB_SYSCTL)
  ▼ MIB 主循环 mib_sysctl()（main.c:277-383）
  ├─ 六道解码门：namelen 界（0 < n ≤ 12）→ 名字取道（≤8 内嵌 / >8 一次 sys_datacopy）
  │   → oldp/newp 配对（旧数据宽宏、新数据严格）→ 错误映射（01 篇）
  └─ mib_dispatch(&call, oldp, newp)（tree.c:1332-1475，10 篇）
       ├─ 逐层解析：静态数组 O(1) 一跳 + 动态有序链表早停（05 篇）
       ├─ 末位负数 → 元标识符：QUERY / CREATE / DESTROY / DESCRIBE（08/11 篇）
       ├─ 每层先问可见性（PRIVATE 滤除，07 篇）再问形状（叶子/函数/远程，10 篇）
       ├─ REMOTE 节点 → 三 grant 转发给拥有者（12 篇）
       └─ 常规叶子 → mib_readwrite：读旧写新、verify 回调、bool 消毒（09 篇）
  ▼ 回程
  ├─ 成功但装不下：部分拷贝 + 报完整长度 + ENOMEM（main.c:341-356——调用方据此知道该带多大的缓冲重试）
  └─ create 撞名：回 EEXIST 且携带现有节点内容（call_reslen 通道，tree.c:652-664）
```

这条旅程上有一个值得驻足的细节：**ENOMEM 在 sysctl 协议里不是纯粹的失败**。它携带"数据有多大"的信息回来，调用方凭这一点完成两次调用式的事务。Rust 侧把这个语义建模为错误通道上的附带长度（`dispatch.rs:217 map_sysctl_reply` 的 `error_reslen`），A-11 契约。

## 4 第二条次主线：一棵"借来的"子树

`kern.ipc`、`net.inet`、`net.local` 这些子树的数据不归 MIB 管——它们是 IPC、LWIP、UDS 服务在启动时"挂"上来的。挂载协议（`COMMON_MIB_*`）走的是一条反向的路：服务 → MIB 声明"这个名字空间归我"（`rmib_register`），此后用户查询命中挂载点时，MIB 把请求原样转发、把应答原样带回；服务死亡则摘下子树、能本地续走就续走（ERESTART 语义，`tree.c:1410-1416`）。这条旅程（注册 → 挂载 → 转发 → 死亡恢复）是 12 篇的主线，客户端半（`libsys/rmib.c`）在 22 篇，跨 stage 的落地依赖登记在 `../edge_todo.md` E-RMIBWIRE。

## 5 文档导航

| 阶段 | 文档 | 一句话 |
|------|------|--------|
| 协议与启动 | 01（主循环与解码）、02（消息契约） | 信怎么来、怎么拆 |
| 树的骨架 | 03（节点模型）、04（静态树）、05（查找） | 树长什么样、怎么找 |
| 原语 | 06（拷贝与 relay）、07（权限） | 字节怎么动、谁许可 |
| 树的生长 | 08（动态节点）、09（数据读写）、10（分发）、11（枚举与描述） | 一次查询的完整机制 |
| 边界 | 12（远程子树） | 别人的子树怎么管 |
| 数据源 | 13~15（kern/vm+hw/minix 子树）、16~20（进程信息五篇） | 每类数据的来历 |
| 客户端 | 21（libc sysctl，外部契约）、22（RMIB 客户端） | 提问方与挂载方 |
| 收口 | 99（全局概念） | 常量、错误码、跨服务引用 |

阅读顺序即编号顺序：协议面（02）先于一切 handler，树模型（03~05）先于动态节点（08），分发（10）先于子系统（13~20）。全局常量随用随查 99，不必通读。

## 6 设计原则

这批文档与 Rust 实现共用四条纪律，后续所有篇章沿用：

1. **位置可回答性**——每篇都能回答"它在 mib_init 或主循环的哪一步"（继承 `01-stage-kernel/00-kernel-overview.md §3`）。
2. **verdict/执行分离**——判定函数（纯函数，逐条对照 C 行号）先行落地并测试；执行半（传输、竞技场、主循环）是独立的后续任务，缺口显式声明而非含糊带过。这是本 stage 的现状基调，也是 `todo.md` 的组织依据。
3. **禁止前向引用**——依赖机制前置；编号顺序即依赖顺序。
4. **ARCH 三处一致**——凡偏离 C 的结构决策（节点 enum 化、错误通道类型化、单线程模型），在 C 对照点、文档、代码注释三处标注 `[ARCH: ...]`。

## 7 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（01~22、99）；跨 stage 的对端工作（kernel/PM/VFS producer、RMIB 客户端协议半、联调包）登记于 `../edge_todo.md`

## 8 参见

- `plan.md` — 文档集的覆盖契约（§5 函数清单）与 ARCH 决策（§4 A-1~A-12）
- `todo.md` — Rust 实现的缺口清单与推进顺序（2026-09-15 首轮架构审查）
- `99-mib-global-concepts.md` — 常量与错误码总表
- `../edge_todo.md` — 跨 stage 条目唯一入口
- `minix3/minix/servers/mib/` — C ground truth
