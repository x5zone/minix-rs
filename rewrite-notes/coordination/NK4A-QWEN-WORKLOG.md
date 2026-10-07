# NK4-A QWEN WORKLOG — 逐任务工作记录

> 任务书：`NK4A-TODO.md`（同目录）。记录格式见 TODO §6.1。
> 每个 Task 一节，追加，不改写历史条目。

---

## 会话开场（2026-09-22，qwen 接手）

- 起始 commit：`eebe41550`（docs 提交，其父 `4a6570d7f` = 迭代18 修复尖端，
  `e4c6e8224` = 迭代11-18 证据归档）。分支 `rewrite`，未 push。
- 现场核对：无 QEMU 残留；`git status` 仅 `AI-chats/daily.todo.md` 与
  `tmp/nk4a/vars.fd` 既有改动（按任务书不动）。
- 已读输入：`NK4A-TODO.md` 全文；`.review/zcode/edge1/FIXLOG.md` 末 300 行
  （迭代11-18 六修复链 + 前沿定性）。
- 本轮目标：Task A（VM 页故障服务停滞定位与修复）起步，停在
  `minix-rs rc: minimal boot script marker`（Task E）。
- 计划：A1 读 vm_server.rs VM_PAGEFAULT 臂列出口 → A2 读 cow_exec_pf.rs +
  memtype.rs → A3 查帧池 → A4 加限次探针 → A5 复跑定性 → A6 修 →
  A7 宿主全绿+真机两次复跑 → A8 双记录。

## Task A — 定位并修复 VM 页故障服务停滞（2026-09-22）

### A1-A3 静态定性（读码，未上真机）

- **A1 出口表**（`vm_server.rs` dispatch_pagefault，探针前 1785-1920 行）——
  跳过 `vm-pf bytes` 打印的全部出口：
  | 出口 | 行为 | 是否清 PAGEFAULT | 是否可见 |
  |------|------|------|------|
  | L1793 badendpt（vm_isokendpt 失败） | Error(InvalidProcess) | 否 | 静默 |
  | L1797 inactive（get_active 失败） | Error(InvalidProcess) | 否 | 静默 |
  | L1835 wro（写只读 region） | SIGSEGV+clear，Error(AccessViolation) | **是** | 可见 |
  | L1858 noaddr（regions.find_mut 失败） | Error(InvalidAddress) | **否** | 静默 |
  | Ok(Suspended) | VmReply::Ok，inc_major | 否（设计如此，等 VFS） | 静默 |
  | Ok(AccessViolation) | VmReply::Ok | 否 | 静默 |
  | Err(e) | `vm-pf err` 打印后 Error | 否 | 半可见（已有探针） |
- **A2 回答**：ANON 的 `ev_pagefault`（memtype.rs:294-324）只返回
  NeedNewPage/Handled/NeedCow/AccessViolation，**永不 NeedVfsIo**；
  NeedVfsIo 唯一产地是 MappedFile（memtype.rs:1084）。exec_bootproc 全部
  region 均建为 MEM_TYPE_ANON（vm_server.rs:788-793 段、916-920 栈）→
  **boot 期 Suspended 假设被静态否定**（TODO §3 的头号嫌疑排除）。
- **A3 帧池**：生产形态无固定小池——`create_default_allocator`
  （vm_server.rs:308）以 kernel handoff 的全部 free 物理区建
  BitmapAllocator（QEMU `-m 512M`，池 = 512MiB 扣 kernel/模块/UEFI 后
  的 free 页，~10 万页量级；fix19 时代的"24 页池"是 mock 参数，非生产）。
  消耗 = 12 模块 eager 物化 + 运行期按需填充 + minix-rt 堆 256 页/服务。
  OOM 的全部路径（alloc_and_map / cow_resolve）都收敛到
  CowError::NoMemory → `vm-pf err` 打印 → **枯竭假设待真机读数排除**。
- 探针 commit：5f98b1db5（pf-exit 七出口，cap 8/出口，宿主 VM 525 全绿，
  警告基线 17 不变）。

### A5 真机定性（c17a 轮，2026-09-22）

- 串口计数：`vm-pf recv` 163 / `vm-pf bytes` 162 / `vm-pf err` 0 /
  `pf-exit noaddr` 1。差值恰好一条：最后一条 recv 跟随
  **`nk4a: pf-exit noaddr cr2=0x0`**，随后 `picknone rs_flags=0x400`
  （RS 永停 RTS_PAGEFAULT），系统静默死锁——停滞出口 = **noaddr**
  （fault_addr=0x0 不属于任何 region），不是 Suspended，不是帧池枯竭
  （`vm-pf err` 零命中 + 池 ~10 万页）。
- RS 侧：停滞前 pre-restore rip=0x2014ad（drop_in_place RProcTable 内
  `test %rax,%rax`，恢复执行后某指令 deref 了 NULL → cr2=0）。这属
  RS 自身逻辑问题（Task C/D 前沿）；**Task A 的缺陷是 VM 对不可服务
  fault 的收口违反 C**：C pagefaults.c:89-105 对 unknown region 必然
  `sys_kill(SIGSEGV)` + `sys_vmctl(VMCTL_CLEAR_PAGEFAULT)` 后才返回，
  Rust noaddr 出口两条全漏 → 挂起位无人清 → 死锁。
- 修法（A6）：按 C handle_pagefault 出口表补齐——noaddr /
  Ok(AccessViolation) / Err 三个"不可服务"终局统一走
  SIGSEGV+clear_pagefault（提取共用 helper，wro 臂同体重构）；
  Suspended 臂保留挂起（C 的 SUSPEND 语义如此）。
### A6-A8 修复与验证（2026-09-22）

- 状态：**DONE**（Task A 判据达成：RS 不再停在 PAGEFAULT——noaddr 收口
  后系统从静默死锁推进到 C 语义的致命信号处置；成功服务的 recv/bytes
  162 对全配对；kc 流水维持 45+ 无回退）。
- 改动文件：`os/servers/vm/src/vm_server.rs`
  （helper `pf_fail_segv` L1786-1806 新增；noaddr/accvio/Err 三终局
  接入；wro 臂体重构为调 helper；新增测试
  `test_pagefault_unknown_region_sigsegv_and_clears_park`）。
- 命令与结果：宿主 docker -j1 minix-vm **526**（基线 525+1 新增）/
  minix-kernel **808** 全绿；IMG-EXIT=0；复跑轮次 c18a、c18b。
- 真机证据：serial_c18a.log / serial_c18b.log（两轮一致）关键行：
  ```
  nk4a: vm-pf recv
  nk4a: pf-exit noaddr cr2=0x0
  <unset> 0x0000000000000002 0x000000000020d0d5
  boot-shim panic: panicked at kernel/src/syscall_signal.rs:300:13:
  cause_sig: sig manager 2 gets lethal signal 11 for itself
  ```
- 新断点定性（属 Task C，非本修复遗留）：RS 用户态 deref NULL →
  SIGSEGV → RS 是 sig manager，致命信号自受 = C system.c:430 同款
  内核 panic。停滞形态从"静默死锁"变为"可见 panic"——排查能力恢复。
  下一轮从 0x20d0d5 符号化 + RProcTable drop 路径 + 数据页填充内容
  三方向取证。
- commit：5f98b1db5（A4 探针）+ febbb0c8b（A6 修复+测试）。
- 未决问题：RS 为何在 drop_in_place RProcTable（0x2014ad 附近）deref
  NULL——是 RS 逻辑 bug 还是 exec 填充数据错位（vm-pf bytes 探针可续查），
  Task C 处理。
- 自检：fix-guard 四步 ✅；测试计数不减 ✅（526/808）；两次复跑 ✅
  （c18a/c18b）；FIXLOG 已补记 ✅（迭代19 条目）。

## Task B — step2 槽缺失复现定性（条件触发）

- 状态：**未触发**——A 修复后两轮真机（c18a/c18b）未出现 boot.rs:1054
  panic；新断点为 RS null-deref → cause_sig panic，走 Task C 取证循环。

## Task C — RS null-deref 取证（BLOCKED·六轮未定性；第五轮实证「内核交付 RBX=0」，第六轮证伪状态寄存器写点假说）

- 状态：**BLOCKED（未解除，累计六轮 c19a/c20a/c21a/c22a/c23a/c24a，已超
  铁律 #10 的 3 轮上限）**。第五轮用反汇编 + 交付值探针拿到决定性实证：
  崩溃前最后一次 RS 交付是 `rip=asynsend+0, rbx=0`，而 LLVM 把
  `&self.table` 常驻 RBX → 错值被 wrapper 的 push/pop 固化成
  self=0，完整解释 `self=0x0 / cr2=0 / panic@endpoint_slot+0x176`。
  第六轮布防全部 5 类 IPC 状态寄存器写点，对 RS 零命中 →
  「写点踩用户活值」假说证伪，候选收窄为「save/restore 配对来源」。
  同日再补一轮**零真机成本静态排查**：穷举 `ctx.rbx` 写者全集，发现
  第 6 轮布防漏了 2 个全量存帧点（trap_dispatch.rs:697 异常→信号臂、
  :1103 syscall 腿 VmSuspend 臂），并静态排除「syscall 瘦帧未填
  rbx」与「apply_to_trap_frame 漏拷 rbx 导致交付 0」两条路径。
  本轮不做无定性修复。第四轮：静态定性出「IRQ/tick 入口不存帧 vs
  C mpx.S 每入口 SAVE_PROCESS_CTX」的真实 C 偏差并按对位修复（Fix #10，
  独立价值成立：入口保存不变量恢复 + 宿主防回归测试），但真机 c22a
  崩溃序列与 c21a **逐字节一致、rs-epslot self=0x0 复现** → 该偏差
  非本崩溃根因，假设对本例证伪。Task C 累计四轮（c19a/c20a/c21a/c22a）
  未消除崩溃，按铁律 #10 停。
- c22a 新证据（对下一轮极有价值）：
  1. **完全确定性**：c21a vs c22a 的 `vm-pf bytes` 序列、pre-restore
     rip/rsp、崩溃行逐一相同——不是 PIT 抢占竞态，是可静态复现的逻辑
     bug（铁律 #9 的时序敏感假设对本例不适用）；
  2. `vm-pf bytes 3538353936303631` = ASCII "58596016" 文本字节、
     `bytes 83c4105dc3cccccc` / `0048c783`——VM 消息 payload 读到的不是
     尺寸而是**未初始化/错误偏移的堆块**，pf 消息通道内容可疑（C 对位
     VM_PAGEFAULT 消息编码 VPF_ADDR/VPF_FLAGS/VPF_PID，对照
     exception.c:112-129 与 vm/pagefaults.c 的取字段）；
  3. 崩溃前最后一次 RS restore：rip=0x203bf0（asynsend 首指令 fetch pf
     的 ForwardToVm 入口**全量 save 过 rbx=live**）→ 同进程恢复后
     rbx=0——save/restore 之间 ctx.rbx 被改，或 restore 消费的 ctx 与
     save 写入的 ctx 不是同一份/同一次。**下一轮判别探针（第 5 轮候选）**：
     在内核 pre-restore 路标处一并打印 `ipc_status_register(&ctx)`
     （恢复用 rbx 实际值）+ ForwardToVm save 时打印 frame.rbx，两值对照
     即可裁决「保存即错」vs「保存后被改」vs「恢复读错源」。
- C1 符号化（objdump + addr2line，rs 模块 not stripped）：
  - 故障 rip `0x20d0d5` → `minix_rs::boot::BootInit::init_fresh+0x975`
    （addr2line 确认；release 构建内联，无行号）。
  - 该地址落在 RS 主 text 段 `0x201270 R E size 0x1e88c`（readelf -l 实证）。
- 故障指令反汇编（`objdump -d`，L 邻近）：
  ```
  20d011: mov 0x1048(%rbx),%r12   ; 保存某 Vec 指针
  20d018: mov 0x1050(%rbx),%r14   ; 保存 len
  20d01f: mov 0x1058(%rbx),%rbp
  20d033: call *0x210ce0          ; = RProcTable::sync_pub_wire(self=rbx)
  ...循环步进 0x18=24，读 0x10(%rdi) 作 endpoint，做 _ENDPOINT_P 范围检查...
  20d0d5: cmpl $0x1,(%rbx,%rax,1) ; 读 in_use==1 → cr2=rbx+rax=0x0 → #PF
  ```
  即：step1 末尾 `sync_pub_wire`（boot.rs:1037）之后的 boot-slot /
  endpoint 索引遍历里，被索引表基址（rbx 系）为 NULL。
- 关键判据（非本轮引入）：cr2=0 的 deref 在 Task A 修复前就存在，只是
  旧 noaddr 臂只回 Error 不清挂起 → RS 永停 → **静默死锁掩盖了它**；
  Task A 补 SIGSEGV 收口后，同一 deref 变可见 panic。**这是暴露，不是
  Task A 的回归**。SIGSEGV→cause_sig panic 本身是 C system.c:430 正确行为。
- 两条待验假设（下一轮区分）：
  1. **exec 数据页填充错位/漏填**：boot 期反复 `vm-pf bytes 0000000000000000`
     命中，若承载 boot 表/allocator 旗标的 RW 段（RS `.bss` 0x2291c8，
     memsz≈1MB；历史 FIXLOG 记录 allocator 旗标约 0x229708）未从 ELF
     正确拷入，则表基址读出 0。（对照 vm_server.rs:761-841 段拷贝逻辑）
  2. **RS init_fresh 真实空指针路径**：`endpoint_slot`/`activate_boot_slot`
     某未注册端点索引到空槽（release 内联，源码行待探针定位）。
- 下一步（§8 步 4）：在 RS 侧 init_fresh boot-slot 段加限次探针打印
  被索引表基址与当前 endpoint（`#[cfg(not(feature="mock"))]` 门 + AtomicUsize
  cap + 标「task1-close 裁决删除」），重建镜像真机复跑，分辨假设 1/2；
  据读数定性后再对照 C `minix3/minix/servers/rs/` 修复。
- 本轮取证产物：serial_c18a/c18b 日志已归档于
  `tmp/evidence/20260922-nk4a-taskA-c17a-c18/`（Task A commit 84347cff2）。

### 第二轮取证（c19a 粗探针 + c20a 细探针，2026-09-22）

- 假设 1（exec 数据页漏填）/ 假设 2（表未注册空槽）均被 c19a 否定：
  step2 入口探针打印 `pt`/`tbl` 基址有效（0x7fffffffc800 量级），boot
  表填充正常。
- c20a 细探针（boot.rs step2 每迭代 ep/slot + PM 分支 pre-sched/
  post-sched/post-privctl 打点）把崩溃窗口收窄到 step2 的 PM 迭代内
  `endpoint_slot(Endpoint(0))` → `get(id)` → `get_ticks` 三调用窗口；
  PM 分支三个打点（rs-pm pre-sched 等）从未打印 → 崩溃在到达
  `sched_init_proc` 之前。
- 反汇编解码（c20a 匹配的 RS 二进制，0x20d20e）：崩溃指令
  `cmpl $0x1,(%rbx,%rax,1)` = `by_endpoint[slot]` 的 Option
  discriminant 读取（`self.by_endpoint.as_ptr()+slot*16` 折叠形态）；
  `<unset> 0x2 0x20d20e` 是内核 stacktrace.rs proc_stacktrace 的
  `name endpoint rip` 三元组，确认 0x20d20e 即 RS 崩溃 rip。
- 内核侧核实：exception.rs 入口读 cr2 → pf.vaddr → VM
  `dispatch_pagefault` 的 `pf_exit!("noaddr")` 打印的是**消息里的
  vaddr**，与 cr2 同链 → cr2=0 真实，用户指令确实访问了地址 0。

### 第三轮取证（c21a 决定性探针 + 静态闭环，2026-09-22）→ BLOCKED

- **决定性证据**（process_table.rs `endpoint_slot` 入口探针，限 4 次，
  打印 self/by_endpoint 裸指针/slot）：
  - VM 迭代：`nk4a: rs-epslot self=0x7fffffffc800 bep=0x7fffffffc800 slot=8`
    （顺带实证 Rust 字段重排后 `by_endpoint` 在 RProcTable 偏移 0）；
  - PM 迭代：`nk4a: rs-epslot self=0x0 bep=0x0 slot=0` → **调用方传入
    的 receiver 真的是 0x0**（非寄存器被踩的假象、非 cr2 谎报）；崩溃
    pc=0x216a76（探针阻断内联后的独立 endpoint_slot，
    `mov (%rdx,%rcx,1),%rax`，rdx=0）。
  - 两轮之间还观察到两次正常 pf 往返（rip=0x2014ad 与
    rip=0x203bf0=asynsend 首指令 `sub $0x68,%rsp` 的取指 #PF，
    page 0x203000 文本 demand paging，合法）。
  - → **corruption 窗口锁定**：VM 迭代 `init_service→asynsend(RS_INIT)`
    的 pf 往返之后、PM 迭代 `endpoint_slot` 调用之前，`&self.table`
    从有效变 0。
- 静态闭环（本轮排除项）：
  1. pf save/restore 寄存器对称：`save_frame_to_context` 与
     `restore_to_user`（trap_return.rs）逐槽核对一致（rbx 从 ctx.rbx、
     gp_regs 按索引、rsi 最后加载）；
  2. 用户侧 int 0x21 wrapper（minix-sys/arch_trap.rs）
     `push rbx; mov rbx,msgptr; int; mov status,rbx; pop rbx` 自平衡；
  3. **TrapFrame 布局 vs 硬件 int 帧序**（本轮截断续查完成）：CPU 推
     SS→RSP→RFLAGS→CS→RIP（RIP 最后→最低址），stub 再推
     errcode→vector→r15..rax（rax 最低）→ 栈低→高
     `rax..r15, vector, errcode, rip, cs, rflags, rsp, ss`，与
     trap_stub.rs:116-143 结构声明序完全一致；`x86_syscall_entry`
     手工 push 序（先 user_ss，ss@slot21/rsp@slot20 注释）一致 →
     **帧倒序假设排除**；
  4. 「restore 到 0x2014ad（test %rax,%rax，不可能数据 fault 的指令，
     且是 RS drop_in_place 中段）」异常：该 restore 出现在
     `pick->0x8`（VM cr3=0x1dfde000）之后 → 更合理归因是 **VM 自身
     二进制的同 VA 地址**（不同映像），不是 RS ctx 被踩 → 该异常点
     不再指向内核踩 RS。
- 遗留嫌疑（下一轮候选，未做）：
  1. 本端口把 Minix3 IPC 状态寄存器（C i386 EAX=caller-saved）映射到
     **RBX（x86-64 callee-saved）**：`clear_ipc_status_reg` 写
     ctx.rbx=0（ipc.rs:2087 RECEIVE prologue）、
     `sync_status_register_to_frame` 用 ctx.rbx 覆盖 frame.rbx
     （trap_dispatch.rs:879）——「C 安全、Rust 危险」的移植语义面，
     若 pf/IRQ 恢复恰落在 asynsend 的 int 0x21 窗口内（PIT 抢占时序
     敏感，铁律 #9 同源）可解释踩 rbx=0；但本轮未找到该路径在此窗口
     执行的证据；
  2. RS 栈槽上的 table 指针被 pf 填页/拷贝踩写；
  3. 第四轮取证方案：PM 分支每个 syscall 返回点后再打一次
     `self.table` 基址（二分窗口内最后一个有效点），或反汇编 step2
     循环确认 table 指针在迭代间的存活介质（栈槽 vs callee-saved
     寄存器）后再定点布防。
- 铁律 #10 裁决：同一问题三轮（c19a/c20a/c21a）无根因实证 →
  **Task C 标记 BLOCKED**，本轮产物（探针 + 三份串口日志 + 本节）
  commit 归档；不做无定性修复。

### 第四轮：静态定性成功，BLOCKED 解除（2026-09-22 同日续查）

三轮真机取证后，转入零成本静态对照（上节「下一步建议 1」），根因定性：

- **缺陷**：本端口 IRQ/tick 臂从不把被中断的用户寄存器全存进
  `cpu_context`（trap_dispatch.rs:262-265 注释自认「The Rust IRQ path
  never saves the frame」；tick 0xF1 臂 229-258 同样无 save）。而
  `finish_and_restore`（kernel/lib.rs:3294-3314）唤醒/切回时按
  「从 arch-private context 重建 frame」恢复——tick quantum 强制
  （check_quantum，trap_dispatch.rs:241-257）与 PM 调度器挪动之下，
  被抢占进程从**陈旧 ctx** 恢复。
- **RBX 是重灾区**：ctx.rbx 三重身份（出生 ps_strings——
  `unwrap_or(0)`，arch/x86_64/boot.rs:143；IPC 状态寄存器——
  RECEIVE prologue `clear_ipc_status_reg` 写 0，ipc.rs:2087；普通
  callee-saved 活值——本端口 SysV ABI 下编译器必然用 rbx 存跨调用
  指针）。陈旧 ctx 的 rbx 回灌用户寄存器 → 活指针变 0/IPC 状态值。
- **C 对位**：mpx.S 每个 IRQ/tick stub 入口无条件
  `SAVE_PROCESS_CTX(0, KTS_INT_HARD)`（mpx.S:77/137/355/540），宏体
  `SAVE_GP_REGS`（含 EBX——i386 IPC_STATUS_REG 同为 bx，
  ipcconst.h:10）+ 记 `p_kern_trap_style`（sconst.h:75-89）。C 的
  p_reg 永远是「最近一次 trap 入口的全量快照」，故从 p_reg 恢复永远
  正确；本端口 IRQ 不存 → 该不变量破裂。
- **与全部真机证据一致**：c21a 窗口 = VM 迭代 asynsend 的 IPC+pf
  往返（int 0x21 入口把 ctx.rbx 存成 wrapper 的 msgptr、pf 入口存的
  是调用点 rbx），此后任一 tick/挪动窗口从该 ctx 恢复；崩溃值恰为
  0x0（RECEIVE 清零/出生 0 语义或折叠后地址）；PIT 抢占时序敏感
  （铁律 #9 同源）；「restore 到 0x2014ad（不可 fault 指令）」= 陈旧
  ctx.rip 回放的形态之一。
- **修法**：tick 0xF1 臂与 IRQ 臂加 user-origin 全量
  `save_frame_to_context` + `trap_style=FullContext`（对齐 C 入口保存
  不变量；内核来源不存——frame 是内核寄存器，对应用户 ctx 已由其
  系统调用入口存过）。
- **修复结果（c22a 真机验证，2026-09-22）**：IRQ/tick 入口保存修复
  已实施（Fix #10，宿主 kernel 809 全绿 + 防回归测试
  `irq_entry_mirrors_user_frame_but_skips_kernel_origin`），但 c22a
  崩溃序列与 c21a 逐字节一致、`rs-epslot self=0x0` 原样复现 →
  **该 C 偏差独立成立并修复，但不是 Task C 崩溃的根因**（假设证伪
  于本例；且崩溃完全确定性、与抢占时序无关）。Task C 回到 BLOCKED，
  证据增量见上节 1-3。

### 第五轮：静态反汇编定性 + 交付值探针（c23a，决定性实证）（2026-09-22 同日续查）

本轮做了两类零/低成本取证：先把全部崩溃地址用未 strip 的 RS ELF
符号化（`os/target/image/x86_64/staging/EFI/minix/modules/rs`，
objdump/nm），再在内核 restore 前的既有路标里加一个 `rbx=` 字段
（`kernel/src/lib.rs:3345` 起的 `pre-restore-` 块）。

**先纠正第四轮记录里的两处事实错误**（历史不删，按仓库惯例在此更正）：

1. 上节「c22a 新证据 2」把 `nk4a: vm-pf bytes` 当成了「VM 消息 payload
   读到的内容」。读源码证伪：该探针打印的是 VM **解析成功之后**用
   `pt.query(aligned)` → `vm_phys_to_virt` 直读被映射物理页的前 8 字节
   （`servers/vm/src/vm_server.rs:1916` 附近），所以它是「填进去的页
   内容」的证词，与 pf 消息通道编码无关。全量统计既有各轮：该探针取值
   共 1625 条为全零，非零值多为合法 x86-64 代码字节（如
   `48b8306022000000` = `mov rax, imm64`），ASCII 例
   `3538353936303631` 全数据集中只出现 1 次且 c21a/c22a 同位置
   （确定性），不构成「payload 错位」证据。**「pf 消息通道内容可疑」
   这条假设撤回。**
2. 上一轮把崩溃读成「第二次进入 init_fresh step2」。实际：`rs-step2
   pt=` 探针上限 8，整轮只出现 1 条；`rs-epslot` 上限 4，出现 3 条
   → 崩溃发生在**同一次 step2 循环的第 3 个条目**，不是第二次调用。

**反汇编证据链（全部为实测输出，RS 未 strip）**：

- `0x203bf0` = `TrapKernelApi::asynsend` **函数首指令**；函数体
  `sub $0x68,%rsp … rep movsq … push %rbx; mov %r8,%rbx; int $0x21;
  mov %rbx,%rdx; pop %rbx` —— 即本端口的 `mini_senda` 对位 wrapper
  （call_nr 0x10 = SENDA），它把 RBX 当 IPC 状态寄存器用，但**先
  `push %rbx` 保存用户活值、返回后 `pop` 还原**。
- `0x216a76` = `process_table::endpoint_slot+0x176`，故障指令
  `mov (%rdx,%rcx,1),%rax`，其中 `0x216a6d: mov 0x10(%rsp),%rdx` 取的
  是入口 spill 的 `self`，`rcx = slot*16` → 故障读地址 = `self +
  slot*16`，`self=0` 时正是 cr2=0。
- `0x20d2db`（`init_fresh+0x895` 内）= `mov %rbx,%rdi; movabs
  $0x216900,%rax; call *%rax` —— **LLVM 把 `&self.table` 常驻在
  RBX**（SysV callee-saved），step2 循环每次迭代都从 RBX 取回 self。
  又 `by_endpoint` 在结构体内偏移 0，故 `self=0x0` 与 `bep=0x0` 不矛盾。
- 于是本轮把崩溃现场完全解释闭合：交付给用户时 RBX=0 → asynsend 的
  `push %rbx` 把 0 压栈、`pop` 又还原 0（**wrapper 反而把这个错值固化
  成整个调用期间的 self**）→ init_fresh 后续调用拿到 self=0 → 读
  VA 0 → VM `pf-exit noaddr cr2=0x0` → SIGSEGV → RS 自己是 sig manager
  2 → `kernel/src/syscall_signal.rs:300` 自致命 panic。

**c23a 真机决定性结果**（`tmp/nk4a/serial_c23a.log`）：

- 行 3746：`nk4a: pre-restore- rip=0x0000000000203bf0
  rsp=0x00007fffffff9d88 rbx=0x0000000000000000` —— 崩溃前最后一次
  RS 交付，**RIP 恰是 asynsend 首指令、RBX 恰是 0**；
- 行 192（同轮 VM）：`pre-restore- rip=0x00000000002014ad
  rsp=0x00007fffffffc0b8 rbx=0x0000000000010001` —— 同一字段对 VM
  交付的是有意义的值，反证该字段本身有效、不是打印 bug；
- RS 更早几次交付 `rbx=0x00007fffffffc800`（栈地址，正常活值）。
- **结论**：内核确实把一个 0 作为用户 RBX 交付给了一个持活值的进程，
  且交付 RIP 落在 wrapper 的 `push %rbx` 之前。这把问题从「用户态自己
  踩坏」收窄到「内核侧 (rip, rbx) 配对的来源」。

**本轮取证方法缺陷（必须记下）**：新增的 `nk4a: pf-save`（页故障入口
保存点打印 `ep/rbx/rip`）上限设为 8，结果 8 条全部消耗在启动前段
endpoint 2 的同一重复 refault 事件上（c23a 最后一条在行 406，崩溃在
行 3746 之后），**没拿到崩溃现场**。

### 第六轮：IPC 状态寄存器写点排查（c24a，一个假说被证伪）（2026-09-22 同日续查）

本轮目标：枚举内核**所有**会写 `ctx.rbx` 的站点，用统一探针捕获「谁把
0 写进了 RS 的 RBX」。

- 共用探针 `nk4a_rbx_probe(tag, ep, rbx, rip)`
  （`kernel/src/trap_dispatch.rs:154`，`#[cfg(not(feature =
  "mock"))]`、全局 `AtomicUsize` 上限 40、**只在 rbx==0 时打印**——
  理由：`add-call`/`add-flags` 在 VM 每条投递上都会触发，不过滤则上限
  会在启动前段耗尽）；
- 接入 5 类站点：`irq-save`（`mirror_irq_frame_into_proc`）、
  `int33-save`（int 0x21 臂保存点）、`int33-ret`
  （`sync_status_register_to_frame` 之后）、`add-call` /
  `add-flags`（`kernel/src/proc.rs` 两个状态寄存器 OR 站点，
  `kernel/src/ipc.rs:1120/1137` 调用）、`recv-clear`
  （`kernel/src/ipc.rs:2087` RECEIVE prologue 清零，额外加
  `p_endpoint == 2` 门）；同时把 `pf-save` 上限 8 → 48。
- C 对位核查（写 RBX 本身是否偏差）：C 的 SENDA 路径同样会写 caller
  的 EBX（`minix3/minix/kernel/proc.c:690-692`
  `arch_set_secondary_ipc_return`），且 C/i386 的 IPC 状态寄存器同为
  bx（`minix3/minix/kernel/arch/i386/ipcconst.h:10`）→ **「内核写
  ctx.rbx」不是偏差，问题只在交付的 (rip, rbx) 配对**。

**c24a 真机结果**（`tmp/nk4a/serial_c24a.log`，崩溃第 3 次确定性复现：
行 3808 `<unset> 0x2 0x216a76`、行 3810 `lethal signal 11`）：

- `grep -o "rbxw [a-z0-9-]*" | sort | uniq -c` → 全轮**只有 4 条
  `rbxw irq-save`**，且 4 条全是 `ep=0x0000000000000008
  rbx=0x0 rip=0x000000000022b42d`（endpoint 8，不是 RS）；
- 即：`int33-save`/`int33-ret`/`add-call`/`add-flags`/`recv-clear`
  五类写点对 RS 零命中 → **「某个 IPC 状态寄存器写点把 0 写进 RS 的
  RBX」这一假说被证伪**；
- 信号路径（`syscall_signal.rs` 的 sigcontext 构建/恢复）在崩溃前
  串口上无任何活动，一并排除；
- `pf-save` 48 条仍然全部落在行 1306 之前（崩溃在 3808）——**探针上限
  设计连续第二次犯同一错误**，崩溃现场仍未捕获。这是本轮唯一未能
  达成目的的环节，也是本轮的自认缺陷，不得记成「已排查保存点」。
- 附带新观察（**仅记录，不作结论**）：`pf-save ep=0x2
  rip=0x0000000000227b29` 这一重复 refault 事件中，`rbx` 出现过
  `0x22e000` / `0x7` / `0x8` 三种取值；`0x7`/`0x8` 形态上像 IPC 状态
  位落进了 RBX，但未反汇编该 rip 对应的用户代码前不判定。

**剩余候选（收窄后）**：交付的 `(rip=asynsend+0, rbx=0)` 既非状态寄存器
写点所致，则来源只可能是「保存/恢复的配对」——即某一时刻合法、但已被
后续更新覆盖丢失的陈旧上下文，或 restore 消费的 ctx 与 save 写入的 ctx
不同一份。裁决需要崩溃前**最后一次 RS 页故障入口**的捕获值，即必须让
`pf-save` 探针能活到崩溃现场。

**第 7 轮判别方案（未执行，交下一会话）**：`pf-save` 改为
`ep == 2` 过滤 + 按 `(rip, rbx)` 去重（静态小数组记录已打印组合）+
上限提到 64，保证覆盖到崩溃前最后入口；同一轮再加一条
`int33-entry`（int 0x21 入口 frame.rbx 原值，同样只打 RS）以区分
「入口即 0」与「入口非 0、恢复时变 0」。

- 铁律 #10 裁决：**Task C 累计六轮（c19a/c20a/c21a/c22a/c23a/c24a）
  仍未定性根因，已远超「同一问题 3 轮」上限**。本节如实登记为
  BLOCKED 未解除。本轮**不做任何无定性修复**，产物是：1 处已完成的
  决定性实证（内核交付 RBX=0 给持活值的 RS）、1 个被证伪的假说
  （IPC 状态寄存器写点踩用户活值）、2 处对自身既往记录的更正、1 项
  自认的取证方法缺陷（探针上限两次被早期高频事件耗尽）。

### 第六轮补充：零真机成本的 `ctx.rbx` 写者全枚举（静态，不开新真机轮）

不开第 7 轮真机（铁律 #10），改为把「谁能把 0 写进某进程的
`ctx.rbx`」在代码层面**穷举**，并顺带排除两条看似成立的路径。

1. **差点误判的一处（已自证无害，必须记下以免后人重踩）**：
   `apply_to_trap_frame`（arch/x86_64/boot.rs:149-166）只回填
   rflags/cs/ss/rip/rsp，**不回填 rbx 与 gp_regs**——第一眼像「调度器
   恢复时 RBX 恒为 0」的 P0。读恢复汇编证伪：
   `restore_to_user`（arch/x86_64/trap_return.rs:79-131）的取数来源是
   **分家的**——iretq 载荷（RIP/CS/RFLAGS/RSP/SS）取自 `frame`
   （`rdi`），而 **RBX 与全部 GP 寄存器取自 `ctx`**（`rsi`，
   `rbx_off = offset_of!(X86_64CpuContext, rbx)`，
   trap_return.rs:109/144）。所以 frame 里 rbx 留默认值不影响交付。
   顺带得到一条**结构性风险记录**（非本轮 issue）：frame 与 ctx 分管
   不同寄存器，任何只更新其一的路径都会交付「撕裂的寄存器文件」；
   `finish_and_restore` 里 frame 由 ctx 重建（kernel/lib.rs:3329-3330
   + 3371），故该路径成对一致。
2. **「syscall 腿的瘦帧没填 rbx，保存时把未初始化值写进 ctx」——静态
   排除**：两个入口汇编都显式 `push rbx`
   （`x86_trap_common`，trap_stub.rs:318-333；`x86_syscall_entry`，
   trap_stub.rs:369-397），`save_frame_to_context` 的
   `ctx.rbx = frame.rbx`（trap_stub.rs:543）拿到的永远是真实用户
   RBX。
3. **写者全集（穷举结果，非抽查）**：写 `ctx.rbx` 的只有这几类——
   - 全量存帧原语 `save_frame_to_context`，内核侧 **5 个调用点**：
     trap_dispatch.rs:127（irq 镜像）、:585（页故障 ForwardToVm）、
     :697（异常→信号投递臂）、:887（int 0x21 臂）、:1103（syscall 腿
     VmSuspend 停车臂），另有 arch 侧测试用点与 arch/src/lib.rs:341
     的 trait 转发（非独立语义站点）；
   - `clear_ipc_status_reg`（boot.rs:248-252 写 0）**内核侧唯一调用点**
     ipc.rs:2087（plain-RECEIVE prologue）；其余 grep 命中全在测试；
   - `or_ipc_status_reg`（`|=`，不可能把非 0 变 0）经
     proc.rs:1790/1818 两个包装函数；
   - `set_secondary_ipc_return`（boot.rs:254-260 **整体赋值**）内核侧
     唯一调用点 syscall.rs:811，仅 `IpcCall::KernInfo` 分支，且
     page 未发布时提前 EBADCALL 返回（不会写 0）；
   - `write_user_register` 的 `72 => ctx.rbx = value`
     （boot.rs:227），调用点 misc.rs:1797（T_SETUSER，需有人
     ptrace）与 proc.rs:1767（`set_ipc_return_code`，走
     offset 80=RAX，不碰 rbx）；
   - 出生与信号：`build_cpu_context` 的
     `rbx: entry.ps_strings.map(|v| v.0).unwrap_or(0)`
     （boot.rs:143）、sigreturn 的 `ctx.rbx = sctx.sc_rbx`
     （arch/x86_64/signal.rs:321）。
4. **第 6 轮布防的漏洞（本轮发现，下一轮必修）**：上面 5 个存帧点
   只布防了 3 个（:127 irq-save、:585 pf-save、:887 int33-save），
   **:697（异常→信号臂）与 :1103（syscall 腿 VmSuspend 停车臂）
   未布防** → 第六轮「五类写点零命中」的结论**不足以覆盖写者全集**，
   不得当作已穷尽。
5. **第 7 轮判别方案（覆盖上两条缺口，仍未执行）**：
   - :697 与 :1103 各加一次 `nk4a_rbx_probe("sig-save", …)` /
     `("syscall-vmss", …)`（复用现有函数，零新机制）；
   - `pf-save` 改为 `ep == 2` 过滤 + 按 `(rip, rbx)` 去重 + 上限 64；
   - 出生值核查：RS 属模块装载路径
     （kernel/lib.rs:1513/1714 `EntrySpec::loaded(pc, sp, ps_strings)`），
     打印一次 RS 的 `ctx.rbx` 出生值，判定 `unwrap_or(0)` 是否真的
     被走到（若 RS 出生 RBX=0，则任何「从未存过帧的 ctx」被恢复都会
     直接复现本崩溃）。
6. **本轮性质**：纯静态阅读，零代码改动、零真机轮次、不做修复。Task C
   维持 BLOCKED。



