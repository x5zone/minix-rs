我把这份当成“准备开始实施前的设计评审”来看的。整体评价其实挺高：**已经不是普通 todo，而是一个相当完整的 SMP bring-up 方案**，尤其是把 C 语义、现有 Rust 资产、三架构差异、测试阶梯和提交纪律串起来了。比如 §3.4 对 BKL 的 UNLOCK→等待→LOCK 交接写得很清楚，§2.4 也抓到了 hosted mock 完全绕开生产路径这个真正容易坑人点。 

但如果问我“现在能不能直接按 S-0→S-13 开干”，我的答案是：

**大框架可以，S-0/S-1/S-2 可以马上做；S-3 之前还有几处必须修正的设计问题。**

我会把它分成“必须改”和“值得改”。

---

# 一、我认为最重要的 5 个问题

## 1. RISC-V 的 `hart_start` 描述有一个明确的 ABI 错误

这是我最确定的一处。

你现在 §3.1 写的是：

> RISC-V `hart_start`：`a0=hartid、a1=入口、a2=特权级(0=S-mode)`。
> §3.2 又按照这个模型设计。

但 SBI HSM 的 `hart_start()` 实际是：

```text
a0 = hartid
a1 = start_addr
a2 = opaque
```

而目标 hart 启动后：

```text
satp = 0
sstatus.SIE = 0
a0 = hartid
a1 = opaque
```

也就是说：

**不存在你文档里所谓的 `a2 = privilege level` 这个参数。**

这是 RISC-V SBI HSM 规范明确规定的。([RISC-V Docs][1])

这会直接影响你的 S-3 设计：

```text
BSP
  |
  +-- hart_start(hartid, start_addr, opaque=bootstrap_ptr)
                                   |
                                   v
                              AP:
                              a0 = hartid
                              a1 = bootstrap_ptr
                              satp = 0
                              SIE = 0
```

所以我建议直接改：

```text
RISC-V:
    hart_start(hartid, start_addr, opaque)
    AP entry:
        a0 = hartid
        a1 = opaque
        satp = 0
        SIE = 0
        MMU-off
```

然后 §3.2 的“arm/riscv 用固件 context 参数传指针”就可以统一成立了。

这个我会标 **P0，S-3 前修掉**。

---

# 2. x86 `#[naked] + .code16 + link_section` 这条路线，方向可以，但目前写得过于乐观

这是整份文档最危险的地方。

你现在的方案 A 是：

> `#[naked]` + `naked_asm!` + `.code16` + `.ap_trampoline`，构建后整体复制到 <1MiB。

Rust 本身确实支持 `naked_asm!`，并且 Rust Reference 也明确允许 x86 的 `.code16/.code32/.code64`，只是必须在 asm block 结束时恢复到默认模式。([Rust 文档][2])

所以：

**“Rust 不能写 `.code16`”不是问题。**

真正的问题是：

### 你写的这个东西其实根本不是一个正常的 Rust function

它的真实语义是：

```text
ELF 中的一小段“可搬运机器码 blob”
                  ↓
         runtime memcpy
                  ↓
       低物理地址执行
                  ↓
     与原来的链接地址无关
```

而 `#[naked] fn` 的语义仍然围绕“函数”展开。Rust Reference 对 naked function 依然要求其 asm 满足函数签名 / calling convention 的安全约束。([Rust 文档][3])

你的 AP trampoline 恰恰是：

```text
没有正常 ABI
没有正常 stack
没有正常寄存器状态
CPU 甚至还处于 16-bit real mode
```

因此从模型上说：

> **它不是“一个没有 prologue 的 Rust function”，而是“一个由 Rust 工具链携带的 boot-time binary blob”。**

这两个概念最好分开。

### 我反而更倾向于一个中间方案

不是你否决的：

```text
.S 文件 + 独立 assembler
```

而是：

```text
Rust crate
    |
    +-- global_asm!
         |
         +-- .ap_trampoline
         +-- .ap_trampoline_data
         +-- local labels
         +-- no external references
```

`global_asm!` 本来就是 Rust 官方提供的“全局汇编”机制，而且相比 function-scope asm，它更适合写整个自定义机器码实体。([Rust 文档][4])

这样你仍然：

* 不需要独立 `.S`
* 仍然在 Rust crate 内
* 仍然走 Rust linker/toolchain
* 但没有假装这个东西是一个可被 Rust 调用的函数

我认为这比当前方案 A 更“语义诚实”。

### 更重要的是，你还缺一个硬约束

必须在设计里加入：

> **`.ap_trampoline` 必须是 position-independent、self-contained、relocation-free 的 flat blob。**

否则：

```text
链接时地址 = 0xFFFF....xxx
运行时地址 = 0x0008....xxx
```

任何：

```text
absolute symbol
external symbol
绝对地址常量
非本地 relocation
```

都会让“复制机器码然后运行”直接变成炸弹。

建议 S-3 的验收门里增加：

```text
readelf -S       → section 存在
readelf -r      → 无未处理 relocation
objdump -d      → 16/32/64 三阶段反汇编正确
objcopy / binary → 实际 blob 大小
```

并且做一个最小“复制到非链接地址后执行”的测试。

**这件事情甚至比 QEMU L2 更重要。**

---

# 3. x86 trampoline 的 mailbox 设计还缺一个非常关键的“可搬运性”问题

你写：

> mailbox 是 `.ap_trampoline` 内一个 `AtomicU32`，物理地址 = blob 基址 + 固定偏移。

这个思路本身没问题，但这里实际上存在三个不同对象：

```text
        trampoline code
              +
        trampoline data
              +
        runtime bootstrap state
```

现在把它们揉成一个 `.ap_trampoline`，会造成两个问题。

### 第一：section 权限

链接器很可能最终把你的 trampoline section 当成代码段处理。

而 mailbox 是运行时可写数据。

所以最后要明确成类似：

```text
.ap_trampoline
    executable, immutable

.ap_trampoline_data
    writable
```

或者：

```text
struct ApTrampolineImage {
    code: [...]
    mailbox: ...
}
```

然后 runtime 建立低地址的最终布局。

### 第二：一个 mailbox 还是每 AP 一个 mailbox

C 里的确可以靠：

```text
设置 __ap_id
启动一个 AP
等它读走
再覆盖
```

这种串行方式完成。

但你文档以后又越来越强调：

> bootstrap structure contains logical id / page table / stack / magic pointer

那我更建议直接定义成：

```text
ApBootstrap {
    logical_cpu_id,
    hw_cpu_id,
    page_table_root_pa,
    kernel_stack_top_pa,
    ready_flag_pa,
}
```

而不是让 trampoline 自己理解：

```text
blob + magic offset
```

**trampoline 只负责取出一个 opaque bootstrap pointer，然后跳出去。**

这样代码边界特别漂亮：

```text
x86 trampoline
    ↓
read bootstrap pointer
    ↓
enter long mode
    ↓
Rust::ap_entry(bootstrap)
```

ARM：

```text
PSCI context_id
    ↓
Rust::ap_entry(bootstrap)
```

RISC-V：

```text
SBI opaque → a1
    ↓
Rust::ap_entry(bootstrap)
```

三者最后完全汇合。

---

# 4. §3.7 把 x86 “IDT 入口”和 “SYSCALL 入口”混在了一起

这是第二个我认为需要认真修正文档的地方。

你现在写：

> IDT 门表 + `lidt` + `SYSCALL` MSR 都已经存在
> …
> 系统调用向量 → `kernel_call_dispatch`

然后 S-8 想统一写：

```text
异常 → exception_dispatcher
系统调用 → kernel_call_dispatch
IRQ → IrqManager
```

这里实际上有**两个不同的硬件入口机制**：

```text
          CPU
           |
    +------+------+
    |             |
   IDT          SYSCALL
    |             |
 vector          LSTAR
    |             |
 trap stub      syscall entry
    |             |
    +-------> Rust
```

`SYSCALL` 根本不是“进入某个 IDT vector”。

所以 S-8 最好明确拆成：

### A. IDT/trap gate

负责：

```text
exceptions
external IRQ
IPI
software INT gates
```

### B. SYSCALL entry

独立负责：

```text
LSTAR
STAR
SFMASK
SWAPGS / GS state
user RSP 保存
register save
syscall trapframe
kernel_call_dispatch
```

尤其是你自己在 §2.1 已经明确说：

> SYSCALL/SYSRET 的 MSR 配置已经存在。

那 S-4 的 per-CPU 初始化就必须重新检查：

```text
IA32_GS_BASE
IA32_KERNEL_GS_BASE
IA32_STAR
IA32_LSTAR
IA32_SFMASK
IA32_EFER
```

哪些是 BSP-only，哪些是 per-CPU。

否则非常容易出现：

```text
BSP syscall 正常
AP 进入用户态后一 syscall → 炸
```

这类“QEMU 看起来已经 4 CPU 了，但只有 CPU0 真能跑”的问题。

**我会把这一项标 P0/P1 边界，至少必须在 S-4/S-8 前把入口模型写清楚。**

---

# 5. shutdown 的三架构实现现在基本上写成了一个不存在的“统一 QEMU 后端”

§3.8 写：

> trait + QEMU `isa-debug-exit` 后端

但 `isa-debug-exit` 是 x86 的 ISA 设备，不应该被当成：

```text
x86_64
aarch64
riscv64
    ↓
同一个 isa-debug-exit backend
```

三者共用。

现在已有成熟的 QEMU testing 方案本身也是按架构区分的，例如 `qemu-exit`：

* x86/x86_64 → `isa-debug-exit`
* AArch64 → semihosting
* RISC-V → `sifive_test` 等 QEMU 设备。([GitHub][5])

所以你的 trait 设计没问题：

```rust
trait QemuExit {
    fn exit(code: u32) -> !;
}
```

但是实现应该明确是：

```text
x86_64
    QemuExitX86
    → isa-debug-exit

aarch64
    QemuExitArm
    → semihosting / arch-specific mechanism

riscv64
    QemuExitRiscv
    → sifive_test / semihosting / QEMU-specific device
```

而真正的：

```text
shutdown(0)
```

语义再放另一层。

这反而会让设计更漂亮：

```text
MinixShutdown
       |
       +--- HardwareShutdown
       |
       +--- QemuTestExit
```

不要让“QEMU 测试退出机制”污染真正的 shutdown 语义。

---

# 二、还有一个我很建议补上的东西：CPU 身份模型

你现在的 `CpuTopology`：

```text
{
    nr_cpus,
    bsp_id,
    cpus: [CpuInfo; 32]
}
```

然后：

```text
CpuInfo {
    hw_id,
    ...
}
```

但是后文又出现：

```text
logical CPU id
hardware CPU id
bsp_id
```

这几个概念现在很容易混。

例如你自己已经指出：

> ARM MPIDR / RISC-V hart ID 不保证和线性 CPU 编号相同。

那我会直接把结构改得非常明确：

```text
CpuTopology {
    cpu_count,
    bsp_logical_id,
    cpus: [CpuInfo; MAX_CPUS],
}

CpuInfo {
    logical_id,
    hw_id,
    ...
}
```

甚至：

```text
LogicalCpuId
HwCpuId
```

做成两个新类型。

因为现在最危险的 bug 就是这种：

```text
CPU #2
   ≠
APIC ID 2
   ≠
MPIDR 2
   ≠
hartid 2
```

你已经意识到了，最好直接在类型层堵掉。

---

# 三、现在缺了一张非常重要的“CPU 状态机”

目前文档比较像：

```text
not started
   ↓
boot_ap
   ↓
init_ap
   ↓
ready
   ↓
scheduler
```

但真正实施时，我建议明确成：

```text
DISCOVERED
    ↓
BOOT_REQUESTED
    ↓
EARLY_ENTRY
    ↓
ARCH_INIT_DONE
    ↓
KERNEL_READY
    ↓
ONLINE
```

失败路径：

```text
BOOT_REQUESTED
    ↓
BOOT_FAILED
```

至少定义：

```text
CPU_DISCOVERED
CPU_BOOTING
CPU_READY
CPU_ONLINE
CPU_FAILED
```

原因是你现在 §3.4 对 C 的描述实际上有两个不同概念：

```text
CPU_IS_READY
```

和

```text
ap_cpus_booted
```

C 本身就不是一个单一 flag。官方源码里也是先统计 `CPU_IS_READY`，然后释放 BKL，再等待 `ap_cpus_booted == n - 1`。([GitHub][6])

因此 Rust 最好不要把：

```text
ready
booted
online
```

全部压成一个 `AtomicBool`。

---

# 四、`ap_cpus_booted` 最好从“计数器”升级为“bitmask”

现在你测试：

```text
ap_cpus_booted == 3
```

问题是：

```text
CPU1 写两次
CPU2 写一次
CPU3 没写
```

理论上也可能得到 3。

虽然正常代码不会这么做，但 SMP bring-up 恰恰应该让错误尽可能可见。

我更建议：

```text
AtomicU32 online_mask
```

例如四核：

```text
BSP = bit 0

expected = 0b1111
online  = 0b1111
```

L3 直接：

```text
assert_eq!(online_mask.load(Acquire), expected_mask);
```

这样同时解决：

* CPU 数量
* CPU 身份
* 重复报到
* 缺少 CPU

而且天然适合你的 `MAX_CPUS = 32`。

测试也从：

```text
ap_cpus_booted == 3
```

提升到：

```text
AP online mask == expected AP mask
```

我认为这是非常值得改的小设计。

---

# 五、当前文档对“内存序”几乎没写，这对三架构 SMP 是一个明显缺口

这点我会特别提醒。

你现在有大量：

```text
AtomicU32
AtomicBool
ap_cpus_booted
mailbox
CPU ready
flags
ack
PLATFORM frozen
```

但 §3 没有一个统一的：

> SMP publication / visibility rule

而 x86 上很容易“测试啥都绿”，到了 ARM/RISC-V 才出现问题。

例如：

```text
BSP:

bootstrap_struct.foo = ...
bootstrap_struct.stack = ...
ready.store(true, Release)


AP:

if ready.load(Acquire) {
    use bootstrap_struct
}
```

或者：

```text
BSP:
    write PLATFORM
    freeze
    Release publication

AP:
    Acquire
    read PLATFORM
```

同样：

```text
request.store(...)
send_ipi()
```

和：

```text
handler:
    request.load(...)
    ...
    ack.store(..., Release)

sender:
    ack.load(Acquire)
```

这些至少应该在 §3 增加一个非常短的：

### 3.X SMP memory-order contract

规定：

```text
Bootstrap data publication: Release → Acquire
CPU online publication: Release → Acquire
IPI request: Release → interrupt → Acquire
IPI completion: Release → Acquire
immutable-after-freeze data: one-time publication + Acquire
```

这样你的四个 per-CPU 迁移项就不会各自重新决定一遍。

---

# 六、S-3 和 L2 之间有一点“小鸡生蛋”

现在：

```text
S-3 = AP 入口代码
S-4 = init_ap
```

但 L2 又规定：

> AP 完成模式梯子 → 执行 Rust fn → magic

问题是 S-3 的时候 `init_ap` 还是 panic。

也就是说需要明确：

```text
S-3:

trampoline
   ↓
ap_early_entry()
   ↓
write magic
   ↓
park/halt
```

而不是：

```text
trampoline
   ↓
init_ap()
```

到了 S-4 才变：

```text
trampoline
   ↓
ap_early_entry()
   ↓
init_ap()
   ↓
...
```

这其实是非常自然的：

```rust
fn ap_early_entry(ctx: *const ApBootstrap) -> !;
```

S-3 测它。

S-4 扩展它。

这样阶段边界就非常干净。

---

# 七、ARM/RISC-V 的“入口没有低地址问题”这句话应该收紧

你现在写：

> ARM/RISC-V 的入口可以就是内核镜像里的一段位置受限代码，无低内存问题。

“没有 `<1MiB` 约束”是对的。

但是还有一个共同约束：

> **PSCI / SBI 启动入口在 MMU 关闭状态下必须是物理可执行地址。**

尤其你的 kernel 是 higher-half。

所以：

```text
link VA:
    0xffff....

runtime PA:
    0x4008....

MMU-off AP entry
```

不能简单：

```text
boot_ap(entry = &ap_entry as usize)
```

然后把这个值直接交给 PSCI / SBI。

你的 §3.1 其实已经隐约表达了这一点，但最好把它提升成明确 invariant：

```text
ApEntry = PhysAddr
Bootstrap = PhysAddr
PageTableRoot = PhysAddr
KernelStackTop = PhysAddr
```

而 Rust 进入 MMU-on 后再转换：

```text
PhysAddr → higher-half VA
```

这会极大减少实现时犯错的机会。

---

# 八、RISC-V §3.3 里面的 PLIC 有点可疑

你写：

> `PLIC 的 per-hart context 使能（收 IPI 前置，可推迟）`

这个我建议**删掉或重新核实**。

PLIC 负责的是 external interrupts；software interrupt / IPI 是另一条路径。RISC-V 官方架构明确把 software interrupt 与 external interrupt 区分开，而 SBI IPI 也是软件中断机制。([RISC-V Docs][7])

所以如果你的 `send_sched_ipi()` 在 RISC-V 上走 SBI IPI，那么：

```text
PLIC enable
```

并不是：

```text
SMP IPI enable
```

你应该明确区分：

```text
timer interrupt
external interrupt / PLIC
software interrupt / SBI IPI
```

这个是另一个 **P1 factual correction**。

---

# 九、x86 init_ap 可能漏了“per-CPU local timer”

你现在 S-4 的重点是：

```text
GDT
TSS
GS_BASE
IDT
stack
```

但你的整个计划后面已经包含：

```text
tick per CPU
timer IRQ
scheduler
```

如果 x86 的 local APIC timer 是 per-CPU，那么 AP 初始化过程中应该有一条明确的：

```text
local timer initialization / calibration / enable
```

而不是只在 S-8 才第一次出现 timer。

这不一定意味着你现在的实现一定缺它——你现有 Rust 代码可能已经有基础设施——但**设计文档必须给它归属**：

```text
S-4:
    CPU-local interrupt/timer baseline

S-8:
    trap/IRQ entry wired

S-10:
    IPI functional

L6:
    timer functional
```

否则 S-4/S-8 实现的时候非常容易发现还有一半东西没地方放。

---

# 十、S-2 的 x86 测试不要写死 APIC ID `0..3`

你现在写：

```text
x86 APIC ID 0..3
```

对于当前 QEMU 默认拓扑大概率成立，但这不是应该成为 topology parser 的语义契约。

更稳妥的是：

```text
nr_cpus == 4
hw_id unique
BSP hw_id ∈ discovered hw_id
all hw_id correspond to MADT entries
```

只有测试 QEMU 默认 machine topology 时，才额外做：

```text
expected_apic_ids == {0,1,2,3}
```

否则你以后：

```text
-smp sockets=2,cores=2
```

甚至某个不同 CPU topology 后，测试自己先炸了。

这和你 §3.2 刚刚确定“逻辑 ID ≠ hardware ID”的原则也是一致的。

---

# 十一、S-0 的验证矩阵还可以再严一点

现在 S-0 写：

> hosted test + x86_64 UEFI + 7 个 x86 QEMU，ARM/RISC-V 可用者。

但这份计划明明把：

```text
三架构
```

写成核心目标。

我建议 baseline 就固定成：

```text
Architecture    Host tests    Production build    QEMU smoke
x86_64             ✓               ✓                 ✓
aarch64            ✓               ✓                 ✓
riscv64            ✓               ✓                 ✓
```

哪怕 S-0 时 ARM/RISC-V 只有：

```text
hello-boot
```

也应该有。

否则很可能变成：

```text
S-0 x86 全绿
S-1...
S-2...
S-3...
S-4...
S-5...
S-8...
```

最后才发现某个 common change 从来没有被 ARM/RISC-V 编译过。

你 §2.4 已经证明了生产路径编译是必须的，所以这里干脆制度化。

---

# 十二、S-7 “AP 持 BKL 进入主循环”建议换一种表述

这一句我会改：

> AP 在 `init_ap` 尾部持有 BKL 进入主循环。

因为读起来很像：

```text
acquire(BKL)
while true {
    scheduler()
}
```

那肯定不对。

而真正需要表达的是：

```text
AP enters scheduler with the same BKL ownership invariant
as the BSP's scheduler entry.
```

也就是：

> **AP 首次进入调度循环时满足与 BSP 相同的 BKL 前置条件；此后的获取、释放和让出行为完全复用现有调度路径。**

这样就不会把“初始持锁状态”和“整个 scheduler loop 永远持锁”混为一谈。

这个尤其值得改，因为你的项目本身正在强化 `BklGuard/BklSection` 的类型语义。

---

# 十三、`boot_lock` 最好补一张 lock-order 表

目前已经出现：

```text
boot_lock
BKL
其他 scheduler/IPI 状态锁
```

但没有明确：

```text
谁可以持有谁的时候获取谁
```

建议加一小节：

```text
Lock order / interrupt context rules

boot_lock
    ↓
BKL
    ↓
per-resource locks

禁止：
BKL → boot_lock

禁止：
AP trap entry → boot_lock
```

如果最终发现：

> `boot_lock` 在现有“只有 BSP 调用 smp_init”的模型下其实没有独立价值

那么完全可以在 review 时把它删掉。

因为 Rust rewrite 的目标不是：

```text
C 有什么锁
→ Rust 也机械复制一个锁
```

而是：

```text
C 的并发约束是什么
→ Rust 怎样最小化地表达这个约束
```

目前文档对这一点其实已经有很好的意识。

---

# 十四、你的执行顺序还可以做一个微调

现在：

```text
S-3 trampoline
S-4 init_ap
S-5 smp_init
S-6 per-CPU 四件套
S-7 AP scheduler
S-8 trap
```

总体合理。

但我会把实际设计前置成：

```text
S-2 topology

S-3a:
    AP bootstrap ABI + memory-order contract
    ↓
S-3b:
    x86 trampoline
    ↓
S-3c:
    ARM/RISC-V early stub
    ↓
S-3d:
    AP alive

S-4:
    arch init_ap
    ↓
S-5:
    smp_init / boot_lock / online mask
    ↓
S-6:
    CpuLocal
    ↓
S-7:
    scheduler
    ↓
S-8:
    trap
    ↓
S-10:
    real IPI
```

也就是**不要把 S-3 当成“一次写完全部三架构 AP 入口”**。

x86 是真正的高风险核心。

ARM/RISC-V 可以先把：

```text
firmware start
→ physical entry
→ minimal stub
→ AP Rust entry
```

跑通。

这样 x86 trampoline 坏的时候，不会连三架构整个 SMP 模型都卡住。

---

# 最后给一个整体评级

我会这样评：

| 部分                       | 评价                |
| ------------------------ | ----------------- |
| 整体架构                     | **A**             |
| C→Rust 语义对齐              | **A-**            |
| BKL / SMP bring-up 思路    | **A**             |
| 三架构抽象                    | **A-**            |
| 测试阶梯                     | **A**             |
| 文档自包含性                   | **A**             |
| x86 trampoline 方案        | **B，目前最大风险**      |
| CPU identity/state model | **B+，需要收紧**       |
| memory ordering          | **B，目前明显缺一层**     |
| trap/syscall 模型          | **B，需要拆开**        |
| QEMU shutdown            | **C+，三架构后端需要重设计** |

所以我的结论不是“这份设计有很多问题”。

恰恰相反：

> **这份设计的大方向已经收敛了；现在剩下的是几个非常尖锐、非常具体的底层坑。**

而且这些坑集中在 **S-3～S-4 这一小块**，不是整个 SMP 设计推倒重来。

---

## 我会优先修改的最小集合

在真正开始 S-3 之前，我只要求先把下面几件事钉死：

**① 修 RISC-V `hart_start` ABI。** ([RISC-V Docs][1])

**② 把 AP trampoline 定义成“flat binary blob”，明确 PIC / relocation-free / writable mailbox / section retention。**

**③ 明确 x86 `IDT entry` 与 `SYSCALL entry` 是两条不同硬件路径。**

**④ 把 `logical_cpu_id` / `hw_id` / `bsp_logical_id` 分离。**

**⑤ 定义 `CPU_BOOTING → READY → ONLINE` 状态以及统一的 Acquire/Release publication 规则。**

**⑥ 把 QEMU exit 改成 arch-specific backend，而不是“三架构共享 isa-debug-exit”。** ([GitHub][5])

**⑦ L3 从 `ap_cpus_booted == 3` 改成 `online_mask == expected_mask`。**

做完这七个，我会认为这份文档就已经很接近可以“冻结设计、开始编码”了。剩下诸如文风、章节组织、Linux/Redox 对比是否需要再多两段，我反而不会再折腾。

尤其是第 2 条，我认为值得在 S-3 开工前专门做一个 **10～30 行级别的 toolchain spike**：不要先实现完整 trampoline，而是先证明“当前 Rust toolchain + linker + UEFI 链路确实能生成一个可复制、无 relocation 的 `.ap_trampoline`”。这一刀如果过不了，立即换 `global_asm!` / 小 `.S`，不会把几十小时押在错误抽象上。Rust 当前确实支持 `.code16`，所以现在真正需要验证的是**可搬运 blob 的整个链路**，而不是“Rust 能不能写 16 位汇编”。([Rust 文档][4])

就这份文档本身而言，我会给 **“可以继续迭代，不需要推倒重写；S-0～S-2 可以直接执行，S-3 前先修上述 7 点”**。

[1]: https://docs.riscv.org/reference/sbi/ext-hsm.html?utm_source=chatgpt.com "9.1. Hart State Management Extension (EID #0x48534D \"HSM\") :: RISC-V Ratified Specifications Library"
[2]: https://doc.rust-lang.org/core/arch/macro.naked_asm.html?utm_source=chatgpt.com "naked_asm in core::arch - Rust"
[3]: https://doc.rust-lang.org/stable/reference/attributes/codegen.html?highlight=track_call&utm_source=chatgpt.com "Code generation - The Rust Reference"
[4]: https://doc.rust-lang.org/stable/reference/inline-assembly.html?highlight=assembly&utm_source=chatgpt.com "Inline assembly - The Rust Reference"
[5]: https://github.com/rust-embedded/qemu-exit "GitHub - rust-embedded/qemu-exit: Exit QEMU with user-defined code · GitHub"
[6]: https://github.com/Stichting-MINIX-Research-Foundation/minix/blob/master/minix/kernel/smp.c "minix/minix/kernel/smp.c at master · Stichting-MINIX-Research-Foundation/minix · GitHub"
[7]: https://docs.riscv.org/reference/aia/MSLevel.html?utm_source=chatgpt.com "4.1. Interrupts for Machine and Supervisor Levels :: RISC-V Ratified Specifications Library"


可以。**如果你的真正诉求是“不要再出现一个独立的 `trampoline.S` / 第二套汇编文件”，我完全赞成，而且我觉得这反而应该成为这份设计的明确原则。**

但这里要把两个概念分得非常清楚：

> **你可以不要 `trampoline.S`，但 x86 不能不要“AP 的早期启动代码”。**

因为这不是软件设计偏好，而是 INIT-SIPI-SIPI 的硬件启动契约。你文档自己已经把这一点写得很准确：x86 AP 被 INIT 复位后，从 `vector << 12` 的低 1 MiB 物理地址、16 位实模式开始执行。

所以我会把我上一条里的建议修正一下：

## 我不建议你再造一个“trampoline 抽象”

你现在已经把 BSP 原来的：

```text
trampoline.S
    ↓
HigherHalf
```

重构成：

```text
HigherHalf trait
    ├── x86_64
    ├── aarch64
    └── riscv64
```

这其实是一个很漂亮的设计。文档也已经明确说 `HigherHalf` 只服务 BSP，并且已经完成三架构实现。

**我不会建议你为了 AP 又复制一套这种“trampoline”概念。**

否则最后项目会变成：

```text
BSP:
    HigherHalf trait

AP:
    ApTrampoline trait / trampoline.S / trampoline blob
```

然后所有 boot 相关代码开始围着 “trampoline” 转。

这反而把模型搞复杂了。

---

# 更好的模型：`AP boot entry`，而不是 `trampoline`

我建议把概念直接改成：

```text
SmpArch::boot_ap(...)
        ↓
architecture-specific AP entry
        ↓
arch_init_ap(...)
        ↓
kernel::smp::ap_main(...)
```

也就是：

```text
                    BSP
                     |
                smp_init()
                     |
               SmpArch::boot_ap
             /         |         \
         x86_64      aarch64    riscv64
           |            |           |
      AP entry      AP entry    AP entry
           |            |           |
       16→32→64       MMU on      satp
           |            |           |
           +------------+-----------+
                        |
                   init_ap()
                        |
                   CpuLocal / BKL
                        |
                   AP main loop
```

这里**没有一个叫 trampoline 的抽象层**。

---

# 但 x86 的 AP entry 本身还是存在

这就是关键区别。

x86：

```text
SIPI
 ↓
physical 0xXXXXX
 ↓
16-bit AP entry
 ↓
protected mode
 ↓
PAE
 ↓
long mode
 ↓
Rust AP entry
```

ARM：

```text
PSCI CPU_ON
 ↓
physical AP entry
 ↓
Rust AP entry
```

RISC-V：

```text
SBI hart_start
 ↓
physical AP entry
 ↓
Rust AP entry
```

因此你完全可以把三者统一为：

```rust
trait SmpArch {
    unsafe fn boot_ap(...);
    ...
}
```

但 **AP 第一阶段是 architecture-private implementation detail**，根本不需要暴露成某种 `Trampoline` trait。

---

# 而且我现在觉得：你甚至没必要把它做成 `#[naked] fn`

这是我对上一条回答最想修正的地方。

你想避免 `trampoline.S`，我第一反应给了：

```rust
#[naked]
fn ap_trampoline()
```

但仔细看你的项目哲学之后，我反而觉得：

**不要强行把 AP entry 包装成 Rust function。**

因为它真的不是 Rust function。

它更像：

> **由 Rust 构建系统携带的一小段 architecture-specific boot image。**

所以最自然的是：

```text
os/arch/src/x86_64/smp.rs

    boot_ap()
    ...
    AP_START_CODE
```

内部使用：

```rust
global_asm!
```

或者必要时使用极小的 `asm!`/机器码定义。

### 这样你得到：

```text
没有 trampoline.S
没有 trampoline trait
没有 Rust function ABI 假象
没有第二套 HigherHalf 抽象
```

但又能满足：

```text
x86 硬件必须有低地址启动代码
```

这非常符合你当前的设计风格。

---

# 我会建议你把 S-3 改名

现在是：

> **S-3 AP 入口代码（x86 trampoline 为主件）**

我会直接改成：

> **S-3 AP early entry（x86_64 为主件）**

然后文档里只保留一句：

> x86 AP 在 INIT-SIPI-SIPI 后必须从低于 1 MiB 的物理地址开始执行，因此需要一个可搬运的 early-entry code image；本项目不使用独立 `trampoline.S`，也不建立新的 `Trampoline` 抽象。

这句话我觉得甚至可以成为这个设计的原则。

---

# 这样你原来的 HigherHalf 设计反而更干净

你现在其实已经有两个完全不同的阶段：

### BSP

```text
firmware
 ↓
BSP early boot
 ↓
paging enabled
 ↓
HigherHalf::jump()
 ↓
kmain()
```

### AP

```text
INIT-SIPI / PSCI / SBI
 ↓
AP early entry
 ↓
paging enabled
 ↓
init_ap()
 ↓
AP main loop
```

注意：

**只有“paging 开启之后进入正常 Rust 地址空间”这个共同点。**

但入口来源不同。

所以不要为了代码形式统一而把：

```text
HigherHalf
```

和：

```text
AP early entry
```

强行统一。

这也是你现在文档里那句：

> `HigherHalf` 与 AP 入口代码是两个不同的东西

我认为是对的。

---

# 甚至 S-3 可以进一步缩小

我现在会把它设计成：

```text
S-3
AP early entry

x86:
    low-memory code image
    16 → 32 → 64
    obtain bootstrap pointer
    enable paging
    jump to common AP Rust entry

ARM:
    MMU-off entry
    obtain context pointer
    install minimal CPU state
    enable MMU
    jump to common AP Rust entry

RISC-V:
    MMU-off entry
    a1 = opaque
    satp
    sfence.vma
    jump to common AP Rust entry
```

然后三架构汇合：

```rust
unsafe fn ap_early_entry(ctx: *const ApBootstrap) -> ! {
    arch_init_ap(ctx);
    kernel_ap_entry(ctx)
}
```

这里的 `kernel_ap_entry` 才是你真正应该共享的东西。

---

# 还有一个更重要的好处：你不用复制 C 的 `trampoline.S`

你的 rewrite 原则不是：

```text
C trampoline.S
        ↓
Rust trampoline.rs
```

而是：

```text
C hardware obligation
        ↓
Rust 最小必要表达
```

C 需要 `trampoline.S`，是因为它的实现方式决定了它需要一个汇编文件。

**你的 Rust 重写完全没有义务保留这个文件形式。**

你真正必须保留的是：

```text
INIT-SIPI
 ↓
<1MiB
 ↓
16-bit
 ↓
long mode
 ↓
Rust
```

而不是：

```text
trampoline.S
```

这两件事完全不是一回事。

---

# 所以我现在会明确支持你的偏好

如果你的底线是：

> **项目里不再出现 `trampoline.S`。**

我认为这个底线完全合理，而且**不需要牺牲 x86 SMP 的正确性**。

我甚至建议把它写进设计约束：

> **AP bring-up 不新增独立 trampoline.S 文件。x86 的 SIPI entry 作为 arch-local early-entry image，由 Rust crate 的现有构建链产生；ARM64/RISC-V64 采用同一概念的 arch-local early entry。三者在进入 `init_ap` 后汇合。**

这样你原先的：

```text
BSP trampoline.S
→ HigherHalf trait
```

和这次的：

```text
AP trampoline.S
→ AP early-entry image
```

形成非常一致的工程原则：

**删掉“trampoline 文件”，保留“硬件要求的那一小段机器码”。**

我反而觉得这比我上一条给你的“把 trampoline 做成 flat blob”作为重点更符合你整个 Minix-RS 的设计路线。你真正要避免的不是那几十字节汇编，而是**让一个本来只是启动阶段的硬件细节，演变成项目里的第二套 boot 抽象体系。**
