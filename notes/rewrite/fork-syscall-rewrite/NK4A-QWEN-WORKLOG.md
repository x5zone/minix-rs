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

## Task C — RS null-deref 取证（BLOCKED，三轮取证上限）

- 状态：**BLOCKED**（铁律 #10：c19a/c20a/c21a 三轮取证已达上限。故障
  形态与 corruption 窗口已定性到「调用方在 PM 迭代把 &self.table=0x0
  传入 endpoint_slot」，但「表基址如何在 VM→PM 迭代之间变 0」的根因
  机制未实证，不假修）。已排除项与证据链见下文三轮记录。
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
  `evidence/20260922-nk4a-taskA-c17a-c18/`（Task A commit 84347cff2）。

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



