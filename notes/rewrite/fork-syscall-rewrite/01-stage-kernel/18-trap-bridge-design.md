# E1 trap 桥设计——用户态如何走进内核,以及如何回来

> 状态:**已批准**(2026-09-16,用户授权执行会话代评审;评审修正一处初稿 guess——SENDA 寄存器角色,见决策二修订)
> 日期:2026-09-16。作者:campaign 执行会话。
> 范围:01-stage-kernel 的 int-33 IPC trap 桥 + minix-sys 用户侧 trap 体(E1 的两半)。
> 前置裁决已定:二次 IPC 返回通道 = 保存上下文 RBX(`CpuContextArch::set_secondary_ipc_return`,commit 207e30644)。

## 一、问题是什么

minix-rs 内核至今从不返回用户态。调度循环(`scheduler_loop`,kernel/src/lib.rs:2828 起)已经能把一个进程的保存上下文恢复到 CPU 上跑,但用户代码一旦想发起 IPC,只有死路:IDT 33 号门虽然配好了 DPL=3 的 trap gate(arch/src/x86_64/trap_entry.rs:291-314,对应 C protect.c:147 的 `{ipc_entry_softint_orig, IPC_VECTOR_ORIG, USER_PRIVILEGE}`),但 handler 地址还是占位;minix-sys 侧的两个 transport(`DirectTrapTransport`/`DirectKernelCallTransport`)全部 `-EIO`。

所以缺的是一条完整回路:**用户寄存器 → 内核分派 → 阻塞或应答 → 回到用户(或切到别的进程)**。这条回路就是"trap 桥"。

## 二、约束穷举(Step 1)

设计不能发明事实。以下每条都带锚点。

**C 一侧(ground truth):**

- C1. i386 softint trap 体(usermapped_glo_ipc.S:25-40 `IPCFUNC`):`push %ebp; push %ebx; SETARGS; int $VEC; mov %ebx,%ecx; pop; pop; ret`——用户侧只保存 EBX(被内核改写为 IPC status),其余调用者寄存器任其破坏。
- C2. 寄存器约定:eax=端点,ebx=消息指针,ecx=IPC 调用号,`int $0x21`(= 33 号向量)。本树 libc 只有 arm+i386 变体,**没有 amd64 参照**——这一点 E1 条目原文已声明。
- C3. C 的 int 门返回后,errno 走 eax(p_reg.ax),IPC status 走 ebx(C: `IPC_STATUS_ADD` 就是 `p_reg.bx |= value`,ipc.h:42-48)。
- C4. C 有两条门:32 号 KERN_CALL_VECTOR(内核调用,不阻塞)与 33 号 IPC_VECTOR(IPC,可阻塞),外加 34/35 两个 usermapped 变体(interrupt.h:31-35;我们的 trap_entry.rs:229-230 注释已对账)。

**Rust 内核一侧(现状):**

- R1. IDT 33 号门已配置:`configure_ipc_entry` 设 DPL=3、trap gate、IST=0(trap_entry.rs:291-314),但 handler 地址未接(占位,E1 条目原文与 lib.rs `init_protection` 注释互证)。
- R2. SYSCALL(LSTAR)腿已经活了:`x86_syscall_dispatch_body`(kernel/src/trap_dispatch.rs:179-236)读 per-CPU `proc_ptr` 定位当前进程,取 **RDI = 用户消息指针**,调 `kernel_call`,把应答码写回 **RAX**。注释明说:"VMSUSPEND 经由 IPC trap 腿到达"——SYSCALL 腿不处理阻塞,遇 NoReply 直接 panic。
- R3. IPC 分派契约:`dispatch_ipc_entry`(kernel/src/syscall.rs:641 起)吃三个输入——`msg.m_type` = 调用号、`p_defer.r2` = src_dst 端点、`p_defer.r3` = SENDA 表指针(C: arch_system.c:492 把寄存器参数放进 p_defer 的对应物);BKL 由它内部获取,经 `transfer()` 移交,由 `kernel_call_finish` 或 `switch_to_user` 释放。
- R4. 阻塞的形状:`IpcOutcome::Blocked` → `KcallResult::NoReply`——被阻塞的调用者不该收到任何返回值,内核转进调度循环挑下一个进程。
- R5. 恢复机器:调度循环从每进程保存的 `X86_64CpuContext` 恢复用户态(trap_return.rs 的 `switch_to_user` asm:数据段选择子、RBX(注释原文"ps_strings / IPC status")、14 个 gp_regs、iretq 帧)。
- R6. 二次返回通道已裁决:`set_secondary_ipc_return` = 整值写保存上下文 RBX(arch/src/x86_64/boot.rs,commit 207e30644);与 IPC status 同寄存器族,各调用各的消费,无交叉危害。
- R7. 消息拷贝:SYSCALL 腿的 `kernel_call(caller, m_user: VirBytes, ...)` 由内核从用户内存拷消息——IPC 腿沿用同一拷贝语义即可,不需要新的拷贝机制。

**用户态消费面:**

- U1. minix-rt 已有接缝:`KerninfoSource` trait + `DirectTrapSource`(init.rs:108-149)——`query_kerninfo` 现返回 `Err(EIO)`,注释承诺"真实 trap 接线在通信机制模块落地时替换"。
- U2. `DirectTrapTransport`(ipc.rs)与 `DirectKernelCallTransport`(syscall.rs)全 `-EIO`,是 minix-sys 全部客户端 wrapper 的底座。

**stale-premise 修正**:E1 条目原文称"③ switch_to_user 仅存在于文档引用,无实现"——已过期。S-6/S-7 落地后 `switch_to_user`/`scheduler_loop` 是活代码(lib.rs:2828 起),SYSCALL 腿也已在跑。条目里真正还欠的只有:int-33 腿的 asm stub + Rust 分派体 + 用户侧 trap 体。

## 三、方案对比与选型(Step 2)

### 决策一:入口走哪条门

**方案 A——复用 SYSCALL(LSTAR)腿,给它补阻塞语义。** 优点:一条腿;LSTAR 比 int 快。缺点:C 明确分两门(约束 C4),32 号内核调用与 33 号 IPC 的语义差异恰恰在"可阻塞";SYSCALL 腿的返回路径是同进程 sysret,补阻塞等于把调度循环塞进 sysret 腿,改动面反而更大;且丢掉 C 的两门结构是 translate(模式 16)。

**方案 B——int 33 接 C 的寄存器约定,C 语义逐条对齐。** 门已配好(约束 R1);阻塞语义天然:分派返回 NoReply 后尾巴进调度循环,不回调用者;与 C 的 32/33 双门结构同构。

**选 B。** LSTAR 快路径留着给将来不阻塞的内核调用,E1 不动它。

### 决策二:int-33 腿的寄存器约定(x86-64 无 C 参照,这是本次设计的主裁决)

**方案 A——C i386 约定直接拓到 64 位:** RAX = src_dst 端点,RBX = 消息指针,RCX = IPC 调用号。优点:唯一有 C 源依据的 ABI(约束 C2),反 guess 纪律(E-RSWIRE 先例)支持;RBX 消息指针与 RBX status/二次返回同族(约束 R6,C3),libc 移植时 C 代码的寄存器心智模型直接复用。缺点:RBX 是 callee-saved,用户 stub 要 push/pop(但 C i386 本来就 push %ebx,C1);与 SYSCALL 腿的 RDI 约定不一致(两门两约定,C 本来也不一致)。

**方案 B——SysV 风格:** RDI = 消息指针,RSI = 端点,RDX = 调用号。优点:与 SYSCALL 腿一致,读代码顺。缺点:纯发明,C 无此物;违背 translate 防线——在有 C 参照的 ABI 上自创约定,就是 E-RSWIRE 条目警告过的"guess"。

**选 A。** RAX/RBX/RCX;errno 返回 RAX,IPC status 继续走保存上下文 RBX 的 OR 通道(约束 R6,R5——trap_return 的 RBX 恢复步就是用户侧回流)。**SENDA 的寄存器角色(评审修正)**:C `SENDA_ARGS`(usermapped_glo_ipc.S:87-90)是 `eax = count, ebx = table`——与普通 IPC 复用同两寄存器、仅换语义,不占用新寄存器;初稿在此处发明的 RDX/RSI 分配无 C 依据,系 guess,撤回。RECEIVE 的 status 出参按 C `GETSTATUS`(usermapped_glo_ipc.S:92-96)语义:wrapper 在用户侧把返回后的 EBX(status)写到调用者给的指针,内核不感知该指针。do_kernel_call(C 单参 `eax`,KERVEC 腿)本 rewrite 由既有 SYSCALL 腿承载,int-33 不设此变体。

### 决策三:保存区与返回模型——TrapFrame 直返,还是统一进 CpuContext?

现状有两套保存:TrapFrame(asm 在内核栈上 push 的完整寄存器文件,trap_stub.rs"Full register file")与每进程 `X86_64CpuContext`(调度恢复的真值,约束 R5)。

**方案甲——同进程 TrapFrame 直返:** int-33 分派完成后若同进程继续(应答了),asm 直接从 TrapFrame iretq 回用户;阻塞时才进调度循环。优点:非阻塞 IPC 返回快,同 SYSCALL 腿形状。缺点:一个进程的用户态真值存在两处(TrapFrame 与 CpuContext),阻塞时必须把 TrapFrame 的内容抄进 CpuContext 再进调度——抄写点就是将来的 bug 温床;C 在这里同样两处(C 把 frame 抄进 p_reg),但 C 只有一套恢复路径。

**方案乙——一律入 CpuContext,出口只有调度循环:** int-33 入口把寄存器存进当前进程的 CpuContext(与 RAX/RBX 返回值一起),分派完无论阻塞与否都尾进调度循环,由调度循环恢复"被选中"的进程。优点:用户态真值单点(CpuContext),恢复机器单一(约束 R5 的 asm 不需要第二个变体);阻塞成为平凡情形;IPC status 的 OR 目标本来就是 CpuContext.RBX,数据流天然合一。缺点:非阻塞 IPC 的返回多走一跳调度循环(可接受——本内核 BKL 大锁下本来就不是零开销路径;C 的恢复也走 `_restore_user_context` 统一出口)。

**选乙。** 用户态寄存器状态的单一真值优先;SYSCALL 腿维持现状不动,但它注定只服务"永不阻塞"的窄面,不是长期统一模型——这一点写进风险节。

### 决策四(小):用户侧 trap 体放哪

minix-sys 的 `DirectTrapTransport`/`DirectKernelCallTransport` 是所有 wrapper 的底座(约束 U2),trap 体(`global_asm!` 或 `asm!` 函数:`mov rax,rsi_args...; int 0x21`)落 minix-sys 新模块 `arch_trap.rs`(单架构 x86-64 先行,arm64/riscv64 保持 EIO 并注释声明);minix-rt 的 `DirectTrapSource::query_kerninfo` 改为调用它发 MINIX_KERNINFO(约束 U1),kerninfo 页映射建立后该桩自动翻活。门控:用户 trap 体在 hosted test 下不可执行——保持 transport trait 抽象,测试仍走 Canned。

## 四、落地切片(批准后按序执行)

1. **内核 asm stub + 分派体**:vector 33 的入口 stub(存完整寄存器到当前进程 CpuContext + p_defer 参数抽取)→ `x86_ipc_dispatch_body`(新,trap_dispatch.rs)→ `dispatch_ipc_entry`;接 `init_protection` 的占位 handler。测试:分派体单测(canned TrapFrame/CpuContext)。
2. **出口合一**:非阻塞结果写 RAX(errno)/RBX(status 已由投递路径 OR)→ `kernel_call_finish` → `scheduler_loop`;阻塞自然落入既有调度。测试:Blocked 后 NoReply + 不碰 RAX 的分派体断言(对照约束 R4)。
3. **用户侧 trap 体**:minix-sys `arch_trap.rs`——`send/sendrec/receive/notify/sendnb/senda` 六个 IPC 原语 + kernel-call 变体,寄存器约定按决策二;hosted 测试继续走 Canned,通电后才真跳。
4. **minix-rt 接线**:`DirectTrapSource::query_kerninfo` 换真 trap(MINIX_KERNINFO);kerninfo 页的用户映射初始化(写 `MINIX_KERNINFO_USER`,globals.rs)随本切片通电验证。
5. **通电**:QEMU 单核,init 服务 sendrec 回环;E8(SCHED 通电)的前置。

## 五、风险与开放问题

- **两腿并存的长期成本**:SYSCALL 腿(同进程 sysret)与 int-33 腿(统一调度出口)并存,恢复模型不同。若 E8 通电后维持,建议在 01-stage-kernel 记一条"是否收敛单腿"的 OQ,而不是现在静默双轨。
- **嵌套与内核态误触发**:33 号门 DPL=3,内核态 int 33 属 wiring bug——分派体应沿 SYSCALL 腿的先例 panic 而非静默(trap_stub.rs 的 S-3d 教训)。
- ~~SENDA 表指针~~(评审已解决):SENDA 复用 EAX(count)/EBX(table),无新约定;切片 1 单测钉住 p_defer.r3 = 表指针的抽取来源(C r3 语义,proc.c:683)。
- **性能**:决策三的方案乙让每次 IPC 多一跳调度循环。若通电实测成瓶颈,再评估"同进程快速出口"旁路,且必须保住 CpuContext 单一真值不变。
