# 04 — 系统信息库静态树初始化：七个顶层槽位、四次子树连接、一次全树走查

> **分类**: 启动 wiring / 初始化顺序
> **源码**: `minix3/minix/servers/mib/main.c:36-64`（静态表与根）、`main.c:384-410`（`mib_init`）、`minix3/minix/servers/mib/tree.c:1476-1536`（`mib_tree_recurse`/`mib_tree_init`）、四子树各一行（`kern.c:504-508`、`vm.c:150-154`、`hw.c:136-140`、`minix.c:85-89`）
> **说明**: 启动时静态树怎么立起来：七顶层槽位、根、四根 wiring 线、全树点名（计数/链父/继承版本）。子树表的内容在 13/14/15，点名算法的查找用法在 05。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要读启动代码的读者。前置知识：01（两次注册同体不同命）、03（结点形状/四格/版本号）。"静态初始化"在 C 里是编译期填表，MIB 还有一半是运行时点名——本篇讲的就是"编译期摆桌子，运行时点名"。

### 1.2 本章不讲什么

- 子树表里每个结点的语义——那是 13/14/15 的事（本篇只认"四根线插进四个槽"）。
- 动态结点的出生——那是 08 的事（点名只数静态的）。
- 查找顺序——那是 05 的事（本篇只给点名数出的 `csize/clen`，05 消费它们）。
- 远端表复位——那是 12 的事（本篇只认"第三阶段叫了它"）。

### 1.3 编译期布置空槽位，运行时走查初始化

C 语言的静态表分两阶段：编译期用 `MIB_ENODE` 先布置七个空槽位（名字、描述、读写权限先摆好，表大小与表指针先空着——原因是"跨文件的外部数组不能用 sizeof 求长度"，`main.c:388-393` 注释），运行时每棵子树用一行 `MIB_INIT_ENODE` 把实际的表与长度填上。这个两阶段是 C 语言的无奈（跨文件求数组长度），不是设计美学——Rust 侧直接定义七个顶层槽位常量（§4），"先空后填"这一步自然消失（§3 的第一项设计决策）。

走查初始化（`mib_tree_recurse`）对每个静态孩子做四件事：空位跳过（标志位为零表示该槽未使用，不是有效节点，`mib.h:131`），有效节点则计数加一（总节点数与有效孩子数各加一），版本号继承自父节点，并把父指针链上；若该孩子本身又是可挂载孩子的目录节点则继续递归。注意**走查初始化不碰动态链表**——动态结点创建时自行计数（08 的添加函数），重启时丢弃也不重新计数（01 的重启仅保留静态部分在此兑现：静态部分重新走查一遍，动态部分一去不回）。

`mib_init` 的完整顺序（`main.c:384-410`）：依次连接四棵子树（内核、虚拟内存、硬件、Minix 扩展）→ 全树走查初始化 → 远端端点表复位。网络、用户、厂商三类不在该顺序上：网络等远端来挂载（12），用户态部分在 C 库内（21），厂商初始就是空的（03 §2.5）。

### 1.4 小结

七个顶层槽位（六个只读一个可写）加一个根节点（可写、无名、仅内部可见）→ 依次连接四棵子树 → 走查初始化（跳过空位、计数、继承版本、链父、递归）→ 复位远端表。记住"布置槽位—连接子树—走查初始化"三段，13/14/15 的表往里填，05/08 消费走查得出的数量。

---

## 2 C 源码分析

### 2.1 七槽与一根（`main.c:36-62`）

| 槽 | id | 名 | 描述 | 读写 | 说明 |
|----|----|----|------|------|------|
| 1 | `CTL_KERN` | kern | High kernel | 只读 | 四线第一根 |
| 2 | `CTL_VM` | vm | Virtual memory | 只读 | 四线第二根 |
| 4 | `CTL_NET` | net | Networking | 只读 | 空到来挂（12） |
| 6 | `CTL_HW` | hw | Generic CPU, I/O | 只读 | 四线第三根 |
| 8 | `CTL_USER` | user | User-level | 只读 | 住 libc，占位为让 sysctl(8) 列出（`:41-43`） |
| 11 | `CTL_VENDOR` | vendor | Vendor specific | **可写** | 第三方 scratch |
| 32 | `CTL_MINIX` | minix | MINIX3 specific | 只读 | 四线第四根 |

槽位 id 稀疏（1/2/4/6/8/11/32）——数组按最大 id 定长，空洞的槽 `flags==0`，点名跳过（§1.3）。根（`:62`）：`MIB_NODE(_RW, mib_table, "", "")`——可写（init(8) 可种顶层结点，`:59-60`）、无名、用户态不可达（`:57-58`）。

### 2.2 `mib_init` 四行（`main.c:384-410`）

| 行 | 调用 | 效果 |
|----|------|------|
| `:395` | `mib_kern_init(&mib_table[CTL_KERN])` | `MIB_INIT_ENODE(node, mib_kern_table)`（`kern.c:507`） |
| `:396` | `mib_vm_init(&mib_table[CTL_VM])` | 同上（`vm.c:153`） |
| `:397` | `mib_hw_init(&mib_table[CTL_HW])` | 同上（`hw.c:139`） |
| `:398` | `mib_minix_init(&mib_table[CTL_MINIX])` | 同上（`minix.c:88`） |
| `:404` | `mib_tree_init()` | §2.3 |
| `:407` | `mib_remote_init()` | 远端表复位（12） |

四行一模一样（都是 `MIB_INIT_ENODE(node, mib_*_table)`，即 `size=表长 + scptr=表`，`mib.h:315-319`）——"不能对外部数组 `sizeof`"（`:388-393`）所以推迟到运行时填。返回 `OK`（`:409`）。

### 2.3 点名：`mib_tree_recurse` + `mib_tree_init`（`tree.c:1476-1536`）

入口断言（`:1485-1486`）：必须是 NODE+PARENT 才走——别的进来是调用者 bug。循环（`:1497-1512`）：`csize=size` 先记（`:1493`，注释点明以后动态结点也要算进来，所以**不能拿 `csize` 遍历静态表**——遍历用 `IS_STATIC_ID` 即 `size`），空位跳过，活的 `mib_nodes++` + `clen++`，版本继承，链父亲，NODE+PARENT 递归。

`mib_tree_init`（`:1519-1536`）：`nodes=1`（根自己，`:1524`）、`objects=0`（`:1525`，注意 `remotes` 不清零——远端表由 `mib_remote_init` 管，12）、根版本 1 + 父 NULL（`:1531-1532`），然后从根点名。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 空架子后填消失 | `MIB_ENODE` 空表 + `MIB_INIT_ENODE` 后填（跨文件 `sizeof` 无奈） | `TOP_SLOTS` 七常量一步到位（`static_tree.rs:36`） | 无奈不是语义：Rust 同 crate 常量直接定长，`MIB_INIT_ENODE` 这一步没有对应物——`slot_flags`（`:86`）只算读写位，类型位由 03 分类 |
| D2 | 四线变数组 | 四行顺序调用 | `WIRE_ORDER`（`init.rs:32`）+ `WireStep`（`:20`） | 顺序是契约（kern 先），数组让顺序可测（`assert_eq!` 整序）；13/14/15 各消费一根线，线头名字从这里 grep 得到 |
| D3 | 点名 verdict 化 | 循环里计数/链父/递归混写 | `judge_child`（`init.rs:75`：空/计数/计数+递归）+ `fold_static`（`:91`：纯折叠，返 `(live, needs_recurse)`） | 链父指针是 arena 效果（待 04 后续：静态 arena 落地时），verdict 先行——verdict-first 延续 01/02/03；`check_parent`（`:116`）把入口断言变成 `bool` |
| D4 | 根变规格 | `MIB_NODE(_RW, table, "", "")` 一行 | `RootSpec::spec()`（`static_tree.rs:123`：可写恒真） | 根的"可写、无名、内部"三属性里只有可写影响行为（init 可种顶层）；无名/内部是注释级事实，doc 写清即可，不值得类型 |

替代方案及否决：静态 arena（把七槽 + 子表一次性建成真正的树，含父指针）——否决，子表内容在 13/14/15，arena 等 13 落第一棵子树时再建，本篇只钉槽位与点名 verdict（verdict-first 同 01 D-否决）。

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/mib/src/tree/
├── static_tree.rs — 本篇：TopSlot / TOP_SLOTS / slot_flags / top_slot / RootSpec
└── init.rs        — 本篇：WireStep / WIRE_ORDER / InitPhase / PHASE_ORDER /
                      ChildVerdict / judge_child / fold_static / check_parent
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 七槽 | `main.c:46-54` | `static_tree.rs:19,36` | id 稀疏；vendor 唯一可写 |
| 槽读写位 | `mib.h:259-263` | `static_tree.rs:86` | 全 PERMANENT；读写按槽 |
| 查槽 | —（Rust 侧新服务） | `static_tree.rs:99` | 无槽 → `None`（VFS 等） |
| 根规格 | `main.c:62` | `static_tree.rs:116,123` | 可写恒真 |
| 四线/三阶段 | `main.c:384-410` | `init.rs:20,32,39,49` | 顺序钉死 |
| 点名 verdict | `tree.c:1497-1512` | `init.rs:59,75,91` | 空/计数/递归；纯折叠 |
| 入口断言 | `tree.c:1485-1486` | `init.rs:116` | 非 NODE+PARENT 拒绝 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 七槽稀疏 id 全对 | `test_seven_slots` | `main.c:46-54` |
| 槽全 PERMANENT | `slot_flags` 恒或 | `MIB_ENODE` 宏 |
| 线序 kern→vm→hw→minix | `WIRE_ORDER` 整序断言 | `main.c:395-398` |
| 空位不计数 | `judge_child(0)==Skip` | `:1498-1499` |
| 遍历不用 csize | `fold_static` 取切片长 | `:1488-1493` 注释 |

### 4.4 与 C 的差异说明（模式 72 CSSCM）

C 两截（空架子+后填）vs Rust 一步（D1）：文件组织/语言能力差异，语义零差。点名循环 vs verdict+折叠：效果（链父指针写、mib_nodes 全局++）待静态 arena，verdict 先行——已知缺口，arena 在 13 首表落地时建（04 后续工作声明，非遗漏）。

---

## 5 测试要点

> 基线：`cargo test -p minix-mib --lib`，本篇 6 个测试（2 static_tree + 4 init）。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_seven_slots` | `main.c:46-54` | 七槽 id 稀疏 + 查槽命中/落空 | `static_tree.rs` |
| `test_slot_access` | `main.c:47-53` + `mib.h:259-263` | 全 PERMANENT + vendor 唯一可写 + 根可写 | `static_tree.rs` |
| `test_wire_and_phase_order` | `main.c:384-410` | 四线整序 + 三阶段整序 | `init.rs` |
| `test_judge_child` | `tree.c:1497-1512` | 空跳过/叶计数/父递归/函数树不递归 | `init.rs` |
| `test_fold_static` | `tree.c:1497-1512` | 混合数组 (2,true) + 全空 + 单叶 | `init.rs` |
| `test_check_parent` | `tree.c:1485-1486` | 仅 NODE+PARENT 进入 | `init.rs` |

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-mib --lib`：**94 passed**（全 crate；其中本篇 6 个，见上表）
- 本节上表列出与本篇直接相关的 6 个（子集；总数随阶段推进增长）

---

## 6 过渡

桌子摆好（七槽）、线序钉死（四线三阶段）、点名 verdict 就绪。下一站是 05——查找：`IS_STATIC_ID` 判定怎么走、静态 O(1) 与动态链表 O(n) 怎么分工（本篇 `fold_static` 数的 `clen` 在那里被消费）。

## 7 参见

- C 源：`minix3/minix/servers/mib/main.c:36-64,384-410`、`minix3/minix/servers/mib/tree.c:1476-1536`、`minix3/minix/servers/mib/mib.h:252-324`、`kern.c:504-508`、`vm.c:150-154`、`hw.c:136-140`、`minix.c:85-89`
- 阶段文档：`01-mib-init-main.md`（注册）、`03-mib-node-model.md`（上一站，形状）、`05-mib-tree-lookup.md`（下一站，查找）、`13/14/15-mib-subtree-*.md`（四线内容）、`12-mib-remote-subtrees.md`（第三阶段）
- Rust 实现：`os/servers/mib/src/tree/static_tree.rs`、`os/servers/mib/src/tree/init.rs`
