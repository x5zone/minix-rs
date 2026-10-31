# 01 · RISC-V 用户返回栈基址的 SMP 缺陷：共享格、per-hart sscratch 与 trait 契约

> **用途与读者**：这份文档面向不持有本仓库代码的第三方评审者，记录一个 RISC-V 多核启动后的用户返回栈基址缺陷、当前止血补丁的适用边界，以及建议的结构性修法。
> **自包含声明**：文中引用的代码都以「路径 + 符号 + 摘要」给出，关键片段逐字内嵌；不依赖仓库内其它文档即可判断。操作系统与 RISC-V 的通用概念在 §0 解释，读者不需要预先读过本项目。
> **标注约定**：代码事实给出符号锚点；测试读数给出配置与结果；尚未证实之处明确标「待验证」；建议与事实分开写。

**结论摘要**：当前补丁在每次用户返回前刷新一个共享内存格，使 RISC-V 在单核、双核、四核配置下都能通过测试。它能消除已经观测到的故障，但没有消除共享内存格本身：返回路径仍然从该格读取栈顶，安全前提是「同一时刻只有一个 hart 返回到用户态」。这个前提目前由一段临时平台钳位维持，不是返回路径自身的不变量。建议把共享格从返回路径删除，改由 per-hart 的 `sscratch` 承载，并通过 protection trait 在每次用户返回前设置。

---

## 0. 术语与运行环境

### 0.1 项目与执行模型

Minix-RS 是把 Minix3 微内核按语义重写为 Rust 的实现。内核运行在 RISC-V 的 S 模式（Supervisor mode，内核态），用户进程运行在 U 模式（User mode，用户态）。内核是对称多处理（Symmetric Multiprocessing，SMP）的：每个 CPU 核称为一个 hart；启动时最先运行的 hart 称为 BSP（Boot Strap Processor，引导处理器），其余 hart 称为 AP（Application Processor，应用处理器）。

内核用一把大内核锁（Big Kernel Lock，BKL）串行化大部分内核态操作。调度器在返回用户态之前会释放这把锁，因为用户态不能持有内核锁。这个释放点很重要：用户返回路径上的代码不是在内核锁保护下执行的。

### 0.2 RISC-V 的用户态与内核态切换

RISC-V 的 S 模式有一个控制状态寄存器（Control and Status Register，CSR）叫 `sscratch`。在这个内核里，`sscratch` 的约定是：

- 当 CPU 在用户态运行时，`sscratch` 保存**当前 CPU 的内核栈顶**。
- 当用户态发生陷阱（trap，包括中断、异常、系统调用）进入内核时，陷阱入口执行 `csrrw sp, sscratch, sp`：`sp` 变成内核栈顶，`sscratch` 变成刚才的用户栈指针。
- 内核返回用户态之前，必须把**当前 CPU 的内核栈顶**重新写回 `sscratch`；否则下一次用户陷阱会把用户栈指针当成内核栈顶，在用户栈上建立陷阱帧。

内核栈是每个 CPU 一份的：BSP 和每个 AP 各有一个固定的内核栈。用户进程不拥有独立的内核栈；它们共享当前 CPU 的内核栈。因此，返回用户态时写入 `sscratch` 的值必须是**当前 CPU**的栈顶，不能是别的 CPU 的栈顶。

### 0.3 本文涉及的内核对象

| 名称 | 含义 |
|---|---|
| `KERNEL_TRAP_STACK_BASE` | 一个全局内存格，保存 `restore_to_user` 汇编在返回用户态前用来重锚 `sscratch` 的栈顶值。本文的缺陷就出在它是全局共享的。 |
| `restore_to_user` | 架构相关的最后一步：装载用户陷阱帧和寄存器，执行 `sret` 进入用户态，不返回。 |
| `finish_and_restore` | 调度器的最后一步：选择进程、切换地址空间、释放 BKL、调用 `restore_to_user`。它是 `restore_to_user` 在当前代码中的唯一生产调用点。 |
| `current_cpu_id()` | 返回当前逻辑 CPU 编号。RISC-V 没有 S 模式可读的 hart id，所以这个函数用「当前栈指针落在哪个 CPU 的内核栈区间」来推断 CPU。 |
| `CpuId` | 逻辑 CPU 编号的包装类型。 |
| `VirBytes` | 虚拟地址的包装类型。 |
| 陷阱帧（trap frame） | 陷阱入口在内核栈上保存的用户寄存器现场。 |
| `stvec` | S 模式的陷阱入口地址寄存器。用户态陷阱从用户陷阱入口进入内核。 |
| `sret` | 从 S 模式返回 U 模式的指令。 |
| VM | 用户态的内存管理服务器。它通过内核调用请求内核替它修改页表。 |
| VmDm | VM 的设备管理代理；它通过 `PteWrite` 内核调用请求内核写入页表项（Page Table Entry，PTE）。 |
| `dispatch_vmctl` | 内核处理 VM 控制调用的分发函数。它在真正分发之前检查调用者身份。 |
| `VM_HEAP_BASE` | VM 的堆起始虚拟地址。本文的故障表现为这个地址没有页表映射。 |
| `scause` | RISC-V 的陷阱原因寄存器。数值 `0xf` 表示 store page fault（写页错误）。 |
| ATF | 本项目使用的自动化测试套件。`-smp N` 表示用 N 个 CPU 核启动测试虚拟机。 |
| `KC` / `PW` | 串口日志标记。`KC` 是内核调用日志，`PW` 是页表写入日志。 |

### 0.4 为什么 RISC-V 需要「重锚」

用户进程在用户态运行时，`sscratch` 保存内核栈顶。用户陷阱进入内核后，`sscratch` 变成该进程的用户栈指针。如果这个进程在内核态被挂起（park），调度器切换到另一个进程，那么下一次返回用户态时，`sscratch` 仍然保存着前一个进程的用户栈指针。此时必须重新写入当前 CPU 的内核栈顶，否则下一个用户陷阱会把前一个进程的用户栈当成内核栈。

这个动作就是「重锚」（re-anchor）。x86-64 的 CPU 在特权级转换时会从每 CPU 的任务状态段（Task State Segment，TSS）自动装载内核栈指针；aarch64 在返回用户态时会从每 CPU 的 `TPIDR_EL1` 重新装载异常栈。RISC-V 没有这种自动装载，所以内核必须在 `sret` 之前显式写 `sscratch`。

---

## 1. 现象

在 RISC-V 的 QEMU 测试中：

- `-smp 1`（只有 BSP）时，ATF 测试 36 项全部通过（34 passed + 2 skipped，0 failed/broken）。
- `-smp 2` 或 `-smp 4`（有 AP）时，系统能启动、能进入调度器，但在 VM 初始化堆的时候失败。
- 故障点是 VM 在 `memset` 中写 `VM_HEAP_BASE = 0x1400000000`，触发 store page fault，`scause = 0xf`。
- 串口里没有任何 `PW pa=` 页表写入日志，说明 VmDm 的 `PteWrite` 代理根本没有执行。
- VM 的堆分配路径已经进入（分配器的诊断计数为 40），所以不是分配器没有分配，而是分配出来的地址没有页表映射。
- 串口里有一条内核调用日志 `KC cpu=1 nr=-4`：这次内核调用被记到了 CPU 1 的空闲进程上，而不是发起调用的 VM 进程。

把这几条放在一起，故障链就很清楚：VM 的请求没有被当作 VM 的请求处理，所以在权限检查处就被拒绝，页表写入代理没有运行，堆地址没有映射，最终在写堆时触发写页错误。

---

## 2. 根因链

### 2.1 关键数据结构：一个全局共享的内存格

`os/arch/src/riscv64/protection.rs` 里有一个全局可写静态变量：

```rust
// os/arch/src/riscv64/protection.rs
#[unsafe(no_mangle)]
pub static mut KERNEL_TRAP_STACK_BASE: u64 = 0;
```

`no_mangle` 让汇编可以直接用这个符号名引用它。这个变量由下面这个函数写入。注意这个函数同时写了两份状态：一份是当前 hart 的 `sscratch` CSR，另一份是上面的全局内存格。

```rust
// os/arch/src/riscv64/protection.rs
fn set_entry_stack_base(base: VirBytes) {
    debug_assert_eq!(base.get() % 8, 0, "kernel stack top must be 8-byte aligned");
    unsafe {
        asm!("csrw sscratch, {}", in(reg) base.get(), options(nomem, preserves_flags));
        core::ptr::addr_of_mut!(KERNEL_TRAP_STACK_BASE).write_volatile(base.get());
    }
}
```

`sscratch` 是每个 hart 自己的 CSR，因此第一行写入是 per-hart 的，正确。`KERNEL_TRAP_STACK_BASE` 是普通内存，所有 hart 共享同一个地址，因此第二行写入是全局的，不是 per-hart 的。问题就出在第二行。

### 2.2 AP 启动时把自己的栈顶写进共享格

`ProtectionArch` trait 的 `init_ap` 在每个 AP 启动时调用。RISC-V 的实现如下：

```rust
// os/arch/src/riscv64/protection.rs
fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
    if kernel_stack_top.0 != 0 {
        set_entry_stack_base(kernel_stack_top);
    }
    let _ = cpu_id;
}
```

这段代码做了两件事：

1. 把 AP 自己的内核栈顶写入 AP 自己的 `sscratch`。这一份是正确的。
2. 把同一个值写入全局 `KERNEL_TRAP_STACK_BASE`。这一份会覆盖 BSP 在启动时写入的值。

`cpu_id` 参数被直接丢掉（`let _ = cpu_id;`）。这说明实现没有为不同的 CPU 保存不同的内存格。注释里也写着单 hart 假设：这个端口当前只有单 hart 场景时，共享格是安全的；AP 启动落地之后，这个假设不再成立。

### 2.3 BSP 返回用户态时从共享格重锚

`restore_to_user` 的 RISC-V 汇编在 `sret` 之前执行：

```asm
# os/arch/src/riscv64/trap_return.rs，restore_to_user 的 sret 前重锚
la a4, KERNEL_TRAP_STACK_BASE
ld a4, 0(a4)
csrw sscratch, a4
```

它从全局格读取栈顶，然后写入当前 hart 的 `sscratch`。如果这个格子里是 AP 的栈顶，那么 BSP 的 `sscratch` 就被写成 AP 的栈顶。**Rust 侧刚刚写过的 per-hart `sscratch` 值被这条汇编覆盖了。**

### 2.4 时间线

| 时刻 | 执行者 | 动作 | BSP 的 `sscratch` | 共享格 |
|---|---|---|---|---|
| 启动 | BSP | `init` 写 BSP 栈顶 | BSP 栈顶 | BSP 栈顶 |
| AP 启动 | AP | `init_ap` 写 AP 栈顶 | BSP 栈顶 | AP 栈顶 |
| BSP 返回用户态 | BSP | 汇编读共享格并写 `sscratch` | AP 栈顶 | AP 栈顶 |
| BSP 用户陷阱 | BSP | `csrrw sp, sscratch, sp` | 用户栈指针 | AP 栈顶 |

第四步之后，BSP 的陷阱帧建立在 AP 的内核栈上。

### 2.5 栈式 CPU 识别把调用记到 AP

RISC-V 没有 S 模式可读的 hart id，所以 `current_cpu_id()` 用当前栈指针判断 CPU：

```rust
// os/kernel/src/lib.rs:fn current_cpu_id
#[cfg(target_arch = "riscv64")]
pub fn current_cpu_id() -> crate::proc::CpuId {
    let sp: usize;
    unsafe { core::arch::asm!("mv {}, sp", out(reg) sp) };
    if let Some(cpu) = crate::smp::cpu_id_for_stack_pointer(sp) {
        return cpu;
    }
    crate::proc::CpuId::new_unchecked(crate::smp::bsp_logical())
}
```

`cpu_id_for_stack_pointer` 遍历所有 AP 的内核栈区间，判断 `sp` 是否落在某个区间内：

```rust
// os/kernel/src/smp.rs:fn cpu_id_for_stack_pointer
for i in 0..AP_STACK_COUNT {
    let base = unsafe { core::ptr::addr_of!(AP_KERNEL_STACKS[i]) } as usize;
    if sp > base && sp <= base + AP_KERNEL_STACK_SIZE {
        return Some(CpuId::new_unchecked(i as u32));
    }
}
None
```

因为 BSP 的陷阱帧现在在 AP 的栈上，`sp` 落在 AP 的栈区间，`current_cpu_id()` 返回 AP 的编号。于是内核把这次内核调用记成 AP 的空闲进程，而不是 BSP 上的 VM 进程。

### 2.6 为什么最终是 VM 的写页错误

VM 在初始化堆时，需要 VmDm 通过 `PteWrite` 内核调用把 `VM_HEAP_BASE` 的页表项写进页表。内核调用到达 `dispatch_vmctl` 之前会做调用者身份检查。由于调用被记到 AP 的空闲进程，检查失败，`dispatch_vmctl` 没有执行，`PteWrite` 代理也没有执行。页表项没有建立，VM 随后写 `VM_HEAP_BASE` 时触发 store page fault。

这解释了三个观测：

- 为什么 `-smp 1` 正常：没有 AP 启动，共享格不会被覆盖。
- 为什么 `-smp 2/4` 失败：AP 启动覆盖共享格，BSP 下一次返回用户态读到了 AP 栈顶。
- 为什么没有 `PW` 日志：页表写入代理根本没有运行。

---

## 3. 当前止血补丁

### 3.1 补丁做了什么

调度器在 `restore_to_user` 之前插入了一段 RISC-V 专用代码：

```rust
// os/kernel/src/lib.rs:fn finish_and_restore
#[cfg(target_arch = "riscv64")]
{
    let cpu = crate::current_cpu_id();
    let stack_top = if cpu == smp.bsp_cpu_id() {
        crate::kernel_info().map(|k| k.kern_stack_top.0).unwrap_or(0)
    } else {
        crate::smp::ap_kernel_stack_top(cpu.raw())
    };
    if stack_top != 0 {
        minix_arch::riscv64::protection::set_trap_stack_base(
            minix_types::VirBytes::new(stack_top),
        );
    }
}
unsafe { CurrentTrapReturnArch::restore_to_user(&frame, &ctx) }
```

`set_trap_stack_base` 只是 `set_entry_stack_base` 的公开包装。因此这段代码在每次用户返回前，把当前 CPU 的栈顶同时写入共享格和当前 hart 的 `sscratch`。随后汇编再从共享格读一次，读到的就是刚刚写入的值。

这段代码在 `bkl_unlock()` 之后执行。也就是说，刷新共享格和读取共享格都不在 BKL 保护下。

### 3.2 为什么它能让测试变绿

当前 RISC-V 配置只让 BSP 运行用户进程。显式指定 AP 的 CPU 亲和性会被改写为 BSP；启动集也落在 BSP 的调度队列里，AP 当前只运行空闲循环。`sched_proc` 里有一段 RISC-V 专用钳位：

```rust
// os/kernel/src/sched.rs:fn sched_proc
#[cfg(target_arch = "riscv64")]
{
    let bsp = smp.as_ref().map(|s| s.bsp_cpu_id().raw()).unwrap_or(0);
    params.cpu = params.cpu.map(|_| bsp);
}
```

这段钳位的理由写在代码注释里：默认固件不投递跨 hart 的软件中断（software interrupt），如果把进程放到 AP 上，它永远不会被唤醒。因此当前只能让 BSP 运行用户进程，AP 只运行空闲循环。这段钳位是临时平台门，不是永久的调度设计；等固件委派或软件中断设备可用之后，它才可能被移除。

在「只有 BSP 运行用户进程」的前提下，同一时刻只有一个 hart 执行用户返回，共享格的读写不会交错。补丁在第一次 BSP 用户返回之前把共享格改回 BSP 栈顶，于是故障链被切断。

### 3.3 证据与边界

当前读数：

- RISC-V ATF `-smp 1`、`-smp 2`、`-smp 4` 均为 36/36（34 passed + 2 skipped，0 failed/broken）。
- 全量测试 `run_all` 为 39 passed / 0 failed / 0 skipped / 0 build failures。
- 内核宿主测试为 850 passed / 0 failed / 8 ignored。
- 三架构内核编译检查为 0 error。

这些读数说明：在当前的 BSP 钳位下，已经观测到的故障不再出现。它们不能证明「两个 hart 同时返回用户态」也安全，因为当前配置根本不会产生这种并发。

补丁没有增加针对这段逻辑的测试或源码 pin。它依赖全系统测试来覆盖，回归保护弱于它对不变量的重要性。

---

## 4. 为什么止血不是根因修复

### 4.1 共享格仍然可以被另一个 hart 改写

补丁把「写共享格」和「读共享格」都保留了下来，因此两个 hart 同时执行用户返回时，仍然可能交错：

| 时刻 | hart A | hart B | 共享格 |
|---|---|---|---|
| 1 | 写共享格 = A 栈顶 | — | A 栈顶 |
| 2 | — | 写共享格 = B 栈顶 | B 栈顶 |
| 3 | 汇编读共享格 | — | B 栈顶 |
| 4 | `sscratch(A)` = B 栈顶 | — | B 栈顶 |

第四步之后，hart A 的 `sscratch` 指向 B 的栈顶。A 的下一次用户陷阱会把陷阱帧建立在 B 的内核栈上，和原来的故障是同一类错误。

`KERNEL_TRAP_STACK_BASE` 是 `static mut`，Rust 侧用的是非原子 volatile 写，汇编侧用的是普通加载。两者之间没有同步关系。64 位对齐的存储在硬件上通常不会撕裂，但「不会撕裂」只保证读到某一个完整值，不保证读到**本 hart 刚写入的值**。两个 hart 并发时，读到另一个 hart 的值是完全合法的硬件行为；在 Rust 的内存模型里，这种并发非原子访问属于数据竞争，存在未定义行为的风险。加一条内存屏障也不能解决这个问题：屏障只能约束顺序，不能把共享格变成 per-hart 的。

还有一个更微妙的点：补丁用 `current_cpu_id()` 来选择要写入的栈顶，而 `current_cpu_id()` 本身又按栈区间推断 CPU。正常启动顺序下，BSP 在第一次返回用户态之前仍然在自己的内核栈上，所以这次刷新能选对 BSP 栈顶，故障链在第一次返回之前就被切断。但如果某条路径已经把 BSP 的陷阱帧放到了 AP 栈上，`current_cpu_id()` 会返回 AP，补丁随后会把 AP 栈顶再写一遍，等于确认了错误状态。这个补丁不是自愈的；它依赖「第一次返回之前栈还是对的」这个前提。

### 4.2 重复真值源：Rust 写了 `sscratch`，汇编又覆盖它

补丁已经在 Rust 侧把当前 CPU 的栈顶写入了 per-hart 的 `sscratch`。紧接着，汇编又从这个共享格读一次并覆盖 `sscratch`。对于当前唯一的调用点 `finish_and_restore` 来说，这次汇编读取是多余的：

- Rust 写入的是当前 hart 的 CSR，天然 per-hart，不会被别的 hart 覆盖。
- 汇编读取的是共享内存，天然共享，会被别的 hart 覆盖。

保留这次读取，等于把已经正确 per-hart 的值丢掉，换成一个可能来自别的 hart 的值。共享格存在的唯一理由就是这次汇编读取；如果删除读取，共享格也可以删除。

### 4.3 文档与代码已经不一致

当前实现附带的注释仍然按单 hart 时代描述：

- 静态变量的文档写着「Single cell, sound today because this port runs riscv64 single-hart」，并说它「written by init/init_ap at boot (before any restore_to_user can run)」。AP 启动已经落地，补丁也在运行期写这个格，这两句都不再成立。
- `restore_to_user` 的注释说它读的是「boot-recorded per-CPU base」，并说这个值「written by init/init_ap, never reachable before the first user return」。补丁之后，这个值在每次用户返回前都会被重写。
- `ProtectionArch` 的文档表格把 RISC-V 的 `init_ap` 描述为「Per-CPU sscratch」，但实现同时写了一个共享内存格，返回路径读的是共享格，不是 per-hart 的 CSR。
- 补丁合并后的静态变量注释还出现断句错误（「must record a The scheduler…」），说明这段说明没有按新行为完整重写。

这些不是纯文字问题。它们把「共享格是单 hart 占位」这个关键前提藏了起来，让后来的读者误以为共享格已经是 per-CPU 的，或者误以为它只在启动阶段写入。

### 4.4 trait 契约与调用面没有覆盖返回路径

`ProtectionArch` 本身已经有 per-CPU 的词汇。它的相关方法都带 `cpu_id`：

```rust
// os/arch/src/arch/protection.rs
pub trait ProtectionArch: Sized {
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self;
    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes);
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes);
}
```

`init_ap` 的文档说它「Creates per-CPU protection structures」，`set_kernel_stack` 的文档说它设置「the kernel stack pointer for privilege level transitions」。所以「当前 CPU 的入口栈」在抽象里不是没有位置。

需要区分一个细节：如果 RISC-V 实现只依赖当前 hart 的 `sscratch`，那么忽略 `cpu_id` 本身并不是错误，因为写 CSR 的代码就运行在目标 hart 上，天然 per-hart。真正的错误是它同时维护了一个共享内存格，并让返回路径读那个格。`cpu_id` 被丢掉只是这个共享格没有 per-CPU 化的症状，不是唯一的根因。

但 trait 的返回路径确实有两处没有做完备。

第一处是调用面。`set_kernel_stack` 需要 `&mut self`，而内核在非 x86 架构上提供的 `with_protection` 只给 `&CurrentProtection`，并且每次构造一个临时的 `init(0, 0)` 实例：

```rust
// os/kernel/src/lib.rs:fn with_protection（非 x86 分支）
#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn with_protection<R>(f: impl FnOnce(&CurrentProtection) -> R) -> R {
    use minix_arch::arch::protection::ProtectionArch as _;
    f(&CurrentProtection::init(0, minix_types::VirBytes::new(0)))
}
```

`init(0, 0)` 里的 0 是「临时实例」的哨兵，避免把真实的 `sscratch` 清成 0。这个辅助函数只给不可变引用，所以操作系统（operating system，OS）层在 RISC-V 上**无法通过 trait 调用 `set_kernel_stack`**。补丁只能绕过 trait，直接调用 RISC-V 模块的自由函数：

```rust
minix_arch::riscv64::protection::set_trap_stack_base(...)
```

这正是抽象没有做完的直接症状。`os/arch/src/arch/protection.rs` 的模块文档明确写着 OS 层不应该引用硬件专属类型；现在 OS 层的调度器直接点了 RISC-V 模块。

第二处是返回路径契约。`TrapReturnArch::restore_to_user` 只接收陷阱帧和寄存器文件：

```rust
// os/arch/src/arch/trap_return.rs
pub trait TrapReturnArch {
    unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> !;
}
```

它没有 `cpu_id`，没有 `stack_top`，也没有一个「准备返回」的钩子。它的文档说实现必须运行在当前 CPU 的内核栈上，但没有说调用者必须在每次返回前保证 per-CPU 入口栈是最新的，也没有禁止实现使用共享状态。x86-64 和 aarch64 可以靠硬件或 per-CPU CSR 自动满足这个前提，RISC-V 不行；于是 RISC-V 实现用共享格补洞，而 trait 没有任何机制阻止这个洞。

### 4.5 缺少针对性的测试

当前测试能证明「在 BSP 钳位下系统能跑」，但不能证明：

- 「`restore_to_user` 不读取任何共享可变状态」。
- 「`init_ap` 为每个 CPU 保存了独立的栈顶」。
- 「两个 hart 同时返回用户态时，各自的 `sscratch` 仍然指向自己的栈」。

现有测试里有一个 `riscv_keeps_the_narrow_firmware_gate_clamp` 源码 pin，它检查 `sched_proc` 里的钳位还在。这个 pin 保护的是平台门，不是返回路径的 per-CPU 不变量。撤掉钳位时它会失败，提醒实现者回来处理；但它不能替代对返回路径本身的测试。

---

## 5. 建议解法

### 5.1 方案 A（推荐）：删除返回路径上的共享格

核心思路：**per-hart 的 `sscratch` 已经是正确的存储位置；不要再从共享内存格读一次。**

具体做法：

1. RISC-V 的 `set_kernel_stack` 只写当前 hart 的 `sscratch`，不再写 `KERNEL_TRAP_STACK_BASE`。
2. 删除 `restore_to_user` 汇编里的三行重锚代码：不再 `la` 共享符号、不再 `ld`、不再覆盖 `sscratch`。
3. `finish_and_restore` 在调用 `restore_to_user` 之前，通过 trait 调用 `set_kernel_stack(cpu, stack_top)`。x86-64 更新 TSS.sp0，aarch64 是文档化的 no-op，RISC-V 写 `sscratch`。三个架构走同一个 OS 层动作。
4. 删除 `KERNEL_TRAP_STACK_BASE`，或者把它降级为只读诊断用途，不再出现在用户返回路径上。

为什么安全：当前 `restore_to_user` 的唯一生产调用点是 `finish_and_restore`，而它在调用前刚刚通过 trait 写入了 per-hart 的 `sscratch`。中间没有别的代码会改 `sscratch`。宿主测试使用 mock 实现，不会执行真实汇编。把「调用者必须在返回前保证 per-CPU 入口栈最新」写进 trait 契约，并在 `restore_to_user` 的安全注释里重复，就可以把这条前提变成显式约束。

这个方案需要解决 `set_kernel_stack` 的可调用性：

- 把 `set_kernel_stack` 改成 `&self`，让实现用内部可变状态（per-CPU 数组或 CSR）完成写入；或者
- 给非 x86 架构补一个 `with_protection_mut`，让 OS 层能拿到可变引用；或者
- 新增一个专门的 `prepare_user_return(cpu, stack_top)` 方法，语义比通用的 `set_kernel_stack` 更窄，避免 `&mut` 的全局别名问题。

无论选哪一种，都不要把「共享内存格」换成「共享 `&mut` 全局对象」；多 hart 返回路径需要的是 per-CPU 状态，不是一把新的全局锁。

### 5.2 方案 B：把共享格改成 per-CPU 数组

如果汇编必须从内存读栈顶，那么共享格至少要变成 per-CPU 数组，例如 `[AtomicU64; MAX_CPUS]`，按 CPU 编号索引。

这个方案的难点是「汇编怎么知道当前 CPU 编号」。RISC-V 没有 S 模式可读的 hart id；`current_cpu_id()` 又是按栈区间推断的。如果栈已经被错误地锚定，按栈推断出来的 CPU 编号也会错，形成循环依赖。可行的补充手段包括：

- 在陷阱入口把 CPU 编号保存到每个 hart 可访问的寄存器（例如把 `tp` 当作内核态 scratch，但必须在返回用户态前恢复用户 `tp`）。
- 把栈顶作为参数传给 `restore_to_user`，由汇编直接写 `sscratch`，完全不读内存。
- 为每个 hart 分配独立的页，汇编通过 `sscratch` 之外的 CSR 或固定偏移访问；但这仍然需要可靠的 hart 编号。

因此方案 B 比方案 A 复杂，且容易再次引入「用错误的 CPU 编号索引正确的数组」的问题。

### 5.3 方案 C：把栈顶作为 `restore_to_user` 的参数

把 trait 签名改成：

```rust
unsafe fn restore_to_user(
    frame: &Self::Frame,
    regs: &Self::RegisterFile,
    kernel_stack_top: VirBytes,
) -> !;
```

RISC-V 实现用参数直接写 `sscratch`；x86-64 和 aarch64 可以忽略这个参数，或者用它做断言。这样汇编不需要读任何全局内存，也不需要知道 CPU 编号；栈顶由调度器在调用点显式提供。

代价是三个架构的实现和 mock 都要改签名。它比方案 A 多改一点接口，但把「返回前必须提供 per-CPU 栈顶」变成了类型系统的一部分，不会再被忘记。

### 5.4 三方案对比

| 维度 | 方案 A：删除共享格，trait 写 `sscratch` | 方案 B：per-CPU 数组 | 方案 C：栈顶作为参数 |
|---|---|---|---|
| 共享可变状态 | 删除 | 保留数组，但 per-CPU | 删除 |
| 汇编是否需要读内存 | 不需要 | 需要 | 不需要 |
| 是否需要可靠 CPU 编号 | 需要（Rust 侧选栈顶） | 需要（汇编侧索引） | 不需要 |
| 改动范围 | 中：trait 可调用性 + 删除汇编读取 | 大：CPU 编号来源 + 数组 + 汇编索引 | 中：三架构签名 + 调用点 |
| 并发安全 | 是（per-hart CSR） | 取决于索引是否正确 | 是（参数传值） |
| 回归风险 | 低 | 中 | 低到中 |

推荐方案 A：它删除共享状态，保留现有的 `sscratch` 机制，不需要在汇编里解决「当前是哪个 CPU」的问题。如果评审认为「调用者必须提供栈顶」应该由类型系统强制，那么方案 C 是更严格的版本。

### 5.5 验收标准

结构性修法至少应满足：

1. 用户返回路径上不存在共享可变内存格；`restore_to_user` 不再引用 `KERNEL_TRAP_STACK_BASE`。
2. `init_ap` 和 `set_kernel_stack` 的 per-CPU 语义有代码或测试证明；`cpu_id` 不再被无声丢弃。
3. trait 契约明确写出：每次用户返回前，调用者必须保证当前 CPU 的入口栈是最新的；实现不得用共享状态替代 per-CPU 状态。
4. 增加源码 pin：如果 `restore_to_user` 重新引用共享符号，测试失败。
5. 增加宿主测试：mock 记录 `set_kernel_stack` 收到的 `cpu_id`，验证调用点传的是当前 CPU。
6. 在撤掉 BSP 钳位之前，补一个两个 hart 同时运行用户进程的测试；至少验证两个 hart 的用户陷阱帧分别落在自己的内核栈上。
7. 更新 `KERNEL_TRAP_STACK_BASE`、`set_entry_stack_base`、`restore_to_user` 的注释，删除单 hart 和「只在启动时写入」的旧前提。
8. 在待办账本里登记结构债，并写明触发条件：撤掉 RISC-V 的 BSP 钳位之前必须完成本修法，否则并发返回会重新引入同一类故障。

---

## 6. 给第三方评审的开放问题

1. 当前止血补丁的安全论证依赖「只有 BSP 运行用户进程」的平台钳位。这个论证是否足够支撑补丁短期留在主线？如果足够，它的失效条件是否已经被测试 pin 住？
2. 删除 `restore_to_user` 汇编里的共享格读取，是否还有未发现的生产调用路径依赖那次重锚？当前代码里唯一的生产调用点是 `finish_and_restore`，但测试内核或未来的热插拔路径是否可能直接调用？
3. `ProtectionArch::set_kernel_stack` 目前是 `&mut self`，非 x86 的 `with_protection` 只给不可变引用。应该改 `&self`、补 `with_protection_mut`，还是新增 `prepare_user_return`？哪一种最符合「OS 语义、不描述机制」的原则？
4. `current_cpu_id()` 在 RISC-V 上按栈区间推断 CPU。补丁用它来选择栈顶，等于用「可能已经被破坏的栈」来判断「应该修复到哪个栈」。在正常启动顺序下这能工作，但它是否足够作为长期方案？是否应该让调度器显式持有当前 CPU 编号？
5. 如果保留内存格，per-CPU 数组的索引应该来自哪里？RISC-V 没有 S 模式可读的 hart id，`tp`、陷阱帧或启动时写入的 per-hart 页，哪一种最可靠？
6. 对 x86-64 和 aarch64，是否也存在「per-CPU 入口状态被实现成共享状态」的同类风险？应该用什么样的跨架构测试来防止这类错误？

---

## 附录 A：关键符号索引

| 符号 | 位置 | 作用 |
|---|---|---|
| `KERNEL_TRAP_STACK_BASE` | `os/arch/src/riscv64/protection.rs` | 全局共享的栈顶内存格；本文缺陷的核心。 |
| `set_entry_stack_base` | `os/arch/src/riscv64/protection.rs` | 同时写 `sscratch` 和共享格。 |
| `set_kernel_stack` | `os/arch/src/riscv64/protection.rs` | `ProtectionArch` 的实现；当前忽略 `cpu_id`。 |
| `init_ap` | `os/arch/src/riscv64/protection.rs` | AP 启动时写 AP 的 `sscratch` 和共享格。 |
| `restore_to_user` | `os/arch/src/riscv64/trap_return.rs` | 返回用户态；汇编从共享格重锚 `sscratch`。 |
| `finish_and_restore` | `os/kernel/src/lib.rs` | 调度器最后一步；唯一的 `restore_to_user` 生产调用点。 |
| `current_cpu_id` | `os/kernel/src/lib.rs` | RISC-V 用栈区间推断当前 CPU。 |
| `cpu_id_for_stack_pointer` | `os/kernel/src/smp.rs` | 判断栈指针落在哪个 AP 栈区间。 |
| `sched_proc` | `os/kernel/src/sched.rs` | RISC-V 的 BSP 钳位所在地。 |
| `riscv_keeps_the_narrow_firmware_gate_clamp` | `os/arch/tests/riscv64_ap_entry_pin.rs` | 保护 BSP 钳位的源码 pin。 |
| `ProtectionArch` | `os/arch/src/arch/protection.rs` | 保护机制 trait；已有 per-CPU 参数。 |
| `TrapReturnArch` | `os/arch/src/arch/trap_return.rs` | 返回用户态 trait；没有 per-CPU 输入。 |
| `with_protection` | `os/kernel/src/lib.rs` | 非 x86 分支只给不可变引用。 |
