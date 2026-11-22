# P-REFUTE：01-vm-init-main 反证报告（round1）

- 审查对象：`/home/xzhao/github/minix-rs/rewrite-notes/02-stage-vm/01-vm-init-main.md`
- 对照（C）：`/home/xzhao/github/minix-rs/minix3/minix/servers/vm/main.c`、`utility.c`、`alloc.c`、`pagetable.c`；`/home/xzhao/github/minix-rs/minix3/minix/lib/libsys/sef.c`、`sef_init.c`；为核实 VM 映像记账另读了 `/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/protect.c`、`pre_init.c`、`pg_utils.c` 与 earm 同形文件。
- 对照（Rust）：`/home/xzhao/github/minix-rs/os/servers/vm/src/main.rs`、`boot.rs`、`global.rs`、`vm_server.rs`、`acl.rs`、`ipc/transport.rs`；`/home/xzhao/github/minix-rs/os/kernel/src/vm_handoff.rs`、`lib.rs`；`/home/xzhao/github/minix-rs/os/arch/src/arch/boot.rs`、`/home/xzhao/github/minix-rs/os/arch/src/arch/frame.rs`。
- 方法：只读。所有反证以当前工作树的源码为准；证据等级为 L1（逐文件读取）与 L2（grep/sed 重放），无仅凭推断的结论。本报告独立写成，未参考同目录其他 panel 成员的结果。
- 结论：5 条主张中，①④⑤ 有成立的反证（P0-fact 两条、P1 一条），②③ 的最强反论被文档自身的限定或代码事实挡住，不立 FINDING。

---

## FINDING REFUTE-01（P0-fact）：extra_pages 的 VM 映像记账不是 C 的对应物

**主张原文**（文档第 312-314 行，§2.2.5）：

> 语义：`mem_init()` 时 `total_pages` 只统计了空闲块；内核的空闲链表**不含** boot 模块（它们是已占用的 blob），所以分配器需要知道真实物理内存更大。…… Rust 交接页的模块列表已经排除 VM 模块、也不含内核伪条目，因此 Rust 对列表全部条目计费，并单独补 VM 映像页（见 §3.6）。

配合 §3.6（文档第 793-799 行的代码节选、第 811 行的表）：

> | `global::total_pages()` | 可用页 + boot 模块/VM 映像/内核额外页（C 的 `total_pages` + `mem_add_total_pages`） |

**反方最强论据**

1. C 的内核在 VM 开始执行之前就把 VM 自己的模块条目清零，并把 blob 还回空闲 memmap。`arch/i386/protect.c:449-454`：
   `add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start); mod->mod_end = mod->mod_start = 0; kinfo.vm_allocated_bytes = alloc_for_vm;`
   因此 C 的 `main.c:485-490` 循环走到 VM 条目时算出的是 `0 - 0 + 1 = 1` 字节，向上取整为 1 页；它不是“VM 映像长度”。同时 blob 的页已被 `add_memmap` 放回 `kinfo.memmap`，`mem_init` 的 `total_pages`（alloc.c:319-331）会把这些页当空闲页计入。文档“内核的空闲链表**不含** boot 模块”这句话对非 VM 模块成立，对 VM 自己的模块恰好不成立。
2. C 的 VM 段帧与栈帧不是通过 `vm_allocated_bytes` 计入总页数，而是通过 `kernel_allocated_bytes_dynamic`。`libexec_pg_alloc`（protect.c:378-386）调用 `pg_map(PG_ALLOCATEME,...)`，其页由 `pg_alloc_page`/`alloc_lowest` 提供，这两处在 `pg_utils.c:82` 与 `pg_utils.c:154` 累加 `kernel_allocated_bytes_dynamic`；`main.c:492-495` 再把该值计入 `mem_add_total_pages`。`vm_allocated_bytes` 在 C 里只被 `get_usage_info_vm`（region.c:1367-1374）读取，`init_vm()` 从不拿它做总页数校准。
3. Rust 的 `vm_allocated_bytes` 不是 VM 模块 blob 的长度，而是 `vm_alloc.used_frames() * PAGE_SIZE`（vm_handoff.rs:666），即 VmBootAllocator 已发出的全部帧：VM ELF 段页、256 KiB 初始用户栈（arch/boot.rs:425、657-690）、以及 handoff 页本身（kernel/src/lib.rs:3103-3129 先分配 handoff 帧，随后才 `build_vm_handoff`）。它与 C 的 `vm_allocated_bytes = alloc_for_vm`（各 allocmem 区域长度之和）也不是同一个量。
4. Rust 的 VM 模块 blob 永远不在 `free_regions` 里：`classify` 明确不排除 VM 自己的模块（vm_handoff.rs:489-491 注释与 `for_each_live_bootstrap` 的 `for module in modules.iter()`，vm_handoff.rs:764-766），内核注释也写明“The Rust port keeps every module blob reserved instead”（kernel/src/lib.rs:2958，交叉引用见 :3027）。于是这些页既不在空闲区，也没有进入 `extra_pages`，`global::total_pages()` 比 C 少算整个 VM blob。
5. 量级不是零。当前工作树 x86_64 镜像的 `modules/vm` 为 516,944 字节（`CLICK_FLOOR` 下取整 126 页，`roundup` 上取整 127 页；命令见下）。按第 1、4 点，C 对 VM blob 的净计入约为 127 页（126 页由 `add_memmap` 放回空闲区并计入 `total_pages`，清零后的 module 条目再保底计 1 页）；Rust 侧只有 handoff 页 1 页，blob 与 module 条目都不计。两者差额约 126 页（约 0.5 MiB），并随 VM 镜像大小变化。

**代码证据（命令与关键输出）**

```bash
$ sed -n '448,455p' minix3/minix/kernel/arch/i386/protect.c
		/* Free VM blob that was just copied into existence. */
		add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start);
		mod->mod_end = mod->mod_start = 0;

		/* Remember them */
		kinfo.vm_allocated_bytes = alloc_for_vm;

$ grep -n "vm_allocated_bytes: vm_alloc.used_frames" os/kernel/src/vm_handoff.rs
666:        vm_allocated_bytes: vm_alloc.used_frames() * PAGE_SIZE,

$ grep -n "handoff_frame = vm_alloc" os/kernel/src/lib.rs
3103:                let handoff_frame = vm_alloc

$ grep -n "for module in modules.iter()" -A 2 os/kernel/src/vm_handoff.rs
764:    for module in modules.iter() {
765:        f(module.start.0, module.len as u64);

$ ls -l os/target/image/x86_64/staging/EFI/minix/modules/vm
-rwxr-xr-x 1 xzhao xzhao 516944 Nov 22 05:52 .../modules/vm
```

earm 同形处理在 `minix3/minix/kernel/arch/earm/protect.c:177-181`（`add_memmap` + 清零 + `kinfo.vm_allocated_bytes = alloc_for_vm`）。

**影响**：只影响 `global::total_pages()` 这一本账（`os/servers/vm/src/query.rs:308` 的 `VSIW_STATS` 口径、26 篇的查询语义），不影响分配器的空闲页，也不影响启动流程。但文档把这条账写成与 C 同口径，并据此说明“单独补 VM 映像页”的理由，前提对 C 不成立。

**若要推翻反方需要什么**

- 证明 `arch_boot_proc`（protect.c）在 VM 的 `init_vm()` 之后才运行——它与 VM 被内核调度先后的代码顺序相反；
- 或证明 Rust 的交接页把 VM blob 放回 `free_regions`，或把 blob 计入 `extra_pages`——`classify`/`for_each_live_bootstrap` 与内核注释都表明没有；
- 或证明 Rust 的 `vm_allocated_bytes` 等于 C 的 VM 模块长度——它是 `used_frames()`，含 handoff 页与 256 KiB 栈帧。

满足任一条，本节反证不成立。

**修复方向（最小）**：§2.2.5 第 1 条改写 C 的实际处理（VM 条目清零后只计 1 页；blob 由 `add_memmap` 归还为空闲；VM 段/栈帧走 `kernel_allocated_bytes_dynamic`），§3.6 不再以“C 的 module loop 收了 VM 映像”为前提，并同步 `os/servers/vm/src/boot.rs:276-280` 的代码注释（那里写着 “C's module loop charged it because VM's module was still in `kernel_boot_info.module_list`”）。

---

## FINDING REFUTE-02（P0-fact）：§3.5 的“唯一口径”表把已实现的栈帧/exec 写成未实现

**主张原文**（文档第 766-773 行，§3.5“当前实现边界”，并声明“本节唯一口径，§3.9 与 §4.4 都以此为准”）：

> | `minix_stack_*` 初始栈帧 | 未实现；`stack`/`ps_str` 以 0 上报 |
> | `sys_exec` | 已实现（`KernelGateway::sys_exec`，真实 `vsp`/`ps_str`） |

**反方最强论据**

1. 同一节的上方表格（文档第 761 行）写的是相反结论：“栈建立（`minix_stack_*`）｜**已实现**：`install_boot_stack` 用 `minix_sys::stack::stack_params` 定尺寸、`stack_fill` 布局 argc/argv/envp/ps_strings，建立 4 MiB 栈区并经 `handle_memory_once` 映射校验，帧字节经 Direct Map 写入；`sys_exec` 使用真实 `vsp`/`ps_str`。唯一余项是 `name_ptr = 0`。”
2. §3.10 小结（文档第 847 行）再次写“`exec_bootproc` 的装载、**栈帧**、`sys_exec` 与 `VMCTL_BOOTINHIBIT_CLEAR` 已实现；`name_ptr` 仍以 0 传递”。
3. 代码与第 1、2 点一致，与“唯一口径”表冲突：`os/servers/vm/src/vm_server.rs:1147` 在 `exec_bootproc` 尾部调用 `self.install_boot_stack(ip.endpoint, ip.name(), entry)`；`vm_server.rs:1175-1187` 用 `minix_sys::stack::stack_params`/`stack_fill` 生成帧，`vm_server.rs:1211-1226` 插入 4 MiB 栈区，`vm_server.rs:1315-1317` 以真实的 `vsp` 与 `filled.ps_str` 调 `sys_exec(endpoint, entry, vsp, 0, filled.ps_str)`；`minix-sys/src/stack.rs:207-210` 返回 `vsp` 与 `ps_str = vsp + ps_off`。
4. 这一行很可能来自过期的代码注释：`vm_server.rs:898-903`（`exec_bootproc` 文档注释，原文 “until it lands, `stack`/`ps_str` are reported as 0”）与 `vm_server.rs:843-848`（`init_boot_procs` 文档注释，原文 “so `stack`/`ps_str` report 0 until it lands”）在 `install_boot_stack` 落地后都已不成立。
5. 真实的边界只是“boot 循环的调用点不在宿主测试形态”：`exec_bootproc`（含对 `install_boot_stack` 的唯一生产调用点）整体在 `#[cfg(not(test))]` 下（`vm_server.rs:915`）。但 `install_boot_stack` 本身没有被 cfg 门住，而且有一个直接的宿主测试 `test_install_boot_stack_writes_frame_and_exec_values`（`vm_server.rs:4090-4184`）断言 `sys_exec` 的 `stack = vsp`、`ps_str` 为绝对地址、帧字节落在 `vsp`。也就是说，“未实现；`stack`/`ps_str` 以 0 上报”连“宿主测试看不到”都解释不了。
6. 与此相关的是 §5.2 的测试清单（文档第 1066-1078 行）只列了 8 个 `vm_server.rs` 测试，漏掉了上面这个直接覆盖 §3.5 栈帧边界的 `test_install_boot_stack_writes_frame_and_exec_values`；这是文档测试清单的覆盖缺口（测试存在，文档没列）。

**代码证据（命令与关键输出）**

```bash
$ grep -n "install_boot_stack(ip.endpoint" os/servers/vm/src/vm_server.rs
1147:        self.install_boot_stack(ip.endpoint, ip.name(), entry)

$ sed -n '1315,1318p' os/servers/vm/src/vm_server.rs
        let mut gateway = self.ctx.gateway.borrow_mut();
        gateway
            .sys_exec(endpoint, entry, vsp, 0, filled.ps_str)

$ grep -n "ps_str: vsp + ps_off" os/libs/minix-sys/src/stack.rs
209:        ps_str: vsp + ps_off as u64,

$ grep -n "report 0 until it lands\|are reported as 0" os/servers/vm/src/vm_server.rs
847:    /// shared ABI), so `stack`/`ps_str` report 0 until it lands; real
902:    /// until it lands, `stack`/`ps_str` are reported as 0 and the boot

$ grep -n "fn test_install_boot_stack_writes_frame_and_exec_values" os/servers/vm/src/vm_server.rs
4090:    fn test_install_boot_stack_writes_frame_and_exec_values() {
```

**影响**：读者按“唯一口径”表会得出栈帧 ABI 未实现、`stack`/`ps_str` 恒为 0 的结论，与同篇三处叙述、代码以及一个专门的宿主测试都相反；也把 §3.10 的“唯一余项是 `name_ptr = 0`”变成无来源的断言。这是文档内部矛盾，不是代码缺陷。

**若要推翻反方需要什么**

- 证明 `install_boot_stack` 未被生产路径调用（唯一调用点在 `exec_bootproc`，生产构建编译并调用，见上）；
- 或证明 `filled.ps_str` 不是传给 `sys_exec` 的真实值（`vm_server.rs:1317` 直接传，宿主测试 `vm_server.rs:4147-4148` 也断言了 `stack = vsp`、`ps_str = placement.ps_str`）；
- 或证明“未实现”指的是宿主测试形态（该函数有直接的宿主测试，反而证明它是已实现且已覆盖的）。

**修复方向（最小）**：把该行改为“已实现（生产路径与宿主测试均覆盖）”，与 §3.5 前表、§3.10 对齐；把 `test_install_boot_stack_writes_frame_and_exec_values` 补进 §5.2 的测试表；同时修掉 `vm_server.rs:843-848` 与 `898-903` 两段过期注释（文档与代码同步项）。

---

## FINDING REFUTE-03（P1）：A-14 的“没有半初始化状态”论证对 rs_handshake 不成立

**主张原文**（文档第 740 行，§3.4 表格 A-14 行）：

> C 在 main.c:151 panic；Rust 选择让 VM 存活并把错误交给 RS [ARCH: A-14]。这条豁免以当前生产只走 `ColdInit` 为前提：`init_vm()` 已完成、**没有半初始化状态需要回滚**，RS 可以按启动失败处理。

**反方最强论据**

1. `rs_handshake()` 不是原子的：它在同一次循环里逐条 `set_acl`，任何一条失败就提前返回（`vm_server.rs:2148-2172`，`let slot = table.vm_isokendpt(entry.endpoint).map_err(...)?` 与 `get_active(...).ok_or(...)?` 都在循环内，成功条目已经把真实掩码写进槽）。因此 rproctab 获取失败时确实没有 ACL 写入，但循环内第 k 条之后遇到无效 endpoint 会留下 k 条真实掩码、其余槽仍为 `AclState::Uninitialized`。
2. `AclState::Uninitialized` 在这棵树里不是拒绝一切，而是按 `DEFAULT` 放行（`os/servers/vm/src/acl.rs:96-110`：`VM_EXIT`/`VM_FORK`/`VM_BRK`/`VM_EXEC_NEWMEM`/`VM_WILLEXIT`/`VM_MMAP`/`VM_MUNMAP`）。也就是说，部分应用的 ACL 表对未处理到的系统服务是**更宽松**的，不是“无状态可回滚”。
3. 失败后 VM 不停止服务：失败臂返回 `DispatchAction::NoReply`（`vm_server.rs:2004-2041`），而 `run_once` 对 `NoReply` 只是不回复（`vm_server.rs:1871-1873`），主循环继续收下一条消息。RS 收到 `sef_init_reply(ESRCH)` 之前，VM 已经可能按这张部分 ACL 表服务内存请求。
4. 顺带的事实：失败应答里的错误码不是内核的原始 safecopy 错误，而是 `ipc_call_rs_init` 把任何 safecopy 失败映射成 `VmError::InvalidEndpoint`，再折成 `ESRCH`（测试 `test_run_once_rs_init_fails_closed_on_safecopy_error` 明确断言 `ESRCH`，`vm_server.rs:4426-4595`，断言在 :4595）。文档写“先回 `sef_init_reply(errno)`”，读者会以为是内核错误码透传。

这不是反对 A-14 的取舍本身（C 的 panic 会让系统停摆，Rust 选择存活是已裁决的架构决定），而是反对文档为它给出的前提。A-14 真正需要的理由是“部分 ACL 表可接受，且 RS 会在收到失败应答后重启/终止 VM”，文档应把这个后果写出来，或让握手事务化（先算完掩码再提交）。

**代码证据（命令与关键输出）**

```bash
$ sed -n '2148,2172p' os/servers/vm/src/vm_server.rs
    fn rs_handshake(&mut self, init: &minix_types::RsInit) -> Result<(), VmError> {
        let table = VmProcTable::get_global();
        let rproctab = ipc_call_rs_init(self, init.rproctab_gid)?;
        for entry in rproctab.iter() {
            if !entry.in_use { continue; }
            let slot = table.vm_isokendpt(entry.endpoint)
                .map_err(|_| VmError::InvalidProcess)?;
            let mut proc = table.get_active(slot).ok_or(VmError::InvalidProcess)?;
            ...
            proc.set_acl(crate::acl::AclState::acl_set(is_sys, mask));
        }
        Ok(())
    }

$ sed -n '96,110p' os/servers/vm/src/acl.rs
            AclState::Uninitialized => {
                // SECURITY FIX [ARCH: A-11]: Restrict to DEFAULT calls instead
                // of allowing all.
                ...
                if AclMask::DEFAULT.contains(call_flag) { Ok(()) }
                else { Err(VmError::PermissionDenied) }

$ sed -n '1871,1873p' os/servers/vm/src/vm_server.rs
            DispatchAction::Suspend => {}
            DispatchAction::NoReply => {}
```

**影响**：A-14 的文档依据不成立会掩盖一个真实的安全后果（失败后部分系统服务以 DEFAULT 权限被服务）。如果将来有人按“没有半初始化状态”去复用这段逻辑（例如 WarmRun 路径，文档同段已提出这一前提），风险面会扩大。

**若要推翻反方需要什么**

- 证明 `rs_handshake` 在全部条目校验通过之前不写任何 ACL（代码是边校验边写，失败即 `?` 返回）；
- 或证明失败后 VM 在 RS 处理前不再服务任何消息（`NoReply` 后主循环继续，没有门）；
- 或证明 `AclState::Uninitialized` 对未处理服务是拒绝而非 DEFAULT 放行（acl.rs 显示相反）。

**修复方向（最小）**：§3.4 的 A-14 行改写为“失败时可能留下部分 ACL 表（未处理槽为 `Uninitialized`，按 DEFAULT 放行），RS 收到 ESRCH 后按启动失败处理”；若不愿接受这个后果，把 `rs_handshake` 改成两段式（先收集全部 `(slot, mask)`，再统一 `set_acl`）。

---

## 主张 ②：is_first_time 四态路由与 C 门控等价（反证不成立）

**主张原文**（文档第 683-706 行，§3.2）：

> **C**：`is_first_time()` 通过 `sys_getproc(RS_PROC_NR)` 读内核进程表。**Rust**：`BootParams::is_first_time` 由交接页携带；当前内核构造交接页时**恒写 1**……因此四态路由在当前生产路径上只有 `ColdInit` 可达。

文档第 706 行还写：“`WarmRun` 在当前生产路径上不可达……当前四态表只用于诊断，不应被读成四条都可用。…… C 的 restart 由 SEF/LU 处理，Rust 尚未实现；不要用 C 的 restart 行为推断 image slot 里的内容。”

**反方最强论据**

- 四态路由器的两个输入是“内核恒写的 1”（`os/kernel/src/vm_handoff.rs:713`）与“本进程静态槽是否为空”（`vm_server.rs:222`、`256-259`），它根本不查询 RS 的 `RTS_BOOTINHIBIT`，因此不具备 C 门控的语义（C 的门控是运行期读内核进程表）。
- C 的 false 分支只有一种行为：SEF 的 LU/restart 状态恢复；Rust 把 false 拆成 `WarmRun`（槽非空）与 `WarmNoState`（槽空）。而一个被重新 exec 的 VM 进程其静态槽必然为空，`main()` 也不会在同一进程里被第二次进入，所以 `WarmRun` 不是“LU/restart 未接”才不可达，而是结构上不可达；反过来，一旦交接页真的透传 C 的 false，正常重启形态会落到 `WarmNoState` 的 panic，而不是 C 的恢复路径。
- 表里“`true` + 有 server = `DoubleInit`”同样要求 `main()` 二次进入，进程入口只跑一次。

**代码证据**

```bash
$ grep -n "is_first_time: 1" os/kernel/src/vm_handoff.rs
713:        is_first_time: 1,
$ sed -n '245,253p' os/servers/vm/src/vm_server.rs
pub fn boot_route(is_first_time: bool) -> BootRoute {
    let installed = server_installed();
    match (is_first_time, installed) { ... }
}
$ sed -n '81,130p' os/servers/vm/src/main.rs   # ColdInit 以外的三臂：WarmRun 空臂、两个 panic
```

**为什么不在本轮立 FINDING**：文档自己已经把“等价”降级为“诊断占位”，明写只有 `ColdInit` 可达、明写 C 的 restart 由 SEF/LU 处理且不要用 C 的行为推断 slot 内容；`WarmRun` 的条件也被后续段落补成“将来接入**同进程**热重启”。也就是说，本轮任务表述中的 ②“与 C 门控等价”并不是文档的主张，而是文档明确否认的主张。残留的只是表格单元格“否（LU/restart 未接）”把不可达原因写得偏窄（真正缺的是同进程重入），属措辞级，不构成 P0/P1/P2 的实质反证。

**若要推翻反方需要什么**：证明文档确实声称等价（它没有），或证明存在一条生产路径把真实 `is_first_time` 送进 VM 且在同一进程内重入 `main`（本树没有）。

---

## 主张 ③：BootTables/BootInit 的四步顺序与 C 一致（反证不成立）

说明：`BootTables`/`BootInit` 这两个名字不在本文档里（它们是 `rewrite-notes/meta/design/03-stage-rs/01-design.v1.md` 的概念）。按本文档的实际内容，本条对应 §4.4.2 的“Phase 1/2a/2b/2c/2d 与 C `init_vm()` 的对应”及 §3.3 的“核心结论”表。

**主张原文**（文档第 984-989 行，§4.4.2）：

> `kernel_layout` 有两份载体但不是两份事实…… A–E 的大阶段顺序不变，但 C 的 `init_proc(VM_PROC_NR)` → `pt_init` 在 Rust 中变成 `init_vm_self_pt()` → `init_vm_slot()`：VM 自身页表先于 VM 槽建立。`init_vm_self_pt()` 只 adopt 内核引导根，不读 `vmprocess` 槽，因此这个对调不改变依赖；读者不要把它读成“阶段顺序完全一致”。

**反方最强论据**

- 顺序并非“不变”，至少有四处可指：C 的 `__minix_init()`（main.c:480）在 `mem_add_total_pages`（485）与 boot 进程 exec（498-520）之前，Rust 的 `mark_initialized()` 在 `mark_vm_instance()` 之后（`vm_server.rs:721-730`）；C 的 `pt_init`（475）在 `init_proc(VM_PROC_NR)`（474）之后，Rust 的 adopted root 在构造期、早于 `init_vm_slot()`；Rust 在 C 对应步骤之间插入了 `register_page_alloc`/`relocate`（676-681）与 `PageFrames::new`（702-704）两个 Rust-only 步骤；C 的 `init_proc` 不建页表，Rust 的 `init_proc` 会建（`vm_server.rs:1372`、`1392`），VM 槽也不例外，而 C 的 VM 槽页表由 `pt_init` 建（`pagetable.c:1253-1309`，`newpt = &vmprocess->vm_pt; pt_new(newpt)`）。
- 尤其最后一条使“`init_proc(VM)` → `pt_init` 变成 `init_vm_self_pt()` → `init_vm_slot()`”这一句并非一一对应：Rust 的 `init_vm_slot()` 自己还做了一次 `init_page_table()`（给 VM 槽建了另一张表），adopted root 与槽页表并存。

**代码证据**

```bash
$ sed -n '649,731p' os/servers/vm/src/vm_server.rs   # init() 的实际次序
$ sed -n '824,839p' os/servers/vm/src/vm_server.rs   # init_vm_slot -> init_proc
$ sed -n '1353,1396p' os/servers/vm/src/vm_server.rs # init_proc: init_page_table / init_regions / set_boot
$ sed -n '1253,1256p' minix3/minix/servers/vm/pagetable.c
        newpt = &vmprocess->vm_pt;
	if(pt_new(newpt) != OK)
		panic("vm pt_new failed");
```

**为什么不在本轮立 FINDING**：文档在紧邻的下一段已经逐条列出两处完成时机差异（`pt_init` 在构造期、`mark_initialized()` 在末尾），并明确写“读者不要把它读成阶段顺序完全一致”；`init_vm_self_pt` 只 adopt 引导根、不读 VM 槽这一点也与 `pagetable/vm_self_map.rs:175-182` 一致。因此“顺序全同”的反论打不到文档的主张。唯一残留是 §4.4.2 的 `init_vm_slot()` 条目只写了 `activate` + `set_boot`，漏了同一次调用链里还会执行 `init_page_table()` 与 `init_regions()`；这是“实现详解”章对一步的描述不完整，属 P2 级措辞，建议在 panel 合并时一并补一句，不单列 FINDING。

**若要推翻反方需要什么**：证明文档声称“完全一致”（它明确否认），或证明上述四个可指的顺序差都不存在（代码显示存在）。

---

## SUMMARY

| 主张 | 判定 | ID / 级别 |
|---|---|---|
| ① `extra_pages` 的模块/VM 映像记账与 C 等价 | 反证成立 | REFUTE-01（P0-fact） |
| ② `is_first_time` 四态路由与 C 门控等价 | 反证不成立：文档已明确声明不等价、只有 `ColdInit` 可达 | — |
| ③ `BootTables`/`BootInit` 四步顺序与 C 一致 | 反证不成立：文档已列明两处时机差并否认“完全一致”；仅 §4.4.2 对 `init_vm_slot` 的描述漏了 `init_page_table`/`init_regions`（P2 级措辞） | — |
| ④ SEF fresh 握手与 C 语义等价 | 成功路径的协议对应成立；失败路径（A-14）的“没有半初始化状态”论证不成立 | REFUTE-03（P1） |
| ⑤ 栈帧/exec 边界描述准确 | 反证成立：§3.5 的“唯一口径”表与同节前表、§3.10、代码均矛盾 | REFUTE-02（P0-fact） |

- 计数：P0=2、P1=1、P2=0。表中 ③ 行提到的 §4.4.2 `init_vm_slot` 描述不完整是观察项，未立 FINDING，故不计入 P2。
- 证据强度：REFUTE-01 依赖 `protect.c`/`pg_utils.c`/`vm_handoff.rs`/`lib.rs` 的逐行内容（L1）与 grep 重放（L2）；REFUTE-02 依赖 `vm_server.rs` 的调用点与 `stack.rs` 的返回值（L1）；REFUTE-03 依赖 `rs_handshake` 的循环体与 `acl.rs` 的 `Uninitialized` 分支（L1）。三条均无 L3 推断结论。
- 同 agent 局限：本报告的验证者与被验证者是同一 session；上述命令均已在本会话内重放，仍建议 panel 汇总时跨工具只读复核 REFUTE-01 的 C 内核侧行号与 REFUTE-02 的 `install_boot_stack` 调用点。
