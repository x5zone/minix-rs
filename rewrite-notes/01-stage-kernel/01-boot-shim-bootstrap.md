# 01 · Boot-shim 引导准备与控制权交接

> **目标读者**：熟悉操作系统基本概念（特权级、分页、进程切换）与 Rust 基本语法（trait、泛型、生命周期）的开发者；能读 C 与基础汇编（`mov`/`push`/`call` 一级的指令不再解释）。不需要预先了解 Minix3 源码——引用的 C 符号都有解码；不需要预先了解 UEFI 协议——用到的调用都在正文里交代。
>
> **前置知识**：虚拟内存、页表、中断与引导流程的大意。读过"自己写 OS"类书籍的读者可跳过 §1.1 第 1 段的固件/引导器背景（阶段表可只看第三行）；§1.1 的"为什么第一跳必须是汇编"论证段（"所以第一跳只能是汇编里的一条指栈命令"起）是后文论证的前提；§1.3 是第一章收束，可按需回读。
>
> **教学目标**（读完本篇应能独立讲出）：① 引导器交棒时机器处于什么状态、为什么第一跳必须是汇编（§1.1/§2.2）；② x86-32 从 `head.S` 入口到 `pre_init` 返回的完整链条、内存清单的建立与裁剪（§2.2–§2.5）；③ 页表建立：恒等映射与内核映射的次序、重叠语义、借槽窗口（§2.6）；④ 引导期内存账：页从哪里来、何时换手给 VM、空闲清单怎么传下去（§2.4/§2.4.2/§2.6/§3.8/§4.5/§4.7）；⑤ 三架构交接形态的差异与设计理由（§3.7/§4.8；各路径当前接线状态见 §3.1.1）；⑥ Rust 实现落点与运行阶段边界：EBS（UEFI 的 `ExitBootServices`，退出固件服务那次调用）前后、快照口径与深拷不变量（§4.1/§4.2/§4.5；本页各条的名词都在正文首次出现处展开）；⑦ 引导期故障怎么用串口路标与断言定位（§6）；⑧ 每条核心机制靠什么证据成立、哪些还没有断言（§5 的证据总账）；⑨ ARM32 的同名函数对照：哪些步骤同构、哪一步必须按引导器重写（§2.7）。

内核不是从零开始的。它被放进内存时，机器已经由固件（UEFI 或 BIOS）与引导加载器（MINIX 自带的 boot2、GRUB 或 U-Boot）跑过一段路：内存控制器已经初始化，可执行文件已经被读进 RAM。地址翻译这一步两条路不一样——UEFI 上的固件是开着分页进来的，而 BIOS 上的引导器（boot2 或 GRUB）在本仓的路径上**关着分页**就把控制权交出来（Multiboot 规范不强制这一状态；C 的 `vm_enable_paging` 仍先清 `PG`，是防御性写法）（这就是 1.1 那张阶段表里"分页未开（x86-32）"的来处）。

但这些成果并不是内核可以直接使用的形式——它们要么散在别人填的结构体里，要么被别的代码写在某个寄存器里，要么只数到"哪几段物理地址可用"这一层：真正空闲的页帧要内核自己记账，而引导器给的这份内存信息还分粗细两档（展开见 §2.4）。

这一篇讲清内核接管之前的那一段：谁把机器带到了"内核可以开始执行"的状态，中间凑齐了哪几样东西，哪一步的次序错了会坏在什么地方。

---

## 1. 概述

### 1.1 内核的第一行代码之前还有一段代码

x86 上加电后，机器先执行固件；固件自检、初始化并找到可引导设备，把控制权交给磁盘上的引导加载器；引导加载器读自己的配置、把内核镜像与若干模块读进物理内存、切到保护模式，最后跳到内核的入口。这一段里只有最后一小截属于内核源码。

| 阶段 | 谁在跑 | 机器状态 | 属于本仓库源码吗 |
|---|---|---|---|
| 加电与自检 | 主板固件（BIOS/UEFI） | 实模式起步；UEFI 随后进入长模式并开分页（§1.1 首段），BIOS 路径留在保护模式 | 否 |
| 加载可执行文件 | 引导加载器（boot2/GRUB/U-Boot） | 保护模式或长模式，已能访问磁盘与内存 | 否（只提供配置模板 `minix3/etc/boot.cfg.default`） |
| 引导准备 | 内核源码的最前一段（`head.S` + `pre_init()`；C/x86-32 路径；Rust 侧是独立 boot-shim，§1.4 定义） | 分页未开（x86-32），无栈可用、无堆、无中断（Rust UEFI boot-shim 的状态见 §3.1，有栈与池分配） | 是 |
| 内核主流程 | `kmain()` 及其后 | 分页已开、内核在高地址执行 | 是（后续各篇） |

第三行才是本篇的对象。"引导准备"这四个字里包含两类工作：**收集**（从引导器留下的数据结构里读出内存布局、模块位置、启动参数）与**搭架子**（把可用内存记成账、把页表建起来、把分页打开）。两类工作都在同一段没有运行环境的 C/x86-32 代码里完成（Rust UEFI 的 `fn main` 运行在 Boot Services 环境，见 §3.1）——没有一块可以依赖的栈（`esp` 里固然有值，但它指向引导器用过的位置，没有任何约定保证那段内存还能用——要自己设一个临时栈）、没有分配器，也没有报错输出通道，更没有别的设施。

上一段里那句"要自己设一个临时栈"是这一整段代码能成立的前提，而且它只能用汇编写。链条是这样：引导器跳到内核入口时，栈指针指的是它自己用过的位置，那块内存属于谁、还能用多久，没有任何约定——Multiboot 规范在"机器状态"一节把除少数几个寄存器以外的处理器状态都列为未定义，并明写"操作系统必须在自己需要栈时立刻建一个"。而 C 函数一进去就要用栈：局部变量、要保存的寄存器、返回地址都落在栈上。于是"重建栈指针"这件事，正好是那件"必须先有栈才能用 C 去写"的事——用 C 写不出自己的第一条 `mov` 到 `esp`。

所以第一跳只能是汇编里的一条指栈命令：`i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 用 `mov $load_stack_start, %esp` 把栈指针指到同一个文件末尾 `.data` 段里预留的 4096 字节上（逐行解码见 §2.2）。

这段内存稍后会被 `cut_memmap` 从可用清单切掉（§2.5）。安全性不来自"它属于内核镜像"，而来自"切账发生在任何分配之前"。它在 `kmain` 后段被 `add_memmap` 归还（§2.4.2 的"还引导段"行）；引导器自己用过的那段栈没有这个保证，沿用它就等于把栈搭在一块随时可能被发给别人的内存上。

本篇反复出现的 **VM/PM/RS** 是 Minix3 的三个启动服务：VM（虚拟内存服务器）、PM（进程管理器）、RS（重生服务器）；它们由内核在启动阶段 exec。**boot-shim** 是独立引导准备可执行体（正式定义见 §1.4）。其余缩写：**DTB** = 设备树 blob；**SBI** = RISC-V 的监管者二进制接口；**OVMF**/**AAVMF** = x86/ARM 的 UEFI 固件实现（QEMU 用）；**MTRR** = x86 的内存类型范围寄存器；**LSR** = 串口 Line Status Register（线路状态寄存器，0x3FD）。本篇后面因此不再单独解释"为什么第一跳是一段汇编"。

### 1.2 自举要凑齐的四样东西

不论引导器是 MINIX 自带的 boot2、GRUB、U-Boot 还是别的什么，把机器交给内核之前必须凑齐四样：

```
    ┌────────────────────────────────┐
    │ 1. 可用物理内存的清单          │  哪些物理区间可以被分配
    │   （内核与模块占用区已排除）   │
    └───────────────┬────────────────┘
                    ▲ 依赖 A（框 2 → 框 1）：内核自己占的那一段要被
                    │   这份清单切掉
                    │ 依赖 B（框 1 → 框 3，跳框）：建表的第二步（`pg_identity`）
                    │   就要读清单算出的内存上界（页表页来自静态池；动态取页发生
                    │   在建表之后的 4KB 路径，见 2.6.2）
    ┌───────────────┴────────────────┐
    │ 2. 内核自己的位置与大小        │  内核镜像在物理内存的哪一段
    │ （物理基址 / 虚拟基址 / 长度） │  以及它要运行在哪个虚拟地址
    └───────────────┬────────────────┘
                    │ 决定这张表必须覆盖的两段地址：此刻正在执行的地址，
                    ▼ 与内核要去的目的地址
    ┌────────────────────────────────┐
    │ 3. 一张内核可运行在其上的页表  │  当前地址没被覆盖的话，开启分页后的
    │   （当前地址 + 目的地地址）    │  下一条指令就取不到自己的代码
    └───────────────┬────────────────┘
                    │ 建完这张表才知道第一个空闲页目录项的下标是多少，
                    ▼ 这个数要回填进框 4
    ┌────────────────────────────────┐
    │ 4. 一份把上面三样交下去的结构  │  内核主流程要读的就是它
    └───────────────┬────────────────┘
                    │ 分页开启、内核落到高地址之后，才有人读得到它
                    ▼
                kmain() 的第一条 C 语句
```

这四个框排成一条竖线（框号是**阅读顺序**，不是依赖顺序；真实依赖是 2 → 1 → 3 → 4），但**它们不是流水线上的四道工序**：竖线只保证阅读顺序，图上画出的一条逆向箭头（框 2 → 框 1，内核位置回头裁清单）与一条以文字标注的跨框依赖（框 1 的清单直接是框 3 建表的输入，写在箭头旁的说明里）都不顺着竖线走。四样之间的依赖方向决定了真正的次序，逐条如下（**编号 D1–D5 是条目名，不是执行序**；按依赖方向，执行序是 D2 → D1 →（D5，建表动作内部）→ D3 → D4（依赖 A 即 D2，依赖 B 即 D1）。读序对照：D1→§2.4/§2.6.1，D2→§2.3/§2.5，D3→§2.6.1，D4→§2.2，D5→§2.6.1）：

D1. **内存清单先于建表**。清单先建、页表后建；建表要用清单里的内存上界判断缓存属性。具体到 C：`memmap` 里内核与模块占用的区间由 `cut_memmap` 切掉（动作在 §2.5，函数本身见 §2.4），`pg_identity` 在第二步读 `mem_high_phys` 决定哪些 4MB 条目标非缓存（§2.6.1）；页表页来自静态池，所以这是"判定输入"依赖，不是"取页"依赖。
D2. **内核的位置先于清单的裁剪**。清单要排除的区间里包含内核镜像自己；内核镜像的物理区间从链接期常量来（`_kern_phys_base`、`_kern_size`），不来自引导器的数据结构。
D3. **页表先于开启分页**，这是本节唯一一条"硬件强制"的次序。`CR0.PG` 置位后的**下一条取指**就已经在走页表；此时页表里如果没有当前指令所在的虚拟地址的映射，CPU 立刻产生页错误，而页错误处理程序此刻还不存在。
D4. **结构先于内核第一条 C 语句**。`pre_init()` 返回的就是这份结构；它按值或按指针交给 `kmain()`。
D5. **恒等映射先于内核映射**。`pg_identity()` 排在 `pg_mapkernel()` 之前。这两步各管一个时刻：恒等表管的是**开启分页那一瞬间**——`CR0.PG` 置位后的下一条取指仍在原来的低地址上，靠它才不会跑飞；内核映射管的是稍后进高半执行的那一步，把内核链接时的高半地址指到镜像**实际所在的低物理地址**。两步写的是同一张页目录，写入的下标范围互相重叠，谁后写谁生效——这次覆盖为什么恰好是需要的、反过来排会坏成什么样，逐数字的推演在 §2.6.1。

> **附注（字段级，不计入上面五条次序约束（D1–D5））**：建表第三步 `pg_mapkernel()` 的返回值——第一个空闲页目录下标——要在建表之后写进交接结构（`kinfo.freepde_start`），否则第 2 章要讲的"借槽"机制没有起点（展开见 §2.6.3）。§2.8 的"五条次序约束（D1–D5）"就是上面这五条，编号一致。

C 的这四样分别落在 `pre_init.c` 与 `pg_utils.c` 两个文件里：收集在 `pre_init.c`，记录与页表在 `pg_utils.c`。这个划分是 Minix3 的既有划分；重写把同一条分界换到了另一处——主线上的准备与运行分属两个可执行体；两个直核形态把引导面留在内核镜像里（"直核"= 不经独立 boot-shim（见 §1.4）、固件直接跳内核镜像的启动方式；§3.3/§3.7 展开），第 3 章讲这件事。

### 1.3 从第一章到第二章

§1.2 已给出五条次序约束（D1–D5；D5 是 D1 内部的建表次序），§1.4 给本篇范围与编号索引，两者不在这里重述。往下读时按 D1–D5 对照第 2 章的 C 源码：第 3 章看 Rust 把"准备"与"运行"分到哪里，第 4 章逐处实现，第 5、6 章是测试与诊断。

读第 2 章时先看符号再看注释：本篇引用的 C 行为以函数体、链接脚本与调用序为准，源码注释只作线索；两者冲突时以代码为准，并在 §5.1 的证据账里记录口径。D1–D5 是给第 2 章的**论证**排序的，不决定节序（节序按 C 代码顺序，二者对照见 §1.2 的读序表），也不是第 3 章的设计提纲。

### 1.4 本篇范围与可以安全跳过的内容

本篇覆盖从引导器跳进内核（或跳进替它准备的那段代码）起，到控制权交给内核主流程为止的全部动作：引导器契约、参数收集、内存记账、模块装载的检查、页表建立与分页开启、以及这段代码在新设计里的组织方式。

两个后文反复使用的名词先立住：**快照** = 一次 `GetMemoryMap` 得到的固件内存图副本（与 MapKey 绑定；MapKey 见 §4.2.1）；**恒等窗口** = 自举期那张低地址 `VA = PA` 映射可见的地址范围。后文的 arena（§3.4.3 定义：boot-shim 预留的一块定长物理内存）、页表页与交接载荷在**切根**（从自举根切到运行期根，§3.7）之前都靠它可读。

全篇有五个与"栈顶"有关的名字，先并排列出，避免后文把它们当同一物：

| 名字 | 属于 | 值/含义 | 谁用 |
|---|---|---|---|
| `load_stack_start` | C 的 `head.S` | 引导器交棒后第一条 `mov %esp` 用的 4 KiB 临时栈顶 | `multiboot_init`（§2.2） |
| `k_initial_stktop` | C 的 `head.S` | `pre_init` 返回后换栈的目标 | `mov %esp`（§2.2） |
| `kernel_boot_stack_top` | Rust 链接脚本符号 | 64 KiB 引导栈数组的顶（2 MiB 收口之前） | 链接期布局 |
| `kern_stack_top` | `KernelInfo` 字段 | 镜像尾（2 MiB 收口之后） | `jump_to_kmain` 的 `sp` |
| `user_sp` | `KernelInfo` 字段 | 用户栈顶 | VM 启动参数 |

以下内容不在本篇展开，遇到时给出指向：

| 可以跳过的内容 | 归属 |
|---|---|
| 链接脚本的虚拟地址/装载地址**区别的推导**、`AT()` 的完整语义、两段式布局 | `02-higher-half-kernel.md` §2.1–§2.2（§3.5 给最小必要语义；完整推导仍在 02 篇）。本篇从 §3.4 起会直接用 LMA（装载地址）与 VMA（执行地址）这两个词，首次出现处给指向，正式定义在 §3.5，不再重复推导 |
| 换栈与跳转必须成对、`k_initial_stktop` 落在哪一段 | `02-higher-half-kernel.md` §2.3–§2.4 |
| ELF 解析器内部（段迭代、错误分类）、段拷贝与 BSS 清零的实现 | `02-higher-half-kernel.md` §4.2 |
| 页表多级结构与页表接口的完整定义 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md`（§1 概念与 §3 设计决策）|
| 引导期建表之后的 Direct Map 覆盖建立 | 不在本篇展开：§4.6 只讲到建表与开启，覆盖建立属分页运行期（Direct Map＝一段固定虚拟窗口到物理地址的线性映射），该机制的专篇尚未生成。本篇只交代它的时序事实与两组引用——时序组：覆盖建立在建表之后、读 `memmap` 时那些页还完好（§3.4.4 的探针读数；§4.5 讲深拷落地区）；对比组：`reserved_regions` 的用途（§3.4）与借槽机制的对比（§3.9）——不展开覆盖建立本身 |
| `kmain()` 起的三张表（GDT/IDT/TSS）与保护结构 | `03-kmain-cstart.md` |
| 设备树/ACPI 表的解析细节与硬件描述符 | `04-platform-discovery.md` |
| 时钟与中断控制器初始化 | `05-clock-interrupt-init.md` |
| UEFI 规范本身（协议、表、Boot Services 分类） | 外部资料；本篇只讲清用到的那几个调用 |

（表里与下文反复出现的一个名字先在这里立住：**boot-shim** 是重写把"引导准备"搬出去的那个独立可执行体——它的产物形态与两个接口见 §3.2。）各条启动路径的当前接线状态集中在 §3.1.1 的"唯一权威表"，其余各节再提到接线状态都指向那里。

本篇用到的编号不止一套，先给一张索引，后面每套首次出现时不再重复解释（各计法的成员与差异以 §3.7 为准，本节只给索引）：

| 编号 | 指什么 | 定义节 | 主要消费者 |
|---|---|---|---|
| D1–D5 | 五条次序约束（D1–D5）（自举依赖与真实次序） | §1.2 | §2.8 |
| C 建表五步 | `pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` | §2.6.1 | §4.6、§4.9 |
| shim 九步 | `prepare_and_enter_kernel_image` 的九个动作 | §4.2.1 | §4.2.1 的动作表 / §4.2 的代码引文 |
| 描述符判定五步 | `adjudicate` 对单条描述符的判定 | §4.2.2 | §4.2.2 / §5.2.1 的 `ebs_map.rs` 44 条 |
| Step 0–4 | 内核侧启动阶段编号 | §4.1 | §4.6、§6.1 |
| 形态 3 / 生产腿 5 / 交接路径 4 / 三类链 6 / 架构×链五行 5 / 门链 5 | 同一批启动路径的六套计法；定义节：形态（§3.7）/生产腿（§3.2 定义、§3.4.1 成员）/交接路径（§4.8）/三类链（§3.7）/架构×链五行（§4.8）/门链（§5.2.3） | §3.2 / §3.7 / §4.8 / §5.2.3 | §3.1.1、§4.8、§5.2.3 |
| M1–M17 | 机制到证据的对照 | §5.1 | §5.3 |


三处需要单独说明的边界：

`k_initial_stktop` 是 `head.S` 在 `pre_init` 返回之后换栈用的那个栈顶符号。本篇只在 §2.2、§2.8、§7.5 与本节这条边界声明里引用它的结论、不复制它落在哪一段的判据（判据在 `02-higher-half-kernel.md` §2.4），理由是语义上的划分：换栈与跳进高半必须成对——栈一换，紧接着的那次 `call kmain` 就要在高半取指——所以"栈顶落在低半还是高半"这个问题本身就属于"内核怎么走到高半去跑"那一章，归 `02-higher-half-kernel.md` §2.3–§2.4。本篇的边界停在内核进入 `kmain` 主流程之前（`pre_init()` 返回是 C 侧的物理落点，Rust 侧对应 `arch_boot`（直核）或 `arch_boot_resume_v1`（跨镜像接收）把控制权交给 `kmain`）。越过这条线的内容都是**显式声明的下游交代**，按节列出：§2.4.2 / §2.6.3 / §2.6.4 交代内存账与页目录在 `pre_init` 之后的最终归属（不交代则第 2 章的账是悬账）；§2.3 的 rootdevname 用户态落链交代"参数的读者在哪"（点名读者并给一次调用链，含 PM 缓存这一层；不展开 PM 内部实现）；§4.4 的 `kmain` 入口校验（§3.5 只引用其结论）、§3.4.2 的 GET_KINFO 载荷收窄与 §3.9 的报告位、§3.8 的 VM 注册各用一小段交代 Rust 侧结构被谁、在哪个阶段消费（交接结构的"读者在哪"是交接设计的一部分）；§5.2.3 的 `entering scheduler` 路标只判到调度器入口，之后不判。

除此之外不再往下游走——`kmain` 之后的内核行为属后续各篇。这里有一处命名陷阱：`k_initial_stktop` 的名字里带着 `__k_unpaged_` 字样，很像在说"它属于分页开启前那段低地址代码"，但名字不是判据——定义处 `mpx.S` 手写了 `LABEL(__k_unpaged_k_initial_stktop)`，引用处 `head.S` 写的是 `k_initial_stktop`，构建时 `objcopy --prefix-symbols=__k_unpaged_` 才把引用改成定义的名字；两处都不能证明它进了低地址段；真判据是链接脚本把哪个对象的 `.bss` 收进哪一条输出段，那条推证和它的证据表在 `02-higher-half-kernel.md` §2.4。

模块启动镜像的 ELF 段拷贝细节同属装载，但它的实现分析（段迭代器、错误变体、BSS 清零）归 `02-higher-half-kernel.md` §4.2。本篇只讲到"哪些文件被读、按什么顺序被读、读不到怎么办"，不展开段拷贝算法。

---

## 2. C 源码分析

本章主线按**四样东西**归类、组内按 C 代码顺序走；三处下游归属（§2.4.2/§2.6.3/§2.6.4）在 §1.4 显式登记。**§1.2 那四样东西，在 C 里是由谁、在哪几步、按什么顺序凑齐的**。引导器把机器交给内核之后，代码只有两处——`head.S` 的几条指令与 `pre_init()` 里的几个函数；`get_parameters` 这一个函数因此分三次出场（拷贝与参数在 §2.3、内存收集在 §2.4、内核伪装模块与切除在 §2.5），三处合起来才是它的全部：

| 节 | 它回答什么问题 | 产出 §1.2 的哪一样 |
|---|---|---|
| 2.1 | 引导器与内核之间到底靠什么对接 | 契约本身（识别头、寄存器、结构体） |
| 2.2 | 第一跳做了哪三件事（"为什么只能是汇编"在 1.1） | 无（搭起能跑 C 的前提） |
| 2.3 | 别人的数据怎么变成自己的记录 | 框 2 的位置与大小、框 1 的雏形与框 4 的主体 |
| 2.4、2.5 | 可用内存怎么记账、谁来查重叠 | 框 1 |
| 2.6 | 页表怎么建、页表页从哪来、建表之后谁接手（末尾附全篇共用的位与常量参考表） | 框 3 及其下游 |
| 2.7 | 换一种架构，同一串函数还成立吗 | 两侧对照 |
| 2.8 | 本章收成一张调用树与五条次序约束（D1–D5）（其中两条只给指向） | — |

需要路线图时看 2.8 开头那张图，它同时也是本章的收束。

### 2.1 引导器交给内核的那份启动信息

引导器与内核之间靠三样东西对接：内核镜像里的一个识别头、跳转时的寄存器约定、以及一份由引导器填好的结构体。识别头嵌在内核二进制里（不是配置文件），按 Multiboot 规范由引导器扫描镜像前 8KB 找到它才认这个内核（三个字段定义在 `i386/head.S` 的 [`multiboot_magic`](minix3/minix/kernel/arch/i386/head.S:multiboot_magic)）——不过本仓这条路径上跑的是 MINIX 自带的 boot2，它按 ELF 头装载，并不读这个识别头；识别头是给 GRUB 一类 Multiboot 合规引导器准备的——所以下面这段识别头解码只对 GRUB 形态有效，读本仓 boot2 路径可以整段跳过，直接看后面的寄存器约定与 `mbi` 结构。

```asm
/*
 * 代码引用：整块是 minix3/minix/kernel/arch/i386/head.S:multiboot_magic 所在的
 * 那一段连续源码，从 `.balign 8` 起，到 `multiboot_depth` 的取值为止。
 * 五处连续 .long 0 的语义见下方逐字段解码。
 */
.balign 8

#define MULTIBOOT_FLAGS (MULTIBOOT_HEADER_WANT_MEMORY | MULTIBOOT_HEADER_MODS_ALIGNED)

multiboot_magic:
	.long MULTIBOOT_HEADER_MAGIC
multiboot_flags:
	.long MULTIBOOT_FLAGS
multiboot_checksum:
	.long -(MULTIBOOT_HEADER_MAGIC + MULTIBOOT_FLAGS)
	.long 0
	.long 0
	.long 0
	.long 0
	.long 0
/* Video mode */
multiboot_mode_type:
	.long MULTIBOOT_VIDEO_MODE_EGA
multiboot_width:
	.long MULTIBOOT_CONSOLE_COLS
multiboot_height:
	.long MULTIBOOT_CONSOLE_LINES
multiboot_depth:
	.long 0
```

逐字段解码：

- `.balign 8`：把识别头对齐到 8 字节边界。这是比协议本身更严的选择（`.balign` 的实参由本文件决定）。
- `multiboot_magic` 的值为 `MULTIBOOT_HEADER_MAGIC`，定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h) 为 `0x1BADB002`。
- `multiboot_flags` 只置了两位：`WANT_MEMORY`（请把基本内存大小也告诉我）与 `MODS_ALIGNED`（模块请按页对齐）。**没有置视频位**（`MULTIBOOT_HEADER_HAS_VBE`，值为 `0x00000004`）——下一段那四个视频字段虽然写着 EGA/80/25，但按结构体注释（`Valid if mh_flags sets MULTIBOOT_HEADER_HAS_VBE`）引导器应当忽略它们。
- `multiboot_checksum` 的值是 `-(magic + flags)`。引导器验算 `magic + flags + checksum == 0`；三个 `long` 相加为零是"这个头部完整"的证明。它不是校验和算法，是一次加法恒等式。
- 紧跟的五个 `.long 0` 是 `header_addr`、`load_addr`、`load_end_addr`、`bss_end_addr`、`entry_addr`。前三个只在 `flags` 里置了 `HAS_ADDR` 位时才被读，后两个同样按位生效；这里都没置位，所以它们保持 0。这五个 0 是一个选择：**这个内核让引导器决定装载位置**，自己不指定。
- 视频部分的四个字段写的是 EGA 文本模式、宽度 80、高度 25，但 `multiboot_flags` 没置视频位，按结构体注释引导器应忽略它们（`MULTIBOOT_VIDEO_MODE_EGA`/`MULTIBOOT_CONSOLE_COLS`/`MULTIBOOT_CONSOLE_LINES`）。

跳转时引导器约定两个寄存器：一个放确认魔术值，一个放它填好的信息结构体地址。这两个值在内核侧被一条断言和一次拷贝立刻消化掉（见 2.2、2.3）。两个魔术值只差一个十六进制位，方向相反：

| 魔术值 | 值 | 方向 | 谁读它 |
|---|---|---|---|
| `MULTIBOOT_HEADER_MAGIC` | `0x1BADB002` | 内核 → 引导器 | Multiboot 合规引导器扫镜像前 8KB（本仓 boot2 不读） |
| `MULTIBOOT_INFO_MAGIC` | `0x2BADB002` | 引导器 → 内核 | `pre_init()` 的第一条断言 |

配置文件里的每条指令各把一份信息送到上面这个交接面上。先说清读这份文件的是谁：解释 `menu=`、`load_mods`、`multiboot` 这套指令语言的，是 **MINIX 自带的第二级引导**（[`boot/boot2.c`](minix3/sys/arch/i386/stand/boot/boot2.c) 的命令表里逐条写着这三个名字），不是 GRUB——GRUB 用自己的 `menu.lst`/`grub.cfg`，只因为同样遵 Multiboot 协议而能引导同一个内核。

```
# 代码引用，minix3/etc/boot.cfg.default 的第一条 menu 行
menu=Start MINIX 3:load_mods /boot/minix_default/mod*;multiboot /boot/minix_default/kernel rootdevname=$rootdevname $args
```

| 指令 | 送到哪里 | 内核哪一段读它 |
|---|---|---|
| `multiboot <内核路径> [命令行]` | 内核镜像被读进某个物理地址，然后跳到它的入口；内核路径与 `[命令行]` 拼成的一整串成为 `mbi->mi_cmdline` | 命令行由 `get_parameters()` 解析；镜像地址不从引导器读，它来自链接期常量（见 2.3） |
| `load_mods <模块通配路径>` | 把每个匹配到的文件**路径**挂进**引导器自己的模块链表**——只登记名字，不读文件内容、不分配地址（模块＝内核随后要逐个启动的那些服务进程的镜像，名单与顺序见 §4.3）；真正的读取与物理地址登记发生在 `multiboot` 执行时的 `module_init`（它是 `exec_multiboot` 内部的一个步骤，不是另一个顶层入口）。`mbi->mi_mods_addr`/`mi_mods_count` 两个字段也不由它填，要等 `multiboot` 执行时按链表现状建表再填（见下面次序段的原文；[`exec_multiboot`](minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot) 759-760 行） | [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 把整个数组 `memcpy` 进 `kinfo.module_list[]` |
| `rootdevname=<设备名>` | 只是命令行里的一个 `key=value` 词，随整串进 `mbi->mi_cmdline` | `get_parameters()` 解析命令行时逐词交给 `mb_set_param()`，最终以 `key=value` 形式落进 `kinfo.param_buf` |

先说清 `rootdevname` 是什么：**它是根文件系统的设备名，写的时候不带 `/dev/` 前缀**——值形如 `c0d0p0`，对应的完整设备路径才是 `/dev/c0d0p0`。这个值由装机脚本剥前缀得到——`minix3/minix/commands/update_bootcfg/update_bootcfg.sh` 里 `rootdevname=` 那一行就是把 `$ROOT`（形如 `/dev/c0d0p0`）过一道 `sed 's/\/dev\///'`；同文件的注释还说明它在 setup 期间会变，所以 `boot.cfg.default` 里只能写成变量 `$rootdevname`，每次刷配置时重算。

回到这三条指令：只有 `rootdevname=` 是"内核能直接看见内容"的——另两条的产物是一个地址或一段内存，内容要看引导器填的结构体。而它能被内核"认出来"，靠的不是引导器，是内核自己的解析函数（见 2.3）。但要注意：**内核自己不解释这个参数**（`grep -rn rootdevname minix3/minix/kernel/` 无命中），它只是把它搬进 `param_buf`；真正需要"根设备是谁"的是用户态（RAMDS——RAM 盘驱动 `ramdisk` 及其启动脚本 `rc`——与 `mib`（暴露内核/系统信息的树状节点表服务）两个读者的具体落点见本节落链图）。这条分工就是微内核分层的直接体现：**对 `rootdevname` 这类纯用户态参数，内核只当搬运工，语义留给用户态**；两个专用键（`cttyline`/`cttybaud`）与 `board=` 是例外，见 §2.3。

`load_mods` 的模块数有一个上限：`MULTIBOOT_MAX_MODS` 定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h) 为 20（数组槽数）；`kinfo_t` 里的 `module_list[]` 就是按这个数开的数组（`minix/param.h` 的 [`struct kinfo`](<minix3/minix/include/minix/param.h:struct kinfo>)）。内核用一条断言把它当硬约束——`assert(mbi->mi_mods_count < MULTIBOOT_MAX_MODS)`：实际模块数最多 19（20 号槽要留给内核），到 20 不是截断，是当场停住。

**约定：`load_mods` 必须写在 `multiboot` 前面。** 约定内容是这样的：`load_mods` 只做一件事——把匹配到的文件名字挂进引导器自己的模块链表（这条命令在 `boot/boot2.c` 的 [`command_load_mods`](minix3/sys/arch/i386/stand/boot/boot2.c:command_load_mods)，它转手交给 `module_add`）。而告诉内核"有模块"的那个标志位，是 `multiboot` 在装载内核时写下去的：`lib/exec.c` 的 [`exec_multiboot`](minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot) 先看模块开关是否开着，开着就按链表当时的内容建一张模块表，建完把 `mi_flags` 的模块位置上（`MULTIBOOT_INFO_HAS_MODS`，值 `0x00000008`），再填条数 `mi_mods_count` 与表地址 `mi_mods_addr`。换句话说，**链表被看见的时刻就是 `multiboot` 执行的时刻**，`load_mods` 写得再对，只要排在后面就赶不上。

**`load_mods` 排在 `multiboot` 后面时，内核停在哪条断言上**：关键在于建表那一步并不要求链表里有东西：链表为空时表照样建出来，只是条数记成 0。这里的"模块开关"是 `exec.c` 的全局 `boot_modules_enabled`（配置级开关，默认开，与 `load_mods` 是否执行无关），所以反序时表仍会建、标志位仍会置上，`get_parameters()` 里那条检查标志的 `assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MODS)` 会**顺利通过**；真正把内核停住的是同一函数再往后的 `assert(mbi->mi_mods_count > 0)`。这个差别直接影响去哪儿查：按"标志没置"查，会去怀疑引导器没解析 `load_mods`；按"条数为零"查，才会去看两条指令的先后。

**内核停在这条断言上时并不是静默挂死**：C 里有两份同名同职责的 assert 实现，内核链接的是系统服务库那一份（[`libsys/assert.c`](minix3/minix/lib/libsys/assert.c) 的 `__assert13`，打印完直接调 panic），不是用户态那一份（[`gen/assert.c`](minix3/lib/libc/gen/assert.c)，打印完走 `abort()`）——所以在这里失败是 panic 停住并出声，而不是用户态那种进程退出。停机收尾具体怎么出声、串口与屏幕两条通路怎么分流，归 §6.3 的失败模式表（第一行）。

### 2.2 head.S：第一跳只做三件事

这条路径上的引导器（§2.1 已点名：MINIX 自带的第二级引导 boot2）跳到 `MINIX` 标签后，入口标签紧接的一条 `jmp multiboot_init` 先跳过识别头（这一步在下面引文之外；`MINIX:` 与这条 `jmp` 在源文件里紧挨在 §2.1 引文段之前——那段引文本身从 `.balign 8` 截起），随后 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 只做三件事：设栈、清帧指针与标志、按 C 调用约定把两个寄存器值变成两个参数。

```asm
/*
 * 代码引用，minix3/minix/kernel/arch/i386/head.S:multiboot_init。
 */
multiboot_init:
	mov	$load_stack_start, %esp	/* make usable stack */
	mov	$0, %ebp
	push	$0	/* set flags to known good state */
	popf	/* esp, clear nested task and int enable */
	push	$0

        push    %ebx	/* multiboot information struct */
	push	%eax	/* multiboot magic number */
        call    _C_LABEL(pre_init)
```

逐行解码：

1. `mov $load_stack_start, %esp`：给自己设一个栈顶。`load_stack` 是同一个文件末尾 `.data` 段的 4096 字节静态空间（`.data` / `load_stack: .space 4096` / `load_stack_start:`），`load_stack_start` 是它的**高端**地址。这一行是"能进 C"的全部原因。
2. `mov $0, %ebp`：清零帧指针。栈回溯靠 `%ebp` 链逐帧走，`0` 表示链到此终止（内核 panic 打印调用栈、QEMU + GDB 远程调试都要用）。
3. `push $0` / `popf`：先把 0 压栈，再把它弹进 EFLAGS。净效果是栈深不变（`popf` 消耗掉刚压的 4 字节），改的是标志寄存器：关中断（`IF=0`）、清嵌套任务位（`NT=0`）。
4. 紧跟的第二个 `push $0`：栈上的一个空位。

它与后面 `push %ebx`、`push %eax`、以及 `call` 推入的返回地址一起，合计 16 字节（4 个双字）；`load_stack_start` 本身按 4096 对齐，这条对齐不是当前链接顺序下的巧合，而是一整条可以逐环验证的构建保证：`i386/Makefile.inc` 的 `.for unpaged_obj in head.o pre_init.o …` 把 `head.o` 固定在 unpaged 对象列表的首位（[Makefile.inc](minix3/minix/kernel/arch/i386/Makefile.inc) 51 行起），该列表经 `OBJS.kernel += ${UNPAGED_OBJS}`（同文件 83 行）按原序进入链接命令行；`kernel.lds` 的 `.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }`（[`kernel.lds`](minix3/minix/kernel/arch/i386/kernel.lds) 18 行）按命令行顺序收输入节，所以 head.o 的 `.data` 是 `.unpaged_data` 的第一个输入节，起点就是 `ALIGN(4096)` 的输出段起点；而 head.o 的 `.data` 以 `load_stack: .space 4096` 开头（[`head.S`](minix3/minix/kernel/arch/i386/head.S) 93–96 行），`load_stack_start` 落在段起点加 4096——仍是 4096 边界。

于是 `pre_init` 入口处的栈指针落在 16 字节边界上——这不是 ABI 的目标形态。这一行源码的注释只写 `/* make usable stack */`，不涉及对齐。（Rust 侧的入口对齐与构建目标细节见 §4.6，本段只判 C。）i386 SysV ABI 要求的是**入口处 `%esp + 4` 是 16 的倍数**（等价于 `%esp ≡ 12 (mod 16)`，因为 `call` 已压入 4 字节返回地址）；从 4096 对齐的栈顶算起，达到 `≡ 12` 需要 20 字节，而这段布局只压了 16 字节，给出的是 `%esp ≡ 0 (mod 16)`——比标准 ABI 少一次 push。**结论**：C 原作的入口栈对齐不满足 SysV 约定；按当前 GCC 的默认假设，`pre_init` 很可能没有生成依赖栈对齐的 SSE 访问；本项目没有保存反汇编证据，因此这条只能作为风险提示（换编译器/优化档可能改变），不是设计选择。Rust 侧不继承这个问题（入口 asm 显式对齐；构建目标与 SSE 细节见 §4.6）。
5. `push %ebx` / `push %eax` / `call pre_init`：i386 的 C 调用约定下第一个参数落在最低地址，所以第二个参数（`multiboot_info` 地址）先压，第一个参数（魔术值）后压。`pre_init(u32_t magic, u32_t ebx)` 的两个形参就位。
6. 到此为止是本篇的范围。下一行 `mov $k_initial_stktop, %esp` 与随后的 `call kmain` 属于换栈与跳转，归 `02-higher-half-kernel.md` §2.3；`k_initial_stktop` 落在哪一段属于它的 §2.4——本篇只在 §2.2、§2.8、§7.5 与 §1.4 这条边界声明里引用那个结论，不重复那套判据（为什么本篇接得住这个结论，见 §1.4）。

三件事就是：设栈、清帧指针与标志、按调用约定传参（"跳过识别头"是入口标签那一跳，不在 `multiboot_init` 内）。

`load_stack` 放在 `.data` 而不是 `.bss`，差别不在"能不能用"——栈是先写后读的结构，内容脏不脏没人管——而在**这段字节的确定性由谁负责**：`.data` 里那 4096 个零字节是镜像文件自带的，装载时按普通内容拷进内存，不依赖装载器的行为；放进 `.bss` 就没有这些字节，清零得靠装载器（本仓 [`libsa/loadfile_elf32.c`](minix3/sys/lib/libsa/loadfile_elf32.c) 在调用方给了 `LOAD_BSS` 这一位时清）。所以这条选择省下的是一个对外部组件的依赖，不是一处崩溃；2.7 引的 ARM32 入口设的是同一个 `load_stack_start`，它在 [`earm/head.S`](minix3/minix/kernel/arch/earm/head.S) 末尾同样落在 `.data`。

**把两条 `push` 对调会怎样**：i386 约定下最低地址就是第一个参数，没有另一种写法。对调之后 `pre_init(magic, ebx)` 收到 `magic = ebx`（`multiboot_info` 的物理地址，一个看起来完全合法的数字），`ebx` 收到 `0x2BADB002`。`pre_init()` 最顶端那条 `assert(magic == MULTIBOOT_INFO_MAGIC)` 当场失败，走 2.1 末尾那条链：屏幕刷出 panic 与诊断缓冲、等一个按键才复位（串口这个出口此时不会出声——`do_serial_debug` 要等命令行解析才可能被置起来，而这条断言排在解析之前）。误诊的方向在这里：这条断言只打印失败的表达式本身（形如 `assert "magic == MULTIBOOT_INFO_MAGIC" failed`），并不打印值；而此刻 `magic` 里躺着的是 `ebx` 送来的地址——一个看起来完全合法的数字。不核对两条 `push` 的次序，容易往"数据结构坏了"的方向查。

### 2.3 get_parameters：把别人的数据变成自己的记录

`i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 是收集阶段的正文。它做的第一件容易被跳过的事是**整份拷贝**：不长期持有引导器给的指针，而是把整个结构体复制进自己的全局 `kinfo`。

```c
/*
 * 代码引用（三处省略号：前两处略去声明行——`mbi` 的声明行保留，因为下面每一条
 * 语句都要用它；第三处略去函数后段的其余语句），
 * minix3/minix/kernel/arch/i386/pre_init.c:get_parameters。
 * 被引到的每一行逐字保留。
 */
void get_parameters(u32_t ebx, kinfo_t *cbi)
{
	multiboot_memory_map_t *mmap;
	multiboot_info_t *mbi = &cbi->mbi;
	...
	/* get our own copy of the multiboot info struct and module list */
	memcpy((void *) mbi, (void *) ebx, sizeof(*mbi));

	/* Set various bits of info for the higher-level kernel. */
	cbi->mem_high_phys = 0;
	cbi->user_sp = (vir_bytes) &_kern_vir_base;
	cbi->vir_kern_start = (vir_bytes) &_kern_vir_base;
	cbi->bootstrap_start = (vir_bytes) &_kern_unpaged_start;
	cbi->bootstrap_len = (vir_bytes) &_kern_unpaged_end -
		cbi->bootstrap_start;
	cbi->kmess = &kmess;

	/* set some configurable defaults */
	cbi->do_serial_debug = 0;
	cbi->serial_debug_baud = 115200;

	/* parse boot command line */
	if (mbi->mi_flags & MULTIBOOT_INFO_HAS_CMDLINE) {
		...
		memcpy(cmdline, (void *) mbi->mi_cmdline, BUF);
		p = cmdline;
		while (*p) {
			var_i = 0;
			value_i = 0;
			while (*p == ' ') p++;
			if (!*p) break;
			while (*p && *p != '=' && *p != ' ' && var_i < BUF - 1) 
				var[var_i++] = *p++ ;
			var[var_i] = 0;
			if (*p++ != '=') continue; /* skip if not name=value */
			while (*p && *p != ' ' && value_i < BUF - 1) 
				value[value_i++] = *p++ ;
			value[value_i] = 0;
			
			mb_set_param(cbi->param_buf, var, value, cbi);
		}
	}

        /* let higher levels know what we are booting on */
        mb_set_param(cbi->param_buf, ARCHVARNAME, (char *)get_board_arch_name(BOARD_ID_INTEL), cbi);
	mb_set_param(cbi->param_buf, BOARDVARNAME,(char *)get_board_name(BOARD_ID_INTEL) , cbi);
	...
}
```

逐段解码：

- `memcpy((void *) mbi, (void *) ebx, sizeof(*mbi))`：把引导器结构体整份拷进 `cbi->mbi`。注意这里有个同名陷阱：`mbi` 是局部指针，而 `kinfo` 的第一个字段也叫 `mbi`（`minix/param.h` 的 [`struct kinfo`](<minix3/minix/include/minix/param.h:struct kinfo>) 里的 `multiboot_info_t mbi;`）——声明行 `multiboot_info_t *mbi = &cbi->mbi;` 就是把前者指向后者。看到 `memcpy` 里光秃秃一个 `mbi` 就以为它是全局变量，是这一段最常见的误读。拷贝之后，`ebx` 指向的那份引导器结构体不再被引用；后面要读模块表与命令行，走的是这份拷贝里记的指针。
- 这五个字段分两类来源，而**分类本身就是这一段的知识**：`mem_high_phys = 0` 是"待累计"的初值，稍后由 `add_memmap` 把它推到最高可用地址；另外四个（`user_sp`、`vir_kern_start`、`bootstrap_start`、`bootstrap_len`）**没有一个来自引导器**，它们取自链接期常量 `_kern_vir_base`、`_kern_unpaged_start`、`_kern_unpaged_end`——也就是内核镜像自己的布局。道理是：**内核被装在哪儿、内核的哪一段是分页前用的，这些事实在链接那一刻就已经定下，内核不必问任何人**。这正是 1.2 那两条依赖的根据：框 2（内核的位置）不接收框 1（清单），反过来倒是框 2 的区间要拿去把框 1 裁短。这段代码有两个细节：`vir_bytes` 类型上的 `&符号` 取的是符号的地址值（值在链接期就定了，运行时只是读一个常数）；`user_sp` 这里先赋 `_kern_vir_base` 只是占位，同一函数后段会被 `USR_STACKTOP` 覆盖。至于这些符号的名字为何带着 `__k_unpaged_` 前缀、哪些对象被算进"分页前那一段"——那是构建期的安排：段划分与边界符号见 §3.5，改名机制的判据在 `02-higher-half-kernel.md` §2.4，本篇不重复。（引文里 `cbi->kmess = &kmess` 的 `kmess` 是 `kmessages` 字段的宏别名，见 [`kernel/glo.h`](minix3/minix/kernel/glo.h)。）
- `bootstrap_len` 是"两个符号地址相减"，得到一小段内核镜像内的区域长度。这段区域叫 `unpaged`。

  > `unpaged` 指的是链接在**低地址**、虚拟地址等于物理地址的那一小段引导期代码与数据——建页表与开分页这几步必须由它完成（否则刚开启分页的下一条取指，要去查一张还没建好的页表）。它不是"未分页的段"（那是 CPU 状态）；这段内存用完由 `kmain` 归还给空闲清单，细节见 §2.4.2 的"还引导段"行（Rust 侧为何没有对应物见 §3.5）。
- 命令行解析是一个手写的逐词扫描状态机，不是 `strtok`：跳过空格、收集 `var` 直到 `=` 或空格、遇 `=` 后收集 `value` 直到空格。`if (*p++ != '=') continue;` 那一行处理"不是 `key=value` 形式的词"——注意 `p++` 已经把游标推过一个字符，所以 `continue` 之后从下一个字符继续扫，不会死循环。
- 每个 `key=value` 都过一遍 `mb_set_param`。然后（命令行解析之后）**无条件**再写两次：`arch=` 与 `board=`，值来自编译期的板卡身份（i386 上固定查 `BOARD_ID_INTEL`）。这两行的效果有边界：`arch=` 只有写点、没有读点，内核的架构身份来自编译期；`board=` 有读点，但当引导器传入同名键时，读取返回第一条匹配，外部输入可以遮蔽 `board=` 这一行写入的值（`arch=` 无读者，不在遮蔽对象内）（仅当外部同名键不是 `param_buf` 的第一条记录；若是首条，内核的 `mb_set_param` 会先删掉它，内核值胜出，见下文反排推演）。因此它们保证的是"`param_buf` 里至少各有一条内核写入的记录"，不是"内核无条件知道自己是什么板"。

`i386/pre_init.c` 的 [`mb_set_param`](minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param) 是这串文本的落地函数，它同时做三件事：识别少数"有专用字段的键"、维护"键不重复"（效力边界见下面对删除扫描的分析：只有待删键是第一条记录时成立）、维护 `param_buf` 的可追加格式。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param。
 */
static int mb_set_param(char *bigbuf, char *name, char *value, kinfo_t *cbi) 
{
	char *p = bigbuf;
	char *bufend = bigbuf + MULTIBOOT_PARAM_BUF_SIZE;
	char *q;
	int namelen = strlen(name);
	int valuelen = strlen(value);

	/* Some variables we recognize */
	if(!strcmp(name, SERVARNAME)) { cbi->do_serial_debug = 1; }
	if(!strcmp(name, SERBAUDVARNAME)) { cbi->serial_debug_baud = atoi(value); }

	/* Delete the item if already exists */
	while (*p) {
		if (strncmp(p, name, namelen) == 0 && p[namelen] == '=') {
			q = p;
			while (*q) q++;
			for (q++; q < bufend; q++, p++)
				*p = *q;
			break;
		}
		while (*p++)
			;
		p++;
	}
	
	for (p = bigbuf; p < bufend && (*p || *(p + 1)); p++)
		;
	if (p > bigbuf) p++;
	
	/* Make sure there's enough space for the new parameter */
	if (p + namelen + valuelen + 3 > bufend)
		return -1;
	
	strcpy(p, name);
	p[namelen] = '=';
	strcpy(p + namelen + 1, value);
	p[namelen + valuelen + 1] = 0;
	p[namelen + valuelen + 2] = 0;
	return 0;
}
```

逐段解码：

- 头两行 `strcmp` 是**唯一的两个**"有专用字段的键"：`SERVARNAME` 与 `SERBAUDVARNAME`（定义在 [`minix/const.h`](minix3/minix/include/minix/const.h)，值为 `"cttyline"` 与 `"cttybaud"`）。命中时它们不是写进 `param_buf` 就完事，而是同时设置 `cbi->do_serial_debug` / `cbi->serial_debug_baud` 这两个结构体字段——因为"从哪个串口做内核调试输出"是内核在启动早期自己要做决定的事，不能等到用户态去读参数。
- "已存在先删"这一段想给的不变量是"同名键最多一份"：它找到 `name` 后紧跟 `=` 的位置，把该条字符串的末尾（`q` 走到 NUL）之后的内容整段左移，覆盖掉旧条目。但这条不变量在真实代码里**只对第一条记录成立**：扫描用的 `while (*p++) ;` 在越过本条记录的 NUL 时，后缀自增已经让 `p` 落在下一条记录的第一个字节上，紧接着那条独立的 `p++` 又多走一步——于是从第二条记录起，匹配检查从记录的第二个字符开始，同名键再也匹配不上。按本节引文即可推演：先写 `A=1` 再写 `B=2`，缓冲区是 `A=1\0B=2\0\0`（相邻记录之间单 NUL）；此时再写 `B=3`，删除扫描找不到已有的 `B`，留下 `A=1\0B=2\0B=3\0\0` 两条 `B`；而再写 `A=3` 时待删键是第一条记录，删除正常生效。也就是说"键不重复"只在待删键恰好是缓冲区第一条记录时可靠成立——这是 C 源码的真实行为，本篇照实记录，不替它圆。
- 找末尾的循环 `for (p = bigbuf; p < bufend && (*p || *(p + 1)); p++)`：它一直走到"当前字节与下一个字节都是 NUL"的位置。也就是说 `param_buf` 不是一条字符串，而是**一串以 NUL 分隔的 `key=value`，整串以一个额外的 NUL 收尾**（末尾效果是两个连续 NUL）。随后 `if (p > bigbuf) p++;` 把写入点挪到那个终止 NUL 之后。空缓冲时 `p == bigbuf`，不跳过，从头开始写。
- 容量检查用 `p + namelen + valuelen + 3 > bufend` 提前拒绝，返回值 `-1`——但调用点全部忽略返回值（`get_parameters` 里三处调用都没有检查：命令行解析循环里一处、固定键 `arch`/`board` 各一处）。这意味着"参数缓冲装不下"是静默丢弃，而不是启动失败。
- 最后五行写入：`name` + `=` + `value` + NUL（`strcpy` 自己带的）+ 再补一个 NUL。两个显式 NUL 写下的是新的终止符。

于是 `rootdevname=<设备名>` 的完整落链是：

```
boot.cfg.default: rootdevname=$rootdevname
   → 引导器把它拼进 multiboot 命令行
   → mbi->mi_cmdline（一整串，含 bootopts=… 等其它词）
   → get_parameters() 逐词扫描，拆出 name="rootdevname" value=<设备名>
   → mb_set_param(kinfo.param_buf, name, value)   ← 这一步是"被认出来"的位置
        · 不是专用字段（只有 cttyline/cttybaud 是），所以只写进 param_buf
        · 以 "rootdevname=<设备名>\0" 的形式追加，并做同名键删除
          （删除对非首条记录失效，见上面逐段解码里对删除扫描的分析）
   → 落到 param_buf 之后，rootdevname 的读者全在用户态，而两条路共用同一个系统调用：
     读者一：命令行 `sysenv rootdevname`（minix3/minix/commands/sysenv/sysenv.c 里
        svrctl(PMGETPARAM, …)）→ PM 启动期已用同一条 GET_MONPARAMS 取过一份缓存（PM 缓存细节见 PM 篇；起点 `pm/main.c:169`），PMGETPARAM 从这份缓存回答 → 系统任务 GET_MONPARAMS
        （minix3/minix/kernel/system/do_getinfo.c）把 kinfo.param_buf 整块拷给调用者
        → 拿它去用的是 RAMDS 的启动脚本 minix3/minix/drivers/storage/ramdisk/rc：
          取回后拼成 /dev/xxx 交给驱动，没设时打印 "rootdevname not set"
     读者二：同一条 PMGETPARAM 路径，mib 服务也走一次：
        minix3/minix/servers/mib/kern.c:mib_kern_root_device 取值后作为
        MIB（服务用来暴露内核/系统信息的树状节点表）节点 kern.root_device 暴露（人可用 svrctl pm get rootdevname 查）
     注意：内核自身没有 rootdevname 的读者（`grep -rn rootdevname minix3/minix/kernel/`
     无命中）——它是留给用户态的名字
```

上面这张图里要把两件事分开：**内核侧有读这张表的机制，但 `rootdevname` 没分到读者**。`kernel/main.c` 的 [`get_value`](minix3/minix/kernel/main.c:get_value) 与 `env_get` 在这条用途上读的是 `board=`（键名定在 [`minix/const.h`](minix3/minix/include/minix/const.h)，`kmain` 开头就用它取板卡身份）；`arch=` 只有写点、没有读点。`env_get` 还服务 verbose/ac_layout/no_apic 等若干键，与本段的板卡身份无关——下一段推演讲的正是 `board` 被引导配置里多写的一个词改写的后果。`rootdevname` 不同：它穿过内核，只为了到达用户态。

所以"`rootdevname` 为什么要经 `mb_set_param` 才被认出来"的答案是：命令行在 `mbi` 里只是一串字节，内核里没有任何一处按字符串搜索引导器内存；`param_buf` 才是内核自己的参数表，而写入它、维护键唯一（效力边界见前文）与分隔格式的唯一入口就是 `mb_set_param`。不经过它，`rootdevname` 就只是一串留在 `mbi.mi_cmdline` 里、之后没人再读的字符。

**删掉"已存在先删"那段循环会怎样**：`get_parameters` 在解析完命令行之后无条件再写一次 `arch=` 与 `board=`，于是当命令行里已经带有 `arch=` 或 `board=` 时，`param_buf` 里会出现同一键的两条记录。读取方 `get_value()`（[`kernel/main.c`](minix3/minix/kernel/main.c)）是从头扫描、**返回第一个匹配**——于是被返回的是命令行里那一份，而不是内核自己按编译期板卡身份写的那一份。这个删除扫描只按 `name=` 前缀匹配，不要求命中点位于记录起点；若待写键是某条后置记录键去掉首字符的后缀（例如 `xboard=1` 遇内核写 `board=`），它会从记录中部开始删、留下伪记录。具体后果落在 `kernel/main.c` 的 [`kmain`](minix3/minix/kernel/main.c:kmain) 开头那句 `machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME))`：板卡身份会被一条外部输入改写，而改写者是引导配置里多写的一个词。顺带把一个自然的推论也修正掉："循环还在，重复就不会发生"并不成立——上面已经看到删除对非首条记录失效，所以当命令行里的 `board=` 不是 `param_buf` 的第一条记录时（它前面还有别的键），同样的双记录与同样的改写后果在循环完好的情况下也会出现（`arch=` 也会留下双记录，但仓内没有它的读者，见本节前文）。这里还有一个分支要排除：若两次内核写之间 `board=bar` 因为前一条被删而被提升为首条，随后写 `board=` 会把它删掉，最终只剩内核值；双记录/遮蔽要求该键在两次写入后仍不是首条。删掉循环只是把"有时失效"变成"必然失效"。

### 2.4 一份内存账的两个操作：add_memmap 与 cut_memmap

`memmap[]` 里的条目从哪来，是这一节的第一件事。引导器给的是**两档**信息里的一档，`i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 先看细的那一档，取不到才退回两个数：

```c
/*
 * 代码引用（保留原文件的混合制表符与空格缩进），
 * minix3/minix/kernel/arch/i386/pre_init.c:get_parameters 的内存收集段。
 */
	memset(cbi->memmap, 0, sizeof(cbi->memmap));
	/* mem_map has a variable layout */
	if(mbi->mi_flags & MULTIBOOT_INFO_HAS_MMAP) {
		cbi->mmap_size = 0;
	        for (mmap = (multiboot_memory_map_t *) mbi->mmap_addr;
       	     (unsigned long) mmap < mbi->mmap_addr + mbi->mmap_length;
       	       mmap = (multiboot_memory_map_t *) 
		      	((unsigned long) mmap + mmap->mm_size + sizeof(mmap->mm_size))) {
			if(mmap->mm_type != MULTIBOOT_MEMORY_AVAILABLE) continue;
			add_memmap(cbi, mmap->mm_base_addr, mmap->mm_length);
		}
	} else {
		assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MEMORY);
		add_memmap(cbi, 0, mbi->mi_mem_lower*1024);
		add_memmap(cbi, 0x100000, mbi->mi_mem_upper*1024);
	}
```

逐条解码：

- 细的那一档看 `mi_flags` 的 bit 6（`MULTIBOOT_INFO_HAS_MMAP`，值 `0x00000040`，定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)）。它是逐区间的表，条目来自 BIOS 的内存图查询（中断 0x15、`EAX=0xE820`）。循环里那句 `if(mmap->mm_type != MULTIBOOT_MEMORY_AVAILABLE) continue;` 说明进账从一开始就只留"可用"这一种语义：保留区、ACPI 数据区都在这一步被丢掉，而不是留着让下游判断。可用的类型值是 `1`，与 E820 里"可用"同号——这里不是一次翻译，是一次沿用。
- 遍历步长那句 `mmap + mmap->mm_size + sizeof(mmap->mm_size)` 与头文件的注释同源：[`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h) 里 `struct multiboot_mmap` 的注释写明 `mm_size` 描述的是**除它自己以外**那些字段的长度，要在条目之间跳就得数 `mm_size + 4` 字节。这个"长度字段不算进自己的长度"的约定也是从那次中断的返回缓冲区继承来的；本仓自己发那次查询的代码在 [`lib/biosmemx.S`](minix3/sys/arch/i386/stand/lib/biosmemx.S)（`movl $0xe820, %eax; int $0x15`），可以对照同一份缓冲区布局。
- 粗的那一档看 bit 0（`MULTIBOOT_INFO_HAS_MEMORY`，值 `0x00000001`），只有两个以 KB 计的数。`else` 分支把它们硬造成两段：`[0, mem_lower*1024)` 与 `[0x100000, 0x100000 + mem_upper*1024)`。这里藏着一个假设——`mem_upper` 的语义是"从 1MB 往上**连续**可用多少"，而 `add_memmap` 收到的是一个长度，于是这条分支把"一整段连续区间"当成了事实。
- 两个分支还有一处不对称：`if` 分支显式写 `cbi->mmap_size = 0;`，`else` 分支不写。第一行的 `memset` 只清 `memmap[]` 数组本身，`mmap_size` 是 `kinfo` 的另一个字段，不在那次清零范围内；`else` 分支仍能正确起来，靠的是 `kinfo` 作为全局对象落在 `.bss`、初值为 0，再加上 `add_memmap` 里那句 `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;` 会把下标补上来。这个零初值前提不是免费的——§2.2 刚说过 `.bss` 的清零由装载器负责——它在 C 的两条装载路径上都被兑现：Multiboot 路径由装载器按 ELF 程序头把 `filesz` 之外的 `memsz` 清零（GRUB 履行的 Multiboot 装载义务）；Minix 自有引导路径在 `loadfile` 时传 `LOAD_KERNEL = LOAD_ALL & ~LOAD_TEXTA`（[`loadfile_machdep.h`](minix3/sys/arch/x86/include/loadfile_machdep.h)），其中含 `LOAD_BSS` 位（[`loadfile.h`](minix3/sys/lib/libsa/loadfile.h)），调用点在 [`exec.c`](minix3/sys/arch/i386/stand/lib/exec.c) 的内核装载处——两条路的 `.bss` 都是零起步。

**引导器只给粗档那一头会怎样**：`else` 分支不是"罕见机器上的边角路"，它是一档会静默变坏的降级。走这条分支时，1MB 之上被记成一整段连续可用区间；若这台机器的保留区确实落在这段范围之内（显卡的镜像区、ACPI 数据区都常见），那么后面的 `i386/pg_utils.c` 的 [`pg_alloc_page`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page) 从最高条目尾部向下发放、迟早会消耗到保留区所在的位置，把页表页这类要长期住着的结构放进本该保留的区域（保留区越靠这段的高地址端，撞上得越早）。症状不是"分配失败"，而是写入静默丢失或机器随机挂死，而且报错的位置离真正的原因（引导器只置了 bit 0）很远。这也解释了本篇 1.2 的框 1 为什么写成"可用物理内存的清单（内核与模块占用区已排除）"而不是"引导器给的内存数"：清单的形态取决于引导器给哪一档，而"排除占用区"这一步（`cut_memmap`）两档都躲不掉。

可用物理内存用一个定长数组记账：`kinfo.memmap[MAXMEMMAP]`，`MAXMEMMAP` 定义在 [`minix/param.h`](minix3/minix/include/minix/param.h) 为 40。数组里每一项是"一段可用区间"，`mmap_size` 是遍历边界（最高占用下标加一，中间可有零长空洞）。两项操作：追加（`add_memmap`）与切除（`cut_memmap`）。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap。
 */
void add_memmap(kinfo_t *cbi, u64_t addr, u64_t len)
{
        int m;
#define LIMIT 0xFFFFF000
        /* Truncate available memory at 4GB as the rest of minix
         * currently can't deal with any bigger.
         */
        if(addr > LIMIT) return;
        if(addr + len > LIMIT) {
                len -= (addr + len - LIMIT);
        }
        assert(cbi->mmap_size < MAXMEMMAP);
        if(len == 0) return;
	addr = roundup(addr, I386_PAGE_SIZE);
	len = rounddown(len, I386_PAGE_SIZE);

	assert(kernel_may_alloc);

        for(m = 0; m < MAXMEMMAP; m++) {
		phys_bytes highmark;
                if(cbi->memmap[m].mm_length) continue;
                cbi->memmap[m].mm_base_addr = addr;
                cbi->memmap[m].mm_length = len;
                cbi->memmap[m].mm_type = MULTIBOOT_MEMORY_AVAILABLE;
                if(m >= cbi->mmap_size)
                        cbi->mmap_size = m+1;
		highmark = addr + len;
		if(highmark > cbi->mem_high_phys) {
			cbi->mem_high_phys = highmark;
		}

                return;
        }

        panic("no available memmap slot");
}
```

逐条解码：

- `LIMIT 0xFFFFF000` 与两次判断：`addr > LIMIT` 直接丢弃整条；区间跨越 `LIMIT` 时把长度截到边界。两条判断的次序有实际意义——先判 `addr` 把整条在 4GB 之上的描述符整条丢掉（不改 `len`），再判跨界。`0xFFFFF000` 而不是 `0x100000000`，源码注释给的理由是"the rest of minix currently can't deal with any bigger"；地址要能安放进 32 位的 `phys_bytes`（`memmap[]` 里 `mm_base_addr`/`mm_length` 本身是 64 位字段，但内核通篇以 32 位的 `phys_bytes` 收口）是这条限制的后果之一，同时低 12 位为零。
- 两条 `assert`（`mmap_size` 未满、`kernel_may_alloc` 为真）加一条提前返回：`if(len == 0) return;`——长度为零的区间被**静默丢弃**，不是断言。`kernel_may_alloc` 是全局开关（定义在 [`i386/pre_init.c`](minix3/minix/kernel/arch/i386/pre_init.c)，初值 1；声明在 [`kernel/glo.h`](minix3/minix/kernel/glo.h)）。它分的不是"引导期 / 非引导期"这种时段，而是**这份账归谁**：`kernel/main.c` 的 [`kmain`](minix3/minix/kernel/main.c:kmain) 开头显式写一次 1，源码注释是"Kernel may use bits of main memory before VM is started"——此刻内核还能自由取用内存；而 `kernel/main.c` 的 [`bsp_finish_booting`](minix3/minix/kernel/main.c:bsp_finish_booting) 在 `switch_to_user()` 前两行（中间隔一空行）把它写成 0，注释同样直说理由："Kernel may no longer use bits of memory as VM will be running soon"。跨过这条线再想改账，撞上的就是上面那几条 `assert(kernel_may_alloc)`：改账动作当场失败，而不是悄悄改写一份已经交给 VM 的账。
- `roundup(addr) / rounddown(len)`：**起点向大、长度向小**。这个方向只保证条目本身页对齐，**不保证**新区间落在原描述符 `[addr, addr+len)` 之内：起点上取整会让终点最多外移一页（例如 `addr=0x1001, len=0x2000` 得到 `[0x2000, 0x4000)`，原区间终点是 `0x3001`）。真正的向内夹取要按 `rounddown(addr+len) - roundup(addr)` 算长度。当前内核的调用点传入的**起点**都页对齐（代码控制的那些都是；固件 E820 基址通常按页粒度报告（规范本身不保证）），所以"终点外移一页"的缺口不可达；但**长度**不保证页对齐——`mi_mem_lower*1024`、模块文件长度都会被 `rounddown(len)` 截掉尾页。
- 追加写到哪一格：`memmap[]` 是定长 40 项的数组，**每一项记一段可用区间**。找空位很朴素——从下标 0 往后看，第一个长度为 0 的格子就是空位（`if(cbi->memmap[m].mm_length) continue;`：长度非零，说明这格已被一段区间占着）。这里**没有单独的"已占用"标志位**，用的是"长度为零"等价于"这格没数据"。这个等价由两件事兜住：`len == 0` 的调用被入口提前拒绝；`0 < len < 4096` 的退化调用会在 `rounddown` 后变成 0 并被写成一个零长度条目（它会把 `mmap_size` 推高一格，只能靠"下次从下标 0 找空位时当空格复用"兜住）。所以零长度项在数组里通常只有一种含义——空格；但"零长度区间根本不会被写进来"这句话过强。
- `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;`：因为是"拿第一个空位"而不是末尾追动，新条目可能落在比当前边界更低的下标上，所以 `mmap_size` 要取 `m+1` 而不能简单自增——它代表的是**遍历边界**（最高占用下标加一）。还要说清一点：**数组中间可以有零长度的空洞**（一次切除把整条抹掉时就会留下），所以每个消费者都得自己跳过零长度项——`pg_alloc_page` 从最高下标往回扫时，第一件事就是 `if(!mmap->mm_length) continue;`，那就是空洞存在的证据。
- `mem_high_phys` 只增不减：它是所有条目的最高地址上界（`addr + len`）。`pg_identity()` 依赖它（见 §2.6）。

切除是这里的核心操作，因为它要把一段区间从记录里去掉，而记录里的一段区间可能被切成两段（`i386/pg_utils.c` 的 [`cut_memmap`](minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap)；追加那一半在 `i386/pg_utils.c` 的 [`add_memmap`](minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap)）。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap。
 */
void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)
{
        int m;
        phys_bytes o;

        if((o=start % I386_PAGE_SIZE))
                start -= o;
        if((o=end % I386_PAGE_SIZE))
                end += I386_PAGE_SIZE - o;

	assert(kernel_may_alloc);

        for(m = 0; m < cbi->mmap_size; m++) {
                phys_bytes substart = start, subend = end;
                phys_bytes memaddr = cbi->memmap[m].mm_base_addr,
                        memend = cbi->memmap[m].mm_base_addr + cbi->memmap[m].mm_length;

                /* adjust cut range to be a subset of the free memory */
                if(substart < memaddr) substart = memaddr;
                if(subend > memend) subend = memend;
                if(substart >= subend) continue;

                /* if there is any overlap, forget this one and add
                 * 1-2 subranges back
                 */
                cbi->memmap[m].mm_base_addr = cbi->memmap[m].mm_length = 0;
                if(substart > memaddr)
                        add_memmap(cbi, memaddr, substart-memaddr);
                if(subend < memend)
                        add_memmap(cbi, subend, memend-subend);
        }
}
```

逐条解码：

- 方向：`start` 向**下**取整到页，`end` 向**上**取整到页。这与 `add_memmap` 的方向相反，因为两处要的保证不同：`add_memmap` 只保证"写进账的条目自身页对齐"（不保证整段落在原描述符内），`cut_memmap` 要"不把用过的页留在账上"（起终点都向外取整，宁可多切一页）。
- `substart/subend` 的两次修正把切除区间夹进当前条目的范围内。这一步让"一条切除跨几个条目"变成若干次互不干扰的子切除。
- `if(substart >= subend) continue;`：本条目与切除区间无重叠，跳过。注意这里比较的是夹取后的值，所以"切除区间完全在本条目之外"与"恰好相切"是同一个分支。
- 命中时先把自己清零（`mm_base_addr = mm_length = 0;`），再用 `add_memmap` 把**左余量**与**右余量**补回去。补回的顺序是从低到高，但 `add_memmap` 找的是"从下标 0 起的第一个空槽"（`i386/pg_utils.c` 的 [`add_memmap`](minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap) 的循环从 `m = 0` 起），所以两条补回落在哪取决于数组此前的状态：若此前没有空洞，它们依次占用刚清空的那个槽与它后面的第一个空槽；若此前已有空洞（前一条解码说过，切除会留），左余量会落进**最早的**那个空洞，而不是刚清空的槽。手算切除后的槽位布局——进而算 `mmap_size`——时按"先填最早空洞"这条规则走才不会错。
- 只有左余量时补一段、只有右余量时补一段、两端都有时补两段。切除区间把整条覆盖时一段都不补。

手算一次。设记录里只有一条 `[0x100000, 0x1000000)`（1MB 起、16MB，`I386_PAGE_SIZE = 4096`），现在要切掉 `[0x400123, 0x401F00)`：一段模块的字节范围，两端都不对齐页。

```
起点向下取整：0x400123 % 0x1000 = 0x123  → start = 0x400000
终点向上取整：0x401F00 % 0x1000 = 0xF00  → end   = 0x401F00 + 0x100 = 0x402000

本条目 [0x100000, 0x1000000) 与 [0x400000, 0x402000) 相交：
  substart = max(0x400000, 0x100000) = 0x400000
  subend   = min(0x402000, 0x1000000) = 0x402000

清空该条目，然后补两段：
  substart(0x400000) > memaddr(0x100000)  → add_memmap(0x100000, 0x300000)  ← [0x100000, 0x400000)
  subend(0x402000)   < memend(0x1000000)  → add_memmap(0x402000, 0xBFE000)  ← [0x402000, 0x1000000)
```

口径：两个 add 的长度由 `substart-memaddr = 0x300000` 与 `memend-subend = 0xBFE000` 直接相减得到；两段长度之和 `0x300000 + 0xBFE000 = 0xEFE000`，加上被切掉的 `0x2000` 等于原来的 `0xF00000`，与手算一致。

**2.4 结算**：走到这里，机器上第一次有了**一份可信的可用物理内存清单**——条目来自引导器（细表或粗数两档之一），内核与各模块占用的区间将在 §2.5 从里面切掉（§2.4 尚未切），`mem_high_phys` 给出了上界。

#### 2.4.1 从记录里取址：两个消费者、两条取整规则与移交截断

除 §2.4 开头那两项操作（追加 `add_memmap`、切除 `cut_memmap`）外，`pg_utils.c` 还有两个从内存记录里取地址的函数——一个从最低处取、一个从最高处取（`i386/pg_utils.c` 的 [`alloc_lowest`](minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest)、`:pg_alloc_page`），本节讲的就是这两个；另有一个 `alloc_pagetable`，它不从记录取页，用的是自己的静态池，放在 §2.6 讲。2.4.1.4 另交代第三件事——移交 VM 时的 16 条截断；它不属于"取址"，但属于同一份清单的生命周期，紧接 `mmap_size` 的边界讨论放在这里。

```c
/*
 * 节选（保留与分配语义直接相关的行；省略号处为未改动的循环变量声明与方法体其余行；
 * 并略去源码原有的函数头注释），minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest 与 pg_alloc_page。
 */
phys_bytes alloc_lowest(kinfo_t *cbi, phys_bytes len)
{
	int m;
#define EMPTY 0xffffffff
	phys_bytes lowest = EMPTY;
	assert(len > 0);
	len = roundup(len, I386_PAGE_SIZE);

	assert(kernel_may_alloc);

	for(m = 0; m < cbi->mmap_size; m++) {
		if(cbi->memmap[m].mm_length < len) continue;
		if(cbi->memmap[m].mm_base_addr < lowest) lowest = cbi->memmap[m].mm_base_addr;
	}
	assert(lowest != EMPTY);
	cut_memmap(cbi, lowest, len);
	cbi->kernel_allocated_bytes_dynamic += len;
	return lowest;
}

phys_bytes pg_alloc_page(kinfo_t *cbi)
{
	...
	for(m = cbi->mmap_size-1; m >= 0; m--) {
		mmap = &cbi->memmap[m];
		if(!mmap->mm_length) continue;
		assert(mmap->mm_length > 0);
		assert(!(mmap->mm_length % I386_PAGE_SIZE));
		assert(!(mmap->mm_base_addr % I386_PAGE_SIZE));

		mmap->mm_length -= I386_PAGE_SIZE;

                cbi->kernel_allocated_bytes_dynamic += I386_PAGE_SIZE;

		return mmap->mm_base_addr + mmap->mm_length;
	}

	panic("can't find free memory");
}
```

##### 2.4.1.1 `pg_alloc_page`：从最高下标取页

**下标序与地址序的关系。** `pg_alloc_page` 从**下标最高**的条目开始取（`for(m = cbi->mmap_size-1; ...)`），取该条目的最后一页（先减长度，再返回"基址+新长度"）。"下标最高 = 地址最高"并不是一条被维护的不变量：进账按引导器报告的升序写入；切除时左余量从下标 0 找第一个空槽回填，右余量同样从下标 0 找第一个空槽，落点取决于数组此前状态：

| 数组此前状态 | 左余量落点 | 右余量落点 | 序是否断开 |
|---|---|---|---|
| 无更早空洞（含 m+1 已被占用） | 刚清空的槽 m | m 之后第一个空槽；若 m+1 已被占用则落到更后空槽或数组末尾 | 若 m+1 已被占用，右余量下标高于 m+1、地址可能更低（取决于更早切割形态） → 断开；m+1 空闲时不断开 |
| 有更早空洞 h < m | 空洞 h | 槽 m | 通常不断开；若空洞 h 的地址反而高于槽 m（取决于更早切割形态），仍会断开 |

所以不能写成"只要上方有条目，升序必破"；能站住的结论只有一条：C 不维护序，并且存在至少一种切割形态使下标序 ≠ 地址序（例如 m+1 被占用时右余量落到更后的空槽），因此 `pg_alloc_page` 按**下标**取不等于按**地址**取。它在取页时把用量记进 `kernel_allocated_bytes_dynamic`。

它取页前在循环体内有三条断言（另有函数入口的 `assert(kernel_may_alloc)`），其中 `assert(!(mmap->mm_length % I386_PAGE_SIZE))` 是"记录里所有长度都必须是页的整数倍"这条不变量在"发页"这一步的校验点（写账一侧由 `add_memmap` 的 `rounddown` 保证，见本节末的取整推演）。

##### 2.4.1.2 `alloc_lowest` 的消费者与校验缺口

`alloc_lowest` 则**不是**一条可行的"从低处分配"路径：切除那一行把长度当成结束地址传给 `cut_memmap`。签名、三种情况与四个前提的完整推演见紧随其后的 2.4.1.3 小节（两条取整规则、退化请求与 `alloc_lowest` 的参数错误）；本节只讲它的消费者与校验缺口。

`alloc_lowest` 的唯一调用点在 SMP 路径：`i386/arch_smp.c` 的 [`copy_trampoline`](minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline) 里 `trampoline_base = alloc_lowest(&kinfo, tramp_size);`，用途是把一小段 16 位启动代码拷到 1MB 以下给辅助处理器执行。紧接着的校验只查了返回值的上界：

```c
/*
 * 代码引用（自函数中部起引；前段为 16 位跳板字节数组与准备代码，此处省略），
 * minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline。
 */
	tramp_size = (unsigned) &__trampoline_end - tramp_start;
	trampoline_base = alloc_lowest(&kinfo, tramp_size);

	/* The memory allocator finds the lowest available memory.. 
	 * Verify it's low enough
	 */
	assert(trampoline_base + tramp_size < (1 << 20));
```

注意 `assert(trampoline_base + tramp_size < (1 << 20))` 检查的是"地址足够低"，而不是"这段内存已经从账上切掉"。所以这条校验通不过时能发现"分配得不低"，却检查不出"分配了但账没改"。

##### 2.4.1.3 两条取整规则、退化请求与 `alloc_lowest` 的参数错误

**若 `cut_memmap` 不把 `start` 向下、`end` 向上取整**：补回余量时走的仍是 `add_memmap`，它自带 `roundup(addr)`/`rounddown(len)`，所以**记录里不会出现非页对齐长度**——这一条不变量由 `add_memmap` 独立保证，与 `cut_memmap` 的取整无关。取整真正改变的只有一种情形：**退化请求**（`start == end`）且首尾地址未落在页边界上。带取整时它被扩成"这一页整页出局"；不带取整时夹取后 `substart >= subend`，一次循环都不命中，等于什么都没切——被切对象照样留在空闲清单里，等着被第二次发出去。（退化请求恰好页对齐时，两个取整都不生效，取不取整同样是"什么都没切"。）其余情形（起点未对齐、终点未对齐、区间跨页、整条覆盖）取不取整**结果相同**，因为补回的余量都要再过 `add_memmap` 的对齐，边界上的半个页本来就会被丢掉。两处取整的分工因此是：`add_memmap` 保证"账上永远不出现非页对齐的条目"，`cut_memmap` 的取整保证"带取整的切除请求至少让涉及的那一页出局"；退化且未对齐的请求在不取整时会漏切，这正是上一段的情形。方向是保守的，代价最多是一页可用内存。

**若 `alloc_lowest` 的切除参数写错会怎样**。`cut_memmap` 的第三个参数是**结束地址**（签名：`void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)`），而这里传的是 `len`——一个长度。于是"实际切掉的区间"是 `[lowest, len)`，"想要切掉的区间"是 `[lowest, lowest+len)`，两者相差正好 `lowest` 个字节。分三种情况：

| `lowest` 与 `len` 的关系 | 实际切掉 | 后果 |
|---|---|---|
| `lowest == 0` | `[0, len)`，与想要的一致 | 行为正确——但这个分支在真实调用序下不可达（`arch_init` 早已把 `[0, 0x1000)` 切掉，见下） |
| `0 < lowest < len` | `[lowest, len)`——切了，但**比分配量少了 `lowest` 字节** | 尾部那 `lowest` 字节仍挂在空闲清单上 |
| `len ≤ lowest` | 夹取后 `substart >= subend`，一次都不切 | 整块被占用的内存从未从账上除去 |

在下列四个前提同时成立时，真实调用序落在第三行（这是条件结论，不是无条件“常态”）。`alloc_lowest` 只在 SMP 路径被调用（`copy_trampoline`），那时 `kmain` 已经跑过 `cstart`，而 `cstart` 末尾的 `arch_init` 先切掉了两块低内存（`i386/arch_system.c` 的 [`arch_init`](minix3/minix/kernel/arch/i386/arch_system.c:arch_init) 的两条 `cut_memmap`：BIOS 区 `[0, 0x4FF]` 与 `[0x90000, 0xFFFFF]`，取整后即 `[0, 0x1000)` 与 `[0x90000, 0x100000)`）。所以表里最低条目的基址不可能还是 0（典型 QEMU/PC 上是 `0x1000`），而 `len` 经 `roundup` 是一页（跳板本体约 2KB）——`len ≤ lowest`，切除一次都不发生。

"常态是第三行"（缺陷状态成立）不是一条无条件结论，它同时压在四个前提上，每个都给复查判据：**前提一**，调用嵌套成立：`kmain` 先调 `cstart`（`main.c:147`），`arch_init` 在 `cstart` 体内、其两条 `cut_memmap` 在 `arch_system.c:277-278`；`smp_init` 在 `kmain` 里晚于 `cstart`（`main.c:311`）。**前提二**，最低条目基址非 0：`arch_system.c` 的 `arch_init`（`:246` 起）末尾两条 `cut_memmap`（`:277-278`）切掉含 0 的低段，E820 下最低条目基址为 `0x1000`。**前提三**，`roundup(tramp_size)` ≤ 最低基址：跳板约 2202 B（`trampoline.S` 的 16 位代码 + 固定 IDT 2048 B + 每 CPU 一个 8 B GDT 槽），`roundup` 为 `0x1000`；`2202 + 8×C ≤ 4096` 得 `C ≤ 236`，超过就跨页。**前提四**，`alloc_lowest` 没有第二个调用点：`rg -n "alloc_lowest" minix3/minix/kernel/arch/i386/` 命中定义、原型与唯一调用（`arch_smp.c:76`）。

任何一个前提在新目标上失效（最低条目被重新加回、跳板尺寸变化、出现第二个调用点），"常态是第三行"就要重推。也就是说这行代码在真实调用序下**就是**在"分配了但账没改"的状态下运行的；这个缺陷在本文核对过的调用序与四个前提下不会当场暴露（任一前提变化，暴露方式需重推；没有运行历史证据能证明它"从未"暴露），靠的是两点：其一，`pg_alloc_page` 从最高下标取页，跳板所在的低条目在观测常态下不是最高条目（下标序与地址序的偏差见 2.4.1.1 小节的"下标序与地址序"段）——这是观测常态，不是结构保证；其二，跳板的生命周期在 `smp_init` 完成、VM 启动之前就结束（本节末"两个取址方向各有其消费者"一段正是这个分工），而 VM 的分配器是首次适配（`minix3/minix/servers/vm/alloc.c` 的 `alloc_mem` 注释明写 first fit），它即使把低地址当空闲页发出，也发生在跳板不再使用之后。这不是它写对了。

后两种缺陷形态（`0 < lowest < len` 与 `len ≤ lowest`）的共同后果是：返回的地址会被 `copy_trampoline` 拷进 16 位启动代码，而这段物理内存**在账上仍然是空闲**（或至少残留一段）；此后任意一次 `pg_alloc_page` 都可以把它当成空闲页发出去，辅助处理器启动到一半时它的启动代码被改成页表内容——症状是某个 AP 起不来或执行到随机地址，而不是"内存分配失败"。这也解释了为什么看到"分配"两个字，要去确认它是否真的把那块内存从可用清单中除去了，而不是只看它返回了一个地址。

两个取址方向各有其消费者，上面两个函数恰好各走一头。`pg_alloc_page` 从最高条目取页，意在让记录里最低的那一段保持连续；唯一要求低地址的消费者是辅助处理器的启动跳板——`i386/arch_smp.c` 的 [`copy_trampoline`](minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline) 经 `alloc_lowest` 取它，并断言结果落在 1MB 以下——而 `alloc_lowest` 是从最低处扫的。但"两个方向互不消耗"只是**观测常态**而非结构保证：下标序可破（2.4.1.1 小节的"下标序与地址序"段），跳板内存仍挂在空闲账上时，任何一次 `pg_alloc_page` 都可能把它发出去——这正是"切除一次都不发生"的后果；真正兜住的只有取页方向与跳板生命周期（见上文"靠的是两点"那一段），不是那四个前提。

往后从这份账里取页的，是 `pre_init()` 之后的 4KB 细粒度映射路径与 VM 接手后的所有分配——引导期这次建表本身用的是静态页目录与静态池，不从账上领页（见 §2.6）；这些取页动作都以这份账为唯一依据。
##### 2.4.1.4 清单移交 VM 时的 16 条截断

另一件必须单说的事：**C 的 VM 接手这份账时会把它截到 16 条**。VM 只读前 `MIN(MAXMEMMAP, NR_MEMS) = 16` 条（`NR_MEMS = 16` 定义在 [`minix/const.h`](minix3/minix/include/minix/const.h)），17–40 号条目（按 C 的 1-based 编号；对应 0-based 下标 16..=39）被静默忽略——`vm/utility.c` 的注释原文是 "Any memory chunk in excess of NR_MEMS is silently ignored"。账在 40 条容量里建、在 16 条口径里被消费，这条截断是"清单怎么传下去"的一部分；Rust 侧怎么处理它，见 §3.4 的 memmap 行。Rust 侧对这份清单也有一组三级容量上限与停机行为：128/64（`free_regions`）/64（`ident`）的权威说明在 §3.4.1 的 `memmap` 行，落地区本身的 128 容量在 §4.5 表；这里不重复数字。

#### 2.4.2 模块内存与 unpaged 段的归属变更（下游延伸；§1.4 已声明的越界交代）

`cut_memmap` 在 `pre_init` 里被调用时只是一次**临时**切除。接下来两处"改这份账"的动作发生在 `pre_init()` 返回之后（本章主线的下游延伸）——要把这份记录的完整生命周期一次讲清，不能只看引导准备那一段。下表把这两处连同 `pre_init` 内的"切占用"一处并列（第一行属 §2.5，只作对照，不是本节主体），对象各不相同；下面一律**按动作命名**（按先后编号容易和别处的"第几点"混起来）：

| 名称 | 对象 | 时刻 | 谁做 | 动作 | 为什么 |
|---|---|---|---|---|---|
| **切占用**（§2.5 执行，不是 §2.4） | 内核与全部模块的字节 | `pre_init` → `get_parameters` | `cut_memmap`（本文 2.4） | 从可用记录里切掉 `[mod_start, mod_end)` | 这段内存里躺着模块的字节，页表页与内核数据不能分配到这里 |
| **还 VM 镜像** | 只有 VM 模块的字节 | 内核装载启动进程时（[`arch_boot_proc`](minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc)） | `add_memmap` + 清空模块描述符 | 把模块的字节段复制进目标地址空间后，把这段物理区间加回记录，并把 `mod_start`/`mod_end` 置 0 | 字节已经搬到别处，原地址可以复用；置 0 是"已回收"的标记 |
| **还引导段** | 内核自己的 unpaged 段 | `kmain` 后段（`cstart`、`system_init` 之后） | `add_memmap(bootstrap_start, bootstrap_len)`（[`main.c`](minix3/minix/kernel/main.c) 的 `kmain` 体内、`system_init` 之后；此时 `kernel_may_alloc` 仍为 1——它要到 `bsp_finish_booting` 末尾、`switch_to_user()` 前两行（中间隔一空行）才写 0） | 归还分页开启前使用的那一小段 | C 的 `unpaged` 段只在启动期需要（本文 §3.5），与模块无关 |

三条容易混的口径：**"切占用"处的是模块字节连同内核镜像自己那一块，"还 VM 镜像"处的是模块字节，"还引导段"处的是 unpaged 段**（边界由链接脚本导出的 `_kern_unpaged_start/end` 给出）。另外，"还 VM 镜像"只还 VM 那一块，其余模块的字节区间在 C 里从头到尾都没加回过——代价见本节末。

"还 VM 镜像"的源码（本篇 §3.8 引用的扣除表把这条 C 对应关系**改述**过一行，不是代码引用）：

```c
/*
 * 代码引用（保留与该动作直接相关的行），minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc 中 VM 模块装载完成处。
 */
		arch_proc_init(rp, execi.pc, (vir_bytes)sp,
			execi.stack_high - sizeof(struct ps_strings),
			ip->proc_name);

		/* Free VM blob that was just copied into existence. */
		add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start);
		mod->mod_end = mod->mod_start = 0;

		/* Remember them */
		kinfo.vm_allocated_bytes = alloc_for_vm;
```

读这段源码要留意的三点：

- `add_memmap` 的实参是 `mod->mod_start` 与 `mod->mod_end - mod->mod_start`（起点与**长度**），与 2.4 里 `cut_memmap(cbi, lowest, len)` 那个把长度当结束地址的写法形成对照：同一个文件里的两组调用对"第三个参数是什么"并不一致（`add_memmap` 的第三个参数是长度、`cut_memmap` 的是结束地址），两组调用只能逐个核对被调函数的签名。
- 置零发生在加回之后，且两个字段一起置 0。此后任何遍历 `module_list[]` 的代码都会看到一条"空模块"；这是"已回收"在数据结构里的表示（而不是另设一个布尔位）。
- 这一次回收只针对 VM 的模块（`vm_allocated_bytes` 紧跟着被记录），因为它的字节已经被复制进 VM 的地址空间。其余模块在 C 里没有对应的加回动作（见本节末的代价分析）。

**其余模块在“内核账”上不归还，代价是什么**：内核 `kinfo.memmap` 的可用总量里，12 个引导模块里除 VM 之外的 11 个（名单与来历见 §4.3）的字节区间一直缺着：`cut_memmap` 把命中区间从记录中切走，再用 `add_memmap` 把左右余量补回；模块落在一条大区间内部时，结果是原条目被切成两段，那一段地址不再以任何条目存在（只有模块恰好覆盖整条目时，才留下一个 `mm_length` 为 0 的空条目），此后没有 `add_memmap` 把它加回。但这不等于系统永久少掉这段内存：VM 启动时先对每个模块 `mem_add_total_pages(len/VM_PAGE_SIZE)` 把总量补进自己的位图，随后 `exec_bootproc` 把模块内容装进进程地址空间，再用 `free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len))` 把 blob 放回 VM 的空闲位图（`minix3/minix/servers/vm/main.c:482-520`）。所以准确说法是两本账：内核引导期的 memmap 不再收回，VM 接手后由 VM 回收；在 VM 接手之前，这段内存不可用。

这条代价不该被夸大成病根：内存宽裕时少这几段照样启动完成（C 就是这么跑完的）；内存本来就不够时，就算内核账把 VM 之外那 11 段也还回去，只要缺口大于还回量，一样会分配失败（若缺口小于还回量，结果可能改变；这条代价只讨论常态）。它至多是压垮可用总量的最后几页，而不是原因。启动期唯一会走到 `pg_alloc_page` 的是装载启动镜像那条链（`i386/protect.c` 的 [`libexec_pg_alloc`](minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc) 传 `PG_ALLOCATEME` 给 `pg_map`），所以真到了耗尽那一刻，panic 会打在装载途中——但那只是**报信的地方**，不是**出错的地方**。

它真正的坏处是安静：不报错、不留痕。也因此有确定的查法——引导期的占用都记在两个账上：`kernel_allocated_bytes` 是内核镜像自己那一块（`_kern_size` 减去 `bootstrap_len`），`kernel_allocated_bytes_dynamic` 在 `pg_alloc_page` 上每发一页累加一次、在 `alloc_lowest` 上每分配一段累加一次；而内核账缺的那几段**两边都不记**；VM 接手后它们会回到 VM 的空闲位图，但引导期到 VM 启动之间不可用。拿固件报告的 E820 可用总量依次减掉这两笔，再去对 `memmap[]` 的可用总量，对不上的那一截就是它。对账时按这两种粒度代入。

"还 VM 镜像"在新设计里的处理与这里不同（落点与理由见 §3.8 的扣除表）；"还引导段"这一步则不存在——Rust 侧没有 unpaged 段，回收区间字段也从交接结构里删掉了（§3.5）。

**2.4.2 结算**（C 侧）：这三处下游动作在 C 里把模块字节交给系统、把内核镜像排除在可用清单之外、并在 `kmain` 后段归还 unpaged 段。Rust 侧没有 unpaged 段，模块字节改由 `boot_modules` 交接并由扣除表整段保留——这些落点属于第 3 章，见 §3.5 与 §3.8，本节不再展开。

### 2.5 overlaps 与 kern_mod：内核把自己当成第 M+1 个模块（M = 模块数）

内存账建好后，要从中切掉两块东西：模块占用的区间与内核自己占用的区间。切之前先检查它们彼此是否重叠——`overlaps` 就是源码注释里说的那道 sanity check。实际布局里相邻模块由 boot2 顺序摆放、互不重叠：boot2 一边装载一边推进 `image_end`（[`lib/exec.c`](minix3/sys/arch/i386/stand/lib/exec.c) 里 `bi->base = image_end;` … `image_end += len;`，下一条再从页对齐处接着放——前一条长度恰为页的整数倍时两者正好相切，否则中间留一段对齐补白），所以正常情况下谁也不会盖住谁。但"相切"恰恰是边界处理最容易写错的那一种相邻关系——本节末尾就专门推演它。真出现重叠（换个引导器、或模块与内核被放在同一处）时——**若没有这道检查**——症状会非常难追：两个镜像互相覆盖而无人报错。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pre_init.c:overlaps。
 */
int overlaps(multiboot_module_t *mod, int n, int cmp_mod)
{
	multiboot_module_t *cmp = &mod[cmp_mod];
	int m;

#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) < mod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
			INRANGE(mod1, mod2->mod_end-1))
	for(m = 0; m < n; m++) {
		multiboot_module_t *thismod = &mod[m];
		if(m == cmp_mod) continue;
		if(OVERLAP(thismod, cmp))
			return 1;
	}
	return 0;
}
```

逐条解码：

- 区间语义是半开区间 `[start, end)`：`INRANGE` 写成 `v >= start && v < end`。`mod_end` 为什么是"最后一个字节之后的那个地址"？依据在引导器一侧：boot2 填的是 `mbm[i].mmo_end = bim->base + bim->len`（起点加长度，同在 [`lib/exec.c`](minix3/sys/arch/i386/stand/lib/exec.c)）。注意 minix3 的 `multiboot.h` 对 `mmo_end` **一句注释都没写**——这个语义只能从填它的那行读出来，而不是从结构体定义里看到。
- `OVERLAP` 判两个点：对方区间的**起点**与对方的**最后字节**。第二项写成 `mod2->mod_end - 1`，就是把半开区间翻译回"最后一个有效字节"。
- `overlaps` 检查 `mod[cmp_mod]` 与其余每一条，只要有一条相交就返回 1。注意它是对每条记录各调用一次（外层循环在 `get_parameters` 里），所以一对重叠会被两个方向各检查一次；这使第二项 `-1` 的必要性变得具体（见下）。

内核自己不带 `multiboot_module_t`，但检查与切除都要把内核算进去（检查那一半是 `i386/pre_init.c` 的 [`overlaps`](minix3/minix/kernel/arch/i386/pre_init.c:overlaps)），于是 `i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 把它塞进模块数组的第一个空槽：

```c
/*
 * 节选（保留与"内核当模块"直接相关的行），minix3/minix/kernel/arch/i386/pre_init.c:get_parameters 后段。
 */
	/* Sanity check: the kernel nor any of the modules may overlap
	 * with each other. Pretend the kernel is an extra module for a
	 * second.
	 */
	k = mbi->mi_mods_count;
	assert(k < MULTIBOOT_MAX_MODS);
	cbi->module_list[k].mod_start = kernbase;
	cbi->module_list[k].mod_end = kernbase + kernsize;
	cbi->mods_with_kernel = mbi->mi_mods_count+1;
	cbi->kern_mod = k;

	for(m = 0; m < cbi->mods_with_kernel; m++) {
		...
		if(overlaps(cbi->module_list, cbi->mods_with_kernel, m))
			panic("overlapping boot modules/kernel");
		/* We cut out the bits of memory that we know are
		 * occupied by the kernel and boot modules.
		 */
		cut_memmap(cbi,
			cbi->module_list[m].mod_start, 
			cbi->module_list[m].mod_end);
	}
```

三处具体事实：

- `kernbase`/`kernsize` 不来自引导器，是链接脚本导出的常量（`_kern_phys_base = 0x00400000`、`_kern_vir_base = 0xF0400000`；`_kern_size` 由链接布局算出、不是字面量。完整摘录与解读见 §3.5）。这里 `_kern_phys_base`/`_kern_size` 第一次被用到（`_kern_vir_base` 在 §2.3 已出现）：**内核要参与"谁占了哪段内存"的核查，就只能自己报出自己的地址范围**——所以它把自己造成第 M+1 个模块（M = 模块数），而不是走一条特例分支。
- `assert(k < MULTIBOOT_MAX_MODS)` 是"内核还要占一个槽"这一事实的形式化：模块数最多只能是 `MULTIBOOT_MAX_MODS - 1`，因为有 1 个槽要留给内核。它与 2.1 里那条"模块数 < `MULTIBOOT_MAX_MODS`"是同一个条件（`k = mbi->mi_mods_count`），作用是在往 `module_list[k]` 写"内核当模块"这一项之前，把"数组还有空槽"这个前提再确认一次。
- 同一个循环里既检查又切除：`mods_with_kernel` 条记录，每条先与其余所有记录比一次，然后从账上切除。切除用的是 `[mod_start, mod_end)`，与 `cut_memmap` 的起止语义一致。

于是"内核与模块占用区"这一整类内存只被一段代码处理，模块与内核之间没有第二套逻辑。

**把 `OVERLAP` 第二项的 `-1` 去掉会怎样**：那一项就变成 `INRANGE(mod1, mod2->mod_end)`。取两个首尾相接的模块：A 占 `[0x100000, 0x200000)`、B 占 `[0x200000, 0x300000)`。它们不重叠——A 的最后一个字节是 `0x1FFFFF`，B 的第一个字节是 `0x200000`。用不带 `-1` 的写法检查：

```
外层 m = 0（A 是 cmp），内层遍历到 B：OVERLAP(B, A) = INRANGE(B, A.mod_end = 0x200000)
   = (0x200000 >= 0x200000) && (0x200000 < 0x300000) = 真
```

于是 `panic("overlapping boot modules/kernel")` 落在一个完全合法的布局上。这正是"半开区间必须翻译对"这一类错误的代价：报错信息说"模块重叠了"，而真实情况是模块之间恰好相切。再注意上面那句"两个方向各检查一次"在这里的意思：真重叠时只要有一个方向报出即可（包含关系只由外层区间检查内层端点的那一次发现），而相切的误报只需要一个方向成立就够——外层以 A 为比较对象的那一次（`cmp_mod = 0`）在内层遍历到 B（`m = 1`）时就已经 panic，反方向那一次根本轮不到执行。

顺带说准一件容易搞反的事：**把 `overlaps` 整个去掉，内存账并不会被弄坏**。同一段物理内存被两个模块声明时，第二次 `cut_memmap` 落在已被排除的区间上，是空操作（按 `add_memmap` 与 `cut_memmap` 的取整规则推演：先切 `[0x120000, 0x150000)`、再切 `[0x130000, 0x140000)`，清单仍是那两截）。代价全在内容侧，就是本节开头那句话：两份镜像在重叠地址上互相覆盖，而内核一声不响——所以 `overlaps` 守的是**模块字节不被踩**，不是内存账不被记错。

### 2.6 页表建立步骤与建表之后的下游（借槽窗口与 VM 继承）

> 本节末尾的 §2.6.5 是 C i386 的位、掩码与常量参考表（Rust 侧常量见各设计节），按需查阅，不必顺序读。

#### 2.6.1 建表五步：三个函数逐行解码，两步见函数表

`pre_init()` 的函数体共八条语句：一条断言 + 一条收集调用 + 五步建表/开启 + 一条 `return`；下面说的"前一条收集、后五条建表与开启"指中间那六条。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pre_init.c:pre_init。
 */
kinfo_t *pre_init(u32_t magic, u32_t ebx)
{
	assert(magic == MULTIBOOT_INFO_MAGIC);

	/* Get our own copy boot params pointed to by ebx.
	 * Here we find out whether we should do serial output.
	 */
	get_parameters(ebx, &kinfo);

	/* Make and load a pagetable that will map the kernel
	 * to where it should be; but first a 1:1 mapping so
	 * this code stays where it should be.
	 */
	pg_clear();
	pg_identity(&kinfo);
	kinfo.freepde_start = pg_mapkernel();
	pg_load();
	vm_enable_paging();

	/* Done, return boot info so it can be passed to kmain(). */
	return &kinfo;
}
```

这段注释本身就是对本篇 1.2 那次序列表述的原文支持：`but first a 1:1 mapping so this code stays where it should be`。五步各自的职能：

| 步 | 函数 | 改的是什么 | 输入 |
|---|---|---|---|
| 1 | [`pg_clear`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_clear) | 静态页目录数组 `pagedir[1024]` 清零 | 无 |
| 2 | [`pg_identity`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity) | 把低地址全部映射成恒等（VA = PA） | `kinfo.mem_high_phys` |
| 3 | [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel) | 把内核镜像映射到它的高虚拟基址，返回第一个空闲页目录下标 | 链接期常量 + `pagedir[]` |
| 4 | [`pg_load`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_load) | 把页目录物理地址写进 `CR3` | 页目录地址 |
| 5 | [`vm_enable_paging`](minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging) | 置 `CR0.PG`、`CR0.WP` 与 `CR4.PSE`、`CR4.PGE`（`PGE` 按 CPU 能力置） | `CR0`/`CR4` 当前值 |

步骤 2 与步骤 3 写同一张数组，这是它们必须有序的机制来源。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity。
 */
void pg_identity(kinfo_t *cbi)
{
	uint32_t i;
	phys_bytes phys;

	/* We map memory that does not correspond to physical memory
	 * as non-cacheable. Make sure we know what it is.
	 */
	assert(cbi->mem_high_phys);

        /* Set up an identity mapping page directory */
        for(i = 0; i < I386_VM_DIR_ENTRIES; i++) {
		u32_t flags = I386_VM_PRESENT | I386_VM_BIGPAGE
			| I386_VM_USER
			| I386_VM_WRITE;
                phys = i * I386_BIG_PAGE_SIZE;
		if((cbi->mem_high_phys & I386_VM_ADDR_MASK_4MB)
			<= (phys & I386_VM_ADDR_MASK_4MB)) {
			flags |= I386_VM_PWT | I386_VM_PCD;
		}
                pagedir[i] =  phys | flags;
        }
}
```

逐条解码：

- 函数开头的 `assert(cbi->mem_high_phys)` 就是"内存清单先于建表"（§1.2/§2.8）的硬出口：它不为零才走得下去，而反排时该值还是 `.bss` 的零初值。
- `I386_VM_DIR_ENTRIES = 1024`、`I386_BIG_PAGE_SIZE = I386_PAGE_SIZE * I386_VM_PT_ENTRIES = 4096 * 1024`（4MB，定义在 [`include/vm.h`](minix3/minix/include/arch/i386/include/vm.h)）。两个数一乘正好是 4GB——对 32 位来说就是**整个地址空间**。所以这个循环不是在算"该映射哪些"，而是**一次把 1024 项全部填完**：在 4MB 大页下这是最省事的做法，代价固定（只要一个页目录，不需要任何页表页）。

  不要把"省事"当成"必须"。真正必需的覆盖只有两块——当前正在执行的地址与内核要去的地址，它们由 1.2 那两条讲过；全铺是达成它的廉价手段，不是它本身的要求。反例就在本篇：64 位不可能全铺，我们的 boot 恒等映射于是取一个固定上界 `IDENTITY_MAP_END`（唯一权威是 [`minix-arch` 的 `BOOT_IDENTITY_MAP_END`](os/arch/src/lib.rs)，kernel 侧以 `use minix_arch::BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END` 别名引用，值就是低 4GB；见 §4.6）——同一条决策，只是 32 位下这个上界恰好等于整个空间。

  全铺也带来了代价，方向与"更安全"相反：内存上界（4MB 对齐后）及以上的不存在地址也被标成 present，只是额外加上 `PWT|PCD` 变成非缓存，所以那一段越界访问不会触发页错误，而是读到不确定的内容——相对"只映射真实内存"的方案，全铺放弃了"越界即页错误"；C 本来也没有这条性质（`PWT|PCD` 不产生页错误），Rust 的差异只在未登记空洞走可缓存（下面几条接着说）。
- 每个条目都置 `I386_VM_PRESENT | I386_VM_BIGPAGE | I386_VM_USER | I386_VM_WRITE`：存在、是大页（该条目直接指向 4MB 物理块而不是二级页表）、用户可访问、可写。这里的 `I386_VM_USER` 要说清一句：boot 阶段内核在 Ring 0 执行，U/S 位不参与内核自己的数据访问判定，所以**这一位在 C 里没有实际效果——前提是 `CR4.SMEP = 0` 且 SMAP 不拦数据访问**（SMEP 禁止 supervisor 从 U=1 的页取指；SMAP 禁止 supervisor 对 U=1 页的数据访问，除非 `AC=1`；C 的 `vm_enable_paging` 只清 `PGE|PSE`，不碰这两位，所以这条依赖固件没有置 SMEP/SMAP）。Rust 侧的恒等映射不置 USER，正是为了不依赖这个前提。有意义的对照是 `pg_mapkernel` **不置**它（内核地址被标成 supervisor-only），而恒等映射也不会留给用户态用——VM 的 `pt_init` 跳过 BIGPAGE 条目、自己新建页目录。新设计里的对应选择（boot 用的恒等映射不置该位）见 §4.6。
- 条件比较的是"内存上界截到 4MB 边界"与"本条目起始地址"：`(mem_high_phys & I386_VM_ADDR_MASK_4MB) <= (phys & I386_VM_ADDR_MASK_4MB)`。注意 `I386_VM_ADDR_MASK_4MB` 的值是 `0xFFC00000`，它是**清掉低 22 位、向下取整到 4MB**（不是向上），且右边的 `phys` 本来就是 4MB 的整数倍、那半边掩码是空操作；真正起作用的是把内存上界抹到边界上。成立时额外加 `I386_VM_PWT | I386_VM_PCD`。源码注释给了理由："We map memory that does not correspond to physical memory as non-cacheable."
- 向下取整在边界上是**保守（多包一圈）**而不是漏包：设内存上界是 `0x2001000`（32MB 加一页），抹到边界得 `0x2000000`，于是 8 号条目（起点正好 `0x2000000`）满足 `<=` 而被标成非缓存——**尽管它开头那一页是真内存**。反方向取整就会漏：那一段里真正不存在的地址会被当成普通可缓存内存，才是真危险。所以这个方向的取舍是"宁可把一段真内存当设备慢一点访问（结果仍正确），也不把设备或空洞当内存去缓存"。
- `PWT`/`PCD`（页写穿透 / 页禁缓存）的用途：那些地址后面不是 DRAM，而是设备寄存器或空洞。设备读写的语义依赖"每次访问都真的到达设备"；若走缓存，读可能命中缓存里没有被设备更新过的旧值，写可能在写缓冲区里停留。所以"超出物理内存上界"这个条件至少圈出内存上界之上那一整圈（设备寄存器与紧贴上界的高端空洞）——上界之下的空洞与保留区不在此列（它们进账时就被丢弃，见 §2.4）；但“不分配”不等于“运行期不访问”，可缓存安全性依赖同一前提（见 §4.6）。
- 这个函数把 4GB 全部置为 PRESENT。所以"有没有映射"在这里不构成区分；缓存属性两位只在（4MB 对齐后的）内存上界及以上这一段参与区分"真实内存"与"设备/空洞"，上界之下的空洞与真实内存没有位级区分。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel。
 */
int pg_mapkernel(void)
{
	int pde;
	u32_t mapped = 0, kern_phys = kern_phys_start;

        assert(!(kern_vir_start % I386_BIG_PAGE_SIZE));
        assert(!(kern_phys % I386_BIG_PAGE_SIZE));
        pde = kern_vir_start / I386_BIG_PAGE_SIZE; /* start pde */
	while(mapped < kern_kernlen) {
	        pagedir[pde] = kern_phys | I386_VM_PRESENT | 
			I386_VM_BIGPAGE | I386_VM_WRITE;
		mapped += I386_BIG_PAGE_SIZE;
		kern_phys += I386_BIG_PAGE_SIZE;
		pde++;
	}
	return pde;	/* free pde */
}
```

逐条解码：

- `kern_vir_start`/`kern_phys_start`/`kern_kernlen` 是文件顶部的静态量，初值来自链接脚本导出的 `_kern_vir_base`/`_kern_phys_base`/`_kern_size`（`_kern_vir_base = 0xF0400000`、`_kern_phys_base = 0x00400000`、`_kern_size` 由链接布局算出；完整摘录见 §3.5）。它们为什么这里要用文件级变量而不是参数：同一个文件里两个函数签名不同——`pg_identity` 收一个 `kinfo_t *`，而 `pg_mapkernel(void)` **不收参数**，它要拿到"内核在哪、多大"就只能读文件级静态量。拿不到参数也要能建表，这件事也印证了同一句话：**建表所需的布局信息不属于引导器给的数据**。
- 两条断言要求两个基址都按 4MB 对齐。索引从 `kern_vir_start / I386_BIG_PAGE_SIZE` 开始：`_kern_vir_base = 0xF0400000`（`minix3/minix/kernel/arch/i386/kernel.lds`），除以 4MB 得下标 `961`。
- 循环把内核镜像按 4MB 一块地映射进高地址区。条目里**没有** `I386_VM_USER`——内核地址对用户态不可访问，这是与恒等映射故意相反的一处。
- 返回值是循环结束后的 `pde`，也就是内核映射占完之后第一个未被占用的页目录下标。注释写作 `free pde`。它的下游见 §2.6.3。

**把 `pg_identity()` 排到 `pg_mapkernel()` 之后会怎样**：两个函数写同一个 `pagedir[]`，索引集合**重叠**：`pg_identity` 写 0..1023 全部 1024 个条目，`pg_mapkernel` 写从 961 开始的若干条目（`_kern_vir_base = 0xF0400000` 除以 4MB）。先写内核映射、后写恒等映射，961 号及之后的若干条目会被"恒等映射 + 4MB 大页"覆盖：`pagedir[961]` 从 `0x00400000 | …`（`_kern_phys_base`）变成 `0xF0400000 | …`（961 × 4MB），两者相差 `_kern_offset = 0xF0000000`，**不相等**。切换到高地址执行时，高半基址 `0xF0400000` 起的映射被 961 号条目解释成物理地址 `0xF0400000`——该物理块不含内核镜像；若它在内存上界之上（被 `pg_identity` 标了 `PWT|PCD`），取指不会产生页错误，若它仍在内存上界之内则是 DRAM、只取到非内核字节——两种都不报错，CPU 随后执行到非预期指令，失败方式不确定。`pg_identity` 覆写下标 0–1023（含 1023）；其中有害的是内核镜像占据的 961 到 `961 + ceil(kern_size/4MB) - 1`。次序错在这里不会报"映射冲突"，也不会在第一条指令上给出可定位的页错误。

第三个函数（本文逐个解码的三个函数里）是唯一改 `CR0`/`CR4` 模式与特性位的那个；装根（`CR3`）在上一步 `pg_load` 已完成：

```c
/*
 * 节选（保留与 CR0/CR4 次序直接相关的行；省略号处为 PAE 能力检查与注释），
 * minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging。
 */
void vm_enable_paging(void)
{
        u32_t cr0, cr4;
        int pgeok;

        pgeok = _cpufeature(_CPUF_I386_PGE);

	...
        cr0= read_cr0();
        cr4= read_cr4();

	/* The boot loader should have put us in protected mode. */
	assert(cr0 & I386_CR0_PE);

        /* First clear PG and PGE flag, as PGE must be enabled after PG. */
        write_cr0(cr0 & ~I386_CR0_PG);
        write_cr4(cr4 & ~(I386_CR4_PGE | I386_CR4_PSE));

        cr0= read_cr0();
        cr4= read_cr4();

        /* Our page table contains 4MB entries. */
        cr4 |= I386_CR4_PSE;

        write_cr4(cr4);

        /* First enable paging, then enable global page flag. */
        cr0 |= I386_CR0_PG;
        write_cr0(cr0);
        cr0 |= I386_CR0_WP;
        write_cr0(cr0);

        /* May we enable these features? */
        if(pgeok)
                cr4 |= I386_CR4_PGE;

        write_cr4(cr4);
}
```

逐条解码：

- 入口先读一次 `CR0`/`CR4`（把"当前值"读回来再按位改），断言保护模式位 `PE` 已置（引导器的责任）。
- **先清，再置**，而且清掉的位里有一个要等到最后才回来：先清 `PG`、清 `PGE|PSE`，随后只重设 `PSE`（4MB 大页成立的前提），再开 `PG`，最后才按 CPU 能力置 `PGE`。**这个先后不能倒**：`pg_identity` 与 `pg_mapkernel` 写的每一条表项都带 `I386_VM_BIGPAGE`（bit 7），而 `PSE = 0` 时这一位是保留位——先开 `PG` 就等于让 CPU 拿着一张每一位都非法的页目录去取指。
- 注释给的两条理由分别是："PGE 必须在 PG 之后启用"与"Our page table contains 4MB entries"（PSE 必须为 1，4MB 大页才成立）。
- `WP`（写保护）在开 `PG` 之后单独置一次。它的效果是"内核态也不能写只读页"，是 W^X 的基础。
- `pgeok` 来自 CPU 能力检测（`_cpufeature(_CPUF_I386_PGE)`）。有能力的 CPU 才置 `PGE`：该位为 1 时，带 G 位的页表项在切换 `CR3` 后仍留在 TLB 里。

> `CR0` 与 `CR4` 是 x86 的控制寄存器，用法都是"读回来、改位、再写回去"。`CR0` 说"机器处在什么模式"，`CR4` 说"启用哪些扩展特性"。上面涉及的四位：`CR0.PE`（bit 0）保护模式，由引导器负责置好、内核只断言它已置；`CR0.PG`（bit 31）分页总开关，置起后每一取指、每一访存都要过页表；`CR4.PSE`（bit 4）大页使能，它允许页目录项直接指向一个 4MB 物理块而不是下一级页表；`CR4.PGE`（bit 7）全局页，配合表项的 `G` 位让映射在切换 `CR3` 时不被冲刷。各位的位值与一处头文件注释错误，见 §2.6.5 的表。

#### 2.6.2 页表页从哪来：静态池与动态取页（主路径的供给）

页表本身也要占物理页，而 4KB 细粒度映射里"被映射的那一页"也可能要现分配——这是两件事，C 里分两层承担：`i386/pg_utils.c` 的 [`alloc_pagetable`](minix3/minix/kernel/arch/i386/pg_utils.c:alloc_pagetable) 管**页表页**（静态池）；4KB 细粒度映射的入口是 `:pg_map`，它在需要现取**被映射页**时调用 2.4.1.1 已讲过的 `:pg_alloc_page`：

```c
/*
 * 节选（只含 alloc_pagetable；pg_map 的页表挂载段在正文散引），minix3/minix/kernel/arch/i386/pg_utils.c:alloc_pagetable。
 */
u32_t *alloc_pagetable(phys_bytes *ph)
{
	u32_t *ret;
#define PG_PAGETABLES 6
	static u32_t pagetables[PG_PAGETABLES][1024]  __aligned(4096);
	static int pt_inuse = 0;
	if(pt_inuse >= PG_PAGETABLES) panic("no more pagetables");
	assert(sizeof(pagetables[pt_inuse]) == I386_PAGE_SIZE);
	ret = pagetables[pt_inuse++];
	*ph = vir2phys(ret);
	return ret;
}
```

- 6 个页表页是**编译期固定的静态池**：`static u32_t pagetables[6][1024] __aligned(4096)`。用完（`pt_inuse >= 6`）直接 `panic("no more pagetables")`，没有扩容路径。
- 页表页的物理地址由 `vir2phys(ret)` 得到，而 `pre_init.c` 里 `vir2phys` 的定义是 `return (phys_bytes) addr;`——引导期恒等映射成立，所以"虚拟地址即物理地址"这句话在这个函数里是可执行的。

`pg_map()` 在 4KB 粒度映射时挂页表：`pagedir[pde] = (ph & I386_VM_ADDR_MASK) | PRESENT | USER | WRITE;`，随后 `pt[pte] = (source & I386_VM_ADDR_MASK) | PRESENT | USER | WRITE;`。这两行是"两级页表"的实现：页目录项指向页表页（`ph` 来自 `alloc_pagetable`），页表项指向物理页（`source` 是调用方给好的页，或传 `PG_ALLOCATEME` 时由 `pg_alloc_page` 现取），两级都带 USER，因为 4KB 细粒度映射的用途之一是给用户地址空间做准备——这里 USER 位有实际意义（条目最终服务用户态）；与 `pg_identity` 的 USER 位不同：后者只服务 Ring 0 的内核访问，无实效（前文已述）。

`pg_map` 入口还有两个约束，它们划定了这条路径的适用范围：

```c
/* 节选（保留入口的占位判断与地址断言），minix3/minix/kernel/arch/i386/pg_utils.c:pg_map。 */
	if(phys == PG_ALLOCATEME) {
		assert(!(vaddr % I386_PAGE_SIZE));
	} else  {
		assert((vaddr % I386_PAGE_SIZE) == (phys % I386_PAGE_SIZE));
		vaddr = pg_rounddown(vaddr);
		phys = pg_rounddown(phys);
	}	
	assert(vaddr < kern_vir_start);
```

两种用法各要什么（另有一条隐含前提：`mapped_pde`/`pt` 是静态状态，`pg_map` 只在 `pde` 增大时换页表；当前调用序保证 pde 单调，否则后续更低的 pde 会把 PTE 写进旧页表）：调用方**已经握着物理页**时，两边必须页内偏移一致（`vaddr % 4096 == phys % 4096`），因为一个页框只能装下偏移相同的那些地址；两个地址随后都向下取整到页边界。调用方**还没有物理页**时，传哨兵 `PG_ALLOCATEME`——它定义在 [`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h)，值为 `((phys_bytes)-1)`，含义是"物理页由你分配"，所以它不是一个合法地址；此时只要求虚拟地址页对齐，每个虚拟页的物理页由 `pg_alloc_page` 在循环里现取（`i386/protect.c` 的 [`libexec_pg_alloc`](minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc) 装载启动镜像时就是这么用的）。最后那条 `assert(vaddr < kern_vir_start)` 把这条 4KB 路径限制在**低半地址**：高半内核区已由 `pg_mapkernel` 用大页一次建好，真正的用户页表则由 VM 在它自己的地址空间里建——这条路径只是内核要访问某个用户地址范围时先搭的架子。

#### 2.6.3 建表之后的下游之一：借槽窗口

`pg_mapkernel()` 那个返回值 `freepde_start` 也是这套机制的起点（消费者是 `i386/memory.c` 的 [`memory_init`](minix3/minix/kernel/arch/i386/memory.c:memory_init) 与 `:createpde`）。它的语义是"内核映射占完之后，页目录里第一个空闲的槽位号"，而它的消费者要的是"两个可以借用的窗口"：

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/memory.c:memory_init。
 */
void memory_init(void)
{
	assert(nfreepdes == 0);

	freepdes[nfreepdes++] = kinfo.freepde_start++;
	freepdes[nfreepdes++] = kinfo.freepde_start++;

	assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);
	assert(nfreepdes == 2);
	assert(nfreepdes <= MAXFREEPDES);
}
```

读这段之前先认四个名字：`get_cpulocal_var(x)` 取本 CPU 的 per-CPU 变量 `x`；`ptproc` 是其中一个槽，指"本 CPU 当前正在服务的进程"；`p_seg.p_cr3_v` 是该进程页目录的内核虚拟指针；`pde = freepdes[free_pde_idx]` 是本次借用的页目录下标（`freepdes[0]` 来自 `kinfo.freepde_start`，而 `pg_mapkernel` 返回的是第一个空闲槽，所以第一个方向写进 `p_cr3_v[961+k]`）。

```c
/*
 * 节选（保留"把对方页表的条目写进当前页表"这一段；省略号处为 `pdeval` 构造、物理分支 PDE 构造与返回地址计算），
 * minix3/minix/kernel/arch/i386/memory.c:createpde。
 */
static phys_bytes createpde(
	const struct proc *pr,	/* Requested process, NULL for physical. */
	const phys_bytes linaddr,/* Address after segment translation. */
	phys_bytes *bytes,	/* Size of chunk, function may truncate it. */
	int free_pde_idx,	/* index of the free slot to use */
	int *changed		/* If mapping is made, this is set to 1. */
	)
{
	u32_t pdeval;
	int pde;
	...
	pde = freepdes[free_pde_idx];
	...
	/* Write the pde value that we need into a pde that the kernel
	 * can access, into the currently loaded page table so it becomes
	 * visible.
	 */
	assert(get_cpulocal_var(ptproc)->p_seg.p_cr3_v);
	if(get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde] != pdeval) {
		get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde] = pdeval;
		*changed = 1;
	}
	...
}
```

链路是：内核要访问"另一个地址空间里的地址"（例如跨进程拷贝），而它只有一个当前生效的页目录；办法是在**当前页表**里临时占用一个空闲槽位，把对方那个地址对应的页目录项写进去，于是对方的一个 4MB 窗口在当前地址空间里可见；用完由调用方在下次切换时自然丢弃。`virtual_copy_f` 经 `lin_lin_copy` 在这里两个方向各借一个槽位（`free_pde_idx` 为 0 与 1），写完调 `reload_cr3()` 刷新 TLB。另一个消费者在 VM 服务里（[`vm/pagetable.c`](minix3/minix/servers/vm/pagetable.c) 里对 `kernel_boot_info.freepde_start` 的取用），用途相近但对象不同：VM 用这些槽位映射各进程的页目录，而不是跨空间拷贝的临时窗口。

这就把 `freepde_start` 从"一个返回值"变成了"一套借用机制"——注意它真正被使用是在 `kmain` 之后（同属下游延伸）：它成立需要三个前提——当前页表里确实有连续的未用槽位、借来的窗口只在当前 `CR3` 下有效、借出者自己记账并保证在切换前不再引用该窗口（单次借用不需要调用方自己清零；但 VM 改动某进程页表时，内核侧缓存的映射由 `VMCTL_CLEARMAPCACHE` → `mem_clear_mapcache()` 显式清零，`memory.c:35`、`do_vmctl.c:164`）。三者在 64 位侧的失效方式见 §3.9。

**若 `pg_mapkernel` 返回的是"最后一个已用槽位"而不是"第一个空闲槽位"会怎样**：`memory_init` 的两行自增会把 `freepdes[0]` 设成内核映射占用的最后一个页目录下标（961 + k - 1，`k` 即内核占用的 4MB 块数 = ceil(`kern_size`/4MB)），`freepdes[1]` 才落到真正的第一个空闲槽。`createpde` 对借来的槽只做两条范围检查（`free_pde_idx` 在 `nfreepdes` 之内、槽号在 1024 之内），不检查它"是否空闲"；而跨空间拷贝的第一个方向固定借 0 号槽（`createpde(srcproc, srclinaddr, &chunk, 0, &changed)` 在 `i386/memory.c` 的 [`lin_lin_copy`](minix3/minix/kernel/arch/i386/memory.c:lin_lin_copy) 里，入口是 `i386/memory.c` 的 [`virtual_copy_f`](minix3/minix/kernel/arch/i386/memory.c:virtual_copy_f)），`vm_memset` 也借 0 号槽（`i386/memory.c` 的 [`vm_memset`](minix3/minix/kernel/arch/i386/memory.c:vm_memset)）。

于是第一次跨地址空间拷贝会把对方地址空间的 PDE 值写进 `p_cr3_v[961 + k - 1]`。这里要说清 `p_cr3_v` 是哪张表：`i386/protect.c` 的 [`arch_post_init`](minix3/minix/kernel/arch/i386/protect.c:arch_post_init) 用 `i386/pg_utils.c` 的 [`pg_info`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_info) 把引导期那张 `pagedir[]` 的物理地址与虚拟指针交给 VM 进程当作它的页表，`lin_lin_copy` 又断言当前 `CR3` 就是它（`i386/memory.c` 的 [`lin_lin_copy`](minix3/minix/kernel/arch/i386/memory.c:lin_lin_copy) 里 `assert(read_cr3() == get_cpulocal_var(ptproc)->p_seg.p_cr3)`）——所以这一改写动的正是**内核此刻正在用的那张表**，不是某个进程的私有页表。

而 `961 + k - 1` 那一条正是内核镜像最后一块 4MB 的映射；`reload_cr3()` 让改写立刻生效，内核在那个 4MB 窗口里的下一次访问就撞上它：对方该槽的条目若不存在（值为 0 或非 present），是一次内核态页错误；若存在，则是静默读到另一个进程的 4MB 内容（内核数据被换成了别人的字节，没有任何报错）。两条路径都经系统任务进入：跨空间拷贝走 `SYS_VIRCOPY`/`SYS_PHYSCOPY` 与 `SYS_SAFECOPYFROM`/`SYS_SAFECOPYTO`（[`system/do_copy.c`](minix3/minix/kernel/system/do_copy.c)、[`system/do_safecopy.c`](minix3/minix/kernel/system/do_safecopy.c)），清零走 `SYS_MEMSET` 与 `SYS_SAFEMEMSET`（[`system/do_memset.c`](minix3/minix/kernel/system/do_memset.c)、[`system/do_safememset.c`](minix3/minix/kernel/system/do_safememset.c)）——发起方可以是任何服务进程，不限于装载启动镜像那一步。所以最早坏在 `memory_init` 之后第一次跨空间拷贝或清零上。

#### 2.6.4 自举页表的下游：VM 接手时的继承规则

先说清一个容易漏掉的中间步骤：`pre_init` 建完这张 `pagedir[]` 之后，`kmain → cstart → prot_init` 会用同样四步（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`，`minix3/minix/kernel/arch/i386/protect.c:360-363`）把它**整体重建一次**；`vm_enable_paging` 不在这里调用，分页此时已经开着，源码注释写的是"Set up a new post-relocate bootstrap pagetable so that we can map in VM, and we no longer rely on pre-relocated data."。重建后的内容与 `pre_init` 那张等价，`freepde_start` 也不变（`pg_mapkernel` 的返回值在 `prot_init` 里被丢弃，槽号不变所以无害）。这里要补一个容易被 §3.7 的反推掩盖的区分：C 确实在分页已开时清活动根，它能撑过窗口靠的是"清表与回填之间只执行已被 4MB TLB 项覆盖的代码/栈，且 `pg_load` 在回填之后才重载 CR3"——这是时序脆弱，不是"无害"；§3.7 禁止的是在运行期**没有回填窗口**地重建活动根。重建末尾的 `pg_load` 会重载 CR3、冲刷 TLB，所以"新表生效"不靠旧表项自然失效。下面说的"VM 继承的那张表"指这张重建后的同一张 `pagedir[]`。

这张自举页表有**两个**下游。第一个是上面讲完的 `freepde_start` 借用窗口——内核自己还要用这张表的高半槽位；**第二个是本节的对象：页目录被交给 VM 的进程表项**——它对表里内容是三条去向：4KB 细粒度条目被继承；大页条目一律不继承——内核映射大页由 VM 自己重建，引导期恒等大页直接丢弃（三条去向在本小节末段交代）。这一步同样越过 `pre_init()` 返回之后（下游延伸）——放在这里，是为了把这张表的最终归属一次交代清楚。这个交接口（`pg_info` 的两个输出参数、`arch_post_init` 的调用点）在 §2.6 的反向推演里已经见过，下面只看它写下的代码：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:arch_post_init。 */
void arch_post_init(void)
{
  /* Let memory mapping code know what's going on at bootstrap time */
  struct proc *vm;
  vm = proc_addr(VM_PROC_NR);
  get_cpulocal_var(ptproc) = vm;
  pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);
}
```

VM 接手时**显式跳过所有 BIGPAGE 条目**：

```c
/* 代码引用，minix3/minix/servers/vm/pagetable.c:pt_init 的复制循环。 */
		/* BIGPAGEs are kernel mapping (do ourselves) or boot
		 * identity mapping (don't want).
		 */
		if(!(entry & ARCH_VM_PDE_PRESENT)) continue;
		if((entry & ARCH_VM_BIGPAGE)) continue;
```

两句注释把两件事分开：内核映射由 VM 自己重建；引导期的恒等映射是"不要的"。也就是说，所有大页条目都不继承：内核映射由 VM 重建、恒等映射直接丢弃；只有 4KB 细粒度条目被继承下来。VM 侧页表的完整结构见 `rewrite-notes/02-stage-vm/07-pagetable-struct.md`。

**§2.6 结算**（2.6.1–2.6.4 收束）：五步走完，机器从"只有一个物理地址空间"变成"页表已就位、内核随时可以站到 0xF04xxxxx 上执行"——恒等映射铺满的 1024 项里，0..960 保证开启分页后的下一条取指不会跑飞；被内核映射覆写的 961 起那几项保证跳过去之后代码与栈都可用，`freepde_start` 则把"还能借哪些槽位"留给后面的跨空间拷贝（借用机制的展开见上一小节（§2.6.3）；64 位侧为什么不再需要它，见 §3.9）。至于换栈与那次跳转本身，归属见 §2.2 末。

#### 2.6.5 本篇用到的位、掩码与常量（参考表）

下面三张表（位/常量、地址宏、CR0/CR4 位）把本篇出现的位与掩码集中在一处，值全部取自 [`include/vm.h`](minix3/minix/include/arch/i386/include/vm.h)。

| 常量 | 值 | 含义 | 本篇哪一步用到 |
|---|---|---|---|
| `I386_VM_PRESENT` | `0x001` | 条目存在 | `pg_identity`、`pg_mapkernel`、`pg_map` 写入的每个条目 |
| `I386_VM_WRITE` | `0x002` | 可写 | 同上 |
| `I386_VM_READ` | `0x000` | 只读（不置 `WRITE` 即只读） | — |
| `I386_VM_USER` | `0x004` | 用户态可访问 | `pg_identity`、`pg_map`（`pg_mapkernel` 不置，理由见 §2.6 前半 `pg_identity` 的解码条目） |
| `I386_VM_PWT` | `0x008` | 写穿透 | `pg_identity` 对非 DRAM 区间 |
| `I386_VM_PCD` | `0x010` | 禁缓存 | 同上 |
| `I386_VM_ACC` | `0x020` | 已访问（由 CPU 置位） | 硬件写，本路径不置 |
| `I386_VM_DIRTY` | `1 << 6` | 已写（由 CPU 置位） | 同上 |
| `I386_VM_BIGPAGE`（同值写法 `I386_VM_PS`） | `0x080` | 本条目直接指向 4MB 物理块 | `pg_identity`、`pg_mapkernel` |
| `I386_VM_GLOBAL` | `1 << 8` | 全局页（换 `CR3` 后留在 TLB） | 本路径不置 |
| `I386_VM_ADDR_MASK` | `0xFFFFF000` | 取 4KB 对齐的物理地址 | `pg_map` 挂页表项 |
| `I386_VM_ADDR_MASK_4MB` | `0xFFC00000` | 取 4MB 对齐的物理地址 | `pg_identity` 的区间比较 |
| `I386_VM_OFFSET_MASK_4MB` | `0x003FFFFF` | 4MB 块内的偏移 | 4MB 窗口内寻址 |
| `I386_VM_PT_ENT_MASK` | `0x3FF` | 页表索引掩码 | `I386_VM_PTE` |
| `I386_VM_DIR_ENTRIES` / `I386_VM_PT_ENTRIES` | `1024` | 页目录/页表条目数 | `pg_identity` 的循环上界、页目录数组长度 |
| `I386_PAGE_SIZE` | `4096` | 页大小 | 全部对齐与取整运算 |
| `I386_BIG_PAGE_SIZE` | `4096 × 1024` = 4MB | 大页大小 | `pg_identity`/`pg_mapkernel` 的步进 |

三条地址换算宏（同文件）：

| 宏 | 展开 | 含义 |
|---|---|---|
| `I386_VM_PDE(v)` | `v >> 22` | 虚拟地址 → 页目录下标（高 10 位） |
| `I386_VM_PTE(v)` | `(v >> 12) & 0x3FF` | 虚拟地址 → 页表下标（中 10 位） |
| `I386_VM_PFA(e)` | `e & 0xFFFFF000` | 页目录/页表项 → 物理地址（低 12 位清零） |

CR0/CR4 的位（同文件）：

| 常量 | 值 | 位 | 含义 | 谁动它 |
|---|---|---|---|---|
| `I386_CR0_PE` | `0x00000001` | bit 0 | 保护模式 | 引导器置；`vm_enable_paging` 只断言它已置 |
| `I386_CR0_WP` | `0x00010000` | bit 16 | 写保护（内核态也不能写只读页） | `vm_enable_paging` |
| `I386_CR0_PG` | `0x80000000` | bit 31 | 开启分页 | `vm_enable_paging` |
| `I386_CR4_PSE` | `0x00000010` | bit 4 | 大页扩展（4MB 页的前提） | `vm_enable_paging` |
| `I386_CR4_PGE` | `0x00000080` | bit 7 | 全局页（按 CPU 能力置） | `vm_enable_paging` |

CR0/CR4 表里有一处注释与位号不符，按值与用法判断：`vm.h` 里 `I386_CR0_WP` 那行的注释写的是 `/* Enable paging */`，与下一行 `I386_CR0_PG` 的注释逐字相同——注释在这一行是从相邻行复制来的，`WP` 的含义要看 `vm_enable_paging` 里它被置的时机（开 `PG` 之后单独置一次）。

### 2.7 同一个函数名在 ARM32 上：自己造一份 multiboot

Minix3 同时支持 x86 与 ARM 两种架构，同一段引导准备在两侧各有一份实现、用的是同一串函数名。先看两侧的入口（`earm/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/earm/head.S:multiboot_init) 与 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init)）：

```asm
/*
 * 代码引用，minix3/minix/kernel/arch/earm/head.S（i386 侧同一段已在 §2.2 给出，
 * 这里不再重复）。
 */
.section .startup
.global MINIX
MINIX:
	b multiboot_init

.text
multiboot_init:
	ldr	sp, =load_stack_start	/* make usable stack */
	mov	fp, #0
	bl	_C_LABEL(pre_init)

	/* ……（此处原文有 6 行注释：内核此时已映射到高地址，pre_init 返回的
	 * boot info 指针仍在 r0；接着换高地址栈并调用 kmain）…… */
	ldr	sp, =k_initial_stktop
	mov	r1, #0
	push	{r1}			/* Terminate stack */
	ldr	r2, =_C_LABEL(kmain)	/* r0 holds kinfo_t ptr */
	bx	r2
```

两侧的相同与不同都很直接：入口标签同名（`MINIX`），第一跳标签同名（`multiboot_init`），临时栈的放置位置与写法一致（`load_stack` 在文件末尾的 `.data` 段，取的是"高端"符号），清帧指针一致（`%ebp` / `fp`），调用 `pre_init` 与随后调用 `kmain` 的一致。不同的是指令集本身，以及参数传递：x86 用栈传两个参数（魔术值 + `multiboot_info` 地址），ARM 用寄存器传参，而 `pre_init` 返回的 `kinfo` 指针留在 `r0` 里一路带到 `kmain`。

真正体现"引导协议不属于内核"的地方在 `pre_init` 内部（ARM 侧是 `earm/pre_init.c` 的 [`setup_mbi`](minix3/minix/kernel/arch/earm/pre_init.c:setup_mbi)，两侧的 `get_parameters` 分别在 `earm/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/earm/pre_init.c:get_parameters) 与 `i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters)）。ARM32 的 `get_parameters` 第一件事不是读引导器结构体，而是**造一个**：

```c
/*
 * 代码引用，minix3/minix/kernel/arch/earm/pre_init.c:setup_mbi。
 */
void setup_mbi(multiboot_info_t *mbi, char *bootargs)
{
	memset(mbi, 0, sizeof(*mbi));
	mbi->flags = MULTIBOOT_INFO_MODS | MULTIBOOT_INFO_MEM_MAP |
			MULTIBOOT_INFO_CMDLINE;
	mbi->mi_mods_count = MB_MODS_NR;
	mbi->mods_addr = (u32_t)&mb_modlist;

	int i;
	for (i = 0; i < MB_MODS_NR; ++i) {
		mb_modlist[i].mod_start = MB_MODS_BASE + i * MB_MODS_ALIGN;
		mb_modlist[i].mod_end = mb_modlist[i].mod_start + MB_MODS_ALIGN
		    - ARM_PAGE_SIZE;
		mb_modlist[i].cmdline = 0;
	}

	/* morph the bootargs into multiboot */
	mbi->cmdline = (u32_t) bootargs;

	mbi->mmap_addr =(u32_t)&mb_memmap;
	mbi->mmap_length = sizeof(mb_memmap);

	mb_memmap.size = sizeof(multiboot_memory_map_t);
	mb_memmap.mm_base_addr = MB_MMAP_START;
	mb_memmap.mm_length  = MB_MMAP_SIZE;
	mb_memmap.type = MULTIBOOT_MEMORY_AVAILABLE;
}
```

逐条解码：

- `memset` 之后自己置 `flags` 的三位：有模块、有内存表、有命令行。x86 上这三位是引导器置的，ARM 上是内核自己置的。
- 模块列表是**编译期常量**：`MB_MODS_BASE + i * MB_MODS_ALIGN`。这意味着 ARM 这一侧假设了引导器（U-Boot 一类）已经把模块放在这些固定地址上，`mod_start`/`mod_end` 只是把这个约定抄进结构体。
- 内存表只有一条：`mb_memmap` 一个条目，`mm_base_addr = MB_MMAP_START`、`mm_length = MB_MMAP_SIZE`。x86 上这张表可能有十几条（BIOS 报告的内存空洞），ARM 上是一条硬编码的连续区间。
- `mbi->cmdline = (u32_t) bootargs;`：命令行来自引导器通过寄存器传进来的参数（ARM 调用约定下前两个参数在 `r0`/`r1`，`bl pre_init` 把它们原样交给 `argc`/`argv`），注释写作"morph the bootargs into multiboot"。这句话是整个函数的性质说明——它把"另一种引导器给的参数形式"翻译成 Multiboot 的形状。
- 于是 `get_parameters()` 里其余部分（模块表 `memcpy`、`add_memmap` 遍历、`overlaps`、`cut_memmap`、`mb_set_param`）在两种架构上同构（注意 ARM 侧头文件用的是另一套字段名），调用次序一致；但**区间约定不同**。ARM 的 `overlaps` 原文（[`overlaps`](minix3/minix/kernel/arch/earm/pre_init.c:overlaps)）：

```c
#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) <= thismod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
	    INRANGE(mod1, mod2->mod_end))
	for(m = 0; m < n; m++) {
		multiboot_module_t *thismod = &mod[m];
		if(m == cmp_mod) continue;
		if(OVERLAP(thismod, cmp)) {
			return 1;
		}
	}
```

两处与 x86 版不同：**其一**，`INRANGE` 的上界写成 `<= thismod->mod_end`。`mod` 是宏形参，`thismod` 不是——它是外层循环里的局部变量，宏在展开点捕获它，所以这个上界取的是"当前外层条目"的结束地址；`OVERLAP` 总以 `thismod` 作第一实参，这个读法才成立。**其二**，`OVERLAP` 第二项直接取 `mod2->mod_end`，没有 x86 版的 `- 1`。合起来是闭区间判定，正是 §2.5 论证为错误的那一版——不过 ARM 的槽距是 8MB、每个 `mod_end` 又各留了一页（`earm/pre_init.c` 的 [`MB_MODS_ALIGN`](minix3/minix/kernel/arch/earm/pre_init.c:MB_MODS_ALIGN) = `0x00800000`，减去 `ARM_PAGE_SIZE`），相切不会出现，所以这一版在它的固定槽位布局下够用；换成会相切的布局就会误报。这里要读准结论：ARM 的闭区间约定在该布局下**无害**，不是"ARM 也有一处待修缺陷"。ARM 这一侧甚至保留了 `multiboot` 这个词。

撇开两种架构的硬件差别，引导准备里与硬件无关的部分是四步：

| 步 | 语义 | x86-32 落点 | ARM32 落点 | 谁定的 |
|---|---|---|---|---|
| 1 | 接收引导参数 | `head.S` 压栈 + `pre_init(magic, ebx)` | `head.S` 只 `bl pre_init`（两侧形参不同：ARM 是 `pre_init(argc, argv)`，没有 `magic` 也没有 `ebx`），再由 `setup_mbi` 自造结构 | 引导协议（各架构不同） |
| 2 | 建立地址映射 | [`pg_clear`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_clear) + `:pg_identity` + `:pg_mapkernel` | 同名函数（[`pg_identity`](minix3/minix/kernel/arch/earm/pg_utils.c:pg_identity)），1MB Section 代替 4MB 大页 | 分页硬件（各架构不同，语义相同） |
| 3 | 开启分页/MMU | [`pg_load`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_load) + `:vm_enable_paging`（写 `CR3`，置 `CR0.PG`） | 同名函数（[`pg_load`](minix3/minix/kernel/arch/earm/pg_utils.c:pg_load) 写 `TTBR0`；[`vm_enable_paging`](minix3/minix/kernel/arch/earm/pg_utils.c:vm_enable_paging) 置 `SCTLR.M`（ARM 的 MMU 使能位）并使能 I/D cache 与分支预测） | MMU 寄存器（各架构不同） |
| 4 | 把启动信息交下去 | `return &kinfo` → `kmain(&kinfo)` | `r0` 仍持有指针 → `kmain` | 调用约定（x86 用栈、ARM 用寄存器） |

ARM 侧这两步比 x86 多几个前置：`vm_enable_paging` 先写 `TTBCR`（页表基址控制）与 `DACR`（把 16 个域全设为 Client），再读改写 `SCTLR`；开 MMU 之前还有一次 `dcache_clean()`，调用点在 `earm/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/earm/pre_init.c:pre_init)。

步骤 2、3 在各架构都调用同名函数，是因为"清零页目录 → 建恒等映射 → 把内核映射到高地址 → 装载页表基址 → 开启分页"这一串的**语义**与"用哪条指令、页表项多少位"无关；第 1 步的**入口与参数来源**是唯一必须按架构另写的部分，因为"参数从哪来"由引导器决定；步骤 2、3 同名同序，但函数体仍按架构实现（页尺寸与 MMU 寄存器不同）。第 4 步的差别只在调用约定。

### 2.8 本章归纳

把 2.1 到 2.7 的调用关系收成一张图：

```
引导器（boot2 / GRUB / U-Boot / 固件）
  │  1. 识别头（GRUB 类）/ ELF 头（boot2）+ 寄存器约定 + 填好的结构体
  ▼
head.S: multiboot_init
  │  设临时栈、清帧指针、按调用约定传参
  ▼
pre_init(magic, ebx)
  ├── get_parameters(ebx, &kinfo)
  │     ├── memcpy(&kinfo.mbi, ebx)
  │     ├── 链接期常量 → bootstrap_start / bootstrap_len / vir_kern_start
  │     ├── mbi.mi_cmdline → 逐词扫描 → mb_set_param → kinfo.param_buf
  │     ├── mbi.mi_mods_* → kinfo.module_list[]
  │     ├── mbi.mmap_* → add_memmap() × R          （构建可用内存记录；R = 可用（AVAILABLE）条目数）
  │     │   └─ 取不到细表时退回粗档：mem_lower/mem_upper → add_memmap() × 2（只造两段连续区间）
  │     ├── 内核当第 M+1 个模块登记（M = 模块数）
  │     └── 对每条记录 m：overlaps(m) → panic 或 cut_memmap(m)
  │        （检查与切除逐条交错，共 M+1 轮——两行合成一行以贴近 §2.5 的实际次序）
  ├── pg_clear()                （页目录清零）
  ├── pg_identity(&kinfo)       （低地址恒等映射，覆盖当前执行点）
  ├── freepde_start = pg_mapkernel()（内核高地址映射，返回第一个空闲槽位号）
  ├── pg_load()                 （CR3 = 页目录物理地址）
  └── vm_enable_paging()        （CR0.PG 置位，PSE/WP/PGE 按序处理）
  ▼
return &kinfo
```

四样之间的次序约束共五条（D1–D5）（与 §1.2 的 D1–D5 一一对应：清单先于建表、内核位置先于清单裁剪、页表先于开启分页、结构先于 `kmain`、建表内部恒等先于内核映射；另有 §1.2 单列的那条字段级回填，不算次序约束）。五条的证据都在本章，按证据所在小节列：下面三条给论证（清单先于建表、恒等先于内核映射、结构先于 `kmain`）；另两条只给指向——内核位置先于清单裁剪见 §2.3 与 §2.5，页表先于开启分页见 §2.6.1（逐数字推演在那里，本节不重复）。结构先于 `kmain` 的关键那行 `push %eax` 在 §2.2 引文块之外（本节只给源码位置与"同名不同处"的辨析）。

- **内存清单先于建表**。清单在 `get_parameters()` 里就建完并切完（`add_memmap` × R；随后对 M+1 条记录逐条 `overlaps` → `cut_memmap`，检查与切除交错），`pg_clear()`、`pg_identity()`、`pg_mapkernel()` 都在它之后才跑（依据：`i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 里这几句的排列顺序）。**为什么不能反，按依赖的实际层次说**：`pg_identity()` 要用 `mem_high_phys` 决定哪些条目该标成非缓存——这是硬依赖，且它在函数开头就有一条 `assert(cbi->mem_high_phys)`：反排时该值还是 `.bss` 的零初值，断言当场失败（断言拿掉才是"得不出正确的表"）；`pg_mapkernel()` 的初始大页映射不依赖记录（它用链接期常量与已清零的静态页目录）；从记录里动态取页的是 `pre_init()` 之后的 4KB 细粒度路径（`pg_alloc_page`），它同样要求记录已就位。
- **恒等映射先于内核映射**。两个函数写同一张 `pagedir[]`，重叠下标谁后写谁生效；这次覆盖为什么恰好是需要的、反过来排会坏成什么样，逐数字的推演在 §2.6.1（[`pg_identity`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity) 的循环上界与 [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel) 的起始下标），这里不重复。
- **信息结构先于 `kmain`**。`pre_init` 的返回值是 `&kinfo`，`head.S` 把它压栈作为 `kmain` 的第一个参数（依据：`i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的 `return &kinfo;`，与 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 里 `call kmain` 之前那条把返回值压栈的 `push %eax`——它在 §2.2 引文块之外；引文里那条 `push %eax` 是给 `pre_init` 传参用的，同名不同处）。

ARM32 的主要差异是第 1 步（`setup_mbi`）与 MMU 寄存器；`overlaps` 的闭区间差异见 §2.7，不在“只有”之列。

换栈那一步的落点：`head.S` 在 `call kmain` 之前把栈换到 `k_initial_stktop`，它落在高半一侧。本篇不复制这条判据——它属于"内核怎么走到高半去跑"那个主题，推证与证据表在 `02-higher-half-kernel.md` §2.4（§1.4 已给过那条"名字不是判据"的提醒，这里不重复）。本篇只需要这一个事实：`pre_init` 返回后换上的那个栈，与紧接着要跳去的高半在同一侧，所以换栈与跳转能成对。

把本章的断言按用途分一下：`assert(cbi->mem_high_phys)`（§2.6.1 已引）这类断言不能证明布局一定合法，它把"不合法"从后置的沉默故障提前成现场的立即停止——断言改不了事实，只改故障出现的位置。

---

## 3. Rust 设计决策

本章与第 2 章逐项对应：3.1–3.2 换掉引导协议与它的两个接口（对应引导器契约），3.3 固定 crate 依赖方向，3.4–3.7 是交接面本身（结构、段语义、页尺寸能力、三种形态），3.8 是页表页分配器的换手，3.9 是页目录槽位账的退役（借槽机制换 Direct Map），3.10 收成一张 C→Rust 对照表。

### 3.1 引导协议换代：Multiboot 与 GRUB 的角色由固件接口替换 `[ARCH: 引导协议从 Multiboot 换成固件接口]`

> `[ARCH: ...]` 是本项目的架构演进标注，表示该处不是逐字重写，而是有意的架构变化；三档术语（Rewrite / Refactor / Architectural Evolution）见 `AGENTS.md`。

C 的实现前提写在 2.1 与 2.2 里（本节标题里的 GRUB 泛指"第三方引导器"这个角色，本仓这条路径上具体是 §2.1 点名的 boot2）：引导器把内核与模块都读进内存，把它们的地址填进一份结构体，再用两个寄存器把结构体地址交给内核的第一段代码。内核自己不做任何 IO——它连"文件"这个概念都不需要。

重写的目标是 x86-64、aarch64、riscv64 三个架构，其中后两个没有 Multiboot 的等价物，而 UEFI 的启动模型只提供一个可以做 IO 的环境（Boot Services）与两个内存分配接口（页分配 `AllocatePages`、池分配 `AllocatePool`），并不替谁加载内核。于是第一步的分工被重新切开：

| 步骤 | C（Multiboot 协议 + MINIX boot2） | Rust（UEFI 路径） | Rust（U-Boot shim 路径；FW_PAYLOAD 直核形态不在本表，见 §3.7） |
|---|---|---|---|
| 识别可执行文件 | ELF 头（本仓 boot2）／前 8KB 的识别头（GRUB 一类 Multiboot 引导器）（2.1） | UEFI 的 PE/COFF 头（标准可执行格式） | ELF 头 + U-Boot `bootelf` 约定 |
| 读内核与模块到内存 | 引导器代劳：`multiboot` / `load_mods` | boot-shim 自己读：`SimpleFileSystem` 协议 + `loader::KERNEL_PATH` / `MODULE_NAMES` | U-Boot 代劳读文件（`fatload`），boot-shim 按 `BootFileTable` 查地址 |
| 内存布局 | 引导器填 `mbi.mmap_*`；内核遍历 | boot-shim 自己取固件内存图，分成可用表与保留表（交付形状见 §3.4，分流规则见 §4.2） | 读 DTB `/memory` 节点的 `reg`，同样分成两张表 |
| 传递参数 | 寄存器 + `multiboot_info_t` | arena 基址（x86_64 `rbx`、aarch64 `x0`、riscv64 U-Boot shim `a0`）——arena 的定义与分区布局见 §3.4；`BootHandoffV1` 已序列化进 v1 arena（`KernelInfo` 的字段按协议分区写入，发送侧不构造进程内 `KernelInfo`），根页表物理地址在信封的运行期字段里 | 同 UEFI 的协议与形状，入口寄存器是 `a0` |
| 退出固件服务 | 无此步骤（引导器早已离开） | 必须调 `ExitBootServices`：boot-shim 跑在 Boot Services 的**环境之内**，得在某个时刻主动声明"我不再需要你"；声明之后整套服务失效，本篇用到三类——读文件（`SimpleFileSystem`）、分配内存（页分配 `allocate_pages` 与池分配 `allocate_pool`，后者支撑 `Vec`/`Box`）、固件控制台输出（`uefi::println!`） | 无此步骤（U-Boot 已把控制权交出去） |

生产入口的同一约束（文件访问必须在 `ExitBootServices` 之前完成）由 §4.2 的九步表承担：读文件在第 2 步、EBS 在第 6 步。


`exit_boot_services`（**测试内核路径的辅助实现，不是生产入口**）自己的文档也把这件事说成契约；生产入口在 §4.2 的配对循环里直接用 raw `ExitBootServices`，内存图分类进两张表，**不丢弃**：

```rust
// 代码引用，os/boot-shim/src/uefi_helpers.rs:fn exit_boot_services。
pub fn exit_boot_services() {
    // SAFETY: caller has finished all uses of boot services; from this
    // point on only runtime services (if any) may be used. The returned
    // memory map is discarded because the kernel already received the
    // CONVENTIONAL regions via `build_memmap()`.
    unsafe {
        let _mmap = boot::exit_boot_services(MemoryType::LOADER_DATA);
    }
}
```

> `LOADER_DATA` 是 UEFI 内存类型标签，意思是"引导程序向固件分配器租下的内存"（loader 是属格——引导程序的所有物，不是"被加载的数据"——这里只给这次取内存图的缓冲区定标签）；（测试内核路径下）返回的 `_mmap` 可以丢弃，因为测试内核要的清单已由上文 `build_memmap()` 返回的 conventional 列表交付，这只是规范要求的"EBS 前最后一次取图"，取完即弃。

被审视的替代路径与结论（三否一采）：

| 方案 | 结论与理由（可复核） |
|---|---|
| 保留 Multiboot，做 64 位扩展（Multiboot2） | 否决：两个新架构没有等价协议；`head.S` 那套"引导器填结构体 + 寄存器传参"在 aarch64/riscv64 上要各写一份，等于三份引导协议 |
| 各架构独立引导（x86 用 Multiboot、ARM 用 PSCI（ARM 的电源/启动服务接口）、RISC-V 用 SBI） | 否决：分叉的是架构名，不是语义——§2.7 的四步里只有第 1 步（接收引导参数）随引导器变，重写把它收进 §3.2 的固件抽象，其余三步共享同一套语义，不随硬件重新分叉 |
| 内核自带 stub（UEFI 形态：把 boot-shim 链接进内核镜像，内核自己读文件再自举） | 否决，理由是**依赖方向**：boot-shim 与内核并成一个二进制，内核就得认识固件类型、把 `uefi` 拉进自己的依赖表，与 §3.3「内核不依赖引导层」的单向边界冲突——屏蔽固件差异正是 boot-shim 的职责。riscv64 没有 Rust 支持的 UEFI 目标，但只挡住 UEFI 这一种实现；换成 opensbi 形态则改判为采纳（下一行） |
| 内核自带 stub（opensbi 形态：固件直接跳下一级镜像，引导面长在内核镜像里） | **采纳**：它就是 §3.7 的直核自引导形态（riscv64 真板 `FW_PAYLOAD`、QEMU `-kernel`；aarch64 `-kernel` 测试通道）。它不违反依赖方向——`kernel-image` 反向依赖的是 boot-shim 的**库函数**（库 crate 与可执行体 crate 的区分见 §3.3；取件表——`BootFileTable` 这个文件索引结构；memmap 源），不是引导器产物（下文三词分工：**取件表** = `BootFileTable`，**取件库** = boot-shim 导出的装载辅助函数，**取件车道** = 一次取件的来源通道：UEFI `SimpleFileSystem` 或 U-Boot 表），`minix-kernel` 与 `kernel-image` 都不依赖 `uefi`（§3.3 问题二）。定位是辅助启动路径，不作三架构主线 |

#### 3.1.1 接线状态（唯一权威表）

> **现状标注（本篇接线状态的唯一权威表；其余各节再提到接线状态，都指向这里）**：上面第一张表描述架构终态；第二张表是替代路径的裁决记录（三否一采）。当前各固件路径的接线状态如下。
>
> | 路径 | 接线状态 | 覆盖与理由 |
> |---|---|---|
> | x86_64 / aarch64 UEFI 产线 | 已接线：「boot-shim 建根 + v1 arena + 跳独立 `kernel.elf` 高半入口」 | 切换依据见 §3.4 的收口记录与 §4.8 的实现现状 |
> | riscv64 U-Boot shim | 已接线：U-Boot 用 `bootelf` 装载 `riscv64-uboot-shim`，shim 建根后跳进 `kernel-uboot` 的高半入口 | 门脚本 `test-riscv64-uboot-shim.sh` 的六个串口路标（直到 `entering scheduler`）全部出现 |
> | riscv64 UEFI shim | **未接线**，原因是工具链事实而非工程欠账：Rust 编译器至今没有提供 riscv64 的 UEFI 目标（`*-unknown-uefi` 目标列表不含 riscv64） | 等编译器支持后再接 |
> | riscv64 直核自引导 | 辅助/嵌入式形态（非主线），riscv64 的 QEMU 门链当前走它 | 它是硬件规范支持的真实形态，理由见下段"例外条款的背景" |
> | aarch64 直核面 | 测试通道，无专用启动门脚本；由 `test-smp-aps-aarch64.sh` 用 `-kernel` 间接驱动 | 判据是 SMP 交付（`ap-arrived`/`ap-irq-ready`），不是启动路标门，见 §5.2.3 |

> **例外条款的背景**：RISC-V 的固件形态规范（OpenSBI 的 `FW_PAYLOAD` 模式）允许把下一级镜像打包进固件本体，boot 时固件直接跳进 S-mode 目标——真板上这类极简装机（省掉 U-Boot 与文件系统）不需要引导器存在，QEMU 的 `-kernel` 模拟的正是这个时序。所以"引导面长在内核镜像里"是硬件规范支持的真实形态。§3.1 分工表的第三列描述的是 U-Boot shim 路径（`opensbi_helpers::prepare_boot`，由 `os/boot-shim/src/uboot_main.rs:fn uboot_main` 调用）；各路径的接线状态与"未接线的 riscv64 UEFI-shim 路径"指哪一条，见上面的现状标注表，这里不重复。

### 3.2 引导准备入口：一条普通函数，差异收在 `FileLoader`

本节起反复用"腿"：**一条腿 = 一个完整启动路径（一个引导面 + 它交付的内核镜像）**；腿、链、形态、交接路径的计数口径不同，对照表在 §3.7。

C 里"同一串函数名、两套实现"的机制见 §2.7：两种架构各自写 `head.S` 与 `pre_init.c`，靠命名与语义约定保持一致，编译期不会检查它们是否真的对应。

Rust 侧最初把固件相关的部分收进一个只有**一个方法**的 trait（`minix-boot::BootShim`）。R23 审计后把它删了，理由是可核的三条事实：唯一实现是 U-Boot 腿的 `RiscvUbootBootShim`（删之前 `grep -rn "impl BootShim" os/` 只有一处）；没有 `dyn BootShim`，`prepare_boot` 还是关联函数，不参与对象分派；测试接缝实际由 `FileLoader` 提供，不是它。单实现 trait 既不是接缝，也不是已经验证过的扩展点，留着只会让"换固件"的承诺看起来比证据强。

现在 U-Boot 腿的入口是一条普通函数：[`fn prepare_boot`](<os/boot-shim/src/opensbi_helpers.rs:fn prepare_boot>)，由 [`fn uboot_main`](<os/boot-shim/src/uboot_main.rs:fn uboot_main>) 直接调用，签名是 `prepare_boot(&mut BootStorage, bump_pages) -> BootPrepareResult<'a>`。UEFI 生产入口 `prepare_and_enter_kernel_image` 仍是独立入口（§3.7）；两个直核面复用 opensbi_helpers 的库函数，同样不经过它。未来若真出现第二条固件腿，等第二个实现落地时再按两个真实实现提炼接口，而不是先猜一个。

```rust
// 节选（`report` 字段后的中文旁注为编者加注），os/libs/minix-boot/src/boot_shim.rs:struct BootPrepareResult。
pub struct BootPrepareResult<'a> {
    pub report: BootReport<'a>, // 借用调用方 storage 的语义视图
    pub root_page: PhysBytes,
    /// Bump region `[start, end)` (physical).
    pub bump: PhysRange,
    /// Handoff arena base (physical), allocated from the same bump pool after
    /// the module landings.
    pub arena_pa: PhysBytes,
}
```

`bump` 是只前进不回收的线性分配器；`bump_pages` 是给页表页预留的页数，`bump` 区间的 `start`/`end` 是该区间的物理边界（展开见 §3.3 与 §3.8）。

差异被压到一个更小的接口上（`loader.rs` 的 [`trait FileLoader`](os/boot-shim/src/loader.rs)）：各条路径都要拿到"文件字节"，唯一不同的是字节从哪来。

```rust
// 代码引用（trait 级文档注释整段略去，块内不保留省略号），os/boot-shim/src/loader.rs:trait FileLoader。
pub trait FileLoader {
    /// Read a file by path. Returns `None` if the file does not exist or
    /// cannot be read.
    fn read<'a>(&'a self, path: &str) -> Option<FileBytes<'a>>;

    /// Read a file that must exist. Default panics with a useful message.
    fn read_required<'a>(&'a self, path: &str) -> FileBytes<'a> {
        match self.read(path) {
            Some(d) => d,
            None => panic!("boot-shim: required file not found: {}", path),
        }
    }
}
```

`read_required` 是**内核 ELF 取件**的致命落点：它把"文件不存在"从 `None` 变成一个带路径的 panic。启动模块走 §4.3 的 `read + unwrap_or_else(panic!)`，是第二个致命落点，错误消息格式与 `read_required` 不同；两者都是 fail-closed，`read` 本身只返回 `Option`，不决定致命性。两个实现各自只提供 `read`：UEFI 走 `SimpleFileSystem`（并把正斜杠转成 UEFI 要求的分隔符），U-Boot 走预先落在 RAM 里的 `BootFileTable`。除此之外的全部装载逻辑（解析、段拷贝、模块页分配、结构体构造）只有一份。两个实现都在 `read` 里只处理字节来源，错误折成 `Option` 交给各自的调用方。

```rust
// 代码引用（保留读取路径；注释有删节；一处英文注释译为中文），os/boot-shim/src/uefi_helpers.rs:impl FileLoader for UefiFileLoader。
impl FileLoader for UefiFileLoader {
    fn read<'a>(&'a self, path: &str) -> Option<FileBytes<'a>> {
        let fs_proto = boot::get_image_file_system(boot::image_handle()).ok()?;
        let mut fs = FileSystem::new(fs_proto);
        let uefi_path = path.replace('/', "\\");
        let cs = uefi::CString16::try_from(uefi_path.as_str()).ok()?;
        // 固件协议返回新缓冲：这条腿拥有它。
        fs.read(cs.as_ref()).ok().map(FileBytes::Owned)
    }
}
```

U-Boot 侧的实现见 §4.3 的 `entry_path_eq` 与 `UbootFileLoader`；两条 `read` 的差别在字节来源，U-Boot 侧还会按最后一个 PT_LOAD 的文件尾修剪槽容量，`read_required` 与全部装载逻辑完全共用。

去掉 trait 后，实际成立、且不依赖 trait 的收益有这几条：

- **入口形状固定**（只在 U-Boot 生产腿成立）：`prepare_boot` 的 `storage`/`bump_pages` 语义与 `BootPrepareResult<'a>` 的返回形状由签名钉住；但"统一"的对象只有这一条腿，UEFI 与直核面各有自己的入口，不能把它说成跨固件统一。
- **可测试性来自 `FileLoader`**（代码证据，测试形态成立）：`FileLoader` 是 trait——[`loader.rs`](os/boot-shim/src/loader.rs) 的 tests 里有内存表实现 `MockLoader`（`impl FileLoader for MockLoader`），涉及装载的单元测试都经由它喂文件字节。
- **条件编译的落点收敛**（生产中成立）：feature 相关的 `cfg` 只出现在 [`lib.rs`](os/boot-shim/src/lib.rs) 的模块与再导出声明处（7 处 `#[cfg(feature ...)]` 属性：6 条 `uefi` 加 1 条 `opensbi`），业务逻辑里没有条件编译。
- **换固件不动内核**（设计承诺，观察，尚无自动化反例）：新增一种固件形态，改动是 boot-shim 侧的新入口或共享库调用，不需要预先实现一个 trait；`kernel.elf` 不必重新构建（两者分属 `os/kernel-image` 与 `os/boot-shim` 两个产物，这条分界的后果在 §3.7 与 §4.8）。边界要说准：boot-shim 屏蔽的是**固件差异**，板级差异仍落到内核侧的平台发现（[`minix-platform`](os/libs/minix-platform/src/) 的 `PlatformDesc` 与内核侧平台源解析），不能把它说成"新板子只需写 boot-shim"。

这里故意不列"类型安全"与"封装"——这两条常被记在 trait 头上，实际来自 feature 门控：`uefi_helpers` 与 `opensbi_helpers` 各由 [`lib.rs`](os/boot-shim/src/lib.rs) 的 `#[cfg(feature = ...)]` 门控，开启哪个 feature 就只有哪个固件类型存在，**写错固件名本来就编译不过**。UEFI 生产入口也没有借 trait 做抽象：[`fn main`](<os/boot-shim/src/main.rs:fn main>) 按具体类型写——`prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)`。这不是把固件写死在产品里：boot-shim 这个可执行文件只在 UEFI 形态下构建（bin 的 `required-features = ["fw-uefi-image"]` 隐含 `uefi`），固件选择发生在"构建哪个二进制"，不是运行期分派；opensbi 形态的生产路径是 `kernel-image` 自引导面（§3.7），复用 opensbi_helpers 的库函数、不经这份 main.rs。

**OpenSBI 路径的两种角色要分清**（接线状态与覆盖脚本的权威表在 §3.1.1 接线状态表，这里只讲与引导入口有关的部分）：U-Boot 路径复用 boot-shim 库根的 `BootStorage`/`BootReport` 与 opensbi_helpers 的 `BootFileTable`、`bump_alloc_pages`，并调用 `prepare_boot`；`kernel-image` 的直核自举面是另一条机器状态，不经它。memmap 源统一下沉的裁决不合并引导形态层——独立 shim 与 bootface 自举对应不同机器状态，`prepare_boot` 只承担前者的固件差异。

### 3.3 crate 依赖方向：`minix-kernel` 不依赖 boot-shim 可执行体，共享类型住 `minix-boot`

C 的引导链同样不止一个二进制：BIOS 场景有 bootblock 与 boot2 两级引导器（[`boot2.c`](minix3/sys/arch/i386/stand/boot/boot2.c)），GRUB 场景由 multiboot 引导器代劳——引导器与内核不是链接成一个二进制，而是各自链接出独立的可执行文件，运行期只隔一条 multiboot 协议（寄存器递 `mbi` 指针）。内核侧的引导准备 [`head.S`](minix3/minix/kernel/arch/i386/head.S) 与 [`pre_init.c`](minix3/minix/kernel/arch/i386/pre_init.c) 则编在内核镜像之内，`pre_init` 填的 `kinfo` 本来就是内核自己的全局变量。所以 C 无需回答"引导层与内核谁依赖谁"：引导器与内核的边界由协议承载，协议两侧没有编译期关系。

Rust 侧的问题形态不同：boot-shim 独立成产物，却要与内核共享类型定义（`KernelInfo` 与交接的返回形状），这份复用由 crate 依赖表达——"谁依赖谁"第一次成为必须显式回答的构建问题，且答案必须单向：

```
boot-shim ──┬─→ minix-boot   （BootPrepareResult、BootStorage/BootReport、KernelInfo）
            ├─→ minix-arch   （pt_alloc 注册接口、Paging trait）
            └─→ minix-kernel （arch_boot / enter_kernel_image_v1）
minix-kernel ─┬─→ minix-arch
              ├─→ minix-boot
              └─→ minix-plat、minix-platform、minix-types

minix-boot ──→ minix-types

kernel-image ─┬─→ minix-kernel
              ├─→ minix-boot
              ├─→ minix-arch
              └─→ minix-plat
```

（图只画主边；条件依赖与 leaf crates 以下方四份依赖表为准，其中 `kernel-image ┄→ boot-shim` 仅 riscv64/aarch64 target 生效。两个名字看起来相近，但指的不是同一个东西：**boot-shim** 是可执行体 crate，**minix-boot** 是共享库 crate——标题里"内核不依赖 boot-shim"说的是不依赖前者这个*可执行体*；图中 `minix-kernel → minix-boot` 是对*库*的依赖，二者不矛盾。）

依据是四份依赖表（boot-shim / kernel / minix-boot / kernel-image 四个 manifest；命令：`sed -n '/^\[dependencies\]/,/^\[/p' os/kernel/Cargo.toml`（输出中无 `boot-shim` 也无 `uefi`，即证否定；全仓四 manifest 的依赖表见下方四行（`os/kernel` 的 `bitflags` 从略））。文中另提到的 `minix-elf` 是 boot-shim 的普通依赖，不单列成表）：

- `os/boot-shim/Cargo.toml` 的 `[dependencies]` 里有 `minix-types`、`minix-boot`、`minix-arch`、`minix-kernel`、`minix-plat`；`minix-elf`（共享的 ELF 解析器）；非可选的 `fdt`（U-Boot 腿解 DTB）；`uefi = "0.33"` 与 `uefi-raw = "0.9"` 挂在可选的 `dep:uefi`/`dep:uefi-raw` 上，由 `uefi` feature 一并打开。
- `os/kernel/Cargo.toml` 的依赖是 `minix-arch`、`minix-plat`、`minix-platform`、`minix-types`、`minix-boot`——**没有 boot-shim，也没有 uefi**。两个名字相近的 crate 分工不同：`minix-plat` 是板级/机器相关的实现层（早期控制台、中断控制器一类与具体硬件打交道的代码，[`early_console.rs`](os/plat/src/early_console.rs)），`minix-platform` 是平台描述符的解析与分发层（`os/libs/minix-platform/`，面向 ACPI/DTB 这类固件描述）。
- `os/libs/minix-boot/Cargo.toml` 只依赖 `minix-types` 与 `fdt`。
- `os/kernel-image/Cargo.toml` 依赖 `minix-kernel`、`minix-boot`、`minix-arch`、`minix-plat`、`minix-types`。Cargo 的 `[target.'cfg(target_arch = "…")'.dependencies]` 语法允许一条依赖只在指定 target 架构下生效，kernel-image 用它给 riscv64 与 aarch64 各加了两条：`boot-shim`（只开 `opensbi` feature，`uefi` 不开）与 `fdt`（解 DTB 用）——只有以这两个架构为 target 构建时它们才生效。
  - 符合本项目设计目标的方式（独立引导器 + 内核本体两个产物）里，"内核不依赖 boot-shim"对 `minix-kernel` 与 `boot-shim` 成立——引导器依赖内核、内核不回头依赖引导器。这一分工的接线状态见 §3.1.1 的接线状态表（两条 UEFI 产线已跳独立镜像；riscv64 的 QEMU 门链当前走直核自引导这一辅助形态，独立 shim 主线走 U-Boot 路径——主产线归属以该表为准）。
  - `kernel-image` 的情况不同：它是自引导镜像（引导面 `bootface` 长在自己体内，§3.1 例外条款），riscv64/aarch64 目标下按上面那两条 target 表反向依赖 boot-shim 的**库**（取件表、memmap 源等函数）——依赖的是库而不是引导器产物，单向边界不因此破例（道理见问题二）。这张 target 表就是终态：memmap 源下沉到 `minix-boot` 之后，反向依赖的仍是库、不是引导器产物，依赖方向没有变化。

这条单向边界回答的是"共享类型住哪"，与"哪种固件能编译"是两个独立的问题，别混在一起：

**问题一：共享类型为什么住 `minix-boot`**。真正**跨两侧**引用的是 `KernelInfo`（内核经 `arch_boot` 读它，直核面与接收侧构造它）；`BootStorage`/`BootReport`/`BootPrepareResult` 是引导侧的接口类型，放在同一库是为了让多个引导形态共享同一套接口，不是因为内核引用它们。若 `KernelInfo` 定义住在引导层，内核引用它就得依赖 boot-shim——反向依赖、且把固件实现拖进内核依赖面；若住在内核里，共享库 `minix-boot` 就得反向依赖 `minix-kernel`，与已存在的 `minix-kernel → minix-boot` 成环（boot-shim → minix-kernel 本身不是环——依赖图里本来就有，boot-shim 调 `enter_kernel_image_v1`）。所以这层定义下沉到不含 UEFI/SBI 固件实现、但共享参数归一化与 DTB 解析等实现的 `minix-boot`（依赖表是 `minix-types` 与 `fdt`），谁依赖它都不会被拖进固件代码。这条边界不是风格洁癖：`os/kernel/Cargo.toml` 的依赖表里没有 boot-shim、也没有 uefi，用上面那条 `sed` 就能一次验证。

**问题二（完整形态论证归 §3.1 例外条款与 §3.7；此处只给与依赖方向有关的结论）：riscv64/aarch64 上的引导面为什么长成"内核镜像自举"**。背景见 §3.1 表后的引用段：RISC-V 与 ARM 的 `-kernel` 极简装机（QEMU `-kernel` 模拟的是真实硬件的这种装载方式）下，下一级镜像直接进 RAM、固件跳过去，固件与内核之间没有"装载与被装载"的分界，独立 boot-shim 没有存在位置。于是两个架构的引导准备各自由 `kernel-image` 自带的引导面承担——riscv64 是 `bootface`（[`bootface.rs`](os/kernel-image/src/bootface.rs)），aarch64 是 `bootface_a64`（[`bootface_a64.rs`](os/kernel-image/src/bootface_a64.rs)），流程逐段同构（DTB 解内存 → 取件表 → bump 池（bump 池 = 一段只前进不回收的线性分配器：从区间起点逐页发放，游标只增不减，耗尽即拒绝）→ 模块 → `KernelInfo` → 同一个 `arch_boot`）。两者都复用 boot-shim 的**库函数**，但不是独立引导器产物；这一形态的固件对话走 DTB 与 PSCI/DMB（DMB = 数据内存屏障）一类的裸协议，`kernel-image` 与 `minix-kernel` 两个 crate 都不依赖 `uefi`，单向边界原样成立。它的定位是**平行的装机面**：与 UEFI 独立 shim 共享同一套取件库与 `arch_boot` 收敛点——统一发生在层内（类型、取件库、收敛函数），引导形态层不造伪统一（memmap 源统一下沉的裁决，三形态对照见 §3.7）。

**如果让 `minix-kernel` 直接依赖 boot-shim 会怎样**（动机多半是"想复用 `BootPrepareResult` 的定义"）：复用类型走 `minix-boot` 即可，不必依赖引导层（问题一已答）。剩下的否决理由是本节标题本身：**内核不该认识固件形态**——屏蔽固件差异正是 boot-shim（或 bootface）存在的意义。"riscv64 编译不过"（UEFI 目标只有 x86_64/aarch64/i686，riscv64 暂不支持）不构成独立论据，换 opensbi feature 就能过；否决理由从头到尾只有依赖方向一条。

### 3.4 从全局 `kinfo` 到按值交接的 `KernelInfo`（含 wire、探针、所有权三个附节）

C 里 `kinfo` 是一个全局变量。`pre_init.c` 开头就定义 `kinfo_t kinfo;`，`get_parameters(ebx, &kinfo)` 往里填，`pg_utils.c` 里的记账函数通过 `cbi` 形参收到它的地址，`pre_init` 返回 `&kinfo`，`kmain` 再把这个指针传下去。整条链上传递的是**地址**，而地址之所以有效，是因为所有代码都在同一个地址空间里、链接成同一个镜像。

Rust 侧把这份共享状态改成一段**显式交接**，分两个维度说。

本节的交付形状与另外四节分工，读的时候不要混：C 侧"清单被截到 16 条"在 §2.4.1 末；扣减表与 bump 整段保留在 §3.8；接收方的七个落地区在 §4.5；换手给 VM 的时序在 §4.7。本节只讲"交过去的是什么形状"。

**维度一：通道形态**（交接怎么送到接收方）随交接形态变；"跨镜像 / 直核 / 三形态"的定义在 §3.7，这里先用。x86_64 与 aarch64 的 UEFI 链都走跨镜像跳转：boot-shim 建好自举根、开分页，把一块八页 arena（定义与分区见本节后半）的物理地址递给独立 `kernel.elf` 的高半入口（x86_64 用 `rbx`，aarch64 用 `x0`，见 §3.7）；接收方从 arena 解码七个分区、落进自己的 `.bss`，再走只增量的高半 resume。riscv64 的 U-Boot shim 走第三条跨镜像 v1 腿（同样只递 arena 基址，入口寄存器 `a0`，见 §4.8）；riscv64 的直核自引导没有独立 shim，引导面在同一镜像内直接调 `arch_boot`；这是硬件形态决定的辅助启动路径（§3.3 问题二），与两条 UEFI 产线不同源。

**维度二：载荷内容**（送过去的是什么）三形态一致：必须按值深拷进接收方的私有存储，不能靠指针跨边界存活。理由：跨镜像档里两个分别编译的二进制各有自己的 `.bss`，指针在接收方指向完全无关的内存，连 `&'static KernelInfo` 也只保证生命周期、不保证页在对方页表里被映射；直核面虽然没有跨镜像指针，但同一套落地区把三形态收成一条路径（少一套特例），也让"解码之后再消费"成为统一次序。三形态一致的是"必须深拷进私有存储"这一条要求；具体来源分两组：v1 三条腿的切片由固定分区与 `byte_len` 描述符重建，接收方按分区偏移与元素类型解码、不再解释裸 `(ptr, len)`；两个直核面的切片由 `build_kernel_info` 的 `&'static` 输入直接给出，没有 wire 解码步骤（§4.4）。

> **收口记录**：x86_64 与 aarch64 的 UEFI 产线采用独立镜像交接（§3.7），本节与此一致——记录保留是为了给"为什么 §3.4 的通道描述里没有低地址内联窗口"一个可考的出处。riscv64 各路径的接线状态见 §3.1.1 的接线状态表，这里不重复；本段要留的独特信息只有一条：这次切换不改变 §3.4 讲的 KernelInfo 形状与快照口径。

#### 3.4.1 交接的两个维度与 `KernelInfo` 结构

```rust
// 代码引用（略去文档注释），os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],
    pub reserved_regions: &'static [MemoryRegion],
    pub memmap_sources: &'static [SourceRecord],
    pub startup_objects: &'static [SourceRecord],
    pub kern_virt_base: VirBytes,
    pub kern_phys_base: PhysBytes,
    pub kern_size: u64,
    pub user_sp: VirBytes,
    pub kern_stack_top: VirBytes,
    pub syscall_entry: VirBytes,
    pub boot_modules: &'static [BootModule],
    pub platform_sources: &'static [PlatformDescSource],
    pub param_buf: &'static [u8],
    pub authority: InputObjectId,
}
```

> **编号图例**：正文里的 `D01-Fnn`、`R2`/`Round 1`、`R14`/`R16`/`R17`/`R18`/`R19` 等是评审批次号，`D-1`/`D-4`/`D01 裁决项 3` 是当轮设计/裁决编号，`T2`、`Phase A.2` 是过程代号；`A+D` 指 §4.2.1 动作 2 的顺序（先读 ELF、预订 LMA 跨度，再分配 root/bump、拷贝段），`裁决 A` 指删除 UEFI `BootShim` 辅助实现的裁决（见 §3.1.1/§3.2；trait 本身已在 R23 删除）；都不是源码符号；它们只作出处索引，跳过不影响结论。需要回查时：字段删除→§3.5/§3.9、扣减换手→§3.8 定义/§4.7 读法、GET_KINFO 载荷→§3.4.2。**"字段删除"** 是本文对一次设计裁决的简称：删除 `KernelInfo` 的 `bootstrap_start/len` 与 `free_upper_idx` 三个字段，论证见 §3.5 与 §3.9。

`kernel_info.rs` 的 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 共 14 个字段（字段删除时去掉了 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个——见 §3.5 与 §3.9；`authority` 由 R23 的 F-R17-04 加入，总数从 13 变成 14）。按与 C 的关系分成下面几类：

| 关系 | 字段 | 说明 |
|---|---|---|
| 保留（同名或近义） | `memmap`<br>`user_sp` | `memmap` 从 `multiboot_memory_map_t memmap[MAXMEMMAP]` 变成 `&'static [MemoryRegion]`，长度自带。C 那条"VM 只读前 `MIN(MAXMEMMAP, NR_MEMS)=16` 条"的截断（§2.4.1 末的"另一件必须单说的事"段）**不再继承**：Rust 把容量提到 128（常量现居 [`globals.rs`](os/kernel/src/globals.rs:MEMMAP_REGION_STORE_LEN)，C `MAXMEMMAP=40` 提额——UEFI 图常超 40 条），VM 交接消费上限 64 条的账，超过即在容量断言处停机（注意三级上限：`KernelInfo.memmap` 128 条，`VmBootHandoff.free_regions` 64 条，`VmBootHandoff.ident` 64 条（后两者不是同一个常量）；超过 64 会在交接容量断言处停机）——16 条静默截断是 C 的行为，不是 Rust 的 |
| 改型（C 有、形状变） | `param_buf`：`char[MULTIBOOT_PARAM_BUF_SIZE]` → `&'static [u8]`，上限仍按 1024 校验<br>`module_list[]` → `boot_modules: &'static [BootModule]` | |
| 改名 | `vir_kern_start` → `kern_virt_base` | （`freepde_start` 曾改名 `free_upper_idx`，字段删除时整体退场，见 §3.9） |
| 升为字段 | `kern_phys_base`<br>`kern_size` | C 里这两个值不是 `kinfo_t` 的字段，而是链接脚本导出的符号 `_kern_phys_base`、`_kern_size`（`_kern_phys_base = 0x00400000`、`_kern_size` 由链接布局算出；完整摘录见 §3.5）。Rust 把它们放进交接结构，来源改为 boot-shim 解析内核 ELF 的 PT_LOAD 段（§4.4 的字段表）。`kern_size` 用 `u64` 而不是 `usize`——wire 语义（wire = 跨镜像交接协议里按固定字节宽度写进 arena 的那份表示，与进程内结构相对）需要固定宽度，不随目标指针宽度变（32 位目标是 C 侧的，本仓重写目标全为 64 位，这里钉的是交接协议而非当前目标集） |
| 删除 | 21 个字段：<br>`mbi`、`mem_high_phys`、`mods_with_kernel`、`kern_mod`、`kmessages`<br>`do_serial_debug`、`serial_debug_baud`、`minix_panicing`、`user_end`、`boot_procs[]`<br>`nr_procs`、`nr_tasks`、`release[]`、`version[]`、`vm_allocated_bytes`<br>`kernel_allocated_bytes`、`kernel_allocated_bytes_dynamic`、`mmap_size`、`freepde_start`<br>`bootstrap_start`、`bootstrap_len` | 逐个的去向见下表（`mmap_size` 由切片长度承载；`freepde_start`/`bootstrap_*` 的删除判据见 §3.9 与 §3.5） |
| 新增 | `reserved_regions`<br>`memmap_sources`<br>`startup_objects`<br>`kern_stack_top`<br>`syscall_entry`<br>`platform_sources`<br>`authority` | 各自来源见下；`memmap_sources` 与 `startup_objects` 是 v1 source/startup 记录，供启动期 `validate_requiring_sources`（strict 门）审计，`KernelInfo::validate`（非 strict）在 `memmap_sources` 非空时也查“来源记录覆盖投影内存图”；strict 另要求 `startup_objects` 盖住内核镜像；启动期之后没有读者（`store_kernel_info` 与两道校验门都在启动期读）。`authority` 是这份内存图的输入权威，见下表后的绑定说明 |

**权威与内存图绑定（F-R17-04）**。`authority` 记录这份内存图是从哪个输入对象投影出来的：`DtbBootInput`（设备树）、`UefiMemoryMap`（`GetMemoryMap` 快照）或 `StartupObjects`（镜像自身的活对象）。它由构造点写入——wire 解码把 [`BootHandoffV1`](os/libs/minix-boot/src/handoff_v1.rs) 信封里的 `ram_authority` 复制进来，两个直核面在 `build_kernel_info` 调用点传 `DtbBootInput`，UEFI 腿传 `UefiMemoryMap`——`validate_requiring_sources()` 因此不再收参数，直接读字段。改之前权威是调用方的另一个实参，`KernelInfo` 与权威在类型上可以任意配对；混合记录表里"DTB 记录 + 声明 UEFI 权威"这类错配只会在下一次覆盖审计时才暴露。把权威放进结构体没有让错误的构造变得不可能（构造点仍可传错），但它把"图和它的声明"变成同一个值，解码路径不再有两个可以接反的参数。

**先讲清前两格装的是什么：内存图交付成两张表**。`KernelInfo` 携带内存信息只用两个列表，形状固定，来源不固定：

| 列表 | 语义 | 内核侧怎么用 |
|---|---|---|
| `memmap` | 可用内存；物理内存管理器从这里分配帧，但列表**不保证已扣除全部占用区**。内核镜像在自举腿与 U-Boot shim 腿的表内；两条 UEFI 腿在 A+D 之后用 `AllocateAddress` 把整段 `[kern_phys_base, +kern_size)` 预订成 `LOADER_DATA`，因此**不在**表内、分类落 `ReservedMapped`（无 `WRITE_BACK` 时落 `Side`，§4.2.2）。模块同理：DTB 腿的 bump 页在表内，UEFI 腿是 `LOADER_DATA` 因而不在。分配前仍须扣掉占用区（§3.8 扣除表；`vm_handoff::classify`，UEFI 腿的内核行退化为空操作）。LMA/VMA、conventional 与 `LOADER_DATA` 的定义见 §3.5/§4.2.2 | 内核不保留可变 memmap 副本（Phase A.2 已移除，`FREE_MEMMAP` 已没有符号定义（仅 `bootface_a64.rs` 的一行注释提及该字符串））；`vm_handoff::classify` 在交接时对 memmap 做扣除，产出 free list 与 deduction record |
| `reserved_regions` | 可用但不可分配。运行期经 Direct Map 能访问，但不得再分配出去 | Direct Map 覆盖的候选之一（[`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4） |

五条生产腿产出这两张表的方式不同，形状相同：

- **两条 UEFI 链**：在 `ExitBootServices` 的配对循环里读 `GetMemoryMap`，按固件的类型标签分流（分流规则见 §4.2）。分类必须与退出共用同一个 MapKey，所以它只能发生在那一个循环里（`MapKey` 的定义、配对理由与重试语义见 §4.2）。
- **riscv64 的 U-Boot shim**：读 DTB `/memory` 的 `reg`（`DtbMemoryMap`，[`memmap_source.rs`](os/libs/minix-boot/src/memmap_source.rs)），再用固件常驻区上界 `KERN_PHYS_BASE` 削出保留段（OpenSBI 占 `[0x8000_0000, 0x8020_0000)`）。
- **两个直核面**：同一份 DTB 解析实现；aarch64 传 `fw_end = 0`，因为该形态的 DRAM 里没有固件常驻区——`fw_end=0` 只关闭"固件前缀裁剪"，DTB `/reserved-memory` 声明的区间仍进保留表（该形态典型为空，但机制上不保证）。

收敛点是**数据形状加两个构造入口**（直核腿共用 `build_kernel_info`，v1 腿共用 `BootStorage`/`BootReport`），不是行为多态。两条直核腿把两张表放进 [`fn build_kernel_info`](<os/boot-shim/src/lib.rs:fn build_kernel_info>) 的同样两个参数位；三条 v1 发送腿（两条 UEFI 链 + U-Boot shim）把同两张表写进各自的 `BootStorage`，报告视图的字段形状与 `KernelInfo` 一致。消费者只认区间、不认来源。这里没有一条"内存图来源"的 trait 承担统一，原因不是懒：UEFI 那条腿的分类时机被固件协议绑住——MapKey 与 `ExitBootServices` 一一配对，缓冲区不足与 MapKey 失效还要重试——一个只返回两张表的签名携带不了这些约束。

**"升为字段"行的一个设计细节：为什么三个布局数字由 shim 从 ELF 算出，而不是由调用者传入**。设想把接口改成 `prepare_boot(storage, kern_virt_base, kern_phys_base, kern_size, bump_pages)`。这三个数字必须与内核镜像的链接布局一致，而它们现在的来源是 ELF 的 PT_LOAD 段（`loader.rs:compute_kernel_layout` 取所有 PT_LOAD 的 `min(paddr)`、`min(vaddr)`、`max(paddr+memsz)-min(paddr)`）。改成调用者传参后，写错的地方不会被 `check-layout.sh` 拦住：那个脚本校验的是**镜像自身**（首段 VMA/LMA、段间平移一致、跨距对齐），不校验调用者手里的常量。错值进入 `arch_boot_impl` 之后，"把内核映射到高地址"那一步会按错误的 `kern_virt_base` 建映射，症状是开启分页后取指失败——一个"参数错了"的问题表现为"页表建错了"。让 shim 自己解析镜像，"数字与镜像一致"就不再是调用者的责任，而是唯一数据来源（镜像本身）的自然结果。

#### 3.4.2 旧字段台账：`kinfo_t` 的 21 个字段去哪了

这 21 个字段的删除多数是换载体，少数是随读者消失而退役。多数：C 的 `kinfo` 是全局变量，编译期常量、内核自有状态、记账表都顺手堆在里面；Rust 的 `KernelInfo` 收窄为纯交接结构，只装引导侧必须告知、接收方无法自行导出的内容。于是职责归位成三类：**上浮为常量**——编译期已知的布局与规模事实（`nr_procs`/`nr_tasks`）与构建身份（`release[]`/`version[]`）；`user_end` 不属此类（Rust 未回传、无消费者，见下表）；**移交专门设施或记录结构**——内核自有的诊断与记账状态（`kmessages`、`do_serial_debug`/`serial_debug_baud`、`minix_panicing`、三个以 `_bytes` 结尾的记账字段）；**换表达不换信息**——内容改由 `KernelInfo` 现存字段承载或推导（`mbi` 的内容散布进各字段，`mods_with_kernel`/`kern_mod` 由 `kern_phys_base`/`kern_size` 直接表达，`boot_procs[]` 从 `boot_modules` 现算）。找不到新读者的不只 `mem_high_phys`（恒等映射只短暂存在、帧分配直接遍历 memmap，该标量随之退役）；`bootstrap_start/len` 与 `freepde_start` 也在该次删除中因无真实读者退场（§3.5/§3.9）。

被删的 21 个字段逐个的去向（"接管者"一栏给的是 `KernelInfo` 侧去向；`nr_procs`/`nr_tasks`/`release`/`version` 四个字段在 `KernelInfo` 侧上浮为常量/构建身份，但在 `GET_KINFO` 的 `KinfoStruct` 里仍是活字段，见 §3.4.2）：

| C 字段 | 接管者 / 去哪了 | 依据 |
|---|---|---|
| `mbi` | 不进内核：UEFI/SBI 没有 Multiboot 结构体，交接信息由 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 另行承载 | - |
| `mem_high_phys` | 删除。C 里它是"最高物理地址"标量，有真实读者：恒等映射铺满整个 4GB 目录，靠它把 RAM 顶以上没有内存的段标成禁缓存；PCI 驱动另拿它找 RAM 之上的 MMIO 空洞。Rust 的恒等映射只在 bootstrap 内短暂存在、随后在**同一张已激活的根上**逐页撤掉、由 Direct Map 接管（不重建根——重建会清零活动根、下一条取指页错误，见 §3.7 反向推演；C 的同一动作靠 TLB 窗口撑住，见 §2.6.4；逐页拆解本篇不展开，边界见 §1.4），帧分配器直接遍历 memmap 区域列表——这个标量在 Rust 里没有读者，维护它只是写后即弃的状态 | C 读者：[`pg_identity()`](minix3/minix/kernel/arch/i386/pg_utils.c)、[`complete_bars()`](minix3/minix/drivers/bus/pci/pci.c) |
| `mods_with_kernel` | 删除：C 里它只为一个用途——把内核临时登记成第 M+1 个模块（M = 模块数），让重叠检查与 memmap 扣除一并处理内核自身（[`pre_init()`](minix3/minix/kernel/arch/i386/pre_init.c) 的登记段）。Rust 里内核布局由 `kern_phys_base`/`kern_size` 单独表达，扣除与检查各自直接用区间，不需要这个计数 | - |
| `kern_mod` | 删除：C 里它是上面那个"临时第 M+1 个模块"的下标，随 `mods_with_kernel` 一起消失 | - |
| `kmessages` | 独立的内核消息/诊断设施 | [`kmess.rs`](os/kernel/src/kmess.rs)（`KernelConsole`） |
| `do_serial_debug` | 早期控制台初始化自己决定串口 | [`early_console.rs`](os/plat/src/early_console.rs) |
| `serial_debug_baud` | 同上（平台实现固定串口分频，不保留可调波特率状态） | 同上 |
| `minix_panicing` | 内核侧由 `PANIC_DIAG_ACTIVE` 维护互斥；boot-shim 的 `fn panic` 只是 panic 出口，不承载该状态 | `PANIC_DIAG_ACTIVE`（`os/kernel/src/lib.rs` 的 panic 诊断互斥静态量）、[`fn register_panic_diagnostic`](<os/kernel/src/lib.rs:fn register_panic_diagnostic>) |
| `user_end` | C 侧是 `USR_DATATOP`（i386 上等于 `USR_STACKTOP`）；Rust 未回传该字段，也没有消费者 | [`const USER_STACK_TOP`](<os/libs/minix-types/src/types/boot.rs:const USER_STACK_TOP>) 只对应 `user_sp` |
| `boot_procs[]` | 内核侧从 `boot_modules` 构造 boot image 表 | [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) |
| `nr_procs` / `nr_tasks` | 常量与表长度 | [`const NR_PROCS`](<os/kernel/src/proc_table.rs:const NR_PROCS>)、`:const NR_TASKS`（12 是启动模块数 `NR_BOOT_MODULES`，不是 `nr_tasks` 的载体） |
| `release[]` / `version[]` | 内核构建身份，来源为 `minix-types` 里与 config.h 对位的固定字符串常量（`OS_RELEASE`/`OS_VERSION`） | [`misc.rs`](os/kernel/src/misc.rs) 的 GET_KINFO 段、[`kerninfo.rs`](os/libs/minix-types/src/types/kerninfo.rs) |
| `vm_allocated_bytes` | VM 交接记录的字段 | [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) |
| `kernel_allocated_bytes` | 同上（记录里的 `kernel_allocated_static`） | 同上 |
| `kernel_allocated_bytes_dynamic` | 同上（记录里的 `kernel_allocated_dynamic`，值来自 bump 分配器用量） | [`fn boot_alloc_used_bytes`](<os/kernel/src/boot_alloc.rs:fn boot_alloc_used_bytes>) |
| `mmap_size` | 未映射为 Rust 字段：C 的"遍历边界（最高占用下标加一）"由切片长度 `.len()` 直接承载——`memmap` 从定长数组 + 边界标量变成 `&'static [MemoryRegion]`，长度自带 | [`globals.rs`](os/kernel/src/globals.rs:MEMMAP_REGION_STORE_LEN) 的容量常量与 §2.4 的边界语义 |
| `freepde_start` | 曾改型 `Option` 跨 handoff 携带，字段删除时整体退场（删除论证与报告位填 0 的去向见 §3.9） | [`misc.rs`](os/kernel/src/misc.rs) 的 `getinfo_kinfo` |
| `bootstrap_start` / `bootstrap_len` | 曾以 `(PhysBytes(0), 0)` + 硬断言携带，字段删除——误设回收区间的接口面随字段一起消失（论证、过度回收风险表与 wire 版本变更见 §3.5） | §3.5 |

C 的两个专用键在 Rust 侧的处置：`cttyline`/`cttybaud` 不再有专用字段，`normalize_param_buf` 把它们当普通 `key=value` 保留（能进 `param_buf`），但早期串口的选择由 `early_console` 的平台/编译配置固定，运行期不可调。


**外部可见面的一条架构演进** `[ARCH: GET_KINFO 载荷收窄为五个 used 字段 + 两个 C 布局占位；其余字段由 GET_IMAGE/GET_MONPARAMS/GET_KMESSAGES 等分开提供]`：C 的 `GET_KINFO` 把整个 `struct kinfo` 拷给调用者（[`do_getinfo.c`](minix3/minix/kernel/system/do_getinfo.c) 的 `length = sizeof(struct kinfo)`）；Rust 只拷 48 字节的 `KinfoStruct`（[`kinfo.rs`](os/libs/minix-types/src/types/kinfo.rs)），只含七个字段——其中**五个有 Rust userland 读者**：`user_sp`、`nr_procs`、`nr_tasks`、`release`、`version`；另两个是保留的 C 布局位：`freepde_start` 恒填 0（Rust VM 不借槽，见 §3.9）、`vir_kern_start` 有取值来源但仓内零读者。C 侧至少四个活读取点：`dmp_kernel.c` 读四个字段，`procfs/root.c` 读 `nr_procs`/`nr_tasks`，`pci.c` 的 `complete_bars()` 读 `mem_high_phys`，VM 的 `main.c`/`pagetable.c`/`utility.c`/`region.c` 合计读 ≥11 个字段（`memmap`/`mmap_size`、`module_list`/`mods_with_kernel`/`kern_mod`、`boot_procs`、`user_sp`、`vir_kern_start`、`freepde_start`、三个 `_bytes` 等）。Rust 收窄的是**这条系统调用的载荷**，不是"C 只有这些读者"：C 侧其余字段由 `GET_IMAGE`/`GET_MONPARAMS`/`VmBootHandoff` 等替代通道承载，`mem_high_phys` 在 Rust 侧已删除。

受影响的 C 字段——`memmap[]`/`mmap_size`、`module_list[]`/`boot_procs[]`、`param_buf`、`kmessages` 指针、`bootstrap_start/len` 等——不再经这条系统调用出现：模块表走 `GET_IMAGE`、`param_buf` 走 `GET_MONPARAMS`、`kmessages` 走 `GET_KMESSAGES`；内存账不走 GETINFO——它在 VM 启动前由内核写进固定地址的 boot handoff 页（`VmBootHandoff.free_regions` 加扣除记录），VM 在 `os/servers/vm/src/boot.rs` 读它。行为基线定为 Rust userland 的 `KinfoStruct`；按 C 全布局读被删字段的程序在本仓没有消费者。

表中两个"控制台"落点名字相近、层次不同：`EarlyConsole`（[`early_console.rs`](os/plat/src/early_console.rs)）是硬件输出通道——`minix-plat` 里的跨架构 trait，每架构只实现一个 `write_byte`（x86-64 COM1、aarch64 PL011、riscv64 SBI ecall），启动与诊断输出经它直写串口；`KernelConsole`（[`kmess.rs`](os/kernel/src/kmess.rs)）是内核诊断收口——对应 C 的 `kputc`，把流经的每个字节先记进 10000 字节环形缓冲（C `struct kmessages` 的重写）再转发给当前架构的 `EarlyConsole` 实现，环内容经 `GET_KMESSAGES` 快照供用户态读取。分工一句话：前者管"字节怎么落到硬件"，后者在其上叠加"内核诊断的记录"。

几处需要展开的字段，各自的来源与理由：

- `reserved_regions`：非 conventional 类型的内存区间。它曾装两种语义：其一（x86 内联窗口）是内核链进 boot-shim PE、在低物理地址执行时，VM 建进程页表必须逐段恒等复现的代码/GDT/IDT/TSS 窗口；其二（UEFI 链通用）是内核运行期经 Direct Map 访问的固件分配（loader 数据、内核栈一类不在 conventional 列表里的页），建立 DM 覆盖时必须列入候选（依据见 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4）。x86_64 切独立镜像后第一种用途退场：`vm_handoff` 的 kern_ident 输入返回空（架构 MMIO 恒等窗口也随 T2 切分退役）；第二种用途保留，作为 Direct Map 覆盖的 source 4。各条腿怎么产出这份列表见本节开头的两张表。
- `startup_objects`：**规范集合**——内核镜像、每个引导模块、根页表页、bump 池、交接 arena（腿上有才写）。五条生产腿写同一个集合（四个写点：两条 UEFI 链共用 uefi_helpers 一处写点，U-Boot 链在 `prepare_boot`，两个直核面各一处）：UEFI 腿在 EBS 之后补写五类并提交（序列化点只读）；U-Boot 腿的 root/bump/arena 都在 `prepare_boot` 内分配并立即写入 storage 的 startup 槽（序列化点只读）；两个直核面在 `bootface`/`bootface_a64` 写 root 与 bump（无 arena）。strict 门 `validate_requiring_sources` 除 source 覆盖外还断言"至少一条记录盖住内核镜像物理跨度"——缺记录是生产者回归，不是空列表特例；手工构造的测试内核走 `validate_before_tables` 或直接 `arch_boot_impl`，不经过 strict 门。容量 20 = 16 条规范记录 + 4 槽余量。
- `kern_stack_top`、`syscall_entry`：C 里这两个值通过链接期符号与 `prot_init()` 内部计算得到；这里把它们显式纳入结构体，交接便不再依赖"两边链接脚本碰巧有同名符号"。
- `platform_sources`：固件描述符来源列表（DTB / RSDP），按 boot-shim 的偏好排序，内核取第一个解析成功的源。解析归 `04-platform-discovery.md`，本篇只讲它怎么被放进结构体。
- `param_buf`：对应 C 的 `char param_buf[MULTIBOOT_PARAM_BUF_SIZE]`（1024，`MULTIBOOT_PARAM_BUF_SIZE` 定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)）。它是 2.3 那张参数表在 Rust 侧的落点，生产侧由 `minix_boot::normalize_param_buf` 统一规范化：输入按 NUL 或 ASCII 空白分词（UEFI 记录以 NUL 分隔，DTB `bootargs` 以空格分隔），逐条要求可打印 ASCII 的 `key=value`；非 ASCII、无等号、空键与内部键（`ARCH`/`BOARD`）逐条拒绝；`ARCH` 由编译目标生成，`BOARD` 只有真实来源才生成（当前生产调用没有可靠来源，显式传 `None` 省略，不伪造——测试 `generates_arch_and_board_first` 传显式板名验证的正是"有来源才生成"这一支）。

  与 C 的可观察分歧共十条，按三档术语纪律统一标注 `[ARCH: 参数表语义规约化；十条可观察分歧见下]`：

- 其一，重复键——C 的"已存在先删"只对首条记录生效（§2.3 的源码推演），但结果分两支：若重复键的首个副本是第一条记录，旧条被删、新值追加，**后写胜出、只剩一条**；若首个副本不是第一条，旧条留在原位、新值追加到缓冲末尾，账上两条而读取返回第一条。Rust 统一为首条生效、后者逐条拒绝，因此在"重复键是第一条"这一支上与 C 相反（C 后写胜出），属于新增的可观察分歧，不是"C 缺陷语义的载体重构"。
- 其二，命令行里的 `ARCH=`/`BOARD=` 词——C 不拒；若外部写的是小写 `arch=`/`board=`，首条时会被内核随后写入的同名键覆写，大写拼写则与 C 的生成键不同名、不会被覆写。Rust 一律拒绝外部输入覆盖内部键，检查大小写不敏感（小写 `arch=`/`board=` 同样被拒，`rejects_internal_key_override` 覆盖）。
- 其三，`BOARD`——C 无条件写一条记录，但读取返回首条匹配、可被外部同名键遮蔽，因此不保证生效（§2.3）；Rust 按来源生成、无来源则省略（上文）。
- 其四，容量耗尽——Rust 一条放不下就丢弃它及其后全部记录，C 只丢当前条、后面的词仍收下（`drops_tail_records_whole_at_capacity` 锁住前者）。
- 其五，内部键拼写——Rust 生成大写 `ARCH=`/`BOARD=`，C 写小写 `arch=`/`board=`。
- 其六，分隔符集合——Rust 按 NUL 与 ASCII 空白（含 tab/CR/LF）分词，C 只按空格，同一 `bootargs` 可得到不同条数（`splits_nul_and_whitespace_records` 锁住前者）。
- 其七，字符集——Rust 拒绝非 ASCII/不可打印记录，C 原样存（`rejects_malformed_and_non_ascii`）。
- 其八，空键——Rust 拒绝 `=value`，C 存成一条 `=value`。
- 其九，`ARCH` 取值——C 写 `arch=i386`，Rust 写 `ARCH=x86_64`（编译目标）。
- 其十，记录顺序——C 先放命令行记录、后写内核生成键；Rust 生成键在前、外部记录按输入序在后。这十处都落在 `GET_MONPARAMS` 的 wire 上可观察；仓内消费者核查：kernel 与 minix-boot 读 `"board"` 键零命中，无读者受害。参数源分两类，按 §3.7 的三类链落到各腿：两条 UEFI 链读当前镜像的 `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions`；U-Boot shim 与两个直核面（按 §3.7 归 U-Boot 一族与直核面两类）读 DTB `/chosen/bootargs`。消费者 `GET_MONPARAMS` 仍按 C 的 1024 字节定长零填充；没有外部输入时缓冲里只有生成的那条 `ARCH=...`——与 C 的 `mb_set_param` 必写行为仅就 ARCH 一项同形。

#### 3.4.3 `#[repr(C)]` 的边界与 arena wire 协议

`#[repr(C)]` 在 `KernelInfo` 上的作用需要说清楚边界：字段顺序被钉住，切片内部 `(ptr, len)` 的顺序仍由工具链决定；因此 `KernelInfo` 继续只作为进程内结构使用（直核面、QEMU 测试内核，以及 UEFI 接收侧解码后在进程内构造的值；发送侧只构造 `BootReport` 借用视图），不承担跨镜像布局契约。跨镜像三腿（两条 UEFI 产线 + riscv64 U-Boot shim）改用独立协议 `BootHandoffV1`：标量全部写成固定宽度，载荷放进一个八页 arena（arena：boot-shim 预留的一块定长物理内存，里面摆着 v1 交接协议的信封与各载荷分区，分区布局见本节末）的固定分区，描述符只记 `byte_len`——地址由 arena 基址加协议偏移推导，`(ptr, len)` 不再上线。协议有两层版本号，读时不要混：**协议身份**恒为 v1（`BOOT_HANDOFF_V1_MAGIC`/名字里的 v1，指"第一代交接协议"这个族）；**信封版本号**（`BOOT_HANDOFF_V1_VERSION`，当前 9）随信封内布局变更递增、收发双方精确比对——本文出现的"版本 8→9"都是后者（"wire 版本"是同一计数器的另一个叫法；7→8 与 8→9 都指它）。

```rust
// 代码引用（略去字段文档注释；末尾 arena 分区注释为改述，源文件为英文原文
// `handoff_v1.rs` 的 `ARENA_CAP` 文档）。os/libs/minix-boot/src/handoff_v1.rs:struct BootHandoffV1。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoffV1 {
    pub magic: u32,
    pub version: u32,
    pub size: u32,
    pub ram_authority: u32,   // 〔编注〕内存图的权威来源：0 = boot DTB、1 = UEFI 内存图；接收方只认这两个值，声明了权威却无对应 source 记录则 fail-closed（§3.4 的 startup/source 规范集合与 §4.4 的 `validate_requiring_sources`）
    pub arena_len: u64,
    pub kern_virt_base: WireVirtAddr,
    pub kern_phys_base: WirePhysAddr,
    pub kern_size: u64,
    pub user_sp: WireVirtAddr,
    pub kern_stack_top: WireVirtAddr,
    pub syscall_entry: WireVirtAddr,
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
    pub memmap_byte_len: u64,
    pub reserved_regions_byte_len: u64,
    pub boot_modules_byte_len: u64,
    pub platform_sources_byte_len: u64,
    pub param_buf_byte_len: u64,
    pub sources_byte_len: u64,
    // Startup-object records (their own partition and capacity).
    pub startup_byte_len: u64,
}

// 八页 arena（32768 B）自低往高：信封 ≤256 B → wire 分区（memmap/reserved/modules/
// platform/param 五段，至 WIRE_END=7872）→ source 分区（至 29184）→
// startup 分区（至 30624，规范集合定稿后容量 20）。
// 信封与各组分区相加 30624 ≤ 32768，余 2144 B：信封 256 B + wire 分区 7616 B
// （内含 memmap/reserved/modules/platform/param 五段）+ source 分区 21312 B +
// startup 分区 1440 B。v9 删掉了 v8 的 6144 B 语义工作分区：v8 的同样四项合计 36768 B（九页 36864 B），v9 前移后为 30624 B（八页 32768 B）；发送侧的语义表
// 住进 shim 自己的 `BootStorage`，arena 里只剩 wire 这一份表示。
pub const ARENA_CAP: usize = 8 * 4096;
```

arena 的分区是固定偏移：信封住首部，其后依次是内存图、保留区、模块、平台源、参数字节的 wire 形式；v9 之后 arena 里**没有**第二份语义表——发送侧的分类结果同时写进它自己的 `BootStorage`（调用方持有的定长数组）和 wire，接收方再按 `byte_len` 把 wire 复制进自己的 `.bss` 落地区。这样 arena 只有一份权威表示，发送侧构造进程内视图时借用 `BootReport<'a>`（`BootStorage` 的视图），不再用 `'static` 切片指向 arena。接收方先校验 `magic`、`version`、`size` 与 `arena_len`，再按各 `byte_len` 把七个分区复制进自己的 `.bss` 落地区；越界、不能整除或容量不足都在构造任何引用之前失败。运行期字段（根页表物理地址与 bump 三件套）在 shim 建好根之后原地补进信封，因此三腿都只需传 arena 基址一个数（x86_64 `rbx`、aarch64 `x0`、riscv64 U-Boot shim `a0`）。

#### 3.4.4 深拷不变量与真机探针

**不做深拷、把 `KernelInfo` 的切片按原指针存进内核的全局槽会怎样**。两条 UEFI 产线的七个分区都由 boot-shim 在启动服务池或 arena 里产出；无论哪一条，接收方都必须在首次进程根切换之前把值搬进自己的 `.bss`——arena 只在恒等窗口存活，池页在历史真机上还出现过零化。真机记录给了这一现象的两组读数（无归档日志，仅为当时观察；可靠性低于可复跑探针），零化时点不同：`res=127 zeroed=127`（`res` 是到达分类点的 `reserved_regions` 条目数，`zeroed` 是其中因长度读回零而被拒的条数——127 对 127，也就是全部读成了零），以及"15 条 conventional 条目在建立 Direct Map 覆盖时还完好、到 VM 交接分类点长度全读成 0"。后者的后果不是某个字段错，而是 VM 的自由内存清单塌成零条——VM 在启动参数校验处 panic，而"内存图丢了"这件事在 panic 输出里看不到。两组读数指向同一个设计结论（值必须在入口尽早深拷——这是一条预防性不变量，它的依据是"零化发生过且无法在故障输出里归因"，不是"某条机制已由实验证实"）；机制上有两条候选：源码注释给过的一条是"内核自己的页表不重放这些堆页的 1:1 映射"，另一条是"建立 Direct Map 覆盖时还完好"这组读数更像的"被当空闲复用后清零"。两条候选各带可检验的推论——"页表不重放"说与切表时点相关、"复用清零"说与快照/复用的时序相关——本篇用本仓探针做了初步排除（读数与三层判别见 §5.2.3）。



#### 3.4.5 数据所有权与借用：`BootStorage`、`BootReport` 与 `FileBytes`

v8 之前的发送侧有一处"假所有权"：UEFI 腿把 `KernelInfo` 的 `&'static` 切片直接指向 arena 的语义分区，模块、平台源与参数的切片则靠 `Box::leak` 凑出 `'static`；类型上写着"活到进程结束"，实际上只在跳转前有效。v9 把两件事分开：

- **`BootStorage` 是所有者**。调用方在 shim 自己的 `.bss` 里放一个定长数组结构（内存图 128 项、保留区 256 项、模块 12 项、平台源 4 项、参数 1024 B——这五类载荷的容量与 wire 分区一致，两边不能各自漂移；另有来源记录 40 项（DTB 收集器的输入上限；DTB 腿序列化时把 40 条前缀写入 wire 的 296 项分区，超过 40 条由收集器 fail-closed；UEFI 腿由分类器直接写 wire，不受这 40 槽限制）与 startup 记录 20 项（wire 分区容量））。分类器把同一条目同时写进 storage 与 wire；模块装载器直接写进 storage 的模块槽；平台源与参数也一样。数组长度只有在写入前缀之后才提交（两张内存表由 `commit_map_tables` 一次提交，其余表由各自的 `set_*_len` 提交），报告永远看不到未写槽。来源记录只有 DTB 发送腿写入 storage（UEFI 腿由分类器直接写 wire，storage 的 source 槽留空）。
- **`BootReport<'a>` 是视图**。它借用 storage 的七个切片（五类载荷加 source/startup 记录），加上六个标量（内核基址/大小、用户栈顶、内核栈顶、系统调用入口）。生命周期参数把它绑定在 storage 上：没有 `'static` 所有者时，报告无法被提升为 `'static`。内核入口 `enter_kernel_image_v1` 的第二个参数就是 `&BootReport<'_>`，跳转前只读它；接收镜像仍然用 `KernelInfo` 的 `'static` 切片，因为那里的切片指向自己 `.bss` 里的深拷落地区，是真实所有权。
- **U-Boot 腿同构**。`opensbi_helpers::prepare_boot` 收 `&mut BootStorage`，内存图/保留区/来源记录/参数/模块/平台源全部写进 storage；arena 也在该函数内从同一 bump 池分配，五类 startup 记录（内核、模块、root、bump、arena）在返回前写齐。`BootPrepareResult<'a>` 只携带报告视图与两个类型化地址（`root_page: PhysBytes`、`bump: PhysRange`、`arena_pa: PhysBytes`），UBoot 主函数用 `write_envelope_report` 和同一份报告序列化，不再有 `Box::leak` 或 `from_kernel_info`。
- **`FileBytes<'a>` 显式区分借用与拥有**（D-4）。`FileLoader::read` 返回 `FileBytes::Borrowed(&'a [u8])` 或 `FileBytes::Owned(Vec<u8>)`：U-Boot 腿的 `fatload` 已经把文件放在 RAM 里，直接借出（不再 `to_vec`）；UEFI 腿经固件协议读文件，必须拥有一个新缓冲。借用期间的合同是三条：持有者存在时底层存储不得被覆盖（U-Boot 表在 shim 运行前写好、之后不再改）；源/目标重叠检查保留（loader 的 `first_source_destination_overlap` 仍然在拷贝前运行）；loader 本身不被长期独占（借用只跟随一次读调用，不把 `&self` 存进结果之外的结构）。`FileBytes::into_owned()` 只在确实需要拥有时复制，不改变默认零拷贝路径。

协议头带版本号，接收方拒绝不认识的版本。这一点把跨镜像契约从"两个镜像恰好由同一工具链构建"的工序假设，变成可显式验证的版本化 ABI；旧的一页 `BootHandoff` blob 因此在切根（从自举根切到运行期根）落地后删除。

### 3.5 `bootstrap_start/len`：从"可回收区间"到"必须为 0"，再到字段删除

C 的这两个字段装的是链接脚本导出的一小段内核镜像内区域的边界（2.3 已看到赋值）。这段区域的性质写在链接脚本里：

```ld
/*
 * 节选（保留段划分与偏移量相关的行），minix3/minix/kernel/arch/i386/kernel.lds。
 */
_kern_phys_base = 0x00400000;	/* phys 4MB aligned for convenient remapping */
_kern_vir_base =  0xF0400000;	/* map kernel high for max. user vir space */
_kern_offset = (_kern_vir_base - _kern_phys_base);

SECTIONS
{
	. = _kern_phys_base;
	__k_unpaged__kern_unpaged_start = .;

	.unpaged_text : { unpaged_*.o(.text) }
	.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }
	.unpaged_bss  ALIGN(4096) : { unpaged_*.o(.bss COMMON) }
	__k_unpaged__kern_unpaged_end = .;

	. += _kern_offset;

	...
	.text             : AT(ADDR(.text) - _kern_offset) { *(.text*) }
	...
	.bss ALIGN(4096)  : AT(ADDR(.bss) - _kern_offset) { *(.bss* COMMON)
		__k_unpaged__kern_size = . - _kern_vir_base;
		_kern_size = __k_unpaged__kern_size;
	}
	_end = .;
}
```

逐条解码：

- 前三个赋值就是 2.6 里 `kern_phys_start`/`kern_vir_start` 的来源，也是 02 篇讲 `AT()` 的两条基址。
- `_kern_size` 不是字面量而是布局算出的符号：`.bss` 收尾处 `__k_unpaged__kern_size = . - _kern_vir_base`（整幅镜像的虚拟跨度），随后 `_kern_size = __k_unpaged__kern_size` 把它导出成 2.5/2.6 引用的那个名字——它的值随每次构建的段布局而定，没有固定数字。
- `. = _kern_phys_base;` 让第一段的位置计数器落在物理基址上；`unpaged` 三个段（来自文件名匹配 `unpaged_*.o` 的目标文件）放在这里，它们的**执行地址等于装载地址**（没有 `AT()` 偏移）。
- `__k_unpaged__kern_unpaged_start/end` 把这段夹出边界，交给 `get_parameters` 填进 `bootstrap_start/len`。用一对起止符号而不是单一起点，是因为下游要的是半开区间 `[start, end)` 的长度——回收接口 `add_memmap` 收 `(addr, len)`，只有起点算不出长度。
- `. += _kern_offset;` 之后的所有段才带偏移（`AT(ADDR(...) - _kern_offset)` 表示"执行地址在高处、装载地址在低处"）。两个地址必须分开的理由：引导器只按装载地址把字节写进物理内存，而分页开启后 CPU 按执行地址取指；`AT()` 就是链接期把同一个段的两个地址钉在一起的方式。
- 于是这段 `unpaged` 区域是"分页开启前执行、分页开启后就不再需要"的代码/数据（它的执行地址等于物理地址，靠恒等映射就能访问）。C 在 `kmain` 后段（`system_init` 之后、`bsp_finish_booting` 之前——精确锚点见 §2.4.2 的"还引导段"行）把它 `add_memmap` 归还给通用分配器，且归还发生在 `kernel_may_alloc` 写 0 之前，不撞 §2.4 那组断言。

Rust 侧没有这样一段，这里要分清两件事：**链接布局**上生产内核镜像（两条 UEFI 产线 + U-Boot 接收镜像 + riscv64 直核生产镜像）的所有段都在高半虚拟地址（C 的 `unpaged` 段是唯一"执行地址 = 物理地址"的例外；QEMU 直核测试内核按 VMA=LMA 平铺，见 §4.8，不在这条陈述内）；

**实际执行地址**上，两条 UEFI 链的 boot-shim 建好根、开分页后跳进独立 `kernel.elf` 的高半入口，内核从第一条指令起就在高半（§3.7）；riscv64 的自引导链则起步于物理视图（medany——RISC-V 默认的 PC 相对寻址代码模型，代码按"装在哪就按哪解释地址"的方式编译——的平移视图，见 §4.8），高半地址要等开分页之后才被解引用。

**判据结论**：当前所有生产链接布局（两条 UEFI 链、U-Boot 接收镜像、riscv64 生产直核镜像，均为高半 VMA）与 VMA=LMA 的 QEMU 测试内核都不存在"分页开启前执行、之后可归还"的**内核段**；内联形态已随 T2 退役，不在本条判据范围：判据从正面写：没有一段是"在链接布局上可单独界定、且分页后不再需要"的；`bootface` 代码虽然只在分页前执行，但它是内核镜像的一部分、没有独立链接边界，不能单独归还。因此这个字段对在 Rust 侧没有语义内容——它后来的处置（先写死为 `(0, 0)`、最终整个删除）都从这一句来。

这个字段对在 Rust 侧的处置走过两个阶段，最后停在**删除**：先是把 `(0, 0)` 写死并配一条硬断言（"必须为 0"），字段删除后离开 `KernelInfo`，wire 版本 7→8：本字段对释放 16 字节，同版本删除的 `free_upper_idx` 再释放 8 字节，合计 24 字节，envelope 从 184 缩到 160（该字段的删除论证见本节下文的过度回收风险表；`free_upper_idx` 的删除论证见 §3.9）。为什么"写死 + 断言"还不够、要连字段一起删：`bootstrap_start/len` 表达的其实是**一个布尔事实**（"存在分页开启前专用、之后可回收的段"——Rust 侧恒为否），却占两个整数、两个 builder 参数和 16 字节 wire；所有生产调用点都传 `(PhysBytes(0), 0)`，唯一的"读者"是那条断言自己。更重要的是保留字段就保留了一个误设的接口面——`validate` 的断言只能把非零值拦在 `kmain` 入口，拦不住"合法类型、错误语义"的值进入类型系统。误设的后果是本篇推演过的真实事故来源：

| 若 `bootstrap_len` 被设成 `kern_phys_base.0`（下表以 riscv64 链为例：`kern_phys_base = 0x80200000`） | 那块内存实际存着什么 | 被误分配后先坏在哪 |
|---|---|---|
| `[0, OpenSBI 固件尾)` | SBI 运行时服务（console、timer、IPI）的代码与跳板 | 下一次 SBI 调用（早期控制台输出、定时器）进入被覆盖的地址 |
| DTB 所在区间 | 设备树 blob；boot-shim 用它建内存表、读 `/chosen/bootargs`，并把物理地址作为 `PlatformDescSource` 传给内核 | 内核解析 DTB 时读到被改写的属性，硬件发现失败（归 04 篇） |
| U-Boot 镜像与 `BootFileTable` | 生产链上 `fatload` 预载的文件；表本身是 shim 镜像 `.bss` 的一部分（`build_shim_table` 构建），不是预载物 | 后续从表里再取文件时读到垃圾；表魔数校验失败即中止 |
| boot-shim 自身 | 正在执行的 boot-shim 代码与栈 | 直接取指失败（这一块在误回收发生时可能已经不被引用） |

这张表里的每一行都落在"某块仍被使用的固件/引导代码内存被写掉"，而四者的共同点是：它们都不在 boot-shim 报告的可用内存里，而是被"从 0 开始、长度到内核物理基址"这个区间扫进去的。

这张表同时解释了字段历史上的另一处险情：早期版本曾把 `bootstrap_len` 填成 `kern_phys_base.0`（把"上界地址"当"区间长度"），`add_memmap(0, kern_phys_base)` 会把上表所列的全部固件结构扫进"可回收"———早期 `uefi_helpers` 的 `build_kernel_info` 调用点曾有一条注释（字段删除时随参数一起删除），原文把自己的旧取值记为险情来源：`the previous PhysBytes(0) + len = kern_phys_base.0 was a dangerous over-reclaim that swept up OpenSBI/DTB/U-Boot/boot-shim itself`（x86_64 链上该区间是 `[0, 0x200000)`，即 [`x86_64.ld`](os/kernel-image/x86_64.ld) 的 `KERNEL_PHYS_BASE`）；现仓在 `opensbi_helpers.rs` 的测试模块保留一句"结构上不可能"的回归说明；`os/kernel/src/lib.rs` 的 `kmain` Phase F 段曾在 R14 前仍称 `validate` 要求 `bootstrap_len == 0`，R14 已清理；Phase A 的取证注释仍提到该字段，属历史叙述。那次险情靠人工改值解决；字段删除之后，"提供一个可误设的回收区间"这个接口面整体消失，险情在类型上不可复发。C 侧的归还机制照旧运转（`add_memmap` 对零长请求静默吸收的语义，2.4 已见），本篇不再为它携带任何 Rust 侧对应物。

### 3.6 `HUGE_PAGE_SIZE` 与 `map_huge`：放进基础 trait 还是扩展 trait

C 的建表两个函数都直接用 4MB 大页：`pg_identity` 给每个页目录项置 `I386_VM_BIGPAGE`，`pg_mapkernel` 也置同一位（2.6 已贴）。大页在这套架构上不是可选能力，而是"1024 项 × 4MB = 4GB"这套单级页目录布局的必要条件（限定语是"只建一张页目录就覆盖 4GB"这个布局目标；对自举真正需要覆盖的两段地址，§2.6.1 已说明全铺只是省事手段、不是必须）：单个页目录项直接指向 4MB 物理块、不需要再挂页表页，一级页目录就能覆盖 4GB。

64 位侧情况不同：层数变多，每个架构支持的大页尺寸集合与"怎么表示这是一个大页"都不同。Rust 侧把这部分能力放进一个扩展 trait（`paging_ext.rs` 的 [`trait HugePages`](<os/arch/src/arch/paging_ext.rs:trait HugePages>)）：

```rust
// 代码引用，os/arch/src/arch/paging_ext.rs:trait HugePages。
pub trait HugePages: Paging {
    /// Supported huge page sizes, sorted largest first.
    const HUGE_PAGE_SIZES: &'static [PageSize];

    /// Preferred huge page size for Direct Map.
    const HUGE_PAGE_SIZE: PageSize;

    /// Fallback huge page size when the preferred size is not available.
    const FALLBACK_HUGE_PAGE_SIZE: PageSize;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: PageSize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    /// Whether this architecture/CPU supports the given huge page size.
    fn supports_huge_page(size: PageSize) -> bool;
}
```

这里没有默认实现，也没有第二个能力入口：每个架构必须显式回答自己的页大小能力（x86-64 在 `supports_huge_page` 内查 `CPUID.80000001H:EDX.GBPAGES`，ARM64/RISC-V 按自己的翻译模式回答）。能力事实由 `BootPageSupport` 快照收集，选页由纯规划函数完成，`map_huge` 只执行已经验证过的写入。

四条理由（1）–（4），逐条对应可检查的事实：

（1）`HugePages: Paging` 是**超集**关系：boot 阶段用 `A: BootArch` 一个 bound 就能同时取到 `A::Paging`（`new_from_page`/`enable`）、`A::DirectMap` 的窗口常量与 `A::DmInstaller` 的装叶实现（`arch_boot_impl<A: BootArch>` 就是这么写的；大页入口经 `HugePages` 另取）。运行时只用 `P: Paging` 的消费者不必携带大页负担。
（2）标识方式因架构而异，但标识位不进入 trait。x86-64 靠条目的 PS 位（bit 7）区分大页与"指向下一级表"，`map_huge` 在实现内用局部 `huge_flag` 取它；ARM64 与 RISC-V 没有这一位。把标识位提成"三个架构都要提供、其中两个恒为 0"的 trait 常量，等于把架构差异写进共有接口，所以它留在实现里。
（3）大页是**可选能力**而不是基础能力。x86-64 的首选是 1GB、回退 2MB；把 `map_huge` 放进 `Paging` 会让所有 `Paging` 实现都必须实现它，包括那些不打算支持大页的实现。
（4）`Paging` 的职责面保持窄：`new_from_page`、`from_active_root`、`map`、`unmap`、`query`、`enable` 这些是"页表能做什么"；大页参数是"硬件能提供多大的块"。

被否决的方案：

| 方案 | 否决理由（对应上面哪条） |
|---|---|
| 把 `HugePages` 的常量面（`HUGE_PAGE_SIZES`/`HUGE_PAGE_SIZE`/`FALLBACK_HUGE_PAGE_SIZE`）移入 `Paging` | 违反第 2、3 条：尺寸阶梯是可选能力，放进基础 trait 等于让所有 `Paging` 实现都声明自己支持大页；标识位留在实现内，正因为它不是三架构共有的常量（`map_huge` 方法本身不在这次搬移范围内） |
| 另立一个只服务 boot 的 `PageTableBoot` trait | 与 `Paging::map/query` 重复——同一套硬件操作出现两个 trait，调用方要同时满足两个 bound |
| 不定义 trait，直接把 `map_huge` 写成具体类型的方法 | boot 路径无法用泛型统一三种架构；`arch_boot_impl<A>` 的 `A::Paging` 会退化成三个 `#[cfg]` 分支 |

x86-64 的实现给出这套设计的两个可核对点（`paging.rs` 的 [`impl HugePages for X86_64Paging`](<os/arch/src/x86_64/paging.rs:impl HugePages for X86_64Paging>) 的常量与 `fn map_huge`）；标识位不出现在常量里，而是在 `map_huge` 内取局部：

```rust
// 代码引用，os/arch/src/x86_64/paging.rs:impl HugePages for X86_64Paging 的常量与 map_huge 守卫段。
    const HUGE_PAGE_SIZES: &'static [PageSize] = &[PageSize::GIB_1, PageSize::MIB_2];
    const HUGE_PAGE_SIZE: PageSize = PageSize::GIB_1;
    const FALLBACK_HUGE_PAGE_SIZE: PageSize = PageSize::MIB_2;
...
        let huge_flag = X64PteFlags::PS.bits();
...
        if shift == PDPT_SHIFT {
            unsafe { write_entry(pdpt, i3, (paddr.0 & ADDR_MASK) | pte_flags | huge_flag) };
            return Ok(());
        }

        // Check if PDPT entry is already a 1GB leaf (PRESENT + PS both set).
        // If so, demoting it would corrupt the existing mapping.
        if e3 & X64PteFlags::PRESENT.bits() != 0 && e3 & X64PteFlags::PS.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
```

- `HUGE_PAGE_SIZES = &[1GB, 2MB]`、`HUGE_PAGE_SIZE = 1GB`、回退 2MB：尺寸阶梯是"首选失败往下退"。谁选尺寸不在这里，而在内核的纯规划层（`os/kernel/src/boot_policy.rs`：外层入口 [`plan_boot_pages`](os/kernel/src/boot_policy.rs:plan_boot_pages)，内核映射档位由它内调的 [`plan_kernel_mapping`](os/kernel/src/boot_policy.rs:plan_kernel_mapping) 给出——§3.7 引文里出现的正是外层那个）；`boot_validate_and_prepare` 采集能力事实、校验恒等窗，并要求分配器已注册（未注册即 fail-closed，见 §4.1）：

```rust
// 代码引用，os/kernel/src/boot_policy.rs:fn plan_kernel_mapping 的尺寸选择段（首选不支持或布局不齐才退到回退）。
    let page_size = if preferred.supported() && layout_satisfies(preferred.size(), layout) {
        preferred.size()
    } else if fallback.supported() && layout_satisfies(fallback.size(), layout) {
        fallback.size()
    } else if !fallback.supported() {
        return Err(KernelMapError::FallbackUnsupported(fallback.size()));
    } else {
        return Err(KernelMapError::NoKernelPageSize);
    };
```

  三条件（虚拟基址、物理基址、长度）同时对齐到首选尺寸才用它；首选不受硬件支持时先降级而不是先看对齐。这条阶梯不是风格：`check-layout.sh` 的 L5 断言要求镜像跨距按 2MiB 对齐，正是为了让它至少能落进回退档（断言清单见 §5.2）。
- `AlreadyMapped` 守卫处理的是"目标槽位已经是一个 1GB 大页"的情形。注释给了理由："demoting it would corrupt the existing mapping." 这条守卫也回应本节的问题本身：扩展 trait 承担的不只是尺寸查询，还有"不得降级已有大页"这条非平凡契约——若把 `map_huge` 收进基础 trait，所有 `Paging` 实现都得背上它，哪怕该架构根本没有 1GB 叶。

**去掉 `AlreadyMapped` 守卫会怎样**：直接按 2MB 路径往下走。`e3` 里原本存的是 `paddr | PRESENT | PS`（因为 1GB 大页的 PDPT 条目就是叶条目），代码在"条目已存在"分支里做的是 `(e3 & ADDR_MASK) as *mut u64`——也就是把那个 1GB 物理块的基址当成下级页表指针。接下来 `write_entry(pd, i2, ...)` 写的是 `(e3 & ADDR_MASK) + i2*8`（写入地址来自被误当页表指针的那个块基址，与 `map_huge` 的形参 `paddr` 同名不同物；`paddr` 只作为写入值参与）：这段内存正被 1GB 映射覆盖，可能就是内核自己的数据页。写完之后 1GB 映射还在（条目没变），但它的前 4KB 已经被当成页表改写。守卫的代价是"这次 2MB 映射失败"，去掉守卫的代价是"被映射的那段物理内存头部被写坏"。

### 3.7 控制权交接的三种形态：同镜像内调用、跨镜像跳转与直核自引导 `[ARCH: 内核从内联执行改为独立镜像 + 交接载荷]`

> **术语对账**：本篇用多个不同维度的计数描述同一组启动路径（核心三轴 + 三个更细的次级计法），读者不要混淆——
> **形态**（本节，三种）= 按执行视图分类：同镜像内调用 / 跨镜像跳转 / 直核自引导；其中同镜像内调用是测试/历史形态，当前生产腿只有跨镜像与直核两类；
> **生产腿**（§3.2 定义“一条腿”，§3.4.1 列五条成员）= 按"谁产出了两张内存表"分类：两条 UEFI 链 + U-Boot shim + 两个直核面（riscv64 `bootface` 与 aarch64 `bootface_a64`）；
> **交接路径**（§4.8，四种）= 按"谁执行装载与建根"分类：v1 send / v1 receive / riscv64 自引导 / aarch64 direct-core；
> **产线** = 链的别称，不参与计数；计数一律用生产腿/链。
> **三类链**（定义在本对账，唯一处）= UEFI 一族（两条 UEFI 链）/ U-Boot 一族（U-Boot shim 生产腿 + `test-riscv64-uboot.sh` 的直载测试链）/ 直核面（两个 `bootface`）——按成员展开共 6 条链，其中 5 条是生产腿、1 条（直载测试链）不是；
> **架构 × 链五行**（§4.8 交接表，五条生产腿各一行）与**真机门链五条**（§5.2.3 表五行，按脚本集合）是两个更细的计法。
> 读计数时以本对账为纲；§4.1 的阶段总览表只按两列（UEFI / 非 UEFI）讲阶段，不构成独立计法。
> 正文以“五条生产腿”为主计数；其他计法在需要时引用本对账，不改变正文的规范口径。每套计数只在其声明的用途内使用；跨节比较先换算单位（例如"五条生产腿"与"六条链"差在直载测试链，"三种形态"与"四条交接路径"差在谁执行装载/建根）。**两个"5"成员不同**：架构×链五行含 aarch64 直核面、不含 `test-riscv64-uboot.sh` 直载测试链；真机门链五条含直载测试链、不含 aarch64 直核面（§5.2.3）。

C 里没有"交接"这个问题：内核是唯一的二进制，`pre_init` 返回的指针就是同一个地址空间里的地址，`head.S` 的 `call kmain` 是同一次调用。

（下文反复用到 AArch64 的 TTBR0/TTBR1：它们是两张页表基址寄存器，分别承载低半与高半地址的翻译表；切换地址空间只换 TTBR0，TTBR1 在上下文切换中不被替换。）

Rust 侧的收敛点分两处：直核家族收敛在 `arch_boot_finish`（`arch_boot_impl` 的调用在其体内；三个 cfg 分支的通用实现锚点见 [`fn arch_boot_impl`](<os/kernel/src/lib.rs:fn arch_boot_impl>)），跨镜像三腿的接收侧收敛在 `arch_boot_resume_v1` → `resume_additive`。三条对外入口分别是 `arch_boot`（直核/测试）、`enter_kernel_image_v1`（跨镜像 v1 发送，三条腿：两条 UEFI 链 + riscv64 U-Boot shim；它不在本镜像内收敛，终点是跳独立镜像的入口指令）与 `arch_boot_resume_v1`（接收）：

```rust
// 代码引用（保留签名与首段注释），os/kernel/src/lib.rs:fn arch_boot_finish（**直核家族**——arch_boot 三个 cfg 分支——共用的尾部；跨镜像的发送/接收两端都不经它：发送侧尾部是 `br {entry}` 跳独立镜像，接收侧走 `resume_additive`）。
fn arch_boot_finish<A, H>(kernel_info: &KernelInfo, root_page: PhysBytes) -> !
where
    A: BootArch,
    H: crate::boot::HigherHalf,
{
    // Fail-fast before any page table is built: a malformed handoff must not
    // reach the mapping machinery and only surface later at kmain. The
    // pre-table gate skips the boot_modules non-empty check because QEMU
    // bootstrap test kernels legally carry no init-server image.
    kernel_info.validate_before_tables();
    kernel_info.check_boot_postconditions();
    store_kernel_info(kernel_info);
    let kinfo: &'static KernelInfo = crate::kernel_info()
        .expect("arch_boot: KERNEL_INFO store failed");
    register_panic_diagnostic();
    arch_boot_impl::<A>(kinfo, root_page);
    // ……（省略 SAFETY 注释，见源文件）……
    unsafe { H::jump_to_kmain(kinfo, kinfo.kern_stack_top) }
}
```

x86_64 的 `arch_boot` 只做诊断打印并调用 `arch_boot_finish::<X86_64BootArch, X86_64HigherHalf>`；`arch_boot_impl` 的调用在这个 helper 内，不在 `arch_boot` 的函数体里。

签名里的两个参数就是交接的全部内容：一份借用的启动信息（`&KernelInfo`，入口立即深拷）与一个根页表页的物理地址。返回值是 `!`——它不返回。

跨镜像交接的产线上多一层（x86_64、aarch64 与 riscv64 U-Boot shim 三条 v1 发送腿同走 `lib.rs` 的 [`fn enter_kernel_image_v1`](<os/kernel/src/lib.rs:fn enter_kernel_image_v1>)；TTBR0/TTBR1 是 aarch64 侧的细节）：内核以独立 ELF 的形式存在于高半（aarch64 侧经 TTBR1），boot-shim 先分配一块八页 arena 并把 v1 协议写进去，然后建表、开分页、把运行期字段补进 arena，最后**绝对跳转**过去；内存图来源分两路：两条 UEFI 腿在 `ExitBootServices` 的配对循环里分类最终快照，riscv64 U-Boot shim 没有 EBS，直接从 DTB `/memory` 取表；入口寄存器只带 arena 基址——x86_64 用 `rbx`（镜像 trampoline 转到 `rdi`）、aarch64 用 `x0`、riscv64 U-Boot shim 用 `a0`。

```rust
// 代码引用（保留公共路径与 aarch64 跳转臂；x86/riscv 跳转臂、步骤注释与校验有省略），os/kernel/src/lib.rs:fn enter_kernel_image_v1。
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }
    let support = boot_validate_and_prepare::<BootArchImpl>(
        report.kern_virt_base,
        report.kern_phys_base,
        report.kern_size,
        report.kern_stack_top,
    );
    let layout = DirectMapLayout::<BootArchImpl>::from_arch();
    let kernel_layout = KernelLayout { virtual_base: report.kern_virt_base.0, physical_base: report.kern_phys_base.0, size: report.kern_size };
    let policy = plan_boot_pages::<BootArchImpl>(&support, kernel_layout, &layout)
        .unwrap_or_else(|e| panic!("enter_kernel_image_v1: page-size policy: {e}"));
    build_bootstrap_root_and_enable::<BootArchImpl>(root_page, &policy.kernel());
    let (bump_base, bump_end) = boot_alloc::boot_alloc_region()
        .expect("enter_kernel_image_v1: bump region not registered");
    let bump_next = boot_alloc::boot_alloc_next();
    let arena = unsafe { core::slice::from_raw_parts_mut(arena_pa as *mut u8, ARENA_CAP) };
    patch_runtime_fields_in(
        arena,
        RuntimeFields { root_page: root_page.0, bump_base, bump_next, bump_end },
    );
    let entry = core::hint::black_box(report.kern_virt_base.0);
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "isb",
            "br {entry}",
            in("x0") arena_pa,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
```

三处具体事实：

- 交接载荷带 `bump_next`。原因在 §4.7 与 `BootAlloc::resume` 的文档里：两个镜像各链接一份 `BootAlloc`，接收方若从区间基址重新发页，会把 boot-shim 已经用作页表页的页再发一次。两条 UEFI 腿的 arena 由 UEFI `AllocatePages` 单独分配、不占 bump 池，因此没有"载荷页"要跳过；U-Boot shim 腿的 arena 由 `bump_alloc_pages` 从同一物理区间另行分配（它与 `BootAlloc` 是两个独立游标，见 §4.2.1 的"勿混"说明），排在模块落点之后；它排在 1024 页的 `boot_alloc_region()` 区间之后（起点不低于 `bump_end`），因此**不在**该区间的整段扣除内；它在 `startup_objects` 里有记录；扣除表没有单独排除它，而是按生命周期合同处理：解码是 arena 的最后一次读，VM 自由页在那之后才发布，因此这些帧可以重新发给 VM（§4.7 的 poison 测试钉住这条时序）。
- `in("x0") arena_pa`：arena 基址绑在固定的第一参数寄存器上，而跳转目标放在任意寄存器。代码注释给出了理由——若在 asm 块里写隐式 `mov x0, ...`，寄存器分配器可能把 `entry` 也分配到 `x0`，两条约束顺序不定。
- 接收侧先把七个分区直接解码进 `store_kernel_info` 的七个 `.bss` 落地区（随后 `store_kernel_info` 因源与目标同址成为 no-op），再只做 additive 阶段（`arch_boot_resume_v1` 之后三架构都先经 `resume_after_v1_decode`；aarch64 的壳再转 [`fn arch_boot_resume_high_half`](<os/kernel/src/lib.rs:fn arch_boot_resume_high_half>)（带两行路标），x86_64/riscv64 的壳直接进 `resume_additive`）：

```rust
// 代码引用（首行与结尾的设计史注记略去——结尾那段是源文件里的 `///` 设计史注记，约五到六行；末行在源文件里与后续内容同行，此处截断至句号），
// os/kernel/src/lib.rs:fn resume_additive 的文档注释（调用序合并后，加性序列的
// 设计注记挂在通用体上；arch_boot_resume_high_half 退化为带路标的委托壳）。
/// The boot-shim already built the bootstrap page-table root and enabled
/// paging; that root is *currently loaded* in TTBR0/TTBR1 and maps *this* code.
/// This entry runs only the boot phases that are purely *additive* on the live
/// root — validate/register allocators, land the KernelInfo into this image's
/// `.bss`, establish the direct map, then enter `kmain`. It must NOT rebuild the
/// tables: calling [`build_bootstrap_root_and_enable`] here would
/// `new_from_page(root_page)` and zero-fill the *active* root (including the L0
/// slot holding this very function and the MMIO window), so the next
/// instruction fetch would fault.
```

riscv64 上是第三种形态的出生地：内核镜像自己就是引导体——在没有 U-Boot 与文件系统的 `-kernel` 场景里，入口是 [`main.rs`](os/kernel-image/src/main.rs) 按架构分道的 `rust_image_main`，riscv64 的实现是 [`fn bootface`](<os/kernel-image/src/bootface.rs:fn bootface>)（aarch64 的对应面是 `bootface_a64.rs` 的 [`fn bootface_a64`](<os/kernel-image/src/bootface_a64.rs:fn bootface_a64>)，只在该架构构建时编译），从设备树解析内存、按 `BootFileTable` 取模块——模块来源只有表这一条，表的**地址**只有 DTB `/chosen` 的 `opensbi,boot-file-table` 属性一条通道（仓库内六个脚本——`test-atf-riscv64`、`test-cmd-smoke-riscv64`、`test-kernel-image-riscv64-bootface`、`test-riscv64-boot-full`、`test-smp-aps-aarch64`、`test-smp-aps-riscv64`——都用 `fdtput` 注入表址）。曾经的备用 `a2` 参数通道在 D01 裁决项 3 中删除（仓内从未有生产者：QEMU/OpenSBI 只约定 `a0`/`a1`）；表址先过 `in_memmap_range` 再过魔数，失败即 halt。细节在 `bootface.rs` 的入口段，本篇不展开。

U-Boot shim 腿自建表、不经过此通道——它把表与模块序列化进 arena，交给接收镜像的 `arch_boot_resume_v1`；调 `arch_boot` 的是两个直核面。U-Boot shim 路径落地后也补齐了第二种形态的完整链：`riscv64-uboot-shim`（物理链接，VMA=LMA，自带 bootstrap 栈）吃下 U-Boot `bootelf -p` 的 `(extra_argc, extra_argv)` 参数通道，把内核 ELF 与 12 个引导模块准备成 `BootHandoffV1` arena，DTB 以 `PlatformDescSource` 指针随信封交付，交给配套特性 `fw-riscv64-uboot` 构建的 `kernel-image` 接收镜像走 `arch_boot_resume_v1`——接收侧与 aarch64 同构，复用同一个加性恢复序列，不重建根表。riscv64 的 `jump_to_kmain` 因此带一个执行位置探针：直核面仍在低地址恒等窗口里执行，medany 代码模型的 PC 相对 `la` 解析出的是装载地址，要加上镜像的链接↔装载偏移；接收侧已被 shim 跳到高半链接地址，`la` 解析的已是真值，偏移不得再加（两节分工：§2.6.4 讲 C 侧的 TLB 窗口，本节讲 Rust 侧为何禁止重建活动根；不合并）——同一段 asm 用 `auipc` 读当前 PC 与 `kern_virt_base` 比较后分道。五条内存来源（两条 UEFI 链的分类结果、U-Boot shim、两个直核面）交付的形状统一为两张表，收敛点与各腿的产出方式见 §3.4；引导形态层本身不合并。

**两个替代做法各会撞什么**（两处）：

其一，aarch64 退回内联执行。内联时内核代码住在 PE 镜像的运行物理地址上（低半、`VA = PA`、在 TTBR0 覆盖范围内）。第一次 `switch_address_space` 会换 TTBR0 到进程页表并发 `tlbi vmalle1is`，内核自身的取指映射随之消失——现场证据（`elr`/`vbar` 钉在同一个低地址上）外包给 [`rewrite-notes/01-stage-kernel/01-code-design.md`](01-code-design.md)；本篇只登记这条指向，不自称独立可核。跨镜像形态把这个问题的根去掉：内核代码链接在高半（TTBR1），而 TTBR1 在上下文切换中不被替换。

其二，接收侧按"干净启动"重建 bootstrap 根（调 `Paging::new_from_page`）。该函数的第一件事是把 512 个根条目清零（`write_bytes(ptr, 0, 512)`），而此时这个根**正在被使用**——承载当前函数的那条指令所在的 L0 槽就在其中。清零本身不会立刻作废已缓存的 TLB 项——三个架构都不因改写页表而自动清 TLB——故障落在下一个**冷**取指上（TLB 回填或切换后的 kmain 入口取指）：之前靠 TLB 残留还能跑几条指令，这段"还能跑"的窗口让故障点与根因隔得更远，栈上多半连一条 panic 消息都来不及打印；"resume 面只做 additive 阶段"这条约束就是从这里来的。

**直核自引导在真板上的位置** `[ARCH: FW_PAYLOAD 直核形态（Minix3 无对应）]`。`FW_PAYLOAD` 直核形态的规范背景与真板意义见 §3.1 例外条款段（一句话版：固件打包下一级镜像直接跳 S 模式目标，QEMU `-kernel` 模拟同一时序）。三架构的落位并不对称：

| 架构 | 直核形态的位置 | 原因 |
|---|---|---|
| x86_64 | 无对应形态 | 平台规范强制 BIOS/UEFI 作为唯一引导面，没有"固件直接跳任意 payload"的规范形态 |
| aarch64 | 覆盖测试为主 | QEMU `-kernel` 是主力验证通道；真板启动一般仍经 UEFI/U-Boot |
| riscv64 | 测试与真板嵌入式并存 | `FW_PAYLOAD` 是规范形态，真板极简装机直接使用；`-kernel` 是它在 QEMU 的对应物 |

所以直核自引导是**辅助启动路径**，不是三架构的主线：主线是"独立 boot-shim + 独立内核镜像 + 交接载荷"（§3.1 的分工表；两条 UEFI 产线已落地，riscv64 的 UEFI-shim 路径见 §3.1.1 的接线状态表）。这张矩阵同时解释了 `bootface`/`bootface_a64` 为什么保留而不作主产线——它们服务的是特定嵌入式形态与测试通道。

### 3.8 引导期页表页分配器的换手

交接形态（3.7）定下来之后，这一节看引导期页表页分配器怎么在这两个镜像之间换手。C 的**页表页**来源是 `alloc_pagetable` 的 6 页静态池，用完 panic（§2.6.2）；`pg_alloc_page` 从可用内存记录的最高条目尾部取页，服务的是 4KB 被映射数据页，不是页表页（§2.4.1/§2.6.2）。页表分配策略与"页表操作"写在同一份文件里，页表代码直接调用分配函数。

Rust 侧把**页表页**分配抽出来（`pt_alloc.rs` 的 [`fn register`](<os/arch/src/arch/pt_alloc.rs:fn register>)、`:fn alloc_pt_page`）；4KB 数据页的运行期分配归 VM 物理分配器，不在这个槽里：`Paging`/`HugePages` 的实现只调用一个没有参数的函数指针槽，谁来提供页由"注册"决定。

```rust
// 代码引用（省略号处为模块级说明；块内中文注释为译述，英文 SAFETY 注释有节略，原文见源码），os/arch/src/arch/pt_alloc.rs。
type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;
...
static PT_ALLOC: PtAllocSlot = PtAllocSlot(UnsafeCell::new(uninit_alloc));
static PT_REGISTERED: AtomicBool = AtomicBool::new(false);
// 写者侧抢写权：串行化写指针本身；PT_REGISTERED 只是读者侧的发布标志。
static PT_CLAIMED: AtomicBool = AtomicBool::new(false);
pub fn register(alloc_fn: fn() -> Result<(PhysBytes, VirBytes), PageTableError>) {
    // Double registration within one domain is a boot bug — two allocators
    // would fight over the singleton. (The kernel guards its own call with
    // `is_registered()` so a test kernel may register first; the VM server
    // registers in its own address space.)
    // ...（省略：两种构建档下的 write-once 说明——debug 中止、release 拒绝二次写入）...
    let claimed = PT_CLAIMED.swap(true, Ordering::AcqRel);
    debug_assert!(!claimed, "pt_alloc::register called twice");
    if claimed {
        return;
    }
    // SAFETY: the PT_CLAIMED swap admits exactly one writer; the pointer
    // write happens before PT_REGISTERED is published.
    unsafe {
        core::ptr::write(PT_ALLOC.0.get(), alloc_fn);
    }
    // Release: publish the fn pointer write to any thread that Acquire-loads
    // `is_registered() == true` before calling `alloc_pt_page()`.
    PT_REGISTERED.store(true, Ordering::Release);
}
...
#[inline]
pub fn alloc_pt_page() -> Result<(PhysBytes, VirBytes), PageTableError> {
    // ...（省略：register() 必须先于任何 map 操作的模块不变量与 SAFETY 说明）...
    assert!(is_registered(), "pt_alloc::alloc_pt_page called before register");
    // Acquire: pairs with `register()`'s Release store — guarantees the
    // fn pointer write is visible to this thread once registration is
    // observed (write-once-then-read-only; see module docs).
    let _ = PT_REGISTERED.load(Ordering::Acquire);
    let alloc_fn = unsafe { core::ptr::read(PT_ALLOC.0.get()) };
    alloc_fn()
}
```

boot 阶段的实现是一个 bump 分配器（`boot_alloc.rs` 的 [`BootAlloc::alloc`](os/kernel/src/boot_alloc.rs:BootAlloc::alloc)）：

```rust
// 代码引用（块内中文注释为译述；英文原注释见源码），os/kernel/src/boot_alloc.rs:BootAlloc::alloc。
    pub fn alloc(&self) -> Result<(PhysBytes, VirBytes), PageTableError> {
        // 从本实例的原子状态重建共享的 BumpRegion 值，跑公共算法，再把游标写回。
        let mut region = BumpRegion::resumed(
            self.base.load(Ordering::Relaxed),
            self.next.load(Ordering::Relaxed),
            self.end.load(Ordering::Relaxed),
        );
        let Some(pa) = region.alloc_pages(1) else {
            return Err(PageTableError::AllocationFailed);
        };
        self.next.store(region.next(), Ordering::Relaxed);
        // Zero-fill the page. In test mode, skip the write (no real memory).
        #[cfg(not(test))]
        // SAFETY: pa is a valid, 4KB-aligned physical address returned by the
        // bump allocator. Under identity mapping (boot stage), VA=PA, so the
        // pointer is valid. The page is not aliased by any other live reference.
        unsafe { core::ptr::write_bytes(pa as *mut u64, 0, 512) };
        Ok((PhysBytes(pa), VirBytes(pa)))
    }
```

换手由一次初始化与三次注册组成（下表三行：第 1 行是初始化、不注册；第 2 行里两个二进制各注册一次，共两次；第 3 行一次），各有锚点：

| 时刻 | 谁写这个槽 | 代码 | 依据 |
|---|---|---|---|
| shim 进程内、进入内核镜像之前 | 初始化 bump 池（**不写注册槽**） | `boot_alloc::init_boot_pt_alloc(bump_base, bump_end);`（U-Boot 腿写成 `result.bump.start()`/`result.bump.end()`） | [`uefi_helpers.rs`](os/boot-shim/src/uefi_helpers.rs)、[`uboot_main.rs`](os/boot-shim/src/uboot_main.rs) |
| shim 进程内 Step 0 与**接收镜像**的 `resume_additive`（同一 helper `boot_validate_and_prepare`） | 只有"未注册"时才写——每个二进制各有一份 `PT_REGISTERED` 静态，接收镜像的这份文件在它自己的 `.bss` 里首次注册。shim 侧不直接调 `register`，而是在 `boot_validate_and_prepare` 内经 `is_registered()` 守卫间接注册；两次注册不冲突：UEFI 腿写的是 boot-shim 的那份、接收镜像写的是自己的那份，互相独立 | `if !pt_alloc::is_registered() { pt_alloc::register(boot_alloc::boot_pt_alloc); }` | [`boot_validate_and_prepare`](os/kernel/src/lib.rs:fn boot_validate_and_prepare) |
| VM 初始化 | VM 在自己的地址空间里注册（并注册归还钩子） | `if !minix_arch::pt_alloc::is_registered() { minix_arch::pt_alloc::register(crate::alloc_page::vm_pt_alloc); }` 与 `if !minix_arch::pt_alloc::is_free_registered() { minix_arch::pt_alloc::register_free(crate::alloc_page::vm_pt_free); }` | [`vm_server.rs`](os/servers/vm/src/vm_server.rs) |

内核侧与 VM 侧的写者都用 `is_registered()`（或 `is_free_registered()`）先自检；shim 侧只初始化 bump 池，不直接调用 `register`。这个槽是“写一次”的，重写是 bug，因此没有“后注册者覆盖前者”的语义。VM 侧的分配函数把页表页从 VM 的物理内存分配器取出，归还函数把它还回同一个池（C 的对应物是 `pt_free` 把页表页还回 `alloc_mem` 池）；两者成对，未注册归还钩子时销毁路径只清零根、中间页表页泄漏。

扣除那一侧不在 `pt_alloc` 里，而在构造 VM 交接记录时（`VmBootAllocator` 是 VM 侧的启动帧分配器：从每个池区间末端向下消耗，方向与从起点向上的 `BootAlloc` 相反）（`vm_handoff.rs` 的 [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) 调用的 `:fn classify`）；引导期建表之后的 Direct Map 覆盖建立则在 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs)：

```rust
// 节选（略去行号引用；kernel image 一行加了 A+D 之后的归属说明，模块行加了按腿归属的说明，C 对应关系一行按 §2.4.2 的指针改述，见上文），os/kernel/src/vm_handoff.rs 的扣除区间表头与 bump 行。
//! | Range | Source |
//! |-------|--------|
//! | kernel image | `KernelInfo.kern_phys_base/kern_size`（UEFI 腿在 A+D 之后已预订为 `LOADER_DATA`，本行对 free list 退化为空操作；自引导腿与 U-Boot shim 腿仍是实际扣除） |
//! | VM self root page | `arch_boot` parameter (= handoff `root_paddr`) |
//! | boot bump region | `boot_alloc::boot_alloc_region()` (whole range —
//!   consumed pages and remainder are both kernel-reserved) |
//! | bootstrap-used frames | per-pool-region used suffixes of
//!   `VmBootAllocator` (walk order: descending from each region's end) |
//! | reserved module blobs (deduction record) | `KernelInfo.boot_modules`
//!   — **every** blob including VM's own on the legs whose blobs live in the
//!   free list (self-boot / U-Boot); UEFI blobs are `LOADER_DATA`, so this row
//!   is a no-op there. On the self-boot / U-Boot legs VM's segment frames come
//!   from this free list before VM starts, so reclaiming its blob corrupts VM
//!   before entry. |
//!
//! Note the two module lists are different on purpose: the deduction record
//! keeps every blob, while the handoff module list (`VmBootHandoff.modules`,
//! built by [`build_vm_handoff`]) excludes VM — C: `kinfo.module_list[]`.
```

```rust
// 代码引用（节选，实际调用段），os/kernel/src/vm_handoff.rs:fn classify。
    // 五个扣除来源统一交给 for_each_live_bootstrap；bump 整段的 push 在
    // 该函数的 `if let Some((base, end)) = boot_bump` 分支里。
    for_each_live_bootstrap(
        kernel_info.kern_phys_base,
        kernel_info.kern_size,
        kernel_info.boot_modules,
        Some(root_paddr),
        boot_alloc_region(),
        &suffixes[..suffix_n],
        |base, len| record.push(base, len),
    );
```

表头那句注释给出口径："consumed pages and remainder are both kernel-reserved"——扣的是**整段**，不是已用部分。理由在本节末的反向推演里。表里还有一行与 C 的差异需要说明：C 在 VM 模块的字节被复制进地址空间之后把它回收（就是 2.4.2 表里的"还 VM 镜像"那一行），而**实现**把**所有**启动模块的字节区间都算作内核保留。表头把两份清单分开陈述：扣除记录含 VM 自己，交接模块清单（`VmBootHandoff.modules`）才按 C 的 `kinfo.module_list[]` 排除 VM。理由是启动顺序里的一个 Rust 侧前提：**模块字节留在 blob 原地，VM 启动那一刻才被搬进自己的地址空间**——C 侧不同，C 的注释写明回收发生在 "just copied into existence"（复制完成后），所以 C 能在复制后归还而后续装载只伤到无人再读的 blob 原件。Rust 侧复制被推迟到 VM 启动，清单就必须从任何搬运发生之前就把全部模块 blob（含 VM 自己的）划出：内核的 boot-exec——`kmain` 的引导执行阶段，逐个 `exec` 启动镜像里的服务进程——在 VM 服务起来之前只装载 VM 一个服务器（`EntrySpec::loaded`）；其余启动服务是 `EntrySpec::DEFERRED`，由 RS 运行期 EXEC 装载——它们的段帧同样取自这份自由清单。若按 C 的时点归还 VM 的 blob，后续装载与 EXEC 的帧分配可能盖掉尚待搬运的 VM 镜像字节。

**改成按"已用部分"扣除会怎样**（用 `boot_alloc_used_bytes()` 而不是 `boot_alloc_region()`）。设 bump 区间是 `[0x1E00000, 0x2200000)`（1024 页 = `0x400000` 字节，与 `prepare_boot(storage, 1024)` 的预算同量级），boot 阶段用掉前 100 页（`100 × 0x1000 = 0x64000` 字节），于是游标停在 `0x1E00000 + 0x64000 = 0x1E64000`，而 VM 的账上会剩下 `[0x1E64000, 0x2200000)` 可用。下一次 `map_huge` 分配中间页表页时 `BumpRegion::alloc_pages(1)` 从 `0x1E64000` 切走一页并把它写零（`write_bytes(pa as *mut u64, 0, 512)`）——而这一页刚被交给 VM 的物理内存管理器。若 VM 已经把它分配给某个进程页表，写零就把那张表的一个条目变成 0；切换 `CR3` 之后，被这条目覆盖的虚拟地址取指即页错误。现场只能看到一个"凭空消失"的映射。

### 3.9 `free_upper_idx`：从字段到报告位（不再跨 handoff 携带；字段删除）

页表页由谁发说完，剩下的是页目录槽位的账——C 的 `freepde_start` 是 §2.6.3 那条借槽链的起点：`memory_init` 把它自增两次填进 `freepdes[]`，`createpde` 用这两个槽位在当前页表里开临时窗口，`virtual_copy_f` 借窗口做跨地址空间拷贝。这套机制的三个前提（完整陈述见 §2.6.3：当前页表里有连续空槽、窗口只在当前 `CR3` 下有效、借出者在切换前不再引用该窗口；VM 改页表时由 `VMCTL_CLEARMAPCACHE` → `mem_clear_mapcache()` 显式清）在 64 位侧逐条失效：① 内核不再借当前页表的槽位，Direct Map 窗口是固定虚拟基址到物理地址的线性映射，不需要“空槽”；② Direct Map 挂在自举根/内核根上，而内核根不随进程切换替换，窗口不依赖“当前 CR3”；③ 没有借出动作，就没有归还义务——用户空间拷贝由 VM 在自己的地址空间里完成。

64 位侧跨地址空间访问换了载体：内核有 Direct Map 窗口（一段固定虚拟基址到物理地址的线性映射，由 `arch_boot_impl` 的 Step 4 建立），加上软件页表走查（[`pte_walk.rs`](os/kernel/src/pte_walk.rs) 一类的实现）就能在**不切页表、不借槽位**的前提下读到任意进程的页表项；用户空间的拷贝则由 VM 服务在自己的地址空间里处理。于是"借用空闲槽位"的三个前提在 64 位侧都不再是必要结构。

这个值先是被保留成 `Option<usize>` 跨 handoff 携带（`None` = boot-shim 未计算），报告面用 `unwrap_or(0)` 抹平。那一步的教训值得留一句：`Option` 能区分"没有值"（`None`）与"某个值"（`Some(0)`——"0 号槽位空闲"，会被下游当成有效索引），却区分不出"算对的槽号"与"算错一位的槽号"——在没有真实计算者时随便填一个数，只会把后者伪装成前者。

字段删除把权衡推到底：既然所有生产者都写 `None`、唯一消费者把 `Option` 归零，"跨 handoff 携带"就没有携带任何信息——字段从 `KernelInfo` 删除（wire 版本 7→8 的完整字节构成见 §3.5）。报告面保留 C 的布局位：[`misc.rs`](os/kernel/src/misc.rs) 的 `getinfo_kinfo` 固定填 `0`（C `KinfoStruct.freepde_start` 字段不动 48 字节 wire；GET_KINFO 载荷的收窄演进见 §3.4 的 `[ARCH]` 标注）。64 位侧报告 0 是诚实的——没有借槽机制，"第一个空闲槽位"这个概念本身不存在；C 侧那套机制里偏移一位的后果（改写内核映射、下一次访问页错误或静默读错）见 §2.6。

对照表：

| 维度 | C 的槽位机制 | Rust 的窗口机制 |
|---|---|---|
| 借用者 | `createpde`（跨地址空间拷贝） | Direct Map 窗口 + 软件页表走查 |
| 借用范围 | 当前页表的 4MB 窗口（一个页目录项） | 内核固定映射窗口（整个物理地址空间的可达部分） |
| 归还时机 | 下一次切换地址空间时自然失效 | 不需要（窗口是常驻映射） |
| 失效条件 | 借来的条目只描述对方的一个 4MB 块 | 窗口覆盖范围由 DM 建立时的资格过滤决定 |

**在 64 位侧照搬"借用空闲 PML4 槽位"会怎样**（PML4：x86-64 四级页表的顶层——页映射表第四级——每进程一个 512 项的顶级目录，上文 C 4MB 单级页目录在它这里对应的是这个角色的放大版）。每进程页表的高半槽位是从 bootstrap 根继承的（`X86_64Paging::inherit_supervisor_half` 把源根的 PML4 的 256 至 511 号槽位（即半开区间 256..512，共 256 项）原样复制过来）。被借用的槽位在 `switch_address_space` 之后属于另一个地址空间：借出去的映射在下一次切换时静默失效，不会报错。症状是跨空间拷贝偶发读到"上一张页表留下的地址"——一个只在特定调度顺序下出现的错误，而不是一次可复现的崩溃。

### 3.10 从 Ch2 到 Ch3 的选读对照（不是映射索引）

> 本节是"事实→设计"的过渡表，不是 C→Rust 的权威映射；权威索引在 §7.5。三张表（本节、§4.9、§7.5）冲突时：C 侧符号落点以 §7.5 为准；Rust 侧符号以本文定义节与源码为准（§7.5 只覆盖 C→Rust 索引），与 §4.9 的声明相同。

| Ch2 的 C 事实 | Ch3 的设计回应 | 一句话差异 |
|---|---|---|
| 引导器代劳加载并把结果填进结构体（2.1/2.2） | 3.1 固件接口换代 + 3.2 引导入口/`FileLoader` | 加载工作从引导器转移到 boot-shim，固件差异收进一条入口函数与一个取件 trait |
| 全局 `kinfo` + 指针传递（2.3） | 3.4 按值 `KernelInfo` + 深拷落地区 | 地址不能跨镜像，值可以 |
| `mbi.mmap_*` 一份内存图，逐条 `add_memmap`（2.3/2.4） | 3.4 两张表（可用 / 保留）+ 两个构造入口 | 一份内存图按语义拆成两份；来源记录进 `KernelInfo`/source 分区，不参与地址推导 |
| `unpaged` 段可回收（§2.3/§2.4.2） | 3.5 字段删除（先"必须为 0"，后整体退场） | 没有分页前执行的内核段，就没有要归还的区间——布尔事实不占两个字段 |
| 4MB 大页是两个建表函数的实现细节（2.6） | 3.6 `HugePages: Paging` 扩展 trait | 大页从"实现细节"升级为"可查询的硬件能力" |
| 单二进制内的 `pre_init → kmain` 调用（2.2） | 3.7 交接形态（同镜像内调用 / 跨镜像跳转 / 直核自引导） | 交接面从一个指针变成一个函数签名 |
| 静态池 6 页 + 从记录尾部取页（§2.4/§2.6） | 3.8 注册-替换的分配器槽 | 分配策略与页表代码解耦，boot 与 VM 各注册一份 |
| `freepde_start` → 借槽位做临时窗口（2.6） | 3.9 机制换 Direct Map + 走查；字段先改型 `Option`、字段删除时整体退场（报告位固定填 0） | 借用会失效，窗口不会——连携带都省了 |

**换一种读法会漏掉什么**：若把这些设计当作互不相关的偏好来读，3.5（`bootstrap` 区间语义重定义）的成立前提会被漏掉——"内核镜像里没有一段'分页开启前专用、之后可归还'的段"，因为不再有这样的段，所以没有要归还的区间。3.6（大页能力放进扩展 trait）的前提是另一条：大页是**按架构可选的**硬件能力（64 位侧各架构的档位集合不同），接口必须能查询能力而不是写死尺寸——它与 3.5 无共同前提，硬安一个只会让读者去 §3.6 找不存在的论证。把各自的前提说出来，读者才能判断哪些设计会随执行模型一起变，哪些只是当前的实现选择。

---

## 4. 实现详解

本章把第 3 章的设计决策落到代码：4.1–4.3 走一遍入口与两条固件路径的调用链（含共用装载函数），4.4–4.5 看 `KernelInfo` 的两个构造入口、校验与七个 `.bss` 落地区，4.6–4.7 是自举根建立/开分页与分配器换手的落地，4.8 是跨镜像交接与直核自引导，4.9 收成 C 函数对照表。各节标题后的 `→` 指回它所实现的设计决策。

### 4.1 入口与调用链（→3.1、3.2、3.8）

UEFI 入口是固件加载 `.efi` 后调用的那个函数。它只做一件事：调用 `prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)`，把后续工作交给这个函数。后者内部再分三段：准备（读内核 ELF 并预订其 LMA 跨度 → 分配根页表页与 bump 区 → 拷贝段与装载模块 → 平台源与参数 → 分配 arena 并序列化非内存图载荷）、退出固件服务（配对循环里分类最终内存图并调 `ExitBootServices`）、交接（解码/提交长度、构造报告并复核、激活自举根、补运行期字段、跳独立镜像）。入口本身不返回。

本章的 Step 编号在这里立住（§4.6 与 §6.1 共用）：Step 0 校验与分配器注册检查（`boot_alloc_region()` 为 `None` 即停机；R18 之前这里有一段从 memmap 切 256 页的兜底，已删除）；Step 1 恒等映射；Step 2 内核高半映射（2b 高半设备别名、2c 设备窗口页）；Step 3 开分页（`enable`）；Step 4 DM 覆盖（本篇不展开，边界见 §1.4）。

先给两条固件路径一张总览图——本章后面各节再逐段展开，这张表只负责导航：

| 阶段 | UEFI 路径 | 非 UEFI 路径（自引导/直核面 + U-Boot shim；按“谁建根”归类，跨镜像细节见 §4.8） |
|---|---|---|
| 谁在跑引导面 | 固件直接调 boot-shim 的 `.efi` 入口（[`fn main`](<os/boot-shim/src/main.rs:fn main>)） | 内核镜像自带的引导面（[`main.rs`](os/kernel-image/src/main.rs) 的 `rust_image_main` → `bootface`/`bootface_a64`，复用 boot-shim 库的取件车道） |
| 读内核与模块 | boot-shim 自己读：`SimpleFileSystem`（固件提供的文件系统协议）加 12 项 `MODULE_NAMES`（§4.3） | `-kernel` 直核：从 DTB 解内存、从表/DTB 通道取模块（§4.8）；U-Boot shim 腿由 shim 自己的 `BootFileTable` 取件（§4.8） |
| 内存与参数准备 | 两条 UEFI 产线共用 `prepare_and_enter_kernel_image`：分配八页 arena，先写信封与非内存图载荷，再在配对循环里把分类结果同时写进 wire 与 shim 自己的 `BootStorage`（§4.2、§4.4） | 直核面共用 DTB 解析实现（`DtbMemoryMap`）；U-Boot shim 走 v1 序列化（§4.8）；UEFI 链由配对循环分类——三条来源都进同一次分类，只有 `memmap`/`reserved_regions` 取值不同（`param_buf` 由各路径参数源经同一 normalizer 产出；§4.4） |
| 退出固件服务 | 两条 UEFI 产线都在配对循环里用 raw `ExitBootServices`，分类与退出用同一个 MapKey | 无此步骤（引导器早已把控制权交出去） |
| 进入内核 | 两条 UEFI 产线：建根、补运行期字段、跳独立镜像（x86_64 经 `rbx`、aarch64 经 `x0` 传 arena），接收端 `arch_boot_resume_v1`（§4.6、§4.7、§4.8） | 直核面：镜像自身建表并进 `arch_boot`；U-Boot shim：独立 shim 建根，接收镜像进 `arch_boot_resume_v1`（§4.8） |

（注意：上表两列不是同一个入口的两个实现；U-Boot 腿走 `prepare_boot`，UEFI 走独立入口，见 §3.2。）三类链（UEFI 一族、U-Boot 一族、直核面——计数维度以 §3.7 术语对账为准）在这一章里各走各的阶段，收敛点按腿分两个：直核家族收敛在 `arch_boot_finish`，跨镜像三腿经 `arch_boot_resume_v1` 汇入 `resume_additive`（各架构委托壳见 §4.8）——完整交代见 §4.8。

```rust
// 节选（保留入口骨架；注释有删节与改述），os/boot-shim/src/main.rs:fn main。
#[entry]
fn main() -> Status {
    // 1024 pages (4 MiB): … the handoff arena's fresh chain all draw
    // lower-level page-table pages from this bump pool via boot_pt_alloc.
    boot_shim::uefi_helpers::prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)
}
```

逐行解码：

- `main.rs` 的 [`fn main`](<os/boot-shim/src/main.rs:fn main>) 传给 `prepare_and_enter_kernel_image` 的 `boot_shim::BOOT_PT_BUMP_PAGES` 是**页表页预算**（常量值 1024，定义在 `lib.rs`；1024 页 = 4MiB；两个直核面另用 `bootface` 的 `PT_BUMP_PAGES = 256`）。第一段注释把这份预算的构成列了出来：恒等映射、内核高地址映射、Direct Map 覆盖建立、引导身份窗口的逐页拆解（把引导期铺的恒等映射逐页撤掉、给 Direct Map 让位）、以及交接 arena 之后的页表链条——它们都从同一个 bump 池里取页。预算是"上限"而不是"必须用完"：`BootAlloc` 只在分配时推进游标。**未用部分不退回**——VM 交接记录按 `boot_alloc_region()` 把整段扣除（见 4.7 的扣除表），用掉的与剩下的都算内核保留。（UEFI 路径下 bump 区本就不在 conventional 列表里，这条扣除对 free list 是空操作；扣整段防的是自引导等路径把 bump 区记进清单的情形。映射责任移交 VM 之后，`vmctl_set_addr_space` 不再向这个池取 kerninfo 注入页，预算只剩启动期本身的消耗。）
- 两条 UEFI 产线的入口是同一个 `prepare_and_enter_kernel_image`：arena 分配、载荷序列化、配对循环都在 EBS 之前，EBS 之后依次解码/复核、构造 `BootReport`、建根、补字段与跳转（发送侧不构造 `KernelInfo`；`KernelInfo` 由接收镜像解码后形成）。分界线之后的代码不再出现任何 `uefi::` 调用；路标走裸串口。
- 进入内核的分道只剩接收端的一处：x86_64 的 `rust_image_main` 从 `rbx` 取 arena 基址，aarch64 从 `x0` 取；两者都调用 `arch_boot_resume_v1`，再从既有的自举根上做增量阶段（`rust_image_main` 的分道臂代码——含 riscv64 的两种交付形态——逐臂引在 §4.8，本节只讲 boot-shim 侧的入口链）。内核不再有"链进 PE 低地址内联执行"的生产路径，VM 也就不再需要在进程页表里复现低地址窗口（`reserved_regions` 的恒等重放半随恒等窗口退场，DM 候选半保留，见 §3.4）。

**若把页表分配器的注册挪到进入内核之后会怎样**。先锚定视角：这里的"分配器"是**接收镜像自己链接的那份 `BootAlloc` 副本**（§3.4：两个镜像各链接一份）——shim 进程内 `init_boot_pt_alloc` 初始化过的是另一份，与本推演无关；接收副本在 resume 之前 `boot_alloc_region()` 为 `None`。在此视角下：`prepare_and_enter_kernel_image` 开头那次 `init_boot_pt_alloc` 照常执行，但 `register` 实际发生在 `boot_validate_and_prepare`（由 `enter_kernel_image_v1` 调用，带 `is_registered()` 守卫），把它挪到 `enter_kernel_image_v1` 的跳转之后（接收侧即 `arch_boot_resume_v1` 的增量阶段之后；`arch_boot` 是直核/测试入口，不是 UEFI 链的接收点）。`boot_validate_and_prepare` 在 `boot_alloc_region()` 为 `None` 时**直接停机**（本轮删除了该兜底）：从 `memmap` 里挑第一个够大的区间来兜底，等于把内核镜像、已装载模块或 arena 所在的位置当成空闲起点，把活着的物理页再发一次。没有这条兜底路径之后，挪后注册的后果不是"换一个池继续跑"，而是在第一个断言处留下 `arch_boot: boot allocator not registered` 并停机。这里要分开两个对象：`require_boot_region` 看的是 `boot_alloc_region()`（bump 区间），`alloc_pt_page` 看的是 `PT_REGISTERED`（页表分配器）；本段挪动的是后者，不会触发前者的 `boot allocator not registered` 停机。注册是进入内核的**前置条件**，不是可以从内存图推断的默认值。

### 4.2 UEFI 路径：`prepare_and_enter_kernel_image` 的顺序与描述符分类（→3.1、3.4）

两条 UEFI 产线（x86_64 与 aarch64）共用同一个入口：`os/boot-shim/src/uefi_helpers.rs` 的 [`fn prepare_and_enter_kernel_image`](<os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image>)。它返回 `!`：建根、开分页、跳独立镜像，不返回 boot-shim。U-Boot 腿的入口是 `opensbi_helpers::prepare_boot`；UEFI 侧走自己的入口，两者不共享 trait。

```rust
// 节选（保留顺序与关键约束；注释有删节；未展开段：参数统计的 `println`、`system_table`/`boot_services`/`ops` 构造、arena 的 `assert_range_clear_of_kernel`、`NeedBiggerBuffer` 扩容断言分支、`dumpdtb_blob` 的 `#[cfg]` 构造），os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image。
pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {
    uefi::println!("boot-shim: prepare_and_enter enter (handoff v1 arena)");

    // A+D：先读 ELF、预订其 LMA 跨度，再分配 root/bump，最后拷贝段。
    let file_loader = UefiFileLoader;
    let elf_data = file_loader.read_required(loader::KERNEL_PATH); // FileBytes：UEFI 腿是 Owned
    let layout = loader::compute_kernel_layout(elf_data.as_slice()).expect(...);
    reserve_kernel_span(&layout);      // AllocateAddress([kern_phys_base, +kern_size), LOADER_DATA)
    record_kernel_span(&layout);
    let root_page = alloc_root_page();
    let (bump_base, bump_end) = alloc_bump_region(bump_pages);
    minix_kernel::boot_alloc::init_boot_pt_alloc(bump_base, bump_end);
    unsafe { loader::load_segments_into_phys_memory(elf_data.as_slice()) }.expect(...);
    let kern = layout;
    uefi::println!("boot-shim: kernel loaded (entry staged)");

    // 语义表住在 shim 自己的 .bss（D-1）；loader 直接写进它的模块槽。
    let storage = UEFI_BOOT_STORAGE.get();
    // …（root/bump 的 assert_range_clear_of_kernel 略：位置在本行之前，见步骤 2）…
    let module_len = loader::load_boot_modules_into(&file_loader, alloc_module_pages,
        storage.module_slots_mut());
    storage.set_modules_len(module_len);
    uefi::println!("boot-shim: 12 boot modules loaded");
    // …（模块的 assert_modules_clear_of_kernel 略：位置在本行之后，
    //    说明见步骤 2 与 §5.1 M16）…
    let platform_len = find_platform_sources_into(storage.platform_slots_mut());
    storage.set_platform_len(platform_len);
    let param_stats = collect_param_buf_into(storage.param_buf_mut());
    storage.set_param_len(param_stats.bytes);

    let arena_pa = alloc_arena();
    let arena = unsafe { core::slice::from_raw_parts_mut(arena_pa.0 as *mut u8, ARENA_CAP) };
    let mut serializer =
        Serializer::new(arena, minix_boot::InputObjectId::UefiMemoryMap)
            .expect("handoff arena too small");
    // 标量信封先定稿；报告借用 storage，载荷也从同一份 storage 编码。
    let report = storage.report(kern.kern_virt_base, kern.kern_phys_base, kern.kern_size);
    serializer.write_envelope_report(&report).expect(...);
    serializer.write_boot_modules(report.boot_modules).expect(...);
    serializer.write_platform_sources(report.platform_sources).expect(...);
    serializer.write_param_buf(report.param_buf).expect(...);

    let mut buffer_bytes = probe_map_buffer_size(boot_services_raw);
    let classification = loop {
        let buffer_ptr = alloc_map_buffer(buffer_bytes);
        let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_bytes) };
        match unsafe {
            let (memmap_slots, reserved_slots) = storage.map_tables_mut();
            ebs_pair::pair_and_classify(&ops, boot_services_raw,
                image_handle.as_ptr() as usize, buffer, pa_limit(), &mut serializer,
                memmap_slots, reserved_slots)
        } {
            Ok(classification) => break classification,
            Err(PairError::NeedBiggerBuffer(needed)) => { /* 扩容后重试 */ }
            Err(error) => panic!("boot-shim: EBS pairing loop failed: {error:?}"),
        }
    };
    crate::raw_serial("boot-shim: [raw] boot services exited (paired)\r\n");

    // ……（此处省略约 35 行：startup 分区写入段——canonical 集合的
    //    kernel/modules/root/bump/arena 记录在此落盘，见 §3.4 的
    //    startup_objects 展开；配对循环体内另有一处省略（DTB 头 preflight，在首次 `GetMemoryMap`
    //    之后、`ExitBootServices` 之前完成，不是配对成功后的额外一次调用）；
    //    本处省略段只含 EBS 之后的 startup 分区写入。本节引文不展开它。引文里 dumpdtb_blob 是 `Option<Vec<u8>>`
    //    （DTB 暂存区的自有缓冲，不是静态切片）、ops 是 EBS 配对用的函数表包装，两者都在省略段之前的准备步骤中产生）……

    // 分类已把条目写进 storage；wire 只报长度，arena 里不再有第二份语义表。
    let descriptors = decode_descriptors(serializer.as_bytes()).expect(...);
    let memmap_len = (descriptors.memmap_byte_len / 16) as usize;
    let reserved_len = (descriptors.reserved_byte_len / 16) as usize;
    storage.commit_map_tables(memmap_len, reserved_len);
    let report = storage.report(kern.kern_virt_base, kern.kern_phys_base, kern.kern_size);
    cross_validate_config_dtb(report.platform_sources, report.memmap);
    assert_bootstrap_outside_memmap(report.memmap, root_page.0, PAGE_SIZE);
    assert_bootstrap_outside_memmap(report.memmap, bump_base, bump_end - bump_base);
    assert_bootstrap_outside_memmap(report.memmap, arena_pa.0, ARENA_CAP as u64);
    report_memmap_source(dumpdtb_blob.as_deref(), report.memmap);

    crate::raw_serial("boot-shim: [raw] arena=");
    raw_serial_hex(arena_pa.0);
    // …（conv/res 两行略）…
    crate::raw_serial("\r\nboot-shim: [raw] entering kernel image v1\r\n");
    minix_kernel::enter_kernel_image_v1(arena_pa.0, &report, root_page)
}

```

#### 4.2.1 九个动作与每步的约束

九个动作与每步的约束（正文里的"第 N 步"一律指**动作 ID**；动作 1 在时间上排在动作 2 的 ELF 预订之后，见动作 1 约束列；另注意三套编号的对照：本节九步 = `prepare_and_enter_kernel_image` 的九个动作；第 6 步内展开的"五步"是 `adjudicate` 对**单条描述符**的五步判定；§4.1 的两列表是按阶段归并的概览，不参与计数）：

| 动作 ID | 时间序 | 动作 | 约束 |
|---|---|---|---|
| 1 | 2 | 分配根页表页与 bump 区，`init_boot_pt_alloc` | 必须在 DM 可达上界之下；bump 池同时给 shim 建根与接收侧续用。A+D 之后本步排在第 2 步的 ELF 预订**之后**（先订跨度、再订自举页，避免自举页落进镜像的拷贝目标） |
| 2 | 1（读/预订）/ 3（拷贝/模块） | 读 `kernel.elf`、预订其 LMA 跨度；拷贝段与加载 12 个模块在动作 1 之后（时间列的 3 是动作 2 的拷贝/模块段；动作 3 在时间 4） | 必须在 EBS 之前（依赖 `SimpleFileSystem`）。A+D 之后的顺序是：读 ELF → `reserve_kernel_span` 用 `AllocateAddress` 把 `[kern_phys_base, +kern_size)` 整段预订为 `LOADER_DATA`（失败即 halt，不降级）→ `record_kernel_span` 记下该跨度（在拷贝**之前**）→ 把段拷进已预订的 LMA → 载模块。root/bump 的分配是第 1 步、发生在预订之后；预订先于**此后**的一切分配（ELF 读入的 `Vec` 是更早的一笔 UEFI 池分配，无法排到预订之前——布局要读完字节才知道；若它落进跨度，`AllocateAddress` 拒绝并停机，不会静默重叠），因此固件此后发出的任何页（模块、arena、池缓冲）都不可能在镜像跨度内，root/bump 也不可能落在镜像的拷贝目标之下；`alloc_module_pages` 的拒绝逻辑与 root/bump/模块的复查保留为第二道防线 |
| 3 | 4 | 定位平台描述符来源，归一化 `param_buf` | 必须在 EBS 之前（依赖 Configuration Table——固件在 EBS 前交给应用的描述符表，DTB/RSDP 指针挂在其中——与 `LoadOptions`——固件为本镜像保留的命令行字节串） |
| 4 | 5 | 分配八页 arena（`alloc_arena()` 经 UEFI `boot::allocate_pages(MaxAddress(bootstrap_alloc_limit()))`——该上界为无参函数返回的 DM 可达界；`bump_alloc_pages` 是 OpenSBI 侧的另一个分配器，勿混），`LOADER_DATA` | 整段落在恒等/DM 可达界之下；接收方经恒等窗口读它 |
| 5 | 6 | 从 `BootStorage` 的报告写信封、模块、平台源、参数（非内存图载荷） | 第 5 步开头 `Serializer::new` 对前 `ARENA_CAP` 字节整段零化（源文件 `handoff_v1_write.rs`）；信封的静态标量字段此时定稿（root/bump 等运行期字段在建根后回填）；载荷切片来自 storage，不是 `'static` 指向 arena |
| 6 | 7 | 分配 `GetMemoryMap` 缓冲，进入配对循环 | 循环内先 `GetMemoryMap`，再分类进 arena 的两个内存图分区，最后用同一 MapKey 调 raw `ExitBootServices` |
| 7 | 8 | 解码描述符，把 `memmap`/`reserved` 的活前缀长度提交给 `BootStorage` | 解码在 EBS 之后、任何断言之前发生；只有分类成功的那一份内存图存活；重试前 `reset_map_partitions` 清掉旧 wire 条目；分类同时写进 storage 与 wire，不存在第二份 arena 语义表 |
| 8 | 9 | 构造 `BootReport<'a>`，再用它复核 root/bump/arena 都不在 conventional 列表里 | 断言消费第 7 步提交的 storage 视图；失败即裸串口打印后停机（此时已无 Boot Services 可退）；报告的生命周期绑定 storage，内核入口只借用它，不提升为 `'static` |
| 9 | 10 | 建根、开分页、把运行期字段补进 arena、跳转 | 建根之后到跳转之前不再分配；`patch_runtime_fields_in` 不零化 arena |

第 5 步只写非内存图载荷；startup 分区在配对循环成功、`ExitBootServices` 之后由省略段补写（canonical 五类记录，见 §3.4）。它是上表九步之外的一个动作，记为 6b（不占九步的编号）——上表的编号覆盖到跳转为止（含 EBS 之后的步骤 7–9）；startup 写法夹在步骤 6 与 7 之间。

**MapKey 是什么**：UEFI 给每次 `GetMemoryMap` 的结果发一个标识符（`MapKey`，一个 `usize`）；此后任何改变内存图的操作（包括 `ExitBootServices` 内部的簿记）都会使旧 key 失效。`ExitBootServices` 的契约要求调用方传入"我最后一次 `GetMemoryMap` 时的 key"——固件用它确认调用者看到的内存图与它自己记录的一致；不一致就以 `EFI_INVALID_PARAMETER` 拒绝，调用方重取内存图、重新分类、再试。这就是分类必须与退出共用同一循环的原因：两者必须读同一份快照。

配对循环的失败面按状态码分派：`BUFFER_TOO_SMALL` 只在第一次 EBS 之前允许扩容缓冲；`EFI_INVALID_PARAMETER` 允许重分类一次（MapKey 变了）；其他状态立即失败。总尝试次数用尽或复用缓冲仍不足时 fail-closed，不沿用旧分类结果。

#### 4.2.2 描述符分类：五步判定与两道内联闸

**第 6 步的分流规则**（[`fn classify`](<os/boot-shim/src/ebs_map.rs:fn classify>) 与单条判定 [`fn adjudicate`](<os/boot-shim/src/ebs_map.rs:fn adjudicate>)）。判据不是"类型标签加 RAM 顶两遍扫描"，而是与 `adjudicate` 源码注释逐行对应的五步加两道内联闸：

| 步 | 判据 | 命中动作 | 未命中 |
|---|---|---|---|
| Step 1 | 类型已知（≤ `LAST_KNOWN`）、属性仅 `KNOWN` 位、无 `AMBIGUOUS`/`PROFILE_ONLY`、无 ISA 掩码（`ISA_VALID`/`ISA_MASK` 均不置位） | 继续 | 报错（`UnknownType`/`AmbiguousAttribute`/`ProfileAttribute`/`UnknownAttribute`/`IsaAttributes`） |
| Step 2 | 类型 ∈ {`UNACCEPTED`, `PAL_CODE`} | `RefuseToBoot`（先于一切后续步） | 继续 |
| 内联闸 | `checked_shape`：描述符结构可表示 | 继续 | 报错 |
| Step 3 | 类型 ∈ {`RESERVED`, `UNUSABLE`} → `Excluded`；∈ {`MMIO`, `MMIO_PORT_SPACE`} → `DevicePath` | 定档返回（先于属性判定） | 继续 |
| 内联闸 | `checked_len`：物理范围可表达 | 继续 | 报错 |
| Step 4 | 属性含 `TYPE_INDEPENDENT` | `Side` | 继续 |
| Step 5 | 属性 `WRITE_BACK == 0` → `Side`；否则按类型表：`CONVENTIONAL`→可分配、`LOADER_CODE`/`LOADER_DATA`/`BOOT_SERVICES_CODE`/`BOOT_SERVICES_DATA`/`ACPI_RECLAIM`→`ReservedMapped`、其余→`Side` | 定档返回 | —（类型表兜底） |

下表已给五类落点；这里只补充两道内联闸的覆盖差异，不再复述落点。**两道内联闸的覆盖范围不同**：`checked_shape` 在 Step 2 之后、Step 3 之前，所以所有通过 Step 1 的描述符（含 `Excluded`/`DevicePath`）都经过它；`checked_len` 在 Step 3 之后、Step 4 之前，而 Step 3 命中者已经定档返回——`Excluded` 与 `DevicePath` **不经过** `checked_len`。读表前先把判据令牌立住（与 [`ebs_map.rs`](os/boot-shim/src/ebs_map.rs) 的模块常量逐项对应）：

| 令牌 | 取值/含义 | 失败落点 |
|---|---|---|
| `LAST_KNOWN` | `UNACCEPTED`（15）；baseline 已知类型是 0–15 | 类型 > 15 → `UnknownType`（OEM/OS-loader 保留带） |
| `KNOWN` | baseline 定义的全部属性位（`UNCACHEABLE` 到 `ISA_MASK`，共 16 位） | `a & !KNOWN != 0` → `UnknownAttribute` |
| `AMBIGUOUS` | 两套官方文本对同一位置命名冲突的位（`WRITE_PROTECT`/`READ_PROTECT`/`EXECUTE_PROTECT`/`NON_VOLATILE`/`MORE_RELIABLE`） | 任一置位 → `AmbiguousAttribute` |
| `PROFILE_ONLY` | 只存在于某一固件编码的位（`READ_ONLY`/`SPECIAL_PURPOSE`/`CPU_CRYPTO`） | 任一置位 → `ProfileAttribute` |
| `ISA_VALID`/`ISA_MASK` | ISA 有效性位与掩码 | 任一非零 → `IsaAttributes` |
| `checked_shape` | 结构可表示：页数非零、`pages×4096` 不溢出、起点页对齐（`MMIO_PORT_SPACE` 除外）、`start+len ≤ 2^64`、虚拟起点对齐且不溢出 | `ZeroPageCount`/`LengthOverflow`/`UnalignedStart`/`RangeOverflow`/`UnalignedVirtualStart`/`VirtualRangeOverflow` |
| `checked_len` | 在 `checked_shape` 之上再要求 `start+len ≤ pa_limit` | `OutOfPhysRange` |
| `RefuseToBoot` | Step 2 的错误出口：整张图分类失败、启动中止（fail-closed），不是“跳过该条” | `PolicyError::RefuseToBoot` |
| `Side` | 侧桶：既不入 free list 也不入保留表，只计入诊断侧计数 | — |

手算五例（前两例成功定档；例 2 在 Step 3 即定档返回。后三例分别是 Step 1 报错、`Side` 定档与 `checked_len` 报错）：`CONVENTIONAL`+`WRITE_BACK` → Step 1 过、Step 2 不中、形状过、Step 3 不中、范围过、Step 4 不中、Step 5 类型表命中 `可分配`；`RESERVED`+`WRITE_BACK=0` → Step 3 先判 `Excluded`（Step 5 的 `Side` 永远不会看到它——这就是“类型强制档位先于属性”的可执行含义）；`CONVENTIONAL`+`attr::UNKNOWN_FOR_TESTING` → Step 1 的 `a & !KNOWN != 0` 命中，报 `UnknownAttribute`；`LOADER_DATA`+`WRITE_BACK`+`RUNTIME` → Step 4 命中 `Side`（`TYPE_INDEPENDENT` 里的 `NON_VOLATILE`/`SPECIAL_PURPOSE` 在 Step 1 已被 `AMBIGUOUS`/`PROFILE_ONLY` 拦下，能走到 Step 4 的只有 `RUNTIME`；属性独立档先于类型表）；`CONVENTIONAL` 起址 `pa_limit - 0x1000` 且页数为 2 → `checked_len` 命中 `OutOfPhysRange`（范围尾越过 `pa_limit`），整张图作废。分类器没有 `ram_top` 变量，也没有“第二遍按 RAM 顶分流”这一步。

- `RESERVED`、`UNUSABLE` → 排除；`TYPE_INDEPENDENT` 与 `WRITE_BACK == 0` → `Side` 桶（不进任何表，只计数）；其余按 `Side` 处理，都不进 free list。

`Excluded`、`Side` 与 `DevicePath` 为什么不进两份表：它们是固件声明“不要分配 / 不是内存 / 带独立语义”的区间，交给内核只会让它在 4 KiB 粒度下错误地恒等映射设备窗口，或把保留段当空闲内存。测试内核调用 [`fn build_memmaps`](<os/boot-shim/src/uefi_helpers.rs:fn build_memmaps>) 时的诊断输出是 `boot-shim: memmaps conv=… reserved=… unmapped=…`（空表时打印 `(empty map)`），没有单独的 `reserved-big` 行。

另有一条与分流正交的界：**凡通过 Step 3 的描述符**（含 `Side`；`Excluded`/`DevicePath` 因不产出范围而跳过）都必须让整段落在页表可表达的物理地址上限 `pa_limit` 内，越界即整张图作废（`PolicyError::OutOfPhysRange`，经 `MapError::Policy`/`RejectedDescriptor` 上抛），不做截断。`pa_limit` 是页表地址宽度（aarch64 / x86_64 各自的物理上限），不是 RAM 顶；`Side` 虽不进两份表，仍受这道界约束。

`AllocateType::MaxAddress(limit)` 的语义是“分配出来的末字节不超过 `limit`”（固件约束是 `base + size - 1 <= limit`），所以传“最后一个可接受的字节”即 `bound - 1` 就足够；再减请求大小反而更严，会拒绝本可容纳的请求。这个界是 `min(BOOT_IDENTITY_MAP_END, VM DM 窗口的物理上界)`：bump 池与 arena 既要能被“恒等写通道”（`VA = PA` 的自举写路径——进入内核前固件页表仍在生效、低 4GB 已被恒等覆盖，自举建表依赖这条通道写页表页；覆盖建立机制不在本篇展开，见 §1.4 的边界声明与 §3.8 的代码落点）写到（构建期间），又要能被 VM 的 Direct Map 窗口表示（构建之后使用者要能通过窗口访问它）。

第 1、2 步的界：

```rust
// 代码引用（略去文档注释），os/boot-shim/src/uefi_helpers.rs。
fn bootstrap_alloc_limit() -> u64 {
    boot_dm_admissible_end()
        .checked_sub(1)
        .expect("DM-admissible bound must sit above address zero")
}

pub fn alloc_root_page() -> PhysBytes {
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit()),
        MemoryType::LOADER_DATA,
        1,
    )
    .expect("Failed to allocate root page below DM-admissible bound");
    PhysBytes(ptr.as_ptr() as u64)
}
```

### 4.3 两种文件来源共用的装载函数（→3.2）

模块契约是本篇里最容易在改动时踩坏的东西：它的"顺序"部分没有任何编译期检查，只能靠装载顺序、运行期断言与测试兜住。装载前有三道固定编号的防线：**A** 每个段的文件字节区间与该段及**所有**段的目标 LMA 区间不相交（`first_source_destination_overlap`）；**B** 各段的目标 LMA 区间两两不相交（`first_overlapping_pair`）；**C** 模块区间不落进内核物理跨度（`alloc_module_pages` 拷贝前拒绝 + `assert_modules_clear_of_kernel` 装载后复查）。文件字节区间彼此之间的重叠不查——只读拷贝无害。任一条命中即 halt。Rust 侧把 C 的 `overlaps`（§2.5，守模块与内核之间）按 A/B/C 三道防线落地：A 与 B 守内核自身各 PT_LOAD 段（本节），C 守模块 vs 内核跨度（§4.2 的 `record_kernel_span`、`alloc_module_pages`、`assert_modules_clear_of_kernel`）。模块之间的重叠在装载器侧不单独查（按名字顺序装载与页分配隐含排除）；接收侧 `KernelInfo::validate` 另按半开区间复核一次（§4.4）。

```rust
// 代码引用（文档注释的说明段整段略去，块内不保留省略号），os/boot-shim/src/loader.rs。
pub const KERNEL_PATH: &str = "/EFI/minix/kernel.elf";
pub const DUMPDTB_PATH: &str = "/dumpdtb.dtb";
pub const MODULES_DIR: &str = "/EFI/minix/modules";

pub const MODULE_NAMES: &[&str] = &[
    "ds", "rs", "pm", "sched", "vfs", "memory", "tty", "mib", "vm", "pfs", "mfs", "init",
];

// 代码引用：编译期计数断言。
const _: () = assert!(MODULE_NAMES.len() == NR_BOOT_MODULES);
```

- 名字表有 12 项（复算：`sed -n '/pub const MODULE_NAMES/,/];/p' os/boot-shim/src/loader.rs | grep -o '"[a-z]*"' | wc -l` 输出 12），`NR_BOOT_MODULES = 12` 定义在 `minix-boot`。12 不是随手定的数：它等于 C `boot_image` 表的用户态段长度，少一项会有服务器没有启动名，多一项则没有对应的进程号可绑（C 侧 `NR_BOOT_PROCS` 与本篇的 `NR_BOOT_MODULES` 是同一张表的两个计数口径；每个名字绑哪个进程号，由表项自带的 `*_PROC_NR` 给出——[`kernel/table.c`](minix3/minix/kernel/table.c) 的 `{DS_PROC_NR, "ds"}` 等条目）。
- 顺序按 C 的 boot image 表（[`kernel/table.c`](minix3/minix/kernel/table.c) 里 `struct boot_image image[]` 的用户态段）排列：`ds, rs, pm, sched, vfs, memory, tty, mib, vm, pfs, mfs, init`。
- **位置不是进程号**。内核把第 `i` 个模块绑定到 `BOOT_MODULE_PROC_NRS[i]`，而后者里的数字来自 `com.h`——例如 `ds` 在位置 0，进程号是 6。名字表这一行代码上方给了这条注解，它是顺序契约的一部分。

装载循环：

```rust
// 代码引用（略去函数文档注释；`assert!` 的容量前置检查保留原样；行内注释有一处截断），os/boot-shim/src/loader.rs:fn load_boot_modules_into。
pub fn load_boot_modules_into<L: FileLoader>(
    loader: &L,
    alloc_pages: PageAllocator,
    out: &mut [BootModule],
) -> usize {
    assert!(
        out.len() >= MODULE_NAMES.len(),
        "boot-shim: module slot array has {} slots, the contract needs {}",
        out.len(),
        MODULE_NAMES.len()
    );
    for (slot, &name) in out.iter_mut().zip(MODULE_NAMES.iter()) {
        let path = build_module_path(name);
        let data = loader.read(&path).unwrap_or_else(|| {
            panic!(
                "boot-shim: required boot module '{}' not found at {} \
                 (kernel needs all {} in BOOT_MODULE_PROC_NRS order)",
                name,
                path,
                MODULE_NAMES.len()
            )
        });
        let data = data.as_slice();

        let num_pages = (data.len() + 4095) / 4096;
        let phys_base = alloc_pages(num_pages).unwrap_or_else(|| {
            panic!(
                "boot-shim: out of memory loading boot module '{}' ({} pages)",
                name, num_pages
            )
        });

        // SAFETY: `alloc_pages` returned `num_pages` pages of writable
        // physical memory; `data.len() <= num_pages * 4096`.
        unsafe {
            let dest = phys_base as *mut u8;
            dest.copy_from_nonoverlapping(data.as_ptr(), data.len());
            let remaining = num_pages * 4096 - data.len();
            core::ptr::write_bytes(dest.add(data.len()), 0, remaining);
        }

        // `name` 已是 `MODULE_NAMES` 常量表里的 `&'static str`——无需堆分配。
        *slot = BootModule {
            name,
            start: PhysBytes(phys_base),
            len: data.len(),
        };
    }
    MODULE_NAMES.len()
}
```

四处具体事实：

- `num_pages = (data.len() + 4095) / 4096`：向上取整到页。分子写 4095 而不是 4096 是按整数除法做 ceil 的标准形——`x/4096` 本身向下取整，先加 `4096-1` 才让任何非零余数进位；写 4096 会在 `data.len()` 恰为整页时多要一页，写小了（≤4094）则只要余数非零就会少一页，最后一段字节落进未分配的页。
- 分配出来的页里，`data.len()` 之后的余量被显式清零（`write_bytes(dest.add(data.len()), 0, remaining)`）。模块镜像按文件字节数拷贝，页尾的余量清零保证同一页内的任何对齐空洞都是确定值，而不是上一轮使用留下的内容。
- `BootModule.len` 记录的是**文件字节数**，不是页数。消费者按 `[start, start+len)` 计算占用区间。
- 缺件是 panic 而不是 `continue`。错误消息里带着模块名、路径与总数，落点在 boot-shim 侧；如果改成静默跳过，错误会推迟到内核侧的数量断言，那里只有"期望 12、实际 N"，没有缺失的名字。

OpenSBI 路径的 `FileLoader` 实现是查表：

```rust
// 代码引用（略去文档注释；块内中文注释为改述/译述），os/boot-shim/src/opensbi_helpers.rs:fn entry_path_eq 与 FileLoader for UbootFileLoader。
fn entry_path_eq(entry: &BootFileEntry, path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.len() >= BOOT_FILE_PATH_MAX {
        return false;
    }
    entry.path[..bytes.len()] == *bytes && entry.path[bytes.len()] == 0
}

impl<'a> FileLoader for UbootFileLoader<'a> {
    fn read<'b>(&'b self, path: &str) -> Option<crate::loader::FileBytes<'b>> {
        let entry = self.table.find(path)?;
        // SAFETY: U-Boot's `fatload` placed `entry.len` bytes at
        // `entry.phys_addr`. The region is identity-mapped during boot
        // and not concurrently modified.
        let slice = unsafe {
            slice::from_raw_parts(entry.phys_addr as *const u8, entry.len as usize)
        };
        // 表里存的是槽容量，不是文件长度；按最后一个 PT_LOAD 的
        // `offset+filesz` 修剪，`BootModule.len` 才是真实文件字节。
        // 非 ELF 回退整槽（测试/未来格式）。
        let used = minix_elf::segment_iter(slice)
            .ok()
            .and_then(|it| it.map(|seg| (seg.offset + seg.filesz) as usize).max())
            .filter(|&end| end > 0 && end <= slice.len())
            .unwrap_or(slice.len());
        // U-Boot staged the file in RAM; hand out a borrow instead of a copy.
        Some(crate::loader::FileBytes::Borrowed(&slice[..used]))
    }
}
```

`entry_path_eq` 把"路径必须 NUL 结尾"变成匹配条件的一部分：`entry.path[bytes.len()] == 0`。表由 `uboot_main::build_shim_table` 在 shim 的 `.bss` 内构建：它写入路径字节 + 一个终止 NUL；字段其余字节由 `.bss` 零初值保证，路径超界由构建时的两条 assert 当场拦下（消息 `does not fit the BootFileEntry field`）。因此 `entry_path_eq` 的 NUL 条件是构建器与查表器之间的契约；构建器若写坏路径，`find` 返回 `None`，随后 `read_required` 报"找不到内核"。`BootFileTable::valid_entries` 里 `entry_count` 被夹到容量上限（`(self.entry_count as usize).min(BOOT_FILE_TABLE_MAX_ENTRIES)`），因此一个写坏的 `entry_count` 只会让多余的条目被忽略，不会越界读。

**若把 `MODULE_NAMES` 里 `tty` 与 `mib` 两项互换会怎样**：装载会全部成功（12 个文件都在），`const _: () = assert!(... == NR_BOOT_MODULES)` 也通过——数量没变。但是位置 6 上的 `BootModule` 记的是 `mib` 的镜像与名字，内核按位置把它绑到 `tty` 的进程号上；位置 7 上则是 `tty` 的镜像，被绑到 `mib` 的进程号。启动过程依旧能走到 `kmain`，进程表也能建起来；症状出现在第一次对 `tty` 或 `mib` 发系统调用时——调用被打到错误的进程号，表现为"服务没有响应"或"收到协议外的回复"，与"装载时顺序错了"之间隔着整个启动流程。

### 4.4 `KernelInfo` 的构造：两个构造入口与一条校验门链（→3.4、3.5）

五条生产腿（两条 UEFI 链、U-Boot shim、两个直核面——计数口径见 §3.7 术语对账）在**数据形状**上收敛，但构造入口分两组（各腿如何产出两张表以 §3.4.1 为权威，本节只讲构造入口）：两个直核面共用 [`fn build_kernel_info`](<os/boot-shim/src/lib.rs:fn build_kernel_info>)（boot-shim 库根导出）；三条 v1 腿（两条 UEFI 链 + U-Boot shim）共用 `BootStorage`/`BootReport` 形状与 arena 协议，发送侧不构造 `KernelInfo`。这是"层内收敛原则"（§3.4 开头：收敛数据形状，不合并引导形态层）的落点：每条路径仍各自产出自己的两张表（`memmap` 与 `reserved_regions`），共享的是形状而不是一个构造函数。

```rust
// 代码引用（略去函数文档注释与 `minix_boot::`/`minix_types::` 路径限定符；代码内注释有删节与改述），os/boot-shim/src/lib.rs:fn build_kernel_info。
pub fn build_kernel_info(
    memmap: &'static [MemoryRegion],
    reserved_regions: &'static [MemoryRegion],
    kern_virt_base: VirBytes,
    kern_phys_base: PhysBytes,
    kern_size: u64,
    boot_modules: &'static [BootModule],
    platform_sources: &'static [PlatformDescSource],
    param_buf: &'static [u8],
    memmap_sources: &'static [SourceRecord],
    startup_objects: &'static [SourceRecord],
) -> KernelInfo {
    KernelInfo {
        memmap,
        reserved_regions,
        memmap_sources,
        startup_objects,
        kern_virt_base,
        kern_phys_base,
        kern_size,
        // Per-arch user stack top; the single authority is
        // `minix_types::USER_STACK_TOP`.
        user_sp: VirBytes(minix_types::USER_STACK_TOP),
        kern_stack_top: VirBytes(kern_virt_base.0 + kern_size),
        syscall_entry: VirBytes(kern_virt_base.0),
        boot_modules,
        platform_sources,
        param_buf,   // 〔编注〕各路径 normalizer 的产物；空切片等价于 C 的 `param_buf[0] == '\0'`
    }
}
```

`reserved_regions` 的取值按路径：UEFI 路径传配对循环判为 `ReservedMapped` 的列表（`LOADER_*`/`BOOT_SERVICES_*`/`ACPI_RECLAIM` 且 `WRITE_BACK`；`Excluded`/`DevicePath`/`Side` 不进该表）；U-Boot shim 路径与 riscv64 直核面传 `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)` 削出的 reserved（OpenSBI 足迹以下非空）；aarch64 直核面传 `DtbMemoryMap::new(&fdt, 0)`，reserved 典型为空（`fw_end=0` 的语义与例外见紧随其后的字段表行）。

| 字段 | 谁填 | 值从哪来 |
|---|---|---|
| `memmap` | builder 调用方 | UEFI：配对循环最后一次成功 `GetMemoryMap` 分类出的 conventional 列表；U-Boot/riscv 直核：`DtbMemoryMap::new(&fdt, KERN_PHYS_BASE).collect_with_sources()`（DTB 的 `/memory`；U-Boot 路径的 DTB 走 `argv[0]`，直核面走 a1，同一份解析实现住在 minix-boot） |
| `reserved_regions` | 调用方 | UEFI：配对循环判为 `ReservedMapped` 的列表（`Excluded`/`DevicePath`/`Side` 不进）；U-Boot/riscv 直核：`DtbMemoryMap` 以 `KERN_PHYS_BASE` 裁剪的 reserved；aarch64 直核：`fw_end = 0` 关闭固件前缀裁剪，DTB `/reserved-memory` 声明仍进表（典型为空） |
| `kern_virt_base` / `kern_phys_base` / `kern_size` | 装载函数 | ELF 各 PT_LOAD 段的 `min(vaddr)` / `min(paddr)` / `max(paddr+memsz) - min(paddr)` |
| `user_sp` | builder | `minix_types::USER_STACK_TOP` 单一权威常量 |
| `kern_stack_top` | builder 派生 | `kern_virt_base + kern_size`——**镜像尾**（末段 memsz 端点，即 Rust 的 `os/kernel-image/*.ld` 在 `.bss` 末尾 `ALIGN(0x200000)` 收口后的地址，2MiB 对齐；C 的 `kernel.lds` 没有这道收口，`_kern_size` 算到 `.bss` 结束，两者不可混用）。它与 ld 符号 `kernel_boot_stack_top`（64KiB 引导栈数组的顶）是两个对象：数组顶定在收口**之前**，与镜像尾相差一段收口填充（0 到 2MiB−16；数组顶按 16 字节对齐、镜像尾按 2MiB 对齐，故差值最大为 2MiB−16）。`jump_to_kmain` 把 sp 置到镜像尾（[`higher_half.rs`](os/kernel/src/arch/riscv64/higher_half.rs:jump_to_kmain) 的 `mv sp, stack_top`），栈向下生长先穿过收口填充——填充属于末段 memsz（三个 `os/kernel-image/*.ld` 的收口注释都写明），已映射、可写；清零只对跨镜像腿成立（loader 按 `memsz` 清零），两个直核面的 `_start` 只清到 `__bss_end`，收口填充不保证为零——栈是先写后读，安全性不依赖清零。填充等于给引导栈一段额外余量，不改变安全性 |
| `syscall_entry` | builder 派生 | `kern_virt_base`（引用性启动元数据；实际门地址由各架构 asm 标签提供，当前三个架构都按基址使用） |
| `boot_modules` | 装载函数 | 按 `MODULE_NAMES` 顺序的 12 项 |
| `memmap_sources` / `startup_objects` | 调用方经 builder 两个参数传入 | 直核面：构造前算好（`bootface`/`bootface_a64` 的 startup 集合与 DTB 来源记录）并传入；v1 发送腿不调用 builder，发送侧写 wire、接收侧解码落 `.bss`（§3.4 的展开与 §4.2 的写入段） |
| `platform_sources` | 调用方 | UEFI：Configuration Table 扫描结果；OpenSBI：`[DTB]` 或空 |
| `param_buf` | 调用方 | 各条路径的 normalizer 输出：UEFI 读 `LoadOptions`，OpenSBI/直核读 DTB `/chosen/bootargs` |

`boot_validate_and_prepare` 是这份结构在内核侧的入口校验（`arch_boot` 的调试打印已先读过部分字段），它在建表之前先做校验：

```rust
// 节选（保留校验段；尺寸阶梯与诊断路标宏略去——阶梯的代码与解码在 §3.6；注释省略，分配器注册检查以省略号代替），os/kernel/src/lib.rs:fn boot_validate_and_prepare。
    let kv = kern_virt_base.0;
    let kp = kern_phys_base.0;
    let ks = kern_size;

    assert!(ks > 0, "arch_boot: kern_size must be > 0");
    assert!(kp.is_multiple_of(0x1000), "arch_boot: kern_phys_base must be page-aligned");
    assert!(kv.is_multiple_of(0x1000), "arch_boot: kern_virt_base must be page-aligned");
    assert!(kern_stack_top.0.is_multiple_of(16),
        "arch_boot: kern_stack_top must be 16-byte aligned");
    ... // require_boot_region：boot_alloc_region() 为 None 即停机（无兜底）
    let support = detect_boot_page_support::<A>()
        .unwrap_or_else(|e| panic!("arch_boot: page-size capability facts: {e}"));
    assert!(support.phys_addr_limit() >= IDENTITY_MAP_END, "...");
    support
```

内核侧的校验按执行序列排成一条门链：

1. `boot_validate_and_prepare`（`arch_boot` 建表前）：`ks > 0`、两个基址页对齐、`kern_stack_top` 16 字节对齐、boot bump 区间已注册（`boot_alloc_region()` 非 `None`，否则停机；`pt_alloc` 由本函数按 `is_registered()` 守卫注册，不是 fail-closed）、页大小能力与物理上限。
2. `validate_before_tables`（pre-table gate）：`kern_size != 0`、`memmap` 非空、`param_buf.len() <= 1024`——它**不查** `boot_modules` 非空（QEMU 测试内核合法携带空模块表）。
3. `check_boot_postconditions`：高半地址关系（`syscall_entry` 落在镜像跨度内；`kern_stack_top` 不越过跨度上界，恰等于镜像尾合法；用户栈不越入内核半区等）。
4. `KernelInfo::validate`（`kmain` 入口）与接收腿的 `validate_requiring_sources`（strict 变体）：前者复核非地址不变量、检查生产内核的 `boot_modules` 非空，并复核内核镜像与各模块、模块与模块的半开区间不重叠（相切放行；跨度加法用 `checked_add`，防 release 回绕把判断变成恒真）；后者按结构体自带的 `authority` 另要求 source/startup 记录覆盖内核镜像。

下表按门列出各校验点挡的 bug 类（十二行；第 4 项含非 strict 与 strict 两变体，故门列有五个门标签，另有“规划期（不是门）”一行；编号链仍是四项）（同名项在不同门重复挡是故意冗余；`validate_before_tables` 即 §3.7 注释里的 pre-table gate）：

| 门 | 校验点 | 挡的是哪一类 bug |
|---|---|---|
| `boot_validate_and_prepare` | `ks > 0` | boot-shim 没装载成功却继续往下走 |
| `boot_validate_and_prepare` | 两个基址页对齐 | 布局常量写错（链接脚本改了而工具没跟） |
| `boot_validate_and_prepare` | `kern_stack_top` 16 字节对齐 | ABI 要求（AAPCS64 / SysV 的栈对齐） |
| `boot_validate_and_prepare` | `detect_boot_page_support` 成功且 `phys_addr_limit() >= IDENTITY_MAP_END` | 页大小能力缺失或 DM 窗口上界不足（启动最早、最难归因） |
| `validate_before_tables` | `kern_size != 0` | `kmain` 之前就拿到空镜像 |
| `validate_before_tables` | `memmap` 非空 | 内存图丢了 |
| `validate_before_tables` | `param_buf.len() <= 1024` | 参数缓冲超过 C 的固定容量 |
| `check_boot_postconditions` | `syscall_entry` 落在镜像跨度内；`kern_stack_top` 不越过跨度上界（恰等于镜像尾合法）；用户栈不越入内核半区 | 高半地址关系错（镜像内入口/栈顶偏移写错，或用户栈上界越入内核半区） |
| `KernelInfo::validate`（`kmain` 入口） | 复核 `kern_size`/`memmap`/`param_buf` 非地址不变量；`boot_modules` 非空（仅生产内核；QEMU 测试内核合法为空） | 同前几行，在更晚的入口再挡一次；模块装载失败 |
| `KernelInfo::validate`（交叉表） | `memmap` 与 `reserved_regions` 不相交 | 保留区被当成空闲页发出 |
| `KernelInfo::validate`（接收侧复核） | 内核镜像与各模块、模块与模块按半开区间互不重叠（相切放行；跨度加法 `checked_add`） | 跨二进制载荷把模块放到内核或另一个模块身上（信任边界上的第二道复核） |
| `validate_requiring_sources`（接收腿 strict） | 按 `KernelInfo.authority` 查 source/startup 记录覆盖内核镜像 | 生产者漏写 provenance，或声明了没有记录的权威 |
| 规划期（不是门） | `plan_kernel_mapping` 的首选/回退选择 | 镜像跨距不满足回退尺寸对齐 → 规划阶段给出可读错误 |

**"去掉 `bootstrap_len == 0` 断言"在当前代码里已无从谈起**：字段与断言已随字段删除（论证见 §3.5），这条反向推演保留的是它曾对应的真实险情。字段删除后，"提供一个可误设的回收区间"这个接口面整体消失，险情在类型上不可复发——这是 §3.5 删除论证的现在时态，本节不再重述故障链。

### 4.5 内核侧第一步：`store_kernel_info` 的七个落地区（→3.4）

`arch_boot` 的早期动作是把按值传入的结构深拷进内核自己的 `.bss`（`lib.rs` 的 [`fn store_kernel_info`](<os/kernel/src/lib.rs:fn store_kernel_info>)），然后只使用这份副本。

```rust
// 节选（保留七个落地区各自的核心语句；省略号处为注释与相邻块的接续行），
// os/kernel/src/lib.rs:fn store_kernel_info。
pub fn store_kernel_info(kernel_info: &KernelInfo) {
    unsafe {
        let store = &mut *crate::globals::RESERVED_REGION_STORE.get();
        let src = kernel_info.reserved_regions;
        assert!(
            src.len() <= store.len(),
            "store_kernel_info: reserved_regions exceeds the .bss landing pad"
        );
        if src.as_ptr() != store.as_ptr() {
            core::ptr::copy(src.as_ptr(), store.as_mut_ptr(), src.len());
        }
        ...
        let mstore = &mut *crate::globals::MEMMAP_REGION_STORE.get();
        let msrc = kernel_info.memmap;
        assert!(
            msrc.len() <= mstore.len(),
            "store_kernel_info: memmap exceeds the .bss landing pad"
        );
        if msrc.as_ptr() != mstore.as_ptr() {
            core::ptr::copy(msrc.as_ptr(), mstore.as_mut_ptr(), msrc.len());
        }
        let bstore = &mut *crate::globals::BOOT_MODULE_STORE.get();
        let bnames = crate::globals::BOOT_MODULE_NAME_STORE.get();
        let bsrc = kernel_info.boot_modules;
        assert!(
            bsrc.len() <= bstore.len(),
            "store_kernel_info: boot_modules exceeds the .bss landing pad"
        );
        if bsrc.as_ptr() != bstore.as_ptr() {
            for (i, module) in bsrc.iter().enumerate() {
                let bytes = module.name.as_bytes();
                let n = bytes.len().min(crate::globals::BOOT_MODULE_NAME_LEN - 1);
                let slot = &mut (*bnames)[i];
                slot[..n].copy_from_slice(&bytes[..n]);
                let name: &'static str = match core::str::from_utf8(&slot[..n]) {
                    Ok(s) => s,
                    Err(e) => core::str::from_utf8(&slot[..e.valid_up_to()]).unwrap_or("?"),
                };
                bstore[i] = minix_boot::BootModule { name, start: module.start, len: module.len };
            }
        }
        let pstore = &mut *crate::globals::PLATFORM_SOURCE_STORE.get();
        let psrc = kernel_info.platform_sources;
        assert!(
            psrc.len() <= pstore.len(),
            "store_kernel_info: platform_sources exceeds the .bss landing pad"
        );
        if psrc.as_ptr() != pstore.as_ptr() {
            core::ptr::copy(psrc.as_ptr(), pstore.as_mut_ptr(), psrc.len());
        }
        let param = &mut *crate::globals::PARAM_BUF_STORE.get();
        let csrc = kernel_info.param_buf;
        assert!(
            csrc.len() <= param.len(),
            "store_kernel_info: param_buf exceeds the .bss landing pad"
        );
        if csrc.as_ptr() != param.as_ptr() {
            param.fill(0);
            param[..csrc.len()].copy_from_slice(csrc);
        }
        let sstore = &mut *crate::globals::SOURCE_RECORD_STORE.get();
        let ssrc = kernel_info.memmap_sources;
        assert!(
            ssrc.len() <= sstore.len(),
            "store_kernel_info: memmap_sources exceeds the .bss landing pad"
        );
        if ssrc.as_ptr() != sstore.as_ptr() {
            core::ptr::copy(ssrc.as_ptr(), sstore.as_mut_ptr(), ssrc.len());
        }
        let tstore = &mut *crate::globals::STARTUP_RECORD_STORE.get();
        let tsrc = kernel_info.startup_objects;
        assert!(
            tsrc.len() <= tstore.len(),
            "store_kernel_info: startup_objects exceeds the .bss landing pad"
        );
        if tsrc.as_ptr() != tstore.as_ptr() {
            core::ptr::copy(tsrc.as_ptr(), tstore.as_mut_ptr(), tsrc.len());
        }
        let mut k = *kernel_info;
        k.reserved_regions = core::slice::from_raw_parts(store.as_ptr(), src.len());
        k.memmap = core::slice::from_raw_parts(mstore.as_ptr(), msrc.len());
        k.boot_modules = core::slice::from_raw_parts(bstore.as_ptr(), bsrc.len());
        k.platform_sources = core::slice::from_raw_parts(pstore.as_ptr(), psrc.len());
        k.param_buf = core::slice::from_raw_parts(param.as_ptr(), csrc.len());
        k.memmap_sources = core::slice::from_raw_parts(sstore.as_ptr(), ssrc.len());
        k.startup_objects = core::slice::from_raw_parts(tstore.as_ptr(), tsrc.len());
        *KERNEL_INFO.get() = Some(k);
    }
}
```

七个落地区的容量与溢出行为（另配一个名字字节池）：

| 落地区 | 容量常量 | 值 | 溢出行为 |
|---|---|---|---|
| `RESERVED_REGION_STORE` | `RESERVED_REGION_STORE_LEN` | 256 | panic |
| `MEMMAP_REGION_STORE` | `MEMMAP_REGION_STORE_LEN` | 128（C `MAXMEMMAP=40` 提额；常量定居 globals.rs） | panic |
| `BOOT_MODULE_STORE` | `minix_boot::NR_BOOT_MODULES` | 12 | panic |
| `PLATFORM_SOURCE_STORE` | `PLATFORM_SOURCE_STORE_LEN` | 4（真实上限 2 × 有意余量） | panic |
| `PARAM_BUF_STORE` | `PARAM_BUF_STORE_LEN` | 1024（C 的 `MULTIBOOT_PARAM_BUF_SIZE`） | panic（先清零再复制） |
| `SOURCE_RECORD_STORE` | `SOURCE_RECORD_STORE_LEN` | 296 条记录（`WIRE_SOURCES_CAP`；wire 分区容量 21312 B，每条 72 B）。发送侧 `BootStorage` 的 source 槽是 40（DTB 收集器输入上限，超过 fail-closed），wire/接收侧 stores 是 296 | panic |
| `STARTUP_RECORD_STORE` | `STARTUP_RECORD_STORE_LEN` | 20 条记录（`WIRE_STARTUP_CAP`，规范集合定稿后；wire 分区容量 1440 B） | panic |
| `BOOT_MODULE_NAME_STORE`（名字字节池） | `BOOT_MODULE_NAME_LEN` | 16（每槽，复制时截到 15 字节） | 截断（不 panic） |

在七个落地区里，名字池是唯一允许截断的（`param_buf` 的超长在规范化阶段已按整条记录丢弃，不会超容量落到这里）：`n = bytes.len().min(BOOT_MODULE_NAME_LEN - 1)`（16 - 1 = 15），保留一个字节给 NUL 语义上的余量。截断后用 `core::str::from_utf8` 造 `&'static str`；若 15 字节截断切开了多字节序列，取 `valid_up_to()` 的最长合法前缀，而不是用 `from_utf8_unchecked` 把非法字节当成 `str`。生产名字全是 ASCII，但函数是公开的，不能把安全性寄托在调用者的内容上。测试内核路径用 write-once 静态表（`AtomicBool` 单写者 + 初始化后只读，无 `static mut`/别名面），并收进 `qemu-test-kernels` feature（生产不链接）；完整 `BootStorage` 迁移见 F-R20-03。

`if src.as_ptr() != store.as_ptr()` 不是防御性假设：`store_kernel_info` 有三个调用点——`arch_boot_finish` 传原始 handoff（真正拷贝一次）；`arch_boot_resume_v1` 先把七个分区直接解码进这七个 store，随后 `resume_additive` 的 store 源就是 store 本身（同址，跳过）；`kmain` 拿到的是全局副本、再存一次也是同址。`ptr::copy` 是为**部分重叠**保留（`copy_nonoverlapping` 对部分重叠是 UB）；精确同址由守卫跳过，不会调用到拷贝。

三个容量各有来由：`RESERVED_REGION_STORE_LEN = 256` 是真机 probe 的 127 条 reserved 翻倍后向上取整到 2 的幂（独立容量；`vm_handoff` 的 ident 上限是 64，两者不是同一个常量）；`MEMMAP_REGION_STORE_LEN = 128` 直接定居 globals.rs——C 形状账本模块（`memmap.rs` 的 add/cut + 空洞 + 高水位）删除后，交接扣减由 [`vm_handoff.rs`](os/kernel/src/vm_handoff.rs) 的 `FreeRangeSet`（有序不相交页对齐集合）承担（扣减换手后），行为契约由"与参考区间减法逐页等价"的对拍测试锁住（读数与归因限定见 §3.4 的"不做深拷…会怎样"，实现侧增量见本节末）；`BOOT_MODULE_NAME_LEN = 16` 与 `vm_handoff::copy_name` 的 16 字节 `proc_name` 字段同宽，落地不会比既有长度契约更紧。另外几个容量定案：`PLATFORM_SOURCE_STORE_LEN = 4` 是"一个 DTB/ACPI 源加一个回退"的真实上限再乘二；`PARAM_BUF_STORE_LEN = 1024` 直接对齐 C 的定长语义，复制前先清零，短缓冲留下的尾部因此是全零而不是上一轮内容；`SOURCE_RECORD_STORE_LEN` 与 `STARTUP_RECORD_STORE_LEN` 直接取 wire 分区容量（296 与 20），与解码端同一口径；startup 容量 16→20 是规范集合的余量（1 内核 + 12 模块 + root + bump + arena = 16，留 4 槽给新增腿）。

**只做一半深拷会怎样**：`reserved_regions` 深拷进去，但 `memmap` 与 `boot_modules` 保持原指针。后果是 free list 退化成空记录，第一次物理页分配失败时，现场离"三个切片里有两个没落地"隔着整个分类阶段。机制、读数与三层证据判别统一见 §3.4.4，本节不重复。

### 4.6 自举根的建立与开启（→3.6、3.8）

沿用 §4.1 立的 Step 编号（本节展开 Step 1–3；Step 4 的 DM 覆盖本篇不展开，边界见 §1.4；§6.1 用它排路标）。这套编号与 §2.6.1 描述的 C 两级页表流程（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五步）**不是同一个体系**：C 的五步是 32 位两级页表的建表顺序，Rust 的 Step 0–4 是 4 级页表的启动阶段编号（多出校验与 DM 两段、没有独立的 `pg_load`）。五步在 Rust 侧的落点：`pg_clear` 并入根页清零（`new_from_page`）、`pg_identity` 并入 Step 1、`pg_mapkernel` 并入 Step 2、`pg_load` 并入 `enable` 的 `mov cr3`、`vm_enable_paging` 即 Step 3。

Step 1 与 Step 2 是两段映射循环；aarch64 另有 Step 2b（设备 MMIO 窗口的高半别名）；Step 2c 是三架构共用的一段附属步骤（编号 2c 不占主步序号；x86/riscv 设备表非空，aarch64 空表故为 no-op）。它们的范围判据不同：

```rust
// 节选（保留 Step 1/2 的循环体；注释、#[allow] 与运行期断言有删节），
// os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable。
// （DEV_PIECE 是文件级常量 `const DEV_PIECE: u64 = 1 << 21;`——2 MiB 分片尺寸，
//   此处随调用序一并展示，非函数体局部定义。）
    let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let device_windows = identity_device_windows();
    let mut addr: u64 = 0;
    while addr < IDENTITY_MAP_END {
        let block_end = addr + kern_huge.bytes();
        let mut piece_addr = addr;
        let piece_size = if identity_block_touches_device(addr, kern_huge.bytes(), device_windows) {
            PageSize::MIB_2
        } else {
            kern_huge
        };
        while piece_addr < block_end {
            let piece_flags =
                if identity_piece_is_device(piece_addr, piece_size.bytes(), device_windows) {
                    PageFlags::kernel_read_write() | PageFlags::DEVICE
                } else {
                    id_flags
                };
            let _ = paging.map_huge(VirBytes(piece_addr), PhysBytes(piece_addr),
                                    piece_size, piece_flags);
            piece_addr += piece_size.bytes();
        }
        addr = block_end;
    }

    // Step 2: Kernel high-address mapping. C: pg_mapkernel() — pg_utils.c:186
    let kern_virt = layout.virtual_base;
    let kern_phys = layout.physical_base;
    if kern_virt != kern_phys {
        let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let mut offset = 0u64;
        while offset < layout.size {
            paging.map_huge(VirBytes(kern_virt + offset), PhysBytes(kern_phys + offset),
                            kern_huge, kern_flags)
                .expect("kernel map: map_huge failed — kernel image mismatch");
            offset += kern_huge.bytes();
        }
    }
```

Step 1 的注释与循环行为按六条交代：前两条是判据，其后四条是配套说明（第六条的分片规则 aarch64 与 riscv64 都适用）：

- 范围是 `IDENTITY_MAP_END`（唯一权威在 [`minix-arch`](os/arch/src/lib.rs) 的 `BOOT_IDENTITY_MAP_END`，值就是低 4GB；kernel 侧是别名 `use minix_arch::BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END`，boot-shim 侧 §4.2 的分配上界以 `BOOT_IDENTITY_MAP_END` 为一个界，再与 VM DM 窗口物理上界取小），不是"可用内存的并集"。理由写在 Step 1 之前的注释里：UEFI 的加载器代码/数据可能落在 `LOADER_DATA`/`LOADER_CODE` 类型的内存里，它们不是 conventional；不覆盖这些区间，切页表后的下一条取指就取不到码。
- 标志是 `kernel_read_write() | EXECUTABLE`（supervisor-only + 可执行），不是"用户可访问"。RISC-V Sv39 上，S 模式取指一条 `U=1` 的页是被禁止的——`sstatus.SUM` 只放宽 supervisor 对用户页的 load/store，取指不受它影响——所以恒等映射必须是 supervisor-only。（源码 Step 1 的 NOTE 已按同一口径陈述：`the supervisor may not execute a page with U=1 irrespective of sstatus.SUM (SUM only relaxes loads/stores)`。）C 侧口径见 §2.6.1（`pg_identity` 置 USER 位但在 Ring 0 无实效；`pg_mapkernel` 不置）；Rust 侧的差异是直接不置它。

- 缓存属性是另一处与 C 的差异，两侧的划法不同：C 按单一内存上界划分——**上界所在的 4 MB 块及其以上**统一置 `PWT|PCD`，条目仍 PRESENT（边界块内、上界以下的真实内存也会被多包一圈，例子见 §2.6.1；[`pg_identity`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity) 对全部 1024 项都置 `I386_VM_PRESENT`），所以 C 的空洞访问同样不触发页错误——`PWT|PCD` 防的是垃圾字节进缓存，fast-fail 在 C 里本来就没有提供；64 位侧按"登记窗口"划分——登记过的 MMIO 窗口在 Step 1 内按 2MiB 分片置 `DEVICE`（机制见第六条），未登记的空洞统一可缓存 `id_flags`。这个差异连同它的代价要交代清楚：其一，**前提：未登记空洞在运行期没有访问者**——物理访问只从账本分配的 DRAM 来，会真访问的 MMIO 全部进了登记窗口；这条前提当前没有运行期断言，靠代码审查与平台源登记维持（原注释口径曾是 "some ranges may not be backed by physical memory, but that's fine; the CPU won't access them"；现注释已改写为 map_huge 错误集口径，见第五条）；其二，fast-fail 的削弱被接受——C 的 `PWT|PCD` 也不提供页错误，被放弃的只是"空洞访问不污染缓存"这层兜底，防线从映射属性换成账本纪律：分配只来自账本、MMIO 只来自登记窗口。

- Step 1 的窗口清单是 `identity_device_windows()`（aarch64 = 平台 MMIO 基窗，riscv64 = `minix_arch::kernel_identity_mmio_regions()` 的 PLIC/UART 子集（`identity_device_windows()` 在该架构上就是对该表的筛选；x86 上返回空），x86 = 空——x86 的 LAPIC/IO-APIC 不在 Step 1 置属性，靠固件建立的 MTRR）；Step 2c 的 `device_window_regions()` 是另一条腿，把会从高半访问的固定 MMIO 页装进自举根，x86 的 LAPIC/IO-APIC 走的是它。

- 循环里 `let _ = paging.map_huge(...)` 忽略错误。先纠正这条忽略自己原来的理由：源码注释曾写"some ranges may not be backed by physical memory, the CPU won't access them"，但 `map_huge` 的错误集（`InvalidAddress`/`AlreadyMapped`/`NotMapped`/`AllocationFailed`/`PermissionDenied`/`NotSupported`，[`paging.rs`](os/arch/src/arch/paging.rs:PageTableError)）里**没有**"物理内存不存在"这一类——`map_huge` 根本不探测物理内存，空洞不可能从这里产生错误。Step 1 里可达的失败类别只有 `AllocationFailed` 一类（其余五类：`AlreadyMapped` 见下句；地址由常量生成→`InvalidAddress`；只做映射→`NotMapped`；尺寸来自能力快照→`NotSupported`；`PermissionDenied` 在当前 `os/arch` 无生产者）：某块需要新配一级中层页表、而引导 bump 分配器发不出页；预算在 UEFI/U-Boot 腿是 1024 页（两个直核面是 256 页）、实际只需几张中层表，正常不会触发，但若真发生且该块正是 CPU 要取指/访问的，故障会在 `enable()` 的下一条取指上出现——恰好是上一条反向推演的场景（注释里已把这一后果写明，不把它说成“可以接受”）。`AlreadyMapped` 属**结构性排除**（不可达）：根是新建清零的、分片按地址严格前进不重访任何 VA，同一槽位不会被写两次（Step 2 的同槽异尺寸守卫针对的是 Step 1 与 Step 2 之间，见下条）。注释原文已按这个口径改写（`os/kernel/src/lib.rs` 的 Step 1 循环内）。
- 设备窗口分片对 aarch64 与 riscv64 都适用。先回答一个自然的互斥疑问：Step 1 铺的 1GiB 恒等叶会不会与设备窗口的 2MiB `DEVICE` 分片撞 §3.6 的 `AlreadyMapped` 守卫？不会——分片发生在 Step 1 自己的循环里（同一循环按 `identity_block_touches_device` 决定块尺寸：碰窗口的块拆成 2MiB 分片、其余保持 1GiB），窗口部位从头到尾只写一次 2MiB 叶，不存在"先 1GiB 后 2MiB 覆盖"的次序；Step 2c 是另一条腿（把会从高半访问的固定 MMIO 页装进自举根的窗口集合 `device_window_regions()`），与 Step 1 的分片不共享槽位判定。`identity_device_windows()` 返回两类窗口——aarch64 的 MMIO 物理窗口，以及 riscv64 的 PLIC/UART 固定窗口（`minix_arch::kernel_identity_mmio_regions()`）。与窗口相交的块按 `DEV_PIECE`（2 MiB）分片，落在窗口内的分片改置 `DEVICE` 标志——MMIO 寄存器要求强序访问（不投机、不合并、不重排），普通可缓存映射在 QEMU 上能过、真机上不行；注释同时要求这里的分片与 Step 2b 的高半别名对同一物理页使用同一套属性（ARM 手册：同一物理页用两种属性映射，结果不可预测）。不碰窗口的块保持 `kern_huge` 粒度。**注意现行三个生产镜像的物理基址都不是 1 GiB 对齐**（x86_64 `0x200000` / aarch64 `0x40200000` / riscv64 `0x80200000`），且 `layout_satisfies(GIB_1)` 还要求 `kern_size % 1GiB == 0`——生产镜像跨度约 4 MiB，按 size 就恒不满足；因此规划层选 2 MiB 回退档，现行 Step 1 全部是 2 MiB 叶。1 GiB 只在单元测试的合成 layout 里成立，是理论路径。

Step 2 的守卫 `if kern_virt != kern_phys` 记录在注释里：`kern_virt == kern_phys` 时 Step 2 与 Step 1 是同一个映射，跳过它可以避免在 riscv64 上把同一个 VPN[2] 槽用不同的页尺寸写两次，破坏恒等映射。

aarch64 上还有一段 Step 2b：把设备 MMIO 窗口按 2 MiB 映射到内核高半（`MMIO_VA_BASE`）。低半那份恒等映射会随第一次 `switch_address_space` 换掉 TTBR0 而消失，而 TTBR1 钉在自举根上、不参与切换——串口与 GIC 需要一个高半别名才能在切换之后继续可用（Step 3 末尾的 `use_high_mmio_window()` 配套它；属性与 Step 1 的恒等别名一致，理由同上一条）。

之后还有一段 Step 2c：把 `device_window_regions()` 里 `bootstrap = true` 的设备窗口装进自举根（x86 的 LAPIC/IO-APIC、riscv 的 PLIC/UART；另有 ACLINT SSWI 运行期窗口，`bootstrap = false`，DTB 门控，不在 Step 2c 安装）。这些窗口的槽位按 2 MiB 对齐，逐段用 `map_huge` 装成 2 MiB 叶；aarch64 的设备表为空，Step 2c 直接跳过（它不是错误，控制器走系统寄存器、设备走高半别名）。注意 Step 1 的清单是 `identity_device_windows()`；在 riscv64 上它是 `kernel_identity_mmio_regions()` 的 PLIC/UART 子集，整张表还含 Step 2c 用的窗口，所以不能把整张表当成 Step 1 清单。x86_64 分支返回空表，LAPIC/IO-APIC 只由 Step 2c 安装（x86 的 `kernel_identity_mmio_regions` 当前无生产读者）。

Step 3 的 `enable()` 在 x86-64 上做四件事：

```rust
// 节选（保留与硬件状态顺序相关的行；注释有删节，`asm!` 块按等价的单行写法重排），os/arch/src/x86_64/paging.rs:X86_64Paging::enable。
            const MSR_EFER: u32 = 0xC000_0080;
            const EFER_NXE: u64 = 1 << 11;
            let mut efer: u64;
            asm!("rdmsr", in("ecx") MSR_EFER, out("rax") efer, out("rdx") _, options(nostack));
            asm!("wrmsr", in("ecx") MSR_EFER, in("rax") efer | EFER_NXE, in("rdx") 0u64, options(nostack));
            // Switch CR3 to our own root page table.
            asm!("mov cr3, {}", in(reg) self.root_paddr);
            // Ensure WP (Write Protect, CR0 bit 16) is set so the kernel
            // cannot write read-only pages (W^X enforcement). UEFI firmware
            // does not guarantee WP is enabled.
            let mut cr0: u64;
            asm!("mov {}, cr0", out(reg) cr0);
            cr0 |= 1 << 31;
            cr0 |= 1 << 16;
            asm!("mov cr0, {}", in(reg) cr0);
            if cpu_supports_pge() {
                let mut cr4: u64;
                asm!("mov {}, cr4", out(reg) cr4);
                cr4 |= 1 << 7;
                asm!("mov cr4, {}", in(reg) cr4);
            }
```

顺序是：置 `EFER.NXE` → 写 `CR3` → 置 `CR0.PG` 与 `CR0.WP` → 按能力置 `CR4.PGE`。（注意 x86_64 的 UEFI 链进来时固件分页已在运行：这一步实际生效的动作是"换 `CR3` 根"，`CR0.PG` 的置位是与"固件未开分页"形态（如 C 的 BIOS 引导链）统一的写法——本仓生产链不存在那条 BIOS 链，此处指 C 侧语义的统一，不是本仓有一条 BIOS 产线；`NXE` 必须最先的理由见下一条。）

- `EFER.NXE` 必须最先：`NX` 是页表条目里的最高位（bit 63），它的使能开关在 `EFER` 这个 MSR 的 `NXE` 位（bit 11）上；`EFER.NXE = 0` 时任何带 NX 的页表条目在走表时被视为保留位，触发页错误且错误码 bit3（保留位）为 1。内核的 Direct Map 叶与用户数据/栈叶都带 NX，所以准确的要求是"第一条带 NX 的条目被使用之前必须先置 NXE"（放进 `enable` 即可满足）。注释指出 UEFI 交接时恰好置了它，但这条不变量被当作"我们自己的"，不依赖固件。
- `CR3` 写的是内核自己的根：`build_bootstrap_root_and_enable` 返回前会把根页表物理地址记进内核全局（`set_current_root_phys(root_page)`），后续阶段（如装载 VM 的 ELF 时给同一张表加映射）通过 `Paging::from_active_root` 取回一个句柄。`from_active_root` 与 `new_from_page` 的区别是后者会清零根（`write_bytes(ptr, 0, 512)`），前者不会。
- `PGE` 与 C 的 `vm_enable_paging` 对应（2.6 的 `pgeok`）：只在 CPU 支持时置，且必须在 `PG` 之后。

五步与 C 的一一对应及差异，见 §4.9 的函数对照表（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五行），这里不重复。表外还有 Step 2b（aarch64 设备 MMIO 窗口的高半别名）与 Step 2c（x86/riscv 设备窗口装入自举根）——C 没有对应物：x86 的串口是端口 I/O，ARM 侧是纯 MMIO、必须走内存映射；LAPIC/IO-APIC 与 PLIC/UART 在启动期就要经固定高半 VA 访问。

**若恒等映射只覆盖可用内存、或地址对称时照做第二步**（两处）：

其一，恒等映射只覆盖 `memmap` 里报告的可用内存。UEFI 的加载器代码与数据可以落在 `LOADER_CODE`/`LOADER_DATA` 这类非 conventional 内存里；`enable()` 写完 `CR3` 的下一条取指就在这类内存中，而页表里没有它——页错误发生在 `enable()` 内部，此时页错误处理程序（IDT 与异常向量要到 `kmain` 才建立）还不存在；`EarlyConsole` 虽然已经能打印建表路标（§6.1 的 `step1+2`/`step4`），但没有能接住缺页的异常路径，现场仍是"开启分页后立刻静默停止"。

其二，`kern_virt == kern_phys` 时照做 Step 2。Step 1 已经把这个地址范围建好（块尺寸 `kern_huge`，设备窗口所在的块按 2MiB 分片）；第二次按 Step 2 自己的尺寸与标志再写一遍——通常与 Step 1 的块尺寸相同，但设备窗口分片过的槽位例外——在 x86-64 上可能只是重复写同一个值，但在 riscv64 上，"同一个 VPN[2] 槽位写两次"意味着第二次的叶尺寸可能与第一次不同（1GiB 与 2MiB 梯度的差别）——一条 1GiB 叶与一条"指向下级页表"的条目互斥，先写的那一条会被覆盖，恒等映射覆盖范围随之改变，跳转到内核高地址之后启动信息里记录的 `kern_phys_base` 仍然可用而镜像的一部分不再可访问。



**入口栈对齐与 SSE（承接 §2.2）**：Rust 侧不继承这个问题：三个镜像的入口 asm 显式把栈对齐到 16 字节，`jump_to_kmain` 另按 psABI 做 `sub rsp, 8` 调整。x86_64 内核镜像以 `x86_64-unknown-none` 构建，该目标默认不启用 SSE/SSE2（`rustc --print cfg` 只给出 `fxsr`）；boot-shim 的 `x86_64-unknown-uefi` 目标另在 `os/.cargo/config.toml` 显式加了 `-C target-feature=-sse,-sse2,-mmx`，两条腿的取舍分开记。

### 4.7 分配器换手的落地（→3.8）

三处写者与各自的自守卫在 3.8 已列出。这里补上"游标"的两种恢复方式与扣除表的读法。

同一份 `BootAlloc` 有两种进入方式：boot-shim 侧的 `init` 与跨镜像接收侧的 `resume`（内核侧原来还有一条"从 memmap 切兜底区间"的 `init` 路径，已删除）。两种方式的代码与语义：

```rust
// 代码引用（`init` 函数体完整；后续 `resume` 的文档注释与省略段见源文件），os/kernel/src/boot_alloc.rs。
    pub fn init(&self, base: u64, end: u64) {
        self.base.store(base, Ordering::Relaxed);
        self.next.store(base, Ordering::Relaxed);
        self.end.store(end, Ordering::Relaxed);
    }

    /// Resume a bump allocator whose region was already partly consumed by a
    /// previous stage (cross-image handoff). `next` is the boot-shim's
    /// post-build cursor; it is validated here rather than trusted.
    pub fn resume(&self, base: u64, next: u64, end: u64) {
        assert!(
            BumpRegion::resumed(base, next, end).is_consistent(),
            "BootAlloc::resume: inconsistent region base={base:#x} next={next:#x} end={end:#x}"
        );
        self.base.store(base, Ordering::Relaxed);
        self.next.store(next, Ordering::Relaxed);
        self.end.store(end, Ordering::Relaxed);
    }
```

- `init(base, end)` 把游标置回 `base`：跨镜像腿由 boot-shim 在进入内核之前设定区间（UEFI 两条路径都在 `prepare_and_enter_kernel_image` 内；U-Boot shim 在 `prepare_boot` 返回后）；两个直核面在自己的 `bootface`/`bootface_a64` 里初始化并注册。`minix-kernel` 自身不再调用它：Step 0 看到 `boot_alloc_region()` 为 `None` 时直接停机。
- `resume(base, next, end)` 从"已被消耗到的位置"继续：跨镜像路径必须用这一种，因为接收侧链接的是这份分配器的另一份副本（`resume` 的文档注释写明了这一点）。跨镜像接收侧的 `arch_boot_resume_v1`（三架构同一入口）在解码信封后、进入本架构委托壳之前调用它——三架构的直接委托壳都是 `resume_after_v1_decode`；aarch64 的该壳是一行包装，再转 `arch_boot_resume_high_half`，x86_64/riscv64 的壳直接进 `resume_additive`。

两种方式的次序约束：跨镜像路径上必须先 `resume_boot_pt_alloc`、再进各架构委托壳（aarch64 经 `arch_boot_resume_high_half`，x86_64/riscv64 直接进 `resume_additive`）。顺序反了的话，Step 0 会看到 `boot_alloc_region()` 为 `None`——兜底已删，后果是 fail-closed 停机，而不是把游标重置回某段 memmap 的起点。

扣除表的读法：`build_vm_handoff` 把"内核保留"与"引导已用"两类区间收进同一份记录（见 3.8 引用的表），其中 bump 区是**整段**；`VmBootAllocator` 的已用后缀（该分配器从每个区间末端向下消耗，已用部分是连续尾段——表里第二类）是同段内的另一笔。两份记录随交接页一起交给 VM，VM 在消费侧校验 `LiveBootstrap ∩ VM-free = ∅`——也就是说，"内核记账对不对"这件事不是靠 kernel 单方面声明，而是在 VM 侧再复核一次。

**两个 bump 实例共享一套算法**。U-Boot shim 的固定池从 `MODULE_REGION_BASE` 起按"根页 → 1024 页页表子区间 → 模块页 → arena"的顺序发放；内核镜像的 `BootAlloc` 只续用其中的页表子区间。两个实例**不共享游标**——合并曾被试过：shim 的游标跨过子区间末尾（模块与 arena 在子区间之外），`BootAlloc::resume` 直接看到 `next > end`，QEMU U-Boot 门在 3/6 个路标处 panic，合并方案已回滚。共享的是纯值核心 `minix_boot::BumpRegion { start, next, end }`：`alloc_pages` 的边界检查、`used_bytes` 与一致性谓词都只有这一份。编译期断言固定常量：池基址与大小按 4 KiB 对齐、`KERNEL_MAX_LMA_END <= MODULE_REGION_BASE`、根页 + 页表子区间 + `ARENA_CAP` 不超过池大小、`BUMP_END <= boot_dm_admissible_end()`；运行期断言实例区间：`prepare_boot` 分配后检查根页就是池首页、页表子区间紧接其后且不越池尾，`BootAlloc::resume`/`region` 用 `BumpRegion::is_consistent` 校验跨镜像三元组。编译期只断言常量，运行时输入的合法性不假装编译期已证。

**arena 生命周期合同**。`arch_boot_resume_v1` 的解码是 arena 的最后一次读取：七个载荷全部复制进内核 `.bss` store，`KernelInfo` 的切片指向这些 store，`arena_pa` 不保留。VM 自由页清单在 `kmain → build_vm_handoff` 才发布，严格晚于解码，于是 `LastRead(arena) ≺ Publish(VMFreePages)`——arena 页可以按生命周期回收，不做永久扣除。这条合同只对 arena 会落进 conventional 的腿有意义（U-Boot shim 与合成测试）；两条 UEFI 腿的 arena 是 `LOADER_DATA`→`ReservedMapped`，从不在 free list 里，回收与否对 VM 无影响。startup-object 记录仍保留 arena 的身份（记录的是"它曾经是什么"，不是"它还活着"）。宿主测试 `decoded_map_survives_arena_poison_and_reissues_the_arena_frames` 用真实序列化 arena 解码后整块 poison，再跑与 VM 相同的 `classify_regions`：自由清单与解码值一致，落在 conventional 区间里的 arena 页被重新发给 VM，内核镜像仍被扣除。若未来有人在跳转后读 arena，必须同时移动解码边界或把 arena 加进扣除表，否则 VM 可能发到一份活的 wire 镜像。

**去掉内核侧 `is_registered()` 守卫会怎样**：让 `boot_validate_and_prepare` 无条件 `register(boot_alloc::boot_pt_alloc)`。`register` 里先原子 `PT_CLAIMED.swap(true, AcqRel)` 抢写权，抢到的写者再写函数指针、最后 `PT_REGISTERED.store(true, Release)` 发布；debug 构建的 `debug_assert!(!claimed, ...)` 在第二次注册时失败，release 构建直接返回、保留第一个注册者。因此第二次注册不会改写函数指针：若第一个注册者是某个测试内核（它用了一个避开内核镜像的池），后续注册被拒绝，页表页仍来自测试内核的池。

### 4.8 跨镜像交接与直核自引导（→3.7）

三架构按"架构 × 链"五行并列（"交接路径=四"的归并口径见 §3.7 术语对账与本节表后括号）。分道代码在 `os/kernel-image/src/main.rs` 的 `rust_image_main`——注意不是 boot-shim 的 `main.rs`（那个文件的入口骨架在 §4.1 引过，分道臂不在其中）——本节按架构逐臂给出；先从 aarch64 的接收面看起：

```rust
// 节选（保留分道与 resume 调用；注释改述为机制陈述），os/kernel-image/src/main.rs:fn rust_image_main（aarch64）。
#[cfg(target_arch = "aarch64")]
fn rust_image_main(arena_pa: usize) -> ! {
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);
    // 两条链的分道：直核 `-kernel`（QEMU 观察到不设 `x0` 参数——非规范保证，仅本仓 QEMU 虚机实测；真板 `-kernel` 行为以板级文档为准）走自引导面；
    // boot-shim UEFI 链 x0=arena 物理地址走 v1 解码 + 高半 resume 面。
    if arena_pa == 0 {
        bootface_a64::bootface_a64();
    }
    early_console::use_high_mmio_window();
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (handoff v1 arena)\n",
    );
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)
}
```

（`bootface_a64()` 的返回类型是 `!`——自引导面正常不会返回；`arena_pa == 0` 是唯一走直核的取值，v1 链不会传 0。）

```rust
// 代码引用（保留两个 cfg 臂的完整分道；注释节选），os/kernel-image/src/main.rs:fn rust_image_main（riscv64，按交付形态分特性）。
#[cfg(all(target_arch = "riscv64", not(feature = "fw-riscv64-uboot")))]
fn rust_image_main(boot_hart: u64, dtb_phys: u64) -> ! {
    bootface::bootface(boot_hart, dtb_phys)
}

// U-Boot 接收镜像：a0 = boot-shim 的 enter_kernel_image_v1 跳转递来的 v1 arena
// 物理地址；同一 v1 解码与加性恢复，不重建 shim 已激活的根表。
#[cfg(all(target_arch = "riscv64", feature = "fw-riscv64-uboot"))]
fn rust_image_main(arena_pa: usize) -> ! {
    let _kernel_entry: usize = core::hint::black_box(KERNEL_ENTRY_ANCHOR as usize);
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (handoff v1 arena, riscv64)\n",
    );
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)
}
```

| 架构 | 谁建根表 | 谁开分页 | 交接值 | 内核入口 |
|---|---|---|---|---|
| x86_64 | boot-shim | boot-shim（`enter_kernel_image_v1` 内的 `enable`） | `BootHandoffV1` arena 物理地址（`rbx`） | 独立镜像高半 `_start` |
| aarch64（UEFI 链） | boot-shim | boot-shim（`enter_kernel_image_v1` 内的 `enable`） | `BootHandoffV1` arena 物理地址（`x0`） | 独立镜像高半 `_start` |
| aarch64（直核） | 内核镜像自身（[`bootface_a64`](<os/kernel-image/src/bootface_a64.rs:fn bootface_a64>)） | 内核镜像自身（`arch_boot` → `enable`） | 无入参（`x0=0` 分道，DTB 从 RAM 基址固定位置读）；模块来源是 `BootFileTable` 且表址**只有 DTB `/chosen` 一条通道**（riscv64 直核面原有的备用 `a2` 通道已在 D01 裁决项 3 删除；取件库复用 opensbi_helpers——§3.2 断言的证据落点） | 独立镜像 `_start` |
| riscv64（`-kernel` 直核） | 内核镜像自身（`bootface`） | 内核镜像自身（`arch_boot` → `enable`） | `a0`=hartid、`a1`=DTB；表址只从 DTB `/chosen` 取 | 独立镜像 `_start` |
| riscv64（U-Boot 一族 / shim 生产腿） | `riscv64-uboot-shim` | shim（`enter_kernel_image_v1` 内的 `enable`） | `BootHandoffV1` arena 物理地址（`a0`） | 独立镜像高半 `_start` |

（表的行按"架构 × 链"列——五条生产腿各占一行；上一条术语对账里说的"四种交接路径"按"谁执行装载与建根"归并：两条 UEFI 链与 U-Boot 一族的 shim 生产腿同属 v1 send/receive 一对（该族的直载测试链不参与交接路径计数），riscv64 与 aarch64 的直核面各自一类。）

"四条交接路径"是 §3.7 对账按**协议两段 + 直核两类**的切分（v1 send / v1 receive / riscv64 自引导 / aarch64 direct-core），不是腿数——落到腿上有五条（三条 v1 发送腿同时属于 send 与 receive 两段）。这些腿有两个收敛点，按腿各归其一：直核家族（`arch_boot` 三个 cfg 分支）收敛在 [`arch_boot_finish`](<os/kernel/src/lib.rs:fn arch_boot_finish>)——`arch_boot_impl` 建表后经它跳 `kmain`；跨镜像三腿收敛在 `arch_boot_resume_v1`——三架构都先经 `resume_after_v1_decode`（aarch64 的壳再转 `arch_boot_resume_high_half`，带两行路标），三者在 调用序合并后都委托到泛型体 `resume_additive`（validate/落 KernelInfo/记录根/establish DM 的同一组加性阶段在其体内），最后落到同一个 `jump_to_kmain`。发送侧没有"收敛到 arch_boot"——它的终点是跳独立镜像的入口指令：aarch64 用 `br {entry}`、riscv64 用 `jr {entry}`、x86_64 用一段 global-asm stub：它的两个 SysV 实参是 `(entry, arena_pa)`——`rdi` 收 entry、`rsi` 收 arena——stub 内 `mov rbx, rsi; mov rax, rdi; cli; cld; jmp rax`，所以接收侧看到的仍是 `rbx` = arena。§3.7 只引了 aarch64 的 asm；x86 的完整 stub 就是上面这一段。`arch_boot_resume_v1` 里的 `resume_boot_pt_alloc` 只是 `BootAlloc::resume` 的转发（§4.7 的两种进入方式之一；第三种“从 memmap 切兜底区间”的 `init` 已删除），注册不发生在它里面。

`enter_kernel_image_v1` 里跳转前的准备（建根、读游标、补运行期字段）与跳转写法（aarch64 `br {entry}` / riscv64 `jr {entry}` / x86_64 的 stub：实参与跳转序列见本节上段，不再重复）——§3.7 只给了 aarch64/riscv 的跳转。这里补一条与"谁建根"有关的约束：跨镜像路径下，`build_bootstrap_root_and_enable` 被 boot-shim 调用一次、生成的表**已经激活**；接收侧必须避免二次建表（3.7 的反向推演已给后果），而 `bootface` 路径下建表发生在内核镜像自身，`arch_boot` 走的是常规 Step 1–4。

两处实现现状：

- x86_64 的独立镜像入口已实现（接线状态见 §3.1.1）：`rust_image_main(arena_pa)` 从 `rbx` 取 arena 基址，调用 `arch_boot_resume_v1`；`_start` trampoline 按入口契约建栈、清 `.bss`（只碰 rdi/rcx）、`cli`/`cld`、把 `rbx` 转进 SysV 第一参数 `rdi`。内核镜像的产出与布局校验仍由 `check-layout.sh` 的 L0–L9 守住。
- riscv64 在 QEMU `-kernel` 直核场景下镜像按物理基址视图执行（源码注释把这一实测事实记作 `medany`）。"两基址相等"发生在哪类产物要说准：**QEMU 直核测试内核**按 VMA=LMA 平铺链接（如 `test-paging-enable-riscv64` 的 `link.ld`，其位置计数器一行即 `. = 0x80200000`——VMA=LMA 平铺），`kern_virt_base == kern_phys_base`，§4.6 的 Step 2 守卫（相等即跳过 Step 2）就是为它们写的；**生产直核镜像**与 U-Boot 接收镜像共用高半 VMA 的 `riscv64.ld`，两基址不等、守卫走正常分支。生产链（`bootface` + 文件系统 + 高半链接）走的是另一套布局。U-Boot 链的 `kernel-image`（启用 `fw-riscv64-uboot`）接收镜像与直核镜像共用同一份 `riscv64.ld`（高半 VMA `0xFFFFFFC000000000`、物理装载基址 `0x80200000`），差别只在入口分道特性与交给它的第一个参数；shim 侧则是一片独立的物理链接产物（VMA=LMA，见 `os/boot-shim/riscv64-uboot.ld`）。两条布局的基址常量以链接脚本为准，复算命令：`grep -n 'KERNEL_VIRT_BASE\|KERNEL_PHYS_BASE\|ENTRY' os/kernel-image/*.ld`（该命令对三份脚本各命中若干行，这里只摘 riscv64 的三个值： `KERNEL_VIRT_BASE = 0xFFFFFFC000000000`、`KERNEL_PHYS_BASE = 0x80200000`、`ENTRY(_start)`）。

**让 riscv64 照 aarch64 那样"先建表、再跳镜像"会怎样**。aarch64 的镜像入口之所以直核与 UEFI 两种场景都能用，靠的是入口里的一条分道：`rust_image_main(arena_pa)` 在 `arena_pa == 0` 时走自引导面（`-kernel` 直核下 QEMU 不设 `x0`）、非零时走高半 resume 面；x86_64 没有直核面，`0` 只作诚实失败。riscv64 的入口（`rust_image_main(boot_hart, dtb_phys)`）没有"交接指针"这个入参——直核场景只给 hartid 与 DTB，表址从 DTB `/chosen` 取；若照搬 aarch64 的协议而不补一条分道，空指针解引用会发生在 `early_console` 初始化之前（主语限定为**非 U-Boot 特性**的 riscv64 直核入口——`fw-riscv64-uboot` 构建的接收臂有 `arena_pa` 入参、已由分道处理，这个反事实只适用于直核形态，串口上没有任何输出。反过来，若 aarch64 退回内联执行，撞的是 §3.7 第一处替代做法已经推演过的同一个问题（TTBR0 被换掉、取指映射作废），现场证据（`elr`/`vbar` 钉在同一低地址）也在那里。

### 4.9 与 Minix3 C 的函数落点对照（差异表，非索引）

> 本节按"函数→落点"排，C 侧符号的权威索引在 §7.5；Rust 侧符号以本节与 §3 的定义节为准；冲突时按同一规则：C 侧看 §7.5，Rust 侧看定义节。

| C 函数（第 2 章） | Rust 落点 | 保留 / 改型 / 淘汰 / 新增 | 差异原因一句话 |
|---|---|---|---|
| [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) | [`fn main`](<os/boot-shim/src/main.rs:fn main>) | 改型 | 引导器跳转到内核入口 → 固件跳转到 boot-shim 入口 |
| `pre_init(magic, ebx)` | `prepare_and_enter_kernel_image`（UEFI 两条路径）与 `opensbi_helpers::prepare_boot`（U-Boot 路径） | 改型 | `pre_init` 的八条语句（§2.6.1 计数）被拆成固件入口与共享 builder |
| `get_parameters(ebx, cbi)` | `prepare_and_enter_kernel_image` + `ebs_pair::pair_and_classify` + `build_kernel_info`（仅直核面） | 改型 | 拷一份引导器结构体 → 调固件接口拿两份内存列表 |
| `mb_set_param(...)` | [`normalize_param_buf`](<os/libs/minix-boot/src/boot_params.rs:fn normalize_param_buf>) + [`param_buf`](os/libs/minix-boot/src/kernel_info.rs:param_buf) | 改型 | 统一分词/拒绝/整条丢弃规则（十条可观察分歧见 §3.4.2），ARCH 生成、BOARD 有来源才生成；UEFI 与 DTB 两源共用 |
| `add_memmap(cbi, addr, len)` | `ebs_pair::pair_and_classify` / `build_memmaps`（构建侧）；内核侧扣减为 [`FreeRangeSet`](os/kernel/src/vm_handoff.rs) 的区间集合（扣减换手后，C 形状账本模块删除） | 改型 | 构建侧改为配对循环分类；内核侧按有序不相交区间集合记账，行为由对拍测试锁等价 |
| `cut_memmap(cbi, start, end)` | [`FreeRangeSet::cut`](os/kernel/src/vm_handoff.rs)（`classify` 扣减步调用） | 改型 | 引导侧报告全部 DRAM，切除移到内核侧；C 的第三个参数是"结束地址"，Rust 集合的 `cut` 收 `start, len`——C 里 add/cut 参数形不一致（§2.4 那处 `alloc_lowest` 传错参数就出在这个差异上），Rust 侧统一为长度 |
| `alloc_lowest(cbi, len)` | 当前无对应物；低地址代码的拷入端已有（[`fn install_at`](<os/arch/src/x86_64/ap_early_entry.rs:fn install_at>)，地址由调用者给） | 暂未移植 | 唯一的消费者是 32 位 AP 启动跳板（SMP 归 SMP 篇（尚未生成））；`install_at` 的注释写明扫描空闲表找最低区间的动作归 SMP 篇的 `smp_init`，**重做时不得复刻 §2.4 那处"把长度当结束地址"的误用**（这条规则依赖的四个前提与各自的复查判据见 §2.4.1 的前提清单） |
| `overlaps(mod, n, cmp_mod)` | 防线 A/B/C/D：A `first_source_destination_overlap`（源 × 目标）、B `first_overlapping_pair`（内核段间）、C `alloc_module_pages`/`assert_modules_clear_of_kernel`（模块 × 内核）、D `KernelInfo::validate`（模块 × 模块） | 改型 | C 的一个函数在 Rust 侧拆成四道防线（§4.3）；D 在接收侧复核 |
| `pg_clear()` | [`X86_64Paging::new_from_page`](os/arch/src/x86_64/paging.rs:X86_64Paging::new_from_page) 的清零 | 改型 | 根页由调用者提供 |
| `pg_identity(&kinfo)` | [`build_bootstrap_root_and_enable`](os/kernel/src/lib.rs:build_bootstrap_root_and_enable) Step 1 | 改型 | 范围与页尺寸由常量与阶梯决定 |
| `pg_mapkernel()` | 同函数 Step 2 | 改型 | 返回值不再被使用 |
| `pg_load()` | [`X86_64Paging::enable`](os/arch/src/x86_64/paging.rs:X86_64Paging::enable) 的 `mov cr3` | 保留（同一动作） | 硬件动作不变 |
| `vm_enable_paging()` | 同函数 `enable` 的 `EFER.NXE` + `CR0` + `CR4` 段 | 改型 | 多了 NXE；少了 `PSE` |
| `pg_alloc_page(cbi)` | 无同名对应物：4KB 被映射数据页的运行期分配归 VM `phys_mem`；引导期页表页见 `alloc_pagetable` | 职责转移 | 从记录尾部取页 → 运行期由 VM 物理分配器承担 |
| `alloc_pagetable(&ph)` | [`fn alloc_pt_page`](<os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page>) + 三架构实现 | 改型 | 6 页静态池 → 注册式分配器 |
| `pg_map(phys, vaddr, vaddr_end, cbi)` | [`X86_64Paging::map`](os/arch/src/x86_64/paging.rs:X86_64Paging::map)（4KB 粒度） | 保留（语义） | 4KB 细粒度映射的用途不变 |

这张表里 `alloc_lowest` 一行标的是"暂未移植"，它也不是"因为不需要所以删掉"：`alloc_lowest` 的服务对象（把 16 位启动代码放到 1MB 以下）属于 AP 启动，而 64 位侧的对应机制（AP 停车邮箱 + 启动记录）在 [`smp.rs`](os/kernel/src/smp.rs) 与 [`main.rs`](os/kernel-image/src/main.rs) 的 `park` 段里，不经过"从记录最低处分配"这条路径。

**把 C 的静态页表池原样搬到 64 位会怎样**（保留"6 页，用完 panic"；这段属 §3.8 的分配器换手，放在 §4.9 表后只为对照 `alloc_pagetable` 行，不是本表的 C→Rust 落点）。`PG_PAGETABLES = 6` 是两级页表的预算：32 位下 `pg_map` 一次最多挂一个页表页，6 页够用。4 级页表下一次 4KB 映射要穿过 PML4 → PDPT → PD → PT 四级，其中下面三级都可能需要新建页表页；实际需要的页数随地址分布变化（映射分散时每个 PML4 槽都要一条新链）。固定 6 页按每个 PML4 槽最多 3 页算只够两个槽，第三个槽（第二次跨 PML4 边界）就会耗尽，表现为 `panic("no more pagetables")` 落在启动中途——而"6 页"这个数字在 64 位下没有任何依据。

---

## 5. 测试：先推导需要证明什么，再看覆盖（独立评测方视角）

这一章的写法与前几章相反：不从现有测试出发，而是**从本篇讲过的机制出发**，逐条推导"要证明它成立，需要什么可观测、可证伪的断言"，然后标注覆盖状态。**已登记**的未覆盖项以 **TODO** 形式登记——它们是对源码工作流的**需求输入**（唯一登记处是 5.3 的表）；少数缺口当前**没有**登记 TODO（M3 的运行时"不向清单领页"断言、M6 的全量 4GiB 叶子映射真机走查——理由分别见 §5.1 的 M3/M6 行与 §5.4）。

三类运行期证据来源：**宿主断言**（镜像还没运行就能查）、**宿主测试**（纯逻辑与接口契约）、**真机链**（QEMU 串口日志断言）；另有一类**引文与源码断言**（C 源逐字引文、C/Rust 调用顺序与源码内断言，不是本仓执行面，M1、M4 与 M16 用到）。类型列里的 `＋` 表示多类共同支撑、`→` 表示先宿主后真机；`调用序证据`/`静态检查`/`源码` 归入引文与源码断言类。

### 5.1 机制 → 需要证明什么（推导表）

| # | 机制（正文位置） | 需要证明什么（可观测、可证伪） | 证据类型 | 覆盖状态 | TODO |
|---|---|---|---|---|---|
| M1 | 第一跳设栈与传参（§2.2，C 侧） | 内核入口 esp 指向 `load_stack_start`；`pre_init` 收到 `(magic, ebx)` | 引文对账 | 引文块逐字对账（C 侧无执行面，不在本仓测试面）；真机链只覆盖 Rust 等价路径的第一跳，不覆盖 C 侧 `head.S` | — |
| M2 | 内存清单的建与裁（§2.4，Rust 等价物） | `add_memmap` 接受任意 `addr`/`len`，写账前起点向大、长度向小取整；零长区间静默丢弃；`addr` 超过 `LIMIT` 整条丢弃，跨 `LIMIT` 截断；`cut_memmap` 起点向下、终点向上取整（Rust 侧非对齐输入原样保留、取整发生在 `FreeRangeSet::cut`；C 的 `<4GB` 硬截断在 Rust 侧没有同形截断——整图按页表物理上限 `pa_limit` 直接作废（`PolicyError::OutOfPhysRange`，经 `MapError::Policy`/`RejectedDescriptor` 上抛；只对到达 `checked_len` 的描述符生效，`Excluded`/`DevicePath` 在 Step 3 早退、不经过它），DM 窗口上界只约束自举分配） | 宿主测试 | **已覆盖（含替换口径）**：扣减由 [`vm_handoff.rs`](os/kernel/src/vm_handoff.rs) 的 `FreeRangeSet` 承担——行为契约由 9 条 vm_handoff 侧测试（行为锁 + "与参考区间减法逐页等价"对拍，对拍即其中一条）锁定（外向取整、零长跳过、中切分、容量 fail-closed）；非对齐输入保持原样、取整在 `cut` 时发生（对拍锁定），DM 窗口裁剪在 `classify` 后段；`KernelInfo::validate` 自身断言 memmap 有序、不相交、非空、base 与 len 均页对齐，还断言 memmap 与 reserved_regions 两张表互不重叠（交叉校验，防止 PMM 发出保留帧；源码注释把该设计项记为 R16 DESIGN-10，R17 实施批落码），另断言内核镜像与各模块、模块与模块之间按半开区间互不重叠（相切放行；R18 在接收侧补的第二道复核）；`FreeRangeSet` 在建集合时做排序归并，两者分工见 §5.2.1。C 的 `<4GB` 硬截断没有同形断言。旧 `memmap.rs` 17 条随模块删除，其断言被行为锁与对拍覆盖 | — |
| M3 | 清单先于建表（§1.2/§2.8，Rust 侧） | Step 0/1 不向清单领页：兜底删除后，`boot_alloc_region()` 为 `None` 直接停机，不存在"从清单切 256 页"的分支；Rust 的页表页来自注册分配器（bump/VM），不向 memmap 取页；4KB 数据页的运行期分配由 VM `phys_mem` 承担 | 真机＋负控 | 源码面：`require_boot_region` 的 fail-closed 负控 `missing_boot_allocator_registration_fails_closed`；真机路标 `kernel: step1+2 mappings ok` 证明建表跑通；"不向清单领页"仍无独立运行时断言（源码序钉覆盖） | — |
| M4 | **深拷先于一切动用空闲清单者**（§4.5 不变量） | 六个入口在 `store_kernel_info` 之前不向空闲清单领页、不消费空闲清单（`validate_before_tables` 的"memmap 非空"校验是允许的读取）；交接分类点读 `KernelInfo` 切片非零 | 真机＋调用序证据 | 宿主测试 `test_store_kernel_info_lands_firmware_slices`（落地区行为）+ 调用序钉 `os/kernel/tests/store_before_alloc_pin.rs`（调用序合并后钉在两个泛型体上：`arch_boot_finish` 与 `resume_additive` 的源码序 store 先于建表/establish；六个入口委托壳（3 个 `arch_boot` + aarch64 `arch_boot_resume_high_half` + x86/riscv 两个 `resume_after_v1_decode`）钉"必须经泛型体"；R2 补：`arch_boot_resume_v1` 的三步序 validate < `resume_boot_pt_alloc` < 委托亦入钉——此前该次序无钉，resume 腿的注册前置靠的是无钉守卫；兜底删除后该守卫变为硬失败）。另有一条真机探针读数（§3.4 的深拷探针，`pa=0x5e9b000` / 三次读数 / `verdict=replayed`）：它只排除候选①，候选②未排除，故不构成完整覆盖。运行期钩不可行：QEMU 测试内核直接调 `arch_boot_impl` 分阶段验证，无法区分生产入口与测试 | TODO-01（已关闭，见 5.3） |
| M5 | 页表建立与开分页（§2.6 C / §4.6 Rust） | 开分页后的下一条取指有映射（能打印第一个内核路标） | 真机 | 真机链覆盖（现行实际映射档 = 2 MiB；1 GiB 首选档未跑，即 TODO-05） | TODO-05（开放，见 5.3） |
| M6 | 恒等映射覆盖低 4GB 固定上界（§4.6 Step 1） | `IDENTITY_MAP_END` 以下全部有映射、标志为 supervisor-only | 宿主测试＋真机 | **部分覆盖**：宿主侧 `identity_piece_policy_covers_low_4gib_and_marks_device_windows` 在合成窗口与 1 GiB 块配置下验证策略函数自洽（不跑生产 Step 1；x86_64 宿主的生产窗口表为空）；`paging.rs` 的 16 条里只有一部分（`map_huge` 能力/编码）与 M6 相关，其余是通用页表层。**全量 4GiB 实际叶子映射**的缺口与页尺寸档位不是同一件事；§5.3 的 TODO-05 只登记 1 GiB 首选档的真机启动，全量走查当前未登记 | — |
| M7 | 分配器"写一次"（§3.8） | 二次 `register` 在 debug 构建失败、release 构建拒绝改写 | 宿主测试 | **已覆盖**：[`pt_alloc_write_once.rs`](os/arch/tests/pt_alloc_write_once.rs) 2 条（`register` / `register_free` 各一：debug 下二次注册 panic，release 下被拒且指针不变） | TODO-02（已关闭，见 5.3） |
| M8 | 落地区容量与 fail-fast（§4.5） | 超过容量时快速失败而非截断 | 宿主测试 | **已覆盖**：落地区行为（`test_store_kernel_info_lands_firmware_slices`）+ 七枚超容量负控（`test_store_kernel_info_rejects_{reserved,memmap,boot_module,platform,param,source,startup}_overflow`） | TODO-03（已关闭，见 5.3） |
| M9 | bump 分配器（§3.8/§4.2） | 顺序推进、耗尽拒绝、页对齐、reinit 后归位、跨镜像区间自洽 | 宿主测试 | **已覆盖**：[`boot_alloc.rs`](os/kernel/src/boot_alloc.rs) 10 条 | — |
| M10 | 布局契约（§5.2 与 §3.6） | L0–L9 十条主编号（L0–L8 全架构 + L9 仅 riscv64） | 宿主断言 | **已覆盖**：`check-layout.sh`（全量，见 5.2.2） | — |
| M11 | 模块顺序契约（§4.3） | 12 项按 `MODULE_NAMES` 顺序返回、缺件 panic；装载前两道防线：目标 LMA 两两不相交（`first_overlapping_pair`）、文件字节区间不与任何目标 LMA 相交（`first_source_destination_overlap`） | 宿主测试＋真机前置检查 | **已覆盖**：`loader.rs` 顺序/缺件测试＋真机链的 `mdir` 前置 | — |
| M12 | EBS 阶段边界（§4.1） | EBS 之后不再出现固件调用；裸串口路标仍输出 | 真机＋静态检查 | **部分覆盖**：门脚本至少断言 `[raw] entering kernel image v1` 一条；aarch64 按形态断言三条之一：`memmap source` 行、无 dumpdtb 变体、或 `DTB_CFGTABLE=1` 的 DTB preflight 拒绝行（后者没有 memmap source 行），x86 的五条标记含 `entering scheduler`；退出本身那条与 `[raw] arena=…` 仍靠日志人工看。静态检查：文本级静态检查 `os/boot-shim/tests/ebs_boundary_audit.rs` 覆盖生产入口（`prepare_and_enter_kernel_image` 的配对循环成功标记之后），零 `uefi::`/分配/释放/drop，带三枚注入负控（`uefi::`、`allocate_pages`、嵌套块尾部）与注释豁免自检 | TODO-04（已关闭，见 5.3） |
| M13 | UEFI 侧快照函数（§4.2：`build_memmaps`、`assert_bootstrap_outside_memmap` 等） | 两列表对"可分配 / 保留"两档互斥且完备（`Excluded`/`DevicePath`/`Side` 按设计不进表）；root/bump 不与 free 列表相交 | 宿主测试→真机 | **部分覆盖**：`assert_bootstrap_outside_memmap` 的 5 条宿主测试（清单见 §5.3 TODO-07）；`build_memmaps`/`alloc_root_page`/`alloc_bump_region` 调用点单根（Boot Services），宿主无法构造 | TODO-07（部分关闭：纯函数五案已交接，其余开放，见 5.3） |
| M14 | 交接三形态（§3.7/§4.8） | aarch64 resume 只做 additive（不重建根）；`x0` 分道（空指针→自引导面）；x86 shim 三路标可见 | 真机＋引文 | **部分覆盖**：aarch64 UEFI 链只到 EBS 后的 `memmap source` 行，三条标记都在 `entering kernel image v1` 之前，脚本不判内核随后是否存活，因此"resume 只做 additive"没有真机证据（§5.2.3 已声明该分工）；riscv64 两条覆盖"交接序列走通且路标出现"；x86 `test-cmd-smoke.sh` Stage 3b 已断言三条 shim 路标（`kernel loaded` / `12 boot modules loaded` / `[raw] entering kernel image v1`） | TODO-06（已关闭，见 5.3；aarch64 additive 的缺口记在 §5.4） |
| M15 | 失败路径防线（§4.2 步骤 8） | root/bump 与 free 列表相交时立刻 panic（配对循环成功后、裸串口可打印） | 真机 | 断言在源码内、每次启动都执行 | — |
| M16 | 内核镜像重叠防线（§4.2：`record_kernel_span`、`alloc_module_pages`、生产入口重查） | 模块页分配落在内核镜像物理跨度之外（拷贝前拒绝）；装载后按页取整重查模块，并复核 root/bump/arena 均不与内核重叠 | 真机＋源码＋宿主测试 | 断言助手为纯函数（`assert_range_clear_of_kernel` / `assert_modules_clear_of_kernel`），5 条宿主测试覆盖相离/相切、包含、部分重叠、内核在区间内、模块字节越界；UEFI 生产入口每次启动执行：内核布局确定后、拷贝前记录跨度；`alloc_module_pages` 拷贝前拒绝；装载后按页取整复审模块并复核 root/bump（防御性；可达失败在 `alloc_module_pages` 拷贝前），`prepare_and_enter_kernel_image` 另查 arena；`alloc_module_pages` 的 `AllocatePages` 分配面宿主不可构造（Boot Services）。其余腿：U-Boot shim 的 `opensbi_helpers::prepare_boot` 在装载前经 `load_kernel_with_loader_gated` 加了一道门——从 ELF 程序头算出 LMA 跨度，先断言它不越过链接期 4 MiB 上界（`KERNEL_MAX_LMA_END = KERN_PHYS_BASE + 0x40_0000`，另有编译期断言 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE`），再断言它不与 bump 池相交，任一失败都在**第一个段字节写入之前**停机；装载后的 root/bump 复核保留为第二道防线。UEFI 之外的三条腿没有同形的运行期"目标跨度拒绝"：U-Boot 腿的 `alloc_module_pages` 是纯 bump 分配，由编译期池不相交断言与装载前 gate 覆盖；两个直核面的 `place_modules` 另有"源区间 × bump 池"的拷贝前检查，目标侧同样由编译期池不相交关系覆盖 | — |
| M17 | ARM32 闭区间 `overlaps`（§2.7） | 闭区间只在 8MB 槽距、`mod_end` 各留一页的固定布局下无害；换成会相切的布局即误报 | 引文对账 | **未覆盖**：无运行期断言，§2.7 只有源码对照 | — |

「待源码侧确认」的覆盖状态**不推测**——拿不准的写"待确认"，由源码工作流核实后回填。

TODO 列的状态口径**以本表为唯一权威**：`TODO-n（已关闭）`指该项曾是 TODO、后由宿主测试或真机链覆盖；`TODO-n（开放）`指仍未覆盖；`—`指该机制从未登记过 TODO；`TODO-n（部分关闭）`指该 TODO 的若干子项已由测试或门链覆盖、其余仍开放，子项划分写在该行括注与 5.3 的对应行。5.3 只放详情与交接记录，不另立状态口径。

### 5.2 覆盖现状：三层证据各自证明什么

#### 5.2.1 宿主测试层（纯逻辑与接口契约）

宿主测试跑的是纯逻辑与接口契约，真实启动链要 QEMU。这一层与真机层的分界线可以逐条列出。

可复制命令与实测输出关键行（下面两段是实际的命令与输出，预期输出用 `test result:` 行核对）：

```bash
$ cd os && cargo test -p boot-shim --features test-all --lib
...
test result: ok. 135 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cd os && cargo test -p minix-boot -p minix-elf
running 192 tests
test result: ok. 192 passed; 0 failed; ...
running 23 tests
test result: ok. 23 passed; 0 failed; ...
```

测试数口径：本节所有数字按"`#[test]` 出现次数"计，含 `#[should_panic]` 标注的条目，不含构造辅助函数与 mock 类型；下面的逐文件表与上面的 `cargo test` 转录来自同一天实跑（输出按 boot-shim / minix-boot / minix-elf 分组重排，数字逐字来自实跑；`for` 列表的枚举顺序不参与输出顺序）。逐文件复算：

```bash
$ for f in os/boot-shim/src/loader.rs os/boot-shim/src/opensbi_helpers.rs \
           os/boot-shim/src/uefi_helpers.rs os/libs/minix-boot/src/kernel_info.rs \
           os/libs/minix-boot/src/platform.rs \
           os/libs/minix-boot/src/boot_params.rs \
           os/libs/minix-boot/src/memmap_source.rs os/libs/minix-boot/src/handoff_v1.rs \
           os/libs/minix-boot/src/handoff_v1_write.rs os/libs/minix-boot/src/handoff_v1_read.rs \
           os/libs/minix-boot/src/fdt_layout.rs os/libs/minix-boot/src/lib.rs \
           os/libs/minix-boot/src/bump.rs os/libs/minix-boot/src/boot_storage.rs \
           os/libs/minix-elf/src/lib.rs \
           os/boot-shim/src/ebs_map.rs os/boot-shim/src/ebs_pair.rs \
           os/boot-shim/src/dtb_copy.rs os/boot-shim/src/resv_claims.rs \
           os/boot-shim/src/lib.rs os/boot-shim/tests/ebs_boundary_audit.rs; do
  printf '%-50s %s\n' "$f" "$(grep -c '#\[test\]' $f)"; done
os/boot-shim/src/loader.rs                         22
os/boot-shim/src/opensbi_helpers.rs                11
os/boot-shim/src/uefi_helpers.rs                   12
os/boot-shim/src/ebs_map.rs                        44
os/boot-shim/src/ebs_pair.rs                       17
os/boot-shim/src/dtb_copy.rs                       15
os/boot-shim/src/resv_claims.rs                    13
os/boot-shim/tests/ebs_boundary_audit.rs           5
os/boot-shim/src/lib.rs                            1
os/libs/minix-boot/src/kernel_info.rs              28
os/libs/minix-boot/src/platform.rs                 8
os/libs/minix-boot/src/boot_params.rs              8
os/libs/minix-boot/src/memmap_source.rs            74
os/libs/minix-boot/src/handoff_v1.rs               2
os/libs/minix-boot/src/handoff_v1_write.rs         10
os/libs/minix-boot/src/handoff_v1_read.rs          21
os/libs/minix-boot/src/fdt_layout.rs               29
os/libs/minix-boot/src/lib.rs                      2
os/libs/minix-boot/src/bump.rs                    5
os/libs/minix-boot/src/boot_storage.rs            5
os/libs/minix-elf/src/lib.rs                       23
```

22 + 11 + 12 + 44 + 17 + 15 + 13 + 1 = 135（`--lib`），另有集成测试 `tests/ebs_boundary_audit.rs` 5 条；与 `cargo test -p boot-shim --features test-all --lib` 的实测一致（默认 uefi-only 形态只编 loader + uefi + ebs_map + ebs_pair + dtb_copy + resv_claims + lib.rs：135 − `opensbi_helpers` 11 = 124；135 这个读数必须带 `test-all`）；`minix-boot` 的 192 = `kernel_info.rs` 的 28（含 R17 的三条后置负控、R18 的两条跨度复核负控、R19/R20 的三条跨度溢出负控与 R21 的一条长度对齐负控） + `platform.rs` 的 8 + `boot_params.rs` 的 8 + `memmap_source.rs` 的 74 + `handoff_v1.rs` 的 2 + `handoff_v1_write.rs` 的 10（本轮删双写测试 −1） + `handoff_v1_read.rs` 的 21 + `fdt_layout.rs` 的 29 + `lib.rs` 的 2 + `bump.rs` 的 5 + `boot_storage.rs` 的 5。（历史读数与逐次增删见 git 记录；当前值以本逐文件表为准：minix-boot 192、boot-shim 135。）`minix-elf` 的 23 与实测一致。

推导表（5.1）里引用的其余宿主测试层（kernel 侧，`cargo test -p minix-kernel --features mock` 形态）：[`boot_alloc.rs`](os/kernel/src/boot_alloc.rs) 10 条、[`vm_handoff.rs`](os/kernel/src/vm_handoff.rs) 的 classify/FreeRangeSet 系列（扣减换手后含 9 条行为锁与对拍；旧 `memmap.rs` 17 条随模块删除）、[`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 10 条、[`paging.rs`](os/arch/src/x86_64/paging.rs) 16 条（属 **minix-arch** 包，不在 `minix-kernel --features mock --lib` 的 870 条（`cargo test -p minix-kernel --features mock --lib`，本轮实测：862 passed + 8 ignored）里；可跑形态需 `--features "x86_64 runtime-window"`，CI 当前未采集）、[`lib.rs`](os/kernel/src/lib.rs) 的 `store_kernel_info` 系列 8 条（1 枚落地区行为 + 7 枚超容量负控）。[`pt_alloc.rs`](os/arch/src/arch/pt_alloc.rs) 自身为零，其单例契约由集成测试 [`pt_alloc_write_once.rs`](os/arch/tests/pt_alloc_write_once.rs) 2 条覆盖（单元测试二进制里 paging.rs 的 destroy 测试已占用同一全局单例，无法在其中再测注册）；v1 协议的布局门禁由 [`handoff_v1.rs`](os/libs/minix-boot/src/handoff_v1.rs) 的编译期断言与 2 条尺寸/偏移测试覆盖。

列出的测试各自挡的是哪条契约（选列，未逐条穷举）：

| 测试（文件名:函数名） | 挡的契约 |
|---|---|
| `loader.rs:test_load_boot_modules_loads_all_in_contract_order` | 12 项按 `MODULE_NAMES` 顺序返回（顺序契约） |
| `loader.rs:test_load_boot_modules_panics_on_missing` | 缺件必须 panic 而不是跳过（用 `catch_unwind` 观测） |
| `loader.rs:test_first_overlapping_pair_{disjoint,overlap,abutting,empty,returns_first}` | 半开区间的重叠判定（`abutting` 那一条正是 §2.5 的 `-1` 语义） |
| `loader.rs:test_first_source_destination_overlap_{detects_cross_segment,ignores_zero_length}` | 跨段源破坏（源缓冲落进任一目标跨度）与零长段的豁免（防线①） |
| `loader.rs:test_bss_region_is_zeroed_after_load` | 段余量清零到 `memsz`、且不越过 `memsz`（实现分析归 02 篇 §4.2，此处只登记它的存在） |
| `loader.rs:test_compute_kernel_layout_from_mock_elf` | 布局派生（`min(vaddr)`、`min(paddr)`、`span`） |
| `opensbi_helpers.rs:test_bump_alloc_rejects_zero_pages` | `bump_alloc(0)` 必须拒绝（否则返回重复地址） |
| `opensbi_helpers.rs:test_bump_alloc_advances_pointer` | 连续分配严格递增 |
| `opensbi_helpers.rs:test_boot_file_table_validates_magic` | 魔数不匹配即拒（防 `a0` 指错） |
| ~~`opensbi_helpers.rs:test_build_kernel_info_bootstrap_zero_means_no_reclaim`~~ | 字段删除时一并移除——过度回收在类型上不可复发，无测试可保护 |
| `opensbi_helpers.rs:test_build_kernel_info_riscv64_fields_match_input` | 字段逐一映射（含派生字段 `kern_stack_top`/`syscall_entry`） |
| `uefi_helpers.rs:test_build_kernel_info_fields` | UEFI builder 的字段装配（该文件 12 条单元测试之一） |
| `boot_params.rs:generates_arch_and_board_first` 等 8 条 | 参数规范化：生成键、NUL/空白分词、内部键拒绝、非 ASCII/无等号拒绝、重复键首条胜出、整条丢弃 |
| `kernel_info.rs:test_validate_rejects_*`（16 条）与 `test_validate_accepts_*`（3 条），另加 `test_validate_requiring_sources_*`（4 条）、`test_postconditions_reject_*`（4 条）与 `test_getters_return_field_values`（1 条）= 该文件 28 条 | `validate()` 的全部不变量（含 D2-05 增的有序/不相交与非空/页对齐，两张表各扫一遍）与它们的合法边界；strict 门、postconditions 上界、getter 与字段一致 |


M13 的覆盖状态见 §5.1；TODO 详情见 §5.3：UEFI 侧四个函数里，纯判定函数 `assert_bootstrap_outside_memmap` 已有五案宿主测试（清单见 §5.3 TODO-07）；其余三个（`build_memmaps`、`alloc_root_page`、`alloc_bump_region`）的调用点单根（`main.rs` 的 [`fn main`](<os/boot-shim/src/main.rs:fn main>) → Boot Services），宿主无法构造；明细与去向见 5.3 的 TODO-07。

**把缺件处理改回静默跳过会怎样**：`load_boot_modules_into` 里换成 `if let Some(data) = loader.read(&path) { ... }`。`test_load_boot_modules_panics_on_missing` 立刻变红——这条测试的可观测形式正是 panic，它挡的正是"错误被推迟到内核侧"那条路；那条链的完整走向（内核侧只剩"期望 12、实际 N"）在 §4.3 已经给过。

#### 5.2.2 宿主断言层（镜像还没运行就能查）

`check-layout.sh` 把"boot-shim 装载内核时要读的字段"在宿主上判定完。可复制命令与退出码：

```bash
$ bash os/kernel-image/check-layout.sh all      # 或 x86_64 / aarch64 / riscv64
$ echo $?
0     # 全部断言通过；1 = 任一断言失败；2 = 用法/环境错误
```

断言清单（脚本头部注释逐条列出 L0–L8；L9 的说明在实现处、只在 riscv64 分支执行）：

| 断言 | 检查什么 | 防哪种漂移 |
|---|---|---|
| L0 | 脚本预期表里的两个基址 = 链接脚本里声明的两个基址 | 两处手写常量漂移（断言对象是"工具与真源的关系"） |
| L1 | ELF 类型 = `EXEC` | 内核装载面没有重定位处理器，`static-pie` 不可用 |
| L2 | `e_machine` 与预期三元组相符 | 拿错架构工件（用 x86 工件当 aarch64 用） |
| L3 | 首段 `PT_LOAD` 的 VMA/LMA = 该架构预期基址 | 布局常量与链接脚本不符 |
| L4 | 所有 `PT_LOAD` 的 `(vaddr - paddr)` 是同一个平移量 | "一个 `kern_virt_base ↔ kern_phys_base` 平移"的前提 |
| L5 | 镜像跨距 2MiB 对齐 | `plan_kernel_mapping` 的页尺寸选择至少能落进回退档 |
| L6 | 入口等于首段 VMA，且 `_start` 符号等于入口、落在首段内 | 入口符号与段布局脱节 |
| L7 | 引导栈符号存在、跨度 64KiB、落在末段内（不检查自顶向下方向）——这是宿主侧的弱检查；sp 初值与数组顶的真实关系（含收口填充余量）由 ld 脚本的排布保证，见 §4.4 字段表 `kern_stack_top` 行 | 栈符号与镜像范围脱节 |
| L8 | `minix_kernel::arch_boot` 符号在镜像里 | `--gc-sections`/归档粒度把内核启动图裁出镜像 |
| L9（riscv64） | ELF 实测跨距 = `bootface.rs` 里的 `KERN_SIZE` | 镜像布局常量与自引导面的期望值漂移 |

L4 与 L5 分别对应 02 篇的段平移结论与 §3.6 的大页阶梯：L4 保证"两套地址之间只有一个偏移量"这件事对每个段都成立，L5 保证"选出的页尺寸"有一个可用的档位。L8 是"链接保留"的可观测证据——`KERNEL_ENTRY_ANCHOR` 持有 `arch_boot` 的函数指针，链接脚本 `KEEP` 该节，两者一起挡住裁剪。

**去掉 L5（跨距对齐断言）会怎样**：一个跨距为 `2MiB + 4KiB` 的镜像在 `boot_validate_and_prepare` 里既进不了 1GiB 档也进不了 2MiB 档（`kern_size` 同时对两者都不整除——L5 检查的是**跨距**（全部 `PT_LOAD` 段的跨度），阶梯检查的是 `kern_size`（同一组段 `min(paddr)` 与 `max(paddr+memsz)` 之差）；无额外填充时二者相等，L5 因此保证阶梯至少落进 2MiB 档），落到 `KernelMapError::NoKernelPageSize`（启动入口把它转成可读 panic）。现场是"启动第一步就 panic"，根因在链接脚本的段间距；没有宿主断言就只能靠真机二分定位。

#### 5.2.3 真机层（各链各自证明什么）

**深拷探针读数（本仓 QEMU/OVMF）**：在 `test-kernel-map`（`os/qemu-tests/test-kernels/kernel/` 下的 QEMU 测试内核）加一枚对照页——内存快照之后经 `AllocatePages(MaxAddress(4 GiB − 4 KiB), LOADER_DATA)` 分配、填位置标记，然后在三个时点整页校验和：EBS 前、EBS 后、`arch_boot_impl` 开分页后。读数为 `pa=0x5e9b000`、`pre = post_ebs = post_enable = 0x4a00_0000_0001_ff00`、`verdict=replayed`，即该页的 1:1 映射在内核自举根下落成、内容未被触碰；同一页在快照 conventional 表中的归属为 0（`in_snapshot_free=0`）。

以下三层判别是本条不变量的权威记录（设计章只引用；证据归本节），§4.5、§5.1 M4、§6.3 只引用、不重述。探针证明了什么、没证明什么，分三层说。**第一层（探针已排除）**：候选①"页表不重放 1:1"对**本次 OVMF 形态、低于 4 GiB、快照后分配、单页样本**的 LOADER_DATA 页不成立——排除范围限于这个样本，不自动推广到"该类页面全体"，更不跨到历史真机形态。**第二层（探针未排除）**：候选②"被当空闲复用清零"的结构前提（快照把该页列为空闲）在本次 OVMF 形态未出现，但历史真机的快照怎么列、零化时序长什么样，探针没有覆盖——候选②对两组历史零化读数**仍未排除**。**第三层（独立假设）**："入口引用被跨 `noreturn` 跳转打坏"不在本探针的检验范围；它目前唯一的支撑是"深拷后症状消失"，这是干预相关性，不是对该机制的观测——要把它从假设升级为结论，需要一次直接观测（例如在入口与首个读者处各记录一次引用值/页表项作对照）。所以本探针的准确表述是**初步排除一个候选**，不是完成判别。探针保留在 `test-kernel-map`（只加打印、不改 PASS 判据），作为该不变量的可复跑证据。

| 链 | 脚本 | 这条链证明什么 | 判定标记 |
|---|---|---|---|
| UEFI + ESP 盘（aarch64） | `os/qemu-tests/test-shim-bootmarks-aarch64.sh` | boot-shim 能自己从 ESP 读内核与 12 个模块（读文件这一半） | `boot-shim: kernel loaded (entry staged)`、`boot-shim: 12 boot modules loaded` 与第三个标记之一（有 dumpdtb 的形态出 `boot-shim: memmap source: …`；无 dumpdtb 形态出同前缀的无 dumpdtb 变体；`DTB_CFGTABLE=1` 注入形态则出 `DTB preflight failed…`、**没有** memmap source 行）三条路标都出现——前两条（读文件）在 `ExitBootServices` 之前；第三条是内存图来源行，在 startup 写入、解码与三道断言之后（`DTB_CFGTABLE=1` 形态则是配对循环内、EBS 之前的 DTB preflight 拒绝行），脚本不判内核随后是否存活（`panicked at` 只作提前收尾的快速失败，不是通过条件） |
| OpenSBI + U-Boot（riscv64 U-Boot 直载测试链） | `os/qemu-tests/test-riscv64-uboot.sh` | U-Boot 的 `fatload` → `bootelf` 交棒透明可用：与 OpenSBI 直载路径同一个测试内核（`hello-boot-*`，非生产内核）经 U-Boot 装载也能走到一致的 PASS（内核入口之后不依赖固件服务） | `### TEST_RESULT: PASS hello-boot-riscv64 ###`（由被装载的测试内核打印，脚本从串口日志匹配） |
| `-kernel` 直核（riscv64 自引导面） | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh` | DTB `/chosen` 表址通道可用：表址被解析、表被消费并 fail-fast（脚本故意让模块定位失败、不假装模块存在）；备用 `a2` 通道已删除（裁决项 3），本门只验 DTB `/chosen` 通道 | `### TEST_RESULT: PASS test-kernel-image-riscv64-bootface ###`——条件含表址行、`table holds 0x0000000000000000 entries` 与 `module placement failed` 的行序 |
| UEFI + ESP 盘（x86_64） | `os/qemu-tests/test-shim-bootmarks-x86.sh` | 同一条 v1 链在 x86_64 上读盘、交接、进入调度器 | 5 条：`kernel loaded` / `12 boot modules loaded` / `[raw] entering kernel image v1` / `resuming high-half boot (handoff v1 arena, x86_64)` / `entering scheduler` |
| U-Boot shim（riscv64 U-Boot 路径） | `os/qemu-tests/test-riscv64-uboot-shim.sh` | `riscv64-uboot-shim` 构建表、装内核与 12 模块、交接到 U-Boot 接收镜像 | 6 条：`uboot leg enter (d73)` / `table built (MNXBOOT1` / `kernel + 12 modules loaded (d73)` / `[raw] entering kernel image v1` / `resuming high-half … riscv64` / `entering scheduler` |

真机门链五条以本节表格五行为准（两条 UEFI 链、riscv64 U-Boot 直载测试链、riscv64 U-Boot shim、riscv64 直核——这五条各有独立脚本；aarch64 直核面由 `test-smp-aps-aarch64.sh` 用 `-kernel` 驱动，但该脚本判的是 SMP 交付（`ap-arrived`/`ap-irq-ready`），不是启动路标门，**不计入**这五条。此处"链"指**有脚本门覆盖的组合**，与 §3.7 对账的"五条生产腿"同数不同集，不混用），跑法与"关键行缺失时怎么办"见 §6.4 第一层。本节只负责"证明什么"与通过标准。三条 riscv64/aarch64 链的终点各不相同：aarch64 只到 EBS 后的 `memmap source` 行；riscv64 U-Boot shim 到 6/6 路标（含 `entering scheduler`）；riscv64 直核 `bootface` 的 PASS 是故意让模块定位失败以验错误路径；x86 UEFI 与 U-Boot shim 链的标记集另含 `entering scheduler`（`kmain` 尾段），但调度器之后（用户态服务）的存活由后续各篇（如 `02-higher-half-kernel.md`）的门链接手，不在本篇判定范围。`BootFileTable` 取件车道不在 riscv64 U-Boot 直载测试链的测试内核里（它直构 `BootPrepareResult`、绕过 `prepare_boot`），覆盖它的是 OpenSBI 直载路径的脚本（`test-riscv64-boot-full.sh` 一类会在设备树上注入 `opensbi,boot-file-table`）。

退出码语义（表中五个脚本一致）：0 = PASS，1 = FAIL，2 = SKIP（前置工具缺失）。这条区分让"没跑"与"跑了但失败"在手工运行时可以分辨：没有 U-Boot 的开发机上，riscv64 U-Boot 直载测试链返回 2 而不是 1。当前 CI 只接线了 `test-riscv64-uboot.sh`，其余四条是手工门；表中“覆盖”指手工验证过，不等于 CI 常态守护。

脚本还顺带证明了模块数与路径：`test-shim-bootmarks-aarch64.sh` 在上盘之前会 `mdir` 检查 `/EFI/minix` 下有 `kernel.elf` 且 12 个模块名逐个存在，任一条缺失就在上盘阶段失败——也就是说"12 个模块文件必须在盘上"这件事在真机链的前置检查里被显式验证。x86_64 侧的对等物是 `test-cmd-smoke.sh` 的 ESP（EFI 系统分区，固件读引导文件的 FAT 分区）前置检查（同样的 `mdir` 三查），它的 Stage 3b 已经逐条断言 `kernel loaded`、`12 boot modules loaded` 与 `[raw] entering kernel image v1` 三条 shim 路标；专用脚本 `test-shim-bootmarks-x86.sh` 另断言 5 条（含接收端与调度器）。

**把 SKIP 当作 FAIL 处理会怎样**。没有 U-Boot 与 QEMU 的开发机上这条链长期红，随后人的处理方式是"忽略这条输出"；真正切断 `fatload` 的那次改动会淹没在常态红里，而这条链恰好是唯一能证明"U-Boot 交棒（fatload → bootelf）下同一个内核照样跑通"的判据。

### 5.3 TODO 交接表（对源码工作流的需求输入）

| TODO ID | 机制 | 断言（可观测） | 详情（状态口径以 §5.1 为准） | 交接状态 |
|---|---|---|---|---|
| TODO-01 | M4 深拷序 | 六入口在首个页分配前已完成 `store_kernel_info`（调试钩检查点） | 已覆盖：源码序钉（六入口按各架构最靠近泛型体的委托壳计：三个 `arch_boot` + aarch64 `arch_boot_resume_high_half` + x86/riscv 两个 `resume_after_v1_decode`；共享入口 `arch_boot_resume_v1` 另以 validate < `resume_boot_pt_alloc` < 委托的三步序钉住）；mock 入口 `arch_boot_test` 与 QEMU 测试内核的分阶段调用均绕过 store，故运行期断言不成立（钉注释已写明这次回退的判据） | 已交接完成 |
| TODO-02 | M7 写一次 | `register` 二次调用在 debug 下失败、release 下被拒 | 已覆盖：[`pt_alloc_write_once.rs`](os/arch/tests/pt_alloc_write_once.rs) 2 条（`register` 与 `register_free`），debug/release 两形态实跑 | 已交接完成 |
| TODO-03 | M8 落地区溢出 | 超容量输入触发 fail-fast（不截断） | 已覆盖：七枚 `should_panic`（reserved 257 / memmap 129 / boot modules 13 / platform 5 / param 1025 / source 297 / startup 21），各持 boot-globals 锁 | 已交接完成 |
| TODO-04 | M12 EBS 边界 | EBS 之后的代码路径中不出现 `uefi::` 调用 | 已覆盖（文本级）：`os/boot-shim/tests/ebs_boundary_audit.rs` 切出 `prepare_and_enter_kernel_image` 体，去注释后扫配对循环成功之后的语句（零 `uefi::`/`uefi::boot::`/allocate/free/Box/Vec/alloc/drop），带三枚注入负控（`uefi::`、`allocate_pages`、嵌套块尾部）与注释豁免自检 | 已交接完成（文本级） |
| TODO-05 | §4.6 大页首选档 | 构造 1 GiB 对齐、跨度为 1 GiB 整数倍的合成测试镜像后跑真机；现行三个生产布局不可达该档 | 条件式，需先有合成镜像 | 已登记待交接（非生产缺口） |
| TODO-06 | M14 x86_64 路标链 | x86_64 上 `kernel loaded` / `12 boot modules loaded` / `[raw] entering kernel image v1` 三条路标出现 | 已覆盖：`test-cmd-smoke.sh` Stage 3b 逐条 `grep -qF`，缺任一条即 FAIL 并回打串口尾 12 行。本条判据（三条路标）当前为绿；脚本整体退出码未在本轮复核，du/stat/find 三段的现状不属于本条判据 | 已交接完成 |
| TODO-07 | §4.2 UEFI 侧宿主可测面 | 两列表互斥完备；root/bump 不与 free 列表相交 | `assert_bootstrap_outside_memmap` 已覆盖（5 条：相离与贴边共用一条通过测试 `test_bootstrap_outside_memmap_accepts_disjoint_and_abutting`；四条 panic 案 `test_bootstrap_outside_memmap_rejects_contained_range` / `test_bootstrap_outside_memmap_rejects_partial_overlap` / `test_bootstrap_outside_memmap_rejects_wrapping_region` / `test_bootstrap_outside_memmap_rejects_wrapping_allocation`）；`build_memmaps` 的调用点单根，可测面随分类循环纯函数化后重定义 | 纯函数五案（5 条宿主测试）已交接完成；其余随分类循环纯函数化 |
| TODO-08 | §6.3 boot-shim 镜像位于 4 GiB 之上 | 该前提是 Step 1 固定上界的隐含条件 | 无代码/测试守护；当前无运行期断言 | 已登记（R18），待真机/启动加载器核对 |
| TODO-09 | §6.1 建根返回后、跳转前无发送侧路标 | 卡在建根后半与卡在跳转后无法用串口区分 | 补一条发送侧路标（不在 R18 改动范围） | 已登记（R18），待诊断路标批 |

**交接方式**：TODO-01~09 的详情登记在本表；状态以 §5.1 的覆盖标签为准；源码工作流修复后，本表的覆盖状态随之回填。

### 5.4 自检：写不出可观测断言的机制

本节只点名"写不出可观测断言"的机制；每条机制的状态一律以 §5.1 的覆盖标签为准，§5.3 只放 TODO 详情，这里都不重述。按这个口径：M3 的"不向清单领页"子项、M6 的"全量 4 GiB 实际叶子映射"子项、M17 整体当前写不出可观测断言（M3 的 fail-closed 负控与 M6 的分片覆盖宿主断言已有），因此**未登记 TODO**（§5.1 对应行的 `TODO` 列为 `—`，理由见该行与本节）；M5/M13 的缺口已登记到 §5.3。M4 与 M14 的断言强度偏弱（前者是静态序钉、后者靠真机"没有崩"反证），强度已写在 §5.1 的覆盖状态里。

---

## 6. 诊断

本章回答"引导失败了怎么查"：6.1 讲 EBS 前后两条输出通道与里程碑路标的读法，6.2 是从静默到可读的 panic 路径，6.3 列典型失败模式与对应信号，6.4 给可复制命令。这一章按用途查，不按执行顺序读。

### 6.1 EBS 分界与裸串口通道内的两种写法

在 UEFI 路径上，两条通道按 `ExitBootServices` 分界：EBS 之前是固件控制台（`uefi::println!`，经 ConOut——固件的控制台输出协议）；EBS 之后是裸串口（`raw_serial`/`raw_serial_line`）。U-Boot shim 与两个直核面没有 EBS 这一步，只有裸串口。因此里程碑输出分两段。下文的 `raw_serial` 与 `raw_serial_line` 是同一条裸串口通道内的两种写法，不是两条通道。

```rust
// 节选（保留循环与 asm 两行；省略号处为 SAFETY 注释），os/boot-shim/src/lib.rs:fn raw_serial。
#[cfg(target_arch = "x86_64")]
pub fn raw_serial(text: &str) {
    for byte in text.bytes() {
        // ...
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3f8u16, in("al") byte, options(nomem, nostack));
        }
    }
}
```
被省略的 SAFETY 注释给出这条写入的安全性依据：COM1 数据口是 QEMU/PC 平台的固定 I/O 资源，而引导准备阶段 boot-shim 是唯一写者（固件已经把控制权交出去、内核还没起来，没有并发写者），`out` 指令没有内存副作用。

`raw_serial` 与 `raw_serial_line` 的差别：后者在写之前等发送侧腾空（x86_64 读 LSR bit5，有界轮询）并在 `\n` 前补 `\r`；前者在 x86_64 上不等、也不补——aarch64 上这两个函数共用同一个带节流的发射器（源码注释明写本架构上"等腾空"与"不等"没有区别），差别只剩 `\n` 前补不补 `\r`。前者用于"已经知道自己在写什么、只想尽快留下痕迹"的场合（例如 EBS 之后的第一行），后者用于 panic 路径。

里程碑清单（每条给源码位置；这些字符串是排查时的定位依据。清单里没有独立的 step3 路标——开分页夹在 `step1+2` 与 `step4` 之间，源码未单独打点）：

| 输出 | 阶段 | 位置 |
|---|---|---|
| `boot-shim: prepare_and_enter enter (handoff v1 arena)`（x86_64 与 aarch64） | 进入 arena 交接准备 | `uefi_helpers.rs:prepare_and_enter_kernel_image` |
| `boot-shim: kernel loaded (entry staged)` | 内核段已拷贝 | 两条 UEFI 产线共用 |
| `boot-shim: 12 boot modules loaded` | 12 个模块已就位 | 两条 UEFI 产线共用 |
| `boot-shim: param_buf bytes=… rejected=… dropped=…` | 参数源读数 | `collect_param_buf` |
| `boot-shim: [raw] boot services exited (paired)`（x86_64 与 aarch64） | 配对循环已退出（裸串口） | `prepare_and_enter_kernel_image` |
| `boot-shim: memmap source: …`（x86_64 与 aarch64） | 最终内存图来源行（最终快照与分类结果的一致性检查在 EBS 之后跑） | 同上 |
| `boot-shim: [raw] arena=… conv=… res=…`（x86_64 与 aarch64） | arena 基址与分类条目数 | 同上 |
| `boot-shim: [raw] entering kernel image v1`（三条 v1 发送腿：x86_64、aarch64、riscv64 U-Boot shim） | 跳转前最后一条 shim 路标（裸串口） | `uefi_helpers.rs`（两条 UEFI 腿）/ `uboot_main.rs`（U-Boot shim 腿） |
| `kernel: arch_boot entered` | 内核侧入口路标（当前仅 x86_64 的 `arch_boot` 打印；aarch64/riscv64 镜像无此行，v1 接收也不打印——三架构都先经 `resume_after_v1_decode`；aarch64 的壳再转 `arch_boot_resume_high_half`（x86_64/riscv64 的壳直接进 `resume_additive`），见 §4.8） | [`os/kernel/src/lib.rs`](os/kernel/src/lib.rs)（`arch_boot` 按架构有三个互斥定义，故锚到文件） |
| `kernel: v0 enter` / `kernel: v1 …` / `kernel: v2 …` / `kernel: v3 …` / `kernel: v4 capability facts ok` | `boot_validate_and_prepare` 的五个阶段路标，依次打出（v1–v3 是中间阶段，独立定位价值低于首末两点；表中不逐行列出其文案）；v1 链上 v0–v4 整组打印**两次**——发送侧 `enter_kernel_image_v1` 跳转前一次、接收侧 arena 解码后（`resume_additive` 内）再一次，出现第二组才是接收端已开始跑；直核面在进入 `arch_boot` 之后走到这里 | [`boot_validate_and_prepare`](os/kernel/src/lib.rs:fn boot_validate_and_prepare) |
| `### minix-rs kernel-image: resuming high-half boot (handoff v1 arena)`（x86_64 带 `, x86_64`；aarch64 无后缀；riscv64 带 `, riscv64`） | 接收端已进入 Rust 面 | `kernel-image/src/main.rs:rust_image_main` |
| `kernel-image: step4 DM coverage (additive on active root)` / `kernel-image: DM ok, jumping to kmain (high half)` | 接收端 additive 阶段路标（仅 aarch64 壳打印；x86_64/riscv64 的壳走 `resume_additive`，标签不同） | [`arch_boot_resume_high_half`](os/kernel/src/lib.rs:fn arch_boot_resume_high_half) |
| `kernel: step0 validate ok` / `step1+2 mappings ok` / `step4 DM coverage ok` | 直核建表逐步路标；其中 `step1+2 mappings ok` 也见发送侧（`enter_kernel_image_v1` 建根时），两条腿共用这一串 | [`lib.rs`](os/kernel/src/lib.rs) 的 `boot_stage!` |
| `kernel: step2c device window empty (skip)` | aarch64 直核设备窗口为空时的跳过标记 | `lib.rs` 的 `boot_stage!` |
| `kernel: step4 DM coverage (preflight + install)` | 接收侧 DM 覆盖的两段（preflight 与 install） | 同上 |
| `  param_buf bytes=… first=…`（两个前导空格，无 `boot-shim:` 前缀） | 直核面参数源读数；`first` 是第一条记录 | 两个 bootface 的 `normalize_param_buf` 调用点 |

（`build_memmaps` 的诊断打印是 `boot-shim: memmaps conv=… reserved=… unmapped=…`；空表时打印完整串 `boot-shim: memmaps conv=0 reserved=0 unmapped=0 (empty map)`。没有单独的 `reserved-big` 行；这些是测试内核调用 `build_memmaps` 时的诊断读数（`build_memmap()` 是只取 conventional 表的单表包装），不进主路径里程碑。）

复算命令（按文件覆盖表内全部来源；串前缀扫描约命中 10/16 组，其余走别的宏或裸字符串，见各行位置列）：`grep -n 'uefi::println!\|raw_serial' os/boot-shim/src/main.rs os/boot-shim/src/uefi_helpers.rs`；`grep -n 'boot_stage!\|### minix-rs kernel-image' os/kernel/src/lib.rs os/kernel-image/src/main.rs os/kernel-image/src/bootface*.rs`。

**删掉 `[raw] entering kernel image v1` 这一行会怎样**：发送侧在调用 `enter_kernel_image_v1` 之前打印 `[raw] arena=…` 与这一行；建根与开分页发生在调用内部。删掉这一行后，串口停点按三档读：`[raw] arena=…` 表示还没进调用；`kernel: v4 capability facts ok` 表示已进调用但 Step 1/2 映射尚未完成（建根前半）；`kernel: step1+2 mappings ok` 表示 Step 1/2 已过（建根后半或跳转之后），若随后出现接收侧 `###`/第二组 `v0` 则接收端已起。第二档与第三档之间没有发送侧路标——“卡在建根后半”与“跳转后卡死”无法用串口区分，这是诊断面的已知缺口；补法是在建根返回之后、跳转之前加一条发送侧路标（见 §5.3 TODO-09，不在本篇改动范围）。

### 6.2 panic 路径：从静默到可读

```rust
// 节选（保留缓冲写入与输出段；注释有删节，英文注释逐字保留），os/boot-shim/src/main.rs:fn panic。
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    struct StackWriter<'a> {
        buf: &'a mut [u8; 256],
        len: usize,
    }
    impl core::fmt::Write for StackWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.len < self.buf.len() {
                    self.buf[self.len] = b;
                    self.len += 1;
                } // Dropping the tail beyond 256 B is fine: truncated diagnostics beat silence.
            }
            Ok(())
        }
    }
    let mut buf = [0u8; 256];
    let mut writer = StackWriter { buf: &mut buf, len: 0 };
    let _ = core::fmt::Write::write_fmt(&mut writer, format_args!("{}", info));
    let written = writer.len;
    let message = core::str::from_utf8(&buf[..written])
        .unwrap_or("boot-shim panic: (non-utf8 message)");
    raw_serial_line("boot-shim panic: ");
    raw_serial_line(message);
    if !minix_types::run_panic_diagnostic_hook(message) {
        raw_serial_line("boot-shim panic: diagnostic hook not registered");
    }
    loop {
        core::hint::spin_loop();
    }
}
```

四条约束与理由（前两条是同一件事的两面，合并陈述）：

- 用固定栈缓冲而不是 `format!`：堆在 EBS 之后不可用（UEFI 池分配器返回空）。`StackWriter` 只做逐字节搬运。
- 超出 256 字节丢弃尾部而不是截断格式化过程：注释写明"诊断信息截断好于静默"。
- 输出经 `raw_serial_line`（带 `\r` 补全与发送侧等待）：panic 消息只走裸串口，不碰任何可能已失效的固件通道。
- 最后尝试诊断钩子：诊断钩子静态量随 `minix-types` 编进**每个二进制各自的一份**——跨镜像交接意味着 boot-shim 与内核各有自己的 `.bss`，shim 侧的钩子恒未注册（§3.4 的"两个 `.bss`"论证同样适用于静态量）。于是两条腿分开说：**shim 侧**（本节引文的 handler），钩子检查必然落空、兜底信息是*无条件*行为，不携带"早于注册"的诊断含义；**内核侧**——内核自己的 panic handler 在 `arch_boot` 入口注册钩子（`register_panic_diagnostic`），之后内核阶段的 panic 才会把同一条消息按内核格式再打一遍（带 CPU 号与回溯），钩子未注册在那边才是"panic 早于注册"的真实信号。

**在 panic handler 里调用 `format!` 或固件服务会怎样**：第二次故障（分配失败、协议空指针）会在处理第一次故障的过程中发生——串口上得到的是"panic 处理自身的崩溃"或什么都没有，而原始 panic 的消息与位置被彻底覆盖。"panic 路径上不能分配、不能调固件服务、只能纯 CPU 操作加裸串口"这条约束就是从这里来的。

### 6.3 典型失败模式与诊断信号

（复现这些失败的可复制命令见 6.4；两条链路的互指：本节给"哪个信号说明什么"，6.4 给"怎么把信号抓下来"。）

表的前两行是 **C 原作的失败模式**（`boot.cfg` 指令次序、`head.S` 的 `push` 次序）——重写产品没有这两个触发点，列出只为对照 §2.1/§2.2 的断言行为，排查重写链时直接跳过；其余各行是 Rust 链上会遇到的，或（划线那行）已不可复发的历史条目。

| 失败模式 | 触发点 | 现场信号 | 下一步确认 |
|---|---|---|---|
| 引导器指令次序错（`load_mods` 在 `multiboot` 之后） | [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 的 `assert(mbi->mi_mods_count > 0)`（同函数前面那条查 `HAS_MODS` 的会放过去，理由见 2.1） | 未开串口调试（默认）：屏幕刷出 `Minix panic. System diagnostics buffer:` 与 `press any key to reboot`、等一个按键才复位；命令行里写了 `cttyline=`（`do_serial_debug` 为真）时走**互斥**的另一条通路——panic 由串口同步输出后立即复位，不刷屏、也不等按键。这条断言排在命令行解析之后，所以 `cttyline=` 已经生效；对照下一行那条在解析之前，串口一定不出声 | 核对 `boot.cfg.default` 的 `;` 分隔次序 |
| 参数压栈次序错（x86-32） | [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的 `assert(magic == MULTIBOOT_INFO_MAGIC)` | 同样刷屏等按键，但串口一定不出声——这条断言在命令行解析之前，`do_serial_debug` 还是 `.bss` 的零初值；报出的表达式里带着 `magic` 这个名字，而它当时装的是地址（值本身不打印，见 §2.2 的说明） | 核对 `head.S` 两条 `push` 的次序 |
| 模块缺失 | `loader.rs:load_boot_modules_into` | `boot-shim: required boot module 'X' not found at <路径>` | 核对盘上 12 个模块名（脚本的 `mdir` 前置检查会先报） |
| 内核 ELF 读不到 | `FileLoader::read_required` | `boot-shim: required file not found: /EFI/minix/kernel.elf` | 核对 ESP 的 `/EFI/minix/kernel.elf` 与固件是否挂载了该分区 |
| bump 区间耗尽 | `BootAlloc::alloc` 返回 `AllocationFailed` | 内核侧 `map_huge`/`boot_pt_alloc` 失败，或 `enter_kernel_image_v1: bump region not registered` | 复算 `prepare_and_enter_kernel_image(N)` 的页数预算与镜像规模 |
| v1 信封/载荷无效 | `arch_boot_resume_v1` 的 `decode_*` | `kernel-image: handoff v1 envelope invalid: <变体>`（或 memmap/reserved/modules/platform/param/source/startup 七个分区中任一段的对应行） | 核对 shim 的序列化顺序与描述符长度；先看 `[raw] arena=… conv=… res=…` 是否已打印 |
| 发送侧走完、接收端未起（无声卡死） | `[raw] entering kernel image v1` 之后、接收侧 `### minix-rs kernel-image: resuming high-half boot…` 之前 | 串口最后一行通常是 `kernel: step1+2 mappings ok`（卡在建根内部或建根完成后），更早则是 `kernel: v4 capability facts ok` 或 `[raw] arena=…`——三档读法见 §6.1 末段；接收侧第二组 `v0` 不出现 | 按 §6.1 末段三档先定位发送侧停点；再按 §6.4 第三层在 `enter_kernel_image_v1` 或接收侧委托链下断点 |
| ~~`bootstrap_len` 非零~~ | （字段与断言均已删除，该失败模式不可复发） | 历史消息曾是 `bootstrap_len must be 0 (Rust port has no unpaged section), got N`——现行代码 grep 不到它，排障不要按这条找 | — |
| boot-shim 自身镜像被装载在 4GiB 之上 | 固件装载器（UEFI 无此保证：规范不约束 loader 镜像的物理落点） | 恒等映射只铺低 4GiB（`IDENTITY_MAP_END`），`bootstrap_alloc_limit()` 只守 root/bump/arena/模块页四段（`alloc_module_pages` 同以该界分配并带端点复核）——boot-shim 自己的代码与栈若在 4GiB 之上，开分页后的第一条冷取指就落在无映射区 | 复核 OVMF 的实际装载地址（本仓门链上观测低于 4GiB，不是规范保证）；这是 §4.6 Step 1 固定上界隐含的前提，无代码守护（见 §5.3 TODO-08） |
| 快照位置错（自举页被报成可用） | `assert_bootstrap_outside_memmap` | `conventional memmap [a,b) overlaps bootstrap allocation [c,d)` | 核对快照调用点在三次分配（root/bump/arena）之后 |
| 大页阶梯无档可用 | 规划层 `plan_boot_pages`（两个调用点：直核 `arch_boot` 家族与 v1 发送侧 `enter_kernel_image_v1`——后者服务 x86_64、aarch64、riscv64 U-Boot shim 三条腿） | 两条消息前缀对应两族：`arch_boot: page-size policy: …`（直核）/ `enter_kernel_image_v1: page-size policy: …`（v1 发送三腿）；消息体同为 `no kernel page size satisfies the layout` | 跑 `check-layout.sh` 的 L3/L4/L5 |
| 页表切换后取指故障 | `Paging::enable` 之后第一条指令 | QEMU `-d int` 日志里的异常记录；`CR2`/`EPC` 指向当前指令 | 核对恒等映射范围是否覆盖该地址（§4.6） |
| `KernelInfo` 切片读回全零 | `vm_handoff::classify` 或后续消费者 | VM 侧"no free memory regions"类 panic | 核对 `store_kernel_info` 是否落了七块（§4.5） |

**把 `assert_bootstrap_outside_memmap` 降级为 `debug_assert` 会怎样**（release 构建下它就消失了）：它挡的是"活的页表页与 bump 池被当作空闲内存交给 VM"这一类问题，而这类问题的发作要经过"VM 取帧 → 覆盖页表页 → 切换 CR3 → 取指失败"四步。断言在 `prepare_boot` 路径上于装载/建根之前运行（该路径没有 EBS 步骤），在 v1 生产入口上于配对循环成功之后、建根之前运行（此时消息仍可经裸串口输出，消息里同时给出 conventional 区间与自举区间两个坐标），而故障在现场只剩取指错误——把这条断言降级，等于把一个可读的坐标信息换成一次无坐标的崩溃。

### 6.4 可复制命令

按"看输出 → 看异常 → 看页表走查"三层排列。每段都给出预期输出的关键行，以及关键行缺失时说明什么。

```bash
# 第一层：串口捕获（最常用）。真机门链五条（两条 UEFI 链、riscv64 U-Boot 直载测试链、
# riscv64 U-Boot shim、riscv64 直核——计法说明见 §5.2.3；aarch64 直核面
# 无专用启动门脚本（由 test-smp-aps-aarch64.sh 间接驱动，判据是 SMP 交付）、不计入五条）。
# 用现成脚本，它会把串口写入日志文件并 grep 标记。
$ bash os/qemu-tests/test-shim-bootmarks-x86.sh         # x86_64 UEFI：shim→独立镜像交接门
$ bash os/qemu-tests/test-shim-bootmarks-aarch64.sh      # aarch64 UEFI：三条标记之一（前两条读文件；第三条是 memmap source 行，DTB_CFGTABLE=1 形态则是 DTB preflight 拒绝行）
$ bash os/qemu-tests/test-riscv64-uboot.sh              # riscv64 U-Boot 直载测试链：fatload + bootelf（装 hello-boot 测试内核）
$ bash os/qemu-tests/test-riscv64-uboot-shim.sh         # riscv64 U-Boot shim：六路标门（表、装载、交接、调度器）
$ bash os/qemu-tests/test-kernel-image-riscv64-bootface.sh   # riscv64 直核自引导（bootface 入口腿）
# （aarch64 直核面由 test-smp-aps-aarch64.sh 用 -kernel 驱动，判据是 SMP 交付而非启动路标，不计入五条门链）
# 关键行：UEFI 链看 `boot-shim: kernel loaded (entry staged)` 与 `boot-shim: 12 boot modules loaded`；
#        riscv64 两条链看 `### TEST_RESULT: PASS … ###`；riscv64 U-Boot shim 链的六条标记措辞不同，
#        按 §5.2.3 表对应行判定。
# 若关键行缺失：aarch64 脚本会打印"出现在/未出现"两行；U-Boot 与 bootface 脚本只回打串口尾部 20 行，按各自脚本的实际输出排查。

# 裸 QEMU（需要自己准备固件与磁盘时）。x86_64 的 OVMF 路径与 aarch64 的 AAVMF 路径参数不同，
# 固件路径按发行版替换：
$ qemu-system-x86_64 -bios <OVMF_CODE.fd> -drive format=raw,file=disk.img -nographic -serial mon:stdio
$ qemu-system-aarch64 -machine virt,gic-version=3 -cpu cortex-a72 -nographic \
      -drive if=pflash,format=raw,file=<AAVMF_CODE.fd> -drive format=raw,file=disk.img
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel bios-or-image.elf

# 第二层：异常与页表走查（-d 追踪项逗号分隔；mmu 会放大日志量，只在定位时开）
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel image.elf \
      -d int,mmu,cpu_reset -D qemu.log
# 关键行：qemu.log 里的异常记录形如 riscv_cpu_do_interrupt: hart:0, async:0, cause:…, epc:0x…, desc=…
# 若关键行缺失：说明 CPU 没有产生异常，问题在输出通道或更早的停机（例如 halt 驻留）。

# 第三层：GDB 远程调试（-s 开 gdbstub，-S 暂停在第一条指令）
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel image.elf -s -S
$ gdb-multiarch image.elf
(gdb) target remote :1234
(gdb) break kernel::rust_image_main   # `kernel` 是 kernel-image 这个 bin 的 crate 名（见 os/kernel-image/Cargo.toml）；裸短名匹配不到 mangled 符号
(gdb) break minix_kernel::arch_boot   # 三个 cfg 版本互斥，断点落在当前架构那一份
(gdb) break minix_kernel::arch_boot_resume_high_half   # 仅 aarch64 镜像有；riscv 直核会话改 break minix_kernel::arch_boot（resume_additive 只在 U-Boot 接收镜像里）
(gdb) continue
(gdb) info registers
```

三层的分工：

- 第一层回答"哪一步没走到"：里程碑清单（6.1）把 boot-shim 的动作切成十几段，缺哪一段就知道卡在哪个区间。
- 第二层回答"卡的时候 CPU 在做什么"：`int` 给出异常类型与故障地址，`mmu` 给出 MMU/页表活动记录（`page` 是 user-mode 专用项，system 模式不产生页表遍历）。`-d mmu` 的日志量与 `-d int` 不在一个数量级，只用于定位单次页表遍历。
- 第三层回答"值是什么"：在 `arch_boot` 或接收侧委托链（`arch_boot_resume_v1` → 三架构均 `resume_after_v1_decode` → aarch64 再转 `arch_boot_resume_high_half` → `resume_additive`）断下来，检查 `KernelInfo` 的字段与 `root_page` 是否合理（`kern_size`、`memmap` 切片长度、`kern_stack_top` 是否等于镜像尾——`bootstrap_len` 已随字段删除，不是现存的检查项）。

**只用第三层（GDB）而不开 `-d int` 会怎样**。页表切换后的故障是"取指即失败"：GDB 断在切换之后的第一条指令上时，故障已经发生，而"故障地址是哪个、是取指还是读"这类信息只有 QEMU 的异常记录才给。缺了第二层，现场只剩"停在了某处"，没有 `epc`/`CR2` 可对照页表范围。

---

## 7. 参见

### 7.1 前置与后续篇

- [`02-higher-half-kernel.md`](02-higher-half-kernel.md)（链接布局、ELF 段拷贝、成对跳转、`k_initial_stktop` 两条线索）
- [`03-kmain-cstart.md`](03-kmain-cstart.md)（保护结构）
- [`04-platform-discovery.md`](04-platform-discovery.md)（DTB/RSDT 解析与描述符）
- [`05-clock-interrupt-init.md`](05-clock-interrupt-init.md)（时钟与中断控制器）
- [`smp_gpt_v3.md`](smp_gpt_v3.md)（SMP 设计评审稿：`online_mask` 的误用与 AP 进入/握手语义；`alloc_lowest` 的消费者与 `smp_init` 是计划中的 SMP 成稿，当前文件没有这两项）

### 7.2 关联 C 源码清单

本清单只列主干文件；正文逐段引用的小文件（`glo.h`、`Makefile.inc`、`archconst.h`、`pci.c`、`boot2.c`、`biosmemx.S`、`loadfile*.h`、`ramdisk/rc`、`pm/main.c`、`pm/misc.c`、`vm/main.c`/`utility.c`/`region.c`、`dmp_kernel.c`、`procfs/root.c`）不重复列出，路径见各段行内链接与 `file:line` 锚点。

- `minix3/minix/kernel/arch/i386/{head.S,pre_init.c,pg_utils.c,kernel.lds,mpx.S,memory.c,arch_smp.c,arch_system.c,protect.c}`
- `minix3/minix/kernel/arch/earm/{head.S,pre_init.c}`
- `minix3/minix/include/minix/{param.h,const.h}`
- `minix3/minix/kernel/{table.c,main.c,system/do_getinfo.c,system/do_copy.c,system/do_safecopy.c,system/do_memset.c,system/do_safememset.c}`
- `minix3/minix/servers/vm/alloc.c`、`minix3/minix/lib/libsys/assert.c`、`minix3/lib/libc/gen/assert.c`
- `minix3/sys/lib/libsa/loadfile_elf32.c`、`minix3/sys/arch/i386/stand/lib/exec.c`、`minix3/minix/kernel/arch/earm/pg_utils.c`
- `minix3/minix/commands/{sysenv/sysenv.c,update_bootcfg/update_bootcfg.sh}`
- `minix3/minix/servers/{mib/kern.c,vm/pagetable.c}`
- `minix3/minix/include/arch/i386/include/vm.h`
- `minix3/sys/arch/i386/include/multiboot.h`
- `minix3/etc/boot.cfg.default`

### 7.3 关联 Rust 源码清单

本清单只列主干文件；逐文件调用点见正文。

- `os/boot-shim/src/{main,lib,loader,uefi_helpers,opensbi_helpers,uboot_main,ebs_map,ebs_pair}.rs`
- `os/boot-shim/riscv64-uboot.ld`
- `os/libs/minix-boot/src/{kernel_info,boot_shim,handoff_v1,handoff_v1_read,handoff_v1_write,platform,boot_params,memmap_source,fdt_layout}.rs`
- `os/kernel/src/{lib,boot_alloc,globals,vm_handoff,misc}.rs`
- `os/arch/src/arch/{pt_alloc,paging,paging_ext}.rs`
- `os/arch/src/x86_64/paging.rs`
- `os/kernel-image/check-layout.sh`、`os/kernel-image/src/{main,bootface,bootface_a64}.rs`
- `os/servers/vm/src/{vm_server,alloc_page}.rs`
- `os/libs/minix-types/src/types/boot.rs`

### 7.4 构建与回归入口

- `os/qemu-tests/README.md`
- `os/qemu-tests/{test-shim-bootmarks-aarch64,test-shim-bootmarks-x86,test-riscv64-uboot,test-riscv64-uboot-shim,test-kernel-image-riscv64-bootface,test-kernel-image-riscv64,test-riscv64-boot-full,test-cmd-smoke,test-atf-riscv64,test-cmd-smoke-riscv64,test-smp-aps-aarch64,test-smp-aps-riscv64}.sh`
- `os/qemu-tests/run_all.sh`
- `os/kernel-image/check-layout.sh`

### 7.5 符号速查（C ↔ Rust ↔ 章节）

按符号查读用（跨篇符号只给本篇章节；同一符号列出主要出现处，不保证穷举）：

| C 侧符号 | Rust 侧对应 | 本篇章节 |
|---|---|---|
| `MINIX` / `multiboot_init`（`head.S`） | `os/boot-shim/src/main.rs:fn main` → `prepare_and_enter_kernel_image` → `enter_kernel_image_v1` → `arch_boot_resume_v1` | §2.2 / §3.7 / §4.1 / §4.9 |
| `load_stack` / `load_stack_start` | **没有同名对应物**：C 侧是 4 KiB 临时栈；Rust 的 64 KiB 引导栈顶是 `kernel_boot_stack_top`（L7），两者只角色相似、不是同一对象 | §2.2 / §4.4 / §5.2.2 |
| `pre_init` | `prepare_and_enter_kernel_image`（UEFI 两条路径）/ `opensbi_helpers::prepare_boot`（U-Boot 路径） | §2.6 / §3.7 / §4.1–§4.2 / §4.9 |
| `get_parameters` | `prepare_and_enter_kernel_image` + `ebs_pair` / `build_kernel_info` | §2.3 / §2.4 / §4.2–§4.4 |
| `add_memmap` / `cut_memmap` | 构建侧：`ebs_pair::pair_and_classify`（生产）/ `build_memmaps`（测试辅助）；内核侧：`vm_handoff::FreeRangeSet` 扣减（C 形状账本模块已删除） | §2.4 / §4.9 |
| `alloc_lowest` | 暂未移植（AP 启动归 SMP 篇；低地址代码拷入端 `install_at` 已有） | §2.4.1 / §4.9 |
| `overlaps` | 防线 A/B/C/D：A `first_source_destination_overlap`（源 × 目标）、B `first_overlapping_pair`（段间目标 × 目标）、C `alloc_module_pages`/`assert_modules_clear_of_kernel`（模块 × 内核跨度）、D `KernelInfo::validate`（模块 × 模块，接收侧复核） | §2.5 / §4.9 / §4.3 |
| `pg_clear` / `pg_identity` / `pg_mapkernel` / `pg_load` / `vm_enable_paging` | Step 1–3、`new_from_page`、`enable` | §2.6 / §4.6 / §4.9 |
| `freepde_start` | 字段已退场（字段删除）；`GET_KINFO` 报告位固定填 0 | §2.6.3 / §3.9 |
| `alloc_pagetable` | `os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page`（注册式分配器） | §2.6.2 / §3.8 / §4.7 |
| `pg_alloc_page` | 无同名对应物（运行期 VM 物理分配器；引导期页表页见 `alloc_pagetable`） | §2.4.1 / §3.8 |
| `bootstrap_start` / `bootstrap_len` | 字段已退场（字段删除；C 侧回收机制照旧） | §2.3 / §3.5 |
| `mem_high_phys` | 无对应物（Step 1 固定上界） | §1.2 / §2.4 / §3.4 |
| `kinfo`（全局） | `KernelInfo`（按值交接 + `.bss` 深拷） | §2.3 / §3.4 / §4.5 |
| `kmain` | `kmain`（高半执行） | §2.2 / §2.8 |
| `k_initial_stktop` | 角色对应（换栈目标），**不是同一对象**：`kern_stack_top` 是镜像尾，真正的栈数组顶是 `kernel_boot_stack_top` | §2.2 / §2.8 |
