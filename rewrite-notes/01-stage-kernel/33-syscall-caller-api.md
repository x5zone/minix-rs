# 33-syscall-caller-api: 系统调用层的 caller-by-nr 接口 —— 用进程号代替进程引用

> **源码**: `minix3/minix/kernel/system/do_schedule.c`（`do_schedule(struct proc * caller, message * m_ptr)`）、`minix3/minix/kernel/system.c`（`p_delivermsg_vir` 保存，:141）、`minix3/minix/kernel/proc.c`（`do_ipc` 从 per-CPU 存储取当前进程，:601）、`minix3/minix/kernel/system/do_privctl.c`（调用者权限检查，:47）、`minix3/minix/kernel/proc.h`（`proc_addr` 宏，:269）
> **关联 Rust**: `os/kernel/src/syscall.rs`（`kernel_call` 三件套、46 个派发臂、`ArchSyscall` trait）、`os/kernel/src/trap_dispatch.rs`（两条链顶）、`os/kernel/src/proc_table.rs`（`ProcessTable::caller_slot_mut`，现为审计记录）、`os/kernel/src/cross_space.rs`、`os/kernel/src/vm.rs`、`os/kernel/src/syscall_{clock,copy,device,process,signal}.rs`、`os/kernel/src/{misc,grant,kmess}.rs`
> **创建**: 2026-09-19（edge1 K20 收口交付；大纲与设计决定见工作区 `01-stage-kernel/.design/33-{outline,outline-review,design}.v1.md`）

---

## Ch1: 概念

### 1.1 C 世界里的"调用者"是一个指针

Minix3 的系统调用处理函数把调用者当作**指针**接收。`do_schedule` 的签名就是代表：`int do_schedule(struct proc * caller, message * m_ptr)`（`minix3/minix/kernel/system/do_schedule.c:8`）。这个指针来自内核的进程数组：`proc_addr(n)` 展开为 `(&(proc[NR_TASKS + (n)]))`（`minix3/minix/kernel/proc.h:269`），也就是说"调用者是进程表里的一行"这件事在 C 里被直接表达成了一个指向该行的指针，并沿着调用链一路传递。

IPC 腿走的是另一条路：`do_ipc(reg_t r1, reg_t r2, reg_t r3)` 不接收调用者参数，而是在函数体内从 per-CPU 存储取当前进程——`struct proc *const caller_ptr = get_cpulocal_var(proc_ptr);`（`minix3/minix/kernel/proc.c:601`）。同一个内核里因此并存两种取用风格：**调用者传参**（system call 腿）与**调用者就地取**（IPC 腿）。

### 1.2 Rust 改写后的矛盾：两个同时可变的借用指向同一行

Rust 改写把"进程表"建模为 `ProcessTable`，把"一行"建模为 `KProcess`（`os/kernel/src/proc_table.rs:fn get（L110，工具生成）`、`fn get_mut（L117，工具生成）`）。当调用者以句柄形式传参时，链顶函数就同时持有两个可变借用：

```rust
pub fn kernel_call(
    caller: &mut KProcess,          // 进程表里的第 caller_nr 行
    proc_table: &mut ProcessTable,  // 含该行的整张表
    ...
)
```

这两个参数在类型系统看来是**两块互不相干的内存**，而实际语义是"表包含行"。编译器无法证明它们不重叠，于是每一处"先动调用者、再动表"的代码都必须绕开借用检查。K7 把这类绕行收敛到了一个具名构造器 `ProcessTable::caller_slot_mut`（`os/kernel/src/proc_table.rs:636`），用不受 `&mut self` 约束的生命周期把槽位引用"洗"出来，并在注释里写清它的别名契约。

单点收敛降低了风险，但没有消除它：

1. 契约是**时间性**的——"两次引用不在同一时刻触及同一字段的字节"，这句话只能靠注释与评审维持，类型系统不检查；
2. 风险窗口**横跨整条调用链**——句柄要逐层传递，139 处（宽口径复核后为 138 处）签名都在携带它；
3. 新代码是否遵守"只从这一处洗"的纪律，取决于写代码的人有没有读过那段注释。

### 1.3 出路：把能力降级为句柄

系统调用层真正需要的信息只有一项：**调用者是第几号进程**。`ProcNr` 是一个值（`Copy`），不携带借用；进程表本身由链顶持有，任何一个用点都可以在需要时从表里重新取出那一行。

这就是本次改写的核心决定（方案 A，caller-by-nr）：**签名参数 `caller: &mut KProcess` 换成 `caller_nr: ProcNr`，每个用点当地重借**。它把"引用"降级为"索引"，别名风险由编译器接管：值参数跨帧传递不产生别名，任何"同时动调用者与表"的位置都退化成**顺序**的短暂借用。

跨体系看，这不是新发明，而是把同仓已有的形态推广到全部臂：IPC 腿早就是"传索引不传引用"（`os/kernel/src/syscall.rs:fn dispatch_ipc（L762，工具生成）` 接收 `caller_idx`，其上方注释记录了它从 `(caller: &mut KProcess, proc_table)` 改写为 `(procs, caller_idx)` 的历史，`os/kernel/src/syscall.rs:fn dispatch_ipc_entry（L690，工具生成）` 内注释）。Redox 的内核把"当前进程"表达为 per-CPU 索引（由 CPU 号到调度器槽位），Linux 的 `current` 同样是 per-CPU 宏——**跨层传索引、用点再解析**是这两个系统的一致做法。操作系统理论里，这对应能力（capability）与句柄（handle）之分：`ProcNr` 是可验证的句柄（越界即 `None` 或 panic），`&mut KProcess` 是不可验证的裸能力。

---

## Ch2: 最终接口形态

### 2.1 值参数取代句柄

改写后的签名统一为：

```rust
pub fn kernel_call(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    m_user: minix_types::VirBytes,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
    user_copy: &dyn crate::ipc::UserCopy,
) -> KcallResult
```

（`os/kernel/src/syscall.rs:fn kernel_call（L479，工具生成）`；保存用户态回信地址的那一行与 C 的 `caller->p_delivermsg_vir = (vir_bytes) m_user;`（`minix3/minix/kernel/system.c:141`）对齐，改写后由 `proc_table.get_mut(caller_nr)` 当地完成。）

链顶三件套的形态：

| 函数 | 位置 | 形态 |
|------|------|------|
| `kernel_call` | `os/kernel/src/syscall.rs`（L479，工具生成） | `(caller_nr, proc_table, m_user, priv_table, clock_state, user_copy)` |
| `kernel_call_dispatch` | 同上（L528，工具生成） | `(caller_nr, proc_table, msg, priv_table, clock_state)` |
| `kernel_call_dispatch_inner` | 同上（L561，工具生成） | `(caller_nr, proc_table, msg, priv_table, clock_state, bkl_section)` |
| `kernel_call_finish` | 同上（L3111，工具生成） | `(caller_nr, proc_table, msg, result, priv_table)` |
| `kernel_call_resume` | 同上（L3194，工具生成） | `(caller_nr, proc_table, priv_table, clock_state)` |

`kernel_call_finish` 是写点最密集的地方（挂起上下文、`KCALL_RESUME` 旗标、回信拷贝），改写后每一处写都变成一次独立的 `proc_table.get_mut(caller_nr)`，不再是"跨帧携带的句柄 + 一次洗白"。

### 2.2 统一参数次序

所有转换后的签名采用同一参数次序：**`caller_nr` 在首位，紧跟进程表，其余参数保持原序**。这条约定是给机械改写用的——插入位置全局唯一，改写器不必再维护"每个被调函数的参数次序表"。它来自批 2a 的实证：早期按"就地把 caller 参数替换成 caller_nr"改写时，实参位置与各被调函数参数次序系统性错位，单批 35 处需要人工重排，十轮修复仍不收敛；改为统一次序后一次通过。

### 2.3 用点重借的三种形态

- **只读**：`proc_table.get(caller_nr)`，例如权限门 `caller_has_sys_proc_with_table(proc_table.get(caller_nr)…, priv_table)`（`os/kernel/src/syscall.rs:fn dispatch_schedule（L936，工具生成）`、`fn dispatch_privctl（L1397，工具生成）`、`fn dispatch_vmctl（L2157，工具生成）`）；
- **只写**：`proc_table.get_mut(caller_nr)`，例如 `SYS_PRIV_YIELD` 给调用者置 `RTS_NO_PRIV`（`os/kernel/src/syscall.rs:fn dispatch_privctl（L1397，工具生成）` 内 `privctl_yield` 臂）；
- **顺序读写**：先读后写拆成两次短借用，例如 `kernel_call_resume` 先读挂起上下文、再清 `KCALL_RESUME`、最后交给 `kernel_call_finish`。

`self` 端点解析是只读形态的常见落点：`SELF` 要求把"调用者自己的端点"取出来，改写前读的是句柄字段，改写后是 `proc_table.get(caller_nr).map(|c| c.p_endpoint)`（`os/kernel/src/syscall_clock.rs:fn dispatch_times（L104，工具生成）` 是本战役的模板例）。

### 2.4 两条链顶的衔接

trap 入口不再洗白调用者句柄，而是把进程号直接交给链顶：

- 中断 33（IPC 腿）：`x86_ipc_dispatch_body` 以 `cur_nr` 调 `dispatch_ipc_entry(cur_nr, table, &mut msg, priv_table)`（`os/kernel/src/trap_dispatch.rs:fn x86_ipc_dispatch_body（L287，工具生成）`）；
- `syscall` 指令腿：`x86_syscall_dispatch_body` 以 `cur_nr` 调 `kernel_call(cur_nr, table, m_user, …)`（`os/kernel/src/trap_dispatch.rs:fn x86_syscall_dispatch_body（L401，工具生成）`）。

`cur_nr` 的来源是 per-CPU 存储里的当前进程号（C 侧对应 `get_cpulocal_var(proc_ptr)`，`minix3/minix/kernel/proc.c:601`），与 1.1 节所述的 C 两种风格在 Rust 侧归一为同一种：**入口给号，用点取行**。

### 2.5 闭包带表

跨地址空间拷贝族的 `proc_cr3` 参数是一个闭包，负责把端点解析成页表根。改写前闭包以值捕获调用者句柄，与"同时把 `&mut` 表传进去"冲突（借用检查报 E0502）；最终形态让闭包**显式接收进程表**：

```rust
pub fn data_copy_vmcheck(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    src: AddressRef,
    dst: AddressRef,
    length: usize,
    proc_cr3: &dyn Fn(&ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult
```

（`os/kernel/src/cross_space.rs:fn data_copy_vmcheck（L120，工具生成）`；同族还有 `write_to_process_vmcheck（L196，工具生成）`、`memset_vmcheck（L252，工具生成）`。）底层三个函数 `cross_space_copy`、`cross_space_memset`、`cross_space_write` 同样改为接收 `proc_table` 并调用该闭包（`os/kernel/src/vm.rs:fn cross_space_copy（L333，工具生成）`、`fn cross_space_memset（L386，工具生成）`、`fn cross_space_write（L430，工具生成）`）。闭包形态的这次调整消解了全部 E0502 冲突，也让"页表根从哪里来"这件事在类型上显式。

---

## Ch3: 调用图

```
                     ┌──────────────────────────────────────────┐
  中断 33 (IPC 腿)   │ x86_ipc_dispatch_body(cur_nr)            │
                     │   ├─ dispatch_ipc_entry(caller_nr,…)     │
                     │   └─ kernel_call_finish(caller_nr,…)     │
                     └──────────────────────────────────────────┘
                     ┌──────────────────────────────────────────┐
  syscall 指令腿     │ x86_syscall_dispatch_body()              │
                     │   └─ kernel_call(caller_nr, table, …)    │
                     └──────────────────────────────────────────┘
                                     │
                     kernel_call → kernel_call_dispatch
                                 → kernel_call_dispatch_inner
                                     ├─ 46 个派发臂（Syscall 枚举）
                                     │    ├─ dispatch_schedule / dispatch_privctl
                                     │    ├─ dispatch_trace / dispatch_kill / dispatch_update
                                     │    ├─ dispatch_getinfo / dispatch_sprof / dispatch_diagctl
                                     │    ├─ dispatch_vircopy / dispatch_vmctl / dispatch_abort
                                     │    └─ CurrentArchSyscall::dispatch_{devio,sdevio,vdevio,
                                     │         iopenable,readbios,padconf}
                                     └─ 家族助手（privctl_{yield,set_sys,add_io,add_mem,
                                          add_irq,update_sys}、mcontext 对、copy_struct_*）
                                 → kernel_call_finish(caller_nr, table, …)
                                 → （挂起恢复）kernel_call_resume(caller_nr, table, …)

  跨地址空间族： data_copy_vmcheck / write_to_process_vmcheck / memset_vmcheck
                  → cross_space_copy / cross_space_write / cross_space_memset
                  → 闭包 Fn(&ProcessTable, Endpoint) -> Option<PhysBytes>
```

图中的每个名字都在本文档的锚点表里给出位置；臂的完整清单在 `os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner（L561，工具生成）` 的 `match` 内，46 个枚举成员与共享权威 `minix_types` 的 `KERNEL_CALL` 家族一一对应（由测试 `test_syscall_enum_tracks_minix_types_kernel_call_family` 钉住）。

---

## Ch4: 残余 laundering 清单

设计决定要求批 8 收口时给出"残余 laundering 清单"——即仍需要 `caller_slot_mut` 的调用点。**清单为空**。证据（在仓库根执行）：

```console
$ grep -rn "caller_slot_mut" os/kernel/src/
os/kernel/src/proc_table.rs:636:    pub unsafe fn caller_slot_mut<'a>(&mut self, nr: ProcNr) -> &'a mut KProcess {
os/kernel/src/proc_table.rs:638:            nr_to_idx(nr).unwrap_or_else(|| panic!("caller_slot_mut: invalid proc nr {nr:?}"));
os/kernel/src/proc_table.rs:643:            "caller_slot_mut: slot {nr:?} is SLOT_FREE"
```

三行命中全部位于函数自身（定义与两处 panic 文案），**没有任何调用方**。函数与其注释按设计保留，作为"这条别名契约曾经存在过、以及它证明了什么"的审计记录。

活签名同样归零：

```console
$ grep -rn "caller: &mut KProcess" os/kernel/src/*.rs
os/kernel/src/syscall.rs:798:    // (caller: &mut KProcess, proc_table) to (procs, caller_idx); this
os/kernel/src/proc_table.rs:622:    /// ~139 `caller: &mut KProcess` signatures in the syscall layer) is a
```

两行命中都是注释（IPC 腿改写史与 `caller_slot_mut` 的文档注释），**没有一个活签名**。

---

## Ch5: 备选方案与取舍

### 5.1 方案 B：`SyscallCtx<'a>` 上下文对象

把"表 + 调用者号 + 权限表 + 时钟"打包成一个上下文结构体，提供 `caller(&mut self) -> &mut KProcess` 与 `caller_ro(&self) -> &KProcess`，把借用纪律编码进类型：调用 `caller()` 借走了上下文，同一时刻再取表就不可能。

这个方案比方案 A 更严格，但改动面更大：链上现有的六到七个平铺参数要整体重构，每个调用点都要改写，而且上下文对象与 `kernel_call` 三件套已有的参数（表、权限表、时钟）大量重叠，需要先解决"谁是权威"的问题。收益（更严格的纪律）相对方案 A 的边际很小——方案 A 已经把编译器能管的部分全部交给编译器。

### 5.2 方案 C：维持 K7 单点 + 纪律评审

零改动、零风险，但"纪律靠评审而非类型"的处境不变，138 处签名继续携带句柄，别名风险窗口继续横跨调用链。K20 立项的触发条件（unsafe 审计要求、CpuLocal 语义演进）已经具备，因此不采纳。

### 5.3 为什么是"值参数"而不是"上下文对象"

判据是**风险面与改动面的比值**。方案 A 把 138 个签名换成值参数，用点当地重借，风险面直接消失；方案 B 让风险面消失得更彻底（连顺序重借都要显式声明），但要求把整条链的参数形态推倒重来。当方案 A 已经能让"同时可变的两处借用"在类型上不可能出现时，方案 B 的额外严格性买不到相应的收益。

---

## Ch6: 验证

**宿主测试**。`cargo test -p minix-kernel --lib` 实测 775 通过 / 0 失败 / 8 忽略；本线所辖 crate 全量回归：`minix-arch` 237、`minix-platform` 13、`minix-boot` 17、`minix-rt` 53、`minix-sys` 232、`minix-types` 276、`boot-shim` 13，全部通过。改写期间不新增测试（既有断言即等价证据），只按新接口适配夹具：调用者的身份（端点、权限号）落在它自己的进程表槽位上。

**真机载体**（批 8 收口时实跑）：

| 载体 | 架构 | 覆盖 | 结果 |
|------|------|------|------|
| `test-user-trap` | x86-64 / OVMF | CPL3 用户进程经中断 33 两次进出（未定义调用号 → `EBADCALL`；`MINIX_KERNINFO` → `OK` + 次通道寄存器）+ `SYS_GETINFO` 的 `hz` 写回 | PASS |
| `test-rt-birth` | x86-64 / OVMF | 生产启动路径加载 `minix-rt` 用户 ELF 并交棒（`load_vm_elf` → 段与栈 → `build_cpu_context` → 入口），出生链日志与用户态 panic 回传 | PASS |
| `test-rt-birth-riscv64` | riscv64 / OpenSBI | `ecall` 边界的同一条出生链 | PASS |
| `test-rt-birth-aarch64` | aarch64 / AAVMF | `svc` 边界的同一条出生链 | PASS |

**静态判据**。`caller_slot_mut` 调用方为 0（Ch4 命令）；活签名计数为 0（Ch4 命令）。告警对账按类别与战役起点对照：`unused import` 19 对 19、`unnecessary block` 18 对 18、`unused variable` 7 对 8、`unnecessary mut` 4 对 4，本战役新增项归零。

**批次规模**。战役起点 138 处签名（宽口径：`grep -rn "caller: &mut .*KProcess" kernel/src/*.rs | grep -v "///"`），分布为 syscall.rs 66、misc.rs 27、syscall_copy.rs 14、syscall_process.rs 8、syscall_device.rs 6、syscall_signal.rs 5、syscall_clock.rs 5、cross_space.rs 4、vm.rs 1、kmess.rs 1、grant.rs 1。实施按调用图簇切批：模板批（syscall_clock.rs）→ 硬核批（cross_space + grant + vm + kmess + 六个强制宿主 + `ArchSyscall` trait）→ syscall_signal → syscall_device → syscall_process → syscall_copy → misc → 链顶（syscall.rs 32 个签名 + trap_dispatch 两处调用点）。

---

## Ch7: 边界

**不在本接口内**。IPC 腿内部仍用 `caller_idx`（表内下标）而非 `ProcNr`，这是它自己的既有形态：`dispatch_ipc` 接收下标以便与 `procs` 切片配合（`os/kernel/src/syscall.rs:fn dispatch_ipc（L762，工具生成）`），转换只发生在腿的入口 `dispatch_ipc_entry`。`ProcNr` 与下标之间的换算由 `nr_to_idx` 承担（`os/kernel/src/proc_table.rs:fn nr_to_idx（L1381，工具生成）`）。

**端点解析仍是全局查找**。`SELF` 之外的端点通过 `endpoint_to_nr` 查找（`os/kernel/src/proc_table.rs:fn endpoint_to_nr（L652，工具生成）`），这一步与本接口无关，属于进程表自身的查找语义。

**调用者身份只有一处权威**。改写之后，"调用者是谁"这个问题在链上的唯一答案就是 `caller_nr`，身份字段（端点、权限号、挂起上下文、`p_defer`）一律从进程表槽位读取。测试夹具同样遵守这条：任何需要"调用者带权限"的用例，都把权限号写到进程表槽位上，而不是构造一个游离的 `KProcess` 值——游离值曾经能"看起来像调用者"，现在不能了，这本身就是接口收紧的收益。
