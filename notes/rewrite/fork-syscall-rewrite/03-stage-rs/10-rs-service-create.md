# 10-rs-service-create: 服务创建机制

> **分类**: 阶段 4 — 服务创建与配置（从槽位到运行进程的第四步：创建）
> **源码**: `minix3/minix/servers/rs/manager.c`（`create_service`—531、`clone_service`—713、`activate_service`—1013、`get_service_instances`—1334、`clone_slot`—1800、`swap_slot_pointer`—1856、`swap_slot`—1870）、`minix3/minix/lib/libsys/srv_fork.c`（`PM_SRV_FORK`）、`minix3/minix/include/minix/com.h:738-745`（`VM_RS_MEM_*`）、`minix3/minix/include/minix/priv.h`（`ROOT_SYS_PROC`/`VM_SYS_PROC`/`DYN_PRIV_ID`/`LU_SYS_PROC`/`RST_SYS_PROC`）
> **Rust 模块**: `os/servers/rs/src/service_create.rs`（`check_create_preconditions`/`mark_child_created`/`rebuild_args`/`clone_slot`/`link_replica`/`activate_service`/`swap_index`/`swap_slot`）+ `os/servers/rs/src/boot.rs`（`KernelApi` 扩展 + `VmRsMemReq`）+ `os/servers/rs/src/process_table.rs`（`swap_rows`/`set_endpoint_index`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md`（槽位配置）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/09-rs-exec.md`（`read_exec`/`srv_execve`/`free_exec`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`alloc_slot`/`free_slot`/`rproc_ptr`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/03-rs-privilege.md`（priv 结构 + `sys_privctl` 操作面）
> **说明**: 服务的"诞生"是 RS 的一次 11 步编排：fork 出子进程后，把 priv、调度器、可执行映像、VM 权限依次就位。本文档固化编排的顺序语义与失败回滚的挂接点，并把 `clone_slot`（副本槽）与 `swap_slot`（槽交换）两个被 15/16 复用的纯原语建模为 Rust 值语义（ARCH A-3）。

---

## 1. 概念：服务进程是怎么"诞生"的

### 1.0 章节引言

`08-rs-slot-config.md` 把 `rs_start` 校验落地成槽位字段，`09-rs-exec.md` 让槽位持有可执行映像——但还没有**进程**。在 Minix3 里，"把槽位变成运行中的系统服务"是 RS 独有的能力：它 fork 子进程、向内核登记 priv、向调度器登记调度参数、exec 映像、并把 VM 相关的最后几块拼图（RS/VM 实例 pin、VM 调用掩码）装好。本文档回答的问题是：**create_service 的 11 步编排顺序是什么、失败时如何回滚、副本与更新如何复用同一批槽位原语**。

> **本章不讲什么**（机制一律移交）:
> - `rs_start` 的校验与槽位落地（`08-rs-slot-config.md`）——本文档只消费已就绪的槽位
> - 映像的读入/共享/释放（`09-rs-exec.md`）——本文档只陈述 `read_exec`/`srv_execve`/`free_exec` 三个调用点
> - 发布（`11-rs-publish.md`）、运行与 RS_INIT 握手（`12-rs-init-run.md`）——本文档止步于"创建完成"
> - `cleanup_service`/`free_slot` 的机制（`15-rs-terminate-restart.md`）——本文档只固化"失败路径调用它们"这一外部行为
> - `srv_fork`/`sys_privctl`/`vm_memctl` 的消息布局（`19-rs-external-interfaces.md`）——本文档只陈述 `KernelApi` 边界

### 1.1 为什么 RS 亲自创建（WHY）

服务不是普通进程：它的 priv 结构、调度器、exec 映像、VM 权限必须与 RS 表里的槽位**严格同步**，任何一步错位都会让"RS 眼中的服务"和"内核眼中的进程"分叉——而 RS 的全部监控/重启/Live Update 逻辑都建立在槽位是权威的这个假设上。如果交给 PM 走普通 `fork+execve`，RS 就失去了对中间态的控制。所以 `create_service` 是一次**编排**：每个外部调用之后紧跟表更新或下一个外部调用，顺序即契约。

### 1.2 一次创建的解剖（WHAT）

```
槽位就绪（08）
  │  create_service（manager.c:531-707）
  ├─ 1. 前置三闸门（547/555/563）── 失败 free_slot + EPERM
  ├─ 2. srv_fork(uid, 0)（576）── PM 创建子进程（A-1）
  ├─ 3. getprocnr(pid)（584）── 失败 panic
  ├─ 4. 表登记（589-596）── IN_USE/endpoint/pid/计时器/rproc_ptr/in_use
  ├─ 5. priv 面（600-605）── SYS_PRIV_SET_SYS + sys_getpriv
  ├─ 6. sched 面（609）── sched_init_proc
  ├─ 7. exec 面（622-643）── read_exec? → srv_execve → !use_copy 时 free_exec
  ├─ 8. setuid(0) VFS 阻塞 hack（656）
  ├─ 9. RS 实例 pin（658-669）── ROOT_SYS_PROC → VM_RS_MEM_PIN
  ├─10. VM 实例（671-695）── MAKE_VM + get_service_instances 全实例 pin
  └─11. vm_set_priv（698-705）── VM 调用掩码
  → OK（707）
```

失败路径有一个**统一出口**：第 5~11 步任一步失败都 `cleanup_service(rp)`（+ 部分路径 `vm_memctl(RS_PROC_NR, PIN)` 重新 pin RS 自己），然后返回错误码。`cleanup_service` 的机制在 15，本文档只固化"调用即回滚"这一外部行为。

### 1.3 生命周期位置

`create_service` 位于服务生命周期的**创建**步：`RS_UP → check_call_permission（04）→ alloc_slot + init_slot（08）→ create_service（本文档）→ publish_service（11）→ run_service（12）`。它同时被四条路径复用：`RS_UP` 首次创建、`clone_service`（replica/副本，13/16）、`reincarnate_service`（15）与 `restart_service` 的 clone 路径（15）——所以 `clone_slot`/`swap_slot` 被设计成独立的纯原语，而不是内联在 `create_service` 里。

---

## 2. C 源码分析

### 2.1 前置三闸门（manager.c:537-568）

`create_service` 一进来先算两个布尔量（manager.c:542-544）：

```c
use_copy   = (rpub->sys_flags & SF_USE_COPY);
has_replica= (rp->r_old_rp
    || (rp->r_prev_rp && !(rp->r_prev_rp->r_flags & RS_TERMINATED)));
```

- **闸门 1**（manager.c:547-552）：`SF_NEED_REPL`（该服务必须从已有副本创建）但没有副本 → 打印 + `free_slot(rp)` + `EPERM`。
- **闸门 2**（manager.c:555-560）：`SF_NEED_COPY`（必须有内存映像）但没有 `SF_USE_COPY` → 同上。
- **闸门 3**（manager.c:563-568）：无副本路径且 `r_cmd` 为空（`strcmp(rp->r_cmd, "") == 0`）→ 同上。

`has_replica` 的关键细节：**已 `RS_TERMINATED` 的 prev replica 不算数**（manager.c:543-544）——链上可能挂着等清理的死亡槽，不能作为创建来源。old_rp（Live Update 的旧版本）无条件计数。

### 2.2 fork 与表登记（manager.c:573-601）

`child_pid = srv_fork(rp->r_uid, 0)`（manager.c:576）。`srv_fork` 是 RS 专用的 fork 面（`lib/libsys/srv_fork.c`：`_taskcall(PM_PROC_NR, PM_SRV_FORK, &m)`，消息带 `uid`/`gid`），把 uid 传下去、group 强制 wheel（注释 "Force group to wheel for now"）。失败 → `free_slot(rp)` + 返回错误（manager.c:577-580）。

`getprocnr(child_pid, &child_proc_nr_e)`（manager.c:584）拿子进程 endpoint；失败走 `panic`——RS 把"fork 成功但查不到 endpoint"视为不可能的内部错误。

登记块（manager.c:587-597）逐字段更新槽位：

| 字段 | 值 | C 行 |
|------|----|------|
| `r_flags` | `= RS_IN_USE`（**覆盖**赋值，不是 OR——释放过的行带陈旧标志） | 589 |
| `rpub->endpoint` | 子进程 endpoint | 590 |
| `r_pid` | 子进程 pid | 591 |
| `r_check_tm` | 0（还没检查过） | 592 |
| `r_alive_tm` | `getticks()`（当前存活） | 593 |
| `r_stop_tm` | 0（没在退出） | 594 |
| `r_backoff` | 0（不重启） | 595 |
| `rproc_ptr[_ENDPOINT_P(ep)]` | `rp`（快速索引） | 596 |
| `rpub->in_use` | `TRUE` | 597 |

### 2.3 priv / sched / exec 三连（manager.c:600-650）

- **priv**（manager.c:600-605）：`sys_privctl(ep, SYS_PRIV_SET_SYS, &rp->r_priv)` 把槽位里的 priv 结构装进内核，随后 `sys_getpriv(&rp->r_priv, ep)` 把内核权威版本**同步回来**。任一失败 → `cleanup_service(rp)` + `vm_memctl(RS_PROC_NR, PIN)` + `ENOMEM`。
- **sched**（manager.c:609-613）：`sched_init_proc(rp)`（utility.c:364-382，03 已有建模）——断言 user 进程无 scheduler、sys 进程必须有，然后 `sched_start(...)` 把 `r_scheduler/r_priority/r_quantum/r_cpu` 交给调度器。
- **exec**（manager.c:622-650）：`use_copy` 时不读文件（映像已在槽里）；否则 `read_exec(rp)` 读入（manager.c:625-629，失败 → cleanup + ENOMEM 回滚），然后 `srv_execve(child_proc_nr_e, rp->r_exec, rp->r_exec_len, rpub->proc_name, rp->r_argv, environ)`（manager.c:634-635，失败 → cleanup_service）。exec 完成后无条件 `vm_memctl(RS_PROC_NR, VM_RS_MEM_PIN)`（manager.c:636）——**fork 之后 RS 自己的内存必须重新 pin**，否则未来写会 pagefault（注释见 manager.c:570-572）。非 `use_copy` 路径最后 `free_exec(rp)`（manager.c:643）——一次性映像用完即释放。

### 2.4 setuid(0) hack 与 RS/VM 特例（manager.c:652-697）

**`setuid(0)`**（manager.c:656）：注释明确说明这是**临时 hack**——非阻塞 fork 的目的原本是避免 VFS 参与 fork（VFS 可能正阻塞在向 MFS 的 sendrec 上），但反过来 VFS 可能还没收到 PM 的 fork 消息；如果立即调 `mapdriver()`（11），VFS 会拒绝加驱动条目。`setuid(0)` 强制 PM→VFS 的阻塞通信，保证 mapdriver 时序正确。这是**外部行为的一部分**（D-12），Rust 侧必须等效处理（19 接线）。

**RS 实例 pin**（manager.c:658-669）：`r_priv.s_flags & ROOT_SYS_PROC`（RS 的 replica）→ `vm_memctl(endpoint, VM_RS_MEM_PIN)`——RS 的实例必须驻留内存。

**VM 实例**（manager.c:671-695）：`r_priv.s_flags & VM_SYS_PROC` → 先 `vm_memctl(endpoint, VM_RS_MEM_MAKE_VM)` 告诉 VM"新 VM 实例来了"，然后：

```c
rs_rp = rproc_ptr[_ENDPOINT_P(RS_PROC_NR)];
get_service_instances(rs_rp, &rs_rps, &nr_rs_rps);
for(i=0;i<nr_rs_rps;i++)
    vm_memctl(rs_rps[i]->r_pub->endpoint, VM_RS_MEM_PIN, 0, 0);
```

VM 只在收到 MAKE_VM 后才真正允许 RS 实例 pin 内存，所以 RS 要把**自己的全部实例**重新 pin 一遍。`get_service_instances`（manager.c:1334-1352）用 static 5 槽数组收集 rp 自身 + prev/next/old/new 五个方向的实例。

### 2.5 vm_set_priv 收尾（manager.c:698-705）

`vm_set_priv(endpoint, &vm_call_mask[0], TRUE)` 把槽位的 VM 调用掩码交给 VM（第三个参数 `TRUE` 表示允许）。这是创建的最后一步；成功 → `return OK`（manager.c:707）。

### 2.6 clone_service：副本创建（manager.c:713-781）

`clone_service(rp, instance_flag, init_flags)` 是"复制一个服务实例"的编排，被 13（`do_clone`/`do_restart` 的副本路径）与 16（Live Update 的新版本）复用：

1. **VM 特例**（manager.c:728-733）：目标 endpoint 是 `VM_PROC_NR` 且 `instance_flag == LU_SYS_PROC` 且 `rp->r_next_rp` 已存在 → `cleanup_service_now(rp->r_next_rp)` 并断开——VM 目前只可靠支持一个 replica（注释 "XXX TO-DO"）。
2. **clone_slot**（manager.c:737-739）：见 2.8。
3. **链方向**（manager.c:741-750）：`LU_SYS_PROC` → 挂 `rp->r_new_rp`/`replica->r_old_rp`（old/new 链，16 消费）；否则 → `rp->r_next_rp`/`replica->r_prev_rp`（prev/next 副本链，15 消费）。
4. **flags**（manager.c:751-752）：replica 的 `s_flags |= instance_flag`、`s_init_flags |= init_flags`（LU/RST 标记）。
5. **create_service(replica)**（manager.c:759-763）：失败 → 断开链 + 返回错误。
6. **RS 备份信号管理器**（manager.c:765-777）：replica 同时带 `ROOT_SYS_PROC|RST_SYS_PROC` 时（即"用于重启 RS 的 RS 副本"），`update_sig_mgrs(rs_rp, SELF, replica->endpoint)` 让现有 RS 把信号指向副本，再 `update_sig_mgrs(replica_rp, SELF, NONE)`。失败 → 断开 + `kill_service(replica_rp, "update_sig_mgrs failed", r)`。

### 2.7 activate_service（manager.c:1013-1026）

极简原语：`ex_rp` 若带 `RS_ACTIVE` 则清除，`rp` 若无 `RS_ACTIVE` 则置位。被 16（update_service 的实例切换）与 15（reincarnate）调用。**只有 active 实例能被 label 查到**（02 `lookup_slot_by_label` 过滤 `RS_ACTIVE`），所以"激活"就是"让新实例成为服务的代表"。

### 2.8 clone_slot：浅拷贝 + 深拷贝（manager.c:1800-1849）

1. `alloc_slot(&clone_rp)`（manager.c:1808-1813）——注意 **alloc_slot 不置 `RS_IN_USE`**（manager.c:2067-2083：只找第一个空闲行），置位是调用方的事；clone_slot 返回后由 clone_service/create_service 的流程决定。
2. `sys_getpriv(&rp->r_priv, rpub->endpoint)`（manager.c:1818-1821）——先把源槽 priv 与内核同步（源可能已被内核改过），失败 `panic`。
3. **浅拷贝**（manager.c:1824-1825）：`*clone_rp = *rp; *clone_rpub = *rpub;`——配置、priv、exec 指针、IPC 列表全数复制。
4. **深拷贝修正**（manager.c:1827-1847）：
   - `r_init_err = ERESTART`（默认 init 错误）；`r_flags &= ~RS_ACTIVE`（副本不激活）；`r_pid = -1`；`rpub->endpoint = -1`（还没进程）；
   - `r_pub = clone_rpub`（恢复 pub 指针——**只存在于 C 的双表布局**）；
   - `build_cmd_dep(clone_rp)`（从 r_cmd 重建 r_args/r_argc）；
   - `SF_USE_COPY` → `share_exec(clone_rp, rp)`（共享映像，09）；
   - 四链 `r_old_rp/r_new_rp/r_prev_rp/r_next_rp = NULL`；
   - `s_flags |= DYN_PRIV_ID`（副本永远动态 priv id）；`s_flags &= ~(LU_SYS_PROC|RST_SYS_PROC)`；`s_init_flags = 0`。

### 2.9 swap_slot / swap_slot_pointer（manager.c:1856-1932）

`swap_slot(src_rpp, dst_rpp)` 把两个槽**整体交换**，被 16 的 `update_service` 用来把旧/新实例对调。步骤：

1. 保存四份原值（manager.c:1886-1890）。
2. 交换：`*src_rp = orig_dst_rproc; *src_rpub = orig_dst_rprocpub; ...`（manager.c:1892-1896）——private 与 public 两个表都交换。
3. 恢复：`src_rp->r_pub = orig_src_rproc.r_pub`（manager.c:1899-1900）——**每个槽的 r_pub 回到自己那行的 pub 条目**；`r_upd` 同理（1901-1902）。
4. `build_cmd_dep` ×2（manager.c:1904-1906）。
5. 两行的四链各自 `swap_slot_pointer`（manager.c:1908-1916）——`swap_slot_pointer(&p, src, dst)` 把指向 src 的引用改为 dst、指向 dst 的改为 src（manager.c:1856-1865）。
6. `RUPDATE_ITER` 遍历 update 链，每个 `rpupd->rp` 也做同样替换（manager.c:1919-1921）——**update 描述符还指着旧槽**，必须跟着换。
7. `rproc_ptr` 两个 endpoint 槽位交换（manager.c:1922-1925）。
8. 调整入参：`*src_rpp = dst_rp; *dst_rpp = src_rp`（manager.c:1928-1929）。

---

## 3. Rust 设计决策

### 3.1 纯语义切片（与 09 同模式）

`create_service` 的编排依赖 9 个外部面（`srv_fork`/`getprocnr`/`sys_privctl`/`sys_getpriv`/`sched_init_proc`/`srv_execve`/`vm_memctl`/`vm_set_priv`/`setuid`），全部走 `KernelApi`/消息面（19 接线）。与 `exec.rs` 的取舍一致（09 §3.1），本模块拥有**纯、可测**的切片：

- `check_create_preconditions` —— 三闸门判定（无 IO）
- `mark_child_created` —— 表登记（纯表操作，ticks 注入）
- `rebuild_args` —— `build_cmd_dep` 的槽位版本（r_cmd → r_args/r_argc）
- `clone_slot` / `link_replica` / `activate_service` / `swap_slot` —— 被 13/15/16 复用的原语

**create_service 的编排本身 DEFERRED（19）**：当 `KernelApi` 面全部落地后，11 步序列按本文档 §2 的顺序组装；每步失败的错误码与回滚挂接点已在 §2 固化，组装时直接套用。

### 3.2 KernelApi 扩展（ARCH A-1）

`boot.rs` 的 `KernelApi` trait 新增 4 个方法（默认 fail-closed `unimplemented!`，19 接线）：

| 方法 | C 面 | 语义 |
|------|------|------|
| `srv_fork(uid, gid) -> Result<Pid, i32>` | `srv_fork` → `PM_SRV_FORK`（libsys/srv_fork.c） | **ARCH A-1**：no_std 无 libc `fork`；外部行为（经 PM 创建子进程）通过 PM 消息面保持 |
| `getprocnr(pid) -> Result<Endpoint, i32>` | `getprocnr` → PM_GETEPINFO | pid → endpoint |
| `vm_memctl(ep, VmRsMemReq, a, b)` | `vm_memctl`（VM_RS_MEMCTL） | `VmRsMemReq` 枚举映射 `VM_RS_MEM_*`（com.h:741-745） |
| `vm_set_priv(ep, CallMask, allow)` | `vm_set_priv` | VM 调用掩码（`CallMask`，03） |

### 3.3 clone_slot 的值语义（ARCH A-3）

C 的"浅拷贝 + r_pub 恢复"是**双表布局的产物**：`rproc` 与 `rprocpub` 分开存，`r_pub` 自引用指针必须手动恢复。Rust 的 `ServiceSlot` 把 pub 半内嵌（02 §3.1），所以：

- 浅拷贝 = `ServiceSlot::clone()`（整行值复制）；
- `r_pub` 恢复步骤**消失**（ARCH A-3——指针自引用被结构体布局消除）；
- 深拷贝修正 = 字段级改写：`init_err=ERESTART`、清 `ACTIVE`、`pid=None`、`endpoint=NONE`、`rebuild_args`、`SF_USE_COPY → exec = src.exec.clone()`（`Arc` 共享，09 A-5）、四链清空、`DYN_PRIV_ID`、清 `LU/RST`、`init_flags=0`。

`sys_getpriv` 同步（manager.c:1818-1821）DEFERRED（19）——Rust 签名 `clone_slot(table, src)` 不带 kernel 参数，同步在 19 接线时于调用点完成（或用带 `&mut dyn KernelApi` 的重载）。
（T5 定案，2026-08-16：**不用** kernel 参数重载——同步在 19 shell 完成并注入结果，纯函数层不出现
`KernelApi`，见 99 §3.4。）

### 3.4 swap_slot 的引用交换（ARCH A-3）

C 的 `swap_slot_pointer` 是裸指针比较替换；Rust 用 `Option<SlotId>`，等价为：

```rust
pub fn swap_index(v: &mut Option<SlotId>, src: SlotId, dst: SlotId) {
    if *v == Some(src) { *v = Some(dst); } else if *v == Some(dst) { *v = Some(src); }
}
```

`swap_slot(table, src, dst)` 的顺序与 C 一一对应：

1. `table.swap_rows(src, dst)` —— 整行交换（`Vec::swap`）；C 的"双表交换 + r_pub 恢复"折叠成一次交换；
2. `rebuild_args` ×2（manager.c:1904-1906）——R15：`argc` 只计**完整写入** args 缓冲的 token（含 NUL），放不下的尾部 token 整体丢弃且缓冲尾保持 NUL 终止；C 的 `strcpy` 对 512 字节满缓冲本就会溢出（UB），Rust 不继承；
3. 两行四链 `swap_index`（manager.c:1908-1916）；
4. 两行 endpoint 的 `by_endpoint` 索引交换（manager.c:1922-1925）——R12：`clone_slot` 产物的 `Endpoint::NONE`（manager.c:1831）在 `endpoint_slot`/`set_endpoint_index` 越界时 `None`/忽略（fail-closed，不 panic）；测试 `test_swap_slot_with_vacant_row_no_panic` 锁定；
5. 返回 `(dst, src)`（C 的 `*src_rpp = dst_rp; *dst_rpp = src_rp`）。

**RUPDATE_ITER（manager.c:1919-1921）DEFERRED（16）**：per-slot `r_upd` 描述符未建模（02 P2-3），等 16 落地时把 update 链的引用交换补进 `swap_slot`（或由 16 的调用点负责）。

### 3.5 失败回滚的边界声明

第 5~11 步失败时 C 调 `cleanup_service(rp)`（+ 部分路径 pin 回滚）。`cleanup_service` 的机制（两段式：标记 `RS_DEAD` + late_reply；真清理 sched_stop + srv_kill + 脚本 + detach/free_slot）在 15。本文档的契约：**编排层（19 组装时）在每步失败后调用 15 的 cleanup 面**；`service_create.rs` 只传播 C 等价错误码（`EPERM`/`ENOMEM`/`s`）。前置三闸门失败时 C 直接 `free_slot`（manager.c:550/558/566），这是 create_service 内部行为，Rust 组装时同样处理。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/service_create.rs`：

| 函数 | C 对应 | 说明 |
|------|--------|------|
| `check_create_preconditions(table, rp)` | manager.c:542-568 | 三闸门；`has_replica` 内联（old_rp 或非 TERMINATED 的 prev_rp） |
| `mark_child_created(table, rp, endpoint, pid, ticks)` | manager.c:587-597 | `r_flags = RS_IN_USE`（**覆盖**）+ 8 字段登记 + `set_endpoint_index` |
| `rebuild_args(slot)` | manager.c:289-324 | r_cmd → NUL 分隔 r_args + argc；被 clone_slot/swap_slot 复用 |
| `clone_slot(table, src)` | manager.c:1800-1849 | 值复制 + 8 项深拷贝修正 |
| `link_replica(table, rp, replica, flag, init_flags)` | manager.c:735-756 | LU → new/old 链；否则 → next/prev 链 + flags |
| `activate_service(table, rp, ex_rp)` | manager.c:1013-1026 | ACTIVE 位迁移 |
| `swap_index(v, src, dst)` | manager.c:1856-1863 | `Option<SlotId>` 相等替换 |
| `swap_slot(table, src, dst)` | manager.c:1870-1932 | 整行交换 + 引用重定向，返回 `(dst, src)` |

配套修改：`process_table.rs` 增加 `swap_rows`（整行 `Vec::swap`）与 `set_endpoint_index`（`by_endpoint` 写入，ARCH A-4）；`boot.rs` 扩展 `KernelApi` + `VmRsMemReq`。

### 4.2 关键不变量

1. **`alloc_slot` 不置 `RS_IN_USE`**（manager.c:2067-2083）：clone_slot 返回的槽是"已分配未占用"；调用方（13/15/16）在真正使用前置位。Rust 测试必须遵守（否则第二次 alloc 返回同一行）。
2. **`mark_child_created` 用覆盖赋值**：`r_flags = RS_IN_USE` 而非 `|=`（manager.c:589 注释：释放过的行可能带 `RS_TERMINATED`/`RS_DEAD` 等陈旧标志）。
3. **副本永远动态 priv id**：`clone_slot` 强制 `DYN_PRIV_ID` 并清除 `LU_SYS_PROC|RST_SYS_PROC`（manager.c:1842-1847）——副本不能继承"实例身份"。
4. **swap 后引用完整性**：两行四链 + `by_endpoint` 全部重定向；任何指向 src/dst 的 `SlotId` 在 swap 后要么指向对方、要么指向正确的新行。

---

## 5. 测试要点

`service_create.rs` 内 13 项测试（`cargo test -p minix-rs --lib service_create`），覆盖：

1. **preconditions**：`NEED_REPL` 无副本 → `EPERM`；prev 副本 `TERMINATED` 不算数；prev 存活（且命令非空）→ `Ok`；`NEED_COPY` 无内存副本 → `EPERM`；空命令 → `EPERM`。
2. **mark_child_created**：endpoint/pid/`alive_tm`/`backoff=0`/`in_use`/`by_endpoint` 全量断言。
3. **clone_slot**：`ERESTART`、清 `ACTIVE`、`pid=None`、`endpoint=NONE`、四链全 `None`、`DYN_PRIV_ID`、清 `LU/RST`、`init_flags=0`、`argc` 从命令重建、**源槽不受影响**。
4. **link_replica**：`LU_SYS_PROC` 走 new/old、普通走 next/prev、flags 落位。
5. **activate_service**：ex 清位 + rp 置位；无 ex 分支只置位。
6. **swap_slot**：内容交换、`by_endpoint` 跟随内容、第三槽 `c.prev_rp` 从 a 重定向到 b、返回值 `(dst, src)`。
7. **rebuild_args 满缓冲（R15）**：512 字节无 NUL 命令 → `argc` 只计完整写入 token、尾部 NUL 终止（`test_rebuild_args_full_buffer_argc`；C 的 `strcpy` 满缓冲溢出是 UB，Rust 不继承）。
8. **vacant 行 swap（R12）**：`clone_slot` 产物的 `Endpoint::NONE` 参与 `swap_slot` 时 `by_endpoint` 越界 `None`/忽略，不 panic（`test_swap_slot_with_vacant_row_no_panic`）。
9. **踩坑延续**：所有测试在两次 `alloc_slot` 之间先置 `IN_USE`（02 文档化的 C 语义），否则同 id 0 导致断言全乱。

测试总数声明：本文档范围为 **13 项**（`service_create` 模块内）。全局 `cargo test -p minix-rs --lib` = 208 通过（2026-08-16，随并行模块增长，以各 doc 范围为准）。

---

## 6. 过渡

`create_service` 结束时，槽位里有了**活着的进程**（endpoint/pid 已登记、priv/sched/exec/VM 已就位），但服务还没有对外发布、还没有进入 RS 的监控视野。下一步：

- **11-rs-publish**：`publish_service` 把服务写进 DS label、VFS mapdriver、PCI/ACL、devman——"服务可以被别人找到"；
- **12-rs-init-run**：`run_service` 发 RS_INIT 并等待 ready 握手——"服务正式运行"；
- **07/15/16** 随后接管监控、终止、热升级；它们全部复用本文档的 `clone_slot`（副本）/`swap_slot`（新旧对调）/`activate_service`（激活迁移）原语。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/09-rs-exec.md` —— `read_exec`/`srv_execve`/`free_exec` 调用点的 exec 面契约
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md` —— `alloc_slot`/`init_slot` 输入
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` —— `alloc_slot` 不置 IN_USE 语义、`rproc_ptr`（ARCH A-4）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/03-rs-privilege.md` —— priv 结构 + `SYS_PRIV_SET_SYS`/`UPDATE_SYS` 操作面
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/15-rs-terminate-restart.md` —— `cleanup_service`/`free_slot`/`kill_service` 回滚机制
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— `swap_slot` 的 RUPDATE_ITER 消费点
- `minix3/minix/servers/rs/manager.c:531-786,1013-1026,1334-1352,1800-1932` —— ground truth
- `minix3/minix/lib/libsys/srv_fork.c` —— `PM_SRV_FORK` 消息面（A-1）
