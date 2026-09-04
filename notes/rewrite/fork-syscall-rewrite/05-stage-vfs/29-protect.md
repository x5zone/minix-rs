# 29 — protect：九位权限判定、chmod-chown与umask

本文讲清权限层如何处理四类操作：权限判定（按调用者 uid/gid 与文件九位决定读/写/执行是否允许）、chmod 修改模式位、chown 修改属主与组、umask 设置默认屏蔽掩码：九位分主组客三档（属主/组/其他人），按 uid-gid 选择主-组-客三档并位移后做子集判断；root 默认全通过，仅无执行位不可执行；组判定覆盖主组与补充组；chmod 须属主或 root，非属主组时清除 setgid 位；chown 须满足三项条件（须是文件属主/新 uid 须与旧 uid 相同即不转交/新组须是自己所属组）；umask 取反存储，新生文件模式掩码即默认屏蔽掩码——权限判定核心是档位选择。

前置阅读：`13-path-lookup.md`（路径解析）、`02-fproc-struct.md`（凭证结构）、`10-pm-protocol.md`（凭证设置的调用点）。

> 本章不讲什么：
> - 寻路与按 fd 取 vnode 的执行—— `13-path-lookup.md`/`14-filedes.md`（本篇只给权限判定的调用点）
> - 凭证字段的设置—— `10-pm-protocol.md`（`pm_setuid` 族，本篇只给判定所用的 id）
> - 凭证结构的定义—— `02-fproc-struct.md`（本篇只给 id 的选用）
> - 下发 FS 的执行—— `12-request-wrappers.md`（本篇只给 FS 请求 trait）

---

## 1 概念

### 1.0 引言与前置

权限判定回答三个问题：调用者是谁（调用者的 uid/gid，real 还是 eff 取决于调用场合）、文件是谁（文件的属主与九位模式）、要做什么（读/写/执行中的哪几位）。三组输入齐备后，先确定档位，再做子集判断，最后做只读挂载检查，判定结果即出。本章假设读者已知 fd 与 vnode 为何物（见 13/04），只讲“允许还是拒绝”。

### 1.1 为什么权限判定是按档位查表

九位分主组客三档：属主三位（位移 6）、组三位（位移 3）、客人三位（位移 0）。确定档位的方法是按 uid-gid 选择主-组-客三档并位移：是文件属主取位移 6，是同组取位移 3，是其他人取位移 0；取完与 7 相与，得到当前档位的三位。档位不是算术，是查表——九位中读哪三位，完全取决于调用者身份。补充组同样计入组档：主组不是，补充组是，同样取位移 3——主组与补充组两组并查，漏掉一组即误判。

### 1.2 root 的特权与边界

root 做权限判定，默认全通过（读/写/执行三位视为全有），执行有一条边界：目录恒可查找（对目录而言查找即执行位），非目录须至少有一位执行位——无执行位不可执行。特权是有边界的：root 能读不能执行无执行位的文件，是“最小惊奇”原则的体现——连 root 都受限的才是真例外。过期 id（-1）连权限检查点都不让进：属主已删除（删除中的用户），权限判定直接返回 `EACCES`。

### 1.3 主组与补充组的两组并查

主组与补充组：主组（单个 gid）与补充组（sgroups 数组）。判定组档时两组并查：主组命中即命中，补充组命中同样命中。`in_group` 即遍历补充组数组：逐项比较，命中即返回 `OK`，遍历完未命中返回 `EINVAL`（历史返回码：查无此组返回 `EINVAL`，判定层视为不在组中）。

### 1.4 chmod 的属主检查

修改模式位（chmod）须通过属主检查：文件属主或 root 方可修改，其余返回 `EPERM`；只读挂载检查随后：属主也不能修改只读挂载上的模式位（返回 `EROFS`）。修改后清除 setgid：非属主组时清除 setgid 位——借他人之组保留 setgid 修改模式位是提权通道，修改时即清除。以 FS 返回值为准：模式位以 FS 返回值为准，本地不重复计算。

### 1.5 chown 的禁止转交规则

修改属主（chown）须遵守禁止转交规则：先做只读挂载检查（只读挂载上直接拒绝），再检查 chown 三项条件（须是文件属主/新 uid 须与旧 uid 相同即不转交/新组须是自己所属组）。root 跳过三项条件，直接通过。-1 表示不修改（uid/gid 各记一个哨兵位，判定层视为 `None`）；超过 2^31-2 返回 `EINVAL`。修改后三项同步（uid/gid/模式位皆以 FS 返回值为准）。

### 1.6 umask 的取反存储

umask 存取皆取反：存储的是新掩码与 0777 相与后取反，返回的是旧掩码取反后的值。一个函数同时完成存储与返回，使配对可测——存取是同一逻辑的两面，拆成两个函数即有配对错位风险。掩码是新生文件的默认屏蔽掩码（新生文件模式掩码），屏蔽哪九位，全在此取反值中。

### 1.7 与其他 OS 的权限对照

- **Linux** 以 `inode_permission` 实现同构逻辑：属主/组/客人按位移取档（`MAY_READ/WRITE/EXEC` 子集判断）、root 特例（`capable(CAP_DAC_OVERRIDE)` 跳过档位，`execute_ok` 无执行位不可执行对应 1.2 的边界）、只读挂载检查在最后（`mnt_may_suid`/`SB_RDONLY` 对应只读检查在最后）；补充组对应 `in_group_p`。
- **Redox** 的权能以持有即有权对应档位查表；Redox 以方案内属主检查对应属主检查与禁止转交规则。
- **seL4** 无权限位，准入即能力有无——档位查表在 seL4 中由能力推导替代：有能力者恒准，无需九位。

### 1.8 小结

权限判定分四段（权限判定→确定档位→子集判断→只读检查），三组条件（调用者身份/文件档位/只读挂载是否只读）决定流程走向，一条不变量贯穿始终：每步判定都有依据——属主检查有依据（文件属主记录），档位选择有依据（九位模式原文），子集判断有依据（位运算），只读检查有依据（挂载标志）。每步判定都有依据，是本篇的核心要求，权限判定核心是档位选择。

---

## 2 C 源码分析

### 2.1 头注释契约（`protect.c:1-9`）

四调用入口（`5-9`：chmod 兼 fchmod、chown 兼 fchown、umask、access）。

### 2.2 `do_chmod` 流程（`protect.c:25-92`）

`do_chmod`：配置解析（`42-44`，全路径读挂载写结点）→ 按路径与按 fd 分流（`46-60`：按路径则复制路径并打开 vnode，按 fd 则按 fd 取 vnode 并复引 `dup_vnode`）→ 断言 vnode 存在（`62`）→ 属主或 root 检查（`67-68`）→ 只读检查（`69-70`，顺序在属主检查之后）→ 修改后清除 setgid（`72-81`：非属主组时清除 setgid 位 + 下发 FS + 同步 FS 返回的模式位）→ 分别释放并归还（`83-90`：按路径则释放 vnode 与只读挂载引用，按 fd 则释放 filp）→ 返回（`91`）。

### 2.3 `do_chown` 流程（`protect.c:98-177`）

`do_chown`：取调用者主组（`115-116`）→ 按路径与按 fd 分流（`118-138`）→ 只读检查先行（`140`，顺序在属主检查之前，与 chmod 相反）→ chown 三项条件（`145-150`：非 root 则检查须是文件属主/新 uid 须与旧 uid 相同即不转交/新组须是自己所属组）→ -1 保留不修改（`155-156`）→ 越界检查（`158-159`）→ 下发 FS 并同步（`160-165`：uid/gid/模式位三项以 FS 返回值为准）→ 分别释放并归还（`168-175`）。

### 2.4 `do_umask` 流程（`protect.c:182-192`）

`do_umask`：取新掩码（`187`）→ 取旧掩码取反值（`189`）→ 取反存储（`190`，`new & RWX_MODES` 后取反）→ 返回旧掩码取反值（`191`）。

### 2.5 `do_access` 流程（`protect.c:198-232`）

`do_access`：取请求模式位（`210`）→ 配置解析（`212-214`，读锁对）→ 模式位检查（`217-218`：超出 R/W/X 且非 F_OK 即 `EINVAL`）→ 打开 vnode（`221-223`）→ 调用判定（`225`，`forbidden`）→ 释放并归还（`227-230`）。

### 2.6 `forbidden` 判定流程（`protect.c:238-287`）

`forbidden`：过期 id 检查（`251`，属主 id 为 -1 即 `EACCES`）→ 按调用选择 real/eff（`255-256`，ACCESS 用 real，其余用 eff）→ root 特例（`258-266`：目录或任一执行位即全通过，否则仅读写）→ 确定档位并位移（`268-272`：属主 6/组 3（含补充组 `270`）/客人 0，取三位）→ 子集判断（`276-277`，请求位须被档位包含）→ 只读检查在最后（`282-284`，写操作遇只读挂载即 `EROFS`）→ 返回（`286`）。

### 2.7 `read_only` 流程（`protect.c:292-302`）

`read_only`：断言 vnode 存在（`300`）→ 存在只读挂载引用且只读标志即 `EROFS`（`301`，无只读挂载即 `OK`）。

### 2.8 `in_group` 流程（`utility.c:128-141`）

`in_group`：遍历补充组数组（`132-134`，numeric for）→ 命中即 `OK` → 遍历完未命中即 `EINVAL`（`136`，历史返回码：查无此组返回 `EINVAL`，判定层视为不在组中）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `protect.c` 的位运算，而是吸收 Linux/Redox 的权限判定模型后做取舍。以下决策对应 `.design/29-design.v1.md` D1-D7。

### D1 调用枚举

- **C**：头注释四入口，chmod/chown 各兼 f 变体（`protect.c:1-9,46-60,118-138`）。
- **Rust**：`ProtectCall` 六值 + `by_path()`（`os/servers/vfs/src/protect.rs:69,84`）。
- **为什么**：兼任以调用号分值，枚举使分流可穷举。替代方案（四值 + 布尔）被否决：兼任是调用面的事实（27-D1 同源惯例）。

### D2 chmod 与 chown 的属主检查

- **C**：chmod 属主检查后做只读检查（`67-70`）/ chown 只读检查后做三项条件检查（`140-151`）。
- **Rust**：`chmod_gate()` + `chown_gate()`（`os/servers/vfs/src/protect.rs:95,110`）。
- **为什么**：顺序差异是关键（chmod 先检查属主、chown 先检查只读挂载）；三项条件原因各不同，枚举原因才可讲清。替代方案（三布尔与）被否决：四种错误同为 EPERM 但原因各不同。

### D3 setgid 清除与 id 边界检查

- **C**：清除 setgid 位 + -1 保留不修改 + 越界检查（`75-76,154-159`）。
- **Rust**：`strip_setgid()` + `keep_id(Option)` + `check_id_bounds()`（`os/servers/vfs/src/protect.rs:138,150,155`）。
- **为什么**：-1 表示不修改哨兵，建模为 `None`（C 的 `(uid_t)-1` 直译即魔法数——模式 17 反例）。替代方案（调用点分散书写）被否决：清除逻辑分散则容易漏清除。

### D4 umask 取反存储

- **C**：取旧值取反后存储新取反值并返回旧取反值（`189-191`）。
- **Rust**：`umask_swap()` 返回 `(stored, returned)`（`os/servers/vfs/src/protect.rs:166`）。
- **为什么**：存储与返回是同一逻辑的两面；实现中亲手验算纠正过存返颠倒（测试锁定）。替代方案（两个函数）被否决：配对拆散即有错配风险。

### D5 access 模式检查

- **C**：超出 R/W/X 且非 F_OK 即 EINVAL（`217-218`）。
- **Rust**：`R_OK/W_OK/X_OK/F_OK` 位旗 + `check_access_mode()`（`os/servers/vfs/src/protect.rs:45,174`）。
- **为什么**：F_OK（0）是“只查存在”的空集；R/W/X 请求位与 R/W/X 模式位同值不同域（注记在常量处）。替代方案（裸 u32 检查）被否决：位旗使四位穷举可测。

### D6 权限判定核心

- **C**：五段判定 + 补充组遍历（`238-287` + `utility.c:128-141`）。
- **Rust**：`ForbidInput` + `forbidden_decision()` + `in_supplementary()`（`os/servers/vfs/src/protect.rs:188,194,229`）。
- **为什么**：五段是权限判定核心是档位选择的具体展开；real/eff 选择内化（ACCESS 用 real 之 255-256 一测即知）；补充组以切片传入（C 的 OK/EINVAL 二值返回布尔化）。替代方案（五个函数链）被否决：链式早返即本函数，拆分反增调用成本。

### D7 只读检查与 FS 对话

- **C**：`read_only` + req 返回值同步（`292-302,78-80,160-164`）。
- **Rust**：`readonly_gate()` + `ProtectFs{chmod, chown}`（`ScriptedProtect` 按脚本同步返回值 vs `RefusingProtect` 常拒）+ `chmod_propagates()` 契约探针（`os/servers/vfs/src/protect.rs:267,276,290,305,351,369`）。
- **为什么**：只读检查在最后是“只读挂载不可写”的要求；返回值同步是“以 FS 返回值为准”的要求（本地不重复计算）。替代方案（本地计算 mode）被否决：与 C 相悖（P0 级偏移）。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | 锁配对转借用注记；权限判定无锁 | `protect.rs:290` 模块注记 + 本文档 D1/D2 + 29 正文 §1.1 |
| A-5 SUSPEND/revive 显式化 | 本篇无 SUSPEND（路径解析与判定及下发 FS 皆同步）；判定结果唯 Done | `protect.rs:375` + 本文档 D1 + 29 正文 §1.1 |
| A-18 档位选择查表类型化（位算→枚举检查） | 6/3/0 位移即查表 | `protect.rs:229` + 本文档 D6 + 29 正文 §1.1 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── protect.rs              — 本篇：调用枚举/属主检查/setgid 清除/掩码/模式检查/权限判定/只读判定
├── fproc.rs                — 凭证结构对照（02，id 选用层）
├── link.rs                 — SU_UID 复用（27，同源常量）
└── minix-types             — Uid/Gid/Mode/errno 值（types，域类型层）
```

> 设计决策：§3 D1（调用枚举）/ D2（chmod 与 chown 的属主检查）/ D6（权限判定核心）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| RWX 位 | `const.h:117-119` | `protect.rs:32,38` | 421 八进制 |
| access 请求位 | `unistd.h:168-171` | `protect.rs:45` | R/W/X/F（F 空集） |
| set 位/掩模/界 | `const.h:112-116`/`syslimits.h:53,60` | `protect.rs:54,60` | set/777/2^31-2 |
| 六调用 | `protect.c:1-9` | `protect.rs:69,84` | 按路径与按 fd 分流 |
| chmod/chown 属主检查 | `protect.c:64-70,140-151` | `protect.rs:95,110` | 顺序差异 + 三项条件 |
| 清除与边界三项 | `protect.c:75-76,154-159` | `protect.rs:138,150,155` | 清除/保留/边界 |
| umask 取反存储 | `protect.c:189-191` | `protect.rs:166` | 存返配对 |
| 模式检查 | `protect.c:217-218` | `protect.rs:174` | 空集与越界 |
| 权限判定核心 | `protect.c:238-287` | `protect.rs:188,194,229` | 五段 + 切片补充组 |
| 只读检查 | `protect.c:292-302` | `protect.rs:267` | 无只读挂载即通过 |
| FS 对话 | `request.h` req 族 | `protect.rs:276,290,305,351,369` | 脚本/常拒双实现 + 探针 |
| 错误族 | `protect.c` 全文件 | `protect.rs:385,398 ProtectError::to_errno` | 5 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| chmod 属主检查（属主或 root） | `chmod_gate` | 属主检查在前 | `protect.c:67` |
| chown 不转交（须是文件属主/新 uid 须与旧 uid 相同即不转交/新组须是自己所属组） | `chown_gate` 三项条件 | 三项条件全过 | `protect.c:147-149` |
| umask 取反存储（掩码内外有别） | `umask_swap` 配对 | 存返同测 | `protect.c:189-191` |
| 子集判断（请求位须被包含） | `(perm\|want)==perm` | 位运算 | `protect.c:277` |
| 只读检查在最后（五段之末） | 五段之末 | 写操作遇只读挂载 | `protect.c:282-284` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **313 passed / 0 failed**（既有 307 + 本篇新增 6；`minix-types` 独立）。
> 本章直接影响 6 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_calls_and_owner_doors` | `protect.c:1-9,64-70,140-151` | 六调用 + 属主检查 | `protect.rs:416` |
| `test_small_change_and_mask` | `protect.c:75-76,154-159,189-191` | 清除与边界三项 + 取反存储配对 | `protect.rs:458` |
| `test_asking_mode_door` | `protect.c:217-218` | 空集与越界 | `protect.rs:478` |
| `test_verdict_core` | `protect.c:238-287` + `utility.c:128-141` | 五段矩阵 + 补充组 | `protect.rs:489` |
| `test_fs_dialogue` | `request.h` req 族 | 脚本/常拒 + 契约探针 | `protect.rs:588` |
| `test_errno_map_covers_protect_c` | `protect.c` 全文件 | 5 变体→errno + 位域 | `protect.rs:608` |

测试策略：调用以六值全枚举锁定；属主检查以顺序差异 + 三项条件覆盖；setgid 清除以清除/保留/边界覆盖；掩码以存返配对覆盖（含存返颠倒纠错）；模式检查以空集/越界覆盖；权限判定以过期/root/档位四形/子集/只读矩阵覆盖；FS 以脚本/常拒 + 探针覆盖；错误以 5 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**313 passed / 0 failed**
- 本节列出与本模块直接相关的 6 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/protect.rs`

---

## 6 过渡

本篇在 13（寻路）之后、28（stat）之前，是权限判定的归属层：13 只管路径解析到 vnode，27 只管目录项变更，本篇管变更之前的权限判定（chmod/chown 权限判定）；没有本篇，28 的 stat 不知权限有无，30 的锁不知检查点开合。

```
13-path-lookup: eat_path 路径解析（路径解析到 vnode）
   │
27-link: 目录项变更 ───────────────┐
                                  ├─► 本篇：chmod_gate 属主检查 → forbidden_decision 确定档位
02-fproc-struct: 凭证结构 ─────────┘   → readonly_gate 只读挂载检查 → ProtectFs 下发 FS
           │                              │
           ├─► 28-stadir：stat 族的执行（权限有无的下一站）
           └─► 30-fcntl-lock：锁检查点（检查点开合的下一站）
```

阅读顺序提示：若关心“权限有无的下文”，下一站 `28-stadir.md`（stat 族的执行）；若关心“锁检查点”，再下一站 `30-fcntl-lock.md`（fcntl 与记录锁）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/protect.c:1-302`（七函数全族）、`minix3/minix/servers/vfs/utility.c:128-141`（`in_group` 补充组）、`minix3/minix/include/minix/const.h:112-119`（`I_SET_*/RWX_MODES/RWX 位`）、`minix3/sys/sys/unistd.h:168-171`（`R/W/X/F_OK`）、`minix3/sys/sys/syslimits.h:53,60`（`UID/GID_MAX`）
- 阶段文档：`13-path-lookup.md`（路径解析执行）、`02-fproc-struct.md`（凭证结构）、`10-pm-protocol.md`（凭证设置）、`28-stadir.md`（下一站）、`30-fcntl-lock.md`（下下一站）、`09-main-loop.md`（调用分发）
- Rust 实现：`os/servers/vfs/src/protect.rs:1`（本篇判定层）、`os/servers/vfs/src/link.rs:35`（`SU_UID` 复用）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（名拷语义）
