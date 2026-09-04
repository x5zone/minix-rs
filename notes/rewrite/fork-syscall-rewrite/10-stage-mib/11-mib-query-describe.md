# 11 — 系统信息库枚举与描述：把树中节点信息复制给用户态

> **分类**: 枚举序列化 / 交换格式
> **源码**: `minix3/minix/servers/mib/tree.c:90-173`（`mib_copyout_node`）、`:179-239`（`mib_query`）、`:925-966`（`mib_copyout_desc`）、`:972-1090`（`mib_describe`）
> **说明**: `QUERY` 看孩子长什么样，`DESCRIBE` 看/写描述串：两次序列化（节点快照 + 描述流）与一次"只写一次"的描述设置。10 的元 op 在这里落地。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要实现 libc `sysctlgetmibinfo` 对端（21）与测试（15）的人。前置知识：02（交换格式形状）、07（可见性）、10（元 op 投递）。"序列化"即把树里的数据摆成用户能解析的字节流。

### 1.2 本章不讲什么

- 分发怎么投到这里——那是 10 的事。
- 描述的内存归属（`strdup`/`OWNDESC`/随结点释放）——08 §2.3/§2.4 已建模（`remove_delta.free_desc`），本篇只判"能不能设"。
- 字符串拷入——06（`copyin_str`）。

### 1.3 两次复制：节点快照与描述流

枚举查询复制"孩子长什么样"：为每个孩子生成一张节点快照（标志去除内部位、版本号、名字、大小、立即值、孩子窗口或函数标记），先静态后动态，**不排序**——用户态库自行排序（`sysctlgetmibinfo(3)`，`:207-212` 注释 + 21）。描述查询复制"描述字符串是什么"：为每个孩子生成一条变长描述记录（编号、版本、长度、字符串），按 4 字节对齐打包，读取需用下一条描述宏循环（02 §2.3）。

两次复制共享三条规则：若用户提供的接收区已越界则跳过该节点但长度照计（范围检查未覆盖即跳过，长度仍计入总数）；私有节点对普通用户隐身（快照保留形状但值清零 `:115-132`，描述直接报告长度为零 `:934-935`——枚举过程不因单个私有节点而中止整组，静默跳过）；版本号随快照一起返回（用户可用它做下一次的一致性检查，03 §1.6）。

描述字符串的设置是"只写一次"：仅当当前无描述、节点非永久、调用者为超级用户且版本号匹配时才允许设置（`:994-1031` 的六项检查），设置成功后标记为拥有描述（以后随节点释放，08）。NetBSD 根本不支持设置描述（`:751-754` 注释）——这是本服务的扩展，但读取路径与 NetBSD 兼容。

### 1.4 小结

枚举查询：先过版本号一致性检查 → 依次生成静态孩子快照 → 再生成动态孩子快照（不排序）。描述查询：若携带新数据则先定位目标节点、经过六项设置检查、设置描述、回显新描述；若未携带新数据则依次生成静态描述流与动态描述流并按 4 字节对齐打包。两次复制共享三条规则：越界则跳过但长度照计、私有节点对普通用户隐身、版本号随快照返回。

---

## 2 C 源码分析

### 2.1 `mib_copyout_node`（`:90-173`）

| 步骤 | 位置 | 行为 |
|------|------|------|
| 越界 | `:97-98` | `inrange` 外 → 回 `sizeof(scn)` 不动（长度照计） |
| 标志 | `:107-108` | `VERSION \| 去内部三位`（PARENT/VERIFY/REMOTE 永不暴露，双关含义都不） |
| 号/名/版本/尺寸 | `:109-112` | id、名拷贝、版本、`node_size` |
| 可见性 | `:115` | `!PRIVATE \|\| authed`（07 `can_see` 同谓词） |
| 立即值 | `:121-132` | 立即 + 可见 → 按类型填 bdata/idata/qdata（不可见留零：形状给，值不给） |
| NODE 特则 | `:135-169` | 尺寸改报 `sizeof(scn)`（NetBSD 同款）；远端报缓存窗口（不进服务问，`:140-144` 可靠性注释）；真爹报本地窗口（可见才）；函数结点置 `SYSCTL_NODE_FN`（假地址，防 ASR 泄漏，`:149-155`） |
| 拷出 | `:172` | `copyout`（错码上浮） |

### 2.2 `mib_query`（`:179-239`）

新数据在场 → 拷入 `sysctlnode` → 版本门（VERS_1，`:194-195`）→ staged 版须配父/根（`:201-204`，08 `create_ver_ok` 同形）；静态表（空位跳，`:216-225`）→ 动态表（`:228-236`）；回总长（`:238`）。**顺序不保证**（`:207-212`）。

### 2.3 `mib_copyout_desc`（`:925-966`）

私有 + plain → 回 0（`:934-935`，长度 0 不是错误——枚举继续）；长 = 串长+1（含终结，无串则 1，`:937-941`）；scratch 断言装下（`:943`，MAXDESCLEN 封顶故为断言非错误）；填 num/ver/len/串（`:946-954`）；总长 = 头 + 串（`:956`）；拷出；回 4 对齐长（`:965`，间隙垃圾是用户自己的，`:961-964`）。

### 2.4 `mib_describe`（`:972-1090`）

有新数据：拷入 → 版本门（`:987-988`）→ 定位（落空 `ENOENT`，`:991-992`）→ 私有读 `EPERM`（`:994-996`）→ 有描述指针即设置流（superuser `:1003-1005` → 非挂载 `:1012-1014` `EBUSY` → 无旧描述 `:1016-1018` → 非永久 `:1020-1022` → 版本配 `:1029-1031` → scratch 拷入 `:1037-1040` → `strdup` `:1043-1048`（败 `EINVAL` 非 ENOMEM）→ `OWNDESC` `:1051`）→ 回显新描述（`:1060`，此前权限已过，此处必非零）。无新数据：静态描述流（`:1067-1076`）→ 动态描述流（`:1079-1087`）→ 回总长（`:1089`，注释指回 query 的顺序说明，`:1063`）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 标志剥离变函数 | 内联位运算（`:107-108`） | `export_flags`（`query.rs:23`）+ `export_version_ok`（`:29`） | "内部三位永不暴露"是安全 invariant（ASR 侧信道见 `:149-155`），值得函数 + 版本字节断言 |
| D2 | 可见性复用 07 | 三处手写同谓词（`:115,:934,:1389`） | `expose_immediate`/`desc_visible` 调 `can_see`（`query.rs:56`，`describe.rs:129`） | 同谓词第三次出现（07 两处 + 本篇两处）——复用不断言两次（DRY 跨篇，07 为主） |
| D3 | 描述长度变算术 | `strlen+1`/分支/宏三处散写 | `desc_len`（`describe.rs:29`）+ `packed_size`（`:48`）+ `roundup_desc`（`:39`） | "无串亦一 NUL"（`:941`）与"对齐间隙是用户数据"（`:961-964`）两易错点收进纯函数 + 断言 |
| D4 | 描述字符串设置的六项检查独立为枚举 | 条件分支链式返回 | `SetDescRefusal` 六种拒绝原因 + `check_set_desc`（`describe.rs:71,88`） | 与 08 的销毁拒绝同构（六种拒绝原因各对应错误码）；检查顺序与 C 源码一致（私有可见性→超级用户→是否为挂载点→是否已设→是否永久→版本一致性） |
| D5 | 函数标记变常量引用 | `SYSCTL_NODE_FN` 裸用（`:168`） | `minix-types` `SYSCTL_NODE_FN`（02 补钉，本篇测试复钉） | 假地址的值（0x1）是 ABI（trace(1) 判非零即函数）；值钉两处（02 值表 + 本篇行为测试）防单边漂移 |

替代方案及否决：walker（静态+动态双循环）一步到位——否决，arena 在 13 首表落地；verdict 先行（verdict-first 延续）。

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/mib/src/
├── query.rs    — 本篇：export_flags/version_ok / query_ver_ok / report_size /
│                  expose_immediate / child_window / is_func_marker / is_node_type
└── describe.rs — 本篇：DESC_HEADER/ALIGN / desc_len / roundup_desc /
                   packed_size / fits_scratch / SetDescRefusal+check_set_desc /
                   is_node_flags / desc_visible
```

`SYSCTL_NODE_FN` 住 `minix-types` `sysctl.rs`（02 值表补钉）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 标志剥离 | `:107-108` | `query.rs:23,29` | 去三位 + 版本字节断言 |
| 版本门 | `:201-204` | `query.rs:38` | 零/父/根 |
| 尺寸上报 | `:112,135-137` | `query.rs:47` | NODE 报交换宽 |
| 立即暴露 | `:121-132` | `query.rs:56` | 立即+可见（07 复用） |
| 孩子窗口 | `:157-166` | `query.rs:67` | 远端缓存/本地/函数无 |
| 函数标记 | `:167-168` | `query.rs:87` + 常量 | 非远端非父即标记 |
| 描述长度 | `:937-941` | `describe.rs:29` | 含终结；无亦一 |
| 打包 | `:956-965` | `describe.rs:39,48` | 头+串，4 对齐 |
| scratch 断言 | `:943` | `describe.rs:57` | MAXDESCLEN 封顶故断言 |
| 设置六杠 | `:994-1031` | `describe.rs:71,88` | 顺序六拒 |
| 描述可见 | `:934-935` | `describe.rs:129` | 07 复用；零长非错 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 内部三位零暴露 | `export_flags` 掩码 + 测试 | `:102-108` |
| 版本字节恒 VERS_1 | `export_version_ok` | `:107` |
| 私有描述零长非错 | `desc_visible` 语义 | `:934-935` |
| 设置一次性 | `AlreadySet` 杠 | `:1016-1018` |
| 失败非 ENOMEM | 调用方（02 §2.7） | `:1046` |

### 4.4 与 C 的差异说明（模式 72 CSSCM）

§2 四节 vs §4 两模块：双循环 walker/拷贝/`strdup`/scratch staging 移交 arena+transport（§1.2）；`mib_query` 版本门与 08 `create_ver_ok` 同形不同址（各调各处，行为同）。category：边界移交。

---

## 5 测试要点

> 基线：`cargo test -p minix-mib --lib` + `cargo test -p minix-types --lib`，本篇 8 个测试（5 query + 2 describe + 1 常量）。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_export_flags_strips_internal` | `:102-108` | 去三位/版本字节/私有位保留 | `query.rs` |
| `test_query_ver_ok` | `:201-204` | 零/父/根/他 | `query.rs` |
| `test_report_size` | `:112,135-137` | NODE 报交换宽/叶报实宽 | `query.rs` |
| `test_expose_immediate` | `:121-132` | 立即+可见四组合 | `query.rs` |
| `test_child_window_and_marker` | `:157-168` | 远端/真爹/函数/叶子 + 标记值 | `query.rs` |
| `test_desc_lengths` | `:937-965` | 终结/打包/对齐/scratch | `describe.rs` |
| `test_set_desc_guards` | `:994-1031` | 六杠顺序 + 双 Ok | `describe.rs` |
| `test_minix_subtree_ids`（补断言） | `minix/sysctl.h:10` | `SYSCTL_NODE_FN==0x1` | `sysctl.rs` |

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-mib --lib`：**94 passed**（全 crate；其中本篇 7 个，见上表：`query.rs` 5 个 + `describe.rs` 2 个）
- `cargo test -p minix-types --lib`：**139 passed**（全 crate；`SYSCTL_NODE_FN` 的值已在 `types/sysctl.rs` 的 `test_minix_subtree_ids` 中钉住，无需为本篇新增独立测试）
- 本节上表列出与本篇直接相关的 7 个（子集；`query` + `describe` 各自独立可测）

---

## 6 过渡

枚举与描述 verdict 就绪：版本门、剥离、窗口、打包、六杠。下一站是 12——远程子树：挂载/卸载/转交/死亡检测（本篇 `child_window` 的远端窗口在那里第一次被填上真数）。

## 7 参见

- C 源：`minix3/minix/servers/mib/tree.c:90-173,179-239,925-966,972-1090`、`minix3/sys/sys/sysctl.h:1442-1454`、`minix3/minix/include/minix/sysctl.h:10`
- 阶段文档：`02-mib-message-contract.md`（交换格式）、`07-mib-auth-model.md`（可见性复用）、`08-mib-dynamic-nodes.md`（版本门同形）、`10-mib-dispatch.md`（上一站，元投递）、`12-mib-remote-subtrees.md`（下一站）、`15-mib-subtree-minix.md`（统计口消费版本）、`21-mib-client-libc.md`（排序消费方）
- Rust 实现：`os/servers/mib/src/query.rs`、`os/servers/mib/src/describe.rs`、`os/libs/minix-types/src/types/sysctl.rs`（`SYSCTL_NODE_FN`）
