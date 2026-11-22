# D02 独立设计（第一遍）— 内核镜像：布局、摆放、映射与进入高半

> 角色：独立 Rust 设计者（P-DESIGN 第一遍）。只读输入：`01-stage-kernel/01-boot-shim-bootstrap.md` 第 1、2 章，`01-stage-kernel/02-higher-half-kernel.md` 第 1、2 章，以及这两章引用的 Minix3 C 源。未读本仓 Rust 实现、文档第 3 章以后、任何评审与设计快照。
> 本文只回答一件事：把 C 的“布局—摆放—映射—进入”语义翻译成 Rust 时，状态、身份、所有权、错误、并发、抽象边界与宿主测试各应该长什么样。

## 1. C 语义合同复述

### 1.1 四个时刻与它们的依赖

C 把同一份镜像经过四个时刻：链接、摆放、映射、进入（文档 02 §1.1）。四个时刻的产物是下一个时刻的输入，顺序不可交换：

1. **链接**决定两套地址。带 `AT(ADDR(sec) - _kern_offset)` 的输出段，虚拟地址在高一侧，装载地址被压回低一侧；不带 `AT()` 的 `.unpaged_*` 三段两套地址相同（`minix3/minix/kernel/arch/i386/kernel.lds:14-35`）。两套地址的差值对同一组段恒定，这就是 J1 的原文来源。
2. **摆放**由内核之外的代码执行：外部装载器读 ELF 程序头的 `p_paddr` 把段体写到低一侧；`unpaged_*` 对象之所以进低侧，是 `objcopy --prefix-symbols` 改名后由链接脚本的输入文件通配收走的（`minix3/minix/kernel/arch/i386/Makefile.inc` 的 `.for unpaged_obj in …` 块）。
3. **映射**由内核自己的低侧代码完成，次序固定为 `pg_clear` → `pg_identity` → `pg_mapkernel` → `pg_load` → `vm_enable_paging`（`minix3/minix/kernel/arch/i386/pre_init.c:217-238`）。`pg_identity` 与 `pg_mapkernel` 写同一张 `pagedir[]`，且索引集合重叠，所以恒等映射必须先于内核映射（`minix3/minix/kernel/arch/i386/pg_utils.c:162-202`）。
4. **进入**是换栈加跳转，成对且不返回：`mov $k_initial_stktop, %esp` 之后压终止槽、压 `kinfo` 指针、`call kmain`，末尾是 `hang: jmp hang`（`minix3/minix/kernel/arch/i386/head.S:79-91`）。

### 1.2 四条不变量是运行期义务，不是静态摆设

- **J1 平移一致**：同一组可加载段的 `vaddr − paddr` 相等。C 侧低侧组平移为 0、高侧组为 `_kern_offset`，两组各自一致而不是全表一致（`kernel.lds:4-6,22`）。破坏后的第一现场是段体写到错误物理地址，症状推迟到第一次取指。
- **J2 跨距与粒度**：镜像跨距必须能由目标架构的映射粒度整块覆盖。C 侧靠 4 MiB 基址对齐与大页（`pg_utils.c:191-192` 两条断言），Rust 生产镜像要求 2 MiB 收口、64 KiB 引导栈在末段（文档 02 §1.2 的 J2）。
- **J3 映射先于进入**：进入高半之前，取指、栈、数据三者都已映射。C 侧的证据是五步次序；把 `pg_identity` 排到 `pg_mapkernel` 之后会让内核条目被恒等条目覆盖，症状不是页错误而是取到非内核字节（文档 02 §2.4 的反向推演）。
- **J4 换栈与跳转成对且不返回**：换栈必须在任何压栈之前，跳转之后不得回到旧栈。C 侧 `call` 会压返回地址，所以设栈排在 `call` 之前；ARM32 用 `ldr sp` 加 `bx r2`，两个动作携带的都是绝对值（`minix3/minix/kernel/arch/earm/head.S:42-46`）。

### 1.3 交接面上的语义（与 D01 的缝）

- 内核接手的三份数据（可用内存清单、内核位置记录、交接结构）按值交接；接收侧必须持有私有副本，不能持有发送侧地址（文档 01 §1.1、I1）。C 侧的对应动作是 `get_parameters` 把整个 `mbi` 拷进 `kinfo`（`pre_init.c:108`），`kmain` 再把 `local_cbi` 拷进全局 `kinfo`（`minix3/minix/kernel/main.c:127`）。
- 内核自己的位置（虚拟基址、物理基址、跨距）在链接期就确定，不来自引导器（`pre_init.c:111-117` 取的是链接脚本符号的值）。
- bootstrap 区间是借来的：`kmain` 在 `system_init()` 之后把它交回空闲链（`main.c:294-301`）。因此 `kmain` 用的栈不能落在这段区间里；C 侧 `k_initial_stack` 在高半 `.bss`（`minix3/minix/kernel/arch/i386/mpx.S` 的 `k_initial_stack` 标签），满足这条要求。
- 校验先于消费：外部字节（ELF、命令行、内存图）在被建表或分配消费之前必须完成形状检查。C 侧靠 `assert` 与调用序，`assert` 失败就是停机（`pre_init.c:219`）。

### 1.4 哪些是语义，哪些只是 i386 的实现

必须保留的是：两套地址的存在与分组平移、摆放者读 `p_paddr`、映射先于进入、进入的成对性、栈的可访问性与非回收性、内核位置来自链接期、bootstrap 区间可归还。

不必保留的是：4 MiB 大页、`CR4.PSE/PGE` 的开关次序、`pagedir[1024]` 铺满 4 GiB、恒等映射置 `USER` 位、`PWT|PCD` 的整段规则、`objcopy` 双份对象。这些是 i386 与 32 位地址空间的承载手段，不是语义本身。

## 2. Rust 设计

### 2.1 总形状

一条单向流水线，四个名词、三个动词：

```
RawImage(bytes) --validate--> ImageLayout + [LoadSegment]
                                   |
                                   v
                            BootPlan(windows) --map--> BootstrapRoot --activate--> ActiveRoot
                                   |
                                   v
                              EnterTarget{entry, stack_top, arg} --enter--> !
```

- 名词：**ImageLayout**（一份镜像的地址事实）、**Window**（一段地址区间加权限）、**BootstrapRoot** / **ActiveRoot**（建好但未激活 / 已激活的根页表）、**EnterTarget**（入口地址、栈顶、启动参数）。
- 动词：**validate**（把外部字节变成受检事实）、**map**（按计划写根页表）、**enter**（换栈跳转，不返回）。

### 2.2 状态

状态只保留“下一步能做什么”的差异，用类型表达，不用布尔量：

- `RawImage` → `ImageLayout`：校验一次，之后只读。校验门是单一入口，失败返回 `Err`，不产生半成品。
- `BootstrapRoot<Unbuilt>` → `BootstrapRoot<Ready>` → `ActiveRoot`：`map` 只存在于 `Unbuilt` 上，`activate` 消费 `Ready` 返回 `ActiveRoot`。这样“先开分页再建表”在类型上不可写出，J3 从纪律变成编译期约束。
- 阶段之外的事实一律是派生值，不设可变状态：栈顶由 `ImageLayout` 派生，内核窗口由 `ImageLayout` 派生，恒等窗口由“引导足迹”派生（见 D7）。

不做全局可变状态。C 的 `pagedir[]`、`mapped_pde`、`pt`、`kernel_may_alloc` 都是文件级可变状态，Rust 侧没有对应物。

### 2.3 身份

- 地址分型：`PhysAddr` 与 `VirtAddr` 各自是不透明的 newtype，互相转换只有一个入口 `Translation`，它只能由 `ImageLayout::translation()` 或平台的恒等关系构造。C 侧 `vir2phys(void *addr)` 直接把指针当地址用（`pre_init.c:24`），这类隐式恒等在 Rust 侧不允许存在。
- 镜像的身份是 `ImageLayout`，字段固定为：`virt_base`、`phys_base`、`span`、`entry`、`stack_top`。其余量都由它派生，避免同一事实存两份。
- 根页表的身份是 `ActiveRoot`，它记录“已激活”这一事实并携带后续 Direct Map 追加映射所需的句柄；未激活的根不能交给任何消费者。

### 2.4 所有权

- **镜像字节**归摆放者所有。内核侧只拿到 `ImageLayout + [LoadSegment]` 这样的值，不持有字节切片。这与 D01 的“按值交接”同一条规则：跨边界的只有值。
- **内存账**在 D02 只被只读借用：用来判定“哪段物理内存是 RAM”（决定恒等窗口的缓存属性）与“内存上界”（决定恒等窗口的上界）。D02 不分配、不裁剪内存账；账的可变所有权属于 D01 与 VM。
- **根页表帧**由 `BootstrapRoot` 独占持有。帧的来源是镜像里预留的一段对齐存储（`&'static mut` 切出），不是堆。`activate` 把所有权移交给 `ActiveRoot`，此后只允许追加映射，不允许重建。
- **引导栈**归镜像所有：它是镜像布局的一部分，`stack_top` 只是指向它的派生值，不转移所有权。归还 bootstrap 区间的动作由 `kmain` 之后完成，所以栈必须落在内核窗口内、且在 bootstrap 区间之外。
- 内置可变性只出现在一个地方：建表期间同一段物理内存同时通过物理视图与（激活后的）虚拟视图可达。用一个只在 `BootstrapRoot` 内部可见的 `UnsafeCell` 包装加文档化纪律表达，`unsafe` 不扩散到流程层。

### 2.5 错误

- 外部字节引起的失败一律 `Result`：ELF 头、程序头表、段形、平移不一致、跨距不满足粒度、栈不在窗口内、根页表槽位耗尽。错误类型是 `BootError` 枚举，每个变体带最小现场（哪一段、哪个地址），不做 `InvalidInput` 这种归一化。
- 内部不变量被破坏（例如已校验的布局再次校验失败）才 `panic`。C 的 `assert` 在 Rust 侧只保留给这一类。
- 不引入 errno。errno 是系统调用与 IPC 表面的契约（`AGENTS.md` Key Constraints），引导路径没有这个表面；给 `BootError` 硬套 errno 只会制造假的兼容性。

### 2.6 并发

- 引导阶段是单核、关中断的。用线性 token 表达：`BootCpu` 在入口创建，`!Send`，所有建表与进入函数取 `&mut BootCpu`。SMP 起来之后没有任何 API 还接受这个 token，于是“引导期独占”不需要运行时检查。
- 建表期间的写不与其他 CPU 竞争，但可能与“同一物理页的另一个虚拟别名”竞争；这条别名规则写进 `BootstrapRoot` 的文档，不在类型系统里假装解决。
- 关中断是进入 Rust 之前由汇编建立的前提。若架构能廉价读到中断状态（x86 EFLAGS.IF、ARM DAIF、RISC-V sstatus.SIE），在 debug 构建里断言一次，作为 J4 前提的证据，而不是靠注释。

### 2.7 抽象边界

通用层拥有**次序**这一语义：恒等 → 内核 → 设备 → 激活 → 进入。架构层只承接**承载**：

```rust
trait BootMapping {
    fn map_identity(&mut self, range: PhysRange) -> Result<(), MapError>;
    fn map_kernel(&mut self, virt: VirtRange, phys: PhysAddr, perm: Perm) -> Result<(), MapError>;
    fn map_device(&mut self, range: PhysRange) -> Result<(), MapError>;
    fn activate(self) -> ActiveRoot;
}

trait BootEnter {
    unsafe fn enter(target: EnterTarget) -> !;
}
```

`Perm` 只有语义档位（只读/可写/可执行/设备），没有 PTE 位。`map_identity` 不接受权限参数，因为恒等窗口只服务引导足迹，不该有“用户可访问”这种档位。PSE、PGE、CR3、satp、TTBR 全部留在架构实现里。

### 2.8 宿主测试

能在宿主证的：

- 布局：J1 的分组平移一致、`translation(phys_base) == virt_base`、J2 的跨距与栈位置、入口落在窗口内、栈顶对齐。
- 解析：畸形输入表（截断、溢出、`filesz > memsz`、表项越界）逐个变体命中，任何输入不 panic。
- 建表计划：用记录型 `BootMapping` 假实现断言调用次序、断言内核窗口不带用户权限、断言恒等窗口覆盖引导足迹与根帧、断言 `activate` 最后且只一次。
- 进入契约：用 `MockEnter` 断言传给进入的栈顶与参数；对 `enter` 的 `!` 返回类型做编译期检查。

不能在宿主证的：真实的 CR3/satp/TTBR 写入、跳转后 PC 的落点、TLB 与缓存属性、`.bss` 清零。这些属于链接检查（`readelf` 类）与 QEMU 门，宿主测试只证模型，不证硬件。

## 3. 概念更少的最小方案

最小方案只保留四个名词与三个动词（§2.1），并做三处删减：

1. **删掉“低侧对象副本”这一整类概念**。C 需要 `unpaged_*.o` 双份，是因为 32 位可以在关分页状态下执行，而建表函数必须在关分页时可用。64 位（x86-64 长模式）分页不可关，ARM64/RISC-V 可以用物理视图执行，但都不需要“同一函数在镜像里存在两份”。替代物是一个极小的汇编 trampoline：它只做“记录物理基址—跳进 Rust”与最后“换栈跳转”，不调用任何需要重定位的 Rust 函数。
2. **删掉 `k_initial_stktop` 这类链接符号**。栈顶是派生值，来自 `ImageLayout::stack_top`，于是“换布局就换符号值”的耦合消失，测试载体也不必伪造符号。
3. **删掉“恒等映射铺满地址空间”**。恒等窗口只覆盖引导足迹（trampoline 代码、根帧、必要的固件交接区），设备窗口单独列出。C 的全铺是省事手段，不是语义要求（文档 02 §2.4 已指出这一点）。

因此最小方案的概念表是：镜像（布局加段表）、窗口（区间加权限）、根（建好/激活）、进入目标；动作是校验、映射、进入。没有第五个名词。

## 4. 重要决策与候选

### D1 链接形态：按“谁读 `p_paddr`”分，而不是按架构分

- 候选 A：生产镜像一律带 `AT()`（低地址 LMA），测试载体用单地址脚本；两者由同一份 Rust 代码经 `ImageLayout` 走同一路径。
- 候选 B：所有形态统一带 `AT()`，测试载体也写双地址。
- 候选 C：取消 `AT()`，由装载器自己算“虚拟基址减平移”。

选择 A。理由：`AT()` 的唯一作用是给读 `p_paddr` 的装载器一个低地址（文档 02 §3.1 的判据在 C 侧同样成立，见 `kernel.lds:25-35`）。测试载体没有外部装载器，写 `AT()` 只会多一套没有读者的地址；候选 C 把平移规则从链接期挪到每个装载器里，等于让每个装载器各自维护 J1，错一处就是静默错位。

### D2 谁解析 ELF：共享的零分配只读解析器

- 候选 A：独立 crate，返回 `PT_LOAD` 迭代器，错误是枚举，零堆分配。
- 候选 B：复用 C 的 `exec_elf.c` 逻辑移植。
- 候选 C：解析只留在 boot-shim，内核只收布局标量。

选择 A。理由：`p_paddr`、`p_filesz`、`p_memsz`、`p_flags` 有多个消费者（内核装载、后续用户 ELF 装载），同一份“段迭代 + 边界检查”语义只应有一处实现；零分配使它能同时用于固件分配期与内核早期。候选 B 会把 C 的错误处理风格带进来；候选 C 让内核无法校验自己收到的布局与镜像段表是否一致，I4 的“校验先于消费”就断了。

### D3 栈顶来源：派生值，不是链接符号，也不是 wire 标量

- 候选 A：镜像里预留引导栈区域，`stack_top` 由 `ImageLayout` 派生。
- 候选 B：照抄 C，声明外部链接符号并在汇编里取绝对值。
- 候选 C：由发送侧算好，作为 `KernelInfo` 标量传过来。

选择 A。理由：符号方案要求每种链接形态都定义同名符号，测试载体没有（J2 只约束生产镜像）；wire 标量则把“内核自己的布局事实”变成外部输入，违反 §1.3 的来源规则，还多一个可被篡改的字段。派生值只有一处真相，且可用宿主测试检查“栈在窗口内、在 bootstrap 区间外、顶部对齐”。

### D4 建根的表示与次序归属：通用层管次序，架构层管写表

- 候选 A：通用 `BootPlan` 依次调用 `BootMapping` 的四个方法，`activate` 消费根。
- 候选 B：每个架构各写一份完整建根流程（C 的形状）。
- 候选 C：通用层直接操作一张抽象的页表数组，架构只提供位定义。

选择 A。理由：J3 的次序是跨架构语义，必须只有一处；把次序复制到三份架构代码里，等于让三份代码各自可能把 `activate` 提前。候选 C 把 PTE 位抬进语义层，违反 `AGENTS.md` 的“不把 CR3/PTE 位暴露给 OS 语义层”。候选 B 是 C 的现状：`pg_utils.c` 与 `earm` 的同名函数各写一遍，次序靠人记。

### D5 进入：一个 `!` 返回的 trait 方法，契约写在类型上

- 候选 A：`unsafe fn enter(EnterTarget) -> !`，每架构一份汇编实现。
- 候选 B：每架构一个 `_start` 自己完成换栈与跳转，Rust 侧不设接口。
- 候选 C：在 Rust 里用 `global_asm!` 内联，入口函数直接调用。

选择 A。理由：J4 的三条要求（换栈在任何压栈之前、跳转不返回、栈已映射）需要一个能写文档、能被 mock、能被编译期检查的契约；`-> !` 把“不返回”写进类型，调用点不可能写“跳转之后再清理”。候选 B 让契约散落在三份汇编注释里；候选 C 与 A 等价但把汇编藏进流程层，降低可审性。

### D6 错误策略：外部字节 `Result`，内部不变量 `panic`

- 候选 A：分层如上。
- 候选 B：全部 `panic`（C 的 assert 风格）。
- 候选 C：全部 `Result`，连内部不变量也返回错误。

选择 A。理由：外部字节是攻击面也是移植面，必须可诊断、可测试；内部不变量失败意味着代码错误，继续运行只会把损坏带进建表。候选 B 让畸形镜像的唯一表现是停机且没有现场；候选 C 逼每个调用点处理“不可能发生”，噪声会掩盖真正的错误分支。

### D7 恒等窗口的范围：引导足迹，不是整个地址空间

- 候选 A：只映射 trampoline 所在页、根帧、以及进入高半前必须触碰的数据。
- 候选 B：照抄 C，铺满低 4 GiB。
- 候选 C：映射整个可用物理内存。

选择 A。理由：恒等映射存在的唯一理由是“分页开启后、跳到高半前，当前取指与压栈仍然可访问”。候选 B 把不存在地址也标成 present，越界访问不报页错误而是读到不确定内容（文档 02 §2.4 对 `pg_identity` 的分析）；候选 C 把设备与空洞一起当 RAM，缓存属性无从区分。最小窗口让“越界即页错误”这条性质第一次成立。

## 5. 故意不从 C 复制的东西

| 不复制 | C 的做法与位置 | 理由 |
|---|---|---|
| 双份对象与符号前缀 | `objcopy --prefix-symbols` 生成 `unpaged_*.o`（`Makefile.inc` 的 `.for` 块） | 64 位不需要“关分页执行同一函数”，一个 trampoline 足够；双份让调试符号、覆盖率与 panic 回溯都出现两个同名实体 |
| 全 4 GiB 恒等映射 | `pg_identity` 填满 1024 项并置 `USER`（`pg_utils.c:162-184`） | 省事手段而非语义；`USER` 位在 C 里靠“固件没开 SMEP/SMAP”才无害，Rust 不继承这种环境假设 |
| `vir2phys(void *)` 隐式恒等 | `pre_init.c:24` | 地址类型分型之后，恒等只能是显式构造，避免“指针恰好等于物理地址”成为全局默认 |
| `k_initial_stktop` 链接符号 | `head.S:84` 与 `mpx.S` 的标签 | 见 D3；符号把布局与进入焊死，且每种链接形态都要维护 |
| 全局 `kinfo` 加跨边界指针 | `pre_init` 返回 `&kinfo`，`kmain` 再拷（`pre_init.c:237`、`main.c:127`） | 交接面按值；接收侧持有私有存储，指针不出现在边界上（I1） |
| 外部字节上的 `assert` | `pre_init.c:219`、`pre_init.c:167-169` | 见 D6；畸形输入应可诊断，而不是停机 |
| `kernel_may_alloc` 全局开关 | `pre_init.c:33`、`main.c:140-142` | 用一个全局布尔表达“现在能不能分配”，在 SMP 与异步路径上都不可靠；Rust 侧用所有权与阶段类型表达 |
| 4 MiB 大页与 `PSE/PGE` 舞蹈 | `pg_utils.c:204-245` | 架构承载细节，进 `BootMapping` 实现，不进共享语义层 |
| `pg_map` 的顺序状态机 | `pg_utils.c:267-310` 的 `static mapped_pde`/`pt` | “只许顺序前进”的隐含契约由调用序保证；Rust 侧建根是计划加显式状态，不需要隐藏游标 |
| C 入口栈对齐的侥幸 | `head.S:68-77` 实际给出 `%esp ≡ 0 (mod 16)`，不是 SysV 要求的 `≡ 12`（文档 01 §2.2 第 4 条） | 这是缺陷不是设计；Rust 入口由汇编显式对齐 |

## 6. 不覆盖与残余风险

- `usermapped` 段在 C 里位于内核高半窗口内，引导映射只需把它当普通内核内存覆盖；用户态可见性由 VM 的页表初始化重建，不属于 D02。本文不为其设专门概念。
- 设备窗口的来源（平台描述符）归 `04-platform-discovery.md`；本文只规定“设备窗口必须作为独立窗口列出、且不能出现在恒等窗口里”。
- `ActiveRoot` 交给 Direct Map 之后的追加映射策略不在本文范围；本文只保证所有权转移点唯一。
- 宿主测试覆盖模型，不覆盖硬件；J1/J2 的静态检查与 J3/J4 的真机观测之间的分工，必须在实现章各自给出证据，不能互相顶替。
