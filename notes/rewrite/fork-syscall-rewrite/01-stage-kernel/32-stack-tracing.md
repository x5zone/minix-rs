# 32-stack-tracing: 栈回溯（Stack Tracing）—— frame-pointer 链遍历 + 跨地址空间读取

> **源码**: `minix3/minix/kernel/arch/i386/exception.c`（proc_stacktrace 家族）、`minix3/minix/kernel/system/do_diagctl.c`（DIAGCTL STACKTRACE）、`minix3/minix/kernel/utility.c`（util_stacktrace）
> **关联 Rust**: `os/arch/src/arch/stacktrace.rs`（`StacktraceArch` trait）、`os/arch/src/x86_64/boot.rs:357`、`os/arch/src/arm64/boot.rs:280`（impl）
> **创建**: 2026-08-13（Task 2 反向覆盖：`StacktraceArch` 有 Rust 实现零文档 → 新建补足）
> **Review**: 2026-08-13 CONVERGED（Gates 0/A/B/C/D/D-6/E/G/H 全 PASS，VERIFY-CHECK 100%）

---

## Ch1: 概念

### 1.1 栈回溯是什么 + 为什么需要

**栈回溯（stack unwinding / backtrace）** 是内核诊断的基础机制：给定一个进程（或内核自身）的 CPU 上下文，沿其调用栈逐帧向上，打印每一帧的返回地址，从而还原"代码执行到了哪条路径"。内核在三种场景使用它：

1. **崩溃诊断**——内核异常（page fault / GP 等）时打印当前进程 + 内核自身的回溯（C `exception.c:155-170`）
2. **诊断 syscall**——用户显式请求某个进程的回溯（C `DIAGCTL_CODE_STACKTRACE`）
3. **panic 路径**——`panic()` 时打印内核栈回溯（C `util_stacktrace()`）

为什么需要**专门**的机制而不是读寄存器？因为 CPU 寄存器只保存**当前帧**的 PC/SP/FP——调用链上更早的帧只存在于内存中的栈上。要还原调用链，必须**理解栈帧布局**并逐帧遍历。

### 1.2 frame-pointer 链：`[saved_fp][return_addr]` 8 字节对

x86-64 / aarch64 / riscv64 在开启 frame pointer（FP）时，每个函数入口保存调用者的 FP 到栈上，构成**链表**：

```
帧 N (调用者)          帧 N+1 (被调用者)
┌─────────────────┐   ┌─────────────────┐
│ saved_fp ───────┼──▶│ saved_fp ───────┼──▶ ...
│ return_addr     │   │ return_addr     │
└─────────────────┘   └─────────────────┘
   ▲ fp (帧 N+1 的 FP 寄存器指向其 saved_fp 槽)
```

- `fp + 0` = 调用者的 FP（`saved_fp`）
- `fp + 8` = 返回地址（`return_addr`，x86-64 是 `[rbp+8]`，aarch64 是 `[x29+8]` 即 LR，riscv64 是 `[s0+8]` 即 ra）

遍历算法：`fp → 读 [fp+0] 得 next_fp，读 [fp+8] 得 return_addr → fp = next_fp → 重复`，直到 `fp == 0`（栈底）。

**为什么返回地址在 fp+8**：调用约定保证调用者将返回地址压栈后、被调用者再保存 FP——两个 8 字节槽相邻。注意 x86-64/aarch64 的 ABI **栈对齐是 16 字节**（`rsp % 16 == 0` 于 call 指令前），但**帧记录本身是 8 字节对**——对齐不影响 `[fp][fp+8]` 布局。

### 1.3 内核栈 vs 用户栈回溯（权限差异）

回溯要读的栈数据位于**目标进程的地址空间**：

- **内核进程**（如 `SYSTEM`）：栈在直接映射区，内核可 `memcpy` 直读
- **用户进程**：栈在用户地址空间，内核不能直接解引用用户指针（`%gs` 分页属性 / 未映射页）——必须走**跨地址空间复制**（`data_copy`，见 doc 24 cross-space）

C 用 `PRCOPY` 宏统一二者：

```c
#define PRCOPY(pr, pv, v, n) \
  (iskernel ? (memcpy((char *) v, (char *) pv, n), OK) : \
     data_copy(pr->p_endpoint, pv, KERNEL, (vir_bytes) (v), n))
```

`iskernelp(whichproc)` 判定目标是否内核进程。**读取失败（如用户栈帧未映射）不是致命错误**——回溯打印占位符 `(v_bp 0x%lx ?)` 后终止，诊断路径必须容错。

### 1.4 上下文可用性：KTS 与从用户栈恢复 FP

C 的 `proc_stacktrace()` 先判定目标进程的陷阱类型（`p_kern_trap_style`，KTS）：

| KTS | 上下文状态 | FP 来源 |
|-----|-----------|---------|
| `KTS_NONE` | 进程正在运行（未陷入）| 打印 WARNING，继续尝试 |
| `KTS_SYSENTER` / `KTS_SYSCALL` | 陷阱代码**只保存了部分寄存器**（i386 SYSENTER 快速路径）| 从用户栈 `sp + 16` 复制出保存的 bp |
| 其他（完整上下文）| 寄存器完整保存 | `p_reg.fp` 直接可用 |

`KTS_SYSENTER/SYSCALL` 分支依赖一个**布局耦合**：陷阱汇编（`usermapped_glo_ipc.S`）在用户栈 `sp+16` 处保存了 bp——回溯代码硬编码这个偏移（`exception.c:351-353` 注释明言此依赖，doc 28 §栈布局依赖 已侧注）。这是 i386 时代"快速系统调用只保存必要寄存器"的硬件约束。

### 1.5 健壮性：循环检测 + 截断上限 + 读取失败容错

栈数据可能损坏（被覆盖、未映射、循环），回溯必须**有界**：

1. **读取失败**：`data_copy`/`memcpy` 失败 → 打印 `(v_bp ... ?)` 终止（C `PRCOPY != OK`）
2. **循环/后向**：`saved_fp <= fp` → 打印 `(hbp 0x%lx ?)` 终止——正常帧链必然严格递增（栈向下生长，越早的帧地址越高），非递增 = 损坏
3. **截断上限**：C `n > 50` 步截断（`truncated after %d steps`）；Rust `MAX_STACK_FRAMES = 32`

这三条是"诊断路径不能二次崩溃"原则的体现：回溯本身必须比它诊断的崩溃更健壮。

### 1.6 三架构 FP 寄存器统一抽象

| 架构 | FP 寄存器 | 通用寄存器数组索引 | PC 来源 |
|------|-----------|-------------------|---------|
| x86-64 | `rbp` | `gp_regs[5]`（GP_RBP，os/arch/src/x86_64/signal.rs:38）| `rip`（命名字段）|
| aarch64 | `x29` | `gp_regs[28]`（X29）| `pc`（ELR_EL1，命名字段）|
| riscv64 | `s0` / `fp` | `gp_regs[6]`（GP_S0，os/arch/src/riscv64/signal.rs:80）| `pc`（sepc，命名字段）|

三架构的**帧记录布局一致**（`[saved_fp][return_addr]` 8 字节对）——差异只在"FP 存在哪个寄存器槽"和"PC 从哪读"。这是典型的"统一抽象 + 架构差异参数化"：核心算法（链遍历）共享，寄存器提取 per-arch。

> **跨架构统一抽象（先行）**：`StacktraceArch` trait 将差异收敛为两个方法 `frame_pointer()` / `program_counter()`，链遍历算法（`walk_frames`）在 trait 默认实现中三架构共享——这正是本仓库 multi-arch 文档"统一抽象先行"原则的实例（对比 doc 14 异常分发、doc 31 FPU 的 trait 化）。

---

## Ch2: C 源码分析

### 2.1 `proc_stacktrace()` — 入口：KTS 分流 + 上下文恢复（exception.c:333-373）

```c
void proc_stacktrace(struct proc *whichproc)
{
	u32_t use_bp;

	if(whichproc->p_seg.p_kern_trap_style == KTS_NONE) {
		printf("WARNING: stacktrace of running process\n");
	}

	switch(whichproc->p_seg.p_kern_trap_style) {
		case KTS_SYSENTER:
		case KTS_SYSCALL:
		{
			u32_t sp = whichproc->p_reg.sp;
			/* ... 16 byte offset is a dependency on the trap code
			 * in kernel/arch/i386/usermapped_glo_ipc.S ... */
			if(data_copy(whichproc->p_endpoint, sp+16,
			  KERNEL, (vir_bytes) &use_bp,
				sizeof(use_bp)) != OK) {
				printf("stacktrace: aborting, copy failed\n");
				return;
			}
			break;
		}
		default:
			/* Full context is available; use the stored ebp */
			use_bp = whichproc->p_reg.fp;
			break;
	}

#if USE_SYSDEBUG
	proc_stacktrace_execute(whichproc, use_bp, whichproc->p_reg.pc);
#endif /* USE_SYSDEBUG */
}
```

要点：
- **KTS_NONE**（进程正在运行，上下文在 CPU 上不在内存）：只打 WARNING，不中止——回溯可能拿到陈旧栈数据
- **sp+16 恢复 bp**：`data_copy(whichproc->p_endpoint, sp+16, KERNEL, ...)`——从**用户地址空间**复制 sp+16 处的 bp（`data_copy` 的 `endpoint` 参数编码目标地址空间，见 doc 24）；失败 → `aborting, copy failed` 直接 return
- **`USE_SYSDEBUG` 编译期门**：整个回溯执行被宏包住——非调试构建完全裁剪（`#if USE_SYSDEBUG`）
- 最终调用 `proc_stacktrace_execute(whichproc, use_bp, p_reg.pc)`

### 2.2 `proc_stacktrace_execute()` — 链遍历核心（exception.c:287-330）

```c
static void proc_stacktrace_execute(struct proc *whichproc, reg_t v_bp, reg_t pc)
{
	reg_t v_hbp;
	int iskernel;
	int n = 0;

	iskernel = iskernelp(whichproc);

	printf("%-8.8s %6d 0x%lx ", whichproc->p_name, whichproc->p_endpoint, pc);

	while(v_bp) {
		reg_t v_pc;

#define PRCOPY(pr, pv, v, n) \
  (iskernel ? (memcpy((char *) v, (char *) pv, n), OK) : \
     data_copy(pr->p_endpoint, pv, KERNEL, (vir_bytes) (v), n))

	        if(PRCOPY(whichproc, v_bp, &v_hbp, sizeof(v_hbp)) != OK) {
			printf("(v_bp 0x%lx ?)", v_bp);
			break;
		}
		if(PRCOPY(whichproc, v_bp + sizeof(v_pc), &v_pc, sizeof(v_pc)) != OK) {
			printf("(v_pc 0x%lx ?)", v_bp + sizeof(v_pc));
			break;
		}
		printf("0x%lx ", (unsigned long) v_pc);
		if(v_hbp != 0 && v_hbp <= v_bp) {
			printf("(hbp 0x%lx ?)", v_hbp);
			break;
		}
		v_bp = v_hbp;
		if(n++ > 50) {
			printf("(truncated after %d steps) ", n);
			break;
		}
	}
	printf("\n");
}
```

要点（算法即 1.2 节的帧链遍历）：
- 输出格式：`进程名 endpoint pc` 开头，随后每帧一个 `0x%lx` 返回地址
- **PRCOPY 宏**（1.3 节）：iskernel 判定 → memcpy 直读 vs data_copy 跨空间；`sizeof(v_hbp)` = 8（reg_t 64 位）——即读 `[v_bp]` 与 `[v_bp+8]` 两个 8 字节槽
- **两个终止条件**：PRCOPY 失败（`(v_bp ?)`）+ `v_hbp <= v_bp`（循环/后向，`(hbp ?)`）
- **50 步截断**：`n++ > 50` → `(truncated after %d steps)`

### 2.3 内核异常路径 — 崩溃时回溯（exception.c:155-170）

```c
  { 
  	reg_t k_ebp = REG(6);
  	printf("KERNEL stacktrace, starting with ebp = 0x%lx:\n", k_ebp);
  	proc_stacktrace_execute(proc_addr(SYSTEM), k_ebp, frame->eip);
  }

  if (saved_proc) {
	  printf("scheduled was: process %d (%s), ", ...);
	  printf("pc = 0x%x\n", (unsigned) saved_proc->p_reg.pc);
	  proc_stacktrace(saved_proc);
	  panic("Unhandled kernel exception");
  }
```

`exception_handler`（内核态异常路径）中：
- **内核自身回溯**：从异常帧取 `ebp`（`REG(6)`），以 `proc_addr(SYSTEM)` 为"进程"执行 `proc_stacktrace_execute`——内核栈在直接映射区，PRCOPY 走 iskernel memcpy 分支
- **被抢占进程回溯**：`saved_proc`（异常发生前正运行的进程）→ `proc_stacktrace(saved_proc)` → `panic("Unhandled kernel exception")`

这是 1.1 节场景 1 的落点：内核异常 = 双向回溯（内核帧 + 受害者进程帧）+ panic。

### 2.4 DIAGCTL STACKTRACE syscall（do_diagctl.c:43-46）

```c
    case DIAGCTL_CODE_STACKTRACE:
	if(!isokendpt(m_ptr->m_lsys_krn_sys_diagctl.endpt, &proc_nr))
		return EINVAL;
	proc_stacktrace(proc_addr(proc_nr));
	return OK;
```

`SYS_DIAGCTL` 的 `DIAGCTL_CODE_STACKTRACE` 子命令：对指定进程（`proc_nr` 从消息参数取）执行回溯。这是用户/系统管理触发路径（1.1 节场景 2）。注意 C 的 case 体先做 `isokendpt` 端点合法性校验（失败返回 `EINVAL`），再执行回溯并 `return OK`——完整的 DIAGCTL 分发见 doc 13（`dispatch_diagctl`，syscall.rs:2108）。

### 2.5 `util_stacktrace()` — panic 路径内核栈回溯（utility.c:39）

```c
  printf("kernel on CPU %d: ", cpuid);
  util_stacktrace();
  ...
  /* Abort MINIX. */
  minix_shutdown(0);
```

`panic()` 中打印 CPU id + 内核栈回溯 + `minix_shutdown`。注意与 2.3 的区别：2.3 是**异常**路径（有 frame 可读 ebp），`util_stacktrace` 是 **panic** 路径（无 frame，从当前 CPU 寄存器展开）——arch 特定实现（i386 在 `arch/i386/exception.c` 下方）。完整 panic 流程见 doc 27（`panic/kputc/_exit`）。

---

## Ch3: Rust 设计决策

| # | 决策点 | C 方案 | Rust 方案 | 理由 |
|---|--------|--------|-----------|------|
| D1 | 架构差异表达 | `#if`/三份 i386 代码 | **`StacktraceArch` trait**（Pattern 14）| 硬件抽象为 trait，行为选择不散落 cfg 分支；测试可注入 |
| D2 | 回溯输出收集 | `printf` 直接打印 | **`emit: FnMut(u64)` 回调** | no_std 诊断路径不分配（panic 时堆不可信）；输出目标由调用方定 |
| D3 | 跨地址空间读取 | `PRCOPY` 宏（iskernel 分支）| **`read_word: impl Fn(u64) -> Option<u64>` 闭包** | 把"user 需 data_copy / kernel 直读"抽象为注入依赖，None = 读失败终止 |
| D4 | 截断上限 | `n > 50` | **`MAX_STACK_FRAMES = 32`** | 显式封顶约束诊断路径内核时间（C 上限靠 printf 可写性兜底）|
| D5 | KTS 上下文恢复 | sp+16 从用户栈恢复 bp | **无 KTS 概念**（ARCH）| Rust 陷阱入口总保存完整 gp_regs（os/arch/src/x86_64/signal.rs:38 GP_RBP=5），i386 SYSENTER 部分保存约束不存在 |
| D6 | syscall 接线 | DIAGCTL STACKTRACE 直调 | **已接线**（syscall.rs:2176）| Rust `dispatch_diagctl` 完整实现：endpoint→nr 解析（C isokendpt 对应）+ `cross_space_copy` read_word 闭包（C PRCOPY 对应）+ `walk_frames` 输出（见 §4.5）；与 C 的差异：无 KTS 特判（完整 gp_regs 总是可读）+ Suspended 视同读失败 |

### D1 细节：为何是 trait 而非 `#[cfg(target_arch)]`

三架构差异 = FP 寄存器槽索引 + PC 来源，**帧记录布局一致**（8 字节对）。若用 `#[cfg(target_arch)]` 在调用点分支，行为选择散落各处且不可测；trait 把差异收敛为 `frame_pointer()` / `program_counter()` 两个方法，**链遍历算法放默认实现三架构共享**（对应 1.6 节"统一抽象先行"）。

### D3 细节：read_word 的安全契约

`walk_frames` 无 unsafe 方法——读取边界责任在闭包：调用方负责让 `read_word` 安全读用户空间（未来接线时 = `data_copy_vmcheck` 包装，doc 24）。`Option<u64>` 表达"读失败即终止"，与 C 的 PRCOPY 失败语义一致。

### D5 细节：KTS 简化的代价

C 的 `KTS_SYSENTER/SYSCALL` 分支 + `usermapped_glo_ipc.S` sp+16 布局耦合（2.1 节）在 Rust 中**不存在**：Rust 的 x86-64 陷阱入口总是保存完整 gp_regs 数组（对比 doc 28 的 usermapped 数据机制——C 用它做快速 IPC，Rust 无此路径）。`frame_pointer()` 直接读 `gp_regs[GP_RBP]`。这是 **ARCH 简化**（架构演进）：C 的硬件约束分支在 Rust 目标架构上无对应物。

---

## Ch4: 实现

### 4.1 `StacktraceArch` trait 定义（os/arch/src/arch/stacktrace.rs:41-113，`MAX_STACK_FRAMES` 在 :30）

```rust
pub const MAX_STACK_FRAMES: usize = 32;

pub trait StacktraceArch: CpuContextArch {
    fn walk_frames<F>(
        cpu_context: &Self::CpuContext,
        read_word: impl Fn(u64) -> Option<u64>,
        mut emit: F,
    )
    where
        F: FnMut(u64),
    {
        // 默认 frame-pointer 链遍历（x86_64/aarch64/riscv64 共享）：
        let mut fp = Self::frame_pointer(cpu_context);
        let pc = Self::program_counter(cpu_context);
        let mut count = 0;
        emit(pc);                 // 当前 PC 先行输出
        count += 1;
        while fp != 0 && count < MAX_STACK_FRAMES {
            let saved_fp = match read_word(fp) { Some(v) => v, None => break };
            let return_addr = match read_word(fp + 8) { Some(v) => v, None => break };
            if return_addr == 0 { break; }
            emit(return_addr);
            count += 1;
            if saved_fp <= fp { break; }   // 循环/后向检测
            fp = saved_fp;
        }
    }

    fn frame_pointer(cpu_context: &Self::CpuContext) -> u64;
    fn program_counter(cpu_context: &Self::CpuContext) -> u64;
}
```

> **C 映射**：默认 `walk_frames` = `proc_stacktrace_execute` 的链遍历（2.2 节）；`frame_pointer`/`program_counter` = C 的 `p_reg.fp`/`p_reg.pc` 提取（2.1 节 default 分支）。差异：C 的 PRCOPY 失败打印占位符后终止，Rust 直接 break（无输出格式需求）；C 的 iskernel 分支由 read_word 闭包承担（D3）。

`walk_frames` 与 C 算法的逐项对照：

| 行为 | C（exception.c:287-330）| Rust（walk_frames）| 差异 |
|------|-------------------------|--------------------|------|
| 起始 PC 输出 | printf(pc) | emit(pc) | 一致 |
| 帧读取 | PRCOPY `[v_bp]` + `[v_bp+8]` | read_word(fp) + read_word(fp+8) | 一致（8 字节槽）|
| 读失败 | 打印 `(v_bp ?)` 终止 | break 终止 | 无输出占位符（调用方决定）|
| 循环检测 | `v_hbp <= v_bp` → `(hbp ?)` 终止 | `saved_fp <= fp` → break | 一致 |
| 截断 | `n > 50` → `(truncated)` | `count >= 32` → 静默停 | 上限值不同（D4）|

### 4.2 x86-64 impl（os/arch/src/x86_64/boot.rs:357-366）

```rust
impl StacktraceArch for X86_64CpuContextArch {
    fn frame_pointer(cpu_context: &X86_64CpuContext) -> u64 {
        // C: whichproc->p_reg.fp — x86-64 stores RBP in gp_regs[GP_RBP].
        // GP_RBP = 5 (see signal.rs:38).
        cpu_context.gp_regs.get(5).copied().unwrap_or(0)
    }

    fn program_counter(cpu_context: &X86_64CpuContext) -> u64 {
        // C: whichproc->p_reg.pc — x86-64 stores RIP as a named field.
        cpu_context.rip
    }
}
```

- `gp_regs[5]` = RBP（`GP_RBP` 常量定义于 `os/arch/src/x86_64/signal.rs:38`，与 doc 16 信号上下文共享同一寄存器布局）
- `rip` 为命名字段——C 的 `p_reg.pc` 在 Rust 中是强类型字段而非数组索引
- 注释标注 `-C force-frame-pointers` 前提：无 frame pointer 时 rbp 是通用寄存器，walk 立即停止（`fp=0`）

### 4.3 aarch64 impl（os/arch/src/arm64/boot.rs:280-289）

```rust
impl StacktraceArch for AArch64CpuContextArch {
    fn frame_pointer(cpu_context: &AArch64CpuContext) -> u64 {
        // gp_regs layout: [0]=X1, ..., [27]=X28, [28]=X29, [29]=X30.
        // X29 (FP) → index 28.
        cpu_context.gp_regs.get(28).copied().unwrap_or(0)
    }

    fn program_counter(cpu_context: &AArch64CpuContext) -> u64 {
        // aarch64 stores PC as ELR_EL1 (named field).
        cpu_context.pc
    }
}
```

- aarch64 的 gp_regs 布局与 x86-64 不同（X29 在 index 28）；注释明示布局，防止索引漂移
- **ABI 保证**：aarch64 调用约定要求帧指针（unlike x86-64 可选）——walk 可靠除非 `-C disable-fp`（罕见）

### 4.4 riscv64（已实现，FIX-32-2 2026-08-13）

`impl StacktraceArch for Riscv64CpuContextArch` 位于 `os/arch/src/riscv64/boot.rs:182-198`：

- `frame_pointer`：`gp_regs[GP_S0]`（GP_S0 = 6，os/arch/src/riscv64/signal.rs:80；X8 = s0/fp）——注意 **X8 是寄存器号，`gp_regs` 索引是 6**（布局跳过 X0/X2/X10，`GP_S0` 常量防索引漂移）
- `program_counter`：`sepc`（命名字段，RISC-V 陷阱返回地址）
- ABI 注释：RISC-V 帧指针**可选**（同 x86-64，异于 aarch64 强制）——walk 是 best-effort（`-fomit-frame-pointer` 编译的代码无链可走）

> **如实记录**：riscv64 target 项目级编译当前**未验证**——`cargo check -p minix-arch --target riscv64gc-unknown-none-elf` 有 250 个**预先存在**错误（core prelude 缺失类，`cannot find derive/Copy/assert` 等，涉及 post_init 等多文件），与本 impl 无关（stash 前后错误数 251=251 零增量）。riscv64 target 全量修复是独立工程，超出本文档范围，登记 backlog。

### 4.5 接线状态与测试补充（2026-08-13 同步）

**接线（DIAGCTL 已实现，2026-08-14 核实）**：
- `dispatch_diagctl`（syscall.rs:2108）`DIAGCTL_CODE_STACKTRACE` 子命令（syscall.rs:2176-2265）**已完整实现**：
  1. `endpoint_to_nr` + `proc_table.get` —— C `isokendpt` 校验对应（失败 `EINVAL`）
  2. `read_word` 闭包：`cross_space_copy`（doc 24）从目标进程地址空间读 8 字节（`AddressRef::Process` → `AddressRef::Physical` 直接映射缓冲），`Completed(Ok)` → `u64::from_le_bytes`，**其余（含 `Suspended`）→ `None` 终止**——C 的 `data_copy` 失败即停语义对应（C 用 `data_copy` 而非 `data_copy_vmcheck`，无 VMSUSPEND 副作用；Rust 同样刻意避免 `VmSuspend`）
  3. `CurrentStacktraceArch::walk_frames(&target_ctx, read_word, emit)` 输出到 EarlyConsole
  4. 返回 `KcallResult::Ok(OK)`
- **与 C 的差异**（实现注释明示）：C 的 `KTS_SYSENTER/SYSCALL` 特判（sp+16 从用户栈恢复 bp）在 Rust **不存在**——Rust 陷阱入口总是保存完整 `gp_regs`，直接读 `frame_pointer()`（D5 语义落点）；对 aarch64/riscv64（完整上下文）与 x86-64（RBP 正确保存时）均正确
- panic 路径（doc 27 §4.1）：内核栈回溯（`util_stacktrace` 等价物）仍**未实现**——`#[panic_handler]` 目前 halt-loop；`StacktraceArch` 面向**进程**栈（C `proc_stacktrace` 语义），panic 内核栈展开是独立工作（需要当前 CPU 寄存器而非进程上下文），不在此 trait 范围

**测试补充（FIX-32-1，本次同步）**：`walk_frames` 是纯算法（read_word 闭包注入），host (x86_64) 可直接构造 `X86_64CpuContext`（`pub const fn new()` + `pub(super) gp_regs`）——在 `x86_64/boot.rs` tests 模块补 4 项测试（见 Ch5）。arm64/riscv64 impl 需交叉 target，host 不测（如实记录）。

---

## Ch5: 测试

### 5.1 walk_frames 单元测试（x86_64/boot.rs tests 模块，FIX-32-1 新增）

| # | 测试名 | 断言 | C 对照 |
|---|--------|------|--------|
| 1 | `test_stacktrace_walk_frames_emits_pc_and_chain` | emit 序列 = [pc, ret1, ret2]（3 帧链）| proc_stacktrace_execute 链遍历 |
| 2 | `test_stacktrace_walk_frames_stops_on_cycle` | saved_fp <= fp → 停（不无限循环）| `(hbp ?)` 终止 |
| 3 | `test_stacktrace_walk_frames_stops_on_fault` | read_word None → 停 | PRCOPY 失败终止 |
| 4 | `test_stacktrace_walk_frames_caps_at_max` | emit 总数 = MAX_STACK_FRAMES（32，含 pc）| `n > 50` 截断 |
| 5 | `test_stacktrace_frame_pointer_uses_gp_regs_5` | frame_pointer 读 gp_regs[5]（RBP）| p_reg.fp |

```rust
#[cfg(test)]
mod stacktrace_tests {
    use super::*;
    use crate::arch::stacktrace::StacktraceArch;

    // 构造：gp_regs[GP_RBP] = 0x2000（首帧 fp），rip = 0x1000
    // 假栈： [0x2000] = 0x4000 (saved_fp)  [0x2008] = 0x5000 (return_addr)
    //        [0x4000] = 0x0     (栈底)     [0x4008] = 0x6000
    #[test]
    fn test_stacktrace_walk_frames_emits_pc_and_chain() { ... }
    ...
}
```

### 5.2 测试统计（截至 2026-08-13）

- `cargo test -p minix-arch --lib`：**172 passed**（含 FIX-32-1 新增 5 项；此前 167）
- arm64/riscv64 impl：host 无法编译（target-gated），**如实记录未测试**
- 完整测试清单：`rg "^\s*fn test_" os/arch/src/x86_64/boot.rs`

---

## Ch6: 参见

- **14-exception-interrupt.md** — 异常分发框架（`proc_stacktrace` 所在 exception.c 的主路径）
- **27-kernel-utility.md** — panic 流程 + `util_stacktrace`（内核栈回溯未实现状态）
- **28-usermapped-data.md** — `usermapped_glo_ipc.S` 栈布局（sp+16 偏移依赖的源头，§栈布局依赖）
- **13-syscall-dispatch.md** — `dispatch_diagctl` 分发（DIAGCTL STACKTRACE 子命令接线状态）
- **24-cross-space-runtime.md** — `data_copy_vmcheck` / 跨地址空间复制（read_word 闭包接线前置）
- **16-signal-handling.md** — gp_regs 布局（GP_RBP=5 常量定义）
- **31-fpu-context-switching.md** — 同批补足文档（trait 化硬件机制的另一实例）
