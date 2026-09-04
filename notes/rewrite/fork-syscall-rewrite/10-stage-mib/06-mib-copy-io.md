# 06 — 系统信息库拷贝输入输出：判断何种数据需要搬运、谁有权限触碰

> **分类**: 拷贝原语 / 数据搬运 verdict
> **源码**: `minix3/minix/servers/mib/main.c:64-258`（`mib_oldp/newp` + 全部拷贝/长度/relay 原语）、`minix3/minix/servers/mib/tree.c:371-420`（`mib_copyin_str`）、`minix3/minix/include/minix/safecopies.h:52-53,64-65`（grant 常量）
> **说明**: 字节怎么进出：旧槽钳制、新数据精确匹配、字符串分页猜、grant 转交。判什么动（verdict）与真的动（`sys_datacopy`/`cpf_grant_magic` 效果，A-12 transport）在此分家。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要读数据读写（09）和远程转交（12）的读者。前置知识：01（old/new 配对 verdict）、02（信封 lane）。"grant"现用现讲：内核发的"代取货单"，凭单 cross-address-space 取字节，不直接碰对方内存。

### 1.2 本章不讲什么

- `sys_datacopy`/`cpf_grant_magic` 怎么执行——transport（A-12，`minix-sys` 落地后），本篇只造 verdict。
- 读写 handler 怎么调这些原语——那是 09 的事。
- 转交的旅程（死亡检测/ERESTART）——那是 12 的事（本篇只判方向与在场）。
- 鉴权——那是 07 的事。

### 1.3 两条核心约束，决定全部设计

第一条：**旧数据接收缓冲区是上限，不是目标量**。`mib_copyout` 从不问"用户想要多少"，只问"缓冲区还剩多少"——取要写长度与剩余空间的较小值，超出部分静默截断（上层会报告完整长度并返回缓冲区太小，01 §2.4）。若用户未提供旧缓冲区（空指针）则完全不触碰，直接报告"已写完"——"没人要的字节无需准备"（这正是 `mib_inrange` 存在的理由）。

第二条：**新数据要么长度完全匹配，要么视为未提供**。`mib_copyin` 要求提供长度与期望长度精确相等——短了不是"少写一点"，而是"完全不同的数据"。半份提供在配对阶段（01）已被丢弃，到这里的都是完整提供的。

字符串是例外中的例外：系统控制调用的接口不带字符串长度（NetBSD 遗产），MINIX3 又没有内核态的字符串拷贝函数（用户态历来显式传长度，`tree.c:385-394` 注释）——于是按页猜测：每次拷贝到页边界，寻找字符串结束符，未找到则下一页。拷贝少了慢，拷贝多了可能触发不必要的缺页，页边界是兼顾点。地址为零直接返回参数错误（空指针不可能是字符串）。

授权转交是另一条路：本服务不当中转仓库，把用户区直接授权给远端服务——旧缓冲区授权为可写（远端服务往用户槽里写答案），新数据区授权为可读（远端服务读取用户写入的数据）。**方向从本服务的视角命名**："我授权你写入我的旧缓冲区"。授权创建失败返回参数错误，**永远不返回内存不足**（`main.c:208,236` 两处明示）——呼应 01 §1.4：缓冲区大小与内存分配是两套语义，不可混用。

### 1.4 小结

旧缓冲区按剩余空间截断、新数据要求长度精确、字符串按页猜测、转交按方向授权、失败统一返回参数错误。记住"判断是否需要搬运与真正执行搬运分开"，09 与 12 的效果代码就不会与判决逻辑搞混。

---

## 2 C 源码分析

### 2.1 旧侧四原语（`main.c:90-152`）

| 函数 | 位置 | 行为 |
|------|------|------|
| `mib_inrange(oldp, off)` | `:90-98` | NULL → FALSE；否则 `off < len`（"这段要准备吗"） |
| `mib_getoldlen(oldp)` | `:105-113` | NULL → 0；否则全长（"别拿它当读长度"，注释警告非常规用） |
| `mib_copyout(oldp, off, buf, size)` | `:120-141` | 空/越界 → 回 `size` 不动（`:130-131`）；否则钳制后 `sys_datacopy`，失败回错码，成功回 `size` |
| `mib_setoldlen(call, oldlen)` | `:147-152` | 错误时暂存上报长度（EEXIST 路，01 §2.4） |

注意 `copyout` 的返回值语义：**成功永远报 `size`（要写的），不是 `len`（动了的）**——动了多少由槽决定，报多少由调用者决定（上层拿它当完整长度）。

### 2.2 新侧三原语（`main.c:158-202`）

| 函数 | 位置 | 行为 |
|------|------|------|
| `mib_getnewlen(newp)` | `:158-166` | NULL → 0 |
| `mib_copyin(newp, buf, len)` | `:172-185` | NULL 或长度不等 → `EINVAL`；零长直接 OK；否则 `sys_datacopy` |
| `mib_copyin_aux(newp, addr, buf, len)` | `:191-202` | 断言非 NULL；零长 OK；从**已拷入数据里拿到的用户指针**再拷（`copyin_str` 的逐页器用它） |

`copyin_aux` 不判配对——调用者（`copyin_str`）保证 `newp` 有效，断言即契约（`:196`）。

### 2.3 字符串分页猜（`tree.c:371-420`）

循环：`chunk = min(PAGE_SIZE - addr%PAGE_SIZE, bufsize)`（`:398-400`）→ `copyin_aux` 拷一块（`:403`，失败即回）→ `memchr` 找 `\0`（`:406`，找到报 `len+pos+1`，`:408-409`，+1 含终结符）→ 没找着推进（`:413-415`）。`buf==NULL` 时往 scratch 拷（`:402`）——纯量长度模式。`bufsize` 耗尽还没见 `\0` → `EINVAL`（`:418-419`）：**装不下的字符串不是字符串**（调用者给的界就是定义）。

### 2.4 转交两原语（`main.c:204-252`）

`mib_relay_oldp(endpt, oldp, grantp, lenp)`：有关 → `cpf_grant_magic(endpt, old_endpt, addr, len, CPF_WRITE)`，无效 grant 回 `EINVAL`（`:218-219`）；无 → `GRANT_INVALID` + 0 长（`:221-224`）。`mib_relay_newp` 镜像，`CPF_READ`（`:235-252`）。两处注释同款："出错码**不能是** `ENOMEM`"（`:208`、`:236`）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 空指针变 `Option` | `NULL` 哨兵 + 分支 | `Option<u64>` 长度（`copy.rs:24,35`），`None` 即关 | C 式哨兵（模式 17）直译是第一 translate 味；`None` 让"关"在类型里，`in_range(None, _)==false` 无需注释 |
| D2 | 钳制变返回值 | 回 `size` + 副作用拷 | `CopySpan{xfer, report}`（`copy.rs:49,61`） | "动了多少"与"报多少"是两回事（§2.1）——结构体让两回事各有名字；transport 执行 `xfer`，上层上报 `report` |
| D3 | 分页猜变纯函数 | 循环 + `copyin_aux` 交织 | `next_chunk`（`copy.rs:112`：页界+缓冲钳制）+ `nul_size`（`:127`：+1 终结符） | 循环的"算块"与"拷块"解耦：算块纯可测（含 `addr%PAGE` 边界），拷块是 transport 效果；`None` 即"界尽"（`:418-419` 的 `EINVAL` 由调用者转） |
| D4 | grant 无效变 `None` | `GRANT_INVALID(-1)` 哨兵 | `RelayRegion{grant: Option<GrantId>, len}`（`relay.rs:51`），`relay_old/new`（`:63,78`） | 与 `minix-types` `GrantId` 文档约定（"Option::None at use sites"，`id.rs:74`）同构——哨兵只活在线上（`GRANT_INVALID` 常量保留供 wire 比对），逻辑层无哨兵 |
| D5 | 失败码变常量 | 注释"must not be ENOMEM"两处 | `RELAY_FAIL = EINVAL`（`relay.rs:96`）+ 注释链 01 §1.4 | 注释会撒谎，常量不会：调用者 `GrantOutcome::Failed → RELAY_FAIL`，想报 ENOMEM 得先改常量名 |
| D6 | 方向变枚举 | `CPF_WRITE`/`CPF_READ` 裸传 | `RelayDir::{Write, Read}` + `flag()`（`relay.rs:28,37`） | "旧授写、新授读"（§1.3）值得一个名字；`flag()` 是 wire 唯一出口，方向弄反编译不过（类型错位非值错位） |

替代方案及否决：`CopyTransport` trait（`copy_out/copy_in` 方法 + 真实现/测试 mock 双实现）——否决，真实现今天只能回 `ENOSYS` stub（P0-code-bug），mock 双实现为凑 Gate D-2 而设是模式 25（不必要抽象）；verdict-值（"判动"）先行，trait 等 transport 落地（A-12）时从 12 的真实需求长出来。

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/mib/src/io/
├── mod.rs    — 本篇：重导出
├── copy.rs   — 本篇：PAGE_SIZE / in_range / get_old_len / CopySpan+copyout_span /
│                get_new_len / check_copyin / next_chunk / nul_size
└── relay.rs  — 本篇：GRANT_INVALID / grant_valid / RelayDir+flag /
                 RelayRegion+relay_old/new / RELAY_FAIL
```

grant 常量（`CPF_READ=1`/`CPF_WRITE=2`）住 `minix-types` `id.rs`（`GrantId` 旁边，线值域）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 范围/长度 | `main.c:90-113` | `copy.rs:24,35` | 关→false/0 |
| 钳制 | `main.c:120-141` | `copy.rs:49,61` | 越界零动全报 |
| 新长度/精确拷 | `main.c:158-185` | `copy.rs:80,93` | 不等即 `EINVAL` |
| 分页/终结 | `tree.c:397-410` | `copy.rs:112,127` | 页界钳制；+1 终结 |
| grant 有效 | `safecopies.h:52-53` | `relay.rs:14,17` | `> -1` |
| 方向 | `main.c:216-217,241-242` | `relay.rs:28,37` | 旧写新读 |
| 转交在场 | `main.c:210-252` | `relay.rs:51,63,78` | 关→无效+零 |
| 失败码 | `main.c:208,236` | `relay.rs:96` | `EINVAL` 恒 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 拷不出槽 | `copyout_span` 钳制 | `:133-134` |
| 报数恒全 | `report=size` 无条件 | `:128,131` |
| 新数精确 | `check_copyin` 等值 | `:177-178` |
| 页界不跨 | `next_chunk` 取模 | `:398` |
| 失败非 ENOMEM | `RELAY_FAIL` 常量 | `:208,236` |

### 4.4 与 C 的差异说明（模式 72 CSSCM）

§2 四节 vs §4 两模块：`copyin_aux` 无独立 verdict（断言即契约，§2.2 声明——调用者保证，判无可判）；`sys_datacopy`/`cpf_grant_magic` 执行层移交 transport（A-12）。category：边界移交 + 设计决策（D1/D4 哨兵消除，行为等价）。

---

## 5 测试要点

> 基线：`cargo test -p minix-mib --lib`，本篇 7 个测试（4 copy + 3 relay）。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_in_range_and_len` | `main.c:90-113,158-166` | 关 false/0 + 边界 63/64 | `copy.rs` |
| `test_copyout_span_clamps` | `main.c:127-141` | 全/尾钳/越界零动/关 | `copy.rs` |
| `test_check_copyin_exact` | `main.c:177-181` | 等值/±1/关/零长 | `copy.rs` |
| `test_next_chunk_page_bounded` | `tree.c:397-410,418` | 页界/缓冲钳/耗尽/终结+1 | `copy.rs` |
| `test_grant_validity` | `safecopies.h:52-53` | -1 无效/0 有效 | `relay.rs` |
| `test_relay_directions` | `main.c:216-217,241-242` + `safecopies.h:64-65` | 旧写新读 + 值 2/1 | `relay.rs` |
| `test_relay_presence` | `main.c:210-252` | 关无效零/开带长/失败 EINVAL | `relay.rs` |

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-mib --lib`：**94 passed**（全 crate；其中本篇 7 个，见上表）
- `cargo test -p minix-types --lib`：**139 passed**（全 crate；CPF 常量由 relay 测试钉值）
- 本节上表列出与本篇直接相关的 7 个（子集；总数随阶段推进增长）

---

## 6 过渡

字节进出 verdict 就绪：钳制、精确、分页、授向。下一站是 07——鉴权：谁有资格写（`mib_authed` 缓存 + 四权限位），本篇的"怎么动"在那里加上"谁准动"。

## 7 参见

- C 源：`minix3/minix/servers/mib/main.c:64-258`、`minix3/minix/servers/mib/tree.c:371-420`、`minix3/minix/include/minix/safecopies.h:52-53,64-65`
- 阶段文档：`01-mib-init-main.md`（配对 verdict）、`02-mib-message-contract.md`（信封 lane）、`09-mib-data-access.md`（调用方）、`12-mib-remote-subtrees.md`（转交旅程）、`../07-stage-ds/02-ds-message-contract.md`（grant 先例对照）
- Rust 实现：`os/servers/mib/src/io/copy.rs`、`os/servers/mib/src/io/relay.rs`、`os/libs/minix-types/src/types/id.rs`（CPF 常量）
- 对端：A-12 transport（`minix-sys` 落地后接线）
