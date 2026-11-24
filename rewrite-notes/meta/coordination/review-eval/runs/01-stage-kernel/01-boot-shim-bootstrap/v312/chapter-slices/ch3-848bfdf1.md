## 3. Rust 设计决策

### 3.0 本章导读

#### 问题入口

三条 v1 腿跨镜像，两个直核腿同镜像，三个架构的固件接口又各不相同。如果每个组合各写一份交接协议，内核会在进入 `kmain` 之前就背上多套地址语义。本章要解决的问题是：怎样用一套按值交接的 `KernelInfo`、两张带来源记录的内存表和一条校验门链，把五条生产腿收敛到同一个接收方，同时不把硬件差异带进内核。

#### 前置概念

- 第 1 章的 I1–I4 与三类数据。
- 第 2 章的 C 侧交接：`kinfo` 全局、`get_parameters` 的拷贝、`add_memmap`/`cut_memmap` 的记账、`overlaps` 的区间检查。
- 第 2 章 §2.7 的 ARM32 对照：同语义、不同参数来源。

本章把一条完整接线的启动路径称为“一条腿”。五条生产腿 = 两条 UEFI 链（x86_64、aarch64）+ 一条 riscv64 U-Boot shim + 两个直核面（riscv64 `bootface`、aarch64 `bootface_a64`）；“v1”指跨镜像 arena 交接（三条腿），“直核”指同镜像直接调 `arch_boot`（两条腿）。后文出现的“三组”“六条链”“四条有门链的腿”是不同切面，换算表在 §3.6；读到这些词时以 §3.6 表为准。设计轴上还有三种交接形态：同镜像内调用、跨镜像跳转、直核自引导；当前生产腿只用后两类（跨镜像三条、直核两条），同镜像内调用是已退役的测试形态。说“三形态”指设计轴，说“五条生产腿”指当前接线，二者不是同一计数。本篇的“02 篇”指镜像篇（高半内核，`02-higher-half-kernel.md`），不是本文件的第 2 章。

### 3.1 协议换代：固件接口替换 Multiboot

本章只给设计契约与取舍；关键行与实现走读见第 4 章。

> `[ARCH: ...]` 是本项目的架构演进标注，表示该处不是逐字重写，而是有意的架构变化；三档术语（Rewrite / Refactor / Architectural Evolution）见 `AGENTS.md`。

C 的实现前提写在 2.1 与 2.2 里（这里的 GRUB 泛指"第三方引导器"这个角色，本仓这条路径上具体是 §2.1 点名的 boot2）：引导器把内核与模块都读进内存，把它们的地址填进一份结构体，再用两个寄存器把结构体地址交给内核的第一段代码。内核自己不做任何 IO——它连"文件"这个概念都不需要。

重写的目标是 x86-64、aarch64、riscv64 三个架构，其中后两个没有 Multiboot 的等价物，而 UEFI 的启动模型只提供一个可以做 IO 的环境（Boot Services）与两个内存分配接口（页分配 `AllocatePages`、池分配 `AllocatePool`），并不替谁加载内核。于是第一步的分工被重新切开：

| 步骤 | C（Multiboot 协议 + MINIX boot2） | Rust（UEFI 路径） | Rust（U-Boot shim 路径；`FW_PAYLOAD`（OpenSBI 把下一级镜像打包进固件本体的模式）直核形态不在本表，见 §3.1.1 的例外条款） |
|---|---|---|---|
| 识别可执行文件 | ELF 头（本仓 boot2）／前 8KB 的识别头（GRUB 一类 Multiboot 引导器）（2.1） | UEFI 的 PE/COFF 头（boot-shim 自身；被装载的 `kernel.elf` 仍是 ELF） | ELF 头 + U-Boot `bootelf` 约定 |
| 读内核与模块到内存 | 引导器代劳：`multiboot` / `load_mods` | boot-shim 自己读：`SimpleFileSystem` 协议 + `loader::KERNEL_PATH` / `MODULE_NAMES` | U-Boot 代劳读文件（`fatload`），boot-shim 按 `BootFileTable` 查地址 |
| 内存布局 | 引导器填 `mbi.mmap_*`；内核遍历 | boot-shim 自己取固件内存图，分成可用表与保留表（交付形状见 §3.2，分流规则见 §4.2） | 读 DTB `/memory` 节点的 `reg`，同样分成两张表 |
| 传递参数 | 寄存器 + `multiboot_info_t` | arena 基址（三条 v1 腿的入口寄存器见 §4.1）——arena 的定义与分区布局见 §3.2；`BootHandoffV1` 已序列化进 v1 arena（`KernelInfo` 的字段按协议分区写入，发送侧不构造进程内 `KernelInfo`），根页表物理地址在信封的运行期字段里 | 同 UEFI 的协议与形状，入口寄存器见 §4.1 |
| 退出固件服务 | 无此步骤（引导器早已离开） | 必须调 `ExitBootServices`：boot-shim 跑在 Boot Services 的**环境之内**，得在某个时刻主动声明"我不再需要你"；声明之后整套服务失效，本篇用到三类——读文件（`SimpleFileSystem`）、分配内存（页分配 `allocate_pages` 与池分配 `allocate_pool`，后者支撑 `Vec`/`Box`）、固件控制台输出（`uefi::println!`） | 无此步骤（U-Boot 已把控制权交出去） |

生产入口的同一约束（文件访问必须在 `ExitBootServices` 之前完成）由 §4.2 的九个动作表承担：读文件在动作 2、ExitBootServices（EBS）在动作 6。


`exit_boot_services`（**测试内核路径的辅助实现，不是生产入口**）的文档把这件事说成契约。生产入口在 §4.2 的配对循环里直接用 raw `ExitBootServices`，把内存图分类进两张表，**不丢弃分类结果**；测试 helper 返回的 `_mmap` 是 best-effort，可以丢弃，因为测试内核要的清单已由 `build_memmap()` 返回。`LOADER_DATA` 是 UEFI 内存类型标签，指“引导程序向固件分配器租下的内存”；该 helper 只在 `qemu-test-kernels` 构建里存在，不与 `build_memmaps` 的 MapKey 配对，生产路径用 `ebs_pair` 保证分类与 EBS 共用同一个 key。

被审视的另一路径与结论（三否一采）：

| 方案 | 结论与理由（可复核） |
|---|---|
| 保留 Multiboot，做 64 位扩展（Multiboot2） | 否决：两个新架构没有等价协议；`head.S` 那套"引导器填结构体 + 寄存器传参"在 aarch64/riscv64 上要各写一份，等于三份引导协议 |
| 各架构独立引导（x86 用 Multiboot、ARM 用 PSCI（ARM 的电源/启动服务接口）、RISC-V 用 SBI） | 否决：分叉的是架构名，不是语义——§2.7 的四步里只有第 1 步（接收引导参数）随引导器变，重写把它收进 §3.5 的固件抽象，其余三步共享同一套语义，不随硬件重新分叉 |
| 内核自带 stub（UEFI 形态：把 boot-shim 链接进内核镜像，内核自己读文件再自举） | 否决，理由是**依赖方向**：boot-shim 与内核并成一个二进制，内核就得认识固件类型、把 `uefi` 拉进自己的依赖表，与 §3.5「内核不依赖引导层」的单向边界冲突——屏蔽固件差异正是 boot-shim 的职责。riscv64 没有 Rust 支持的 UEFI 目标，但只挡住 UEFI 这一种实现；换成 opensbi 形态则改判为采纳（下一行） |
| 内核自带 stub（opensbi 形态：固件直接跳下一级镜像，引导面长在内核镜像里） | **采纳**：它就是 §3.6 的直核自引导形态（riscv64 真板 `FW_PAYLOAD`、QEMU `-kernel`；aarch64 `-kernel` 测试通道）。它不违反依赖方向——`kernel-image` 反向依赖的是 boot-shim 的**库函数**（库 crate 与可执行体 crate 的区分见 §3.5；取件表——`BootFileTable` 这个文件索引结构；memmap 源），不是引导器产物（下文三词分工：**取件表** = `BootFileTable`，**取件库** = boot-shim 导出的装载辅助函数，**取件车道** = 一次取件的来源通道：UEFI `SimpleFileSystem` 或 U-Boot 表），`minix-kernel` 与 `kernel-image` 都不依赖 `uefi`（§3.5 问题二）。定位是辅助启动路径，不作三架构主线 |

#### 3.1.1 接线状态（唯一权威表）

> **现状标注（本篇接线状态的唯一权威表；其余各节再提到接线状态，都指向这里）**：上面第一张表描述架构终态；第二张表是另一路径的裁决记录（三否一采）。当前各固件路径的接线状态如下。
>
> | 路径 | 接线状态 | 覆盖与理由 |
> |---|---|---|
> | x86_64 / aarch64 UEFI 产线 | 已接线：「boot-shim 建根 + v1 arena + 跳独立 `kernel.elf` 高半入口」 | 切换依据见 §3.2 的收口记录与 02 篇的 4.5 节 的实现现状 |
> | riscv64 U-Boot shim | 已接线：U-Boot 用 `bootelf` 装载 `riscv64-uboot-shim`，shim 建根后跳进 `kernel-uboot` 的高半入口 | 门脚本 `test-riscv64-uboot-shim.sh` 的六个串口路标（直到 `entering scheduler`）全部出现 |
> | riscv64 UEFI shim | **未接线**，原因是工具链事实而非工程欠账：Rust 编译器至今没有提供 riscv64 的 UEFI 目标（`*-unknown-uefi` 目标列表不含 riscv64） | 等编译器支持后再接 |
> | riscv64 直核自引导 | 辅助/嵌入式形态（非主线）；QEMU 的 `test-kernel-image-riscv64-bootface.sh` 走它 | 它是硬件规范支持的真实形态（理由见下段“例外条款的背景”），但该门只验证 DTB 表址与模块定位失败的错误路径（§5.4）；“启动成功”的主线证据是 riscv64 U-Boot shim 的六路标门（`run_all.sh` / CI 覆盖门），两者不能混读 |
> | aarch64 直核面 | 测试通道，无专用启动门脚本；由 `test-smp-aps-aarch64.sh` 用 `-kernel` 驱动 | 判据是 SMP 交付（`ap-arrived`/`ap-irq-ready`），不是启动路标门，见 §5.4。**形态由构建产物决定（设计裁决）**：该门用 `fw-aarch64-none` 编出的直核产物（入口只编 `bootface_a64` 自引导臂）；UEFI 盘面装的是 `fw-aarch64-uefi-recv` 产物（入口只编 v1 接收臂，`x0` 为零时诚实停机而非转自举） |
>
> 本表按路径列，不是腿计数：第一行覆盖两条 UEFI 腿，`riscv64 UEFI shim` 行是未接线路径。

> **两个 aarch64 产物（设计裁决）**：仓库现在产出两个 aarch64 `kernel-image` 变体——直核 `fw-aarch64-none`（`bootface_a64` 自引导，`-kernel` 门用）与盘面 `fw-aarch64-uefi-recv`（v1 接收臂，`xtask image --arch aarch64` 装盘用）。先前形态是「一个镜像靠 `x0 == 0` 在运行期判别」，设计裁决把它换成编译期分叉：`x0` 零值不是任何固件承诺（QEMU 观察到不设参数，属实测而非规范），一旦某个加载器在 `x0` 放非零/零值就会把形态判反，而症状是 `handoff v1 envelope invalid` panic——指向交接协议而非入口形态。

> **例外条款的背景**：RISC-V 的固件形态规范（OpenSBI 的 `FW_PAYLOAD` 模式）允许把下一级镜像打包进固件本体，boot 时固件直接跳进 S-mode 目标——真板上这类极简装机（省掉 U-Boot 与文件系统）不需要引导器存在，QEMU 的 `-kernel` 模拟的正是这个时序。所以"引导面长在内核镜像里"是硬件规范支持的真实形态。§3.1 分工表的第三列描述的是 U-Boot shim 路径（`opensbi_helpers::prepare_boot`，由 `os/boot-shim/src/uboot_main.rs:fn uboot_main` 调用）；各路径的接线状态与"未接线的 riscv64 UEFI-shim 路径"指哪一条，见上面的现状标注表，这里不重复。
### 3.2 按值交接：`KernelInfo` wire 与字段台账（I1）

C 里 `kinfo` 是一个全局变量。`pre_init.c` 开头就定义 `kinfo_t kinfo;`，`get_parameters(ebx, &kinfo)` 往里填，`pg_utils.c` 里的记账函数通过 `cbi` 形参收到它的地址，`pre_init` 返回 `&kinfo`，`kmain` 再把这个指针传下去。整条链上传递的是**地址**，而地址之所以有效，是因为所有代码都在同一个地址空间里、链接成同一个镜像。

Rust 侧把这份共享状态改为一段**显式交接**，分两个维度说。

本节的交付形状与另外四节分工如下：C 侧"清单被截到 16 条"在 §2.4.2；扣减记录与 bump 整段保留在 §4.6；接收方的六个落地区加一个名字池在 §4.5；换手给 VM 的时序在 §4.6。本节只讲"交接的是什么形状"。

**维度一：通道形态**（交接怎么送到接收方）随交接形态变；三种交接形态（同镜像内调用 / 跨镜像跳转 / 直核自引导）的定义见 §3.0，这里先用。x86_64 与 aarch64 的 UEFI 链都走跨镜像跳转：boot-shim 建好自举根、开分页，把一块八页 arena（定义与分区见本节后半）的物理地址递给独立 `kernel.elf` 的高半入口（入口寄存器见 §4.1）；接收方从 arena 解码六个分区、落进自己的 `.bss`，再走只增量的高半 resume。riscv64 的 U-Boot shim 走第三条跨镜像 v1 腿（同样只递 arena 基址，入口寄存器见 §4.1）；riscv64 的直核自引导没有独立 shim，引导面在同一镜像内直接调 `arch_boot`；这是硬件形态决定的辅助启动路径（§3.5 问题二），与两条 UEFI 产线不同源。

**维度二：载荷内容**（送出的是什么）三形态一致：必须按值深拷进接收方的私有存储，不能靠指针跨边界存活。理由：跨镜像档里两个分别编译的二进制各有自己的 `.bss`，指针在接收方指向完全无关的内存，连 `&'static KernelInfo` 也只保证生命周期、不保证页在对方页表里被映射；直核面虽然没有跨镜像指针，但同一套落地区把三形态收成一条路径（少一套特例），也让"落盘之后再消费"成为统一次序（v1 腿是解码落盘，两个直核腿是借用后落盘）。三形态一致的是"必须深拷进私有存储"这一条要求；按落盘方式分两族：v1 三条腿的切片由固定分区与 `byte_len` 描述符重建，接收方按分区偏移与元素类型解码、不再解释裸 `(ptr, len)`；两个直核面的切片由发送侧 `BootReport<'_>` 借用给出，接收侧落 `.bss` 后再构造 `KernelInfo`，没有 wire 解码步骤（§4.4）。

> **收口记录**：x86_64 与 aarch64 的 UEFI 产线采用独立镜像交接（§3.6），本节与此一致——记录保留是为了给"为什么 §3.2 的通道描述里没有低地址内联窗口"一个可考的出处。riscv64 各路径的接线状态见 §3.1.1 的接线状态表，这里不重复；本段要留的独特信息只有一条：这次切换不改变 §3.2 讲的 KernelInfo 形状与快照口径。

#### 3.2.1 `KernelInfo` 结构与五条腿的发送视图


> [参考] 现役字段速查在 §3.2.1；下面这段 `16 → 13 → 14 → 10` 是设计变迁账，只用于解释字段数。

`kernel_info.rs` 的 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 共 10 个字段。字段计数的完整链条是 16 → 13（去掉 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个，见 §3.4.1 与 §3.4.3）→ 14（加入 `authority`）；wire v10（设计裁决）再删掉 `startup_objects`（当时既是 `KernelInfo` 字段、也是 wire 分区与 builder 参数）、`user_sp`、`kern_stack_top`、`syscall_entry` 四个，落到 10。读表前先分开两套数字：`删除` 行里的 22 是 C `kinfo_t` 的累计退场数，不是本结构的字段数；本结构的字段数就是上面的 16 → 13 → 14 → 10（现役 10），明细见 §3.2.2.1。按与 C 的关系分成下面几类：

读表前先立三个局部术语：`动作 1+2 组合`（读 ELF/预订 → 分配 root/bump → 拷贝段，完整定义见 §4.2.1）；`ReservedMapped` = 固件声明为保留、但运行期可经 Direct Map 访问的档；`Side` = 既不进 free list 也不进保留表的侧桶（完整判定见 §4.2.2）。

| 关系 | 字段 | 说明 |
|---|---|---|
| 保留（同名或近义） | `memmap` | C 的 `multiboot_memory_map_t memmap[MAXMEMMAP]` 变成 `&'static [MemoryRegion]`，长度自带；镜像/模块是否在表内取决于腿（直核与 U-Boot 在表内，UEFI 腿预订为 `LOADER_DATA`，见下）。 |
| 改型（C 有、形状变） | `param_buf`：`char[MULTIBOOT_PARAM_BUF_SIZE]` → `&'static [u8]`，上限仍按 1024 校验<br>`module_list[]` → `boot_modules: &'static [BootModule]` | |
| 改名 | `vir_kern_start` → `kern_virt_base` | （`freepde_start` 曾改名 `free_upper_idx`，字段删除时整体退场，见 §3.4） |
| 升为字段 | `kern_phys_base`<br>`kern_size` | C 里这两个值不是 `kinfo_t` 的字段，而是链接脚本导出的符号 `_kern_phys_base`、`_kern_size`（`_kern_phys_base = 0x00400000`、`_kern_size` 由链接布局算出；完整摘录见 §3.4）。Rust 把它们放进交接结构，三条 v1 腿的来源改为 boot-shim 解析内核 ELF 的 PT_LOAD 段（§4.4 的字段表）；两个直核面用常量（例外见下段）。`kern_size` 用 `u64` 而不是 `usize`——wire 语义（wire = 跨镜像交接协议里按固定字节宽度写进 arena 的那份表示，与进程内结构相对）需要固定宽度，不随目标指针宽度变（32 位目标是 C 侧的，本仓重写目标全为 64 位，这里钉的是交接协议而非当前目标集） |
| 删除 | 22 个字段：<br>`mbi`、`mem_high_phys`、`mods_with_kernel`、`kern_mod`、`kmessages`<br>`do_serial_debug`、`serial_debug_baud`、`minix_panicing`、`user_end`、`boot_procs[]`<br>`nr_procs`、`nr_tasks`、`release[]`、`version[]`、`vm_allocated_bytes`<br>`kernel_allocated_bytes`、`kernel_allocated_bytes_dynamic`、`mmap_size`、`freepde_start`<br>`bootstrap_start`、`bootstrap_len`、`user_sp` | 逐个的去向见 §3.2 的字段去向表（`mmap_size` 由切片长度承载；`freepde_start`/`bootstrap_*` 的删除判据见 §3.4.1 与 §3.4.3） |
| 新增 | `reserved_regions`<br>`source_records`<br>`platform_sources`<br>`authority` | 各自来源见下；`source_records` 是 v1 source 记录，供启动期 `validate_requiring_sources`（strict 门）审计，`KernelInfo::validate`（非 strict）在 `source_records` 非空时也查“来源记录覆盖投影内存图”；v10 起 strict 门改为检查内核物理跨度落在某条 `memmap` 或 `reserved_regions` 区间内（原来靠 `startup_objects` 的一条记录）。`authority` 是这份内存图的输入权威，见下表后的绑定说明 |


`memmap` 的容量口径：Rust 落地区 128 槽（`MEMMAP_REGION_STORE_LEN`），VM 交接消费上限 64 条（`VM_BOOT_HANDOFF_MAX_REGIONS`），`VmBootHandoff.deducted` 另有 47 条；C 的 `MIN(MAXMEMMAP, NR_MEMS)=16` 条静默截断不再继承。完整数字与溢出行为见 §4.5 的“落地区与交接容量”表。
> **v10 删掉的四个成员（设计裁决）**：`user_sp`（每腿都填 `USER_STACK_TOP`，接收侧改为直接读该常量）、`kern_stack_top`（每腿都填 `kern_virt_base + kern_size`，改为 `KernelInfo::kern_stack_top()` 访问器在消费点派生）、`syscall_entry`（生产侧填 `kern_virt_base`、仓内零读者，真值来自 `syscall_entry_va()`），以及 `startup_objects` 分区（唯一读者是 strict 门的一条覆盖断言，现由内存图两表回答）。wire 信封 160 → 128 字节（四个成员在信封里各占 8 B：`startup_objects` 只占 `startup_byte_len` 一个槽，加上 `user_sp`/`kern_stack_top`/`syscall_entry` 三个 u64，共 32 B），报文末尾的 startup 分区（20 × 72 = 1440 B）退场，`WIRE_STARTUP_END`（30624）随之消失，八页 arena 的设计余量从 2144 B 变成 3584 B。`BOOT_HANDOFF_V1_VERSION` 9 → 10，收发两侧精确比较，v9 与 v10 互相拒收。

**权威与内存图绑定**。`authority` 记录这份内存图是从哪个输入对象投影出来的：`RamAuthority::DTB`（设备树）或 `RamAuthority::UEFI`（`GetMemoryMap` 快照）；`RamAuthority` 内部包一个 `InputObjectId`；`InputObjectId` 现在只有 `DtbBootInput` 与 `UefiMemoryMap` 两个变体（分别对应 `RamAuthority::DTB`/`UEFI`），`StartupObjects` 与 `startup_record()` 已随 v10 删除，wire 标签 2（输入对象）、6（kind）与 5（origin）只作保留位（这些编号是 wire 分区内的标签位，不是同名的枚举值）。

它由构造点写入——wire 解码把 [`BootHandoffV1`](os/libs/minix-boot/src/handoff_v1.rs) 信封里的 `ram_authority` 复制进来：两个直核面在 `arch_boot` 调用点传 `RamAuthority::DTB`，两条 UEFI 腿发送侧在 `Serializer::new` 里声明 `InputObjectId::UefiMemoryMap`、接收侧由 `decode_envelope` 解码为 `RamAuthority::UEFI`；riscv64 U-Boot shim 发送侧声明 `InputObjectId::DtbBootInput`、接收侧解码为 `RamAuthority::DTB`（它从 DTB `/memory` 取表，没有 EBS 快照）。

`KernelInfo::validate_requiring_sources()` 读结构体字段；`BootReport::validate_requiring_sources(authority)` 由调用者传入权威。如果权威仍是调用方的另一个实参，`KernelInfo` 与权威在类型上可以任意配对；混合记录表里"DTB 记录 + 声明 UEFI 权威"这类错配只会在下一次覆盖审计时才暴露。把权威放进结构体没有让错误的构造变得不可能（构造点仍可传错），但它把"图和它的声明"变成同一个值，解码路径不再有两个可以接反的参数。

**先讲清前两个字段装的是什么：内存图交付成两张表**。`KernelInfo` 携带内存信息只用两个列表，形状固定，来源不固定：

| 列表 | 语义 | 内核侧怎么用 |
|---|---|---|
| `memmap` | 可用内存；物理内存管理器从这里分配帧，但列表**不保证已扣除全部占用区**。内核镜像在**两个直核自引导腿（riscv64 `bootface`、aarch64 `bootface_a64`）**与 U-Boot shim 腿的表内；两条 UEFI 腿在动作 2 的读/预订步（读完 ELF、预订 LMA 跨度，见 §4.2.1）用 `AllocateAddress` 把整段 `[kern_phys_base, +kern_size)` 预订成 `LOADER_DATA`，因此**不在 `memmap`（自由表）内**，而是分类落 `ReservedMapped` 时进入 `reserved_regions`；无 `WRITE_BACK` 时落 `Side`，不进这份表（§4.2.2）。模块同理：**DTB 来源腿**（两个直核面与 U-Boot shim）的 bump 页在表内，UEFI 腿是 `LOADER_DATA` 因而不在。分配前仍须扣掉占用区（§4.6 的扣除记录；`vm_handoff::classify`，UEFI 腿的内核行退化为空操作）。LMA（装载地址，load address）与 VMA（执行地址，virtual address）的完整推演见 §3.4，`LOADER_DATA` 的定义见 §3.1，`conventional` 与 `WRITE_BACK` 的完整判定见 §4.2.2 的令牌表 | 内核不保留可变 memmap 副本（早期引导段已移除）；`vm_handoff::classify` 在交接时对 memmap 做扣除，产出 free list 与 deduction record |
| `reserved_regions` | 可用但不可分配；运行期可经 Direct Map 访问，但不得再分配出去。 | Direct Map 覆盖的候选之一（[`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4） |

三个词先定义：`conventional` 是固件声明为可自由分配的普通 RAM；`WRITE_BACK` 是描述符的写回缓存属性，缺它表示这段没有普通可缓存语义；`MapKey` 是 `GetMemoryMap` 的快照标识，分类必须与 `ExitBootServices` 共用同一个 key。

五条生产腿产出这两张表的方式不同（按来源分三组：UEFI 组两条、U-Boot 组一条、直核组两条；按落盘方式分两族：v1 三腿解码落盘、直核两腿借用落盘；§3.6 把“产线”定义为链的别称，这里的“组”不是独立计数），形状相同：

- **两条 UEFI 链**：在 `ExitBootServices` 的配对循环里读 `GetMemoryMap`，按固件的类型标签分流（分流规则见 §4.2）。分类必须与退出共用同一个 MapKey，所以它只能发生在那一个循环里（`MapKey` 的配对理由与重试语义见 §4.2）。
- **riscv64 的 U-Boot shim**：读 DTB `/memory` 的 `reg`（`DtbMemoryMap`，[`memmap_source.rs`](os/libs/minix-boot/src/memmap_source.rs)），再用固件常驻区上界 `KERN_PHYS_BASE` 削出保留段（OpenSBI 占 `[0x8000_0000, 0x8020_0000)`）。
- **两个直核面**：同一份 DTB 解析实现；aarch64 传 `fw_end = 0`，因为该形态的 DRAM 里没有固件常驻区——`fw_end=0` 只关闭"固件前缀裁剪"，DTB `/reserved-memory` 声明的区间仍进保留表（该形态典型为空，但机制上不保证）。

收敛点是**数据形状加统一的发送视图**（五条腿都构造 `BootReport<'_>`；两个直核腿直接借用，三条 v1 腿把它序列化进 wire），不是行为多态；这里的“统一”是形状收敛，不保证发送侧视图完整——UEFI 腿的 `source_records` 切片为空，source 分区的权威在 wire（§3.7）。两个直核腿把两张表放进 `BootReport` 的同名字段，三条 v1 发送腿（两条 UEFI 链 + U-Boot shim）把同两张表写进各自的 `BootStorage`，报告视图是 `KernelInfo` 去掉 `authority` 的九字段视图（权威由腿在调用点提供）；接收侧统一经 `land_and_store` 落 `.bss` 后构造唯一 `KernelInfo`（其切片字段为 `&'static [T]`）。消费者只认区间、不认来源。这里没有一条"内存图来源"的 trait 承担统一，原因不是懒：UEFI 那条腿的分类时机被固件协议绑住——MapKey 与 `ExitBootServices` 一一配对，缓冲区不足与 MapKey 失效还要重试——一个只返回两张表的签名携带不了这些约束。

**内存图生产的四层责任**：上面三组内存图来源各自产出同一形状的两张表，但"从固件原始字节到两张表"的路径要跨四层，每层有独立的责任边界，不得混谈：

1. **UEFI 固件事务层**：`GetMemoryMap` + `MapKey` + `ExitBootServices` 是一个原子事务——分类必须与退出共用同一个 MapKey，否则固件拒绝退出。这一层决定了"分类只能在配对循环里做"（UEFI 腿），不能用一个独立函数事后补救。
2. **描述符解析层**：固件给的是原始字节，要变成带类型的描述符（类型、物理起点、页数、属性），需要统一的解析器负责字段边界、对齐与溢出校验。UEFI 腿用自建解析器；DTB 腿用设备树 `/memory` 节点解析。这一层的产出是"类型化的原始描述符"，还没做任何政策判断。
3. **OS 内存语义分类层**：政策侧把描述符按类型与属性分成 Available（可分配）、ReservedMapped（保留但可访问）、Excluded（不可触碰）、Special（延迟处理）四档；这是设计专项的分类（详见 [`01-design-note-memmap-consolidation.md`](01-design-note-memmap-consolidation.md) 的 v12 分类，文件位于 `rewrite-notes/01-stage-kernel/`）。**现行实现把 Special 展开成两档落点**：MMIO 类走 `DevicePath`，其余按属性与类型表走 `Side`（侧桶：既不进 free list 也不进保留表）。因此实现侧的完整落点是五类——可分配 / `ReservedMapped` / `Excluded` / `DevicePath` / `Side`，判定是五步（不是三条规则），逐步判据见 §4.2.2 的表。
4. **内核映射与分配层**：Direct Map 覆盖（`dm_coverage.rs`）与物理帧分配器（VM 的 PMM）是两个独立消费者，它们对"哪些内存可用"的合同与分类层的输出对接——可分配性与可访问性不能脱节（两合同分别见 §3.2 与 §4.6）。

这四层的边界不能混谈：把第 1 层的 MapKey 纪律带入第 3 层的政策讨论（"反正退出前不能改"），或者把第 4 层的 Direct Map（DM）覆盖要求塞进第 3 层的分类判据（"要映射所以得进保留表"），都会让错误的层次为另一个层次的约束背锅。

**"升为字段"行的一个设计细节：为什么三个布局数字由 shim 从 ELF 算出，而不是由调用者传入**。设想把接口改为 `prepare_boot(storage, kern_virt_base, kern_phys_base, kern_size, bump_pages)`。这三个数字必须与内核镜像的链接布局一致，而它们现在的来源是 ELF 的 PT_LOAD 段（`loader.rs:compute_kernel_layout` 取所有 PT_LOAD 的 `min(paddr)`、`min(vaddr)`、`max(paddr+memsz)-min(paddr)`）。改为调用者传参后，写错的地方不会被 `check-layout.sh` 拦住：那个脚本校验的是**镜像自身**（首段 VMA/LMA、段间平移一致、跨距对齐），不校验调用者手里的常量。错值进入 `arch_boot_impl` 之后，"把内核映射到高地址"那一步会按错误的 `kern_virt_base` 建映射，症状是开启分页后取指失败——一个"参数错了"的问题表现为"页表建错了"。让 shim 自己解析镜像，"数字与镜像一致"就不再是调用者的责任，而是唯一数据来源（镜像本身）的自然结果。

`entry_point` 不进 `KernelInfo`/wire：boot-shim 从 ELF `e_entry` 取它并断言等于 `kern_virt_base`，跳转只用发送侧布局值；接收侧从 `kern_virt_base` 进入，不需要单独字段。

**两个直核面是例外**：`bootface`/`bootface_a64` 用自己文件里的 `KERN_VIRT_BASE`/`KERN_PHYS_BASE`/`KERN_SIZE` 常量，不从 ELF 头解析；它们与镜像布局的一致性：两个生产直核面与 x86_64 测试直核入口的跨距都由 `check-layout.sh` 的 L9 对账——x86_64/riscv64 读 `bootface.rs`，aarch64 读 `bootface_a64.rs`，链接期另有双保险：三份 `.ld` 都断言 `__image_end - 首段 VMA <= 0x400000` 且该差值是 2MiB 的整数倍（aarch64/riscv64 减 `KERNEL_VIRT_BASE`，x86_64 减 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`）。源码注释记录过这类常量漂移导致高半只映射半个镜像、`kern_stack_top` 落进 `.bss` 的静默损坏。因此“三个数字来自镜像”只对三条 v1 发送腿成立，直核面是“常量 + 部分构建期对账”的另一条来源。

#### 3.2.2 旧字段去向、GET_KINFO 外部面、控制台与 param_buf

本节按四个读者任务分组：字段去向、GET_KINFO 外部面、控制台、`param_buf`；控制台与 GET_KINFO 是字段的消费者/替身，不是字段去向本身，可先跳过。

字段名以 §3.2 的“删除”行为准；本表只给逐个去向，不重复定义。22 个退场字段与 16→13→14→10 两套数字的关系见 §3.2，二者不是同一集合。

22 个字段的退场多数是换载体，少数是随读者消失而退役。多数：C 的 `kinfo` 是全局变量，编译期常量、内核自有状态、记账表都顺手堆在里面；Rust 的 `KernelInfo` 收窄为纯交接结构，只装引导侧必须告知、接收方无法自行导出的内容。于是职责归位成三类：**上浮为常量**——编译期已知的布局与规模事实（`nr_procs`/`nr_tasks`）与构建身份（`release[]`/`version[]`）；`user_end` 不属此类（Rust 未回传、无消费者，见下表）；**移交专门设施或记录结构**——内核自有的诊断与记账状态（`kmessages`、`do_serial_debug`/`serial_debug_baud`、`minix_panicing`、三个以 `_bytes` 结尾的记账字段）；**换表达不换信息**——内容改由 `KernelInfo` 现存字段承载或推导（`mbi` 的内容散布进各字段，`mods_with_kernel`/`kern_mod` 由 `kern_phys_base`/`kern_size` 直接表达，`boot_procs[]` 从 `boot_modules` 现算）。找不到新读者的不只 `mem_high_phys`（恒等映射只短暂存在、帧分配直接遍历 memmap，该标量随之退役）；`bootstrap_start/len` 与 `freepde_start` 也在该次删除中因无真实读者退场（§3.4.1/§3.4.3）。

##### 3.2.2.1 22 个字段的去向

被删的 22 个字段逐个的去向（"接管者"一栏给的是 `KernelInfo` 侧去向；`nr_procs`/`nr_tasks`/`release`/`version` 四个字段在 `KernelInfo` 侧上浮为常量/构建身份，但在 `GET_KINFO` 的 `KinfoStruct` 里仍是活字段，见本节 GET_KINFO 段）：

| C 字段 | 接管者 / 去哪了 | 依据 |
|---|---|---|
| `mbi` | 不进内核：UEFI/SBI 没有 Multiboot 结构体，交接信息由 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 另行承载 | - |
| `mem_high_phys` | 删除。C 里它是"最高物理地址"标量，有真实读者：恒等映射铺满整个 4 GiB 目录，靠它把 RAM 顶以上没有内存的段标成禁缓存；PCI 驱动另拿它找 RAM 之上的 MMIO 空洞。Rust 的 boot 恒等映射只覆盖最小窗口；启动后只在 `load_elf_into` 的 ELF 段页与 64 MiB VM heap 窗口做 4 KiB 粒度驱逐（撤销这些页的 4 KiB 映射）（不重建根——重建会清零活动根、下一个冷取指页错误（TLB 残留可能先跑几条），见 §3.6 反向推演；C 侧不重建活动根，它用 paged/unpaged 两份 `pagedir` 副本换根，见 02 篇 §2.2 的两份 `pg_utils.o` 与 §2.4 的 `pagedir`；逐页拆解本篇不展开，边界见 §1.4），其余 boot identity 窗口保留在 bootstrap 根上。帧分配器直接遍历 memmap 区域列表——`mem_high_phys` 这个标量在 Rust 里没有读者，维护它只是写后即弃的状态 | C 读者：[`pg_identity()`](minix3/minix/kernel/arch/i386/pg_utils.c)、[`complete_bars()`](minix3/minix/drivers/bus/pci/pci.c) |
| `mods_with_kernel` | 删除：C 里它只为一个用途——把内核临时登记成第 M+1 个模块（M = 模块数），让重叠检查与 memmap 扣除一并处理内核自身（[`pre_init()`](minix3/minix/kernel/arch/i386/pre_init.c) 的登记段）。Rust 里内核布局由 `kern_phys_base`/`kern_size` 单独表达，扣除与检查各自直接用区间，不需要这个计数 | - |
| `kern_mod` | 删除：C 里它是上面那个"临时第 M+1 个模块"的下标，随 `mods_with_kernel` 一起消失 | - |
| `kmessages` | 独立的内核消息/诊断设施 | [`kmess.rs`](os/kernel/src/kmess.rs)（`KernelConsole`） |
| `do_serial_debug` | 早期控制台初始化自己决定串口 | [`early_console.rs`](os/plat/src/early_console.rs) |
| `serial_debug_baud` | 同上（平台实现固定串口分频，不保留可调波特率状态） | 同上 |
| `minix_panicing` | 内核侧由 `PANIC_DIAG_ACTIVE` 维护互斥；boot-shim 的 `fn panic` 只是 panic 出口，不承载该状态 | `PANIC_DIAG_ACTIVE`（`os/kernel/src/lib.rs` 的 panic 诊断互斥静态量）、[`fn register_panic_diagnostic`](<os/kernel/src/lib.rs:fn register_panic_diagnostic>) |
| `user_end` | C 侧是 `USR_DATATOP`（i386 上等于 `USR_STACKTOP`）；Rust 未回传该字段，也没有消费者 | [`const USER_STACK_TOP`](<os/libs/minix-types/src/types/boot.rs:const USER_STACK_TOP>) 是用户栈顶的唯一权威 |
| `user_sp` | v10（设计裁决）删除：每腿都填 `USER_STACK_TOP`，接收侧改为直接读该常量 | 同上一行 |
| `boot_procs[]` | 内核侧从 `boot_modules` 构造 boot image 表 | [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) |
| `nr_procs` / `nr_tasks` | 常量与表长度 | [`const NR_PROCS`](<os/kernel/src/proc_table.rs:const NR_PROCS>)、[`const NR_TASKS`](<os/kernel/src/proc_table.rs:const NR_TASKS>)（12 是启动模块数 `NR_BOOT_MODULES`，不是 `nr_tasks` 的载体） |
| `release[]` / `version[]` | 内核构建身份，来源为 `minix-types` 里与 config.h 对位的固定字符串常量（`OS_RELEASE`/`OS_VERSION`） | [`misc.rs`](os/kernel/src/misc.rs) 的 GET_KINFO 段、[`kerninfo.rs`](os/libs/minix-types/src/types/kerninfo.rs) |
| `vm_allocated_bytes` | VM 交接记录的字段 | [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) |
| `kernel_allocated_bytes` | 同上（记录里的 `kernel_allocated_static`） | 同上 |
| `kernel_allocated_bytes_dynamic` | 同上（记录里的 `kernel_allocated_dynamic`，值来自 bump 分配器用量） | [`fn boot_alloc_used_bytes`](<os/kernel/src/boot_alloc.rs:fn boot_alloc_used_bytes>) |
| `mmap_size` | 未映射为 Rust 字段：C 的遍历边界（最高占用下标加一）与中间空洞是定长数组布局的产物；Rust 的 `memmap` 是紧凑条目列表，`.len()` 只是条目数，不需要单独的高水位标量 | [`globals.rs`](os/kernel/src/globals.rs:const MEMMAP_REGION_STORE_LEN) 的容量常量与 §2.4 的边界语义 |
| `freepde_start` | 曾改型 `Option` 跨 handoff 携带，字段删除时整体退场（删除论证与报告位填 0 的去向见 §3.4） | [`misc.rs`](os/kernel/src/misc.rs) 的 `getinfo_kinfo` |
| `bootstrap_start` / `bootstrap_len` | 曾以 `(PhysBytes(0), 0)` + 硬断言携带，字段删除——误设回收区间的接口面随字段一起消失（论证、过度回收风险表与 wire 版本变更见 §3.4） | §3.4 |

C 的两个专用键在 Rust 侧的处置：`cttyline`/`cttybaud` 不再有专用字段，`normalize_param_buf` 把它们当普通 `key=value` 保留（能进 `param_buf`），但早期串口的选择由 `early_console` 的平台/编译配置固定，运行期不可调。


##### 3.2.2.2 GET_KINFO 收窄与外部面

**外部可见面的一条架构演进** `[ARCH: GET_KINFO 载荷收窄为五个 used 字段 + 两个 C 布局占位；其余字段由 GET_IMAGE/GET_MONPARAMS/GET_KMESSAGES 等分开提供]`：C 的 `GET_KINFO` 把整个 `struct kinfo` 拷给调用者（[`do_getinfo.c`](minix3/minix/kernel/system/do_getinfo.c) 的 `length = sizeof(struct kinfo)`）；Rust 只拷 48 字节的 `KinfoStruct`（布局：`freepde_start`/`user_sp`/`vir_kern_start` 三个 `i64` = 24 B，`nr_procs`/`nr_tasks` 两个 `i32` = 8 B，`release`/`version` 两个 `[u8; 6]` = 12 B，再加 4 B 对齐 padding，合计 48 B）（[`kinfo.rs`](os/libs/minix-types/src/types/kinfo.rs)），只含七个字段（五个有读者 + 两个 C 布局位；外加 4 字节 `_padding` 把结构补齐到 48 字节）——其中**五个有 Rust userland 读者的语义字段**：`nr_procs`、`nr_tasks`、`release`、`version` 由 IS（信息服务）读；`user_sp` 由 RS（重生服务器）的 exec 路径经 `GET_KINFO` 读 `KinfoStruct.user_sp`（offset 8，`os/servers/rs/src/trap_api.rs:223-230`、`:430`）。

`KernelInfo.user_sp` 与 `KinfoStruct.user_sp` 同名不同物：前者是交接结构的字段，后者是 GET_KINFO 的布局位；v10 删除的是前者。另两个是保留的 C 布局位：`freepde_start` 恒填 0（Rust VM 不借槽，见 §3.4）、`vir_kern_start` 有取值来源但仓内零读者（复核：`rg -n 'mem_high_phys' os/` 零命中；`rg -n 'vir_kern_start' os/servers os/kernel` 只在 `os/kernel/src/misc.rs` 的 `KinfoStruct` 构造处出现，没有读取该字段的消费者）。

C 侧至少四个活读取点：`dmp_kernel.c` 读四个字段，`procfs/root.c` 读 `nr_procs`/`nr_tasks`，`pci.c` 的 `complete_bars()` 读 `mem_high_phys`，VM 的 `main.c`/`pagetable.c`/`utility.c`/`region.c` 合计读 ≥11 个字段（`memmap`/`mmap_size`、`module_list`/`mods_with_kernel`/`kern_mod`、`boot_procs`、`user_sp`、`vir_kern_start`、`freepde_start`、三个 `_bytes` 等）。Rust 收窄的是**这条系统调用的载荷**，不是"C 只有这些读者"：Rust 收窄后，这些内容改由 `GET_IMAGE`/`GET_MONPARAMS` 与新增的 `GET_KMESSAGES`/`VmBootHandoff` 承载（新增通道标 `[ARCH]`），`mem_high_phys` 在 Rust 侧已不存在。

受影响的 C 字段——`memmap[]`/`mmap_size`、`module_list[]`/`boot_procs[]`、`param_buf`、`kmessages` 指针、`bootstrap_start/len` 等——不再经这条系统调用出现：模块表走 `GET_IMAGE`、`param_buf` 走 `GET_MONPARAMS`、`kmessages` 走 `GET_KMESSAGES`；内存账不走 GETINFO——它在 VM 启动前由内核写进固定地址的 boot handoff 页（`VmBootHandoff.free_regions` 加扣除记录），VM 在 `os/servers/vm/src/boot.rs` 读它。行为基线定为 Rust userland 的 `KinfoStruct`；按 C 全布局读被删字段的程序在本仓没有消费者。

##### 3.2.2.3 两种控制台与五处字段补充

表中两个"控制台"落点名字相近、层次不同：`EarlyConsole`（[`early_console.rs`](os/plat/src/early_console.rs)）是硬件输出通道——`minix-plat` 里的跨架构 trait，每架构至少实现 `write_byte`（`init` 可选；x86-64 用它初始化 COM1），启动与诊断输出经它直写串口；`KernelConsole`（[`kmess.rs`](os/kernel/src/kmess.rs)）是内核诊断收口——对应 C 的 `kputc`，把流经的每个字节先记进 10000 字节环形缓冲（C `struct kmessages` 的重写）再转发给当前架构的 `EarlyConsole` 实现，环内容经 `GET_KMESSAGES` 快照供用户态读取。分工一句话：前者管"字节怎么落到硬件"，后者在其上叠加"内核诊断的记录"。

几处需要展开的字段，各自的来源与理由：

- `reserved_regions`：被分类器标成 `ReservedMapped` 的内存区间（`LOADER_CODE`/`LOADER_DATA`/`BOOT_SERVICES_CODE`/`BOOT_SERVICES_DATA`/`ACPI_RECLAIM`）；`Excluded`/`DevicePath`/`Side` 不进这份表。它曾装两种语义：其一（x86 内联窗口）是内核链进 boot-shim PE、在低物理地址执行时，VM 建进程页表必须逐段恒等复现的代码/GDT/IDT/TSS 窗口；其二（UEFI 链通用）是内核运行期经 Direct Map 访问的固件分配（loader 数据、内核栈一类不在 conventional 列表里的页），建立 DM 覆盖时必须列入候选（依据见 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4）。x86_64 切独立镜像后第一种用途退场：`vm_handoff` 不再携带 identity 字段（`handoff v9` 删除恒空的 `kern_ident`）；第二种用途保留，作为 Direct Map 覆盖的 source 4。各条腿怎么产出这份列表见 §3.2 的“五条生产腿产出这两张表”三条 bullet。
- `kern_stack_top`、`syscall_entry`：C 里这两个值通过链接期符号与 `prot_init()` 内部计算得到。v10（设计裁决）把它们（连同 `user_sp`）从交接面撤掉：三者都是两端可算的同一常量关系，跨镜像传输只是把恒等式抄一遍；现在消费点分别用 `KernelInfo::kern_stack_top()`（`kern_virt_base + kern_size`）、`minix_types::USER_STACK_TOP`（每架构编译期常量）与 `syscall_entry_va()`。
- `platform_sources`：固件描述符来源列表（DTB / RSDP），按 boot-shim 的偏好排序，内核取第一个解析成功的源。每个 `PlatformDescSource` 是 16 B 句柄 `{ kind: PlatformDescKind, len: Option<NonZeroU32>, phys_addr: PhysBytes }`；`len = None` 表示生产者未记录 blob 长度（旧生产者、测试或读头失败）。wire 侧仍写 `u32`：`None` 编码为 0，解码时 0 还原为 `None`。`RSDP` 是 ACPI 根描述符指针，指向 RSDT/XSDT；`DTB` 是设备树 blob。解析归 `04-platform-discovery.md`，本篇只讲它怎么被放进结构体。
- `param_buf`：对应 C 的 `char param_buf[MULTIBOOT_PARAM_BUF_SIZE]`（1024，`MULTIBOOT_PARAM_BUF_SIZE` 定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)）。它是 2.3 那张参数表在 Rust 侧的落点，生产侧由 `minix_boot::normalize_param_buf` 统一规范化：输入按 NUL 或 ASCII 空白分词（UEFI 记录以 NUL 分隔，DTB `bootargs` 以空格分隔），逐条要求可打印 ASCII 的 `key=value`；非 ASCII、无等号、空键与内部键（`ARCH`/`BOARD`）逐条拒绝；`ARCH` 由编译目标生成，`BOARD` 只有真实来源才生成（当前生产调用没有可靠来源，显式传 `None` 省略，不伪造——测试 `generates_arch_and_board_first` 传显式板名验证的正是"有来源才生成"这一支）。

##### 3.2.2.4 param_buf 的十条可观察分歧

与 C 的可观察分歧共十条，按三档术语纪律统一标注 `[ARCH: 参数表语义规约化；十条可观察分歧见下]`：

- 其一，重复键——C 的"已存在先删"只对首条记录生效（§2.3 的源码推演），但结果分两支：若重复键的首个副本是第一条记录，旧条被删、新值追加，**后写胜出、只剩一条**；若首个副本不是第一条，旧条留在原位、新值追加到缓冲末尾，账上两条而读取返回第一条。Rust 统一为首条生效、后者逐条拒绝，因此在"重复键是第一条"这一支上与 C 相反（C 后写胜出），属于新增的可观察分歧，不是"C 缺陷语义的载体重构"。同名键的整条删除只对首条记录成立；非首条记录还可能被中段匹配误删（§2.3 的 `xboard=2` → `xarch=i386`），这条不改变上面两支的结论，但"非首条"不等于"完全不受影响"。
- 其二，命令行里的 `ARCH=`/`BOARD=` 词——C 不拒；若外部写的是小写 `arch=`/`board=`，首条时会被内核随后写入的同名键覆写，大写拼写则与 C 的生成键不同名、不会被覆写。Rust 一律拒绝外部输入覆盖内部键，检查大小写不敏感（小写 `arch=`/`board=` 同样被拒，`rejects_internal_key_override` 覆盖）。
- 其三，`BOARD`——C 无条件写一条记录，但读取返回首条匹配、可被外部同名键遮蔽，因此不保证生效（§2.3）；Rust 按来源生成、无来源则省略（上文）。
- 其四，容量耗尽——Rust 一条放不下就丢弃它及其后全部记录，C 只丢当前条，后面的词逐条再试——短的仍能收下，长的仍会被丢（`drops_tail_records_whole_at_capacity` 锁住前者）。
- 其五，内部键拼写——Rust 生成大写 `ARCH=`/`BOARD=`，C 写小写 `arch=`/`board=`。
- 其六，分隔符集合——Rust 按 NUL 与 ASCII 空白（含 tab/CR/LF）分词，C 只按空格，同一 `bootargs` 可得到不同条数（`splits_nul_and_whitespace_records` 锁住前者）。
- 其七，字符集——Rust 拒绝非 ASCII/不可打印记录，C 原样存（`rejects_malformed_and_non_ascii`）。
- 其八，空键——Rust 拒绝 `=value`，C 存成一条 `=value`。C 的删除循环在 `namelen = 0` 时 `strncmp(p, "", 0)` 恒真，`p[0] == '='` 可能命中的是**下一条**记录的等号，于是同名键删除会截断后续记录的值（字节推演见 §2.3 的删除扫描段）；这个副作用只在空键输入下出现，Rust 直接拒绝该输入，因此没有对应路径。
- 其九，`ARCH` 取值——C（i386 路径）写 `arch=i386`，ARM 侧写 `arch=earm` 一类（§2.7）；Rust 写 `ARCH=x86_64`（编译目标）。
- 其十，记录顺序——C 先放命令行记录、后写内核生成键；Rust 生成键在前、外部记录按输入序在后。

这十处都落在 `GET_MONPARAMS` 的 wire 上可观察；仓内消费者核查：Rust 侧 `os/kernel` 与 `minix-boot` 没有把 `BOARD`/`board` 当板卡身份读取的消费者——命中只在 `boot_params.rs` 的 `fn normalize_param_buf` 两处（生成内部 `BOARD=` 与拒绝外部同名键）以及测试字面量；C 内核的 `get_value` 读 `board=` 属 C 侧消费者，不在本审计范围。参数源分两类，按 §3.6 的三类链落到各腿：两条 UEFI 链读当前镜像的 `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions`；U-Boot shim 与两个直核面（按 §3.6 归 U-Boot 一族与直核面两类）读 DTB `/chosen/bootargs`。消费者 `GET_MONPARAMS` 仍按 C 的 1024 字节定长零填充；没有外部输入时缓冲里只有生成的那条 `ARCH=...`——与 C 的 `mb_set_param` 必写行为仅就 ARCH 一项同形。

#### 3.2.3 进程内结构不做 ABI 与 arena wire 协议

> [参考] 版本变迁：现役信封版本是 10；下面 `7→8`、`8→9`、`9→10` 只用于解释 wire 形状为什么变化，不影响现役 10 字段/六分区的读法。

`KernelInfo`、`MemoryRegion`、`BootModule` 都不使用 `#[repr(C)]`：字段顺序与切片内部 `(ptr, len)` 的顺序都不是契约。`PlatformDescSource` 是例外：它用 `#[repr(C)]` 只为钉住 16 B 句柄形状，字段顺序仍不是跨镜像 ABI（wire 用 `WirePlatformDescSource`）。它们只作为接收侧进程内结构使用（内核 `.bss` 副本、QEMU 测试内核；五条腿的发送侧只构造 `BootReport<'_>` 借用视图），不承担跨镜像布局契约。

跨镜像三腿（两条 UEFI 产线 + riscv64 U-Boot shim）改用独立协议 `BootHandoffV1`：标量全部写成固定宽度，载荷放进一个八页 arena（arena：boot-shim 预留的一块定长物理内存，里面摆着 v1 交接协议的信封与各载荷分区，分区布局见本节末）的固定分区，描述符只记 `byte_len`——地址由 arena 基址加协议偏移推导，`(ptr, len)` 不再上线。

协议有两层版本号，必须分开：**协议身份**恒为 v1（`BOOT_HANDOFF_V1_MAGIC`/名字里的 v1，指"第一代交接协议"这个族）；**信封版本号**（`BOOT_HANDOFF_V1_VERSION`，当前 10）随信封或 arena 的 wire 布局变更递增、收发双方精确比对——本篇出现的"7→8"、"8→9"与"9→10"都是后者（"wire 版本"是同一计数器的另一个叫法；7→8 删 `bootstrap_start/len` 与 `free_upper_idx`，8→9 删 arena 的语义工作分区并把 source 分区前移，9→10 删三个标量与 startup 分区）。本篇不带限定词的 `v9`/`v10` 一律指 wire 信封版本；`VmBootHandoff` 的版本写 `handoff vN`。同样，`source` 有两个限定用法：`source_records`/source 分区指出内存图来源记录（provenance），`platform_sources`/平台源指 DTB/RSDP 描述符来源；单说 source 时按上下文取前者，不用裸 source 指平台源。

##### 3.2.3.1 wire 字节布局（v10）

信封 `BootHandoffV1` 固定 128 B，字段偏移由 `handoff_v1.rs` 的 `envelope_offsets` 用 `offset_of!` 派生；下表是当前 v10 的读数。所有整数小端；`u64` 8 字节对齐，`u32` 4 字节对齐；偏移都相对 arena 基址。

| 偏移 | 宽度 | 字段 |
|---|---|---|
| 0 | 4 | `magic` |
| 4 | 4 | `version` |
| 8 | 4 | `size` |
| 12 | 4 | `ram_authority`（0 = DTB，1 = UEFI） |
| 16 | 8 | `arena_len` |
| 24 | 8 | `kern_virt_base` |
| 32 | 8 | `kern_phys_base` |
| 40 | 8 | `kern_size` |
| 48 | 8 | `root_page`（运行期补） |
| 56 | 8 | `bump_base`（运行期补） |
| 64 | 8 | `bump_next`（运行期补） |
| 72 | 8 | `bump_end`（运行期补） |
| 80 | 8 | `memmap_byte_len` |
| 88 | 8 | `reserved_regions_byte_len` |
| 96 | 8 | `boot_modules_byte_len` |
| 104 | 8 | `platform_sources_byte_len` |
| 112 | 8 | `param_buf_byte_len` |
| 120 | 8 | `sources_byte_len` |

信封占 128 B，`ENVELOPE_CAP` 为 256 B；之后是固定分区（偏移/容量/元素）：

| 分区 | 偏移 | 容量 | 元素宽度 |
|---|---|---|---|
| `memmap` | 256 | 128 项 | 16 B |
| `reserved_regions` | 2304 | 256 项 | 16 B |
| `boot_modules` | 6400 | 12 项 | 32 B（16 B 名字槽 + start + len） |
| `platform_sources` | 6784 | 4 项 | 16 B |
| `param_buf` | 6848 | 1024 B | 字节 |
| `sources` | 7872 | 296 项 | 72 B |

`WIRE_END = 7872`，`WIRE_SOURCES_END = 29184`；arena 总容量 `ARENA_CAP = 32768`，尾部设计余量 3584 B。描述符只记 `byte_len`，地址由“arena 基址 + 上表偏移”推导，wire 上没有任何指针。任何字段或分区变化都必须升 `BOOT_HANDOFF_V1_VERSION` 并同步收发两侧；当前信封版本是 10。

arena 的分区是固定偏移：信封住首部，其后依次是内存图、保留区、模块、平台源、参数字节的 wire 形式；v9 之后 arena 里**没有**第二份语义表——发送侧的分类结果同时写进它自己的 `BootStorage`（调用方持有的定长数组）和 wire；UEFI 腿的 source 分区例外，只写 wire，`BootStorage` 的 source 切片为空；U-Boot shim 的 DTB source 记录仍经 `fill_source_records` 写 `BootStorage` 的 40 槽，再序列化进 wire（见 §3.7/§4.5）。接收方再按 `byte_len` 把 wire 复制进自己的 `.bss` 落地区。这样 arena 只有一份权威表示，发送侧构造进程内视图时借用 `BootReport<'a>`（`BootStorage` 的视图），不再用 `'static` 切片指向 arena。平台源 **wire 记录** 是 16 B 的 `kind u32 | len u32 | addr u64`：`len` 复用旧 `reserved` 槽位，旧生产者写 0 表示未知、新读者回退单片；因此该记录仍是 16 B，v10 的版本号变化只来自 §3.2 列出的四个成员。接收方先校验 `magic`、`version`、`size` 与 `arena_len`，再按各 `byte_len` 把六个分区复制进自己的 `.bss` 落地区；越界、不能整除或容量不足都在构造任何引用之前失败。运行期字段（根页表物理地址与 bump 三件套）在 shim 建好根之后原地补进信封，因此三腿都只需传 arena 基址一个数（入口寄存器见 §4.1）。

#### 3.2.4 深拷不变量与真机探针

**不做深拷、把 `KernelInfo` 的切片按原指针存进内核的全局槽会怎样**。两条 UEFI 产线的六个 wire 分区都写在 arena 的固定偏移上；发送侧的语义表在 `BootStorage`，底层字节来自 Boot Services 池或 arena。无论哪一条，接收方都必须在首次进程根切换之前把值搬进自己的 `.bss`——arena 只在恒等窗口存活，池页在历史真机上还出现过零化。

真机记录给了这一现象的两组读数（`[观测]`），零化时点不同：`res=127 zeroed=127`（`res` 是到达分类点的 `reserved_regions` 条目数，`zeroed` 是其中因长度读回零而被拒的条数——127 对 127，也就是全部读成了零），以及"15 条 conventional 条目在建立 Direct Map 覆盖时还完好、到 VM 交接分类点长度全读成 0"。后者的后果不是某个字段错，而是 VM 的自由内存清单塌成零条——VM 在启动参数校验处 panic，而"内存图丢了"这件事在 panic 输出里看不到。两组读数指向同一个设计结论：值必须在入口尽早深拷（`[预防性不变量]`）。

机制上有两条候选。**候选①（页表不重放）**——源码注释给过的一条是“内核自己的页表不重放这些堆页的 1:1 映射”（[`globals.rs`](os/kernel/src/globals.rs:static RESERVED_REGION_STORE) 的容量注释）。**候选②（复用清零）**——“建立 Direct Map 覆盖时还完好”这组读数更像的“被当空闲复用后清零”。两条候选各带可检验的推论——"页表不重放"说与切表时点相关、"复用清零"说与快照/复用的时序相关——`test-kernel-map` 的 N-5 探针是 print-only，不参与 PASS 判据；候选①只按源码注释与人工观察保留、候选②只按读数形态保留，两者都未排除（`[观测]`；结论见 §5.3）。这条不变量与 §2.4.2 的“跳板生命周期”同级，都按 `[观测]`/`[预防性不变量]` 读。



### 3.3 两张内存表与来源记录（I2）

两张表不是同一种数据：`memmap` 是“可分配”的候选，`reserved_regions` 是“已占用/不可分配”的候选；同一个物理字节可以在不同阶段从一张表转到另一张表，但不能同时被两张表当成自己的。

`SourceRecord` 给每个投影字节记来源，共 11 个字段：身份组 `id`/`local_source`/`snapshot_id`（记录在输入对象内的编号、输入对象内下标、`GetMemoryMap` 快照号），范围组 `range`/`address_domain`/`virtual_start`（区间、地址域、固件的 `VirtualStart`），分类组 `kind`（`/memory`、MRB（device-tree memory reservation block，设备树内存保留块）、`/reserved-memory`、UEFI 描述符等；审计里的 `kind = Memory` 指 `/memory` 这一类，UEFI conventional 描述符按同一 kind 归类）、`origin`（节点/条目/描述符下标）、`input_object`（输入对象），以及固件原值组 `attribute`（描述符 `Attribute`）与 `raw_type`（固件自己的类型值，与 `kind` 的角色分类不是一回事）。`RamAuthority` 把“这份内存图由哪个输入对象声明”从 `InputObjectId` 里分出来：只有 DTB 与 UEFI 快照两种取值，`StartupObjects` 已随 v10 删除。构造 `KernelInfo` 时 `authority` 与 `source_records` 一起传入；`validate_requiring_sources` 要求 authority 对应的记录覆盖投影。

覆盖审计是 I2 的判据：free 表的每个字节必须能被 `kind = Memory` 的记录解释；reserved 表的每个字节必须能被 reservation 类记录解释。在生产腿的 strict 门里，说不出来源的字节不是“忽略”，是 fail-closed；非 strict 的 `KernelInfo::validate` 只在 `source_records` 非空时复核（§3.2.1）。库函数 `minix_boot::memmap_source::audit_source_coverage`/`audit_source_coverage_for` 是这层检查的两个入口；`KernelInfo::validate_requiring_sources` 在 strict 分支里调用带 authority 过滤的后者，再叠加模块名/位置与内核跨度覆盖。C 侧没有这个结构：C 的 `memmap` 条目只有基址、长度与一个固定为 `AVAILABLE` 的类型标签（`multiboot_memory_map_t.mm_type`），没有“谁声明了这段”的来源记录，来源隐含在“引导器给的”这个前提里；Rust 把它显式化：跨镜像腿的接收侧无法再问引导器，直核腿虽然同镜像，也保留同一份记录以复用同一覆盖审计。

### 3.4 bootstrap 区间、分配器换手与页目录槽位

这一节回答三个彼此相邻但不同的问题：C 的 `bootstrap_start/len` 为什么在 Rust 侧被删、页表页分配器怎样从 shim 换手到接收镜像、`free_upper_idx` 的借槽机制为什么在 64 位退场。三者共同的主题是“进入高半前后，资源和槽位归谁”；阅读顺序先看删除判据，再看换手表，最后看槽位借用链。

#### 3.4.1 `bootstrap_start/len`：从可回收区间到字段删除

> [参考] 设计变迁：本节解释这两个字段为什么被删除；现役 `KernelInfo` 已没有它们（§3.2.1）。判据结论在段末。

C 的这两个字段装的是链接脚本导出的一小段内核镜像内区域的边界（2.3 已看到赋值）。这段区域的设计性质可以压成六条结论（行级细节见第四章与 02 篇）：

- `kernbase`/`kernsize` 来自链接脚本的三个基址赋值，也是 02 篇讲 `AT()` 的两条基址。
- `_kern_size` 不是字面量而是布局算出的符号：`.bss` 收尾处 `__k_unpaged__kern_size = . - _kern_vir_base`（整幅镜像的虚拟跨度），随后 `_kern_size = __k_unpaged__kern_size` 把它导出成 §2.5 引用的那个名字——它的值随每次构建的段布局而定，没有固定数字。
- 链接脚本让第一段的位置计数器落在物理基址上；`unpaged` 三个段（来自文件名匹配 `unpaged_*.o` 的目标文件）放在这里，它们的**执行地址等于装载地址**（没有 `AT()` 偏移）。
- `__k_unpaged__kern_unpaged_start/end` 把这段夹出边界，交给 `get_parameters` 填进 `bootstrap_start/len`。用一对起止符号而不是单一起点，是因为下游要的是半开区间 `[start, end)` 的长度——回收接口 `add_memmap` 收 `(addr, len)`，只有起点算不出长度。C 代码里的引用名没有双前缀（`_kern_unpaged_start`，§2.3 的 `get_parameters` 引文可核对）；构建期 `objcopy --prefix-symbols=__k_unpaged_` 把它改写成 `__k_unpaged__kern_unpaged_start`，与链接脚本定义相遇——`k_initial_stktop` 的构建期前缀规则是同一机制的另一例，其落点见 02 篇 §2.5。
- 链接脚本在 unpaged 段之后加上 `_kern_offset`，其后的所有段才带偏移（`AT(ADDR(...) - _kern_offset)` 表示"执行地址在高处、装载地址在低处"）。两个地址必须分开的理由：引导器只按装载地址把字节写进物理内存，而分页开启后 CPU 按执行地址取指；`AT()` 就是链接期把同一个段的两个地址钉在一起的方式。
- 于是这段 `unpaged` 区域是"分页开启前执行、分页开启后不再需要"的代码与数据（它的执行地址等于物理地址，靠恒等映射就能访问）：`kinfo` 已在 `kmain` 开头拷进 `.usermapped` 副本，活跃的 `pagedir[]`/静态页表池在高半 paged `pg_utils.o` 副本里，段内留下的是 unpaged 副本。C 在 `kmain` 后段（`system_init` 之后、`bsp_finish_booting` 之前——精确锚点见 §2.5.1 的"还引导段"行）把它 `add_memmap` 归还给通用分配器，且归还发生在 `kernel_may_alloc` 写 0 之前，不撞 §2.4 那组断言。

Rust 侧没有这样一段，这里要分清两件事：**链接布局**上生产内核镜像（两条 UEFI 产线 + U-Boot 接收镜像 + riscv64 直核生产镜像 + aarch64 直核镜像 `fw-aarch64-none`）的所有段都在高半虚拟地址（C 的 `unpaged` 段是唯一"执行地址 = 物理地址"的例外；QEMU 直核测试内核按 VMA=LMA 平铺，见 02 篇的 4.5 节，不在这条陈述内）；

**实际执行地址**上，两条 UEFI 链的 boot-shim 建好根、开分页后跳进独立 `kernel.elf` 的高半入口，内核从第一条指令起就在高半（§3.6）；riscv64 的自引导链则起步于物理视图（medany——RISC-V 默认的 PC 相对寻址代码模型，代码按"装在哪就按哪解释地址"的方式编译——的平移视图，见 02 篇的 4.5 节），高半地址要等开分页之后才被解引用。

**判据结论**：当前所有生产链接布局（两条 UEFI 链、U-Boot 接收镜像、riscv64 与 aarch64 生产直核镜像，均为高半 VMA）与 VMA=LMA 的 QEMU 测试内核都不存在"分页开启前执行、之后可归还"的**内核段**。内联形态已退役，不在本条判据范围。正面写法：没有一段是"在链接布局上可单独界定、且分页后不再需要"的；`bootface` 代码虽然只在分页前执行，但它是内核镜像的一部分、没有独立链接边界，不能单独归还。因此这个字段对在 Rust 侧没有语义内容——它随后的处置（先写死为 `(0, 0)`、最终整个删除）都从这一句来。

这个字段对在 Rust 侧的处置走过两个阶段，最后停在**删除**：先是把 `(0, 0)` 写死并配一条硬断言（"必须为 0"），字段删除后离开 `KernelInfo`，wire 版本 7→8：本字段对释放 16 字节，同版本删除的 `free_upper_idx` 再释放 8 字节，合计 24 字节，envelope 从 184 缩到 160（该字段的删除论证见本节下文的过度回收风险表；`free_upper_idx` 的删除论证见 §3.4.3）。为什么"写死 + 断言"还不够、要连字段一起删：`bootstrap_start/len` 表达的其实是**一个布尔事实**（"存在分页开启前专用、之后可回收的段"——Rust 侧恒为否），却占两个整数、两个 builder 参数和 16 字节 wire；所有生产调用点都传 `(PhysBytes(0), 0)`，唯一的"读者"是那条断言自己。更重要的是保留字段就保留了一个误设的接口面——`validate` 的断言只能把非零值拦在 `kmain` 入口，拦不住"合法类型、错误语义"的值进入类型系统。误设的后果是本篇推演过的真实事故来源：

这不是纯假设：曾有一次把 `bootstrap_len` 填成 `kern_phys_base.0`（`[观测]`，仅留作反事实推演的引子）。先把时刻说准：误设发生在直核交接载荷的构造点，那一步只是把 `[0, kern_phys_base)` **加进空闲清单**，不写任何内存；真正会覆盖这些区间的是之后的取页者（内核的 `boot_alloc`、以及 VM 接手后的分配器）。因此下表是**反事实推演**：它列的是"如果这块区间之后被分配出去，谁会先读到坏数据"，不是同一时刻四行同时发生。下表列的是那次误设会扫进的同一区间 `[0, kern_phys_base)` 里的四类内容（不是互斥分区）：

| 若 `bootstrap_len` 被设成 `kern_phys_base.0`（下表以 riscv64 链为例：`kern_phys_base = 0x80200000`；OpenSBI 行有地址锚点，其余三行是当时部署的观察，具体地址未记录） | 那块内存实际存着什么 | 误回收后先坏在哪（按"取页时谁还活着"读） |
|---|---|---|
| OpenSBI 区间 `[0x8000_0000, 0x8020_0000)` | SBI 运行时服务（console、timer、IPI）的代码与跳板 | 内核接手后仍会调 SBI；下一次 SBI 调用（早期控制台输出、定时器）进入被覆盖的地址。这是四行里唯一"取页时仍然活着"的读者 |
| DTB 所在区间 | 设备树 blob；boot-shim 用它建内存表、读 `/chosen/bootargs`，并把物理地址作为 `PlatformDescSource` 传给内核 | 内核在 kmain 之后解析 DTB 时读到被改写的属性，硬件发现失败（归 04 篇）；若 DTB 在取页前已被解析完，则这一行不构成先坏点 |
| U-Boot 镜像与 `BootFileTable` | 生产链上 `fatload` 预载的文件；表本身是 shim 镜像 `.bss` 的一部分（`build_shim_table` 构建），不是预载物 | **不构成先坏点**：shim 交出控制权后不再从表里取文件；这段区间被覆盖的后果只是"若回跳/重入 shim 会读到垃圾"，正常启动路径不会发生 |
| boot-shim 自身 | shim 的代码与栈（仅在 shim 镜像落在 `[0, kern_phys_base)` 的部署里） | **不构成先坏点**：取页发生在内核接手之后，那时 shim 已不再执行；直接取指失败只在回跳/重入时发生。若某次部署里 shim 仍在执行期间就被取页，才属于这一行的原描述 |

这张表里的每一行都落在"某块仍被使用的固件/引导代码内存被写进空闲清单"，而四者的共同点是：它们都不在 boot-shim 报告的可用内存里，而是被"从 0 开始、长度到内核物理基址"这个区间扫进去的。诚实边界：OpenSBI 行有 `[0x8000_0000, 0x8020_0000)` 这个地址锚点；DTB/U-Boot/shim 三行的具体物理地址当时没有记录，只能按"那次部署把三者放在内核物理基址以下"的观察来读，不要把它们当成可复算的地址证据。表中"不构成先坏点"的两行仍然留在表里，是因为它们说明误回收的**区间范围**；它们不承担"四类内容都会先坏"的结论。

这张表也解释了那次险情的机制：把"上界地址"当"区间长度"，`add_memmap(0, kern_phys_base)` 会把上表所列的全部固件结构扫进"可回收"———早期直核构造点（riscv64 链）曾有一条注释（字段删除时随参数一起删除），原文（源码里分两行，这里合并）把自己的旧取值记为险情来源：`the previous \`PhysBytes(0) + len = kern_phys_base.0\` was a dangerous over-reclaim that swept up OpenSBI firmware, DTB, U-Boot image, and the boot-shim itself`（x86_64 链上同一个误设的区间是 `[0, 0x200000)`，即 [`x86_64.ld`](os/kernel-image/x86_64.ld) 的 `KERNEL_PHYS_BASE`）；现仓在 `opensbi_helpers.rs` 的测试模块保留一句"结构上不可能"的回归说明；当前 `kmain` 的早期校验段已不再要求 `bootstrap_len == 0`（该字段已不存在），早期实现的取证注释仍提到该字段。那次险情靠人工改值解决；字段删除之后，"提供一个可误设的回收区间"这个接口面整体消失，险情在类型上不可复发。C 侧的归还机制照旧运转（`add_memmap` 对零长请求静默吸收的语义，2.4 已见），本篇不再为它携带任何 Rust 侧对应物。
#### 3.4.2 引导期页表页分配器的换手

§3.6 的五条生产腿与交接形态定下来之后，这一节看引导期页表页分配器怎么在这两个镜像之间换手。C 的**页表页**来源是 `alloc_pagetable` 的 6 页静态池，用完 panic（[`alloc_pagetable`](minix3/minix/kernel/arch/i386/pg_utils.c:alloc_pagetable)）；`pg_alloc_page` 从可用内存记录的最高条目尾部取页，服务的是 4KB 被映射数据页，不是页表页（§2.4.2）。页表分配策略与"页表操作"写在同一份文件里，页表代码直接调用分配函数。

**把 C 的静态页表池原样搬到 64 位会怎样**（保留"6 页，用完 panic"）。`PG_PAGETABLES = 6` 是两级页表的预算：32 位下 `pg_map` 的一个 4 MB 目录项只需要一个页表页，6 页对常见的小范围映射够用；跨 4 MB 边界或分散映射会多占页，这一点在下面的 64 位推演里放大。4 级页表下一次 4KB 映射要穿过 PML4（x86-64 四级页表的顶层，§3.4.3 详述）→ PDPT → PD → PT 四级，其中下面三级都可能需要新建页表页；实际需要的页数随地址分布变化（映射分散时每个 PML4 槽都要一条新链）。固定 6 页按每个 PML4 槽最多 3 页算只够两个槽，第三个槽（第二次跨 PML4 边界）就会耗尽，表现为 `panic("no more pagetables")` 落在启动中途——而"6 页"这个数字在 64 位下没有任何依据。

Rust 侧把**页表页**分配抽出来（`pt_alloc.rs` 的 [`fn register`](<os/arch/src/arch/pt_alloc.rs:fn register>)、[`fn alloc_pt_page`](<os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page>)）；4KB 数据页的运行期分配归 VM 物理分配器，不在这个槽里：`Paging`/`HugePages` 的实现只调用一个没有参数的函数指针槽，谁来提供页由"注册"决定。

boot 阶段的实现是一个 bump 分配器（`boot_alloc.rs` 的 [`BootAlloc::alloc`](os/kernel/src/boot_alloc.rs:fn alloc)）。它从区间起点向上取页并记 `used_bytes`；换手是下面的事：

换手由一次初始化、三次 `PT_ALLOC` 注册与一次 VM `PT_FREE` 归还钩子注册组成（下表三行：第 1 行是初始化、不注册；第 2 行里两个二进制各注册一次，共两次；第 3 行一次；以下按跨镜像腿计，直核面没有 shim，只有接收镜像的一次注册），各有锚点：

| 时刻 | 谁写这个槽 | 代码 | 依据 |
|---|---|---|---|
| shim 进程内、进入内核镜像之前 | 初始化 bump 池（**不写注册槽**） | `boot_alloc::init_boot_pt_alloc(bump_base, bump_end);`（U-Boot 腿写成 `result.bump.start()`/`result.bump.end()`） | [`uefi_helpers.rs`](os/boot-shim/src/uefi_helpers.rs)、[`uboot_main.rs`](os/boot-shim/src/uboot_main.rs) |
| shim 进程内 Step 0 与**接收镜像**的 `resume_additive`（同一 helper `boot_validate_and_prepare`） | 只有"未注册"时才写——每个二进制各有一份 `PT_ALLOC` 静态（`AtomicUsize`，0 表示未注册），接收镜像的这份在它自己的 `.bss` 里首次注册。shim 侧不直接调 `register`，而是在 `boot_validate_and_prepare` 内经 `is_registered()` 守卫间接注册；两次注册不冲突：UEFI 腿写的是 boot-shim 的那份、接收镜像写的是自己的那份，互相独立 | `if !pt_alloc::is_registered() { pt_alloc::register(boot_alloc::boot_pt_alloc); }` | [`boot_validate_and_prepare`](os/kernel/src/lib.rs:fn boot_validate_and_prepare) |
| VM 初始化 | VM 在自己的地址空间里注册（并注册归还钩子） | `if !minix_arch::pt_alloc::is_registered() { minix_arch::pt_alloc::register(crate::alloc_page::vm_pt_alloc); }` 与 `if !minix_arch::pt_alloc::is_free_registered() { minix_arch::pt_alloc::register_free(crate::alloc_page::vm_pt_free); }` | [`vm_server.rs`](os/servers/vm/src/vm_server.rs) |

内核侧与 VM 侧的写者都用 `is_registered()`（或 `is_free_registered()`）先自检；shim 侧不直接调用 `register`，`init_boot_pt_alloc` 只设定 bump 区间，`boot_validate_and_prepare` 在 shim 进程里经 `is_registered()` 守卫间接注册。这个槽是“写一次”的，重写是 bug，因此没有“后注册者覆盖前者”的语义。VM 侧的分配函数把页表页从 VM 的物理内存分配器取出，归还函数把它还回同一个池。C 这里有两套池，不能混：内核自举页表的 6 页静态池（`i386/pg_utils.c` 的 `alloc_pagetable`，发完 panic、从不归还）与 VM 侧用户页表的池（`servers/vm/pagetable.c` 的 `pt_free` 把页还回 VM 物理分配器所用的同一池，经 `free_mem`）；Rust 的注册槽同时服务内核自举页表（`boot_pt_alloc`，经 `boot_validate_and_prepare` 守卫注册）与 VM 页表（`vm_pt_alloc`）；`register_free` 只对应 VM 侧池。两者成对，未注册归还钩子时销毁路径只清零根、中间页表页泄漏。

扣除那一侧不在 `pt_alloc` 里，而在构造 VM 交接记录时（`VmBootAllocator` 是 VM 侧的启动帧分配器：从每个池区间末端向下消耗，方向与从起点向上的 `BootAlloc` 相反）（`vm_handoff.rs` 的 [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) 调用的 [`fn classify`](<os/kernel/src/vm_handoff.rs:fn classify>)）；引导期建表之后的 Direct Map 覆盖建立则在 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs)：

表头那句注释给出口径："consumed pages and remainder are both kernel-reserved"——扣的是**整段**，不是已用部分。理由在本节末的反向推演里。表里还有一行与 C 的差异需要说明：C 在 VM 模块的字节被复制进地址空间之后把它回收（就是 §2.5.1 表里的"还 VM 镜像"那一行），而**实现**把**所有**启动模块的字节区间都算作内核保留。表头把两份清单分开陈述：扣除记录含 VM 自己；交接模块清单（`VmBootHandoff.modules`）排除 VM 是 Rust 侧的选择（C 的 `kinfo.module_list[]` 本身含 VM，VM 条目在回收后被置零，见 §2.5.1）。理由是启动顺序里的一个 Rust 侧前提：**模块字节留在 blob 原地，VM 启动那一刻才被搬进自己的地址空间**——C 侧不同，C 的注释写明回收发生在 "just copied into existence"（复制完成后），所以 C 能在复制后归还而后续装载只伤到无人再读的 blob 原件。Rust 侧复制被推迟到 VM 启动，清单就必须从任何搬运发生之前就把全部模块 blob（含 VM 自己的）划出：内核的 boot-exec——`kmain` 的引导执行阶段，逐个 `exec` 启动镜像里的服务进程——在 VM 服务起来之前只装载 VM 一个服务器（`EntrySpec::loaded`）；其余启动服务是 `EntrySpec::DEFERRED`，由 RS（重生服务器）运行期 EXEC 装载——它们的段帧同样取自这份自由清单。若按 C 的时点归还 VM 的 blob，后续装载与 EXEC 的帧分配可能盖掉尚待搬运的 VM 镜像字节。

**改为按"已用部分"扣除会怎样**（用 `boot_alloc_used_bytes()` 而不是 `boot_alloc_region()`）。设 bump 区间是 `[0x1E00000, 0x2200000)`（1024 页 = `0x400000` 字节，与 `prepare_boot(storage, 1024)` 的预算同量级），boot 阶段用掉前 100 页（`100 × 0x1000 = 0x64000` 字节），于是游标停在 `0x1E00000 + 0x64000 = 0x1E64000`，而 VM 的账上会剩下 `[0x1E64000, 0x2200000)` 可用。下一次 `map_huge` 分配中间页表页时（调用点可以是 `arch_boot_impl` 的 Step 2/2c 设备窗口，或 Step 4 建立 DM 覆盖）`BumpRegion::alloc_pages(1)` 从 `0x1E64000` 切走一页并把它写零（`write_bytes(pa as *mut u64, 0, 512)`）。这些调用点都在 VM 首次发布自由页之前，所以这次写零不会覆盖 VM 已分配的页；真正的问题是**账本边界**：按已用部分扣除把 bump 区间的剩余部分交给 VM，而 bootstrap 路径仍持有 `boot_alloc_region()`，在 `is_registered()` 为真（`PT_ALLOC != 0`）时仍可能从同一区间取页（例如未来新增的启动期消费者），两者对“这块内存归谁”没有共同的截止点。

整段扣除是一个保守的截止点：bump 池整体不进入 VM 自由清单，代价是**VM 发布前未被接收侧取用的尾部页**（已被接收侧用作页表页的部分仍在使用；整段扣除只是不让 VM 碰这段区间），收益是不需要证明“所有 boot 消费者都在 VM 发布前结束”。
#### 3.4.3 `free_upper_idx` 与页目录槽位：字段、借用链与 64 位做法

页表页由谁发说完，剩下的是页目录槽位的账——C 的 `freepde_start` 是 C 侧借槽链的起点（[`createpde`](minix3/minix/kernel/arch/i386/memory.c:createpde)、[`virtual_copy_f`](minix3/minix/kernel/arch/i386/memory.c:virtual_copy_f)）：`memory_init` 把它自增两次填进 `freepdes[]`，`createpde` 用这两个槽位在当前页表里开临时窗口，`virtual_copy_f` 借窗口做跨地址空间拷贝。

这套机制的三个前提（完整陈述见 [`memory.c`](minix3/minix/kernel/arch/i386/memory.c) 的 `createpde`/`virtual_copy_f` 段）在 64 位侧以三种方式不再必要：① 内核不再借当前页表的槽位，Direct Map 窗口是固定虚拟基址到物理地址的线性映射，不需要“空槽”；② Direct Map 不依赖“当前 CR3/当前 satp 指向哪张根”这个可借用槽位——三架构都实现 `Paging::inherit_supervisor_half`，把源根的高半条目复制进新进程根（三个架构专用名先在这里定义：TTBR1 是 AArch64 承载高半地址翻译的页表基址寄存器；Sv39 是 RISC-V 的 39 位三级页表方案；satp 是 RISC-V 的根页表基址寄存器）：aarch64 的 L0 上半 256–511 项就是 TTBR1 区（[`arm64/paging.rs`](os/arch/src/arm64/paging.rs:fn inherit_supervisor_half)），x86-64 复制高半 PML4 槽位（[`x86_64/paging.rs`](os/arch/src/x86_64/paging.rs:fn inherit_supervisor_half)），riscv64 复制 Sv39 L2 的 256–511 项、satp 切换整根但每张进程根都携带内核半区（[`riscv64/paging.rs`](os/arch/src/riscv64/paging.rs:fn inherit_supervisor_half)）；③ 没有借槽动作，就没有归还义务——跨地址空间拷贝由内核用 Direct Map + 软件页表走查完成（`SYS_VIRCOPY`/`cross_space_copy`），不再借当前页表的槽位。

64 位侧跨地址空间访问换了载体：内核有 Direct Map 窗口（一段固定虚拟基址到物理地址的线性映射，由 `arch_boot_impl` 的 Step 4 建立），加上软件页表走查（[`pte_walk.rs`](os/kernel/src/pte_walk.rs) 一类的实现）就能在**不切页表、不借槽位**的前提下读到任意进程的页表项；用户空间的拷贝则由 VM 服务在自己的地址空间里处理。于是“借用空闲槽位”的三个前提在 64 位侧都不再是必要结构。下文用 `inherit_supervisor_half` 展开的 x86-64 反例是**逐条可对照的一支**；aarch64 与 riscv64 的对应实现已在上一段给出锚点，三支走同一个 trait 方法。

这个值先是被保留成 `Option<usize>` 跨 handoff 携带（`None` = boot-shim 未计算），报告面用 `unwrap_or(0)` 抹平。那一步的教训值得留一句：`Option` 能区分"没有值"（`None`）与"某个值"（`Some(0)`——"0 号槽位空闲"，会被下游当成有效索引），却区分不出"算对的槽号"与"算错一位的槽号"——在没有真实计算者时随便填一个数，只会把后者伪装成前者。

字段删除把权衡推到底：既然所有生产者都写 `None`、唯一消费者把 `Option` 归零，"跨 handoff 携带"就没有携带任何信息——字段从 `KernelInfo` 删除（wire 版本 7→8 的字节数变化见 §3.4；信封字段的完整偏移表在 `handoff_v1.rs` 的 `envelope_offsets`，本篇不复制）。报告面保留 C 的布局位：[`misc.rs`](os/kernel/src/misc.rs) 的 `getinfo_kinfo` 固定填 `0`（C `KinfoStruct.freepde_start` 字段不动 48 字节 wire；GET_KINFO 载荷的收窄演进见 §3.2 的 `[ARCH]` 标注）。64 位侧报告 0 是诚实的——没有借槽机制，"第一个空闲槽位"这个概念本身不存在；C 侧那套机制里偏移一位的后果（改写内核映射、下一次访问页错误或静默读错）见 [`memory.c:createpde`](minix3/minix/kernel/arch/i386/memory.c:createpde)。

对照表：

| 维度 | C 的槽位机制 | Rust 的窗口机制 |
|---|---|---|
| 借用者 | `createpde`（跨地址空间拷贝） | Direct Map 窗口 + 软件页表走查 |
| 借用范围 | 当前页表的 4MB 窗口（一个页目录项） | 内核固定映射窗口（DM 资格过滤后的可达物理区间） |
| 归还时机 | 下一次切换地址空间时自然失效 | 不需要（窗口是常驻映射） |
| 失效条件 | 借来的条目只描述对方的一个 4MB 块 | 窗口覆盖范围由 DM 建立时的资格过滤决定 |

**在 64 位侧照搬"借用空闲 PML4 槽位"会怎样**（PML4：x86-64 四级页表的顶层——页映射表第四级——每进程一个 512 项的顶级目录，上文 C 4MB 单级页目录在它这里对应的是这个角色的放大版）。每进程页表的高半槽位是从 bootstrap 根继承的（`X86_64Paging::inherit_supervisor_half` 把源根的 PML4 的 256 至 511 号槽位（即半开区间 256..512，共 256 项）原样复制过来）。被借用的槽位在 `switch_address_space` 之后属于另一个地址空间：借出去的映射在下一次切换时静默失效，不会报错。症状是跨空间拷贝偶发读到"上一张页表留下的地址"——一个只在特定调度顺序下出现的错误，而不是一次可复现的崩溃。
### 3.5 取件抽象与 crate 依赖方向

这一节把“取件”从引导形态里抽出来：哪些代码属于文件读取、哪些属于 ELF 解析、哪些属于 crate 依赖方向。先给 `FileLoader` 这个测试接缝，再给四份依赖表证明内核不反向依赖 boot-shim 可执行体；引导形态本身不在这里合并。

#### 3.5.1 取件抽象：一条普通函数，差异收在 `FileLoader`

腿、链、形态、交接路径的计数口径见 §3.0 与 §3.6（一条腿 = 一个完整启动路径：一个引导面 + 它交付的内核镜像）。

C 里"同一串函数名、两套实现"的机制见 §2.7：两种架构各自写 `head.S` 与 `pre_init.c`，靠命名与语义约定保持一致，编译期不会检查它们是否真的对应。

U-Boot 腿的引导准备入口是普通函数 `prepare_boot`，不使用单实现 trait 做抽象。理由有两条：没有 `dyn` 分派，`prepare_boot` 是自由函数，不参与对象分派；测试接缝实际由 `FileLoader` 提供，不是任何引导 trait。单实现 trait 既不是接缝，也不是已经验证过的扩展点，留着只会让"换固件"的承诺看起来比证据强。

现在 U-Boot 腿的入口是一条普通函数：[`fn prepare_boot`](<os/boot-shim/src/opensbi_helpers.rs:fn prepare_boot>)，由 [`fn uboot_main`](<os/boot-shim/src/uboot_main.rs:fn uboot_main>) 直接调用，签名是 `prepare_boot<'a>(&'a mut BootStorage, bump_pages) -> BootPrepareResult<'a>`。UEFI 生产入口 `prepare_and_enter_kernel_image` 仍是独立入口（§3.6）；两个直核面复用 opensbi_helpers 的库函数，同样不经过它。未来若真出现第二条固件腿，等第二个实现落地时再按两个真实实现提炼接口，而不是先猜一个。

```rust
// 节选（`report` 字段后的中文旁注为编者加注），os/libs/minix-boot/src/boot_prepare.rs:struct BootPrepareResult。
pub struct BootPrepareResult<'a> {
    pub report: BootReport<'a>, // 〔编注〕借用调用方 storage 的语义视图
    pub root_page: PhysBytes,
    /// Bump region `[start, end)` (physical).
    pub bump: PhysRange,
    /// Handoff arena base (physical), allocated from the same bump pool after
    /// the module landings.
    pub arena_pa: PhysBytes,
}
```

`bump` 是只前进不回收的线性分配器；`bump_pages` 是给页表页预留的页数，`bump` 区间的 `start`/`end` 是该区间的物理边界（展开见 §3.4 与 §4.6）。

差异被压到一个更小的接口上（`loader.rs` 的 [`trait FileLoader`](os/boot-shim/src/loader.rs)）：两个走 `FileLoader` 的 boot-shim 实现族（UEFI 与 U-Boot；UEFI 族含 x86_64/aarch64 两条链）都要拿到"文件字节"，唯一不同的是字节从哪来。

`read_required` 是**内核 ELF 取件**的致命落点：它把"文件不存在"从 `None` 变成一个带路径的 panic。启动模块走 §4.3 的 `read + unwrap_or_else(panic!)`，是第二个致命落点，错误消息格式与 `read_required` 不同；两者都是 fail-closed，`read` 本身只返回 `Option`，不决定致命性。两个实现各自只提供 `read`：UEFI 走 `SimpleFileSystem`（并把正斜杠转成 UEFI 要求的分隔符），U-Boot 走预先落在 RAM 里的 `BootFileTable`。除此之外的装载逻辑（解析、段拷贝、模块页分配、结构体构造）在两个 boot-shim 实现族之间只有一份；两个直核面共用 `bootface_common` 的 `place_modules` 与 `run_self_boot`，`bootface.rs`/`bootface_a64.rs` 只剩本架构常量、入口横幅与各自的 console（下文展开）。两个实现都在 `read` 里只处理字节来源，错误折成 `Option` 交给各自的调用方。

U-Boot 侧的实现见 §4.3 的 `entry_path_eq` 与 `UbootFileLoader`；两条 `read` 的差别在字节来源，U-Boot 侧的生产者在 `build_shim_table` 里按最后一个 PT_LOAD 的文件尾修剪槽长度（kernel 槽 2 MiB、普通模块槽 1 MiB、`mfs` 专用 16 MiB；超过槽容量时打印 `truncating` 后按槽容量写入长度，即**截断后继续**，不是停机），loader 的二次修剪对已在槽内的输入是 no-op；非 ELF 或畸形输入由 `elf_file_end` 返回 `None`，调用方保留整槽长度，不做修剪；`read_required` 与全部装载逻辑完全共用。范围要说准：这里的“只有一份”指**两个 boot-shim 实现族之间**；两个直核面不走 `FileLoader`，但共用 `bootface_common` 的 `place_modules` 与 `run_self_boot`；入口壳只负责常量、横幅与 console 选择。

`bootface_common` 不是只把函数搬了个家，它把两个直核面共同的模块放置约束集中到一处：`NAME_SLOT = 16`、`NAME_POOL` 是 12 个 16 B 槽，`place_modules` 检查条目数 ≥ 12（一条断言；有效条目不足时 fail-closed 返回 `None`）、每槽 basename 与 `NAME_POOL[i]` 相等、长度非零、源区间整段落在 memmap 的单个区间内、源区间不与 bump 池相交、目标页不落内核跨度；通过后才按槽分配 bump 页、拷贝、清页尾、写入名字池。

另有编译期 `const _` 把 `NAME_POOL[i]` 与 `boot_shim::loader::MODULE_NAMES[i]` 逐字节比较；由于两边由同一列表生成，它只能拦住池内截断/NUL 一类构造错误，顺序互换会同时改变两边。名字顺序的守卫在运行期 `place_modules` 的 basename 核对。`PlacementPool` 存在的理由不是打包参数：五个同型字段（名字池地址、bump 起点、bump 终点、内核起点、内核终点）互换后仍能编译并产生“看似合理”的放置，结构体的命名字段把这类调用点错误挡在编译期。

两条调用腿的差异只落在 `PlacementPool`（名字池地址、bump 起止、内核跨度）与 `alloc_pages`/`say`/`say_hex` 回调上：riscv64 用 `0x8200_0000..0x8400_0000`，aarch64 用 `0x4400_0000..0x4600_0000`，容量同为 32 MiB。模块放置之外的流程（DTB 校验、memmap 收集、表址闸、bump 取页、平台源、参数缓冲、`KernelInfo` 组装）也在同一份 `run_self_boot` 里，两壳不再各存一份顺序相同的代码。

去掉 trait 后，有代码或计数证据的收益有两条：

- **可测试性来自 `FileLoader`**（代码证据，测试形态成立）：`FileLoader` 是 trait——[`loader.rs`](os/boot-shim/src/loader.rs) 的 tests 里有内存表实现 `MockLoader`（`impl FileLoader for MockLoader`），涉及装载的单元测试都经由它喂文件字节。
- **条件编译的落点收敛**（按构建形态成立）：**固件选择**相关的 `cfg` 只出现在 [`lib.rs`](os/boot-shim/src/lib.rs) 的模块与再导出声明处（7 处 `#[cfg(feature ...)]` 属性：6 条 `uefi` 加 1 条 `opensbi`）。另有一组 `qemu-test-kernels` gate（10 处）在 `uefi_helpers.rs`，只门控测试内核辅助物（`UefiPoolAllocator`、`TestMapTables`、`build_memmaps`/`build_memmap`、`exit_boot_services`、`classify_map_bytes_into_test_tables`、`exit_boot_services_and_classify`）；生产构建不链接它们。因此准确说法是"固件分支不在业务逻辑里"，不是"整个 crate 只有 7 处 feature 属性"。

另外两条只算单腿事实或设计承诺，不并入上面的收益：

- **入口形状固定**（只在 U-Boot 生产腿成立）：`prepare_boot` 的 `storage`/`bump_pages` 语义与 `BootPrepareResult<'a>` 的返回形状由签名钉住；但"统一"的对象只有这一条腿，UEFI 与直核面各有自己的入口，不能把它说成跨固件统一。
- **换固件不动内核**（设计承诺，观察，尚无自动化反例）：新增一种固件形态，改动是 boot-shim 侧的新入口或共享库调用，不需要预先实现一个 trait；`kernel.elf` 不必重新构建（两者分属 `os/kernel-image` 与 `os/boot-shim` 两个产物，这条分界的后果在 §3.6 与 02 篇的 4.5 节）。边界要说准：boot-shim 屏蔽的是**固件差异**，板级差异仍落到内核侧的平台发现（[`minix-platform`](os/libs/minix-platform/src/) 的 `PlatformDesc` 与内核侧平台源解析），不能把它说成"新板子只需写 boot-shim"。

这里故意不列"类型安全"与"封装"——这两条常被记在 trait 头上，实际来自 feature 门控：`uefi_helpers` 与 `opensbi_helpers` 各由 [`lib.rs`](os/boot-shim/src/lib.rs) 的 `#[cfg(feature = ...)]` 门控，开启哪个 feature 就只有哪个固件类型存在，**写错固件名本来就编译不过**。UEFI 生产入口也没有借 trait 做抽象：[`fn main`](<os/boot-shim/src/main.rs:fn main>) 按具体类型写——`prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)`。这不是把固件写死在产品里：boot-shim 这个可执行文件只在 UEFI 形态下构建（bin 的 `required-features = ["fw-uefi-image"]` 隐含 `uefi`），固件选择发生在"构建哪个二进制"，不是运行期分派；opensbi 形态的生产路径是 riscv64 U-Boot shim（§3.1.1/§3.6）的 `opensbi_helpers::prepare_boot`；`kernel-image` 自引导面是辅助/嵌入式形态，复用同一组库函数但不经这份 main.rs。

**OpenSBI 路径的两种角色要分清**（接线状态与覆盖脚本的权威表在 §3.1.1 接线状态表，这里只讲与引导入口有关的部分；本篇用“OpenSBI 路径”指经 `opensbi_helpers` 的腿，§4.4 的字段表里特指 U-Boot shim 腿，不把两个直核面算进来）：U-Boot 路径复用 boot-shim 库根的 `BootStorage`/`BootReport` 与 opensbi_helpers 的 `BootFileTable`、`bump_alloc_pages`，并调用 `prepare_boot`；`kernel-image` 的直核自举面是另一条机器状态，不经它。角色一：U-Boot shim 的引导入口，调用 `prepare_boot`；角色二：`kernel-image` 的直核自举面，同镜像直接走 `arch_boot`，不使用这个入口。memmap 源统一下沉的裁决不合并引导形态层——独立 shim 与 bootface 自举对应不同机器状态，`prepare_boot` 只承担前者的固件差异。
#### 3.5.2 crate 依赖方向

C 的引导链同样不止一个二进制：BIOS 场景有 bootblock 与 boot2 两级引导器（[`boot2.c`](minix3/sys/arch/i386/stand/boot/boot2.c)），GRUB 场景由 multiboot 引导器代劳——引导器与内核不是链接成一个二进制，而是各自链接出独立的可执行文件，运行期只隔一条 multiboot 协议（寄存器递 `mbi` 指针）。内核侧的引导准备 [`head.S`](minix3/minix/kernel/arch/i386/head.S) 与 [`pre_init.c`](minix3/minix/kernel/arch/i386/pre_init.c) 则编在内核镜像之内，`pre_init` 填的 `kinfo` 本来就是内核自己的全局变量。所以 C 无需回答"引导层与内核谁依赖谁"：引导器与内核的边界由协议承载，协议两侧没有编译期关系。

Rust 侧的问题形态不同：boot-shim 独立成产物，却要与内核共享类型定义（`KernelInfo` 与交接的返回形状），这份复用由 crate 依赖表达——"谁依赖谁"第一次成为必须显式回答的构建问题，且答案必须单向：

依赖关系如下：`kernel-image` 在 riscv64/aarch64 target 下依赖 `boot-shim`；`kernel-image` 与 `minix-kernel` 依赖 `minix-boot`。这两条是 crate 依赖。调用面另算：`boot-shim` 调内核的 `enter_kernel_image_v1` 与 `boot_alloc::init_boot_pt_alloc`；`arch_boot` 是 `kernel-image` 两个直核面的入口，不由 boot-shim 调用。两个名字看起来相近，但指的不是同一个东西：**boot-shim** 是可执行体 crate，**minix-boot** 是共享库 crate——本节论点里"内核不依赖 boot-shim"说的是不依赖前者这个可执行体；`minix-kernel → minix-boot` 是对库的依赖，二者不矛盾。

依据是四份依赖表（boot-shim / kernel / minix-boot / kernel-image 四个 manifest；命令：`sed -n '/^\[dependencies\]/,/^\[/p' os/kernel/Cargo.toml`（输出中无 `boot-shim` 也无 `uefi`，即证否定；全仓四 manifest 的依赖表见下方四行（`os/kernel` 的 `bitflags` 从略））。文中另提到的 `minix-elf` 是 boot-shim 的普通依赖，不单列成表）：

- `os/boot-shim/Cargo.toml` 的 `[dependencies]` 里有 `minix-types`、`minix-boot`、`minix-arch`、`minix-kernel`、`minix-plat`；`minix-elf`（共享的 ELF 解析器）；非可选的 `fdt`（U-Boot 腿解 DTB）；`uefi = "0.33"` 与 `uefi-raw = "0.9"` 挂在可选的 `dep:uefi`/`dep:uefi-raw` 上，由 `uefi` feature 一并打开。
- `os/kernel/Cargo.toml` 的依赖是 `minix-arch`、`minix-plat`、`minix-platform`、`minix-types`、`minix-boot`——**没有 boot-shim，也没有 uefi**。两个名字相近的 crate 分工不同：`minix-plat` 是板级/机器相关的实现层（早期控制台、中断控制器一类与具体硬件打交道的代码，[`early_console.rs`](os/plat/src/early_console.rs)），`minix-platform` 是平台描述符的解析与分发层（`os/libs/minix-platform/`，面向 ACPI/DTB 这类固件描述）。
- `os/libs/minix-boot/Cargo.toml` 只依赖 `minix-types` 与 `fdt`。
- `os/kernel-image/Cargo.toml` 依赖 `minix-kernel`、`minix-boot`、`minix-arch`、`minix-plat`、`minix-types`。Cargo 的 `[target.'cfg(target_arch = "…")'.dependencies]` 语法允许一条依赖只在指定 target 架构下生效，kernel-image 用它给 riscv64 与 aarch64 各加了两条：`boot-shim`（只开 `opensbi` feature，`uefi` 不开）与 `fdt`（解 DTB 用）——只有以这两个架构为 target 构建时它们才生效。
  - 符合本项目设计目标的方式（独立引导器 + 内核本体两个产物）里，"内核不依赖 boot-shim"对 `minix-kernel` 与 `boot-shim` 成立——引导器依赖内核、内核不回头依赖引导器。这一分工的接线状态见 §3.1.1 的接线状态表（两条 UEFI 产线已跳独立镜像；riscv64 的门链里，U-Boot shim 的六路标门 `test-riscv64-uboot-shim.sh` 注册在 `os/qemu-tests/run_all.sh`；kernel-image 的直核自引导门 `test-kernel-image-riscv64-bootface.sh` 是手工门，不在 `run_all.sh` 中——接线归属以该表为准）。
  - `kernel-image` 的情况不同：它是自引导镜像（引导面 `bootface` 长在自己体内，§3.1 例外条款），riscv64/aarch64 目标下按上面那两条 target 表反向依赖 boot-shim 的**库**（取件表、memmap 源等函数）——依赖的是库而不是引导器产物，单向边界不因此破例（道理见问题二）。这张 target 表就是终态：memmap 源下沉到 `minix-boot` 之后，反向依赖的仍是库、不是引导器产物，依赖方向没有变化。

这条单向边界回答的是"共享类型住哪"，与"哪种固件能编译"是两个独立的问题，别混在一起：

**问题一：共享类型为什么住 `minix-boot`**。真正**跨两侧**引用的是 `KernelInfo`（接收侧构造、内核读）与 `BootReport`（`arch_boot`、`enter_kernel_image_v1`/`prepare_kernel_image_v1` 的发送视图）；`BootStorage`/`BootPrepareResult` 是引导侧的接口类型。放在同一库是为了让这些签名不把 boot-shim 可执行体或固件实现拖进内核依赖面，而不是因为“内核完全不引用引导侧类型”。若 `KernelInfo` 定义住在引导层，内核引用它就得依赖 boot-shim——反向依赖、且把固件实现拖进内核依赖面；若住在内核里，共享库 `minix-boot` 就得反向依赖 `minix-kernel`，与已存在的 `minix-kernel → minix-boot` 成环（boot-shim → minix-kernel 本身不是环——依赖图里本来就有，boot-shim 调 `enter_kernel_image_v1`）。所以这层定义下沉到不含 UEFI/SBI 固件实现、但共享参数归一化与 DTB 解析等实现的 `minix-boot`（依赖表是 `minix-types` 与 `fdt`），谁依赖它都不会被拖进固件代码。这条边界不是风格洁癖：`os/kernel/Cargo.toml` 的依赖表里没有 boot-shim、也没有 uefi，用上面那条 `sed` 就能一次验证。

**问题二（完整形态论证归 §3.1 例外条款与 §3.6；此处只给与依赖方向有关的结论）：riscv64/aarch64 上的引导面为什么长成"内核镜像自举"**。背景见 §3.1 表后的引用段：RISC-V 与 ARM 的 `-kernel` 极简装机（QEMU `-kernel` 模拟的是真实硬件的这种装载方式）下，下一级镜像直接进 RAM、固件跳转，固件与内核之间没有"装载与被装载"的分界，独立 boot-shim 没有存在位置。

于是两个架构的引导准备各自由 `kernel-image` 自带的引导面承担——riscv64 是 `bootface`（[`bootface.rs`](os/kernel-image/src/bootface.rs)），aarch64 是 `bootface_a64`（[`bootface_a64.rs`](os/kernel-image/src/bootface_a64.rs)），流程由 `bootface_common::run_self_boot` 承载（DTB 解内存 → 取件表 → bump 池（bump 池 = 一段只前进不回收的线性分配器：从区间起点逐页发放，游标只增不减，耗尽即拒绝）→ 模块 → `BootReport<'_>` → 同一个 `arch_boot`；接收侧落 `.bss` 后构造 `KernelInfo`）。两者都复用 boot-shim 的**库函数**，但不是独立引导器产物；这一形态的固件对话走 DTB 与 PSCI/DMB（DMB = 数据内存屏障）一类的裸协议，`kernel-image` 与 `minix-kernel` 两个 crate 都不依赖 `uefi`，单向边界原样成立。

它的定位是**平行的装机面**：与 UEFI 独立 shim 共享同一套取件库；内核引导收敛层按腿分两处（直核 `arch_boot_finish`、跨镜像 `arch_boot_resume_v1`，最终同落 `jump_to_kmain`，见 §3.6/02 篇的 4.5 节）——统一发生在层内（类型、取件库、收敛层），引导形态层不造伪统一（memmap 源统一下沉的裁决，三形态对照见 §3.6）。

**如果让 `minix-kernel` 直接依赖 boot-shim 会怎样**（动机多半是"想复用 `BootPrepareResult` 的定义"）：复用类型走 `minix-boot` 即可，不必依赖引导层（问题一已答）。剩下的否决理由是本节论证的单向依赖原则本身：**内核不该认识固件形态**——屏蔽固件差异正是 boot-shim（或 bootface）存在的意义。"riscv64 编译不过"（UEFI 目标只有 x86_64/aarch64/i686，riscv64 暂不支持）不构成独立论据，换 opensbi feature 就能过；否决理由从头到尾只有依赖方向一条。
### 3.6 五条生产腿与发送侧（接缝定义）

本节用“五条生产腿”一个主计数；其余计法的换算表在节末。

先把四个容易混的入口按调用关系排成三行，后文只在这个骨架上加细节：

```text
直核面：arch_boot → arch_boot_finish → arch_boot_impl → jump_to_kmain
v1 接收：arch_boot_resume_v1 → resume_additive → jump_to_kmain
v1 发送：prepare_and_enter_kernel_image → enter_kernel_image_v1
```

这是骨架，省略了 `resume_after_v1_decode` 等中间层；U-Boot 发送腿从 `prepare_boot` 进入同一组发送接口。

C 里没有"交接"这个问题：内核是唯一的二进制，`pre_init` 返回的指针就是同一个地址空间里的地址，`head.S` 的 `call kmain` 是同一次调用。

（下文反复用到 AArch64 的 TTBR0/TTBR1：它们是两张页表基址寄存器，分别承载低半与高半地址的翻译表；切换地址空间只换 TTBR0，TTBR1 在上下文切换中不被替换。）

Rust 侧的收敛点分两处：直核家族收敛在 `arch_boot_finish`（`arch_boot_impl` 的调用在其体内；三个 cfg 分支的通用实现锚点见 [`fn arch_boot_impl`](<os/kernel/src/lib.rs:fn arch_boot_impl>)），跨镜像三腿的接收侧收敛在 `arch_boot_resume_v1` → `resume_additive`。三条对外入口分别是 `arch_boot`（直核/测试）、`enter_kernel_image_v1`（跨镜像 v1 发送，三条腿：两条 UEFI 链 + riscv64 U-Boot shim；它不在本镜像内收敛，终点是跳独立镜像的入口指令）与 `arch_boot_resume_v1`（接收）：

x86_64 的 `arch_boot` 只做诊断打印并调用 `arch_boot_finish::<X86_64BootArch, X86_64HigherHalf>`；`arch_boot_impl` 的调用在这个 helper 内，不在 `arch_boot` 的函数体里。

签名里的参数就是交接的全部内容：一份借用的启动视图（`&BootReport<'_>`）、它声明的 RAM 权威，以及一个根页表页的物理地址；接收侧入口立即把这些切片落进 `.bss`。返回值是 `!`——它不返回。

跨镜像交接的产线上多一层（x86_64、aarch64 与 riscv64 U-Boot shim 三条 v1 发送腿同走 `lib.rs` 的 [`fn enter_kernel_image_v1`](<os/kernel/src/lib.rs:fn enter_kernel_image_v1>)；TTBR0/TTBR1 是 aarch64 侧的细节）：内核以独立 ELF 的形式存在于高半（aarch64 侧经 TTBR1），boot-shim 先分配一块八页 arena 并把 v1 协议写进去，然后建表、开分页、把运行期字段补进 arena，最后**绝对跳转**到入口；内存图来源分两路：两条 UEFI 腿在 `ExitBootServices` 的配对循环里分类最终快照，riscv64 U-Boot shim 没有 EBS，直接从 DTB `/memory` 取表；入口寄存器只带 arena 基址；三条腿各自用哪个寄存器、x86_64 的 trampoline 如何转交，见 §4.1 的入口约定。

「虚拟入口」与「物理 arena 基址」这两个值在跳转那一跳上同处。此前它们是一对相邻的裸 `u64`（`-> (u64, u64)` 与 `jump_kernel_image_v1(entry: u64, arena_pa: u64)`），交换后仍能编译：跳转目标会变成 arena 的物理地址，分页刚开后第一次取指就故障，而 `scause`/`CR2` 指向的地址与真实原因无关。现在改由一个值对象按字段名承载。`PreparedJump`、`prepare_kernel_image_v1`、`jump_kernel_image_v1` 都是 crate 内私有实现，唯一公开入口是 `enter_kernel_image_v1`；它不是 wire 类型，只在本仓两处相邻调用之间传递。

三处具体事实：

- 交接载荷带 `bump_next`。原因在 §4.6 与 `BootAlloc::resume` 的文档里：两个镜像各链接一份 `BootAlloc`，接收方若从区间基址重新发页，会把 boot-shim 已经用作页表页的页再发一次。两条 UEFI 腿的 arena 由 UEFI `AllocatePages` 单独分配、不占 bump 池，因此没有"载荷页"要跳过；U-Boot shim 腿的 arena 由 `bump_alloc_pages` 从 U-Boot 可用内存中模块落点之后的区域另行分配（它与 `BootAlloc` 是两个独立游标，见 §4.2.1 的两个分配器说明）；其起点不低于 `bump_end`，因此在 1024 页 `boot_alloc_region()` 区间**之外**；arena 的 startup-object 身份随 startup 分区退场（§3.2.1），arena 的归属由下面的生命周期合同单独成立；这里的“same pool”指同一片 U-Boot 可用物理内存，不是同一个游标：shim 用 `bump_alloc_pages` 从模块落点之后发 arena，接收镜像的 `BootAlloc` 只续用页表子区间，两者游标独立。扣除记录没有单独排除它，而是按生命周期合同处理：解码是 arena 的最后一次读，VM 自由页在那之后才发布，因此这些帧可以重新发给 VM（§4.6 的 poison 测试钉住这条时序）。
- `in("x0") arena_pa`：arena 基址绑在固定的第一参数寄存器上，而跳转目标放在任意寄存器。代码注释给出了理由——若在 asm 块里写隐式 `mov x0, ...`，寄存器分配器可能把 `entry` 也分配到 `x0`，两条约束顺序不定。
- 接收侧由 `arch_boot_resume_v1` 先把六个分区直接解码进六个 `.bss` 落地区；随后 `resume_additive` 的 `store_kernel_info` 包装跳过字节拷贝（源与目标同址），但仍构造 `KernelInfo` 并写 `KERNEL_INFO` 槽；顶部一次性守卫使第二次调用 panic（§4.5），再只做 additive 阶段（接收壳的分派次序见 02 篇的 4.5 节，这里不重复）：

additive resume 的 Step 4 就是建立 Direct Map（02 篇 §3.6/§4.5 只确认它在跳转前完成；完整机制归独立内存管理主题）；本篇 §4.8 只保留启动期概念入口，这里只保留时序位置。`new_from_page` 的真实签名是 `new_from_page(root_page, channel: PteChannel)`：根的清零与后续 PTE 访问必须走调用方声明的写视图——bootstrap 建根用 `PteChannel::Identity`（Direct Map 尚未建立），运行期进程根用 `KernelDm`，VM 自有根用 `VmDm`。这个参数不是装饰：三个通道只是同一根的不同写入口。

riscv64 上是**直核自引导形态**的出生地：内核镜像自己就是引导体——在没有 U-Boot 与文件系统的 `-kernel` 场景里，入口是 [`main.rs`](os/kernel-image/src/main.rs) 按架构分道的 `rust_image_main`，riscv64 的实现是 [`fn bootface`](<os/kernel-image/src/bootface.rs:fn bootface>)（aarch64 的对应面是 `bootface_a64.rs` 的 [`fn bootface_a64`](<os/kernel-image/src/bootface_a64.rs:fn bootface_a64>)，只在该架构构建时编译），从设备树解析内存、按 `BootFileTable` 取模块——模块来源只有表这一条，表的**地址**只有 DTB `/chosen` 的 `opensbi,boot-file-table` 属性一条通道（仓库内六个脚本——`test-atf-riscv64`、`test-cmd-smoke-riscv64`、`test-kernel-image-riscv64-bootface`、`test-riscv64-boot-full`、`test-smp-aps-aarch64`、`test-smp-aps-riscv64`——都用 `fdtput` 注入表址）。

此前的备用 `a2` 参数通道在启动裁决中删除（仓内从未有生产者：QEMU/OpenSBI 只约定 `a0`/`a1`）；表址先过 `in_memmap_range` 再过魔数，失败即 halt。细节在 `bootface.rs` 的入口段，本篇不展开。

U-Boot shim 腿自建表、不经过此通道——它用自建表取件，把模块描述表（`BootModule`：name/start/len）与 memmap 等载荷序列化进 arena；`BootFileTable` 与模块字节留在物理内存；arena 交给接收镜像的 `arch_boot_resume_v1`；调 `arch_boot` 的是两个直核面。

U-Boot shim 路径落地后也补齐了**跨镜像跳转形态**的完整链：`riscv64-uboot-shim`（物理链接，VMA=LMA，自带 bootstrap 栈）吃下 U-Boot `bootelf -p` 的 `(extra_argc, extra_argv)` 参数通道，把内核 ELF 与 12 个引导模块准备成 `BootHandoffV1` arena，DTB 以 `PlatformDescSource` 指针随信封交付，交给配套特性 `fw-riscv64-uboot` 构建的 `kernel-image` 接收镜像走 `arch_boot_resume_v1`——接收侧与 aarch64 同构，复用同一个加性恢复序列，不重建根表。riscv64 的 `jump_to_kmain` 因此带一个执行位置探针：直核面仍在低地址恒等窗口里执行，medany 代码模型的 PC 相对 `la` 解析出的是装载地址，要加上镜像的链接↔装载偏移；接收侧已被 shim 跳到高半链接地址，`la` 解析的已是真值，偏移不得再加（两节分工：02 篇 §2.2 与 §2.4 讲 C 侧的两份 `pg_utils.o` 与 `pagedir`，本节讲 Rust 侧为何禁止重建活动根；不合并）——同一段 asm 用 `auipc` 读当前 PC 与 `kern_virt_base` 比较后分道。五条内存来源（两条 UEFI 链的分类结果、U-Boot shim、两个直核面）交付的形状统一为两张表，收敛点与各腿的产出方式见 §3.2；引导形态层本身不合并。

**两个另一做法各会撞什么**（两处）：

其一，aarch64 退回内联执行。内联时内核代码住在 PE 镜像的运行物理地址上（低半、`VA = PA`、在 TTBR0 覆盖范围内）。第一次 `switch_address_space` 会换 TTBR0 到进程页表并发 `tlbi vmalle1is`，内核自身的取指映射随之消失——现场证据（`elr`/`vbar` 钉在同一个低地址上）外包给 [`rewrite-notes/01-stage-kernel/01-code-design.md`](01-code-design.md)；本篇只登记这条指向，不自称独立可核。跨镜像形态把这个问题的根去掉：内核代码链接在高半（TTBR1），而 TTBR1 在上下文切换中不被替换。

其二，接收侧按"干净启动"重建 bootstrap 根（调 `Paging::new_from_page(root_page, channel)`）。该函数的第一件事是把 512 个根条目清零（`write_bytes(ptr, 0, 512)`），而此时这个根**正在被使用**——承载当前函数的那条指令所在的 L0 槽就在其中。清零本身不会立刻作废已缓存的 TLB 项——三个架构都不因改写页表而自动清 TLB——故障落在下一个**冷**取指上（TLB 回填或切换后的 kmain 入口取指）：之前靠 TLB 残留还能跑几条指令，这段"还能跑"的窗口让故障点与根因隔得更远，栈上多半连一条 panic 消息都来不及打印；"resume 面只做 additive 阶段"这条约束就是从这里来的。

**直核自引导在真板上的位置** `[ARCH: FW_PAYLOAD 直核形态（Minix3 无对应）]`。`FW_PAYLOAD` 直核形态的规范背景与真板意义见 §3.1 例外条款段（一句话版：固件打包下一级镜像直接跳 S 模式目标，QEMU `-kernel` 模拟同一时序）。三架构的落位并不对称：

| 架构 | 直核形态的位置 | 原因 |
|---|---|---|
| x86_64 | 无对应形态 | 平台规范强制 BIOS/UEFI 作为唯一引导面，没有"固件直接跳任意 payload"的规范形态 |
| aarch64 | 覆盖测试为主 | QEMU `-kernel` 是主力验证通道；真板启动一般仍经 UEFI/U-Boot |
| riscv64 | 测试与真板嵌入式并存 | `FW_PAYLOAD` 是规范形态，真板极简装机直接使用；`-kernel` 是它在 QEMU 的对应物 |

所以直核自引导是**辅助启动路径**，不是三架构的主线：主线是"独立 boot-shim + 独立内核镜像 + 交接载荷"（§3.1 的分工表；各路径的当前接线状态见 §3.1.1 的接线状态表）。这张矩阵同时解释了 `bootface`/`bootface_a64` 为什么保留而不作主产线——它们服务的是特定嵌入式形态与测试通道。

**启动路径计数的换算基准（本节末表）**。

> **术语对账**：本篇用多个不同维度描述同一组启动路径。**本表是全文启动路径计数的换算基准**；各节可以在服务上下文时复述成员名单，但跨节比较与换算是以本表为准，不另立计数。正文只以“五条生产腿”为主计数；其他计法只在各自引用的节里使用。
>
> **“生产腿”的判据**：当前源码里接线的完整启动路径，不等同于“主线部署形态”。五条生产腿 = 主线 3（两条 UEFI 链 + riscv64 U-Boot shim）+ 辅助/测试 2（riscv64 `bootface`、aarch64 `bootface_a64`）；直载测试链不计入生产腿与“四条有门链的腿”，但在“三类链 6”与“真机门链 5”两类计数里出现。§3.1.1 的“测试通道”指用途，不否定它是一条接线腿。
>
| 计法 | 数量 | 成员 | 用在哪 |
|---|---|---|---|
| 形态 | 3 | 同镜像内调用（已退役的测试形态）/ 跨镜像跳转（三条 v1 腿）/ 直核自引导（两个直核腿）；直核自引导也在同一镜像内，但自带引导面并自己建根，与已退役的内联执行不同 | §3.6 |
| 生产腿（主计数） | 5 | 两条 UEFI 链 + U-Boot shim + riscv64 `bootface` + aarch64 `bootface_a64` | 正文、§3.1.1 |
| 协议段/交接面 | 4 | v1 发送面 / v1 接收面 / riscv64 自引导 / aarch64 direct-core；send/receive 是同三条 v1 腿的两端，不是四条腿 | 02 篇的 4.5 节 |
| 三类链 | 6 | UEFI 一族 2 条 + U-Boot 一族 2 条（shim 生产腿 + 直载测试链）+ 直核面 2 条；其中 5 条是生产腿，直载测试链不是 | §3.6 |
| 来源分组三组 | 3 | UEFI 组两条、U-Boot 组一条、直核组两条；与“五条生产腿”同成员，只是按取件来源切 | §3.2 |
| 架构×链五行 | 5 | 五条生产腿各一行 | 02 篇的 4.5 节 |
| 真机门链五条 | 5 | 按脚本集合，成员以 §5.4 的表格为唯一权威（本节不重复名单）；不含 aarch64 直核面 | §5.4 |
| 有启动门链的四条腿 | 4 | x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 `bootface`；是“五条生产腿”去掉 aarch64 直核面，也是“五条真机门链”去掉直载测试链 | §5.1 |
>
> “产线”是链的别称，不单独计数。两个“5”成员不同：架构×链五行含 aarch64 直核面、不含直载测试链；真机门链五条含直载测试链、不含 aarch64 直核面。§4.1 的阶段总览表只按 UEFI / 非 UEFI 两列讲阶段，不是独立计法。
### 3.7 所有权与校验门链（I3/I4）

I3 与 I4 在实现上是两条相邻的检查：**载荷先落到自己的存储，再发布；接收侧校验在消费（建表/映射/分配）之前完成**。发送侧的建根只依赖装载/分类门已经保证的报告，不在接收侧门链内（§5.1 的 I4 行）。

- “消费前”不等于“发布前”：直核面由 `arch_boot_finish` 先 `store_boot_report`、再进 `arch_boot_impl` 的 `boot_validate_and_prepare`；v1 接收面在 `resume_additive` 里先 `boot_validate_and_prepare`、后 `store_kernel_info`。
- 本节的“发布”指把落地后的 `KernelInfo` 写进 `KERNEL_INFO` 槽（此后 `kernel_info()` 可读）；“消费”指建表/映射/分配。VM 自由页的“发布”是另一件事（§4.6）。这两组腿的包装调用次序相反。
- v1 接收侧有两个容易同名的事件：六个分区在 `arch_boot_resume_v1` 里先解码进 `.bss` 落地区（发生在 `resume_additive` 的 additive 阶段之前）；`store_kernel_info` 是之后对同一组地址的包装调用（源与目标同址，跳过复制）。

因此“载荷先落盘”对两族都成立；“store 与 boot_validate 次序相反”只指包装调用。

所有权按腿分：

- `BootStorage` 拥有 U-Boot shim 腿上的六类载荷；UEFI 腿只拥有五类（四个 `fill_*` 调用：`fill_map_tables` 一次提交两表；`fill_source_records` 在 U-Boot shim 与两个直核腿使用，UEFI 腿不走它）。
- 两个直核腿各拥有一份栈上的 `BootStorage`，六类载荷齐全（包括经 `fill_source_records` 写入的 source 记录）。
- UEFI 腿的 source 分区由两个阶段直写 wire：分类器先写 UEFI 记录；EBS 配对循环在 `authorize_and_copy` 返回 `OwnedDtb` 后调用 `resv_claims::capture` 生成 DTB `/reserved-memory`/MRB 记录，再经 `push_dtb_sources` 追加并二次 `finish_sources()`。storage 的 source 切片为空；strict 审计按 `authority` 过滤，因此这批 DTB 记录不计入 UEFI 权威的覆盖。
- `fill_modules`/`fill_platform`/`fill_param`/`fill_map_tables`/`fill_source_records` 各自在闭包返回时提交计数，族内半提交状态不可表示；跨族半提交（例如只填了 modules 未填 map）由接收侧的 `memmap`/模块/来源覆盖检查兜住。

`BootReport<'a>` 只是借用视图；跨镜像发送侧把它序列化进 wire，接收侧在自己的 `.bss` 重建切片。接收侧先把切片深拷进自己的 `.bss`，再构造唯一的 `KernelInfo`；`'static` 只由这些 `.bss` 落地区支撑，不从堆上“泄漏”出来。UEFI 腿的 source 记录由分类器与 DTB claims 两阶段直写 wire，不经过 `fill_source_records`；因此“第二次 fill 会 panic”只对实际走 `fill_*` 的腿成立。

校验的逐腿顺序（§4.4.1 的权威表）：

- 直核面：`validate_requiring_sources` → `validate_before_tables` → `check_boot_postconditions` → `store_boot_report` → `boot_validate_and_prepare` → `kmain` 的 strict `validate` 与 `check_boot_postconditions` 复核。
- v1 接收面：`validate_requiring_sources` → `check_boot_postconditions` → `resume_boot_pt_alloc` → `boot_validate_and_prepare`（在 `resume_additive` 内）→ `store_kernel_info` → `establish_boot_dm` → `jump_to_kmain` → `kmain` 的 strict `validate` 与 `check_boot_postconditions` 复核，不经过 `validate_before_tables`。

`check_boot_postconditions` 在 v10 之后只剩 `kern_virt_base > u32::MAX` 一条断言；镜像与模块、模块与模块的半开区间重叠在共享校验体 `validate_view` 里，三档都会跑。直核面的 `arch_boot_finish` 在 `store_boot_report` 之前另有一条可信链断言：记录非空时，`authority` 必须与至少一条记录的 `input_object` 相同；这把“声明”与“记录”在发布前绑在一起（§4.4.1 时间序表第 4 步）。

五类错误各有落点：缺来源记录 → `FreeSpanWithoutMemorySource`；权威与记录不匹配 → “declared RAM authority … has no source records”；重复提交 → 走 `fill_*` 的腿第二次调用 panic；半提交（写而未提交）在现行 API 中不可表示；跨镜像指针 → 接收侧读到零或 fault。

---

#### 失败对照

如果把 boot-shim 与内核并成一个 UEFI 二进制，内核就得认识固件类型、把 `uefi` 拉进自己的依赖表，与“内核不依赖引导层”的单向边界冲突。所以 UEFI 形态被否决，直核形态只允许 `kernel-image` 反向依赖 boot-shim 的库函数，而不是依赖引导器产物。

#### 最小示例

```bash
# 在 os/ 下跑交接库的宿主测试；wire 与字段台账的形状由这一包守住（§5.1）
$ cd os && cargo test -p minix-boot --lib
```

预期全部通过；§5.1 把 wire 的 encode/decode round-trip 与 `KernelInfo` 字段台账列为 I1 的宿主证据。

#### 小结

- 协议换代把“谁读文件、谁填内存图、谁退出固件服务”重新分工；UEFI 腿自己读，U-Boot 腿由 U-Boot 代劳。
- `KernelInfo` 从全局地址改为按值交接；三条 v1 腿走 wire，两个直核腿同镜像也深拷。
- 内存账从 `kinfo` 数组改为两张切片表加 `SourceRecord`，每个投影字节可追溯。
- bootstrap 区间与页表分配器在进入内核前换手；没有“分页后可回收的内核段”这个对象。
- 五条生产腿（两条 UEFI、U-Boot shim、两个直核面）收敛数据形状，不合并引导形态层；门链按腿不同：直核面多一道建表前门，v1 面跳过它，两者都在 `kmain` 做 strict 与 postconditions 复核。

#### 术语

- **wire**：跨镜像交接协议里按固定字节宽度写进 arena 的表示；v1 信封带版本号，收发两侧精确比较。
- **arena**：三条跨镜像 v1 腿交接用的八页物理区，承载信封、分区载荷与运行期字段。
- **v1 腿 / 直核面**：前者跨镜像（两条 UEFI 链、riscv64 U-Boot shim），后者在同一镜像内直接调 `arch_boot`。
- **`BootStorage`**：发送侧自有的定长载荷存储；它拥有 U-Boot shim 腿上的六类载荷；UEFI 腿的 storage 只拥有五类，source 分区由分类器与随后的 DTB claims 两阶段直写 wire（见 §3.7）。
- **`SourceRecord`**：记录“这段内存是谁给的”的来源条目，供 `audit_source_coverage` 审计。
