# 19-rs-external-interfaces: 外部接口契约

> **分类**: 阶段 7 — 外部接口（Rust 依赖面契约）
> **源码**: `minix3/minix/lib/libsys/*`（`sys_getinfo.c:9`、`sys_privctl.c:3`、`sys_kill.c:3`、`sys_statectl.c:3`、`ds.c:36,103,191`、`sched_start.c:46`、`sched_stop.c:9`、`pci_set_acl.c:15`、`pci_del_acl.c:15`）、`minix3/minix/include/minix/ipc.h:1048-1072,1420-1428,1466-1474,1858-1906`（消息槽）、`minix3/minix/servers/rs/*.c`（调用点）、`minix3/minix/include/minix/com.h:627,736,741-745`、`minix3/minix/include/minix/sef.h:85-95`
> **Rust 模块**: `os/libs/minix-types/src/ipc/rs.rs`（typed message payload views，ARCH A-2）、`os/libs/minix-sys/src/lib.rs`（stub API 契约）
> **前置**: 各机制文档（01~18）——本文档是它们的**外部调用点汇总**
> **说明**: 本文档是 RS 对外部世界的**依赖面契约**：sys_*（kernel）、srv_*（PM）、vm_*（VM）、ds_*（DS）、sched_*（SCHED）、libexec/mapdriver/PCI/devman。按 plan §3.3，它是**唯一**允许引用 `lib/libsys/*` 的文档；其余文档引用外部函数时一律指向本文档。Rust 侧落地 **消息槽类型化**（`ipc/rs.rs` 扩展）；`minix-sys` 的 sys_* 实现是 stub 契约（wire-up 时落地）。

---

## 1. 概念：RS 的外部依赖面

### 1.0 章节引言

RS 是用户态服务中**外部依赖最多的一个**：它要读内核信息（sys_*）、创建子进程（srv_*）、请求 VM 换页表（vm_*）、发布/查询标签（ds_*）、启动调度（sched_*）、加载 ELF（libexec）、注册驱动（mapdriver/PCI/devman）。每篇机制文档（01~18）都提到了这些调用，但它们的**签名与消息契约**集中在本文档——wire-up 阶段的锚点。

> **本章不讲什么**（机制一律移交）:
> - 各机制的语义（01~18 对应篇）——本文档只给签名/消息契约与调用点
> - `minix-sys` 的实现（stub 定义 API，wire-up 时实现）
> - 内核侧 syscall 的实现（`01-stage-kernel` 系列）

### 1.1 为什么需要独立契约文档（WHY）

1. **调用点分散**：同一个 `sys_privctl` 出现在 5+ 处（boot/update/utility/request），每篇机制文档都引它——签名必须单一锚点；
2. **消息槽是 C 联合体**：`mess_rs_*` 是裸 C struct，Rust 需要类型化视图（ARCH A-2）；
3. **Rust 依赖面**：`minix-sys`/`minix-types` 的 API 面由本契约定义，wire-up 按此实现。

### 1.2 依赖面总图（WHAT）

```
RS ── sys_getinfo/sys_getimage/sys_getmachine/sys_privctl/sys_getpriv ──► kernel
  ├── sys_setalarm(2)/sys_kill/sys_datacopy/sys_statectl/sys_diagctl_stacktrace/sys_whoami/sys_update
  ├── srv_fork/srv_kill/srv_execve + getnpid/getprocnr ─────────────────► PM
  ├── vm_memctl/vm_set_priv/vm_update/vm_prepare ──────────────────────► VM（VM_RS_UPDATE 消息）
  ├── ds_publish_label/ds_delete_label/ds_retrieve_label_endpt ────────► DS
  ├── sched_start/sched_stop ──────────────────────────────────────────► SCHED
  ├── libexec_load_elf/minix_stack_* ──────────────────────────────────► libexec（ARCH A-8）
  ├── mapdriver（mess_lsys_vfs_mapdriver） ────────────────────────────► VFS
  └── pci_set_acl/pci_del_acl + DEVMAN_BIND/UNBIND ────────────────────► PCI/devman
```

---

## 2. 外部面契约

> **锚点约定**：libsys 函数名在快照中可验证（如 sef_st.c:151 即 `sys_getpriv`），libsys 锚点用文件/行；**RS 调用点**是 ground truth——调用点行号优先。

### 2.1 sys_* 内核面

| 函数 | 签名 | 调用点 | 语义 |
|------|------|--------|------|
| `sys_getinfo` | `(request, ptr, len, ptr2, len2)` | main.c:181（`GET_HZ`） | 读系统频率 `system_hz`（01/07 依赖） |
| `sys_getimage` | `(struct boot_image *image)` | main.c:196 | 读 boot 映像表（01） |
| `sys_getmachine` | `(struct machine *machine)` | main.c:53 | 机器信息（01） |
| `sys_privctl` | `(proc_ep, request, void *p)` | utility.c:412（`SYS_PRIV_UPDATE_SYS`）、main.c:478/485（`SET_SYS`/`YIELD`）、update.c:360（`DISALLOW`） | 权限结构全操作（03/18） |
| `sys_getpriv` | `(&priv, proc_ep)` | utility.c:402、update.c:308/310 | 同步内核权限副本（18） |
| `sys_setalarm` | `(alarm, abs)` | main.c:433、main.c:540 | 周期 alarm（07） |
| `sys_setalarm2` | `(alarm, abs, &time_left, &uptime)` | utility.c:271 | 查询式 alarm（07，update 超时） |
| `sys_kill` | `(proc_ep, signr)` | manager.c:399（`SIGKILL`）、manager.c:1006 | 杀服务（15） |
| `sys_datacopy` | `(src_e, src_a, dst_e, dst_a, len)` | request.c:1122/1138、manager.c:142/162 | 跨进程拷贝（14/17） |
| `sys_statectl` | `(request, address, length)` | utility.c:266/294（`SYS_STATE_ADD_IPC_WL_FILTER`/`SYS_STATE_CLEAR_IPC_FILTERS`） | IPC filter 状态操作（17/23） |
| `sys_diagctl_stacktrace` | `(proc_ep)` | main.c:682 | 崩溃栈回溯（15） |
| `sys_whoami` | `(&ep, name, len, &priv_flags, &init_flags)` | update.c:342 | RS rollback 特例的身份判断（18） |
| `sys_update` | `(src_e, dst_e, flags)` | update.c:243 | 内核 slot 交换（16/18，`SYS_UPD_ROLLBACK`） |

### 2.2 srv_* PM 面

| 函数 | 签名 | 调用点 | 语义 |
|------|------|--------|------|
| `srv_fork` | `(uid, gid)` | main.c:446、manager.c:576 | 创建服务进程（10/18） |
| `srv_kill` | `(pid, signr)` | manager.c:470 | 杀服务进程（15） |
| `srv_execve` | `(proc_e, exec, exec_len, progname, ...)` | manager.c:634 | 服务 exec（09/10） |
| `getnpid` | `(endpoint)` | main.c:426 | endpoint→pid（02/19） |
| `getprocnr` | `(pid, &endpoint)` | main.c:451、manager.c:584 | pid→endpoint（10/18） |
| `waitpid` | `(-1, &status, WNOHANG)` | request.c:1063 | 收割僵尸（07 `do_sigchld`） |

### 2.3 vm_* VM 面

| 函数 | 签名 | 调用点 | 语义 |
|------|------|--------|------|
| `vm_memctl` | `(proc_ep, param, addr, length)` | request.c:779（`VM_RS_MEM_HEAP_PREALLOC`）、request.c:802（`VM_RS_MEM_MAP_PREALLOC`）、main.c:470（`VM_RS_MEM_PIN`） | RS 内存管理（10/16/18）；`VM_RS_MEM_*` 子操作见 com.h:741-745 |
| `vm_set_priv` | `(proc_ep, &vm_call_mask, grant)` | request.c:361、manager.c:698 | VM 调用掩码（03） |
| `vm_update` | `(src_e, dst_e, flags)` | update.c:249 | VM slot 交换（16/18）；`SF_VM_ROLLBACK` 位 |
| `vm_prepare` | `(src_e, dst_e, ...)` | update.c:505 | VM multi 预分配推进（16） |

`VM_RS_MEM_*` 子操作（com.h:741-745）：`PIN=0`/`MAKE_VM=1`/`HEAP_PREALLOC=2`/`MAP_PREALLOC=3`/`GET_PREALLOC_MAP=4`。

### 2.4 ds_* DS 面

| 函数 | 签名 | 调用点 | 语义 |
|------|------|--------|------|
| `ds_publish_label` | `(ds_name, endpoint, flags)` | manager.c:513、manager.c:800 | 发布服务标签（11） |
| `ds_delete_label` | `(ds_name)` | manager.c:878 | 撤销标签（11） |
| `ds_retrieve_label_endpt` | `(ds_name, &endpoint)` | manager.c:247（IPC filter label 解析，17）、manager.c:841（devman 查询） | label→endpoint |

### 2.5 sched_* 调度面

| 函数 | 签名 | 调用点 | 语义 |
|------|------|--------|------|
| `sched_start` | `(scheduler_e, schedulee_e, ...)` | utility.c:375 | 启动服务调度（10/12） |
| `sched_stop` | `(scheduler_e, schedulee_e)` | request.c:342、manager.c:461 | 停止服务调度（10/13） |

### 2.6 libexec / mapdriver / PCI / devman

| 函数 | 签名 | 调用点 | 语义 |
|------|------|--------|------|
| `libexec_load_elf` | `(execi, ...)` | exec.c:17（`load_object` 表） | ELF 加载（09，ARCH A-8：libexec → minix-elf） |
| `minix_stack_params`/`minix_stack_fill` | `(argv, envp, ...)` | exec.c:34/49 | 栈帧构造（09） |
| `mapdriver` | `(label, dev_nr, domain, ...)` | manager.c:820 | 驱动注册到 VFS（11）；消息槽 `mess_lsys_vfs_mapdriver` |
| `pci_set_acl`/`pci_del_acl` | `(&pci_acl)`/`(proc_ep)` | manager.c:833/889 | PCI ACL（11） |
| `DEVMAN_BIND`/`DEVMAN_UNBIND` | `message` | manager.c:846/903 | devman 绑定/解绑（11） |

---

## 3. 消息槽类型化（ARCH A-2）

### 3.1 mess_rs_* 族（ipc.h:1858-1906）

C 的 `message` 联合体按 `m_type` 选择子格式；Rust 用**语义层 struct**（vm.rs 既有模式）表达字段，指针用 `VirBytes(u64)`：

| Rust 类型 | C 消息槽 | 字段 | 使用方 |
|-----------|---------|------|--------|
| `RsReq` | `mess_rs_req`（ipc.h:1887-1896） | `len`/`name_len`/`endpoint`/`addr`/`name`/`subtype` | RS_UP/DOWN/REFRESH/RESTART/UPDATE/CLONE/UNCLONE/LOOKUP/GETSYSINFO 共用 |
| `RsInit` | `mess_rs_init`（ipc.h:1858-1867） | `result`/`type`/`rproctab_gid`/`old_endpoint`/`restarts`/`flags`/`buff_addr`/`buff_len`/`prepare_state` | RS_INIT（12） |
| `RsUpdate` | `mess_rs_update`（ipc.h:1898-1906） | `result`/`state`/`prepare_maxtime`/`flags`/`state_data_gid` | RS_LU_PREPARE（12/16） |
| `RsPmExecRestart` | `mess_rs_pm_exec_restart`（ipc.h:1869-1877） | `endpt`/`result`/`pc`/`ps_str` | PM→RS exec restart（09） |
| `RsPmSrvKill` | `mess_rs_pm_srv_kill`（ipc.h:1879-1885） | `pid`/`nr` | PM→RS srv kill（15） |

### 3.2 mess_lsys_* 族（ipc.h:1048-1072,1420-1428,1466-1474）

| Rust 类型 | C 消息槽 | 字段 | 方向 |
|-----------|---------|------|------|
| `LsysGetsysinfo` | `mess_lsys_getsysinfo`（ipc.h:1066-1072） | `what`（SI_*）/`where`/`size` | 外部→RS（14） |
| `LsysFiCtl` | `mess_lsys_fi_ctl`（ipc.h:1048-1056） | `gid`/`size`/`subtype`（RS_FI_CRASH） | 外部→RS（14） |
| `LsysFiReply` | `mess_lsys_fi_reply`（ipc.h:1058-1063） | `status` | RS→外部（14） |
| `LsysPmSrvFork` | `mess_lsys_pm_srv_fork`（ipc.h:1420-1428） | `uid`/`gid` | RS→PM（10） |
| `LsysVfsMapdriver` | `mess_lsys_vfs_mapdriver`（ipc.h:1466-1474） | `major`/`labellen`/`label`/`ndomains`/`domains[NR_DOMAIN]` | RS→VFS（11） |

### 3.3 快照 message 大小不一致的处理

> **⚠️ ARCH A-2 风险项（快照事实）**：本树 `include/minix/ipc.h` 的 `mess_1`（ipc.h:37-43）按 **56 字节**布局（`_ASSERT_MSG_SIZE` = 56，ipcconst.h:17-19），但 `sizeof(message) == 64`（ipc.h:2675 断言）——i386 下 payload 最大 56 字节 + `__ALIGNED(16)`（ipc.h:2673）→ sizeof 64。`mess_rs_init`（ipc.h:1858-1867）在 64 位布局下为 64 字节，与 `_ASSERT_MSG_SIZE(mess_rs_init)` 的 56 冲突——快照处于 56→64 消息迁移的中间态。
>
> **决策**：Rust 侧以**语义层 struct** 建模（字段级类型，无 `#[repr(C)]` 字节布局），`DecodeFromM1`/`EncodeToM1` 实现 **DEFERRED**——wire-up 时按 64 字节协议定稿传输层，再补 codec。语义字段（§3.1/§3.2 表）不受布局影响。

---

## 4. Rust 依赖面

### 4.1 minix-types ipc/rs.rs 扩展

`os/libs/minix-types/src/ipc/rs.rs` 在既有 RS_* 调用号基础上新增 §3 的 typed payload views：`RsReq`/`RsInit`/`RsUpdate`/`RsPmExecRestart`/`RsPmSrvKill`/`LsysGetsysinfo`/`LsysFiCtl`/`LsysFiReply`/`LsysPmSrvFork`/`LsysVfsMapdriver` + `VM_RS_MEM_*` 常量。字段类型沿用既有约定：`Endpoint`/`VirBytes`/`Pid`/`Uid`/`Gid`/`i32`/`usize`。

### 4.2 minix-sys API 契约

`os/libs/minix-sys/src/lib.rs` 是 stub（`todo!()`），其 API 面即 §2 契约的 Rust 投影：`sys_getinfo`/`sys_privctl`/`sys_setalarm`/`sys_kill`/`sys_datacopy`/`sys_statectl`/`sys_whoami`/`sys_update`/`vm_memctl`/`vm_update`/`vm_prepare`/`ds_*`/`sched_start`/`sched_stop`/`srv_fork`/`srv_kill`/`srv_execve`/`mapdriver`/`pci_*`。签名以 §2 表为准，wire-up 时实现。

---

## 5. 测试要点

`ipc/rs.rs` 新增测试（`cargo test -p minix-types --lib ipc::rs`，9 项，9/9 已落地）：

1. `RS_*` 调用号表（`RS_RQ_BASE=0x700`、`RS_UP..RS_FI`、`RS_INIT=0x714` 等，com.h:463-482）。
2. `RS_SYSCTL_*`/`RS_FI_CRASH` 子函数（com.h:485-492）。
3. `RsReq` 字段（六字段构造 + 读回）。
4. `RsInit` 字段（九字段构造 + 读回）。
5. `RsUpdate` 字段（五字段构造 + 读回）。
6. `RsPmExecRestart`/`RsPmSrvKill` 字段。
7. `LsysGetsysinfo`/`LsysFiCtl`/`LsysFiReply` 字段。
8. `LsysPmSrvFork`/`LsysVfsMapdriver` 字段。
9. `VM_RS_MEM_*` 常量表（com.h:741-745）。

测试总数声明：本文档范围为 `ipc::rs` 模块测试数（以该模块 `cargo test` 输出为准）。

---

## 6. 过渡

本契约是 wire-up 阶段的**锚点**：

- **上游**：16（`vm_update`/`rs_receive_ticks`）、17（`ds_retrieve_label_endpt`/`cpf_*`/`sys_datacopy`）、18（`srv_fork`/`vm_update`/`sys_whoami`/`sys_privctl`）的 IPC 面在此收口；
- **下游**：`minix-sys` 按 §2 表实现；`minix-types` 按 §3 表补 codec（64 字节协议定稿后）；
- **ARCH**：A-2（消息槽类型化，本文档）、A-8（libexec → minix-elf，09）、A-9（`rs_receive_ticks` → `receive_timeout` 原语，16 的 update 超时）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/99-rs-global-concepts.md` —— RS_* 消息类型/RSS_*/SF_*/SEF_* 标志词典
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— `vm_update`/`rs_receive_ticks`/`sys_update` 调用点
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/17-rs-state-data.md` —— `ds_retrieve_label_endpt`/`sys_datacopy`/cpf grants
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/18-rs-self-lifecycle.md` —— `srv_fork`/`sys_whoami`/`sys_privctl`/`vm_update` 特例
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/09-rs-exec.md` —— libexec/minix_stack（ARCH A-8）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/11-rs-publish.md` —— mapdriver/PCI/devman
- `minix3/minix/lib/libsys/*`、`include/minix/ipc.h:1048-1072,1420-1428,1466-1474,1858-1906`、`include/minix/com.h:627,736,741-745` —— ground truth
