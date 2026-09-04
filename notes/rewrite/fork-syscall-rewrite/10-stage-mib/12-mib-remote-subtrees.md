# 12 — 系统信息库远端子树：跨服务的子树挂载与请求转交

> **分类**: 跨服务挂载 / 远程转交
> **源码**: `minix3/minix/servers/mib/remote.c`（全文 477 行）、`minix3/minix/servers/mib/tree.c:1543-1560`（`mib_mount` 头）、`:1560-1598`（参数门）、`:1600-1647`（路径 walk）、`:1649-1780`（目标挂载）、`:1789-1842`（`mib_unmount`）
> **说明**: 远端服务如何把自己的子树挂载到本服务树中、请求如何通过授权凭证转交给远端、远端服务异常退出后如何清理。本篇含远端子树次主线的完整路径图（plan §1.3），也是单向消息约束（请求方不等待回信）行为的主篇。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要实现 IPC/LWIP/UDS 挂载端（22）与读转交的人。前置知识：02（信封/共享号/单向约束）、03（四格矩阵/远端包）、10（续走 verdict）。"grant"见 06。

### 1.2 本章不讲什么

- 转交 verdict（续走三去向）——那是 10 的事（本篇只讲转交**执行**：grant、发信、清场、回信检查）。
- 客户端怎么写注册信——那是 22 的事。
- 分发怎么调到转交——10。

### 1.3 挂载即"认领 + 盖章"

远程子树解决"代码归谁"：IPC 的 `kern.ipc` 逻辑归 IPC 服务，但名字要挂在 MIB 树下。挂载分两步：**认领**（服务发 `MIB_REGISTER`：我是谁 label、根 id、标志、窗口、路径）→ **盖章**（MIB walk 路径：父亲们必须个个是"真本地非私有结点"，终点或盖住旧结点（obscuring），或现造临时结点（temp，名字/描述向服务现问 `COMMON_MIB_INFO`）→ 置 `REMOTE` + 填端点/窗口/根 id → 记入端点表链表）。盖的是"转交章"：以后凡到此结点的请求，MIB 不再看本地（本地被盖住），一律重授 grant 转交服务。

远程子树次主线路径图（plan §1.3，本篇为家）：

```
服务启动 → rmib_register（libsys/rmib.c）                    ← 22
  ├─ asynsend3(MIB_PROC_NR, MIB_REGISTER)（单向！）          ← 02 信封
  ▼ MIB mib_register → mib_do_register                       ← 本篇 §2.2
  ├─ SENDREC 来 → ENOSYS（防交叉死锁，02 M-8）
  ├─ DS label 校验（mib_get_label；无 label 不是服务）       ← DS 08
  ├─ 路径长度门（>8 静默丢）
  ├─ 端点表定位：同端点复用 / 同 label 异端点→先 mib_down 旧的 / 首空槽 / 满则丢
  ├─ 同端点同根 id → 拒（不搞双挂载）
  └─ mib_mount：路径 walk → 挂载点（盖住旧结点 / 现造临时）
  │    ├─ 顶层禁挂（miblen<2 → EPERM：kern 整体不可被接管）
  │    ├─ 标志窗/孩子窗检查
  │    ├─ 父亲们：真本地非私有（防服务拦截特权写）
  │    ├─ 旧结点：标志精确匹配 + 无动态孩子（EBUSY）
  │    └─ 临时结点：COMMON_MIB_INFO 问名/述 → 建 → 链入
  ▼ 用户 sysctl 命中挂载点
  ├─ mib_remote_call（三 grant + 版本快照 + root 标志）      ← 本篇 §2.4
  ├─ 发信失败（IPC 错）→ mib_down 清该端点全部挂载点 + ERESTART 续走本地
  └─ 服务回 ERESTART → mib_do_deregister（交叉时服务请辞）
  ▼ 服务退出/重启 → rmib_deregister / MIB_DEREGISTER → 摘链 → mib_unmount
       ├─ 遮蔽点：去 REMOTE，静态窗口重数，版本 bump
       └─ 临时点：mib_remove 释放（08 差量）
```

### 1.4 死亡是常态，设计按死亡来

remote.c 头注释（`:6-22`）开宗明义：**没有主动的服务死亡通知**（DS 缺订阅原语，TODO），所以死亡只能在转交失败时发现（`ipc_sendrec` 报错 → `mib_down`）。连带三个设计：同 label 新端点注册 = 老的死了（先清再挂，`:128-136`）；转交前把 `can_restart` 快照好（结点可能在调用中消失，10 §1.4）；`req_id` 恒 0（异步预留位，协议已支持异步但实现全同步，`:20-21`）——今天全同步，预留位不许乱用。

### 1.5 小结

认领（label+窗口+路径）→ 盖章（walk+盖/造+记表）→ 转交（三 grant+快照）→ 清场（死亡/请辞/退出）。记住"单向Shape（发信不回）+ 死亡常态 + 顶层禁挂"，22 的客户端就知道该怎么写信。

---

## 2 C 源码分析

### 2.1 端点表（`remote.c:24-49`）

32 个槽位（`MIB_ENDPTS = 1<<5`，`:25`——与 03 的最大端点数同值），每槽含三项：端点编号（或空标记）、挂载点链表头、服务标签（16 字节，`:28`）。`mib_remote_init` 将全表置空（`:40-49`，属 04 的第三阶段）。`mib_down(eid)`（`:55-74`）：遍历链表逐个调用卸载（**先保存下一个节点的指针再调用卸载**——卸载可能释放当前节点，`:64-68`），然后将槽位清零。断言槽位非空且链表非空（`:60-61`）——仅在"确定仍有挂载"时调用。

### 2.2 注册两函数（`:106-233`）

`mib_register`（信封层，`:197-233`）：SENDREC→`ENOSYS`（`:210-211`，注释讲透交叉死锁 + 用户态屏蔽 + 号段复用三理由，02 M-8）；label 获取失败→`EDONTREPLY`（`:217-218`，init 有 label 故"有 label≠是服务"的 TODO 在 `:87`）；路径超长→`EDONTREPLY`（`:221-224`）；余下交 `mib_do_register`；永不回信（`:231-232`）。

`mib_do_register`（策略层，`:109-192`）：槽位定位（若端点已在表中则复用该槽，若标签相同但端点不同则先清理旧槽再复用，否则取首个空槽，若满则打印并丢弃，`:124-148`）；若同端点下已挂载相同根标识则拒绝（`:155-165`，不允许同一服务对同一根重复挂载）；空槽先占位以便挂载过程能找到槽位（`:172-176`）；调用挂载（失败则若无其他挂载则清槽，`:181-187`）；将新挂载点链入该端点链表头（`:189-191`）。

### 2.3 注销两函数（`:239-307`）

`mib_do_deregister`（`:240-286`）：端点未知→静默（`:250-255`）；按根 id 摘链（`:257-263`，未知 id 静默，`:265-270`）；**先摘链后 unmount**（unmount 可能释放，`:272-275`）；链空则槽释放（`:279-282`）；`mib_unmount`（`:285`）。`mib_deregister`（`:292-307`）：同款单向门（`:296-297`），只读 `root_id`（02 §2.2），永不回信。

### 2.4 转交（`mib_remote_call`，`:378-477`）

| 步骤 | 位置 | 行为 |
|------|------|------|
| 取端点 | `:388-389` | 按 `node_eid`，断言非 NONE |
| 三 grant | `:396-413` | 名直授（`CPF_READ`）+ 06 两 relay（失败 `EINVAL` 非 ENOMEM，`:392-393`）；失败按序 revoke（后授先撤，`:401-413`） |
| 组信 | `:422-436` | `COMMON_MIB_CALL` + `req_id=0`（异步预留）+ 根 id/三 grant/用户端点/`flags=!!authed`（TODO 未定义标志集，`:434`）+ 双版本快照；短名亦走 grant（不优化，`:415-421` 注释） |
| 发信清场 | `:439-446` | `ipc_sendrec`；先 revoke 三 grant（无论成败，`:441-446`） |
| 发信失败 | `:455-459` | `mib_down` 清该端点全部 + 回 `ERESTART`（10 续走） |
| 回信检查 | `:461-464` | 非 REPLY/`req_id!=0` → `EINVAL`（02 `check_reply` 同形） |
| 服务请辞 | `:473-474` | status `ERESTART` → `mib_do_deregister`（交叉时序，注释 `:466-472`） |
| 回 status | `:476` | 原样回（含 ERESTART→10 续走） |

`mib_remote_info`（`:316-365`，挂载时用）：槽有效 → 双 grant（名+述，`CPF_WRITE`，败一撤一，`:330-339`）→ `COMMON_MIB_INFO`（`req_id=0`）→ `sendrec` → 双 revoke → 错码上浮 → 回信检查 → 回 status。

### 2.5 挂载（`mib_mount`，`tree.c:1543-1780`）

| 阶段 | 位置 | 行为 |
|------|------|------|
| 顶层禁 | `:1572-1576` | `miblen<2` → `EPERM`（唯一安全类限制 + TODO 白名单制，`:1566-1570`） |
| 标志窗 | `:1583-1591` | 版本/类型/四位子集，否则 `EINVAL` |
| 孩子窗 | `:1593-1598` | `csize≤4096` 且 `clen≤csize`，否则 `EINVAL` |
| 路径 walk | `:1610-1640` | 父必存在（`:1622` `ENOENT`）；元 id 拒（`:1614`）；逐个真本地非私有（`:1630-1637` `EPERM`） |
| 终点 id | `:1643-1647` | 元 id 拒 |
| 盖旧结点 | `:1654-1680` | 标志精确匹配（`FLAGS\|PARENT`，`:1660-1661`）；动态孩子在场 `EBUSY`（`:1673`，卸了恢复不了）；升级版本 |
| 造临时点 | `:1681-1767` | 父满拒（`:1687`）；`remote_info` 问名述（`:1702`）；名体检（`:1711`）；`mib_scan` 查名冲（`:1726`，id 冲突不可能，名冲 `EEXIST`）；一块分配名+述（`:1739`，败 **`ENOMEM`**——MIB 唯一配说 ENOMEM 的分配，`:1741-1744`）；初始化（标志去版本、size 0、名述指针，`:1755-1760`）；`mib_add` 链入 |
| 盖章 | `:1769-1778` | 置 REMOTE + eid/rcsize/rclen/rid；`remotes++`；回结点 |

### 2.6 卸载（`mib_unmount`，`:1789-1842`）

入口断言 NODE+REMOTE（`:1796-1797`）。遮蔽点（有 PARENT）：去 REMOTE，`csize=size` 复位，`clen` 重数静态活槽（`:1813-1818`），动态链表置空（挂载时拒了动态孩子，故无物可丢，`:1820`），版本 bump（`:1823`）。临时点：找父链环（`:1829-1834`，必找着）→ `mib_remove`（08 差量，`:1837`）。统一 `remotes--`（`:1840-1841`，断言大于 0）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 槽定位变枚举 | 循环 + `break` + 注释 | `SlotVerdict::{Reuse,ReapThenReuse,Fresh,Full}`（`remote.rs:74`）+ `locate_slot`（`:97`） | "同 label 异端点=老死了"是本篇最反直觉的一句（`:128-136`），值得变体名；满表静默丢是一等公民（单向无处回）不是 else 分支 |
| D2 | label 变定长值 | `char[16]` + `strlcpy/strcmp` | `Label{bytes:[u8;16]}`（`remote.rs:32`）+ `from_bytes/equals` | 16 含 NUL 的界（`:28`）在构造时钉（超长 `None`）；`equals` 逐字节（`strcmp` 语义，无 NUL 提前停留给调用方保证 terminator） |
| D3 | 挂载门变枚举 | `if` 链散 `return` | `MountHead::{Proceed,TooShort,BadFlags,BadWindow}`（`mount.rs:23`）+ `head_code`（`:60`） | 顶层禁挂（`EPERM`）与窗错（`EINVAL`）码不同因不同（安全 vs 格式），枚举让码有出处 |
| D4 | 回信检查变枚举 | 两处手写同形检查 | `check_reply`（`remote.rs:179`）+ `ReplyCheck::{Deliver,WrongType,WrongId}` | `remote_info` 与 `remote_call` 尾同形（`:359-364` vs `:461-464`）——第二次出现即抽象（02 `RemoteReply` 是 wire 视图，本篇是 verdict） |
| D5 | 恢复数学变函数 | 循环重数内联（`:1813-1818`） | `recount_clen`（`mount.rs:155`）+ `is_obscuring`（`:170`） | "动态孩子不可能在场故重数静态即全数"（`:1673` 保的）值得钉：重数函数 + 测试锁死 |

替代方案及否决：端点表本体（32 槽数组 + 链表手术）一步到位——否决，arena 在 13 首表落地（04 §4.4 声明的延续）；verdict 先行。死亡主动通知机制——C TODO（`:6-22`），非本篇 gap：如实记录为已知上游缺口（§4.4），不虚构设计。

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/mib/src/
├── remote.rs     — 本篇：MIB_ENDPTS/LABEL_MAX / EndptSlot+Label /
│                    SlotVerdict+locate_slot / register_gate/bound /
│                    dereg_slot / ReplyCheck+check_reply / caller_flag /
│                    label_fits
└── tree/mount.rs — 本篇：MountHead+check_head/head_code / path_node/id_ok /
                     TargetVerdict+check_target / temp_alloc_size /
                     recount_clen / is_obscuring / unmount_entry_ok
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 端点表 | `remote.c:24-35` | `remote.rs:16,23` | 32 槽；label 16 |
| 槽定位 | `:124-148` | `remote.rs:74,97` | 复用/先清/首空/满丢 |
| 单向门 | `:210-211,296-297` | `remote.rs:126` | SENDREC→ENOSYS |
| 路径界 | `:221-224` | `remote.rs:139` | 超 8 静默丢 |
| 注销定位 | `:245-255` | `remote.rs:150` | 未知静默 |
| 回信检查 | `:359-364,461-464` | `remote.rs:169,179` | 类型/id/status |
| 调用者标志 | `:434` | `remote.rs:195` | 1/0 + TODO 注 |
| label 界 | `:94-99` | `remote.rs:204` | 超 16 ENAMETOOLONG |
| 挂载三门 | `tree.c:1572-1598` | `mount.rs:23,41,60` | 短禁/窗错 |
| 路径策略 | `:1613-1647` | `mount.rs:77,85` | 真本地非私有；无元 id |
| 目标判定 | `:1659-1678` | `mount.rs:109,120` | 精确匹配；动态忙 |
| 临时块尺寸 | `:1739-1744` | `mount.rs:145` | 头+名+述+1；ENOMEM 唯一 |
| 恢复数学 | `:1804-1823` | `mount.rs:155,170,177` | 重数/遮蔽判/入口断言 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 单向永不回 | gate + 永 `EDONTREPLY` | `:231-232,305-306` |
| 同 label 即死亡 | `ReapThenReuse` | `:128-136` |
| 顶层不可挂 | `TooShort→EPERM` | `:1572-1576` |
| 动态孩子挡盖 | `Busy→EBUSY` | `:1673-1678` |
| 先摘链后释放 | 调用方序（doc 声明，arena 时） | `:64-68,272-275` |

### 4.4 与 C 的差异说明（模式 72 CSSCM）

§2 六节 vs §4 两模块：表手术/label 获取/grant 创建/sendrec/分配/链接移交 arena+transport（§1.2）；C TODO（死亡通知 `:6-22`、init label `:87`、白名单 `:1566-1570`、flags 定义 `:434`）如实记录为上游已知缺口，非本篇 gap。category：边界移交 + 已知上游 TODO（声明）。

---

## 5 测试要点

> 基线：`cargo test -p minix-mib --lib`，本篇 7 个测试（3 remote + 4 mount）。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_locate_slot` | `remote.c:124-148` | 复用/先清/首空/满丢 + 32 | `remote.rs` |
| `test_gates_and_bounds` | `:210-224,245-255,94-99` | 单向门/路径界/注销定位/label 界 | `remote.rs` |
| `test_reply_and_flag` | `:359-364,461-464,434` | 类型/id/status + 调用者位 | `remote.rs` |
| `test_check_head` | `tree.c:1572-1598` | 短禁/窗错/12 位界 | `mount.rs` |
| `test_path_policy` | `:1613-1647` | 真本地非私有四否 + 元 id | `mount.rs` |
| `test_check_target` | `:1659-1678` | 精确匹配/动态忙 + 块尺寸 | `mount.rs` |
| `test_unmount_restore` | `:1804-1823` | 遮蔽判/重数/入口 | `mount.rs` |

测试策略：四去向全覆盖（槽定位）+ 码对码（EPERM/EINVAL/EBUSY/ENOSYS/EDONTREPLY 各归其门）。

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-mib --lib`：**94 passed**（全 crate；其中本篇 7 个，见上表：`remote.rs` 3 个 + `tree/mount.rs` 4 个）
- 本节上表列出与本篇直接相关的 7 个（子集；总数随阶段推进增长）

---

## 6 过渡

远程 verdict 就绪：槽定位、挂载门、转交检查、恢复数学。下一站是 13——kern 子树：第一个函数子树（`clockrate/hardclock/ccpu/cp_time/consdev/drivers/boottime` + verify 双星 + 数据表），本篇的转交在那里第一次被"盖住 mock"（`kern.ipc`）。

## 7 参见

- C 源：`minix3/minix/servers/mib/remote.c`（全文）、`minix3/minix/servers/mib/tree.c:1543-1842`
- 阶段文档：`02-mib-message-contract.md`（信封/单向约束）、`03-mib-node-model.md`（四格/远端包）、`06-mib-copy-io.md`（relay）、`07-mib-auth-model.md`（label 校验隐含服务身份）、`08-mib-dynamic-nodes.md`（`mib_add/remove` 差量）、`10-mib-dispatch.md`（上一站，续走）、`13-mib-subtree-kern.md`（下一站）、`22-mib-rmib-client.md`（注册端）、`../07-stage-ds/08-ds-retrieve.md`（label 机制对端）
- Rust 实现：`os/servers/mib/src/remote.rs`、`os/servers/mib/src/tree/mount.rs`
- 外部消费者：`minix3/minix/servers/ipc/main.c`（`kern.ipc`）、`minix3/minix/net/lwip/mibtree.c`（`net.*`）、`minix3/minix/net/uds/stat.c`（`net.local`）
