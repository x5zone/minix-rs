# 17-rs-state-data: LU 状态数据迁移

> **分类**: 阶段 6 — Live Update（状态数据迁移）
> **源码**: `minix3/minix/servers/rs/manager.c:174-284`（`init_state_data`）、`minix3/minix/servers/rs/request.c:805-844`（`do_update` 的 state data/grants 调用点）、`minix3/minix/servers/rs/update.c:135-163`（`rupdate_upd_clear` 的 revoke）、`minix3/minix/servers/rs/type.h:30-42`（`struct rprocupd`）、`minix3/minix/include/minix/rs.h:58-59,88-100`（`rs_ipc_filter_el`/`rs_state_data`）、`minix3/minix/include/minix/ipc_filter.h`（`IPCF_*`/`ANY_*`）、`minix3/minix/include/minix/sef.h:213-232`（`SEF_LU_STATE_*`）
> **Rust 模块**: `os/servers/rs/src/state_data.rs`（`IpcfFlags`/`SourceIpcFilterEl`/`IpcFilterEl`/`validate_state_data_size`/`validate_eval`/`num_ipc_filter_blocks`/`ipcf_els_buff_size`/`parse_label`/`parse_filter_el`/`vm_fallback_entry`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md`（prepare 阶段调用点）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`rprocupd` 数据形状）
> **说明**: 本文档是 LU 状态迁移机制：prepare 阶段把旧实例的"状态"（eval 表达式 / IPC filter 表 / 自定义数据）复制给新实例。它依赖 19（`ds_retrieve_label_endpt`/`cpf_*`/`sys_datacopy` 外部契约）与 18（RS 自升级的 `cpf_reload`）——本文档只落地**数据形状、纯校验与 label 解析**（`state_data.rs`）。

---

## 1. 概念：把旧实例的"记忆"交给新版本

### 1.0 章节引言

Live Update 的新实例是**从零 fork 出来的**：它的代码段是新的，但运行状态（内存中的工作数据结构、已建立的 IPC 过滤规则）是空的。如果直接切换，新实例就像失忆的人——不知道自己在服务谁、该接收谁的消息。LU 的 prepare 阶段因此要先做**状态迁移**：旧实例（或服务自身）把关键状态整理成数据包，RS 把它复制给新实例。

> **本章不讲什么**（机制一律移交）:
> - LU 状态机主体（`16-rs-live-update.md`）——prepare/update/init/end 四阶段
> - IPC filter 的内核机制（`../01-stage-kernel/23-ipc-filter.md`）——`ipc_filter` 如何在内核生效
> - DS 查询 / cpf grants / 内存拷贝（`19-rs-external-interfaces.md`）——`ds_retrieve_label_endpt`/`cpf_grant_direct`/`sys_datacopy`
> - RS 自升级的 `cpf_reload`（`18-rs-self-lifecycle.md`）——reload 是 18 的机制

### 1.1 为什么需要状态迁移（WHY）

新实例要能无缝接管，至少需要两类状态：

1. **IPC 过滤规则**：RS 通过 `sys_privctl(SYS_PRIV_ALLOW_IPC)` 给每个服务配置"谁能给我发消息"的过滤表（04/23）。新实例是新的 endpoint，过滤规则必须**随实例迁移**，否则内核会拒绝合法消息（或放行非法消息）。
2. **应用自定义状态**：服务在 prepare 回调里可以把内部状态序列化（如 eval 表达式、自定义二进制数据），RS 原样复制。

迁移的粒度是"每服务一个 `rs_state_data` 包"，挂在 rpupd 描述符上（`type.h:38` 的 `prepare_state_data`）。

### 1.2 三种状态载体（WHAT）

`struct rs_state_data`（rs.h:93-100）描述一次迁移的内容载体（eval / IPC filter 两种）与整包 grant 传递通道：

| 载体 | 数据形状 | 何时使用 | 迁移方式 |
|------|---------|---------|---------|
| eval 表达式 | `eval_addr` + `eval_len`（字符串） | `prepare_state == SEF_LU_STATE_EVAL` | `sys_datacopy` 复制 + NUL 结尾（manager.c:196-211） |
| IPC filter 表 | `ipcf_els`（每块 `IPCF_MAX_ELEMENTS` 个元素）+ `ipcf_els_size` | 服务声明了过滤规则 | 逐块 datacopy + label→endpoint 解析（manager.c:215-271） |
| 整包 grants（传递通道） | `size`（整包大小，成功迁移后 = 56） | 任何成功迁移（eval 或 filter，`size > 0`） | 整包 `cpf_grant_direct(CPF_READ)` 授予新实例（request.c:823-827）；`ipcf_els`/`eval_addr` 缓冲另 grant（request.c:829-845） |

注意：C 里没有独立的"自定义数据"载体——迁移内容只有 eval 表达式与 IPC filter 表两种；`cpf_grant_direct` 的 grants 是**传递通道**：RS 把状态包所在的地址以 grant 形式授权给新实例（19），新实例在 `SEF_INIT_LU` 初始化时自行读取（12）。rs_state_data 本身是 RS 进程内的中间表示。

---

## 2. C 源码分析

### 2.1 init_state_data 总流程（manager.c:174-284）

```
dst 清零（size/eval/ipcf 全部置 0，185-189）
  │
  ▼ ① 整包大小校验：src.size != sizeof(struct rs_state_data) → E2BIG（190）
  ▼ ② eval 表达式迁移（prepare_state == SEF_LU_STATE_EVAL）：EINVAL/ENOMEM/datacopy（196-211）
  ▼ ③ IPC filter 块数校验：ipcf_els_size % rs_ipc_filter_size → E2BIG（215-216）
  │     块大小 = sizeof(rs_ipc_filter_el) × IPCF_MAX_ELEMENTS（manager.c:181）
  ▼ ④ 无 filter 表 → 提前返回 OK（220-221）
  ▼ ⑤ 分配目标缓冲（VM 多一块，224-227；malloc 失败 → ENOMEM，228-231）
  ▼ ⑥ 逐块逐元素解析：flags 截止 → label→endpoint → 写入（233-270）
  ▼ ⑦ src==VM → 追加 VM 保底条目（273-277）
  ▼ ⑧ 写回 dst 的 size/ipcf_els/ipcf_els_size（279-281）
```

### 2.2 数据结构

**源形状 `struct rs_ipc_filter_el`**（rs.h:88-92，服务侧声明）：
```c
struct rs_ipc_filter_el {
    int flags;                  /* IPCF_* 标志 */
    char m_label[RS_MAX_LABEL_LEN]; /* 消息源：DS label / ANY_* / 十进制 endpoint */
    int m_type;                 /* 消息类型（RS_* / VM_* …） */
};
```
`RS_MAX_LABEL_LEN = 16`（rs.h:58），`RS_MAX_IPCF_STR_LEN = 16+12 = 28`（rs.h:59，十进制 endpoint 字符串上限）。

**目标形状 `ipc_filter_el_t`**（ipc_filter.h，内核格式）：`{ int flags; endpoint_t m_source; int m_type; }`——label 已被解析成 endpoint。

**`struct rs_state_data`**（rs.h:93-100）：`size`（整包大小，= `sizeof(struct rs_state_data)` = **x86-64 目标布局 56 字节**；本树 C 为 i386，其 sizeof = 28，见 §3.2 ARCH A-14）、`ipcf_els`/`ipcf_els_size`、`ipcf_els_gid`、`eval_addr`/`eval_len`、`eval_gid`。

**rprocupd 挂载点**（type.h:38-39）：`prepare_state_data`（状态包）+ `prepare_state_data_gid`（整包 grant）。

### 2.3 eval 表达式迁移（manager.c:196-211）

- 前置：`SEF_LU_STATE_EVAL`（sef.h:217）时 `eval_len == 0 || eval_addr == NULL` → **EINVAL**；
- `malloc(eval_len+1)` 失败 → **ENOMEM**；
- `sys_datacopy(src_e, eval_addr, SELF, dst, eval_len)` 复制 + 尾部补 `'\0'`；
- 成功则 `dst.size = src.size`。

eval 表达式用于"应用状态较复杂时用表达式描述"（如"等待所有请求完成"），由新实例在 init 时求值（12）。

### 2.4 IPC filter 解析（manager.c:215-271）

**块数校验**：`src.ipcf_els_size % rs_ipc_filter_size != 0` → **E2BIG**，其中 `rs_ipc_filter_size = sizeof(rs_ipc_filter)` = `sizeof(rs_ipc_filter_el) × IPCF_MAX_ELEMENTS`（manager.c:181）。`IPCF_MAX_ELEMENTS = NR_SYS_PROCS × 2 = 128`（ipc_filter.h；`NR_SYS_PROCS = 64`，config.h:32 + sys_config.h）。

**目标缓冲**：`sizeof(ipc_filter_el_t) × IPCF_MAX_ELEMENTS × num_ipc_filters`，VM 额外一块（manager.c:224-227）。

**逐元素解析**（manager.c:233-271）：`for j in 0..IPCF_MAX_ELEMENTS` 且 `rs_ipc_filter[j].flags != 0`（flags 为 0 即表尾）；`m_source` 的解析四步（manager.c:246-263）：

1. `ds_retrieve_label_endpt(label, &m_source)` 成功 → 用 DS 查询到的 endpoint（19）；
2. label 为 `"ANY_USR"` → `ANY_USR`；`"ANY_SYS"` → `ANY_SYS`；`"ANY_TSK"` → `ANY_TSK`；
3. 否则 `strtol(label, &buff, 10)`——**全串必须是十进制数**且无溢出（`errno`）→ endpoint；
4. 任一失败 → **ESRCH**。

`m_type` 仅当 `IPCF_MATCH_M_TYPE` 置位时从源元素取值（否则 0）；`m_source` 仅当 `IPCF_MATCH_M_SOURCE` 置位时解析（否则 0）。

### 2.5 VM 保底条目（manager.c:273-277）

`src_e == VM_PROC_NR` 时，在解析出的 filter 表末尾追加一条**固定保底规则**：

```c
ipcf_els_buff[i][0].flags = (IPCF_EL_WHITELIST|IPCF_MATCH_M_SOURCE|IPCF_MATCH_M_TYPE);
ipcf_els_buff[i][0].m_source = RS_PROC_NR;
ipcf_els_buff[i][0].m_type = VM_RS_UPDATE;   /* VM_RQ_BASE+41 = 0xC29，com.h:736 */
```

保证 VM 在更新期间仍能向 RS 发 `VM_RS_UPDATE`（VM 是 LU 的参与者，规则解析后可能把自己过滤掉——必须保底放行）。

### 2.6 调用点与 grants 生命周期

- **调用点**：`do_update` 建新实例后调 `init_state_data`（request.c:814）；失败 → `rupdate_upd_clear` + 透传错误码。
- **整包 grant**：`prepare_state_data.size > 0` 时 `cpf_grant_direct(rpub->endpoint, &state_data, size, CPF_READ)`（request.c:823-827）；ipcf_els/eval_addr 缓冲单独再 grant（request.c:829-845）。任一 `GRANT_INVALID` → ENOMEM。
- **revoke**：`rupdate_upd_clear` 在清理描述符时 revoke 三个 grant + free 缓冲（update.c:135-163）。
- **RS 自升级**：`main.c:464` 的 `cpf_reload()` 是 RS 自身更新的特殊路径（18）。

---

## 3. Rust 设计决策

### 3.1 state_data.rs 纯切片

与 16 同款：IPC/内存面（`sys_datacopy`/`malloc`/`cpf_grant_direct`/`cpf_revoke`/`ds_retrieve_label_endpt`）归 19，RS 自升级 `cpf_reload` 归 18，`init_state_data` 的编排归 16（request.c:814 调用点）；`state_data.rs` 拥有**数据形状、纯校验与 label 解析**：

| Rust | C 锚点 | 语义 |
|------|--------|------|
| `IpcfFlags` bitflags | ipc_filter.h | `IPCF_*` 四标志 |
| `SourceIpcFilterEl`/`IpcFilterEl` | rs.h:88-92 / ipc_filter.h | label 形状 → endpoint 形状 |
| `validate_state_data_size` | manager.c:190 | `size != sizeof(rs_state_data)` → `E2BIG` |
| `validate_eval` | manager.c:196-198 | EVAL 前置 → `EINVAL` |
| `num_ipc_filter_blocks` | manager.c:215-216 | 块数校验 → `E2BIG` |
| `ipcf_els_buff_size` | manager.c:224-227 | VM 追加块 |
| `parse_label(label, ds_lookup)` | manager.c:246-263 | 四步解析 → `ESRCH` |
| `parse_filter_el` | manager.c:240-270 | MATCH 门控 |
| `vm_fallback_entry` | manager.c:273-277 | VM 保底条目 |

### 3.2 数据形状建模

C 的字节布局按 **x86-64 目标**（项目重写目标，见 recovery.rs 的 `sizeof(long)*8 = 64 on x86-64` 先例）作为**协议常量**建模：`sizeof(struct rs_state_data)=56`、`sizeof(rs_ipc_filter_el)=24`、`sizeof(ipc_filter_el_t)=12`（本树 C 源码为 i386，其中 `rs_state_data` 在 i386 下 sizeof = 28；`rs_ipc_filter_el`/`ipc_filter_el_t` 无指针，两架构同为 24/12）。Rust 结构体不做 C-layout 对齐（wire 拷贝在 19 落地），但块大小校验（E2BIG 门）必须用 C 的字节语义，故以常量表表达：

```rust
pub const RS_STATE_DATA_SIZE: usize = 56;        // manager.c:190 的 sizeof 门
pub const RS_IPCF_FILTER_EL_SIZE: usize = 24;    // 4 + 16 + 4
pub const RS_IPCF_FILTER_BLOCK_SIZE: usize = IPCF_MAX_ELEMENTS * RS_IPCF_FILTER_EL_SIZE; // 3072
pub const IPCF_EL_SIZE: usize = 12;              // sizeof(ipc_filter_el_t)
```

`SourceIpcFilterEl` 用 `&str` 表达 label（纯决策模型），`IpcFilterEl` 用 `Endpoint` 表达 `m_source`。`m_source` 未解析时的默认值：C 为 0（PM 端点），Rust 用 `Endpoint::NONE`——字段仅在 `IPCF_MATCH_M_SOURCE` 置位时有效，安全默认不引入歧义（ARCH A-14，见 §4.2）。

> **R11（2026-08-16）label UTF-8 契约**：C 的 `m_label[RS_MAX_LABEL_LEN]` 是原始字节（rs.h:90），
> Rust `SourceIpcFilterEl.m_label: &'a str` 要求 UTF-8——**决策为保持 `&str` + 边界 fail-closed**：
> 19 消息边界用 `from_utf8` 校验，非 UTF-8 label 拒绝该请求。理由：(1) DS label 语义是服务名字符串
> （Minix3 实践中为 ASCII），无真实非 UTF-8 场景；(2) `parse_label` 的十进制解析用
> `str::parse::<i32>()`（整串消费 + 溢出失败，等价 manager.c:260-263 的 strtol 双检查），改字节需
> 手写 strtol，属 translate 反模式；(3) 拒绝方向安全（不静默错配）。代码标注：
> `state_data.rs` `SourceIpcFilterEl`/`parse_label` 注释。

> **[ARCH: A-14]** — 状态数据协议常量与安全默认，三处一致标注（doc/design/code）。行为对照点：`manager.c:190`（sizeof 门）、`manager.c:240-270`（MATCH 门控 + 解析）、`request.c:823-827`（整包 grant）。三个子项：
> 1. **`RS_STATE_DATA_SIZE=56` 取 x86-64 目标布局**（本树 C 为 i386，`sizeof(struct rs_state_data)` = 28；目标端口为 64 位，wire 常量取 56）；
> 2. **`m_source` 未设 `MATCH_M_SOURCE` 时**：C 写 0（PM 端点），Rust 用 `Endpoint::NONE`（安全默认，字段仅在门控时有效）；
> 3. **空 label fail-closed**：C `strtol("")` 返回 0 且无 errno → `m_source` = 0（PM）被接受；Rust `parse::<i32>("")` 失败 → `ESRCH`（拒绝空 label，不沿袭 C 的 strtol 空串→0）。
> design：本文档 §3.2/§4.2（design 快照已同步标注）；code：`state_data.rs` 常量/结构/`parse_label` 注释。

### 3.3 label 解析

`ds_retrieve_label_endpt` 是 19 的外部契约——`parse_label` 以 **hook 参数** `ds_lookup: impl Fn(&str) -> Option<Endpoint>` 注入，纯切片内只做确定性决策：

1. `ds_lookup(label)` 命中 → 该 endpoint；
2. `"ANY_USR"`/`"ANY_SYS"`/`"ANY_TSK"` → `Endpoint::from_generation_slot(1..3, slot(ANY))`；
3. `label.parse::<i32>()` 成功（全串消费 + 无溢出，对照 C 的 `strtol` + `errno` + `*buff=='\0'`）→ `Endpoint(n)`；**空串例外**：C `strtol("")` 无错误返回 0（→PM），Rust fail-closed 返回 `ESRCH`（ARCH A-14 子项 3）；
4. 否则 → `ESRCH`。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/state_data.rs`：

- 常量：`IPCF_MAX_ELEMENTS`/`RS_MAX_LABEL_LEN`/`RS_STATE_DATA_SIZE`/`RS_IPCF_FILTER_EL_SIZE`/`RS_IPCF_FILTER_BLOCK_SIZE`/`IPCF_EL_SIZE`/`SEF_LU_STATE_EVAL`/`VM_RS_UPDATE`/`ANY_USR`/`ANY_SYS`/`ANY_TSK`；
- `IpcfFlags`：四标志 bitflags；
- `SourceIpcFilterEl`/`IpcFilterEl`：数据形状；
- 校验与解析：`validate_state_data_size`/`validate_eval`/`num_ipc_filter_blocks`/`ipcf_els_buff_size`/`parse_label`/`parse_filter_el`/`vm_fallback_entry`。

### 4.2 关键不变量

1. **块大小整除**（manager.c:215-216）：`ipcf_els_size` 必须是 `RS_IPCF_FILTER_BLOCK_SIZE` 的整数倍，否则 `E2BIG`。
2. **MATCH 门控字段**（manager.c:243-246）：`m_source` 仅在 `MATCH_M_SOURCE` 时解析，`m_type` 仅在 `MATCH_M_TYPE` 时取值——未设时 C 为 0，Rust 用 `Endpoint::NONE`/0 默认（ARCH A-14 子项 2）。
3. **EVAL 前置**（manager.c:196-198）：`SEF_LU_STATE_EVAL` 且缺 `eval_addr`/`eval_len` → `EINVAL`；其他 prepare_state 不做 eval 迁移。
4. **VM 保底仅在 src==VM**（manager.c:273）：`vm_fallback_entry` 由调用方（16 的编排）在 `src_e == VM_PROC_NR` 时追加，模块只提供条目构造。
5. **label 解析失败即 ESRCH**（manager.c:246-263）：四步是**顺序回退链**——DS 查询失败降级到 ANY_*，ANY_* 不匹配降级到十进制 `strtol`，全部失败（含空串 fail-closed，ARCH A-14 子项 3）→ `ESRCH`。

---

## 5. 测试要点

`state_data.rs` 内测试（`cargo test -p minix-rs --lib state_data` → 9/9）：

1. `validate_state_data_size`：56 → OK；0/55/57 → `E2BIG`。
2. `validate_eval`：`SEF_LU_STATE_EVAL` + 缺 addr/len → `EINVAL`；齐全 → OK；非 EVAL → OK。
3. `num_ipc_filter_blocks`：0 → 0；3072 → 1；3071 → `E2BIG`；6144 → 2。
4. `ipcf_els_buff_size`：`(1, false)` = 12×128；`(1, true)` = 12×128×2（VM 追加块）。
5. `parse_label`：DS hook 命中；`ANY_USR`/`ANY_SYS`/`ANY_TSK`；十进制 `"2"` → `Endpoint::RS`、`"0"` → `Endpoint::PM`（strtol 0 是合法端点）；`"123x"`/`"abc"`/溢出/空串 → `ESRCH`（空串 = ARCH A-14 子项 3 的 fail-closed）。
6. `parse_filter_el`：MATCH 门控（source/type 各自解析与否）+ 默认值。
7. `vm_fallback_entry`：三字段断言（WHITELIST|MATCH_SOURCE|MATCH_TYPE、`Endpoint::RS`、`VM_RS_UPDATE`）。
8. `ANY_*` 端点值：`0xFC00`/`0x17C00`/`0x1FC00`（`_ENDPOINT(1..3, slot(ANY))`，`MAX_NR_TASKS=1023` 下 `ANY=0x7C00`）。
9. 常量表：`IPCF_MAX_ELEMENTS=128`、`SEF_LU_STATE_EVAL=4`、`VM_RS_UPDATE=0xC29`、`RS_MAX_LABEL_LEN=16`。

测试总数声明：本文档范围为 `state_data` 模块测试数（以该模块 `cargo test` 输出为准）。全局 `cargo test -p minix-rs --lib` 通过数随并行模块增长（见 12 §5 的累计值约定）。

---

## 6. 过渡

状态迁移是 LU 机制图的"数据面"：

- **调用方**：`do_update`（16）建新实例后调 `init_state_data`（request.c:814）——状态包挂在 rpupd 上随链进入 prepare；
- **外部契约**：`ds_retrieve_label_endpt`/`cpf_grant_direct`/`cpf_revoke`/`sys_datacopy` 在 **19-rs-external-interfaces** 落地——label 解析的 DS 查询、grants 的授权/撤销、内存拷贝都依赖它；
- **RS 自升级**：`cpf_reload`（main.c:464）在 **18-rs-self-lifecycle** 展开——RS 自身更新时重新加载 grants；
- **内核侧**：IPC filter 的生效机制在 `../01-stage-kernel/23-ipc-filter.md`——本 doc 只生产 filter 表数据。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— `do_update` 调用点（request.c:814）、rpupd 链、prepare 阶段
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` —— `UpdateChain`/`rprocupd` 数据形状
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` —— `ds_retrieve_label_endpt`/`cpf_*`/`sys_datacopy` 契约
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/18-rs-self-lifecycle.md` —— RS 自升级 `cpf_reload`/rollback 特例
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/23-ipc-filter.md` —— IPC filter 内核机制
- `minix3/minix/servers/rs/manager.c:174-284`、`request.c:805-844`、`update.c:135-163`、`include/minix/rs.h:58-59,88-100`、`include/minix/ipc_filter.h`、`include/minix/sef.h:213-232`、`include/minix/com.h:736` —— ground truth
