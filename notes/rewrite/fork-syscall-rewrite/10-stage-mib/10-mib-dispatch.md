# 10 — 系统信息库请求分发：逐层解析名字，走到叶子节点或处理函数

> **分类**: 分发循环 / 名字解析
> **源码**: `minix3/minix/servers/mib/tree.c:1332-1474`（`mib_dispatch` 全段）
> **说明**: 系统控制调用旅程的核心：名字按分量逐层消耗与解析，负数元标识符的多路分支，三类节点（普通叶子、函数接管、远端挂载）的判定，以及远端服务返回需重启标记时的本地续走逻辑。本篇含系统控制次主线的完整路径图（plan §1.3）。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要读 handler（09/13~20）与远程（12）的人。前置知识：05（单层查找）、07（写门与可见性）、09（叶子读写）。ENOENT/ENOTDIR/EISDIR 按常规理解。

### 1.2 本章不讲什么

- 单层查找 verdict——那是 05 的事（本篇消费 `find` 结果）。
- 叶子读写 verdict——那是 09 的事（本篇终点调它）。
- 枚举/描述/创建/销毁本体——那是 11/08 的事（本篇只 switch 到它们）。
- 远端调用本体——那是 12 的事（本篇只判续走）。

### 1.3 一层一判

`mib_dispatch` 是个 `for` 循环：从根出发，每轮吃掉一个名字分量（`call_name++`、`namelen--`，指针游走，`:1347-1349`），对找到的孩子判一轮：元标识符？（负数）→ 找着没？→ 私有？→ 远端？→ 叶/函数？→ 名字还有剩？→ 写否？→ 函数调 / 叶子读写 / 继续下钻。**每轮恰好 consume 一个分量**（元标识符轮除外：它必须是最后一个分量，有剩即 `EINVAL`）。循环正常只通过三条路离开：调出去（函数/读写/元 op）、报错、名字走完落到父上（`EISDIR`）。

sysctl 次主线路径图（plan §1.3，本篇为家）：

```
用户态 sysctl(3)/__sysctl（libc）                    ← 21
  ├─ CTL_USER → libc 本地处理（不进 MIB）             ← 21
  └─ 其余 → _syscall(MIB_PROC_NR, MIB_SYSCTL)         ← 02 信封
  ▼ MIB 主循环 mib_sysctl()                          ← 01 六道门
  ├─ namelen/名/old/new 配对 verdict
  └─ mib_dispatch(&call, oldp, newp)  ← ★本篇
       ▼ 从 mib_root 逐层（每轮一分量）
       ├─ id < 0（元标识符，须末位）→ QUERY/CREATE/DESTROY/DESCRIBE → 11/08
       │                                CREATESYM/MMAP/其他 → EOPNOTSUPP
       ├─ mib_find 落空 → ENOENT                          ← 05
       ├─ PRIVATE 且 plain → EPERM                        ← 07 can_see
       ├─ REMOTE → mib_remote_call                        ← 12
       │     ├─ r ≠ ERESTART → 原样返回
       │     ├─ ERESTART + 无本地 → ENOENT（挂载蒸发）
       │     └─ ERESTART + 有本地 → 续走本地（当挂载没发生过）
       ├─ 叶/函数判定（三问，见 §1.4）
       ├─ 叶 + 名有剩 → ENOTDIR
       ├─ 落点 + 新数据 → 写两道杠                        ← 07 check_write
       ├─ 有函数 → node_func                              ← 13~20
       └─  plain 叶 → mib_readwrite                       ← 09
       ▼ 名走完落父上 → EISDIR（"读目录即数据"）
  ▼ 返回：r ≥ 0 报长（溢出 ENOMEM）；r < 0 带 staged 长   ← 01 §2.4
```

### 1.4 通过三个问题确定节点的最终处理方式

每轮对查找到的孩子问三个问题（`:1402-1431`）：是否为叶子节点（类型不是目录节点）？是否为远端挂载节点？是否有处理函数——叶子节点看是否有校验标志位或函数指针（校验优先，`:1426-1427`），非叶子节点看是否缺少父节点标志（缺少即表示由函数接管，省内存的双关设计，`:1430`，03 §2.3）。八种组合收敛为五种去向：远端节点则转交远端服务（需先记录是否可本地重启，因为节点可能在调用期间消失，`:1405-1406`）；叶子节点且名字还有剩余分量则返回非目录错误；落点节点且携带新数据则进入写入权限检查；有处理函数则调用函数；普通叶子则走通用读写；普通父节点则继续下钻。注意**函数叶子也检查剩余名字**（`:1437` 先于一切：函数接管的是"整棵子树"，名字有剩余说明走错了分支，不是"函数再细分"）。

### 1.5 小结

每层处理一个名字分量，依次经过：元标识符判断 → 查找 → 私有可见性检查 → 远端挂载检查 → 节点形状判断 → 剩余名字检查 → 写入权限检查 → 调用函数或读写或下钻；名字耗尽时落在普通父节点上则返回目录错误。记住"每轮消耗一个分量"与"先记录是否可重启再调用远端"，12 的续走逻辑就通了。

---

## 2 C 源码分析

### 2.1 主循环（`tree.c:1346-1474`）

| 步骤 | 位置 | 行为 |
|------|------|------|
| 取分量 | `:1346-1349` | `id=name[0]`，指针+长度推进；入口断言 `namelen≤12`（`:1340`，01 已判） |
| 父断言 | `:1351-1352` | 父必 NODE+PARENT（能走到这的父都是，形状 audit） |
| 元 switch | `:1360-1382` | 负数：有剩 `EINVAL`；QUERY/CREATE/DESTROY/DESCRIBE 分投 11/08；余 `EOPNOTSUPP` |
| 找 | `:1384-1386` | `mib_find` 落空 → `ENOENT` |
| 私有 | `:1388-1390` | `PRIVATE && !authed` → `EPERM` |
| 远端挂载处理 | `:1404-1416` | 先记录是否可本地重启（根据父标志判断）；调用远端；若返回码不是需重启标记则原样返回；需重启但无本地可重启则返回未找到，有则继续本地解析 |
| 节点形状 | `:1425-1431` | 叶子节点看校验位与函数指针，非叶子节点看是否缺少父标志即为函数接管 |
| 剩余名字检查 | `:1437-1438` | 叶子节点且名字还有剩余分量 → 返回非目录错误 |
| 写入权限检查 | `:1446-1458` | 落点为叶子或函数且携带新数据 → 先检查可读写标志，再检查任意用户可写或超级用户 |
| 最终分发 | `:1461-1470` | 调用处理函数 / 叶子通用读写（传递校验回调）/ 继续下钻到子节点 |
| 名字耗尽 | `:1472-1473` | 名字分量耗尽时落在普通父节点 → 返回目录错误 |

### 2.2 注释里的三句实话

- `:1354-1359`：常规 id 永不为负，但 handler 子路（如 PROC2）可用负 id——"元标识符"只是分发层的约定，不是全局禁令（18 的子路）。
- `:1441-1444`：函数落点的写门"是否该提前查值得商榷，但覆盖全部用例"——诚实的技术债注释。
- `:1418-1424`：叶/非叶用不同方式判函数，"为每结点省几个字节"——内存优化的外科手术，03 D-否决 union 直译的同源动机。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 元 switch 变函数 | `switch` 内联 + 散 `return` | `judge_meta(id, remaining)`（`dispatch.rs:42`）+ `MetaOp`（`:25`） | "末位"规则（`:1365`）与"余皆不支持"（`:1377-1380`）值得独立测试（含 `-99 → EOPNOTSUPP`） |
| D2 | 每层判决独立为函数 | 循环体内联九项判断 | `judge_level`（`dispatch.rs:95`）+ `LevelVerdict` 八种结果（`:63`） | 循环本体是走查效果（后续竞技场实现），判决是纯逻辑；是否可本地重启的标志先记录（`:1405-1406`）并编码进 `RemoteCall{can_restart}` 字段——确保调用前快照不丢失 |
| D3 | 形状变函数 | 三目内联（`:1425-1431`） | `resolve_shape`（`dispatch.rs:131`）返 `(has_func, has_verify)` | 叶/非叶双关（`:1430` 省字节）是最易误读的三行；函数+测试钉死"verify 优先"与"无 PARENT 即函数" |
| D4 | 远端调用结果的去向独立为枚举 | 条件分支内联（`:1410-1411`） | `judge_remote_result`（`dispatch.rs:167`）+ `RemoteOutcome::{Return, RestartLocal}` | 三种去向（原样返回、返回未找到需本地无可重启、本地续走）是 12 的核心契约，值得独立命名；重启标记参数化使测试不依赖具体错误码常量 |
| D5 | 终止情形的错误码映射独立为函数 | 多处直接返回错误码散写 | `terminal_code`（`dispatch.rs:183`）→ `Option`（动作类判决本身无错误码） | 写入权限的错误码复用 07 的常量（同值不重复断言）；走查器查表不重复判决 |

替代方案及否决：walk 循环一步到位（含 arena 指针游走）——否决，arena 在 13 首表落地；verdict 先行（verdict-first 延续 01~09）。

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/mib/src/tree/
└── dispatch.rs — 本篇：MetaOp+judge_meta / LevelVerdict+judge_level /
                   resolve_shape / RemoteOutcome+judge_remote_result /
                   terminal_code / EISDIR_EMPTY / is_leaf/remote_flags
```

（plan 表列 `dispatch.rs`（crate 根）——根已被 01 主循环 verdict 占据，故本篇落 `tree/dispatch.rs`（名字解析分发）。路径偏差声明：功能归属与 plan 一致，仅落点下沉一级。）

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 元 op | `:1368-1381` | `dispatch.rs:25,42` | 末位规则；余 EOPNOTSUPP |
| 轮 verdict | `:1384-1470` | `dispatch.rs:63,95` | 八去向；07 写门复用 |
| 形状 | `:1425-1431` | `dispatch.rs:131` | verify 优先；无 PARENT 即函数 |
| 续走 | `:1404-1416` | `dispatch.rs:152,167` | 三去向 |
| 终端码 | `:1386,1438` + 07 | `dispatch.rs:183` | ENOENT/ENOTDIR/EPERM；动作无码 |
| 名尽 | `:1472-1473` | `dispatch.rs:199` | EISDIR |
| 叶/远端谓词 | 类型/位 | `dispatch.rs:202,207` | 单谓词 |

注：`:1389` 私有检查不在 `judge_level` 内——walker 按"找→`can_see`（07）→判"顺序调用（§4.4）。

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 元必末位 | `judge_meta` 首判 | `:1365-1366` |
| 快照先行 | `RemoteCall{can_restart}` 字段 | `:1405-1406` |
| 剩名先于写门 | `judge_level` 顺序 | `:1437` 先于 `:1446` |
| 终端码单一来源 | `terminal_code` | 写门码引 07 常量 |

### 4.4 与 C 的差异说明（模式 72 CSSCM）

§2 两节 vs §4 一模块：walk 循环/arena/调用移交后续（§1.2）；`:1389` 由 walker 调 07（上注）；模块落点 `tree/`（上注）。category：边界移交 + 组织偏差（声明）。

---

## 5 测试要点

> 基线：`cargo test -p minix-mib --lib`，本篇 5 个测试。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_judge_meta` | `:1365-1381` | 四 op/尾剩 EINVAL/CREATESYM+MMAP+野负 EOPNOTSUPP | `dispatch.rs` |
| `test_resolve_shape` | `:1425-1431` | 叶三组合/非叶双组合 | `dispatch.rs` |
| `test_judge_level_terminals` | `:1437-1467` | 剩名/写两杠/函数/读写verify/下钻/远端短路/ANYWRITE | `dispatch.rs` |
| `test_judge_remote_result` | `:1410-1416` | 原返/死路/续走 | `dispatch.rs` |
| `test_empty_is_dir` | `:1472-1473` | EISDIR + 谓词 + 终端码表 | `dispatch.rs` |

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-mib --lib`：**94 passed**（全 crate；其中本篇 5 个，见上表）
- 本节上表列出与本篇直接相关的 5 个（子集；总数随阶段推进增长）

---

## 6 过渡

分发 verdict 就绪：元 switch、轮八判、续走三去向。下一站是 11——枚举与描述：`QUERY`/`DESCRIBE` 的本体（本篇 `MetaOp::{Query, Describe}` 投向它们）与交换格式序列化。

## 7 参见

- C 源：`minix3/minix/servers/mib/tree.c:1332-1474`
- 阶段文档：`01-mib-init-main.md`（调用方）、`05-mib-tree-lookup.md`（单层查找）、`07-mib-auth-model.md`（写门/可见性复用）、`08-mib-dynamic-nodes.md`（CREATE/DESTROY 本体）、`09-mib-data-access.md`（叶子终点）、`11-mib-query-describe.md`（下一站）、`12-mib-remote-subtrees.md`（续走本体）
- Rust 实现：`os/servers/mib/src/tree/dispatch.rs`、`os/servers/mib/src/auth.rs`（`WRITE_DENIED`）
