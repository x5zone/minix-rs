# 31-fpu-context-switching: FPU 上下文切换（lazy 模型 + #NM 陷阱路径）

> **源码**: `minix3/minix/kernel/arch/i386/arch_system.c`（fpu 函数族 :51-215）、`minix3/minix/kernel/proc.c`（copr_not_available_handler :1922-1959）、`minix3/minix/kernel/arch/i386/exception.c`（enable/disable_fpu_exception :375-383）、`minix3/minix/kernel/arch/i386/mpx.S`（copr_not_available :537-552）、`minix3/minix/kernel/system/do_sigsend.c`（save_fpu 路径 :86）、`minix3/minix/include/arch/i386/include/fpu.h`、`minix3/minix/kernel/cpulocals.h`
> **Rust 实现**: `os/arch/src/arch/fpu_arch.rs`（FpuArch trait）、`os/arch/src/{x86_64,arm64,riscv64}/fpu.rs`（三架构 State）、`os/kernel/src/smp.rs`（fpu_owner + SAVE_CTX FPU 保存）、`os/arch/src/arch/exception_dispatcher.rs`（#NM 分发）
> **前置**: [10-switch-to-user.md](10-switch-to-user.md)（上下文切换）、[14-exception-interrupt.md](14-exception-interrupt.md)（异常分发框架）、[16-smp.md](16-smp.md)（每 CPU 数据 + SAVE_CTX IPI）、[19-syscall-signal.md](19-syscall-signal.md)（信号传递）
> **下游**: 无（本主题为独立子系统；#NM 分发结果 FpuTrap 供异常交付路径接线时消费）
> **C 总行数**: ~215 行（arch_system.c fpu 族）+ ~37 行（copr_not_available_handler）+ ~16 行（汇编入口）+ ~14 行（exception.c 开关）+ ~8 行（do_sigsend 路径）

---

## Ch1: 概念

**核心问题**: 进程的计算状态不止通用寄存器——还包括一大片 FPU 寄存器组（x87 数据寄存器 + SSE 寄存器 + MXCSR/控制字）。多进程共享一个物理 FPU。内核在切换进程时必须管理这块状态：不保存则状态串扰，全保存则付出每次切换两次内存拷贝（FXSAVE/XRSTOR，各 512B~4KB）的代价。**如何让"保存/恢复"的成本与进程真实使用 FPU 的行为挂钩，而不是与调度次数挂钩？**

### 1.1 FPU 状态是什么

| 组成部分 | 内容 | 大小（x86-64） |
|---------|------|---------------|
| x87 数据寄存器 | ST0-ST7，80 位扩展精度 | 8×10B |
| SSE 寄存器 | XMM0-XMM15（AVX 则 YMM0-YMM15 512B） | 16×16B/32B |
| 控制状态 | x87 控制字/状态字/标记字 + MXCSR | 28B |
| **FXSAVE 区总量** | 上述全部（含保留区） | **512B**（FXSAVE 格式） |

RISC-V/ARM64 的 FPU 状态是另一套寄存器组（FPSIMD 528B / F/D 264B），但抽象相同：**一大片与整数通用寄存器平行的、与指令执行耦合的计算状态**。因此 Rust 侧用 `FpuArch::State` 关联类型统一表达（见 §4.1）。

### 1.2 两种管理策略：eager vs lazy

| 策略 | 切换时行为 | FPU 指令被拦下的场景 | 开销 |
|------|-----------|---------------------|------|
| **eager（全量）** | 每次切换：保存旧进程 + 恢复新进程 | 无——TS 位永远不置位 | 每次切换 2 次大内存拷贝（~1µs 级） |
| **lazy（延迟）** | 每次切换：只保存**真正用过 FPU 的进程**；新进程首次用 FPU 时现场恢复 | 新进程首次执行 FPU 指令时（#NM） | 切换时几乎为零；首次使用 FPU 时一次恢复 |

关键观察：**绝大多数进程（或一个进程的绝大多数时间段）根本不碰 FPU 寄存器**（整数运算、系统调用、I/O 等待都不涉及）。eager 策略为这些"不用 FPU 的时间"白白支付了拷贝成本。lazy 策略把成本从"每次调度"转移到"每次 FPU 所有权转移"——而所有权转移恰好发生在进程真正使用 FPU 的时刻，成本是必须付出的（它此时确实需要那块状态）。

lazy 的语义等价性：**每个进程看到的 FPU 状态，始终是它上次离开 CPU 时的状态**——区别只在"何时"恢复（离开时预恢复 vs 使用时当场恢复），不改变观察结果。

### 1.3 CR0.TS + #NM 机制：硬件如何检测"谁用了 FPU"

lazy 策略需要一个可靠的"进程首次使用 FPU"信号。x86 提供硬件机制：

1. **CR0.TS（Task Switched）位**：置位后，**任何** x87/MMX/SSE/AVX 指令都触发 **#NM（Device Not Available，向量 7）** 异常——而不是正常执行。
2. 内核在**调度离开时**置位 TS（对"用了 FPU 的旧进程"而言，它离开前已保存了状态，TS 置位无妨）；在 **#NM 处理中清除**（clts），并恢复目标进程的 FPU 状态——此后该进程的 FPU 指令正常执行，直到下一次被切换走再次置位。

因此 TS 位是"FPU 所有权"的硬件化身：**TS=1 表示"FPU 状态属于上一个进程，任何新进程想用它必须先证明自己"**。RISC-V/ARM64 没有 TS 位——它们的 FPU 所有权切换必须由内核主动保存/恢复（架构差异见 §3 设计决策）。

### 1.4 fpu_owner 归属协议

内核维护一个每 CPU 变量 `fpu_owner`：**当前 FPU 硬件状态属于哪个进程**。协议不变式：

```
fpu_owner == Some(p) ⟺ 硬件 FPU 寄存器组保存着 p 上次的计算状态
               （且 p 是唯一信任这份状态的进程）
fpu_owner == None  ⟹  硬件状态不可信，任何进程的 FPU 状态都在其 KProcess.fpu_state 中
```

#NM 处理流程（lazy 恢复主体）：

```
进程 B 触发 #NM（TS=1，说明 FPU 属于某人 else）：
  1. clts（TS 清除——恢复动作本身要执行 FPU 指令，先解锁）
  2. 若 fpu_owner == Some(A) 且 A ≠ B：把 A 的现场保存到 A.fpu_state
     （A 是本 CPU 的前任 owner——每 CPU fpu_owner 只指向本 CPU 的进程，见 §2.6）
  3. 把 B.fpu_state 恢复进硬件（失败 → 见 1.5）
  4. fpu_owner = B
  5. 直接回到 B 的用户态继续执行（不经调度器——B 本来就该在运行）
```

注意第 5 步的语义：**#NM 不是"换进程"事件，而是"当前进程补手续"事件**——B 之前被切换进来时因 lazy 策略没有恢复 FPU，现在补上。因此 C 的处理直接 `restore_user_context(p)` 跳回用户态，不走调度（proc.c:1953）。

### 1.5 恢复失败 → SIGFPE

恢复路径可能失败：硬件 FPU 不可用/损坏（早期 x86 无 FPU 或检测失败）。此时进程无法获得其 FPU 状态，继续运行会产生不可信的计算结果。C 的选择：**释放所有权（fpu_owner=NULL）+ 向进程发 SIGFPE**（proc.c:1946-1952）——进程得到明确信号，而非静默地读到垃圾值。这符合 fail-stop 原则：**状态恢复是二值的，不能部分成功**。

### 1.6 信号路径的 FPU 保存

信号帧（sigcontext）必须包含被中断计算的完整状态，否则 handler 返回后计算状态丢失。因此信号传递时若目标进程"用过 FPU"（proc_used_fpu），必须把硬件现场保存进 sigcontext（do_sigsend.c:86-87：`save_fpu(rp)` + `memcpy(&fr.sf_sc.sc_fpu_state, rp->p_seg.fpu_state, FPU_XFP_SIZE)`）。

这是 lazy 模型的例外点：**#NM 路径"延迟恢复"，信号路径"立即保存"**——因为信号帧是一次性快照，必须完整。

---

## Ch2: C 源码全析

### 2.1 初始化与检测：fpu_init（arch_system.c:51-86）

```c
void fpu_init(void)
{
	unsigned short cw, sw;

	fninit();
	sw = fnstsw();
	fnstcw(&cw);

	if((sw & 0xff) == 0 &&
	   (cw & 0x103f) == 0x3f) {
		/* We have some sort of FPU, but don't check exact model.
		 * Set CR0_NE and CR0_MP to handle fpu exceptions
		 * in native mode. */
		write_cr0(read_cr0() | CR0_MP_NE);
		get_cpulocal_var(fpu_presence) = 1;
		if(_cpufeature(_CPUF_I386_FXSR)) {
			u32_t cr4 = read_cr4() | CR4_OSFXSR; /* Enable FXSR. */

			/* OSXMMEXCPT if supported
			 * FXSR feature can be available without SSE
			 */
			if(_cpufeature(_CPUF_I386_SSE))
				cr4 |= CR4_OSXMMEXCPT;

			write_cr4(cr4);
			osfxsr_feature = 1;
		} else {
			osfxsr_feature = 0;
		}
	} else {
		/* No FPU presents. */
		get_cpulocal_var(fpu_presence) = 0;
		osfxsr_feature = 0;
		return;
	}
}
```

要点：
- **fninit + fnstsw + fnstcw 检测**：执行 `fninit` 后读状态字 `sw` 与控制字 `cw`；`(sw & 0xff) == 0` 且 `(cw & 0x103f) == 0x3f` 判定 FPU 存在（无 FPU 时状态字全 1）。检测结果写入每 CPU 变量 `fpu_presence`（cpulocals.h:72）。
- **CR0.MP|NE**：MP 位让任务切换自动置 TS；NE 位让 x87 算术异常走内部异常（而非外部 IRQ13）。一次写入，无条件设置（有 FPU 时）。
- **CR4.OSFXSR/OSXMMEXCPT 由 CPUID 决定**：`_CPUF_I386_FXSR` → CR4_OSFXSR（FXSAVE/FXRSTOR 可用）；`_CPUF_I386_SSE` → 额外 CR4_OSXMMEXCPT（SSE 异常使能）。未设置则 SSE 指令触发 #UD。
- `osfxsr_feature` 由 CPUID 结果决定（FXSR 有 → 1，无 → 0），后续保存/恢复据此选 FXSAVE/FXRSTOR 还是 FNSAVE/FRSTOR 格式。

### 2.2 状态存储布局：arch_proc_reset（arch_system.c:144-175）

```c
static char fpu_state[NR_PROCS][FPU_XFP_SIZE] __aligned(FPUALIGN);

void arch_proc_reset(struct proc *pr)
{
	char *v = NULL;
	struct stackframe_s reg;

	assert(pr->p_nr < NR_PROCS);

	if(pr->p_nr >= 0) {
		v = fpu_state[pr->p_nr];
		/* verify alignment */
		assert(!((vir_bytes)v % FPUALIGN));
		/* initialize state */
		memset(v, 0, FPU_XFP_SIZE);
	}

	/* Clear process state. */
	memset(&reg, 0, sizeof(pr->p_reg));
	if(iskerneln(pr->p_nr))
		reg.psw = INIT_TASK_PSW;
	else
		reg.psw = INIT_PSW;

	pr->p_seg.fpu_state = v;

	/* Initialize the fundamentals that are (initially) the same for all
	 * processes - the segment selectors it gets to use.
	 */
	pr->p_reg.cs = USER_CS_SELECTOR;
	pr->p_reg.gs =
	pr->p_reg.fs =
	pr->p_reg.ss =
	pr->p_reg.es =
	pr->p_reg.ds = USER_DS_SELECTOR;

	/* set full context and make sure it gets restored */
	arch_proc_setcontext(pr, &reg, 0, KTS_FULLCONTEXT);
}
```

要点：
- `fpu_state` 是**内核静态数组**（`__aligned(FPUALIGN)`，16 字节对齐——FXSAVE 要求）——不是每进程分配。每个进程的 FPU 保存区在 `p_seg.fpu_state`，数组索引即进程号（`p_nr`），零动态分配。
- 仅 `p_nr >= 0`（用户进程）分配保存区并 `memset` 清零；内核进程 `v = NULL`（内核不用 lazy 协议，见 §2.6）。
- `arch_proc_reset` 的职责不止 FPU——它同时是进程上下文初始化（psw + 段选择子 + `arch_proc_setcontext` 全上下文注册）。FPU 只是其中一段。
- ARM64/RISC-V 侧无对应代码（lazy 模型是 x86 特有机制，见 §3 决策）。

### 2.3 保存：save_local_fpu / save_fpu（arch_system.c:88-139）

```c
void save_local_fpu(struct proc *pr, int retain)
{
	char *state = pr->p_seg.fpu_state;

	/* Save process FPU context. If the 'retain' flag is set, keep the FPU
	 * state as is. If the flag is not set, the state is undefined upon
	 * return, and the caller is responsible for reloading a proper state.
	 */

	if(!is_fpu())
		return;

	assert(state);

	if(osfxsr_feature) {
		fxsave(state);
	} else {
		fnsave(state);
		if (retain)
			(void) frstor(state);
	}
}

void save_fpu(struct proc *pr)
{
#ifdef CONFIG_SMP
	if (cpuid != pr->p_cpu) {
		int stopped;

		/* remember if the process was already stopped */
		stopped = RTS_ISSET(pr, RTS_PROC_STOP);

		/* stop the remote process and force its context to be saved */
		smp_schedule_stop_proc_save_ctx(pr);

		/*
		 * If the process wasn't stopped let the process run again. The
		 * process is kept block by the fact that the kernel cannot run
		 * on its cpu
		 */
		if (!stopped)
			RTS_UNSET(pr, RTS_PROC_STOP);

		return;
	}
#endif

	if (get_cpulocal_var(fpu_owner) == pr) {
		disable_fpu_exception();
		save_local_fpu(pr, TRUE /*retain*/);
	}
}
```

要点：
- **save_local_fpu 无条件保存**：只要 `is_fpu()` 且保存区有效（assert），就执行保存——**没有 MF_FPU_INITIALIZED 决策树**（是否"用过 FPU"由恢复侧判断，见 §2.5）。osfxsr 时 `fxsave`（非破坏性，硬件状态保留）；非 osfxsr 时 `fnsave`（保存并**清空**硬件状态），`retain=TRUE` 时立即 `frstor` 把状态放回硬件（保持所有权）。
- **retain 语义**：`retain=1`（进程将保持运行/恢复）→ 保存后状态仍在硬件中（fxsave 天生保留 / fnsave 后 frstor 放回）；`retain=0`（进程将阻塞/让出）→ 保存后状态只存在于保存区，硬件归下一个用。
- **save_fpu 的 guard**：`get_cpulocal_var(fpu_owner) == pr`——**本 CPU 的 fpu_owner 等于 pr 才保存**（非 owner 无现场可存，防止把别人的状态存进自己的区）。保存后**不设 fpu_owner = NULL**——fpu_owner 保持指向 pr（lazy 模型下状态仍归 pr，直到另一个进程 #NM 接管）。
- **save_fpu 不清 TS**：`disable_fpu_exception()`（clts）在保存**前**执行，因为 fxsave/fnsave 本身是 FPU 指令（TS=1 会触发 #NM 死循环）；保存完成后 TS 保持清除——被保存进程恢复运行时直接可用 FPU。
- `fpu_owner = NULL` 只发生在两处：**#NM 处理中失败路径**（§2.6）与 **release_fpu**（§2.7）——正常保存路径不释放所有权。

### 2.4 SMP 感知：save_fpu 的远程路径（arch_system.c:113-133）

```c
#ifdef CONFIG_SMP
	if (cpuid != pr->p_cpu) {
		int stopped;

		/* remember if the process was already stopped */
		stopped = RTS_ISSET(pr, RTS_PROC_STOP);

		/* stop the remote process and force its context to be saved */
		smp_schedule_stop_proc_save_ctx(pr);

		/*
		 * If the process wasn't stopped let the process run again. The
		 * process is kept block by the fact that the kernel cannot run
		 * on its cpu
		 */
		if (!stopped)
			RTS_UNSET(pr, RTS_PROC_STOP);

		return;
	}
#endif
```

要点：**pr 可能正运行在另一个 CPU 上**（SMP）——本地 CPU 无法读另一 CPU 的寄存器组，`save_fpu` 对远程进程**不做本地保存**，而是发 `smp_schedule_stop_proc_save_ctx(pr)` IPI：让远程 CPU 停住该进程并**在 SAVE_CTX 中断路径中保存其上下文**（含 FPU——正是 Rust `smp.rs:498-525` SAVE_CTX FPU 保存路径的 C 对应，doc 16 §4 已覆盖）。`stopped` 记忆 + `RTS_PROC_STOP` 恢复：若进程原本在运行则让其继续（靠"内核不能在它所在 CPU 运行"维持阻塞语义）。

**注意**：`save_fpu` 的远程分支不检查 fpu_owner——它无条件触发上下文保存（调用方保证只有 owner 才需要）；本地分支才有 `fpu_owner == pr` guard（§2.3）。**每 CPU 一个 fpu_owner**（cpulocals.h:73），所以 #NM 处理中保存前任 owner 永远走本地路径（§2.6 详述）。

### 2.5 恢复：restore_fpu（arch_system.c:189-203）

```c
int restore_fpu(struct proc *pr)
{
	int failed;
	char *state = pr->p_seg.fpu_state;

	assert(state);

	if(!proc_used_fpu(pr)) {
		fninit();
		pr->p_misc_flags |= MF_FPU_INITIALIZED;
	} else {
		if(osfxsr_feature) {
			failed = fxrstor(state);
		} else {
			failed = frstor(state);
		}

		if (failed) return EINVAL;
	}

	return OK;
}
```

要点：
- **返回 int（OK/EINVAL），不是 void**——失败语义通过返回值向上传递（调用方 #NM 处理据此发 SIGFPE，见 §2.6）。
- **`proc_used_fpu(pr)` 判定**：`MF_FPU_INITIALIZED` 是"该进程是否曾经拥有过 FPU 状态"的标记（proc.h:175 宏，等价 `p_misc_flags & MF_FPU_INITIALIZED`）。**首次使用**（未置位）→ `fninit()` 重置硬件为干净状态 + 置位标记——不恢复垃圾；之后才真正 `fxrstor`/`frstor`。
- **fxrstor/frstor 返回值是"失败检测"**：fxrstor/frstor 是汇编函数（arch_proto.h:112-114，返回 int）。失败通过 **#GP 陷阱重定向**检测：exception.c:222-229 的内核异常处理检查 `eip` 是否落在 `fxrstor..__fxrstor_end` / `frstor..__frstor_end` 指令区间——命中则把 eip 重定向到 `__frstor_failure`（失败返回路径），函数返回非零 → `if (failed) return EINVAL`。这样"恢复 FPU 失败"作为一个错误传递给 #NM 处理，而不是 panic（正常 #GP 会 panic）。

### 2.6 #NM 陷阱：copr_not_available_handler（proc.c:1922-1959）

```c
void copr_not_available_handler(void)
{
	struct proc * p;
	struct proc ** local_fpu_owner;

	/*
	 * Disable the FPU exception (both for the kernel and for the process
	 * once it's scheduled), and initialize or restore the FPU state.
	 */

	disable_fpu_exception();

	p = get_cpulocal_var(proc_ptr);

	/* if FPU is not owned by anyone, do not store anything */
	local_fpu_owner = get_cpulocal_var_ptr(fpu_owner);
	if (*local_fpu_owner != NULL) {
		assert(*local_fpu_owner != p);
		save_local_fpu(*local_fpu_owner, FALSE /*retain*/);
	}

	/*
	 * restore the current process' state and let it run again, do not
	 * schedule!
	 */
	if (restore_fpu(p) != OK) {
		/* Restoring FPU state failed. This is always the process's own
		 * fault. Send a signal, and schedule another process instead.
		 */
		*local_fpu_owner = NULL;		/* release FPU */
		cause_sig(proc_nr(p), SIGFPE);
		return;
	}

	*local_fpu_owner = p;
	context_stop(proc_addr(KERNEL));
	restore_user_context(p);
	NOT_REACHABLE;
}
```

汇编入口（mpx.S:537-552）：

```asm
LABEL(copr_not_available)
	TEST_INT_IN_KERNEL(4, copr_not_available_in_kernel)
	cld			/* set direction flag to a known value */
	SAVE_PROCESS_CTX(0, KTS_INT_HARD)
	/* stop user process cycles */
	push	%ebp
	mov	$0, %ebp
	call	_C_LABEL(context_stop)
	call	_C_LABEL(copr_not_available_handler)
	/* reached upon failure only */
	jmp	_C_LABEL(switch_to_user)

copr_not_available_in_kernel:
	pushl	$0
	pushl	$COPROC_NOT_VECTOR
	jmp	exception_entry_nested
```

要点：
- **开头 disable_fpu_exception()（clts）**：恢复动作（XRSTOR）本身是 FPU 指令，TS=1 会再次触发 #NM（死循环）。注释明说："Disable the FPU exception (both for the kernel and for the process once it's scheduled)"——clts 后内核与本进程后续 FPU 指令都不再被拦。
- **保存前任 owner 走本地路径**：`local_fpu_owner` 是本 CPU 的 fpu_owner（cpulocals 每 CPU 副本）——**owner 必然在本 CPU 上**（每 CPU 变量只指向本 CPU 运行/阻塞的进程），因此直接 `save_local_fpu(*local_fpu_owner, FALSE)` 保存现场，**无需 SMP 感知的 save_fpu**（§2.4 的远程分支在此用不上）。
- **`assert(*local_fpu_owner != p)`：同进程重入 = 编程错误（panic）**——C 认为 #NM 时 owner 恰好是当前进程是不可能的（TS 置位时本进程必然已切走）。不存在"只需 clts"的优雅分支。
- **失败路径**：restore_fpu(p) 返回非 OK → `*local_fpu_owner = NULL`（释放所有权）+ `cause_sig(SIGFPE)` + return——**走正常调度路径送信号**，不是 #NM 直跳路径（§2.5 的 EINVAL 传递链）。
- **成功路径**：`*local_fpu_owner = p`（所有权移交）+ `context_stop(KERNEL)` + `restore_user_context(p)`（直接回用户态，不经调度）+ `NOT_REACHABLE`。
- **汇编层**：`TEST_INT_IN_KERNEL` 分流——user 模式走 `SAVE_PROCESS_CTX(0, KTS_INT_HARD)` + `context_stop` + `call copr_not_available_handler`；handler **返回仅在失败路径**（SIGFPE 已送）→ `jmp switch_to_user`（正常切换）。kernel 模式 → `exception_entry_nested`（嵌套异常路径，不可恢复则 panic）。
- **kernel 模式 #NM 是编程错误**：内核代码使用 FPU 必须先主动恢复（且内核进程不用 lazy 协议）。

### 2.7 开关与释放：enable/disable_fpu_exception（exception.c:375-383）+ release_fpu（proc.c:1961-1968）

```c
void enable_fpu_exception(void)
{
	u32_t cr0 = read_cr0();
	if(!(cr0 & I386_CR0_TS))
		write_cr0(cr0 | I386_CR0_TS);
}

void disable_fpu_exception(void)
{
	clts();
}

void release_fpu(struct proc * p) {
	struct proc ** fpu_owner_ptr;

	fpu_owner_ptr = get_cpu_var_ptr(p->p_cpu, fpu_owner);

	if (*fpu_owner_ptr == p)
		*fpu_owner_ptr = NULL;
}
```

要点：
- `enable_fpu_exception` = **stts 封装**（仅当 CR0.TS 未置位才写——避免多余 CR0 写回）；`disable_fpu_exception` = **clts() 封装**（exception.c:382，直接清 TS 位）。
- `release_fpu` **跨 CPU 定位 owner**：`get_cpu_var_ptr(p->p_cpu, fpu_owner)`——p 可能阻塞/死在另一 CPU 上，必须在**它所在 CPU** 的 fpu_owner 上清除，而不是本 CPU 的（这正是 `get_cpu_var_ptr`（跨 CPU 索引）与 `get_cpulocal_var`（本 CPU）的区别）。
- `release_fpu` **不碰 TS 位**：它只清 owner 指针（`*fpu_owner_ptr == p` 才清——非 owner 不动，避免误清别人）。清 owner 后该 CPU 下次 #NM 时 `*local_fpu_owner == NULL`，直接跳过保存步骤（§2.6）。
- `release_fpu` 在**进程死亡**时调用（proc.c:1961-1968，进程退出清理路径）——释放所有权，避免悬挂指针指向已回收的 proc 表项。

### 2.8 信号路径：do_sigsend（do_sigsend.c:19 + :84-88）

```c
int do_sigsend(struct proc * caller, message * m_ptr)
{
	...
	/* sigframe 填充（sf_fp/sf_signum/sf_scpcopy/sf_ra 等） */
	if (fr.sf_sc.trap_style == KTS_NONE) {
		printf("do_sigsend: sigsend an unsaved process\n");
		return EINVAL;
	}

	if (proc_used_fpu(rp)) {
		/* save the FPU context before saving it to the sig context */
		save_fpu(rp);
		memcpy(&fr.sf_sc.sc_fpu_state, rp->p_seg.fpu_state, FPU_XFP_SIZE);
	}
	...
}
```

要点：FPU 保存块位于 `do_sigsend` 主函数中（do_sigsend.c:84-88），在 KTS_NONE 检查（信号帧必须包含 trap style，否则 EINVAL）之后。`proc_used_fpu(rp)` 检查 MF_FPU_INITIALIZED（与 restore_fpu 相同的"曾经用过"标记）；`save_fpu(rp)` 立即保存（信号帧需要一次性快照，见 §2.3 的 SMP 分支——目标进程可能在其他 CPU 上）。信号帧的 FPU 布局由 `fpu_sigcontext`（arch_system.c:614）定义——sigcontext 是架构相关的 ABI 结构（osfxsr 时从 xfp_regs 读错误状态，否则 fpu_regs）。

### 2.9 每 CPU 数据与查询：cpulocals.h + is_fpu（main.c:518）

```c
char fpu_presence; /* whether the cpu has FPU or not */
struct proc * fpu_owner; /* who owns the FPU of the local cpu */
```

```c
int is_fpu(void)
```

要点：fpu_presence（**char 类型**——布尔存在性标记）与 fpu_owner 都在 cpulocals 段（`__cpu_local_vars` 结构体，每 CPU 副本，见 doc 16）。`is_fpu()`（main.c:518）是查询入口：返回本 CPU 的 fpu_presence，save_local_fpu 用它做无 FPU 早退检查（§2.3）。

---

## Ch3: Rust 设计决策

| 决策 | 设计 | 备选与理由 |
|------|------|-----------|
| **D1 状态抽象** | `FpuArch` trait + `State` 关联类型（fpu_arch.rs） | 硬件抽象约束（项目约定）：OS 层不接触 CR0/CR4/PTE 位；三架构 State 布局不同（x86-64 FXSAVE 512B / ARM64 FPSIMD 528B / RISC-V F/D 264B），关联类型让恢复/保存按 State 泛型化 |
| **D2 实例化** | stateless ZST `CurrentFpuArch::default()`（泛型静态分发） | trait 对象动态分发对热路径（#NM 恢复）无必要；ZST + 泛型零开销，与现有 `ExceptionArch` 模式一致 |
| **D3 owner 表示** | `Option<ProcNr>`（每 CPU，smp.rs:164）替代 C 的 `struct proc *fpu_owner` | Option 显式编码"无 owner"状态，杜绝 NULL 解引用语义；ProcNr 是 proc 表的稳定索引（进程死亡后表项复用由 proc 层保证，release 路径同步清理） |
| **D4 #NM 分发** | dispatcher 特判 `vector 7 && is_user → ExceptionOutcome::FpuTrap`（新增，本轮实现） | C 在汇编层拦截用户 #NM（不进 exception_handler）；Rust 无汇编层分发 → 在 dispatcher 特判，对齐既有 vector 2/14 模式。kernel #NM 走 handle_nested（panic），匹配 C 的 exception_entry_nested |
| **D5 lazy-restore 主体** | **不实现**——FpuTrap 枚举 + 特判先行；主体待异常交付路径接线时落地 | restore_user_context 直跳语义未接线（doc 10 已知限制）；现在实现会产出 dead code + 假测试。显式缺口 > 假实现（fail-stop） |
| **D6 信号路径 FPU 保存** | **不实现**——31 doc 显式标注为已知缺口 | sigcontext 布局（fpu_sigcontext, arch_system.c:614）与 libs 层信号帧类型绑定，改动面跨 crate；列入 Task 3 候选 |

**跨架构差异说明**（Rust 三架构 vs C 单架构）：

| 维度 | x86-64（C 对应） | ARM64 / RISC-V |
|------|-----------------|----------------|
| #NM 陷阱 | ✅ CR0.TS 硬件机制 | ❌ 无 TS 位——所有权切换必须主动保存/恢复（FpuArch::save/restore 直接调用，无陷阱路径） |
| State 格式 | FXSAVE 512B（osfxsr） | FPSIMD 528B / F+D 264B |
| fpu_owner 每 CPU | ✅ | ✅（同一协议：save 时清 owner） |

因此 FpuTrap 分发分支在 ARM64/RISC-V 上永不触发（其异常源不产生向量 7）——不需要 cfg 门控，dispatcher 是架构无关的，行为由 ExceptionArch 实现自然决定。

---

## Ch4: Rust 实现

### 4.1 FpuArch trait（os/arch/src/arch/fpu_arch.rs）

```rust
pub trait FpuArch: Sized + Send + Sync + Default {
    type State: Default + Copy + Send + Sync + core::fmt::Debug;
    fn init(&self);                       // C: fpu_init — 检测 + CR0/CR4 配置
    fn save(&self, dst: &mut Self::State);  // C: fxsave/fnsave 指令 — 保存现场到 dst（arch_system.c:104-106）
    fn restore(&self, src: &Self::State);   // C: fxrstor/frstor 汇编函数 — 从 src 恢复现场（arch_system.c:201-203）
    fn enable(&self);                     // x86: clts；ARM64/RISC-V: NOP
    fn disable(&self);                    // x86: stts（置 TS → 下次 FPU 指令 #NM）
    fn disable_exception(&self);          // x86: clts + 清 CR4.OSXMMEXCPT；其余: NOP
}
```

实际签名含 `Sized + Send + Sync + Default` supertrait 与 `&self` 接收者
（fpu_arch.rs:59-116）——实例仅用于调用，FPU 控制寄存器是每 CPU 全局状态，
`CurrentFpuArch::default()` 是零开销 ZST（与 `CurrentClockArch` 同模式）。

设计对照（C → Rust）：

| C 符号 | Rust 对应 | 语义保持 |
|--------|----------|---------|
| fpu_init | `FpuArch::init()` | fninit 检测 → State 类型为编译期常量，无需运行时检测分支 |
| save_local_fpu(retain) | save/disable 组合（调用方表达 retain） | retain 是策略不是架构能力——Rust 侧由 smp 层组合 |
| save_fpu (SMP) | smp.rs:498-525 SAVE_CTX 路径 | IPI 远程保存语义已由 16 doc 覆盖 |
| restore_fpu | `FpuArch::restore()` | MF_FPU_INITIALIZED 判定 → Rust 侧在调用方（proc 层）用 State 生命周期/首次标记表达 |
| enable/disable_fpu_exception | `enable()`/`disable()` | 直接语义映射 |
| release_fpu | smp.rs fpu_owner=None 清理 | Option 赋值即释放 |

### 4.2 三架构 State（os/arch/src/{x86_64,arm64,riscv64}/fpu.rs）

- **x86-64**: `X86_64FpuState { data: [u8; FXSAVE_SIZE] }`（FXSAVE_SIZE = 512，align 16）——FXSAVE 格式，`osfxsr_feature=1` 的 64-bit 常态。
- **ARM64**: FPSIMD 寄存器组（Q0-Q31 + FPCR/FPSR），528B 布局。
- **RISC-V**: F/D 扩展寄存器（f0-f31 + fcsr），264B 布局。

三者都实现 `FpuArch::State`：内核的 `KProcess.fpu_state`（os/kernel/src/proc.rs:975）是关联类型实例——**同一份 KProcess 代码在三架构编译出不同大小的保存区**，无需 cfg 分支。

### 4.3 smp.rs 集成（os/kernel/src/smp.rs）

```rust
pub struct CpuLocalData {
    ...
    pub fpu_owner: Option<ProcNr>,   // :164 — 每 CPU FPU 所有权（C: cpulocals fpu_owner）
}
```

SAVE_CTX IPI 的 FPU 保存路径（smp.rs:498-525）：远程 CPU 收到 SAVE_CTX 后执行
`disable_exception()` + `save(&mut proc.fpu_state)` + `fpu_owner = None`——即 C 的
`save_fpu` 远程分支（arch_system.c:121-124）的 Rust 对应。

### 4.4 #NM 分发集成（exception_dispatcher.rs，本轮新增）

`ExceptionOutcome` 新增 `FpuTrap` 变体；`handle()` 在 vector 14 特判之后、用户态 classify 之前拦截：

```rust
pub enum ExceptionOutcome {
    SpuriousNmi,
    FpuTrap,                       // 用户态 #NM：lazy-FPU 恢复路径（C: proc.c:1922）
    Signal(ExceptionSignal),
    ...
}

// handle() 内，vector 14 检查之后：
if vector.get() == 7 && is_user {
    return ExceptionOutcome::FpuTrap;
}
```

**为什么放在 classify 之前**：C 中用户态 #NM 在汇编层就被拦截（mpx.S），
根本不进 `exception_handler` 的 ex_data[] 信号分类。Rust 在 dispatcher 特判等价地
"拦截于分类之前"。`classify_signal(7) → Fpe` 保留为兜底映射（对齐 C ex_data[]
静态表），但用户态实际命中 FpuTrap 分支。

**kernel 模式 #NM**：`is_nested` 检查在特判之前 → 进 `handle_nested` → 无恢复点
→ `KernelPanic`（C: exception_entry_nested 同路径）。nested 的
`FaultContext::FpuRestore` 情形（恢复执行中再次故障）已有独立恢复点
`RecoveryPoint::FpuRestoreFailure`（exception_dispatcher.rs:165-169，测试
nested_fpu_restore 覆盖）。

**FpuTrap 的消费者（待接线）**：交付路径接线后，FpuTrap 的执行体为 C
copr_not_available_handler 的 lazy 恢复主体（§2.6 的 1-4 步）+ 直跳用户态（第 5 步，
依赖 restore_user_context 异常路径，doc 10/14 已知限制）。在此之前 FpuTrap
作为显式枚举值交付，无 dead code。

### 4.5 已知缺口标注

| 缺口 | C 对应 | 状态 |
|------|--------|------|
| lazy-restore 主体（FpuTrap 消费） | proc.c:1922-1958 | 待异常交付路径接线（D5） |
| 信号路径 FPU 保存 | do_sigsend.c:86 + fpu_sigcontext | 待实现（D6，Task 3 候选） |
| trap_entry vector 7 handler=0 | x86_64/trap_entry.rs:197 | IDT 门已登记（handler=0 占位），exception delivery wiring 的一部分（14 doc 已知限制） |

---

## Ch5: 测试

### 5.1 单元测试（exception_dispatcher.rs，本轮新增 2 个）

| 测试 | 验证 | 命令 |
|------|------|------|
| `user_nm_fpu_trap` | user 模式 vector 7 → `FpuTrap` | `cargo test -p minix-arch --lib user_nm_fpu_trap` |
| `nested_nm_panics` | nested vector 7 + 无恢复点 → `KernelPanic` | `cargo test -p minix-arch --lib nested_nm_panics` |
| `nested_fpu_restore`（既有） | nested vector 7 + FpuRestore 上下文 → `FpuRestoreFailure` 恢复点 | 同上 |

### 5.2 回归（既有，验证特判未破坏）

`classify_all_vectors`（vector 7 → Fpe 兜底不变）、`spurious_nmi`（vector 2）、
`user_page_fault`/`vm_page_fault`（vector 14）、`kernel_panic`（vector 13）、
`nested_*` 系列——全部 14 个 dispatcher 测试通过。

### 5.3 FpuArch 三架构（既有，各架构 crate 测试）

State 布局（512B/528B/264B）、enable/disable 幂等、save/restore 往返——由
`os/arch/src/{x86_64,arm64,riscv64}/fpu.rs` 的 `#[cfg(test)]` 模块覆盖。

**测试统计（截至 2026-08-14）**：`cargo test -p minix-arch --lib` 全通过（172 个，
含 dispatcher 14 个）；本节列出与本主题直接相关的 5 个（子集）。

---

## Ch6: 参见

- [14-exception-interrupt.md](14-exception-interrupt.md) — 异常分发框架（vector 7 分类表 + dispatcher 特判清单；本轮已同步）
- [16-smp.md](16-smp.md) — 每 CPU 数据（fpu_owner）、SAVE_CTX IPI 保存路径
- [10-switch-to-user.md](10-switch-to-user.md) — 上下文切换 / restore_user_context（直跳路径限制）
- [19-syscall-signal.md](19-syscall-signal.md) — 信号传递（sigcontext 框架；FPU 保存为已知缺口）
- Minix3 C：`minix3/minix/kernel/arch/i386/arch_system.c`（fpu 函数族）、`minix3/minix/kernel/proc.c`（copr_not_available_handler/release_fpu）、`minix3/minix/kernel/arch/i386/exception.c`（enable/disable_fpu_exception + fxrstor 失败重定向）、`minix3/minix/kernel/arch/i386/mpx.S`（汇编入口）、`minix3/minix/kernel/system/do_sigsend.c`（信号路径）、`minix3/minix/include/arch/i386/include/fpu.h`、`minix3/minix/kernel/cpulocals.h`
- Rust：`os/arch/src/arch/fpu_arch.rs`、`os/arch/src/{x86_64,arm64,riscv64}/fpu.rs`、`os/kernel/src/smp.rs`、`os/arch/src/arch/exception_dispatcher.rs`
