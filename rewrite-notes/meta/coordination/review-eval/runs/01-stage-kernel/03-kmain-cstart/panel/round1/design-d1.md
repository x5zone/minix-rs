# D03 独立设计（第一遍）：kmain 与 cstart 的保护结构

> 独立性声明：本轮只读了文档第 1、2 章与其中引用的 Minix3 C 源（`minix3/`），没有读 `os/` 下的 Rust 实现、文档第 3、4 章、历史评审、设计快照与作者解释。下文的结论只对 C 语义负责；若与现有实现不同，以 C 语义与本设计给出的理由为准，不表示现有实现错误。

## 1. 单位边界

**在本单位内**：从 Rust 内核入口进入 `kmain` 起，到内核自己的保护结构生效、CPU 的异常入口、系统调用入口与用户到内核的入口栈全部就位为止。对应 C 的 `kmain` 前五步与 `cstart` 里的 `prot_init()`（`minix3/minix/kernel/main.c:115`、`minix3/minix/kernel/main.c:403`、`minix3/minix/kernel/arch/i386/protect.c:321`）。

**不在本单位内**：`init_clock`、`intr_init`、`arch_init` 的其余内容（时钟、中断控制器、串口、ACPI、APIC）、页表的多级结构、系统调用的分派、trap frame 的完整保存恢复、进程表、boot 进程装载、AP 启动编排。这些由后续篇承担；本单位只负责把它们的入口条件准备好。

## 2. C 语义合同

以下条文按“谁在什么前提上读到什么”复述，不按函数顺序复述。锚点只指 C 源。

**K1 启动信息的全局化先于保护结构。** `kmain` 收到的是未分页暂存副本的地址；`prot_init` 与 `env_get` 读的是全局 `kinfo`，因此暂存副本必须在 `cstart` 之前搬进最终全局（`minix3/minix/kernel/main.c:128`、`minix3/minix/kernel/main.c:129`）。boot image 表在启动循环之前拷入 `kinfo.boot_procs`（`minix3/minix/kernel/main.c:145`）；该副本的消费者是用户态 VM，第二次同步在启动循环之后（`minix3/minix/kernel/main.c:275`）。`kernel_may_alloc = 1`（`minix3/minix/kernel/main.c:142`）打开内核直接分配物理内存的闸门，必须早于任何内核直接分配，晚于 VM 启动的收尾再关闭。

**K2 保护结构初始化是一次单向事务。** “建表”（改内存）与“生效”（改 CPU 状态）分离；生效动作一旦执行不可回退。C 的顺序是：探测 CPU 能力、清零两张表并设置表指针、建 TSS 与 GDT 槽、`prot_load_selectors` 让表生效、重建页表、置 `prot_init_done`（`minix3/minix/kernel/arch/i386/protect.c:321`）。生效动作的内部顺序是 `lgdt`、填 IDT、`lidt`、`lldt`、`ltr`、重载 CS 与五个数据段寄存器（`minix3/minix/kernel/arch/i386/protect.c:298`）。

**K3 顺序不变量有硬件原因。** `ltr` 的选择子必须由新的 GDTR 解释，所以它在 `lgdt` 之后；TSS 描述符必须先于 `ltr` 存在；门必须先填好再 `lidt`；段寄存器必须在新 GDT 生效之后重载；`prot_init_done` 必须在全部生效动作之后才可见，AP 路径会断言它（`minix3/minix/kernel/arch/i386/arch_smp.c:87`）。

**K4 表布局是硬件与引导链的合同。** GDT 固定槽：0 空、1 内核代码、2 内核数据、3 用户代码、4 用户数据、5 LDT 占位、6 起每 CPU 一个 TSS 槽（`minix3/minix/kernel/arch/i386/include/archconst.h:14` 到 `:21`；`CONFIG_MAX_CPUS` 为 1 时共 7 槽）。IDT 固定 256 项（`minix3/minix/kernel/arch/i386/include/archconst.h:11`）。PIC 构建下 `prot_init` 完成时有 39 条存在门：16 条 IRQ、19 条异常、4 条软中断；DPL=3 的只有 breakpoint、overflow 与向量 32 到 35（`minix3/minix/kernel/arch/i386/protect.c:107`、`:127`）。门的选择子一律是内核代码段（`minix3/minix/kernel/arch/i386/protect.c:227`）。未被列出的向量保持非存在。

**K5 TSS 回答“从用户态进来用哪根栈”。** 每 CPU 一个 TSS；GDT 描述符的基址指向它，限长是结构体大小减一，类型是“可用的 386 TSS”（`minix3/minix/kernel/arch/i386/protect.c:154`）。TSS 有值的字段只有：`ss0` 等于内核数据段选择子，`sp0` 等于内核栈顶减去两个机器字，`iobase` 等于结构体大小（空 I/O 位图）；其余字段清零（`minix3/minix/kernel/arch/i386/protect.c:169` 到 `:177`）。`sp0` 之上两个保留字是内核私有的：切回用户态之前把当前进程指针写到 `sp0`（`minix3/minix/kernel/arch/i386/arch_system.c:495`），`tss_init` 把 CPU 号写到 `sp0 + 一个机器字`（`minix3/minix/kernel/arch/i386/protect.c:181`）。

**K6 TSS 描述符的身份稳定，TSS 内容可变。** `tss_init` 被调用两次：`prot_init` 用引导栈，`arch_init`/`smp_init` 用每 CPU 的运行时栈（`minix3/minix/kernel/arch/i386/arch_system.c:246`、`minix3/minix/kernel/arch/i386/arch_smp.c:267`）。第二次只改 `sp0` 与 CPU 号，不重建描述符、不再次 `ltr`。`ltr` 会把描述符类型从“可用”改成“忙”；同一描述符第二次 `ltr` 会触发 #GP，所以每个 CPU 的装载恰好一次。

**K7 系统调用入口由能力位选择，且是外部行为。** `prot_init` 探测 SYSENTER 与 SYSCALL 并写全局 `minix_feature_flags`（`minix3/minix/kernel/arch/i386/protect.c:321` 起）；`tss_init` 按位写 SYSENTER 的三个 MSR，或写 EFER.SCE 与 STAR（`minix3/minix/kernel/arch/i386/protect.c:184` 到 `:210`）。入口必须自己找到本 CPU 的内核栈：SYSENTER 的 ESP 取自 `sp0`；SYSCALL 的每 CPU 入口桩从 `k_percpu_stacks[cpu]` 取栈顶，再取栈顶第一个字得到当前进程（`minix3/minix/kernel/arch/i386/mpx.S:202` 到 `:220`）。能力字经 `minix_kerninfo` 暴露给用户态，并决定映射给进程的 `minix_ipcvecs` 集合（`minix3/minix/kernel/arch/i386/memory.c:886` 到 `:899`），因此不能当作内部实现细节删掉。

**K8 页表重建是保护初始化里的另一件事。** C 在 `prot_init` 末尾清空、重建并装载引导页表（`minix3/minix/kernel/arch/i386/protect.c:351` 到 `:359`），理由有两条：换成重定位后的引导页表，才能映射 VM 的 ELF 段、不再依赖重定位前的数据；保留恒等映射给 LAPIC 与显存。C 用两份 `pg_utils` 对象与两份 `vir2phys` 实现它，切 CR3 不可回退。

**K9 AP 与 BSP 共用同一段生效动作。** `prot_load_selectors` 被 BSP 与 AP 引导代码共用，差别只在 `ltr` 用哪个 CPU 的选择子（`minix3/minix/kernel/arch/i386/protect.c:298` 起；`minix3/minix/kernel/arch/i386/mpx.S:600` 起）。每个 AP 必须装自己的 TSS。

**K10 失败模型是停机，不是返回错误。** 保护结构生效之前没有可用的内核异常入口，`kmain` 进入时中断标志为 0；顺序错误、描述符错误、重复装载的结果是 #GP、缺页或三重故障。C 用断言与 panic，没有可恢复路径。`tss_init` 的返回值在所有调用点被丢弃，选择子由槽号现算。

**K11 “三张表”不是跨架构合同。** earm 没有 GDT、没有 TSS 描述符；异常入口是 `write_vbar`，`tss_init` 只维护 `sp0` 与 CPU 号（`minix3/minix/kernel/arch/earm/protect.c:33`、`:77`）。跨架构真正的合同是四份答案：当前特权级与段可访问性、异常与中断入口、系统调用入口、从用户态进来用哪根栈；x86 只是把前两份与第四份落在三张表里。

## 3. 设计原则

1. **语义对象与硬件载体分开。** 内核语义层看到的是“异常入口集合”“每 CPU 的入口栈”“系统调用入口”，不是 GDT、IDT、TSS。aarch64 没有 GDT 与 TSS，若接口以表为形状，三个架构会被迫长得像，而差异会渗进语义层。
2. **一次性事务用类型承载。** “保护结构已生效”是后续启动阶段的前提条件，应成为函数签名的一部分，而不是一个全局布尔量。
3. **建表是纯数据，生效才是特权动作。** 纯数据部分可以在宿主上测试；特权动作部分只保留最薄的汇编与寄存器写。
4. **不为 C 的构建技巧付利息。** 两份对象副本、两份 `vir2phys`、`objcopy` 改名、无读者的字段、返回后被丢弃的选择子，都是 32 位 C 构建方式的产物，不是外部行为。

## 4. Rust 设计

### 4.1 对外形状

内核语义层只暴露三个概念：输入规格、生效凭证、每 CPU 入口栈。

```rust
pub struct ProtectionSpec<'a> {
    pub vectors: &'a [VectorBinding],
    pub stacks: &'a [&'static EntryStack], // 下标即 CPU 号
}

pub struct VectorBinding {
    pub vector: u8,
    pub privilege: VectorPrivilege,
    pub handler: Handler,                  // unsafe extern "C" fn()
}

pub enum VectorPrivilege { Kernel, User }

pub trait ProtectionArch {
    type Active;
    fn install(spec: &ProtectionSpec<'_>) -> Self::Active;
    fn attach_cpu(active: &Self::Active, cpu: CpuId, stack: &'static EntryStack);
    fn replace_vectors(active: &Self::Active, vectors: &[VectorBinding]);
}
```

`install` 只在 BSP 上调用一次：建表、装载、返回凭证。`attach_cpu` 给后续 CPU 装它自己的 TSS（先检查描述符 busy 位），共享表已经生效。`replace_vectors` 对应 C 的 APIC 路径整体重建 IDT 再 `lidt`：新镜像在旁路建好，最后一次装载切换，任何时刻 CPU 看到的都是完整的一张表。

跨架构实现不共享机制：x86-64 在 arch crate 的静态存储里放 GDT、IDT、每 CPU TSS；aarch64 放向量表并写 `VBAR_EL1` 与 `TPIDR_EL1`；riscv64 放向量表并写 `stvec` 与每 CPU 的 `sscratch`。语义层不知道这些名字。

### 4.2 状态

状态用 typestate 表达，不用运行时枚举：

- `ProtectionSpec` 是纯输入，可复制，不含任何已生效状态。
- `ProtectionArch::Active` 是私有构造的凭证，不实现 `Clone`，代表“该 CPU 的异常入口、系统调用入口与入口栈已经由内核自己的结构回答”。
- 后续启动阶段（时钟、中断控制器、AP 启动、进程初始化）在签名上要求 `&Active`；这样“先保护结构、后其它初始化”的顺序不需要注释或断言来维持。
- `attach_cpu` 之后该 CPU 才进入 `Active` 描述的状态；BSP 在 `install` 内完成。
- 不保留 `prot_init_done` 全局整数。它的唯一作用是让 AP 路径知道 BSP 已完成；在 Rust 里，`start_aps(&Active)` 的签名已经给出这个前提，凭证的所有权就是可见性。

### 4.3 身份

- **表的身份是地址。** GDT、IDT、TSS、入口栈都放在 `static` 里，`install` 只写内容、不搬动对象；`&'static` 让“地址稳定”成为类型性质，而不是接口外的口头契约。
- **TSS 槽的身份是 CPU 号。** 每 CPU 一个槽，`attach_cpu` 只碰 `cpu` 对应的槽；描述符的基址与限长在 `install` 建好后不再改变。这样 K6 的“描述符身份稳定、内容可变”直接落在两个类型上：`TaskStateSlot` 表示描述符身份，`EntryStack` 表示可绑定的内容。
- **选择子是 arch 私有类型。** 语义层不出现 `u16` 选择子，也不出现槽号。x86-64 的槽布局由 arch 模块决定，唯一的外部约束是用户数据段必须紧挨在用户代码段之前一个槽，因为 `SYSRET` 用同一个基址加 8 得到 SS、加 16 得到 CS。
- **向量号是外部 ABI。** 32 到 35 是 libc 写死的软中断号（`minix3/minix/include/arch/i386/include/ipcconst.h:4`、`:5`），必须保留；异常向量号由架构规定。门的选择子不是外部行为，C 的用户态代码不读选择子（`grep` 只在内核的 `mpx.S`、`arch_system.c`、`protect.c` 命中），所以 64 位布局可以按 `SYSRET` 的需要重排。

### 4.4 所有权

- 内核层拥有 `ProtectionSpec` 的构造与 `Active` 凭证；arch 层拥有静态表存储、描述符编码与特权指令。
- 入口栈由内核层拥有（`static STACKS: [EntryStack; MAX_CPUS]`），arch 只拿到 `&'static EntryStack` 并把栈顶写进 TSS、`TPIDR_EL1` 或 `sscratch`。`Proc` 指针不进入 arch 层：`EntryStack` 头部用不透明机器字保存当前进程，由调度路径写，由入口代码读。
- IDT 的内容由内核层以 `VectorBinding` 切片提供；arch 负责渲染成门。策略（哪些向量、什么特权级）与机制（门位段）分属两层，宿主测试可以只测策略。
- `Active` 的所有者负责在启动编排里把它借给后续阶段；不提供全局可得的“当前保护状态”。

### 4.5 错误

- 规格错误在 `install` 之前用纯函数校验：向量重复、向量号越界、给异常向量配用户特权级、CPU 数超过静态数组、缺少入口栈。校验返回内部枚举 `SpecError`，不是 errno，也不暴露给用户态。
- 校验失败在启动边界直接 panic，消息带具体原因（例如 `protection: duplicate vector 14`）。保护结构生效之前没有调用者可以接住错误，返回 `Result` 只会制造一条永远走不到的恢复路径。
- 生效前的可诊断检查：表对齐、TSS 描述符是否已经是 busy、入口栈顶是否按 16 字节对齐。busy 检查把无上下文的 #GP 换成带 CPU 号的停机消息，这是对 C 行为的可观察改进，不改变成功路径。
- 不新造数字错误码。用户态可见的错误仍然只能用 Minix3 的 errno；本单位的失败不是系统调用失败。

### 4.6 并发

- `install` 在 BSP 上、AP 启动之前执行，天然单线程；`ProtectionSpec` 不需要 `Sync`。
- 共享表在 `install` 返回后只读；`replace_vectors` 在旁路构建完整新镜像再一次装载，避免“表里一半新一半旧”被 NMI 看到。调用契约写明：中断关闭、没有其它 CPU 在执行用户态；C 的对应调用点也在用户态启动之前。
- 每个 CPU 只写自己的 TSS 槽、自己的入口栈头、自己的 `TPIDR_EL1` 或 `sscratch`，没有跨 CPU 写。`attach_cpu` 的 busy 检查与 `ltr` 在同一 CPU 上完成。
- 入口栈头的 `current` 字段用原子机器字保存，写入发生在切回用户态之前且中断关闭；读方是同一 CPU 的入口代码，使用 relaxed 语义即可。

### 4.7 抽象边界

trait 的方法按“CPU 需要被回答的问题”命名，不按表命名：

- `install` 回答四件事：异常与中断跳到哪、哪些向量允许用户态主动触发、系统调用从哪里进、从用户态进来用哪根栈。
- `attach_cpu` 回答“这颗 CPU 的第四份答案是什么”，并把它装进硬件。
- `replace_vectors` 回答“异常入口集合改变时如何原子切换”。

x86-64 的 GDT 段描述符承载 DPL；aarch64 的 EL 与页表权限承载同一语义；riscv64 的 U/S 模式与页表权限同理。语义层不出现“段”这个词；`VectorPrivilege::User` 表示“用户态可以主动触发这个向量”，在 x86 上落成 DPL=3，在 ARM 与 RISC-V 上落成各自的门限规则。

### 4.8 kmain 前五步的去向

- BSS 自检：属于引导链合同，放到内核入口之前的引导层检查与测试，不放进保护结构。Rust 编译器假定静态存储已清零，重复实现一个 canary 不能改变这个假定，只会把引导方的缺陷伪装成内核缺陷。
- 启动信息全局化：内核入口接收 `&BootInfo`，在任何消费者之前复制进内核拥有的 `static`；`BootInfo` 类型不提供指向未分页暂存区的长期引用。这是 K1 的 Rust 表达。
- 板卡识别：平台发现篇。
- `kernel_may_alloc`：内存子系统的启动分配闸门，用一次性凭证表示；保护结构的表全是静态存储，不依赖这个闸门，但闸门仍须在进程初始化之前打开。
- boot image 表拷贝：随启动信息一起冻结，消费者是进程初始化与用户态 VM。

### 4.9 宿主测试

建表与校验不执行特权指令，可以在宿主上测：

1. 规格校验拒绝重复向量、越界向量、异常向量上的用户特权级。
2. 异常集合完整；向量 15 保持非存在；32 到 35 是用户态软中断门。
3. GDT 槽布局满足 `SYSCALL` 与 `SYSRET` 的选择子算术：内核数据段紧跟内核代码段；用户数据段紧跟用户代码段之前。
4. 每 CPU 一个 TSS 槽，槽号随 CPU 号线性增长。
5. x86-64 TSS 布局与架构定义一致：大小 104 字节，`rsp0` 偏移 4，I/O 位图基址偏移 102。
6. 入口栈头部偏移稳定：栈顶等于头部起始地址，`cpu` 与 `current` 的偏移由 `offset_of!` 断言，入口汇编使用同一组常量。
7. 门编码与 C 的位段一致：存在位、DPL、类型、偏移高低两半；用一个固定的期望字节序列对账。
8. 段描述符限长换算：`size = 0` 表示最大区段，`size = 0xFFFFFFFF` 得到页粒度限长。
9. busy TSS 预检查能识别类型位。
10. 软中断向量号与 libc ABI 对账：32、33、34、35。
11. 能力字对外声明 `MKF_I386_AMD_SYSCALL`，不再声明 SYSENTER。
12. aarch64 与 riscv64 的纯数据部分：向量表大小与对齐、入口地址的低位模式位、每 CPU 入口单元。

真机测试不属于本单位的宿主测试；它验证的是“装进去之后 CPU 真的按新表走”，放在三架构的 `test-protection` 载体里。

## 5. 概念更少的最小方案

如果只保留“能证明 C 合同成立”的最小概念集，可以砍到两个类型、一个 trait、两个方法：

```rust
pub struct ProtectionSpec<'a> { /* vectors + 每 CPU 栈，同上 */ }
pub struct Active(()); // arch 关联类型可退化为零大小凭证

pub trait ProtectionArch {
    type Active;
    fn install(spec: &ProtectionSpec<'_>) -> Self::Active;
    fn attach_cpu(active: &Self::Active, cpu: CpuId);
}
```

具体削减：

- **不暴露 `EntryStack` 的绑定操作。** 每 CPU 的运行时栈是静态对象，`install` 时已经知道全部地址，直接把最终栈顶写进所有 TSS 槽；BSP 实际执行仍在引导栈上，但 TSS 的 `sp0` 只在用户到内核转场时被硬件读取，而第一次转场发生在调度器启动之后。C 的“先引导栈、后运行时栈”两次 `tss_init` 是启动顺序的产物，不是外部行为。`attach_cpu` 只负责 `ltr` 与段寄存器重载。
- **不保留 LDT 槽与 `lldt`。** 没有读者，删掉不改变任何用户可见行为。
- **不保留能力探测与双系统调用路径。** x86-64 只走 SYSCALL；`minix_feature_flags` 作为常量对外声明 SYSCALL；32 到 35 的软中断门仍装，作为兼容回退。
- **不保留页表重建。** 引导页表由更早的阶段建好；本单位不碰 CR3。
- **不保留 `prot_init_done`。** `Active` 的所有权与 `start_aps(&Active)` 的签名已经表达顺序。
- **不硬编码 39 条门。** 异常集合与软中断集合由内核层给出；IRQ 门由中断层给出绑定，`install` 只负责“全部装好再切换”。
- **`replace_vectors` 暂不进最小接口。** 它属于中断控制器篇；届时给 trait 增加一个方法，不影响 `install` 与 `attach_cpu` 的合同。

这个最小方案的代价是：`install` 必须拿到所有 CPU 的栈地址（静态数组，成立）；如果未来栈改为动态分配，需要重新引入绑定操作。收益是状态数从“未建、已建未装、BSP 已装、每 CPU 已装、栈已换”压到“未装、已装”。

## 6. 关键决策：候选与拒绝理由

**D1 表存储：静态 vs 启动分配。**
候选 A 静态数组；候选 B 启动期分配。选 A。拒绝 B：保护结构生效之前就需要可用的表地址，分配器本身依赖内存记账与页表；而且表必须永久驻留、地址稳定，静态存储把这两个要求变成类型性质。C 同样用静态数组。

**D2 生命周期：typestate 凭证 vs 运行时枚举 vs 自由函数。**
候选 A 零大小 `Active` 凭证；候选 B 一个带 `state` 字段的结构体；候选 C C 风格的 `init()`/`load()` 自由函数。选 A。拒绝 B：每个后续调用者都要处理“还没装好”的分支，而这个分支在正确启动序列里不可达。拒绝 C：无法阻止重复装载与先使用后装载，C 自己用注释和断言补，Rust 可以用签名补。

**D3 trait 粒度：一个入口环境 trait vs 按表拆成多个 trait vs 每个机制一个 trait。**
候选 A 一个 `ProtectionArch`，含安装、附加 CPU、替换向量；候选 B 拆成“异常入口”与“特权转场”两个 trait；候选 C 每个表一个 trait（GDT/IDT/TSS）。选 A。拒绝 B：aarch64 的异常入口与系统调用入口是同一个 `VBAR_EL1`，拆成两个 trait 会让两个方法写同一个寄存器，顺序依赖重新变成注释。拒绝 C：表是 x86 的载体，不是跨架构概念。

**D4 GDT 槽布局：照抄 C 的 32 位编号 vs 按 64 位 `SYSRET` 重排 vs 保留 C 布局并改用 `IRETQ` 返回。**
候选 A 重排为用户数据段在前、用户代码段在后；候选 B 照抄 3=用户代码、4=用户数据；候选 C 不改布局，返回用户态用 `IRETQ`。选 A。拒绝 B：64 位 `SYSRET` 用 `STAR[63:48]` 加 8 得到 SS、加 16 得到 CS，用户数据段必须紧挨在用户代码段之前一个槽；照抄 32 位编号会让 `SYSRET` 落到错误的选择子。拒绝 C：既然系统调用入口按文档第 1 章的载体表走 `SYSCALL`/`SYSRET`，返回路径应与入口路径成对；`IRETQ` 会引入另一套返回路径与栈帧假设。选择子不是用户可见 ABI（C 用户态代码不引用 `USER_CS_SELECTOR`），重排不改变外部行为。

**D5 系统调用入口：单一 SYSCALL vs 保留 SYSENTER 与 SYSCALL 双路径 vs 只走 SYSENTER。**
候选 A x86-64 只走 SYSCALL，同时保留 32 到 35 的软中断门与对外 SYSCALL 能力位；候选 B 照抄 C 的能力探测与两条路径；候选 C 只走 SYSENTER。选 A。拒绝 B：双路径要两套入口桩、每 CPU 的入口地址展开、两套 MSR 初始化，而 64 位长模式里 SYSCALL 是标准路径；能力字仍然作为外部 ABI 保留，但取值固定。拒绝 C：SYSENTER 不承担 64 位返回语义，且用户态 `minix_ipcvecs` 的 SYSCALL 桩会失去对应入口。

**D6 入口栈绑定时机：C 的两阶段（引导栈后换运行时栈）vs 安装时直接绑定最终栈。**
候选 A 安装时把每 CPU 的运行时栈写进 TSS；候选 B 照抄两次 `tss_init`。选 A。拒绝 B：TSS 的 `sp0` 只在用户到内核转场时被硬件读取，第一次转场在调度器启动之后，而运行时栈是静态对象、地址在安装时已知；两阶段只增加一个中间状态。保留 C 的语义（每 CPU 一根入口栈、栈顶之上两个保留字），去掉的是启动顺序的偶然产物。

**D7 页表重建：留在保护初始化 vs 完全删除 vs 移到独立阶段。**
候选 A 从本单位删除；候选 B 照抄；候选 C 移到地址空间服务。选 A。拒绝 B：C 重建的理由是替换重定位前的引导页表并映射 VM 的 ELF 段；Rust 的引导页表由更早阶段按最终布局建好，再重建会产生两个 CR3 所有者与两份地址翻译实现。拒绝 C：把它包装成另一个阶段仍然是同一件事的第二次所有权。保留的要求是“当前页表能映射 VM 的 ELF 段与设备内存”，由地址空间篇验证。

**D8 LDT 占位：保留 vs 删除。**
候选 A 删除槽与 `lldt`；候选 B 保留。选 A。拒绝 B：C 自己把这条描述符叫 unusable LDT、把装载叫 bogus ldt；没有读者，也没有用户可见行为。若未来需要 LDT，再按需加入，而不是为一个不存在的读者保留一个槽。

**D9 IDT 变更模型：原地改门 vs 整表重建后一次装载 vs 不可变。**
候选 A 整表重建；候选 B 原地写单个门；候选 C 安装后不可变。选 A。拒绝 B：部分更新的表可能被 NMI 或机器检查看到，且需要额外的写锁纪律。拒绝 C：C 的 APIC 路径会整体重置并重装 IDT（`minix3/minix/kernel/arch/i386/apic.c:357`），合同要求允许变更。整表重建让“切换”只有一个观察点。

**D10 失败模型：边界返回 `Result` vs 直接停机 vs C 式断言散落。**
候选 A 纯校验返回内部枚举、安装边界 panic；候选 B 安装返回 `Result`；候选 C 每个步骤断言。选 A。拒绝 B：保护结构生效前没有可恢复的调用者，`Result` 的错误分支不可达却要每个调用者处理。拒绝 C：断言散落会把“规格错误”和“硬件拒绝”混在同一种消息里；先校验再停机可以给出具体原因。

**D11 每 CPU 身份与入口栈元数据：栈顶保留字 vs GS 基址加 `swapgs` vs 每 CPU 独立入口桩。**
候选 A 栈顶保留字（CPU 号与当前进程）；候选 B x86-64 的 GS 基址；候选 C 每 CPU 一段独立入口代码。选 A 作为本单位的合同，入口层可以在此之上选择实现。拒绝 B：`swapgs` 引入一个必须成对维护的隐藏状态，且 NMI 落在 `swapgs` 之前会看到用户 GS 基址；这属于入口篇的风险，不应由保护结构引入。拒绝 C：入口桩按 CPU 复制代码会随 CPU 数增长，且仍然需要每 CPU 的栈指针表。

## 7. 故意不从 C 复制的东西

| C 的东西 | 为什么不复制 | 合同如何继续成立 |
|---|---|---|
| `prot_init` 里的页表重建与两份 `vir2phys` | 理由（重定位前布局、VM 映射）在 Rust 引导链里没有对应物；会造成第二个 CR3 所有者 | 页表要求移到地址空间篇；本单位不碰 CR3 |
| SYSENTER 路径、运行时能力探测、双入口桩 | x86-64 长模式以 SYSCALL 为标准；双路径只为 32 位兼容存在 | `minix_feature_flags` 仍作为外部 ABI 声明 SYSCALL；32 到 35 软中断门保留 |
| LDT 占位槽与 `lldt` | C 自己标注为不可用、bogus；无读者 | 无用户可见行为；未来需要时再加 |
| `tss_init` 的返回值 | 五个调用点全部丢弃，选择子由槽号现算 | 槽号由 arch 私有类型管理，语义层不接触选择子 |
| TSS 里 `cs`/`ds`/`es`/`fs`/`gs` 等无读者字段 | 只在硬件任务切换时有意义，Minix3 不用硬件任务切换 | 清零保留字段，只维护 `sp0`/`ss0`/`ist`/`iobase` |
| `k_percpu_stacks` 与 `sp0` 两份相同值 | 同一事实存两处，更新时必须同步 | 入口栈对象是唯一事实来源，arch 只保存指向它的指针 |
| `prot_init_done` 整数标志 | 顺序可由凭证所有权表达；整数标志只是给 AP 的防御性检查 | `start_aps` 要求 `&Active`；不再需要运行时布尔量 |
| `bss_test` 静态 canary | 它测的是引导方是否清零 BSS，不是保护结构；Rust 编译器假定静态存储已清零 | 引导链测试覆盖；内核入口不重复实现 |
| `kmain` 的两份 `kinfo` 对象与 `objcopy` 改名 | 是 C 的未分页/用户映射两副本构建技巧，不是语义 | 启动信息只保留“暂存引用 → 内核拥有的最终副本”一次所有权转移 |
| 39 条门写成带 `NULL` 哨兵的运行期数组 | 哨兵与遍历是 C 的容器技巧；策略应是一张常量表 | `VectorBinding` 切片，无哨兵；校验保证完整性 |
| 32 位 GDT 的数值槽号（3=用户代码、4=用户数据） | 64 位 `SYSRET` 的选择子算术要求用户数据段紧挨用户代码段之前 | 选择子非用户可见 ABI；语义角色不变 |
| `pg_identity` 的 USER 位、PWT/PCD 细节 | 属于页表与缓存属性，不在保护结构 | 地址空间篇按 C 的映射拓扑验证 |

## 8. 未决与验证缺口

- 入口代码如何取得 CPU 号与当前进程：本设计只承诺栈顶保留字的布局与偏移；具体入口桩（每 CPU 桩或 GS 方案）由异常/系统调用篇决定，但不得改变 TSS 的 `sp0` 合同。
- IRQ 向量号与门的所有权：本设计只要求“中断层给出的绑定在 `install` 时全部装好”，不替中断控制器篇决定 0x50/0x70 还是别的编号。
- AP 的段寄存器重载与 `ltr` 的先后：`attach_cpu` 内部顺序沿用 C 的“GDTR 已就位、再 `ltr`”；AP 的引导代码是否已经在新 GDT 下运行，由 AP 启动篇验证。
- 真机判据（GDT/IDT 基址、`ltr` 后 busy 位、第一次用户转场的栈）不在宿主测试范围内，需要三架构载体与调试器命令，归测试要点篇。
