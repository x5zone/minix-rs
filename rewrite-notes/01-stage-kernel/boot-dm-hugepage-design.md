# Boot Direct Map 与大页能力：按生命周期拆分的四层抽象

> **文档范围**：讨论启动期 Direct Map 建立、大页能力查询、启动选页策略三件事的边界，并给出可落地的改进设计。  
> **代码范围**：`os/arch/src/arch/{boot,direct_map,paging,paging_ext,dm_coverage}.rs`、`os/arch/src/lib.rs`、`os/arch/src/{x86_64,arm64,riscv64}/paging.rs`、`os/kernel/src/{lib,boot_policy,dm_coverage,vm_handoff,smp}.rs`。  
> **行为边界**：不改变 Minix3 的启动语义、Direct Map 的地址布局和 VM 交接协议；支持 1 GiB 页的现有平台保持原有页表内容，不支持 1 GiB 页的平台改用 2 MiB 页作为修复。  
> **支持边界**：当前只支持共享页表所需的页大小能力、有效物理地址上界和翻译配置同构的 SMP 平台。所有可能访问共享内核页表的 CPU 必须对这些条件给出兼容答案；异构组合不在本次改动范围内。  
> **阅读方式**：本文不要求读者手里有仓库。关键接口都给出了简化代码；路径只用于需要核对完整实现时定位。

## 1. 结论摘要

当前实现把“启动期在 bootstrap 页表根上安装 Direct Map”和“运行期通过分页句柄操作页表”分成了不同 trait（Rust 的接口），这个方向是对的。问题集中在十五个地方（前八项是基础问题，后七项是冻结前必修项）：

1. **名字没有说清生命周期**。`DmCoverageArch` 只在启动期调用，但名字听起来像一个运行期服务；它实际是“启动期 Direct Map 叶子安装器”。
2. **同一个硬件能力有两个 trait 入口**。`HugePages::supports_1gb_page` 和 `DmCoverageArch::supports_1gb_page` 都在回答“这台 CPU 能不能用 1 GiB 页”，x86-64 下两者都转发到同一个 CPUID 探针。
3. **能力查询和选页策略没有闭环**。启动代码选择内核映射页大小时只看地址是否对齐，不看系统是否真的支持 1 GiB 页；Direct Map 建立路径会看能力。两条路径可能得出不同结论。
4. **`supports_huge_page` 的默认实现替架构做了硬件承诺**。默认实现只查静态尺寸列表，x86-64 即使没有 GBPAGES 也会对 1 GiB 返回真。只要默认实现存在，新架构就可能忘记承担自己的硬件能力契约。
5. **能力获取和启动规划混在一起**。`plan_boot_pages` 一边查询硬件，一边计算策略，不能被当作纯函数；测试也只能通过修改全局 mock 能力间接控制规划。
6. **`DmPagePolicy` 仍把 1 GiB 当成特殊分支**。策略对象携带了 1 GiB 布尔值、2 MiB 回退和 4 KiB 基础页，但 `plan_range_units` 仍然单独写死“1 GiB 优先”。这不是当前平台的错误，却是策略对象没有完整表达自己声称控制的行为。
7. **策略没有携带架构身份，且在调用点退化为裸数值**。一个架构生成的策略在类型上不能阻止被另一个架构的安装器使用；`build_bootstrap_root_and_enable` 仍只收到 `u64`。
8. **Direct Map 的虚拟地址对齐和溢出边界没有进入契约**。`plan_range_units` 只看物理范围和页大小，最终 `VA = va_base + PA` 是否对齐、是否越界、是否溢出，没有在规划器或安装器边界上明确验证。

还有七个冻结前必修项：

9. **`PageSize::as_usize` 会静默截断**。私有字段和受约束构造保证的是“页大小表示合法”，不是“目标平台一定能用 `usize` 表示”。转换边界必须返回可检查的结果。
10. **泛型策略的派生实现会给架构参数加无关约束**。`#[derive(Clone, Copy)]` 在带 `PhantomData<fn() -> P>` 的泛型结构上仍可能生成 `P: Clone`/`P: Copy` 约束，导致 `HugePages` 没有这些 bound 时访问器无法编译。策略类型应手写 `Copy`/`Clone`。
11. **x86-64 的 CPUID 探针只检查 GBPAGES 位还不够**。查询 `CPUID.80000001H` 之前必须先确认最大扩展叶 `CPUID.80000000H.EAX >= 0x80000001`；否则在只支持较少扩展叶的 CPU 上，探针会读到一个语义不成立的叶子。
12. **`install_boot_dm_range` 是公开的安全函数，却调用 `unsafe` 的叶子安装器**。它只检查了地址，没有能力证明“当前处于单线程启动上下文、bootstrap root 活动、页表页都在恒等映射内”。这个安全契约必须上移到入口，或者用一个受控的启动上下文令牌限制入口。
13. **布局验证必须与地址换算常量严格一致**。运行期 `kernel_phys_to_virt`/`vm_phys_to_virt` 用 `DirectMapArch` 的固定常量做 `VA = PA + BASE` 加法；如果 `DirectMapLayout::validate()` 只要求基址不低于架构常量、终点不越上界，一个几何自洽但整体偏离常量的布局就能通过验证——策略按布局安装叶子，运行期按常量计算地址，映射存在，调用方却访问不到。验证必须要求布局与常量逐项相等，或者让运行期换算消费布局本身；本文不改变现有窗口布局，选择前者。
14. **恒等映射窗口缺少 PA 上界检查**。`build_bootstrap_root_and_enable` 以选中尺寸覆盖 `[0, 4 GiB)` 恒等映射，页大小能整除 4 GiB，不等于平台有效物理地址上界覆盖 4 GiB；启动输入里没有任何校验保证 `phys_addr_limit >= IDENTITY_MAP_END`。bootstrap 树页的既有边界（`min(IDENTITY_MAP_END, VM 窗口容量)`）只管树页自己，管不了整段恒等窗口。
15. **运行期地址换算没有窗口边界契约**。`kernel_phys_to_virt`/`vm_phys_to_virt` 是无条件加法：`PA < phys_addr_limit` 但 `PA >= 窗口容量` 时，换算仍会返回一个没有被该窗口覆盖的虚拟地址。仓库里已有同类前科——`virt_to_phys` 对两个 DM 窗口之外的内核镜像/内核栈地址产出伪物理地址（NK4-C 1.51 系列判例，多处调用点被迫改走别的通道），且有调用点的安全注释写着「DM 窗口覆盖全部物理内存」这种未经证明的平台假设。算术成功不等于地址有效，窗口边界必须进入换算接口或调用前提。

建议的目标结构是：

- 保留四个概念，不合并：地址布局、运行期页表操作、大页能力、启动期安装机制。
- 把 `DmCoverageArch` 改名为 `BootDmInstaller`，把“这是启动期动作”写进类型名。
- 只保留一个公开能力查询：`HugePages::supports_huge_page(PageSize)`。删除默认实现和两个 `supports_1gb_page` 公开入口；每个架构必须显式实现自己的能力契约。
- 引入轻量 `PageSize` 类型，统一能力查询、策略保存和 `map_huge` 的尺寸表示；`PageSize` 只保证数值表示合法，不冒充硬件能力令牌，`usize` 转换使用可检查的 `try_as_usize`。
- 把“获取能力事实”和“规划启动策略”拆成两步：`detect_boot_page_support` 负责查询，`plan_kernel_mapping` / `plan_dm_policy` / `plan_boot_pages` 只消费事实，是纯函数。
- 删除能力快照里按内核/DM 分别保存的重复偏好：同一个页大小只保留一个权威答案。内核映射和 Direct Map 各自决定怎么使用这份事实，不在事实层复制答案。
- 引入统一的架构标记 `BootArch`，把分页实现、`DirectMapArch`、启动安装器、能力快照、布局、策略和窗口绑到同一个架构身份上；`BootPageSupport<A>`、`KernelMapPolicy<A>`、`DmPagePolicy<A>`、`BootPagePolicy<A>`、`DirectMapLayout<A>`、`DmWindow<A>` 都不能跨架构混用。
- `DmPagePolicy` 用 `preferred_page_size: Option<PageSize>` 表达首选大页，不再把 1 GiB 写成算法里的特殊分支。
- `KernelMapPolicy<A>` 保存规划时验证过的 `KernelLayout`，`DmPagePolicy<A>` 保存验证过的 `DirectMapLayout<A>`；执行入口从策略取布局，不再重新读取另一份地址输入。
- `BootPageSupport<A>` 的生产构造只允许来自 `detect_boot_page_support`，`from_parts` 收在 `test-support` 后面；静态页大小配置由 `validate()` 统一检查。
- `install_boot_dm_range` 在第一次写表前完成范围端点、所有叶子地址、窗口边界和溢出的预检；底层安装失败时调用者必须终止启动，不回滚也不继续使用部分 Direct Map。
- `CurrentPaging`、`CurrentDirectMap`、`CurrentBootDmInstaller` 全部投影自 `CurrentBootArch`，不再各自维护一套架构选择。
- Direct Map 基址的大页对齐用编译期断言固定；Kernel DM 使用架构给出的有限 VA 容量，而不是 `u64::MAX`；规划器验证基址，安装器验证每个叶子的 `VA`/`PA` 对齐、范围和溢出。
- `DirectMapLayout::validate()` 要求布局与 `DirectMapArch` 的换算常量逐项相等（`kernel_base`、`kernel_pa_capacity`、`vm_base`、`vm_pa_capacity`、`vm_va_end`）：运行期地址换算用同一组常量做加法，验证允许偏离就等于允许策略与换算分家；测试注入偏离布局只能用来断言被拒绝，降级场景改用布局常量本身不对齐的 mock 架构。
- `boot_validate_and_prepare` 在能力检测后校验 `phys_addr_limit >= IDENTITY_MAP_END`，为 4 GiB 恒等窗口闭合 PA 上界；检查放在 kernel 启动规划层，`minix-arch` 只持有共享常量，不反向依赖 kernel 启动布局。
- 错误类型按 crate 分层：`PageSupportError`（能力事实）与 `DmPolicyError`（DM 规划）留在 `minix-arch`，`KernelMapError`（内核映射规划）与组合用的 `BootPagePolicyError` 留在 `minix-kernel`；下层错误经包装变体转发，不复制变体。
- `establish_boot_dm` 的全局预检与实际安装两遍遍历同一个候选生成器：候选枚举是对不可变输入的确定性纯函数，不依赖 bump 分配游标、不使用消费型迭代器，两遍发射序列逐项相同。
- 2 MiB 回退受支持是当前支持配置的硬性前提——这是支持策略边界，不是技术必然：三种架构的安装器都能安装基础页叶子，仅靠基础页建表在技术上可行。
- 运行期地址换算按三层保证分层（窗口内 / PA 可表达 / 实际可访问）：检查换算放在 `BootArch` 绑定层，正反向共用可换算上界 `min(窗口容量, phys_addr_limit)`，返回 `Option`，成功不等于映射存在；快速换算是安全纯算术 `_unchecked` 函数；unsafe 边界放在指针形成与内存访问处，不放整数加减上。
- `phys_addr_limit()` 在源头闭合编码上限：返回值是 `min(硬件可寻址上界, PTE 编码上界)`，位宽先验证再移位，异常输入（扩展叶缺失、零位宽、超范围位宽）启动失败；由此通用预检放行的地址编码器必然可表达。
- `validate_virtual_range()` 按声明的目标翻译模式做纯范围验证，不读活动寄存器；`phys_addr_limit()` 按架构取得有效 PA 上界——x86-64 查询 CPUID，ARM64/RISC-V 用声明的目标配置——两者都不得靠检测时的活动寄存器推断未来启用的翻译配置；`enable()` 负责落实目标配置并做写后断言。
- `BootArch` 与 `BootDmInstaller` 封口：可实现的集合限定在 `minix-arch` 内部（三内置架构 + mock），跨 crate 测试经 `test-support` 取 mock，不开外部扩展。
- `install_boot_dm_range` 与 `establish_boot_dm` 标为 `unsafe`，安全契约明确要求启动上下文；不再存在“安全函数内部偷偷调用 unsafe 安装器”的边界。
- 迁移顺序改为先修启动选页正确性，再统一能力 API，最后做机械重命名。

这套改动的核心不是“多加一层抽象”，而是**把硬件事实、启动规划和页表执行分开，让每一层只回答自己的问题，让非法状态在类型或构造边界上难以成立**。

## 2. 背景

### 2.1 启动期和运行期是两种执行条件

内核启动时可以假设三件事：

- 只有一个 CPU 在执行启动代码；
- 还没有通用物理页分配器，页表页来自 boot bump allocator（启动期只向前分配、不回收的简易分配器）；
- 当前页表根是 bootloader 或内核刚建好的 bootstrap root。

内核进入运行期后，这三条都不再成立：

- 多个 CPU 可能并发走页表；
- 页表页由运行期分配器管理；
- 当前页表根可能是某个用户进程的地址空间。

同一个“写一条页表项”的动作，在这两种条件下的安全前提、写通道、分配器和失败处理都不同。把两者硬塞进一个 trait，调用方就必须在运行时判断“我现在是启动还是运行”，这会把生命周期差异变成隐藏分支。

本文的设计基于当前代码的以下事实：

- x86-64 的 2 MiB 页是基线能力，1 GiB 页需要 CPUID 检查；
- ARM64 与 RISC-V 的 1 GiB 块在当前实现中视为始终可用；
- 三种架构的 `Paging::PAGE_SIZE` 都是 4096，`FALLBACK_HUGE_PAGE_SIZE` 都是 2 MiB；
- bootstrap 页表页和 boot bump 区域都在恒等映射与 DM 窗口可表示的物理范围内；
- 目前没有运行期消费者依赖 1 GiB 大页分配。

### 2.2 Direct Map 是什么

Direct Map 是内核的一种软件设计模式：划出一段固定的虚拟地址窗口，让物理地址 `PA` 与虚拟地址 `VA` 满足 `VA = PA + 基址`。硬件提供的仍然只是 MMU、页表和 TLB；Direct Map 是内核使用 MMU 的方式，不是硬件机制。

这里的 VM 指 Minix3 的用户态内存管理服务器，不是虚拟机。本项目在启动期为两个窗口建立映射：

- **Kernel DM**：内核使用的窗口，监督态可读写，物理地址范围通常不截断。
- **VM DM**：VM 服务器使用的窗口，用户态可读写，物理地址范围被窗口大小截断。

运行期内核访问物理页时，通过 `DirectMapArch::kernel_phys_to_virt` 得到 Kernel DM 地址；VM 访问物理页时，通过 `DirectMapArch::vm_phys_to_virt` 得到 VM DM 地址。也就是说，**窗口一旦建好，运行期只做地址加法，不再重建窗口**。但“只做加法”自带前提：加法输入的物理地址必须落在该窗口的容量内，否则算出的虚拟地址不属于这个窗口。这个前提在当前代码里只存在于个别调用点的安全注释中，§6.11 把它升级为换算接口的显式契约。

### 2.3 大页的三个层次

以 x86-64 为例，页表从高到低是 PML4、PDPT、PD、PT。叶条目可以指向：

- 1 GiB 页：在 PDPT 层直接作为叶；
- 2 MiB 页：在 PD 层直接作为叶；
- 4 KiB 页：在 PT 层作为普通叶。

x86-64 用条目里的 PS 位标识“这是大页”。1 GiB 页不是所有 CPU 都支持，要看 `CPUID.80000001H:EDX.GBPAGES`（第 26 位）。查询该叶之前必须先读取 `CPUID.80000000H.EAX`，确认最大扩展叶至少为 `0x80000001`；否则 GBPAGES 位没有定义。ARM64 和 RISC-V 的 1 GiB 块在当前实现中视为始终可用。

这里有一个容易混淆的点，必须拆成三个问题：

| 问题 | 答案归属 |
|---|---|
| 当前内核实现有没有这个页大小的代码路径 | `HugePages::HUGE_PAGE_SIZES` |
| 目标硬件能不能解释这个页大小的叶子 | `HugePages::supports_huge_page(size)` |
| 当前这次映射的地址和长度是否满足对齐、包含关系 | 启动策略或区间规划函数 |

`supports_huge_page(size)` 只回答第二个问题，不回答第三个问题。本文会把它的实现和契约补完整。

页大小是启动策略、能力查询和页表写入共同使用的数据。目标接口用轻量 `PageSize` 新类型表示它，避免同一个尺寸在 `u64` 和 `usize` 之间反复转换，也避免把任意整数直接传进页表操作。`Paging::PAGE_SIZE` 仍然保留为基础页表遍历的 `usize` 常量；`PageSize` 与 `usize` 的转换只发生在明确的能力检测或底层写入边界，并且转换失败必须显式处理。

`PageSize` 只证明“这个数值是一个合法页大小表示”，不证明“目标架构或硬件支持这个叶子尺寸”。后者只能由 `HugePages::supports_huge_page(PageSize)` 回答。把 `PageSize::GIB_1` 传给一个不支持 1 GiB 的实现是合法的类型操作，但会产生 `NotSupported`，而不是被类型系统当成硬件能力令牌。

### 2.4 Minix3 C 的基线

32 位 Minix3 的 i386 实现使用 4 MiB 大页，页目录项直接指向 4 MiB 物理块。它没有“1 GiB 页能力协商”这个问题，因为 4 MiB 是该架构固定布局的一部分。

64 位 Rust 实现要在同一套外部行为下支持三种架构，页大小变成可变的硬件能力。于是“选多大的页”从一个编译期常量变成了启动期策略。这是实现层的架构演进，不改变“内核能启动、Direct Map 可用、VM 能接管内存”的外部行为。

### 2.5 SMP 让“能力”从单 CPU 问题变成系统问题

x86-64 的 AP 启动路径把 BSP 当前的 `CR3` 物理地址写进 AP bootstrap 记录，AP 加载同一个页表根。也就是说，BSP 在 `arch_boot_impl` Step 4 建立的 Direct Map 大页，会被所有 AP 共同走过。

AP 启动发生在 `kmain` 的 Phase F，也就是 `arch_boot_impl` 完成、内核进入高层入口之后。此时 BSP 已经选好页大小并写入共享页表。AP 没有机会在 BSP 选页之前说“我不支持这个大小”。

当前 x86-64 的能力探针只覆盖执行探针的 CPU。`cpu_identify` 的注释也说明 AP 路径尚未填充 CPU 身份表；AP 启动路径没有校验页大小能力。因此“BSP 支持 1 GiB”目前被当成“所有 CPU 支持 1 GiB”，这是平台同构假设，不是代码验证过的系统级事实。

本文把 `supports_huge_page(size)`、`phys_addr_limit()` 和共享页表的翻译配置都定义为**系统级契约**：所有可能访问受影响页表的 CPU 都必须兼容。当前实现只支持同构 SMP；异构组合明确不在支持范围内。详见 §6.9。

## 3. 现有结构

### 3.1 四个角色

当前代码实际上有四个职责面：

| 角色 | 回答的问题 | 生命周期 | 代表接口 |
|---|---|---|---|
| 地址布局 | Direct Map 窗口在虚拟地址空间的哪里，如何做 `PA ↔ VA` 换算 | 编译期常量 | `DirectMapArch` |
| 运行期页表操作 | 运行期如何创建、映射、查询、切换页表 | 运行期，按句柄 | `Paging` |
| 大页能力与操作 | 这台机器支持哪些大页，如何映射一个大页 | 运行期能力 + 启动期也会用 | `HugePages: Paging` |
| 启动期安装 | 如何在 bootstrap root 上用恒等写通道安装 Direct Map 叶子 | 只在启动期 | `DmCoverageArch` |

`DirectMapArch` 只放布局与换算，这是正确的。它不应该知道 1 GiB 页是否可用，因为“窗口放在哪里”和“硬件能映射多大”是两件事。

`Paging` 负责运行期页表操作。它的 `from_active_root` 绑定 Kernel DM，VM 侧的 `adopt_active_root` 绑定 VM DM。这个前提是“DM 窗口已经存在”。

`HugePages: Paging` 是 `Paging` 的扩展，负责大页尺寸、映射操作和硬件能力。它既服务于运行期，也被启动期使用。

`DmCoverageArch` 是启动期专用：它不持有句柄，不依赖运行期分配器，所有页表访问都走 bootstrap root 的恒等映射 `VA = PA`。

### 3.2 启动顺序

当前启动链可以简化为：

```
arch_boot_impl<P: HugePages>
  │
  ├─ boot_validate_and_prepare<P>()
  │    └─ 选 kern_huge
  │
  ├─ build_bootstrap_root_and_enable<P>(kern_huge)
  │    ├─ 恒等映射 [0, 4 GiB)
  │    ├─ 内核高半映射
  │    └─ 开启分页
  │
  └─ establish_boot_dm()
       ├─ Kernel DM 先建
       ├─ VM DM 后建
       └─ 每个候选区间调用 establish_dm_range::<CurrentDmCoverage>()

之后进入 kmain
  └─ smp_init()
       └─ AP 加载当前 CR3（同一个 bootstrap root）
```

两个启动入口都会走到 `establish_boot_dm`：

- `arch_boot_impl` 的 Step 4；
- `arch_boot_resume_high_half` 的恢复路径。

之后内核进入 `kmain`，运行期不再调用 `DmCoverageArch`。VM 运行期的页表操作走 `Paging` 句柄和 `DirectMapArch` 的地址换算，不会回到启动期安装器。

### 3.3 为什么说它是启动期专用

`DmCoverageArch` 的 trait 注释写得很清楚：它是 bootstrap root 上的静态表操作，不是按句柄的页表服务。实现者都是零大小类型，不需要保存根地址或写通道。调用点也只有 `establish_boot_dm`。

从依赖关系看，它也不能轻易改成运行期服务：

- 它必须在 DM 窗口建立之前工作，所以不能用 DM 窗口作为写通道；
- 它使用 boot bump allocator 分配中间页表页；
- 它假设单线程，不需要 TLB shootdown；
- 它在 x86-64 上要处理“VM DM 窗口与恒等映射重叠”的特殊拆分策略。

这些前提在运行期都不成立。因此“启动期安装器”和“运行期分页服务”分开是合理的。

### 3.4 当前的能力双入口

`HugePages` 里有一个能力方法：

```rust
pub trait HugePages: Paging {
    const HUGE_PAGE_SIZES: &'static [usize];
    const HUGE_PAGE_SIZE: u64;
    const FALLBACK_HUGE_PAGE_SIZE: u64;

    fn supports_huge_page(size: usize) -> bool {
        Self::HUGE_PAGE_SIZES.contains(&size)
    }

    fn supports_1gb_page() -> bool {
        true
    }
}
```

`DmCoverageArch` 里也有一个：

```rust
pub trait DmCoverageArch {
    fn supports_1gb_page() -> bool {
        true
    }

    unsafe fn dm_install_leaf(
        root_paddr: PhysBytes,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;
}
```

x86-64 的两个实现都转发到同一个 `probe_1gb_page_support()`：

```rust
fn probe_1gb_page_support() -> bool {
    // CPUID.80000001H:EDX.GBPAGES, bit 26
}
```

所以重复的不是 CPUID 探针本身，而是 **trait 入口**。今天两个入口碰巧一致，明天新增一个架构或修改一个默认实现，就可能只改一边。

现有测试 `test_1gb_capability_faces_agree` 只在 x86-64 上比较两个入口，它能发现“某一次修改让 x86-64 的转发断掉”，但防不住其他架构的默认实现漂移。宿主测试使用的 `MockPaging::supports_1gb_page` 又恒为真，因此真正的“CPU 不支持 1 GiB”降级路径不在宿主测试的覆盖范围内。

### 3.5 当前的启动选页

`boot_validate_and_prepare<P: HugePages>` 的逻辑是：

```rust
let huge = P::HUGE_PAGE_SIZE;
let fallback = P::FALLBACK_HUGE_PAGE_SIZE;

let kern_huge =
    if kv.is_multiple_of(huge)
        && kp.is_multiple_of(huge)
        && ks.is_multiple_of(huge)
    {
        huge
    } else if kv.is_multiple_of(fallback)
        && kp.is_multiple_of(fallback)
        && ks.is_multiple_of(fallback)
    {
        fallback
    } else {
        panic!("... must be aligned to at least FALLBACK_HUGE_PAGE_SIZE");
    };
```

这段代码只检查对齐，没有检查 `P::supports_1gb_page()`，也没有检查 `P::supports_huge_page(huge)`。也就是说，**“布局允许 1 GiB 页”被当成了“CPU 支持 1 GiB 页”**。这里的 `kern_huge` 同时用于恒等映射和内核高半映射，所以选错大小会同时影响两条启动映射。

在 x86-64 上，如果 CPU 没有 GBPAGES 能力，但内核镜像恰好按 1 GiB 对齐，启动代码仍会选择 1 GiB，随后 `map_huge` 会在 PDPT 层写入带 PS 位的条目。这样的条目在这台 CPU 上不能形成合法翻译，启动会在页表切换后失败。

更值得注意的是，Direct Map 建立路径走的是另一个判断：`establish_dm_range` 调用 `P::supports_1gb_page()`，在没有 1 GiB 能力时会退回 2 MiB。于是同一台机器上可能出现：

- 内核镜像映射按 1 GiB 选页；
- Direct Map 覆盖按 2 MiB 选页。

两条路径对同一个硬件事实给出了不同答案。这不是页面大小偏好问题，而是启动策略没有使用能力查询。

### 3.6 `supports_huge_page` 的语义缺口

`supports_huge_page` 现在的默认实现只查 `HUGE_PAGE_SIZES`。x86-64 的静态列表包含 1 GiB，所以：

```rust
X86_64Paging::supports_huge_page(1 << 30) == true
```

即使当前 CPU 的 GBPAGES 位是 0，这个表达式仍然为真。它回答的是“架构上有没有这个尺寸”，不是“这台 CPU 能不能用这个尺寸”。

目前这个方法主要被测试使用，生产路径还没有依赖它，所以问题没有暴露。但接口名字是能力查询，语义却不完整。未来任何运行期消费者只要按名字使用它，就会得到一个过于乐观的答案。

### 3.7 安装器内部做策略

`establish_dm_range` 目前这样取能力：

```rust
plan_range_units(clipped, P::supports_1gb_page(), 4096, |pa, size| {
    unsafe { P::dm_install_leaf(...) }
})
```

这带来三个后果：

- 安装器同时负责“怎么安装”和“选多大页”，机制与策略混在一起；
- 测试一个安装器时，必须同时模拟硬件能力，无法只验证安装路径；
- `one_gb` 是裸布尔值，调用方没有契约约束；`4096` 和 2 MiB 回退也是隐式的。

选页策略本身已经在 `plan_range_units` 里，并且接收显式的 `one_gb: bool`。这说明真正的策略函数已经设计成可测试的，只是上层驱动又把能力查询塞回了 trait。

### 3.8 SMP 与页表根

x86-64 的 `smp_init` 在 `kmain` 中启动 AP。AP bootstrap 记录里的 `page_table_root_pa` 来自 BSP 当前的 `current_cr3_pa()`，因此 AP 加载与 BSP 相同的 bootstrap root。该 root 已经包含 Step 4 建好的 Kernel DM 和 VM DM 叶子。

当前代码没有在 AP 路径检查页大小能力：

- `supports_huge_page` 只在执行探针的 CPU 上求值；
- AP 的 CPU 身份表填充仍标注为后续 SMP 工作；
- 没有“所有 CPU 能力交集”或“AP 拒绝加载不支持的大页根”的机制。

因此当前实现隐含了“所有 CPU 的页大小能力相同”这一平台假设。它没有被写进接口，也没有被测试直接验证。

### 3.9 隐藏的 4 KiB 与 2 MiB 前提

当前三种架构都满足：

```rust
const PAGE_SIZE: usize = 4096;
const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;
const HUGE_PAGE_SIZES: &[usize] = &[1 << 30, 1 << 21];
```

但驱动函数把 4096 直接传给 `plan_range_units`，而 `plan_range_units` 内部把 2 MiB 写死。只要新增一种架构或改变回退尺寸，调用链就会悄悄失配。

这不是当前必须立即修复的 bug，但它是一个没有被表达的前提。本文要求把这两个值放进启动策略对象，或者用编译期断言和测试固定下来。

## 4. 问题归纳

### 4.1 命名把启动期说成了运行期

`DmCoverageArch` 这个名字有三个问题：

- `Coverage` 描述的是结果，不是动作；
- `Arch` 描述的是实现方式，不是职责；
- 名字里没有出现 `Boot`，读者无法从类型名判断它只在启动期使用。

`dm_install_leaf` 同样没有说明这是 bootstrap root 上的安装动作。`CurrentDmCoverage`、`X86_64DmCoverage`、`MockDmCoverage` 也继承了同样的模糊性。

### 4.2 能力有两个 trait 入口

同一个硬件事实出现在两个 trait 上，会带来三类风险：

- **漂移**：新增架构时只实现一个 trait 的能力方法，另一个走默认值；
- **误用**：调用方以为两个入口分别代表不同生命周期，实际问的是同一个问题；
- **测试假安全**：一个“两个入口一致”的测试只能证明当前实现一致，不能阻止未来新增第三个入口。

能力不是生命周期的一部分。运行期和启动期需要的是同一个事实，只是消费方式不同。

### 4.3 选页策略与能力查询分裂

启动选页只看对齐，Direct Map 建立看能力。两条路径的输入不同，输出可能不同。

正确的策略应该是：

1. 先问“这个页大小在当前系统上可用吗”；
2. 再问“当前布局是否满足这个大小的对齐和长度条件”；
3. 两者都成立才选它，否则降级。

当前实现漏掉了第 1 步。

### 4.4 默认实现把“有代码路径”等同于“硬件支持”

目标 trait 如果保留：

```rust
fn supports_huge_page(size: PageSize) -> bool {
    Self::HUGE_PAGE_SIZES.contains(&size)
}
```

新架构只要不覆写这个方法，就会自动得到“静态列表里有的尺寸都受硬件支持”的承诺。当前 ARM64 和 RISC-V 可以证明这个默认值对它们成立，但接口没有强迫未来架构也证明一次。

这不是要求三个架构写三套复杂算法，而是要求每个架构明确承担自己的能力保证。默认实现会让“忘记实现”看起来像“已经支持”。

### 4.5 能力获取和纯规划混在一起

`plan_boot_pages<P>` 如果内部调用 `P::supports_huge_page`，它就不再是仅由显式输入决定结果的纯函数。它同时承担：

- 与硬件环境交互；
- 读取架构常量；
- 计算内核映射和 Direct Map 策略。

测试因此只能通过修改 `MockPaging` 的全局能力状态来间接影响规划，无法直接注入“首选支持、回退不支持”这类事实组合。

### 4.6 `DmPagePolicy` 仍把 1 GiB 写成特殊分支

`DmPagePolicy` 携带 `one_gib`、回退大页和基础页，但 `plan_range_units` 仍然写死“1 GiB 优先，再回退”。这意味着：

- 策略对象没有完整表达实际选页规则；
- 1 GiB 是平台固定首选尺寸这一前提只存在于算法实现里；
- 新增另一种首选大页时，必须同时修改策略类型和规划算法。

本文不采用“永远把 1 GiB 定义为 Direct Map 首选，同时保留 `one_gib`”的折中。`preferred_page_size: Option<PageSize>` 已经足以表达当前策略，而且不会把 1 GiB 重新写回算法。

### 4.7 策略没有架构身份，调用点退化成裸数值

`plan_boot_pages<P>` 根据 `P` 获取能力，但返回的 `BootPagePolicy` 不携带 `P`。一个基于 x86-64 能力生成的策略，在类型上不能阻止被交给 ARM64 的安装器。

同时，`build_bootstrap_root_and_enable` 只收到 `policy.kernel_page_size()` 返回的 `u64`。它无法知道这个值是否来自当前启动策略，也无法阻止调用方直接传一个任意对齐值。

### 4.8 Direct Map 虚拟地址约束没有进入规划契约

`plan_range_units` 接收物理范围和页大小策略，不知道 `va_base`。最终叶子地址是 `va_base + pa`。

对于大小 `S`，至少需要：

```text
PA mod S = 0
VA mod S = 0
VA = va_base + PA
```

如果 `PA` 已经按 `S` 对齐，还需要：

```text
va_base mod S = 0
```

当前三种架构的 Kernel DM 和 VM DM 基址都按 1 GiB 对齐，但这个事实没有写进 `DmPagePolicy`、`plan_range_units` 或编译期断言。物理范围对齐不等于最终虚拟地址对齐。

此外，`va_base + pa`、`pa + size`、`va + size` 都可能溢出。它们不一定在当前平台触发，但属于启动安装器的完整正确性边界。

### 4.9 SMP 不变量需要精确表述

`supports_huge_page` 是静态方法，没有“当前 CPU”参数；`BootDmInstaller` 也没有系统范围参数。代码实际上把能力当成全局事实，但文档需要说明：

- 它是当前支持平台前提下的系统级保证；
- 它不是由某一次 CPUID 查询独立证明的事实；
- `-smp4` 只验证同构配置，不证明异构 CPU 能力安全。

如果未来出现页大小能力不同的 CPU，当前实现会在 AP 加载共享根时失败。修复成本远高于现在把平台契约写清楚。

### 4.10 页大小前提没有显式化

`4096`、2 MiB 和 1 GiB 是当前三种架构的共同事实，但它们不是策略对象的完整契约。尺寸合法性、基础页/回退/首选的大小顺序、尺寸是否在架构页大小集合内、窗口基址对齐，需要一套集中验证，而不是只检查一个整数能否被 4 KiB 整除。合法的 `PageSize` 都是 2 的幂，大小顺序一旦成立，整除关系就自动成立，不应再把它当成独立的运行时条件。

### 4.11 后果推演

| 场景 | 当前行为 | 后果 |
|---|---|---|
| CPU 不支持 1 GiB，镜像按 1 GiB 对齐 | 启动选页选 1 GiB；DM 选 2 MiB | 内核映射写入不支持的 1 GiB 条目，启动失败；两条路径结论不一致 |
| CPU 支持 1 GiB，但某个 trait 默认实现写错 | 一条路径用 1 GiB，另一条可能用 2 MiB | 性能下降或行为不一致，测试只在 x86-64 覆盖 |
| 未来架构不支持 1 GiB，但默认方法返回真 | 启动选页选 1 GiB | 新架构首次启动失败 |
| 未来运行期消费者调用 `supports_huge_page(1 GiB)` | 得到静态列表的答案 | 可能误以为 CPU 支持，映射时报错 |
| 安装器继续自己查能力 | 测试无法单独验证安装路径 | 策略和机制耦合，回归定位困难 |
| DM 基址没有按所选大页对齐 | 物理区间对齐，最终 VA 不对齐 | 页表项非法或安装器静默写错 |
| `va_base + pa` 溢出 | 可能回绕到低地址 | 页表写到错误位置 |
| SMP 平台 CPU 页大小能力不同 | BSP 按 1 GiB 建表，AP 加载同一 root | AP 可能无法走页表；当前没有检测或降级路径 |

## 5. 设计原则

### 5.1 一个硬件事实，一个来源

“当前系统是否支持某个大页尺寸”只能有一个权威来源。其他接口如果需要，应该转发或接收这个值，而不是重新定义一遍。能力快照按页大小保存唯一答案，不按消费者保存 `kernel_*` 和 `dm_*` 两份可能漂移的副本。

### 5.2 生命周期分开，能力不分开

启动期安装器和运行期分页句柄的安全前提不同，应该分开。但“支持哪些页大小”不随生命周期变化，不应该跟着分开。

### 5.3 策略在调用方，机制在实现方

- 选多大页：调用方的策略；
- 怎么把一个叶子写进页表：安装器的机制；
- 页表布局在哪里：`DirectMapArch` 的常量；
- 运行期怎么改页表：`Paging` 的机制。

### 5.4 统一能力事实，分别规划映射

启动期间只获取一次能力事实，但内核映射和 Direct Map 可以有不同的页大小决策：

- 能力事实共享：不同映射不能各自判断硬件是否支持 1 GiB；
- 映射策略独立：内核镜像可能只满足 2 MiB 对齐，Direct Map 仍可在完整对齐的物理区间上使用 1 GiB；
- Kernel DM 和 VM DM 必须共享同一份 Direct Map 策略。

“一次启动决策”这个说法不够准确。准确的表述是：**启动期间统一获取能力事实，分别规划各类映射；同类窗口共享策略。**

### 5.5 能力获取与规划分离

硬件能力查询是事实获取，启动策略是纯计算。`detect_boot_page_support` 负责前者，`plan_kernel_mapping`、`plan_dm_policy`、`plan_boot_pages` 负责后者。纯规划函数不应隐藏硬件探测。

### 5.6 能力是系统级平台契约

如果页表会被多个 CPU 共享，能力真值必须覆盖所有会走这张表的 CPU，而且不仅是页大小：`phys_addr_limit`、页表格式和翻译配置也必须兼容。只探测 BSP 的结果不足以证明系统级能力，除非平台明确保证同构。

在当前支持边界内，`supports_huge_page(size) == true` 和 `phys_addr_limit()` 都是“同构平台前提下的系统级保证”，不是“某次 CPUID/TCR 查询独立证明整台机器”的事实。文档和测试必须保持这个区别。

### 5.7 所有启动输入和输出共享架构身份

架构身份不能只停在策略类型上。能力事实来自某个分页实现，Direct Map 布局来自某个 `DirectMapArch`，叶子安装器也属于同一个架构。一个架构的能力快照、布局、策略和窗口必须能互相组合，另一个架构的对应物不能混进来。

`BootArch` 不是第五个职责，而是一个零大小标记：把分页实现、地址布局和启动安装器这三个已有角色绑定成一个命名架构。

### 5.8 地址约束进入契约

物理区间对齐、虚拟基址对齐、有效 VA 窗口、溢出检查，都是启动安装器正确性的一部分。不能只验证物理范围，然后把最终虚拟地址留给实现者“应该知道”。Kernel DM 的 `va_end` 必须来自架构布局，不能用 `u64::MAX` 代替“没有上界”。可以在写表前发现的几何错误必须在第一次页表写入前全部检查完；写表过程中才出现的底层错误一旦发生，启动就必须终止，不能继续使用部分映射。

### 5.9 类型名要说真话

名字应该让读者在看不到实现时就知道生命周期和职责。`BootDmInstaller` 比 `DmCoverageArch` 更接近事实。

### 5.10 不做投机性泛化，但要把前提显式化

当前只有 1 GiB 这一个动态能力需要查询，不需要引入完整的 `HugePageCaps` 框架。但 `PageSize`、`BootPageSupport<A>`、`KernelMapPolicy<A>`、`DmPagePolicy<A>` 不是投机性泛化，它们是当前正确性的一部分；`BootArch` 也只承担架构身份绑定，不承担能力查询。

## 6. 目标设计

### 6.1 目标职责与架构绑定

| 角色 | 目标接口 | 生命周期 | 是否查询硬件能力 |
|---|---|---|---|
| 架构身份 | `BootArch` | 编译期标记 | 否 |
| 地址布局 | `DirectMapArch` | 编译期 | 否 |
| 运行期页表 | `Paging` | 运行期 | 否 |
| 大页能力与操作 | `HugePages: Paging` | 运行期 + 启动期 | 是，唯一来源 |
| 启动期安装 | `BootDmInstaller` | 只在启动期 | 否，能力由已验证策略传入 |

`BootArch` 只做类型绑定，不增加运行时对象，也不承担能力查询：

```rust
// crate 根（os/arch/src/lib.rs）：封口标记模块，各模块的封口
// trait 引用同一处。
mod private {
    /// 封口标记：只在 `minix-arch` 内部实现，外部 crate 无法命名它。
    pub trait Sealed {}
}

pub trait BootArch: crate::private::Sealed + 'static {
    type Paging: HugePages;
    type DirectMap: DirectMapArch;
    type DmInstaller: BootDmInstaller<Self::Paging>;
}
```

每个架构提供一个零大小标记实现，例如：

```rust
pub struct X86_64BootArch;

impl BootArch for X86_64BootArch {
    type Paging = X86_64Paging;
    type DirectMap = X86_64DirectMap;
    type DmInstaller = X86_64BootDmInstaller;
}
```

`CurrentBootArch` 按目标架构选择对应标记，是唯一的架构选择点。旧的 `CurrentPaging`、`CurrentDirectMap`、`CurrentBootDmInstaller` 不应再各自维护一份 `#[cfg(target_arch)]`，而要全部投影自 `CurrentBootArch`：

```rust
pub type CurrentPaging = <CurrentBootArch as BootArch>::Paging;
pub type CurrentDirectMap = <CurrentBootArch as BootArch>::DirectMap;
pub type CurrentBootDmInstaller =
    <CurrentBootArch as BootArch>::DmInstaller;
```

这样运行期代码不需要一次性改写所有调用点，也不会出现“`CurrentBootArch` 选了一个架构，`CurrentPaging` 却独立选择另一个类型”的分裂。`BootArch` 的关联类型组合仍可能被实现者写错，因此每个 `BootArch` 实现都要有注册一致性测试，例如用类型相同的辅助函数检查 `X86_64BootArch::Paging == X86_64Paging`、`DirectMap == X86_64DirectMap`、`DmInstaller == X86_64BootDmInstaller`；这比再引入一个架构 trait 更轻。

`BootArch` 封口（sealed）：用私有超 trait 惯用法把可实现的集合限定在 `minix-arch` 内部——三个内置架构加一个测试用 mock。封口的理由：关联类型的“属于同一套真实翻译配置”这一一致性只能由项目维护者证明；类型系统只检查 trait bound 匹配，挡不住外部实现者组合语法合法但语义不一致的 `Paging`/`DirectMap`/`DmInstaller`。项目目标只有三种架构，把扩展义务收回到注册一致性测试能覆盖的范围内，比 `unsafe trait BootArch`（把安全义务转嫁给未必能承担的实现者）更诚实。跨 crate 测试需要 mock 架构时，经 `test-support` feature 使用 `minix-arch` 内部提供的 `MockBootArch`，不开放 trait 本身。`BootDmInstaller` 的声明同样带 `private::Sealed` 超 trait，mock 安装器只在 `test-support` 下提供。

封口防止的是**外部实现**，不防止对已有方法的直接调用，这两层保证必须分开表述：sealed trait 让外部 crate 无法提供自己的架构组合或安装器，这一层由类型系统强制；`install_leaf` 仍是公开的 `unsafe fn`，有意进入 `unsafe` 块的调用者仍然可以直接调用它——`install_boot_dm_range` 是生产代码的**受支持入口**，这条约束由调用点审查、`rg` 检查与测试维持，不是类型系统强制的。文档不声称封口能让所有安装都经过预检。

### 6.2 命名调整

| 当前名字 | 建议名字 | 理由 |
|---|---|---|
| `DmCoverageArch` | `BootDmInstaller` | 直接说明“启动期 Direct Map 安装器” |
| `dm_install_leaf` | `install_leaf` | trait 已经限定为启动期安装器，方法名不必重复 DM |
| `establish_dm_range` | `install_boot_dm_range` | 动词和生命周期都写清楚 |
| `CurrentDmCoverage` | `CurrentBootDmInstaller` | 类型别名跟随 trait |
| `X86_64DmCoverage` | `X86_64BootDmInstaller` | 架构实现跟随 trait |
| `AArch64DmCoverage` | `AArch64BootDmInstaller` | 同上 |
| `Riscv64DmCoverage` | `Riscv64BootDmInstaller` | 同上 |
| `MockDmCoverage` | `MockBootDmInstaller` | 测试实现跟随 trait |
| `arch/dm_coverage.rs` | `arch/boot_dm.rs` | 模块名与职责一致 |
| 布局/窗口字段 `kernel_pa_limit`/`vm_pa_limit`/`window.pa_limit`（目标设计）与现有 `establish_dm_range` 的 `window_pa_limit` 参数 | `pa_capacity` 系命名（`kernel_pa_capacity`/`vm_pa_capacity`/`window.pa_capacity()`） | 「窗口能覆盖多少物理偏移」是容量，「处理器能安全寻址多高」是 `phys_addr_limit`；两个不同上界共用 `limit` 一词会诱发误认，容量语义用 `capacity` 与之分开 |

`DmRange` 可以保留。它描述的是“Direct Map 候选物理范围”，在 `boot_dm` 模块上下文里不会歧义。`establish_boot_dm` 也可以保留，因为它已经明确写了 boot。

### 6.3 `PageSize` 与能力唯一来源

页大小是能力查询、策略和页表写入共同使用的数据。目标接口用轻量新类型表示它：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageSize(u64);

impl PageSize {
    pub const KIB_4: Self = Self(4096);
    pub const MIB_2: Self = Self(1 << 21);
    pub const GIB_1: Self = Self(1 << 30);

    /// 只接受不小于 4 KiB 的 2 的幂。
    pub fn try_from_bytes(bytes: u64) -> Option<Self> {
        (bytes >= 4096 && bytes.is_power_of_two()).then_some(Self(bytes))
    }

    pub fn try_from_usize(bytes: usize) -> Option<Self> {
        u64::try_from(bytes).ok().and_then(Self::try_from_bytes)
    }

    pub const fn bytes(self) -> u64 { self.0 }

    /// 目标平台无法表示该尺寸时返回 `None`，不做静默截断。
    pub fn try_as_usize(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}
```

这里有两个不同的保证：

| 保证 | 由谁负责 | 反例 |
|---|---|---|
| 数值是一个不小于 4 KiB 的 2 的幂 | `PageSize` 的私有字段和构造函数 | `try_from_bytes(3000)` 返回 `None` |
| 当前架构和目标硬件能解释这个叶子尺寸 | `HugePages::supports_huge_page` | `PageSize::GIB_1` 在没有 GBPAGES 的 CPU 上返回 `false` |

因此 `PageSize::GIB_1` 本身不是能力令牌。`try_as_usize` 只处理“目标平台的 `usize` 能不能表示这个字节数”，不改变硬件能力语义；三种 64 位目标上当前尺寸都能表示，但接口不依赖这个事实。

目标 trait 只保留一个公开能力查询，而且**没有默认实现**：

```rust
pub trait HugePages: Paging {
    const HUGE_PAGE_SIZES: &'static [PageSize];
    const HUGE_PAGE_SIZE: PageSize;
    const FALLBACK_HUGE_PAGE_SIZE: PageSize;

    /// 当前内核实现和目标硬件是否支持这个页大小。
    ///
    /// 返回 `true` 的前提有两条：`size` 出现在 `HUGE_PAGE_SIZES` 中，
    /// 并且当前目标硬件能够解释这个叶子尺寸。只探测硬件位、却遗漏
    /// 当前实现是否有对应叶尺寸代码路径，不是合法实现。
    ///
    /// 在 SMP 平台上，这个答案必须是系统级的：所有可能访问
    /// 受影响页表的 CPU 都必须能解释这个尺寸。
    fn supports_huge_page(size: PageSize) -> bool;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: PageSize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;
}
```

每个架构必须显式承担自己的能力保证。

x86-64 在静态列表基础上增加 CPUID 检查：

```rust
fn probe_1gb_page_support() -> bool {
    // 先确认扩展叶存在，再读 GBPAGES 位。
    let max_extended = unsafe { cpuid(0x8000_0000) }.eax;
    if max_extended < 0x8000_0001 {
        return false;
    }
    let edx = unsafe { cpuid(0x8000_0001) }.edx;
    edx & (1 << 26) != 0
}

impl HugePages for X86_64Paging {
    fn supports_huge_page(size: PageSize) -> bool {
        if !Self::HUGE_PAGE_SIZES.contains(&size) {
            return false;
        }
        if size == PageSize::GIB_1 {
            probe_1gb_page_support()
        } else {
            true
        }
    }
}
```

`cpuid` 是说明用的内部辅助函数，真实实现使用 `core::arch::x86_64::__cpuid` 或现有封装。如果现有封装已经检查 `0x8000_0000H` 的最大扩展叶，直接复用；否则必须在探针内部补上这一层，不能把这个前提留给调用者。探针还应提供一个可注入 `(max_extended, edx)` 的内部函数，让 host 测试能覆盖“最大扩展叶不足”和“扩展叶存在但 GBPAGES 为 0”两条路径。

ARM64 和 RISC-V 也显式实现，不再依赖 trait 默认值：

```rust
impl HugePages for AArch64Paging {
    fn supports_huge_page(size: PageSize) -> bool {
        Self::HUGE_PAGE_SIZES.contains(&size)
    }
}
```

它们的 `HUGE_PAGE_SIZES` 必须按当前启用的翻译模式填写，例如 4 KiB granule、当前页表级数和 RISC-V Sv39 的对齐规则；不能写成“架构上可能存在这个尺寸”。当前 ARM64/RISC-V 的静态列表是 `[1 GiB, 2 MiB]`，所以实现必须用编译期配置/断言把列表与目标模式绑定，不能只凭静态列表声称硬件能力。若未来支持不同 granule、Sv48 或其他翻译模式，列表和 `validate_virtual_range()`/`phys_addr_limit()` 必须一起重算。这仍然是“实现列表”的问题，不再另开一个能力入口。

`Paging::PAGE_SIZE` 仍然是基础页表遍历使用的 `usize`；`detect_boot_page_support` 在边界处把它转换为 `PageSize`。`map_huge` 接收 `PageSize`，底层实现只在计算页表项位移时取字节数。

`MockPaging` 必须可配置：

- 默认值可以保持“支持 1 GiB”，以免破坏现有用例；
- 降级测试必须显式关闭 1 GiB；
- 规划测试应优先注入 `BootPageSupport<A>` 事实，而不是依赖 `MockPaging` 全局状态；
- 如果使用全局测试开关，必须用现有串行锁隔离，避免并行测试互相污染。

契约表：

| 查询 | 回答 | 不回答 |
|---|---|---|
| `supports_huge_page(size)` | 当前实现和目标硬件是否支持这个尺寸 | 某次映射的地址/长度是否对齐 |
| `HUGE_PAGE_SIZES` | 当前实现有哪些叶尺寸代码路径 | 目标硬件是否支持这些尺寸 |
| `plan_kernel_mapping` / `plan_dm_policy` | 这次布局该选哪个尺寸 | 硬件是否支持 |

删除 `supports_1gb_page` 后，现有 `test_1gb_capability_faces_agree` 不再有意义。它应改为 x86-64 模块内部的“`supports_huge_page(1 GiB)` 与私有探针一致”测试，或者直接由 x86-64 的能力测试覆盖。每个架构的显式实现由编译期“缺少方法”错误和架构能力测试共同保证。

### 6.4 能力事实：`BootPageSupport<A>`

能力检测只回答“系统支持什么”，不回答“这次启动选什么”。目标类型把事实和规划分开：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSupport {
    size: PageSize,
    supported: bool,
}

pub struct BootPageSupport<A: BootArch> {
    base_page_size: PageSize,
    /// 当前平台/CPU/翻译配置实际允许的物理地址排他上界。
    phys_addr_limit: u64,
    preferred: PageSupport,
    fallback: PageSupport,
    _arch: PhantomData<fn() -> A>,
}

impl PageSupport {
    pub const fn new(size: PageSize, supported: bool) -> Self {
        Self { size, supported }
    }

    pub const fn size(&self) -> PageSize { self.size }
    pub const fn supported(&self) -> bool { self.supported }
}

impl<A: BootArch> Copy for BootPageSupport<A> {}

impl<A: BootArch> Clone for BootPageSupport<A> {
    fn clone(&self) -> Self { *self }
}

impl<A: BootArch> BootPageSupport<A> {
    /// 仅供 `minix-arch` 自身测试和启用 `test-support` 的跨 crate 测试使用。
    #[cfg(any(test, feature = "test-support"))]
    pub const fn from_parts(
        base_page_size: PageSize,
        phys_addr_limit: u64,
        preferred: PageSupport,
        fallback: PageSupport,
    ) -> Self {
        Self {
            base_page_size,
            phys_addr_limit,
            preferred,
            fallback,
            _arch: PhantomData,
        }
    }

    pub const fn base_page_size(&self) -> PageSize { self.base_page_size }
    pub const fn phys_addr_limit(&self) -> u64 { self.phys_addr_limit }
    pub const fn preferred(&self) -> PageSupport { self.preferred }
    pub const fn fallback(&self) -> PageSupport { self.fallback }

    /// 校验静态页大小配置；生产路径由 `detect_boot_page_support` 调用，
    /// 纯规划函数也会再调用一次，防止测试注入绕开静态配置约束。
    pub fn validate(&self) -> Result<(), PageSupportError> {
        let base = PageSize::try_from_usize(A::Paging::PAGE_SIZE)
            .ok_or(PageSupportError::InvalidBasePageSize)?;
        if self.base_page_size != base {
            return Err(PageSupportError::InvalidBasePageSize);
        }
        if self.phys_addr_limit == 0 {
            return Err(PageSupportError::InvalidPhysAddrLimit);
        }
        if self.fallback.size() != A::Paging::FALLBACK_HUGE_PAGE_SIZE
            || !A::Paging::HUGE_PAGE_SIZES.contains(&self.fallback.size())
            || self.fallback.size() <= base
        {
            return Err(PageSupportError::InvalidFallback);
        }
        if self.preferred.size() != A::Paging::HUGE_PAGE_SIZE
            || !A::Paging::HUGE_PAGE_SIZES.contains(&self.preferred.size())
            || self.preferred.size() <= self.fallback.size()
        {
            return Err(PageSupportError::InvalidPreferred);
        }
        Ok(())
    }
}

pub fn detect_boot_page_support<A: BootArch>()
    -> Result<BootPageSupport<A>, PageSupportError>

/// 能力事实层的错误：静态页大小配置与有效 PA 上界不可信。
/// 归属 `minix-arch`（`arch/paging_ext.rs`）——只描述“这次启动观测到的
/// 事实是否自洽”，不携带任何映射规划语义。
pub enum PageSupportError {
    /// `Paging::PAGE_SIZE` 不能表示为合法 `PageSize`，或与快照不一致。
    InvalidBasePageSize,
    /// 无法获得可信的平台有效物理地址上界，或上界为零。
    InvalidPhysAddrLimit,
    /// 回退尺寸不等于架构回退常量、不在页大小集合内，或不大于基础页。
    InvalidFallback,
    /// 首选尺寸不等于架构首选常量、不在页大小集合内，或不大于回退尺寸。
    InvalidPreferred,
}
```

错误类型按 crate 分层，不让 `minix-arch` 导出一个混入 kernel 映射概念的总枚举。能力事实错误是这里的 `PageSupportError`；Direct Map 规划错误 `DmPolicyError`（`InvalidSupport`、`InvalidDmLayout`、`InvalidDmAlignment`、`FallbackUnsupported`）定义在 §6.6，归属 `minix-arch`；内核映射规划错误 `KernelMapError` 与组合汇总错误 `BootPagePolicyError` 定义在 §6.5，归属 `minix-kernel`。下层规划函数调用 `support.validate()` 时通过包装变体 `InvalidSupport(PageSupportError)` 转发，不复制变体；`plan_boot_pages` 的 `?` 依赖 `From` 实现把三层错误收进 `BootPagePolicyError`。这样“每个错误枚举只回答自己一层的问题”与接口分层一致：能力事实、DM 规划、内核映射规划各自的失败原因不会互相假冒。

`detect_boot_page_support` 的职责只有：

1. 用 `PageSize::try_from_usize(A::Paging::PAGE_SIZE)` 转换基础页，失败返回错误；
2. 查询 `A::Paging::HUGE_PAGE_SIZE` 和 `A::Paging::FALLBACK_HUGE_PAGE_SIZE` 的能力；
3. 从 `A::Paging::phys_addr_limit()` 取得当前平台有效物理地址上界；如果架构无法给出可信上界，返回 `InvalidPhysAddrLimit`，启动失败；
4. 把结果填入 `BootPageSupport<A>`，调用 `validate()` 检查静态配置，然后返回。

它不读取内核镜像布局，不检查 `kv`/`kp`/`ks` 对齐，不决定内核映射或 Direct Map 用多大页。`MockPaging` 的能力开关只在这里生效。

这里刻意不再保存 `kernel_preferred`、`kernel_fallback`、`dm_preferred`、`dm_fallback` 四份字段。当前三种架构的内核映射和 Direct Map 共享同一组候选尺寸；同一个页大小的能力答案只能有一份。`validate()` 要求首选和回退尺寸都来自 `A::Paging::HUGE_PAGE_SIZES`，并且基础页小于回退、回退小于首选，因此同一尺寸不会在首选和回退两个槽位里同时出现。如果未来两个消费者真的需要不同的首选尺寸，应该扩展探测输入或事实表，让每个尺寸仍只有一个权威答案，而不是在快照里复制事实、让两份答案有机会漂移。

`BootPageSupport<A>` 的字段私有，包含页大小事实和当前平台有效 PA 上界 `phys_addr_limit`；`from_parts` 只在 `#[cfg(test)]` 或 `test-support` feature 下可见，生产构建里唯一的构造函数是 `detect_boot_page_support`。这样“这次启动观测到的硬件事实”不能由普通生产代码伪造，而跨 crate 测试仍然可以显式注入受控输入。纯规划函数会调用 `validate()`，所以即便测试注入不合法的静态配置，也会在进入策略层之前失败。

`BootPageSupport<A>` 保存的是同构平台契约下的系统级能力事实，不是 BSP 的局部观察；能力检测可以只执行一次，但平台必须保证所有共享页表的 CPU 都满足同一组页大小、PA 上界和翻译配置。它不是永久能力缓存。运行期需要能力时仍然直接调用 `HugePages::supports_huge_page`。

### 6.5 策略：`KernelMapPolicy<A>`、`DmPagePolicy<A>`、`BootPagePolicy<A>`

策略对象保存已经验证过的决策，并携带架构身份：

```rust
use core::marker::PhantomData;

pub struct KernelMapPolicy<A: BootArch> {
    /// 规划时验证过的内核布局；执行入口必须使用这一份。
    layout: KernelLayout,
    page_size: PageSize,
    _arch: PhantomData<fn() -> A>,
}

pub struct DmPagePolicy<A: BootArch> {
    preferred_page_size: Option<PageSize>,
    fallback_page_size: PageSize,
    base_page_size: PageSize,
    /// 规划时从能力快照保存的平台物理地址上界。
    phys_addr_limit: u64,
    /// 规划时验证过的 Direct Map 布局；窗口也从这一份生成。
    layout: DirectMapLayout<A>,
}

pub struct BootPagePolicy<A: BootArch> {
    kernel: KernelMapPolicy<A>,
    dm: DmPagePolicy<A>,
}

impl<A: BootArch> Copy for KernelMapPolicy<A> {}
impl<A: BootArch> Clone for KernelMapPolicy<A> {
    fn clone(&self) -> Self { *self }
}

impl<A: BootArch> Copy for DmPagePolicy<A> {}
impl<A: BootArch> Clone for DmPagePolicy<A> {
    fn clone(&self) -> Self { *self }
}

impl<A: BootArch> Copy for BootPagePolicy<A> {}
impl<A: BootArch> Clone for BootPagePolicy<A> {
    fn clone(&self) -> Self { *self }
}

impl<A: BootArch> KernelMapPolicy<A> {
    pub const fn layout(&self) -> KernelLayout { self.layout }
    pub const fn page_size(&self) -> PageSize { self.page_size }
}

impl<A: BootArch> DmPagePolicy<A> {
    pub const fn layout(&self) -> DirectMapLayout<A> { self.layout }
    pub const fn preferred_page_size(&self) -> Option<PageSize> {
        self.preferred_page_size
    }
    pub const fn fallback_page_size(&self) -> PageSize { self.fallback_page_size }
    pub const fn base_page_size(&self) -> PageSize { self.base_page_size }
    pub const fn phys_addr_limit(&self) -> u64 { self.phys_addr_limit }

    pub fn kernel_window(&self) -> DmWindow<A> {
        DmWindow::kernel(&self.layout)
    }

    pub fn vm_window(&self) -> DmWindow<A> {
        DmWindow::vm(&self.layout)
    }
}

impl<A: BootArch> BootPagePolicy<A> {
    pub const fn kernel(&self) -> KernelMapPolicy<A> { self.kernel }
    pub const fn dm(&self) -> DmPagePolicy<A> { self.dm }
}
```

字段保持私有，通过只读访问器读取；构造只通过规划函数。`PhantomData<fn() -> A>` 不拥有 `A`，也不影响 `Send`/`Sync` 推导，只用于把策略绑定到产生它的架构。策略同时携带规划时验证过的布局：`KernelMapPolicy<A>` 保存 `KernelLayout`，`DmPagePolicy<A>` 保存 `DirectMapLayout<A>` 和从能力快照复制的 `phys_addr_limit`。执行入口从策略取布局/PA 上界，不能再从 `KernelInfo` 或另一份窗口参数重新拼出未验证的地址。

这里不写 `#[derive(Clone, Copy)]`。Rust 的派生实现会给泛型参数补 `A: Clone`/`A: Copy` 约束，即使实际字段只有 `PhantomData<fn() -> A>`；`BootArch` 并不要求这些 bound。手写 `Copy`/`Clone` 让约束只取决于真实字段。若测试需要 `Debug` 或 `PartialEq`，也按同样原则手写，或只比较访问器返回的 `PageSize`。

内核镜像布局是规划输入：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelLayout {
    pub virtual_base: u64,
    pub physical_base: u64,
    pub size: u64,
}
```

`plan_kernel_mapping` 是纯函数，返回内核映射规划层自己的错误：

```rust
/// 内核映射规划层的错误：归属 `minix-kernel`（`boot_policy.rs`）。
pub enum KernelMapError {
    /// 能力快照的静态页大小配置不合法（转发 `PageSupportError`）。
    InvalidSupport(PageSupportError),
    /// 内核镜像布局的基址/长度加和溢出、VA 区间不合法，
    /// 或物理末端越过 `phys_addr_limit`。
    InvalidKernelLayout,
    /// 需要回退尺寸但回退不受当前平台支持。
    FallbackUnsupported(PageSize),
    /// 首选和回退都不满足内核布局。
    NoKernelPageSize,
}

pub fn plan_kernel_mapping<A: BootArch>(
    support: &BootPageSupport<A>,
    layout: KernelLayout,
) -> Result<KernelMapPolicy<A>, KernelMapError>
```

它只做：

1. 调用 `support.validate()`，确认能力快照的静态页大小配置合法；
2. 读取 `support` 中首选/回退的页大小与支持状态；
3. 检查 `layout.size > 0`，虚拟基址、物理基址和长度的加和不溢出，虚拟范围通过 `A::Paging::validate_virtual_range()`，以及物理末端不超过 `support.phys_addr_limit()`；
4. 检查 `layout` 是否满足首选尺寸的对齐和长度条件；
5. 首选不满足时才检查回退尺寸；如果回退也不受支持，返回 `FallbackUnsupported`；
6. 两者都不满足时返回 `NoKernelPageSize`；
7. 返回 `KernelMapPolicy<A>`，其中同时保存已验证的 `layout` 和选中的 `PageSize`；实现返回的是 `KernelMapPolicy { layout, page_size, _arch: PhantomData }`，布局副本随策略一起传递。

它不查询硬件，不写页表，不分配内存。测试可以直接构造 `BootPageSupport<A>` 和 `KernelLayout`：

```rust
let support = BootPageSupport::<X86_64BootArch>::from_parts(
    PageSize::KIB_4,
    1u64 << 52,
    PageSupport::new(PageSize::GIB_1, false),
    PageSupport::new(PageSize::MIB_2, true),
);
let layout = KernelLayout {
    virtual_base: kv,
    physical_base: kp,
    size: ks,
};
let policy = plan_kernel_mapping::<X86_64BootArch>(&support, layout).unwrap();
assert_eq!(policy.page_size(), PageSize::MIB_2);
assert_eq!(policy.layout(), layout);
```

这直接覆盖“1 GiB 不支持但布局完全按 1 GiB 对齐”的场景。选页函数必须先看 `supported`，不能先看对齐。

`plan_boot_pages` 只负责组合两个纯规划。汇总错误 `BootPagePolicyError` 由 `minix-kernel` 定义，`?` 依赖 `From` 把三层错误收进来：

```rust
pub fn plan_boot_pages<A: BootArch>(
    support: &BootPageSupport<A>,
    kernel_layout: KernelLayout,
    dm_layout: &DirectMapLayout<A>,
) -> Result<BootPagePolicy<A>, BootPagePolicyError> {
    // 顶层先显式校验一次：Support 变体由此可达；两个下层规划各自
    // 保留 InvalidSupport 包装，供独立调用时诊断，From 不做展开。
    support.validate().map_err(BootPagePolicyError::Support)?;
    let kernel = plan_kernel_mapping::<A>(support, kernel_layout)?;
    let dm = plan_dm_policy::<A>(support, dm_layout)?;
    Ok(BootPagePolicy { kernel, dm })
}

/// 组合规划错误：`From` 只把下层错误映射为 `Kernel`/`Dm` 变体；
/// 嵌套的 `InvalidSupport` 保留在下层错误里，不展开成顶层变体。
pub enum BootPagePolicyError {
    Support(PageSupportError),
    Kernel(KernelMapError),
    Dm(DmPolicyError),
}
```

它不查询硬件。`BootPagePolicy<A>` 不是通用能力快照，只回答“这次启动的内核映射和 Direct Map 该用什么尺寸”。

`build_bootstrap_root_and_enable` 接收 `root_page` 和 `&KernelMapPolicy<A>`，不再接收裸 `kern_huge`，也不需要从 `KernelInfo` 重新读取内核映像地址。它从策略取出 `layout()` 和 `page_size()`：

```rust
fn build_bootstrap_root_and_enable<A: BootArch>(
    root_page: PhysBytes,
    policy: &KernelMapPolicy<A>,
)
```

这样调用方既不能把任意对齐值伪装成已验证的启动决策，也不能把另一个架构或另一个布局的策略传进来。

当前 `build_bootstrap_root_and_enable`（`os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable`）把内核映像视为一个连续、统一 `kernel_read_write | EXECUTABLE` 权限的范围，按选中尺寸逐块映射；它没有按段拆分权限或保留映射空洞。因此一个 `KernelLayout` 与当前真实映射粒度一致。身份映射 `[0, IDENTITY_MAP_END)` 也复用同一个选中尺寸，`IDENTITY_MAP_END` 是固定 4 GiB 常量，由编译期断言保证能同时被首选和回退尺寸整除。如果未来内核映像出现段级权限、空洞或独立映射单元，规划输入必须改成按真实映射片段建模，不能继续用整体布局证明每个叶子都满足约束。

恒等映射还带来一条启动输入约束。Step 1 以选中尺寸覆盖 `[0, IDENTITY_MAP_END)`，`IDENTITY_MAP_END` 是 kernel 从 `minix-arch` 导入的 4 GiB 常量 `BOOT_IDENTITY_MAP_END`（`os/arch/src/lib.rs:pub const BOOT_IDENTITY_MAP_END`；boot-shim 用同一常量做分配边界）。页大小能整除 4 GiB，不等于平台有效物理地址上界覆盖 4 GiB——若 `phys_addr_limit()` 低于 4 GiB，恒等叶子会写出硬件可寻址范围。因此 `boot_validate_and_prepare` 在拿到能力事实后必须检查 `support.phys_addr_limit() >= IDENTITY_MAP_END`（上界为排他语义时恰好等于 4 GiB 即可覆盖 `[0, 4 GiB)`），不满足即拒绝启动。这条检查放在 kernel 启动规划层：4 GiB 恒等窗口是 kernel bootstrap root 的设计决定，`minix-arch` 只是持有共享常量，能力检测不反向依赖 kernel 的启动布局。它同时闭合恒等叶子的 PA 上界——每个恒等叶子的物理末端不超过 `IDENTITY_MAP_END`，因此不超过 `phys_addr_limit`。bootstrap 树页（页表根与 boot bump 区）已有独立边界：`validate_bootstrap_tree`（`os/kernel/src/dm_coverage.rs:fn validate_bootstrap_tree`）断言它们落在 `min(IDENTITY_MAP_END, VM 窗口容量)` 内，天然低于这条上界，不需要为此再引入新检查。

### 6.6 启动安装器：策略、窗口与地址边界

目标 trait 只保留安装动作，并用 `P` 绑定分页实现：

```rust
pub trait BootDmInstaller<P: HugePages>: crate::private::Sealed {
    /// 安装一个叶子映射。安全前提**全部**集中在 `# Safety` 里，
    /// 调用者不需要从普通注释拼凑前提。安装器必须能安装策略三个
    /// 尺寸中的任何一个，包括基础页叶子——三档选页的最后一档是
    /// 安装器契约的一部分，不是可选能力。
    ///
    /// # Safety（调用者必须证明）
    ///
    /// - `root_paddr` 指向合法、按页对齐且可经恒等映射访问的页表根；
    /// - 分页已开启，bootstrap root 处于活动状态；
    /// - 单线程启动上下文，没有并发页表访问者；
    /// - 所有页表页（含安装中分配的中间页）都在恒等映射可写范围内；
    /// - 不使用正在建立的 DM 窗口作为写通道；
    /// - `size` 来自与该安装器及目标架构**配对**的、已验证的
    ///   Direct Map 策略，或经过等价的完整预检；架构配对由泛型
    ///   启动包装入口、`BootArch` 关联类型组合与注册一致性检查
    ///   保证——裸 `install_leaf` 不因一个 `PageSize` 自动获得该保证；
    /// - `paddr` 及整个叶子落在有效 `phys_addr_limit` 内，`vaddr`
    ///   落在目标窗口内且区间计算不溢出（受支持入口的预检完成，
    ///   直接调用者自行承担）。
    ///
    /// # 实现负责验证、经 `Err` 拒绝（不是调用者的 unsafe 前提）
    ///
    /// - `size` 是本安装器支持的合法叶尺寸（三种现有实现都在入口
    ///   检查并拒绝 1 GiB/2 MiB/4 KiB 之外的值）；
    /// - `vaddr`/`paddr` 按 `size` 对齐；
    /// - 目标槽已有叶子时返回 `AlreadyMapped`。
    unsafe fn install_leaf(
        root_paddr: PhysBytes,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: PageSize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;
}
```

`DirectMapLayout<A>` 把同一架构的 Kernel DM 和 VM DM 窗口集中到一个可测试对象：

```rust
pub struct DirectMapLayout<A: BootArch> {
    kernel_base: u64,
    kernel_pa_capacity: u64,
    kernel_va_end: u64,
    vm_base: u64,
    vm_pa_capacity: u64,
    vm_va_end: u64,
    _arch: PhantomData<fn() -> A>,
}

impl<A: BootArch> Copy for DirectMapLayout<A> {}
impl<A: BootArch> Clone for DirectMapLayout<A> {
    fn clone(&self) -> Self { *self }
}

impl<A: BootArch> DirectMapLayout<A> {
    pub fn from_arch() -> Self {
        let kernel_base = <A::DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_BASE;
        let kernel_pa_capacity = <A::DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_SIZE;
        let kernel_va_end = kernel_base
            .checked_add(kernel_pa_capacity)
            .expect("DirectMapArch: KERNEL_DIRECT_MAP_BASE + KERNEL_DIRECT_MAP_SIZE must fit in u64");
        Self {
            kernel_base,
            kernel_pa_capacity,
            kernel_va_end,
            vm_base: <A::DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE,
            vm_pa_capacity: <A::DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE,
            vm_va_end: <A::DirectMap as DirectMapArch>::VM_HEAP_BASE,
            _arch: PhantomData,
        }
    }

    /// 检查布局与 `DirectMapArch` 的换算常量完全一致，并满足窗口
    /// 几何不变量。运行期地址换算（*_unchecked）直接用同一组常量
    /// 做 `VA = PA + BASE` 加法，因此任何偏离架构常量的布局——
    /// 即使几何自洽——都会让策略安装的叶子和运行期换算出的地址
    /// 分家。返回布局层自己的错误（与本类型同处
    /// `arch/direct_map.rs`），不让布局层依赖 DM 规划层的
    /// `DmPolicyError`；`plan_dm_policy` 负责把它转换成策略错误。
    /// 测试注入布局和未来 `BootArch` 实现也必须经过这里才能进入
    /// `DmPagePolicy`。
    pub fn validate(&self) -> Result<(), DmLayoutError> {
        let base = PageSize::try_from_usize(A::Paging::PAGE_SIZE)
            .ok_or(DmLayoutError::InvalidLayout)?;
        // 零容量显式拒绝：零是基础页的整数倍、`base + 0 == end`
        // 也能自洽，后续的取模、加法与重叠检查都拦不住它。
        if self.kernel_pa_capacity == 0 || self.vm_pa_capacity == 0 {
            return Err(DmLayoutError::InvalidLayout);
        }
        if self.kernel_base
            != <A::DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_BASE
            || self.kernel_pa_capacity
                != <A::DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_SIZE
            || self.kernel_base.checked_add(self.kernel_pa_capacity)
                != Some(self.kernel_va_end)
            || self.kernel_va_end
                > <A::DirectMap as DirectMapArch>::KERNEL_DM_VA_END_LIMIT
            || self.kernel_pa_capacity % base.bytes() != 0
            || A::Paging::validate_virtual_range(
                self.kernel_base,
                self.kernel_pa_capacity,
            ).is_err()
        {
            return Err(DmLayoutError::InvalidLayout);
        }
        if self.vm_base != <A::DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE
            || self.vm_pa_capacity
                != <A::DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE
            || self.vm_va_end != <A::DirectMap as DirectMapArch>::VM_HEAP_BASE
            || self.vm_base.checked_add(self.vm_pa_capacity)
                != Some(self.vm_va_end)
            || self.vm_pa_capacity % base.bytes() != 0
            || A::Paging::validate_virtual_range(
                self.vm_base,
                self.vm_pa_capacity,
            ).is_err()
        {
            return Err(DmLayoutError::InvalidLayout);
        }
        if self.kernel_va_end > self.vm_base && self.vm_va_end > self.kernel_base {
            return Err(DmLayoutError::InvalidLayout);
        }
        Ok(())
    }

    pub const fn kernel_base(&self) -> u64 { self.kernel_base }
    pub const fn kernel_pa_capacity(&self) -> u64 { self.kernel_pa_capacity }
    pub const fn kernel_va_end(&self) -> u64 { self.kernel_va_end }
    pub const fn vm_base(&self) -> u64 { self.vm_base }
    pub const fn vm_pa_capacity(&self) -> u64 { self.vm_pa_capacity }
    pub const fn vm_va_end(&self) -> u64 { self.vm_va_end }
}
```

`DirectMapLayout<A>` 的字段保持私有，生产路径只通过 `from_arch` 构造。布局验证的失败用布局层自己的错误表达，定义与类型同处：

```rust
/// 布局几何验证失败；布局层唯一的错误形态，由
/// `DirectMapLayout::validate()` 返回，`plan_dm_policy` 转换为
/// `DmPolicyError::InvalidDmLayout`。不让布局层反向依赖规划层。
pub enum DmLayoutError {
    InvalidLayout,
}
```

test-support 下的 `from_parts` 仍然存在，但用途收窄为**测试验证边界本身**：用它可以构造出偏离架构常量的值，然后断言 `validate()` 拒绝它们；它构造不出一个“偏离架构但仍通过验证”的布局。需要测试“首选页大小与基址不对齐导致降级”这类场景时，使用一个布局常量本身符合该场景的 mock 架构（它的 `DirectMapArch` 常量相对它自己的首选页大小不对齐），而不是用 `from_parts` 把真实架构的窗口平移后再指望验证器放行。如果跨 crate 测试无法使用 `#[cfg(test)]`，用 test-support feature 暴露这个构造器。

`DirectMapArch` 增加一个显式常量：

```rust
pub trait DirectMapArch {
    const KERNEL_DIRECT_MAP_BASE: u64;
    /// Kernel DM 能表达的物理偏移上界，也是该窗口的 VA 容量。
    /// 必须满足 `KERNEL_DIRECT_MAP_BASE + KERNEL_DIRECT_MAP_SIZE` 不溢出，
    /// 且终点不超过项目配置的 `KERNEL_DM_VA_END_LIMIT`；后者本身必须落在
    /// 当前 MMU 模式的合法虚拟地址范围内。
    const KERNEL_DIRECT_MAP_SIZE: u64;
    /// 项目为 Kernel DM 选择的虚拟地址排他终点上限；最高页有意保留。
    /// 它不是硬件整个 canonical 地址空间的上界，必须落在当前 MMU 模式
    /// 的合法范围内。
    const KERNEL_DM_VA_END_LIMIT: u64;

    // 现有 VM 开关常量保持不变。
    // ...
}
```

三个架构的取值如下。它们都把 canonical high-half 的最后一页保留下来，使 `kernel_base + kernel_size` 仍能用 `u64` 表示；这不是物理内存上限，而是 Kernel DM 虚拟窗口的容量上限。`KERNEL_DM_VA_END_LIMIT` 是项目配置的 Kernel DM 窗口上界，不是硬件整个 canonical 地址空间的上界；它必须落在当前实际启用的 MMU 模式合法范围内，不能只看 `u64` 能否表示：

| 架构 | 当前分页模式 | `KERNEL_DIRECT_MAP_BASE` | `KERNEL_DIRECT_MAP_SIZE` | `kernel_va_end` | `KERNEL_DM_VA_END_LIMIT` |
|---|---|---|---:|---:|---:|
| x86-64 | 4 级 PML4 | `0xFFFF_8080_0000_0000` | `0x0000_7F7F_FFFF_F000` | `0xFFFF_FFFF_FFFF_F000` | `0xFFFF_FFFF_FFFF_F000` |
| ARM64 | 48 位 VA，TTBR1 | `0xFFFF_8080_0000_0000` | `0x0000_7F7F_FFFF_F000` | `0xFFFF_FFFF_FFFF_F000` | `0xFFFF_FFFF_FFFF_F000` |
| RISC-V Sv39 | 3 级 Sv39 | `0xFFFF_FFC0_4000_0000` | `0x0000_003F_BFFF_F000` | `0xFFFF_FFFF_FFFF_F000` | `0xFFFF_FFFF_FFFF_F000` |

RISC-V 的容量必须与现行行为对账，这里有一处数字讹误需要澄清：Sv39 高半区共 256 GiB，Kernel DM 基址在高半区基址之上 1 GiB（VPN[2] = 257），基址到 VA 顶端的距离是 **255 GiB**——上表的 `0x0000_003F_BFFF_F000`（255 GiB − 4 KiB，最高页保留）就是按这个距离取的。现行代码给 Kernel DM 候选传 `u64::MAX` 窗口（`os/kernel/src/dm_coverage.rs:fn establish_boot_dm`），没有显式容量，有效边界只是 VA 可表示性：PA 超过约 255 GiB 时 `base + PA` 回绕出内核半区，是静默错地址。因此上表常量与现行行为一致（255 GiB 以内的映射相同），变化只在超界行为从「静默回绕」变为「响亮失败」。「16 GiB」只属于 VM DM 窗口（riscv64 的 `VM_DIRECT_MAP_SIZE`）；`os/kernel/src/dm_coverage.rs` 模块注释「the riscv64 kernel DM window bounds PA at 16 GiB」把 255 GiB 误写成 16 GiB，属于注释讹误，迁移时订正注释，不按它改容量。

VM DM 窗口来自同一组 `DirectMapArch` 常量：

| 架构 | VM DM 半开区间 | 与 Kernel DM 是否重叠 |
|---|---|---:|
| x86-64 | `[0x0000_0000_8000_0000, 0x0000_0000_C000_0000)` | 否 |
| ARM64 | `[0x0000_1000_0000_0000, 0x0000_1000_8000_0000)` | 否 |
| RISC-V Sv39 | `[0x0000_0010_0000_0000, 0x0000_0014_0000_0000)` | 否 |

`DirectMapLayout::validate()` 仍要在运行期检查两个半开区间互不重叠。布局必须逐项等于架构常量之后，这个检查针对的不再是三架构的已知常量（它们的互斥由编译期断言固定），而是未来 `BootArch` 实现：一个新架构自己的 `DirectMapArch` 常量如果让两个窗口重叠，构建不会失败——编译期断言是每个架构自愿写的——必须在运行期验证里被拒绝。

`VM_HEAP_BASE == VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE` 已经由各架构的编译期断言固定，因此 `vm_va_end` 可以作为 VM 窗口的排他上界。Kernel DM 不再使用 `u64::MAX`：它的排他上界是 `kernel_va_end`，物理上界是 `kernel_pa_capacity`。候选物理区间超过这个容量时，安装会返回错误，而不是把“没有上界”当成合法状态。

三个架构都必须在模块级增加基址、容量和溢出断言：

```rust
const _: () = {
    let base = <X86_64DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_BASE;
    let size = <X86_64DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_SIZE;
    let preferred = <X86_64Paging as HugePages>::HUGE_PAGE_SIZE;
    let limit = <X86_64DirectMap as DirectMapArch>::KERNEL_DM_VA_END_LIMIT;
    assert!(preferred.bytes() > 0);
    assert!(base % preferred.bytes() == 0);
    assert!(size > 0);
    assert!(size % PageSize::KIB_4.bytes() == 0);
    assert!(match base.checked_add(size) {
        Some(end) => end <= limit,
        None => false,
    });
};

const _: () = {
    let preferred = <X86_64Paging as HugePages>::HUGE_PAGE_SIZE;
    assert!(
        <X86_64DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE
            % preferred.bytes() == 0
    );
    assert!(
        <X86_64DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE
            % preferred.bytes() == 0
    );
};
```

ARM64 和 RISC-V 使用同样的断言，但 `KERNEL_DM_VA_END_LIMIT` 是项目为 Kernel DM 选择的上界，要按各自实际启用的分页模式落在合法范围内；如果未来切换到 5 级页表或 Sv48，必须同时重算 base、size 和 limit。这样基址对齐、Kernel DM 容量和 `base + size` 的可表示性都从“实现者应该知道”变成编译期事实；如果窗口改成不满足架构首选页大小，或终点越过项目上界/当前 MMU 模式的合法范围，构建会失败。`DirectMapLayout::from_arch` 仍用 `checked_add` 防守非已知 `BootArch` 实现，失败时在启动早期 panic 并给出常量名，而不是在调试构建里发生整数溢出。

这里有两层不同的约束，不能互相替代：
- 实际架构的静态布局必须按该架构的首选页大小对齐，并且不越过 `KERNEL_DM_VA_END_LIMIT`；这是编译期不变量，真实架构不满足就直接构建失败。
- `DirectMapLayout::validate()` 在运行期要求布局与架构换算常量逐项相等。因此“首选对齐不满足导致降级”只可能发生在一个架构自身的常量与其首选页大小不对齐的时候——三个受支持架构被编译期断言排除了这种形状，能走到这条降级分支的是 mock 架构或未来没写断言的新架构。

因此，“基址必须满足首选对齐”属于静态配置层（编译期断言），“规划器支持首选降级”属于纯规划层；降级路径的测试用布局常量本身不对齐的 mock 架构驱动，而不是用测试注入的偏移布局。

`DirectMapLayout::validate()` 是布局进入策略前的统一边界，它检查的不变量按序是：布局与 `DirectMapArch` 的换算常量**逐项相等**（`kernel_base == KERNEL_DIRECT_MAP_BASE`、`kernel_pa_capacity == KERNEL_DIRECT_MAP_SIZE`、`vm_base == VM_DIRECT_MAP_BASE`、`vm_pa_capacity == VM_DIRECT_MAP_SIZE`、`vm_va_end == VM_HEAP_BASE`）——运行期地址换算（`kernel/vm_phys_to_virt_unchecked`）用同一组常量做 `VA = PA + BASE` 加法，任何“不低于架构基址即可”的宽松验证都会让策略按布局安装叶子、运行期按常量算地址，两者分家；`base + capacity` 与终点在 checked 加法下一致；终点不越过 `KERNEL_DM_VA_END_LIMIT`；容量按基础页对齐；两个 VA 区间通过 `A::Paging::validate_virtual_range()`；两个半开区间互不重叠。窗口容量的基础页对齐检查也在这里——`validate()` 是布局几何的完整边界，`plan_dm_policy` 不再重复这道检查，只负责能力事实与基址对所选页大小的对齐。`plan_dm_policy` 在返回 `DmPagePolicy` 前调用它，因此测试注入的布局和未来新增的 `BootArch` 实现都不能绕过几何验证；新架构注册时布局来自它自己的 `from_arch`，天然与其换算常量一致，不存在“验证通过但与换算分家”的注册路径。

每个架构还要在自己的配置边界用私有常量（例如 `KERNEL_VA_TOP`）做编译期断言：

| 架构 | high-half 基址 | Kernel VA 上界（架构私有） |
|---|---:|---:|
| x86-64 | `0xFFFF_8000_0000_0000` | `0xFFFF_FFFF_FFFF_F000` |
| ARM64 | `0xFFFF_0000_0000_0000` | `0xFFFF_FFFF_FFFF_F000` |
| RISC-V Sv39 | `0xFFFF_FFC0_0000_0000` | `0xFFFF_FFFF_FFFF_F000` |

断言内容至少包括：`KERNEL_DIRECT_MAP_BASE >= high_half_base`、`kernel_base + size <= KERNEL_DM_VA_END_LIMIT`、`KERNEL_DM_VA_END_LIMIT <= KERNEL_VA_TOP`。通用代码不复制 canonical 位运算，只调用架构实现的 `validate_virtual_range()`；未来改变页表级数、TCR 配置或 Sv39/Sv48 时，架构断言必须失败并要求重算。

`plan_dm_policy` 消费能力事实和布局，不查询硬件：

```rust
/// Direct Map 规划层的错误：归属 `minix-arch`（`arch/dm_coverage.rs`）。
pub enum DmPolicyError {
    /// 能力快照的静态页大小配置不合法（转发 `PageSupportError`）。
    InvalidSupport(PageSupportError),
    /// 布局与架构换算常量不一致，或窗口几何关系自相矛盾。
    InvalidDmLayout,
    /// 回退尺寸与 Kernel DM / VM DM 基址不对齐。
    InvalidDmAlignment,
    /// 回退尺寸不受当前平台支持（当前支持配置的硬性前提，见下文）。
    FallbackUnsupported(PageSize),
}

pub fn plan_dm_policy<A: BootArch>(
    support: &BootPageSupport<A>,
    layout: &DirectMapLayout<A>,
) -> Result<DmPagePolicy<A>, DmPolicyError> {
    support.validate()?; // PageSupportError 经 From 转为 InvalidSupport
    layout.validate().map_err(|DmLayoutError::InvalidLayout| {
        DmPolicyError::InvalidDmLayout
    })?;  // 布局几何的完整边界：与换算常量逐项相等、
                         // 容量按基础页对齐、终点自洽、区间合法且互斥

    let preferred = support.preferred();
    let fallback = support.fallback();
    let base = support.base_page_size();

    // 回退尺寸受支持是当前支持配置的硬性前提。这不是“技术上无法
    // 继续建表”的必然结论：三种受支持架构的安装器都能安装基础页
    // 叶子，仅用基础页覆盖候选区间在技术上可行。把它写成硬前提是
    // 项目支持策略——不为缺少 2 MiB 大页的平台维护第二条建表路径。
    // 它与基础页的大小关系由 support.validate() 统一检查。
    if !fallback.supported() {
        return Err(DmPolicyError::FallbackUnsupported(fallback.size()));
    }
    if layout.kernel_base() % fallback.size().bytes() != 0
        || layout.vm_base() % fallback.size().bytes() != 0
    {
        return Err(DmPolicyError::InvalidDmAlignment);
    }

    // 首选尺寸与回退尺寸的关系已经由 support.validate() 检查；
    // 这里只处理能力事实和布局对齐，不再重复检查整除关系。
    let preferred_size = if !preferred.supported()
        || layout.kernel_base() % preferred.size().bytes() != 0
        || layout.vm_base() % preferred.size().bytes() != 0
    {
        None
    } else {
        Some(preferred.size())
    };

    Ok(DmPagePolicy {
        preferred_page_size: preferred_size,
        fallback_page_size: fallback.size(),
        base_page_size: base,
        phys_addr_limit: support.phys_addr_limit(),
        layout: *layout,
    })
}
```

安装器能力有源码取证：x86-64 的 `dm_install_leaf` 在入口接受 1 GiB/2 MiB/4 KiB 三种尺寸并拒绝其他值（`os/arch/src/x86_64/paging.rs:fn dm_install_leaf`），ARM64（`os/arch/src/arm64/paging.rs:fn dm_install_leaf`）与 RISC-V（`os/arch/src/riscv64/paging.rs:fn dm_install_leaf`）同样接受这三档；现有 `DmCoverageArch` 的 trait 契约本来就把 `size ∈ {1 GiB, 2 MiB, 4 KiB}` 写成安装器义务。因此“没有 2 MiB 就拒绝启动”是支持边界的选择，不是被安装器能力逼出来的结论；`BootDmInstaller` 的目标契约延续这条义务——必须能装基础页叶子，三档选页的最后一档才是真实可达的兜底。

这里把三类结果分开：

| 条件 | 语义 | 结果 |
|---|---|---|
| 首选尺寸不受硬件支持 | 正常降级 | `preferred_page_size = None` |
| 首选尺寸受支持，但 Kernel DM 或 VM DM 基址不满足首选对齐 | 正常降级 | `preferred_page_size = None` |
| 首选尺寸不大于回退尺寸 | 静态配置错误 | `InvalidPreferred` |
| 回退尺寸不受支持 | 平台/配置错误 | `FallbackUnsupported` |
| 回退尺寸不大于基础页，或不在架构页大小集合内 | 静态配置错误 | `InvalidFallback` |
| 回退尺寸与窗口基址不对齐 | 布局错误 | `InvalidDmAlignment` |
| 窗口与架构换算常量不一致、容量为零、容量不按基础页对齐、`base + cap` 与终点不一致、终点越界，或 Kernel/VM 窗口重叠 | 布局几何错误 | `InvalidDmLayout` |

`PageSize` 已经保证基础页不小于 4 KiB 且是 2 的幂。两个合法 `PageSize` 只要前者大于后者，前者就一定能被后者整除，因此这里不再检查“首选是回退的整数倍”或“回退是基础页的整数倍”，也不为这些不可能失败的整除条件写测试。需要测试非法尺寸时，应测试 `PageSize` 的构造器拒绝非 2 的幂，而不是用合法 `PageSize` 制造一个不存在的失败。

静态配置由 `BootPageSupport::validate()` 统一检查，并且先于能力/布局降级：如果一个首选尺寸存在“尺寸顺序错误”或“不在架构页大小集合内”的配置错误，函数返回配置错误，而不是让硬件不支持掩盖它。正常降级只适用于静态配置合法的首选尺寸。

`plan_range_units` 不再知道 1 GiB：

```rust
pub fn plan_range_units<A: BootArch, F: FnMut(u64, PageSize) -> Result<(), PageTableError>>(
    range: DmRange,
    policy: &DmPagePolicy<A>,
    mut emit: F,
) -> Result<(), PageTableError> {
    // preferred: policy.preferred_page_size()
    // fallback:  policy.fallback_page_size()
    // base:      policy.base_page_size()
    let base = policy.base_page_size().bytes();
    if range.base % base != 0
        || range.len % base != 0
    {
        return Err(PageTableError::InvalidAddress);
    }
    let end = range.base.checked_add(range.len)
        .ok_or(PageTableError::InvalidAddress)?;
    if range.len == 0 {
        return Ok(());
    }
    // 从 range.base 逐段推进到 end；每个单位必须完整落在区间内，
    // 不能吞掉空洞。
}
```

`plan_range_units` 是公开的纯规划函数，不依赖调用者先替它验证输入。它自己检查区间起点和长度的基础页对齐、终点溢出；基础页大小由 `PageSize` 的非零不变量保证；`len == 0` 是合法的空操作，不调用回调，但空区间的起点仍必须满足基础页对齐。这样直接测试 `plan_range_units` 时，非法区间不会被静默发射成基础页；安装入口的预检只是重复这道边界，不承担唯一责任。

选页顺序是：

1. 首选大页存在，且当前 `pa` 按它对齐、剩余长度足够；
2. 否则回退大页，且当前 `pa` 按它对齐、剩余长度足够；
3. 否则基础页；
4. 每个发射单位必须满足 `pa` 对齐、`pa + size` 不越过区间末端。

`install_boot_dm_range` 接收窗口和已验证的策略，统一检查虚拟地址。它不再静默裁剪区间：窗口裁剪属于 `establish_boot_dm` 的资源资格过滤，安装器只接受已经落在窗口内的区间；Kernel DM 超出容量时直接失败。

同一套几何检查提取为：

```rust
fn preflight_boot_dm_range<A: BootArch>(
    range: DmRange,
    window: DmWindow<A>,
    policy: &DmPagePolicy<A>,
) -> Result<(), PageTableError>

unsafe fn install_prevalidated_boot_dm_range<A: BootArch>(
    root: PhysBytes,
    range: DmRange,
    window: DmWindow<A>,
    policy: DmPagePolicy<A>,
) -> Result<(), PageTableError>
```

它检查窗口是否属于策略、范围端点和长度的基础页对齐、`range + len` 溢出、窗口容量、策略的 `phys_addr_limit`，以及 `plan_range_units` 将发射的每个叶子的 VA/PA 对齐和边界；它不写页表。私有的 `install_prevalidated_boot_dm_range` 只负责第二遍遍历、调用 `install_leaf` 和传播错误，不再重复地址预检；它的安全前提继承自公开入口。

```rust
/// # Safety
///
/// 调用者必须保证 `root` 指向合法、按页对齐且可经恒等映射访问的页表根，
/// 当前处于单线程启动上下文，bootstrap root 已启用且是活动页表根，
/// 所有页表页都在恒等映射可写范围内，并且没有并发页表访问者。
/// `window` 必须由同一份 `policy` 的 `kernel_window()` 或
/// `vm_window()` 生成，`range` 必须已经按窗口裁剪。
///
/// # Failure
///
/// 地址几何错误会在任何页表修改之前返回。底层 `install_leaf` 在写表
/// 过程中仍可能因为 `AlreadyMapped` 等错误失败；本函数不回滚已经写入
/// 的叶子。调用者必须把任何安装错误当作启动致命错误，停止启动，不能
/// 继续使用部分建立的 Direct Map。
pub unsafe fn install_boot_dm_range<A: BootArch>(
    root: PhysBytes,
    range: DmRange,
    window: DmWindow<A>,
    policy: DmPagePolicy<A>,
) -> Result<(), PageTableError> {
    let base = policy.base_page_size().bytes();
    let kernel_window = policy.kernel_window();
    let vm_window = policy.vm_window();
    let window_matches_policy = (window.va_base() == kernel_window.va_base()
        && window.pa_capacity() == kernel_window.pa_capacity()
        && window.va_end() == kernel_window.va_end()
        && window.flags() == kernel_window.flags())
        || (window.va_base() == vm_window.va_base()
            && window.pa_capacity() == vm_window.pa_capacity()
            && window.va_end() == vm_window.va_end()
            && window.flags() == vm_window.flags());
    if !window_matches_policy {
        return Err(PageTableError::InvalidAddress);
    }
    if window.va_base().checked_add(window.pa_capacity()) != Some(window.va_end()) {
        return Err(PageTableError::InvalidAddress);
    }
    if range.base % base != 0 || range.len % base != 0 {
        return Err(PageTableError::InvalidAddress);
    }
    let range_end = range
        .base
        .checked_add(range.len)
        .ok_or(PageTableError::InvalidAddress)?;
    if range_end > window.pa_capacity() {
        return Err(PageTableError::InvalidAddress);
    }
    if range_end > policy.phys_addr_limit() {
        return Err(PageTableError::InvalidAddress);
    }

    // 第一遍只验证每个将要发射的叶子，不写页表。这样范围尾部不对齐、
    // VA/PA 溢出或越出窗口都会在第一个叶子写入之前暴露。
    plan_range_units::<A>(range, &policy, |pa, size| {
        let va = window.va_base()
            .checked_add(pa)
            .ok_or(PageTableError::InvalidAddress)?;
        let pa_end = pa
            .checked_add(size.bytes())
            .ok_or(PageTableError::InvalidAddress)?;
        let va_end = va
            .checked_add(size.bytes())
            .ok_or(PageTableError::InvalidAddress)?;

        if pa % size.bytes() != 0
            || va % size.bytes() != 0
            || pa_end > range_end
            || pa_end > window.pa_capacity()
            || va_end > window.va_end()
        {
            return Err(PageTableError::InvalidAddress);
        }
        Ok(())
    })?;

    // 第二遍才写表。若底层安装器在此阶段失败，前面的叶子可能已经写入；
    // 调用者按上面的失败契约终止启动，不能把部分映射当成可用状态。
    plan_range_units::<A>(range, &policy, |pa, size| {
        let va = window.va_base()
            .checked_add(pa)
            .ok_or(PageTableError::InvalidAddress)?;
        // SAFETY: 调用者已满足本函数的启动上下文契约；地址边界已在第一遍
        // 验证，尺寸来自已验证的 DmPagePolicy<A>。
        unsafe {
            <A::DmInstaller as BootDmInstaller<A::Paging>>::install_leaf(
                root,
                VirBytes(va),
                PhysBytes(pa),
                size,
                window.flags(),
            )
        }
    })
}
```

上面的代码把预检和实际安装展开，便于核对检查项；实现时第二遍应提取为 `install_prevalidated_boot_dm_range`，由 `establish_boot_dm` 在全局预检之后调用。

这条失败契约不是事务回滚。启动期没有通用页表回收器，也没有必要为一个必然终止启动的错误设计回滚；重要的是把“可以提前发现的几何错误”挡在写表之前，并把“写表过程中才发现的底层错误”明确标成不可继续的致命错误。

`install_boot_dm_range` 的预检只覆盖它自己收到的那个区间。`establish_boot_dm` 还要把 Kernel DM 和 VM DM 的所有候选区间在写第一张叶子之前统一调用同一个 `preflight_boot_dm_range`；这个函数就是安装器前半段的只读提取，不调用 `install_leaf`。这样第二个窗口的地址错误不会等到第一个窗口已经写完之后才发现。

为了避免全局预检之后每个区间再重复一次预检，公开入口和内部安装分成两层：`install_boot_dm_range` 保留自己的预检；实现时把第二遍遍历提取为私有的 `install_prevalidated_boot_dm_range`。`establish_boot_dm` 在所有候选都通过 `preflight_boot_dm_range` 后，直接调用私有安装函数。全局预检只遍历一遍，安装阶段不再重复规划。若直接调用公开入口，它仍然对自己的区间负责。

`establish_boot_dm` 调用安装函数后收到任何错误，都必须让启动失败，不能继续进入 `kmain` 或把部分 Direct Map 交给后续阶段。启动路径中不允许有恢复分支捕获这个错误后继续运行。

全局预检与实际安装必须遍历同一组候选区间，这是实现约束，不是“跑两遍循环”自然带来的性质。候选枚举必须是对不可变输入的确定性纯函数：预检一遍、安装一遍，两遍调用同一个候选生成器，喂同样的 `(kernel_info, root)` 和同样的窗口裁剪；不允许消费型迭代器把第一遍吃空，不允许枚举依赖 boot bump 分配游标（中间页表页恰恰是安装阶段从 bump 分配的），不允许两遍之间有任何可变全局参与筛选或排序。当前候选结构满足这个前提：四个来源生成器（memmap、bootstrap tree、boot module、reserved region，`os/kernel/src/dm_coverage.rs`）只读 `kernel_info` 和注册过的 bump 区间边界——`boot_alloc::boot_alloc_region()` 返回注册时的 `(base, end)`，不随分配游标前进而变化（注册边界与分配游标在 `BootAlloc` 里是分离字段：`os/kernel/src/boot_alloc.rs:fn region` 与 `fn used_bytes` 各读各的）——对同样输入恒定产出同一序列。实现不得为了“保证一致”把预检结果收集进堆数组再回放（启动期没有堆），也不得引入任何让第二遍枚举与第一遍不同的捷径；正确做法就是让两遍走同一个纯函数，并用 mock 安装器断言两遍发射序列逐项相同。

双遍一致只约束“预检与安装重放同一序列”，不约束序列本身的正确性；候选集合还有一层归一化不变量：**同一窗口内的候选物理区间必须两两不重叠**。重叠区间里的第二段会在安装时撞上第一段已写入的叶子（`AlreadyMapped`），而那已经发生在部分叶子写入之后。当前实现的归一化策略是来源优先级加整段丢弃：memmap 描述符、reserved 描述符、模块分配各自天然不相交（同一 `GetMemoryMap` 快照、互不相同的 `AllocatePages` 结果），跨来源重叠由过滤器把低优先级候选整段丢弃（`os/kernel/src/dm_coverage.rs:fn boot_module_candidates`、`fn reserved_regions_candidates`）。因此“两两不重叠”由构造成立，代价是重叠候选的非重叠尾部也被放弃——reserved 描述符横跨 source-2/3 候选时的残留缺口就是这个策略的已知代价。集合级测试覆盖：跨来源重叠时高优先级候选完整保留、低优先级整段丢弃且不产生 `AlreadyMapped`；资源空洞不被任何候选吞掉；Kernel DM 与 VM DM 映射同一物理区间不是错误（不同窗口，叶子不冲突）。预检不做集合级运行期检查：候选序列保持来源顺序、不保证按基址有序，流式相邻检查会误报；逐对重叠检查等于把生成器逻辑再写一遍。集合正确性由生成器的合成测试保证，安装期冲突兜底并按失败契约终止启动。

这个已知尾部损失必须与本次迁移的行为边界分开陈述，两件事不能互相冒充。本次迁移以行为兼容为约束：不改变候选范围并集算法，支持 1 GiB 的既有平台页表内容保持不变，整段丢弃策略原样保留。整段丢弃是否满足覆盖语义——被丢弃的非重叠尾部是否本来就不该映射——是独立的正确性问题：reserved 描述符的尾部仍可能是内核或启动需要经 DM 访问的有效页（DM 访问落在候选间隙即触发 #PF 的故障形态在 riscv64 真链上出现过），当前没有源码或测试证据证明被丢弃范围无害。它作为独立立项处理（§10 第 11 项）：修复方向是按优先级做区间差集归一化——保留高优先级区间，恢复低优先级区间中不与其重叠的部分——需要配套真机证明。在本项闭合前，集合级不变量只承诺较弱的两条：资源空洞不被吞进映射、高优先级候选完整保留；**不承诺“所有有资格映射的字节都被保留”**，测试矩阵也不把这条写成现行断言。

`DmWindow<A>` 的字段和构造。字段私有，生产路径通过 `DmPagePolicy<A>::kernel_window()`/`vm_window()` 生成；`kernel`/`vm` 构造器只供策略实现和受控测试使用：

```rust
pub struct DmWindow<A: BootArch> {
    va_base: u64,
    pa_capacity: u64,
    va_end: u64,
    flags: PageFlags,
    _arch: PhantomData<fn() -> A>,
}

impl<A: BootArch> Copy for DmWindow<A> {}
impl<A: BootArch> Clone for DmWindow<A> {
    fn clone(&self) -> Self { *self }
}

impl<A: BootArch> DmWindow<A> {
    fn kernel(layout: &DirectMapLayout<A>) -> Self {
        Self {
            va_base: layout.kernel_base(),
            pa_capacity: layout.kernel_pa_capacity(),
            va_end: layout.kernel_va_end(),
            flags: PageFlags::kernel_read_write(),
            _arch: PhantomData,
        }
    }

    fn vm(layout: &DirectMapLayout<A>) -> Self {
        Self {
            va_base: layout.vm_base(),
            pa_capacity: layout.vm_pa_capacity(),
            va_end: layout.vm_va_end(),
            flags: PageFlags::read_write(),
            _arch: PhantomData,
        }
    }

    pub const fn va_base(&self) -> u64 { self.va_base }
    pub const fn pa_capacity(&self) -> u64 { self.pa_capacity }
    pub const fn va_end(&self) -> u64 { self.va_end }
    pub const fn flags(&self) -> PageFlags { self.flags }
}
```

- Kernel DM 窗口：`va_base = layout.kernel_base()`，`pa_capacity = layout.kernel_pa_capacity()`，`va_end = layout.kernel_va_end()`；三者来自同一份 `DirectMapArch` 配置，不再用 `u64::MAX` 表示“物理地址不截断”。
- VM DM 窗口：`va_base = layout.vm_base()`，`pa_capacity = layout.vm_pa_capacity()`，`va_end = layout.vm_va_end()`。
- `establish_boot_dm` 仍接收 `kernel_info` 和 `root` 用于候选区间与写表，但窗口只从 `policy.kernel_window()`/`policy.vm_window()` 取；策略保存的布局就是规划时验证过的布局，执行路径不再独立接收另一份 `DirectMapLayout<A>`。`install_boot_dm_range` 还会拒绝不属于该策略的窗口，防止调用方把另一个布局生成的窗口传进来。
- Kernel DM 的物理区间由 `establish_boot_dm` 选择候选，`install_boot_dm_range` 在入口按 `window.pa_capacity()` 检查；超过容量的候选区间返回错误，不做 VM 式静默裁剪。VM DM 仍按资源资格过滤裁剪到 `vm_pa_capacity`，裁剪后的区间才进入安装器。
- `KERNEL_DIRECT_MAP_SIZE`/`kernel_pa_capacity` 是 **Kernel DM 窗口能覆盖的物理跨度**，不是系统可映射 PA 上界；`phys_addr_limit` 才是后者。字段名刻意用 `capacity` 不用 `limit`，就是为了不让这两个上界共用一个词。一个候选区间必须同时落在窗口容量和 `phys_addr_limit` 内；前者不足时即使硬件能寻址也必须拒绝，后者不足时即使窗口很大也不能映射。

这样：

- 选页策略集中在 `plan_dm_policy` 和 `plan_range_units`；
- 安装器只负责表遍历、地址边界检查和叶子写入；
- 测试可以构造 `BootPageSupport<A>` 事实和 `DirectMapLayout<A>`，不需要模拟 CPUID；
- 启动路径只构造一次策略，Kernel DM 和 VM DM 共用；
- 首选大页不再由 `one_gib` 布尔值表达，而是由 `Option<PageSize>` 表达；
- 虚拟地址对齐、Kernel DM 容量、窗口范围和溢出都在安装器边界上验证。

`install_leaf` 保留 `unsafe` 契约，因为它是底层页表写操作。`install_boot_dm_range` 也标记为 `unsafe`：它的地址检查不能证明“当前处于启动上下文”，这个前提必须由调用者承担。`establish_boot_dm` 同样标记为 `unsafe`，只在两个启动入口的 boot 上下文中调用：

```rust
/// # Safety
///
/// 调用者必须证明处于启动上下文，满足与 `install_leaf` 相同的
/// 全组前提：bootstrap root 活动且恒等写通道可用、单线程、页表
/// 页在恒等可写范围内。函数自身负责全局预检（几何错误在任何
/// 页表写入之前返回）与错误传播；任何安装错误都必须让启动入口
/// 终止，不能把部分建立的 Direct Map 交给后续阶段。
pub unsafe fn establish_boot_dm<A: BootArch>(
    kernel_info: &KernelInfo,
    root: PhysBytes,
    policy: &DmPagePolicy<A>,
) -> Result<(), PageTableError>
```

它只从 `policy` 取布局和窗口，收到任何安装错误都必须让启动入口终止；不能把部分建立的 Direct Map 当作可用结果继续传递。

### 6.7 调用链

目标调用链：

```
arch_boot_impl<A: BootArch>
  │
  ├─ let support = boot_validate_and_prepare::<A>()
  │       .unwrap_or_else(|e| panic!("...: {e}"))
  │       // 校验 KernelInfo、注册 boot allocator、执行
  │       // detect_boot_page_support::<A>()，不选页
  │
  ├─ let layout = DirectMapLayout::<A>::from_arch()
  ├─ let policy = plan_boot_pages::<A>(&support, kernel_layout, &layout)
  │       .unwrap_or_else(|e| panic!("...: {e}"))
  │
  ├─ build_bootstrap_root_and_enable::<A>(root_page, &policy.kernel())
  │
  └─ unsafe { establish_boot_dm::<A>(kernel_info, root_page, &policy.dm()) }
       .unwrap_or_else(|e| panic!("...: {e}"))

arch_boot_resume_high_half
  │
  ├─ let support = boot_validate_and_prepare::<CurrentBootArch>()
  │       .unwrap_or_else(|e| panic!("...: {e}"))
  ├─ let layout = DirectMapLayout::<CurrentBootArch>::from_arch()
  ├─ let dm = plan_dm_policy::<CurrentBootArch>(&support, &layout)
  │       .unwrap_or_else(|e| panic!("...: {e}"))
  │
  └─ unsafe { establish_boot_dm::<CurrentBootArch>(kernel_info, root_page, &dm) }
       .unwrap_or_else(|e| panic!("...: {e}"))
       // 恢复路径不做内核镜像选页，只规划它实际需要的 DM

establish_boot_dm<A: BootArch>
  ├─ let kernel_window = policy.kernel_window()
  ├─ Kernel DM：候选区间检查 window.pa_capacity() 后预检
  ├─ let vm_window = policy.vm_window()
  ├─ VM DM：候选区间先裁剪到 vm_window.pa_capacity() 再预检
  ├─ 两个窗口共用同一份 DmPagePolicy<A>，窗口来自策略保存的布局
  ├─ 先对所有 Kernel DM / VM DM 候选调用 preflight_boot_dm_range
  ├─ 全部几何预检通过后才调用 install_prevalidated_boot_dm_range
  ├─ 写第一张叶子
  └─ 不再查询 CurrentPaging 能力

之后进入 kmain
  └─ smp_init() 启动 AP，AP 加载同一个 bootstrap root
```

`boot_validate_and_prepare` 的职责随之改变：它不再返回 `u64`，而是返回 `BootPageSupport<A>`。它负责：

- 校验 `KernelInfo` 的 `kv`/`kp`/`ks` 输入；
- 注册 boot allocator；
- 调用 `detect_boot_page_support::<A>()` 获取能力事实；
- 检查 `support.phys_addr_limit() >= IDENTITY_MAP_END`，为 Step 1 的 4 GiB 恒等映射闭合 PA 上界（§6.5），不满足即拒绝启动。

它不决定内核映射或 Direct Map 的页大小。分配器注册仍然需要“镜像大小按 4 KiB 向上取整”这样的低层断言，但这属于分配器边界，不是页大小策略。

两个启动入口共享同一个能力检测和同一套 DM 规划逻辑，但不必共享内核映射规划：

- `arch_boot_impl` 需要内核镜像映射和 DM，调用 `plan_boot_pages`；
- `arch_boot_resume_high_half` 只恢复高半运行环境，调用 `plan_dm_policy`，不因内核镜像布局不满足某个大页对齐而失败；
- 两条路径的 `establish_boot_dm` 都只接收 `DmPagePolicy<A>`；窗口由策略保存的 `DirectMapLayout<A>` 生成，Kernel DM 与 VM DM 共用策略和布局。

恢复路径不能绕过 DM 策略直接写页表；它只是不需要内核映射策略。

这里有一个必须按真实启动顺序处理的约束：ARM64 的 `TCR_EL1` 在 `AArch64Paging::enable()` 中写入（`os/arch/src/arm64/paging.rs:fn enable`），RISC-V 的 `satp.MODE` 在 `Riscv64Paging::enable()` 中选择（`os/arch/src/riscv64/paging.rs:fn enable`），而目标调用链中的能力检测发生在 `build_bootstrap_root_and_enable()` 之前。因此 `phys_addr_limit()` 和 `validate_virtual_range()` 必须回答“本次启动准备使用的目标翻译模式”，不能盲目读取检测时固件留下的早期寄存器值。目标实现应使用 `Paging` 的目标模式常量/配置，并在 `enable()` 之后断言实际生效的 TCR/`satp` 与规划依据一致；如果无法建立这种一致性，就拒绝启动，而不是回退到 BSP 的早期状态。

泛型函数还必须始终使用 `A::Paging`、`A::DirectMap` 和 `A::DmInstaller`。当前实现里 `boot_validate_and_prepare<P>` 仍直接使用 `minix_arch::CurrentDirectMap`（`os/kernel/src/lib.rs:fn boot_validate_and_prepare`），`establish_boot_dm` 仍使用 `CurrentDirectMap`/`CurrentDmCoverage`（`os/kernel/src/dm_coverage.rs:fn establish_boot_dm`）；迁移到 `A: BootArch` 时必须替换这些调用点，否则策略携带的架构身份不会约束执行路径。`CurrentBootArch` 只应出现在真正选择本次编译目标架构的入口。

### 6.8 `map_huge` 的防御性检查

每个 `map_huge` 实现的开头都执行能力、对齐、VA/PA 完整区间和地址上界检查，位置必须在任何页表分配或写入之前。`map_huge` 是运行期入口，仍然要独立负责这些检查，不能假设启动规划器已经替它验证过：

```rust
if !Self::supports_huge_page(size) {
    return Err(PageTableError::NotSupported);
}
if vaddr.get() % size.bytes() != 0 || paddr.get() % size.bytes() != 0 {
    return Err(PageTableError::InvalidAddress);
}
let _va_end = vaddr.get()
    .checked_add(size.bytes())
    .ok_or(PageTableError::InvalidAddress)?;
let pa_end = paddr.get()
    .checked_add(size.bytes())
    .ok_or(PageTableError::InvalidAddress)?;
if Self::validate_virtual_range(vaddr.get(), size.bytes()).is_err()
    || pa_end > Self::phys_addr_limit()?
{
    return Err(PageTableError::InvalidAddress);
}
```

`Paging` 对通用层只暴露语义化的物理地址和虚拟地址范围契约：

```rust
pub trait Paging {
    const PAGE_SIZE: usize;

    /// 当前系统可以安全映射的物理地址排他上界。
    /// 架构实现必须保证它不超过硬件实际支持的 PA 范围，也不超过
    /// 本架构 PTE 编码器能够正确表达的范围。
    fn phys_addr_limit() -> Result<u64, PageTableError>;

    /// 在本实现声明的目标翻译模式下，[base, base + len) 是否是合法的
    /// 虚拟地址范围。三架构的目标模式都是编译期固定配置（x86-64
    /// 4 级 PML4、ARM64 48 位 VA/TTBR1、RISC-V Sv39），因此这是对
    /// 声明配置的纯验证：不读取 TCR_EL1/satp/CR4 等活动寄存器。
    /// enable() 负责把目标配置写入硬件并断言实际生效值与声明一致。
    ///
    /// 合法性必须包含 canonical 检查，且两端点各自判定，判据按
    /// 架构：Sv39 要求 VA[63:39] 逐位等于 VA[38]；x86-64 四级要求
    /// VA[63:47] 逐位等于 VA[46]；ARM64 48 位要求 VA[63:48] 全 0
    /// 或全 1。同一个值在不同架构的 canonical 结论不同——
    /// 0xFFFF_FC00_0000_0000 在 x86-64 是规范地址，在 Sv39 是
    /// 非规范地址，曾作为 Kernel DM 基址造成难以追查的启动事故
    /// （Sv39 载荷为 0，落在不存在的顶级槽位，DM 加法产出垃圾；
    /// 见 direct_map.rs 的历史注释）。实现者不得用「长度不超过
    /// 上限」的检查替代逐端点的 canonical 判定。
    fn validate_virtual_range(base: u64, len: u64) -> Result<(), PageTableError>;
}
```

PTE 格式位宽 `PTE_PHYS_ADDR_BITS` 保留在架构分页实现内部，不进入通用规划器和公共安装预检。各架构的当前声明如下；它们不需要强行相同：

| 架构 | PTE 格式上限（架构私有） | `phys_addr_limit()`（系统语义上界）来源 |
|---|---:|---|
| x86-64 | 52 | 目标 4 级分页配置下，`CPUID.80000008H:EAX[7:0]`；读取前先确认 `CPUID.80000000H.EAX >= 0x80000008`；叶不可用时返回 `Err`，启动失败 |
| ARM64 | 48（当前实现） | 声明值 `1 << 48`（目标 IPS=5），带平台能力前提：启动读取 `ID_AA64MMFR0_EL1.PARange` 并要求 `PARange >= 0b101`（至少 48 位输出），不满足即拒绝启动；`enable()` 后断言实际 TCR 与声明一致 |
| RISC-V Sv39 | 56 | 平台配置声明的有效 PA 上界；当前支持配置采用 `1 << 48` 的保守值，目标 `satp.MODE=Sv39` 由 `enable()` 落实并断言，内存图不参与该判断 |

x86-64 的 `CPUID.80000008H` 和 `CPUID.80000001H` 遵循同一原则：先查最大扩展叶，再读具体字段。最大扩展叶不足时没有可信 PA 上界，`phys_addr_limit()` 返回错误，启动显式失败；不能默认 52 位，也不能从内核映像或 RAM 位于低地址推断剩余范围。

位宽到上界的转换在 `phys_addr_limit()` 的实现源头闭合以下规则，不停留在 trait 文档：

- **语义**：返回排他上界 `min(硬件/平台报告上界, 本架构 PTE 编码上界)`。“可安全映射”必须两个条件同时成立。
- **统一取小规则**：`min` 对两个方向都适用，不存在第二套失败路径。报告上界小于编码上限时取报告值（ARM64 当前 IPS=5 的 48 位对格式掩码的 52 位，RISC-V 平台 48 位对 Sv39 编码 56 位）；报告值超过编码上限时取编码上限——编码能力才是硬约束，截断后的上界仍然真实。
- **结构性验证（先于移位）**：来源叶缺失（x86-64 最大扩展叶不足 `0x80000008`）、位宽为零、位宽不小于 64（`1u64 << bits` 不可安全计算）三类返回错误、启动失败。移位只发生在验证之后。
- **闭合推论**：`phys_addr_limit() <= 编码上界` 由构造成立，因此 `map_huge` 与安装预检按 `phys_addr_limit()` 放行的物理地址，架构编码器必然能够表达；“通用预检通过、编码器事后拒绝、中间页已分配”的形态被整体排除。
- **错误表达**：能力检测层复用 `PageSupportError::InvalidPhysAddrLimit`，架构内部探针复用 `PageTableError`；不新增错误类型，`PTE_PHYS_ADDR_BITS` 不进通用层。

`phys_addr_limit()` 与 `validate_virtual_range()` 的职责性质不同，不统称“纯验证”：前者在 x86-64 是硬件事实查询（读 CPUID），在 ARM64 是「能力标识验证（PARange 前提）+ 声明配置值」、在 RISC-V 是声明配置的静态值——配置的正确性由 `enable()` 写入硬件并做写后断言落实；后者才是对声明配置的纯范围验证（同输入同输出、不读任何寄存器）。两者共同的硬约束是都不以检测时的活动控制寄存器（`TCR_EL1`/`satp`/`CR4`）决定结果，这一约束有源码依据：ARM64 的 `enable()` 用本端口的编译期配置重写 `TCR_EL1`——UEFI 固件为自己的页表配置的 TCR 与目标模式不必然相同，规划期读活动寄存器必然读到不代表目标模式的值；RISC-V 在 OpenSBI 之后从零开启 Sv39，此前 MMU 未启用。能力标识寄存器（x86 CPUID、ARM64 `ID_AA64MMFR0_EL1`）不在此列：它们是只读事实，读取不受启动阶段影响。`enable()` 之后的断言负责核对硬件实际生效值。若未来某个架构需要运行期动态选择翻译模式，这两个方法的接口必须改为消费显式的不可变配置快照，不允许同一个静态方法在 `enable()` 前后隐式改变判断依据。

ARM64 不能仅凭 `ADDR_MASK` 覆盖到 bit 51 就宣称支持 52 位输出。当前页表编码按 48 位实现（`TCR_EL1.IPS=5`），所以格式上界和有效上界都声明为 48。未来启用 52 位输出必须同时实现 LPA2 所需的描述符格式、控制位和高位地址编码，并以真实描述符路径测试 bit 51 的写入、读取和解释；在这些条件满足前，不得把 ARM64 的格式上限写成 52。

ARM64 的 48 位声明还有一个不能省略的前提：`TCR_EL1.IPS=5` 是软件写进控制寄存器的**配置**，不证明处理器实际支持 48 位物理地址——不同实现可能只有 40 或 44 位（Arm ARM 明确警告输出地址大小配置高于实现支持范围时行为不可依赖）。因此启动时必须读取 `ID_AA64MMFR0_EL1.PARange` 并要求 `PARange >= 0b101`（至少 48 位），不满足即拒绝启动；通过后 `1 << 48` 的声明值才成立。这条检查与 x86-64 先查最大扩展叶是同一模式：**能力标识寄存器先于配置**。它不违反“规划期不读活动寄存器”的原则：`ID_AA64MMFR0_EL1` 是只读的实现能力标识，不是固件可配置状态，任何时刻读值相同——与固件遗留的 `TCR_EL1`（可被重写的配置）语义不同，两者必须区分。仓库现状是 ARM64 路径完全没有读取 PARange（全仓零命中），这条前提是本次设计新增的启动检查。

RISC-V 的内存图只回答“哪些物理区间可以作为资源候选”，不回答“处理器能寻址多高的 PA”。当前支持配置把 `phys_addr_limit()` 声明为 `1 << 48`：它覆盖 4 GiB 恒等映射、当前 RAM/设备窗口和 16 GiB VM DM 的物理偏移容量；如果未来支持更高的物理地址，必须由平台配置提供新的明确上界并增加覆盖测试，不能从内存图最高地址推导。

`detect_boot_page_support` 把这个有效上界存进 `BootPageSupport<A>`，`validate()` 只检查它非零；架构实现保证它同时不超过硬件 PA 能力和自身 PTE 编码能力。`map_huge` 只依据 `phys_addr_limit()` 做通用检查，架构编码器在内部仍然拒绝无法编码的高位，不能静默掩码。`install_boot_dm_range` 用策略保存的同一个有效上界做预检。`install_leaf` 的调用契约要求 `paddr` 已通过这条语义边界。

理由：

- 启动选页已经保证不会请求不支持的尺寸，所以正常路径不会触发；
- 如果未来有调用方绕过选页直接请求 1 GiB，这里会返回错误，而不是写入非法条目；
- 错误类别是明确的 `NotSupported` 或 `InvalidAddress`，比硬件异常更容易诊断；
- 检查发生在写表之前，不会留下“能力不支持但中间页表已经分配”的半成品。

对齐错误应与能力错误分开：

| 条件 | 错误 |
|---|---|
| 尺寸不在当前实现或目标硬件能力内 | `PageTableError::NotSupported` |
| 地址不按尺寸对齐 | `PageTableError::InvalidAddress` |
| 物理地址超出 `phys_addr_limit()`；架构编码器另行拒绝无法编码的高位 | `PageTableError::InvalidAddress` |
| 目标叶子已经被占用 | `PageTableError::AlreadyMapped` |

`map_huge` 不负责降级。调用方要映射 1 GiB 但系统不支持时，它返回 `NotSupported`，由调用方决定是否改用 2 MiB；分页实现不能在不通知调用方的情况下静默改变语义。

当前 `map_huge` 实现还没有统一的地址对齐检查；本次应一并补齐，并保证能力检查先于写表。

如果担心启动路径重复调用 CPUID，可以让 `probe_1gb_page_support` 在第一次调用后缓存结果。当前启动路径调用次数很少，缓存不是前置条件。

### 6.9 SMP：能力必须是系统级平台契约

在本文支持边界内，不变量写成：

> 对任何会被多个 CPU 共享的页表，`supports_huge_page(size) == true` 与 `phys_addr_limit()` 都必须是“当前支持平台前提下的系统级保证”：所有可能访问该页表的 CPU 都能解释这个尺寸的叶子，也都能安全映射该上界以内的物理地址。它们不是由某一次 BSP CPUID/TCR 查询独立证明的整机事实；平台必须满足同构前提。

这条不变量直接约束启动策略：

- BSP 在建立共享叶子之前，必须已经知道所有 AP 的大页能力、有效 PA 上界和翻译配置，或者平台保证这些条件同构；
- 不能因为 BSP 支持 1 GiB 就直接在所有共享页表里使用 1 GiB；也不能因为 BSP 的 `phys_addr_limit` 较大，就把这个结论直接用于共享根；
- 运行期在某个 CPU 上调用 `supports_huge_page` 或 `phys_addr_limit` 时，得到的也必须是平台级答案，而不是“当前 CPU 恰好支持”；
- 共享根的页表格式、翻译模式（x86 页表级数、ARM64 TCR/IPS、RISC-V Sv39 等）和相关控制位必须在所有参与 CPU 上兼容；
- 文档和测试必须区分“单 CPU 查询返回值”和“平台是否满足同构前提”。前者不能单独证明后者。`BootPageSupport` 保存的是满足系统级契约后的结果；如果实现只读取 BSP 的 CPUID/TCR，就只有在平台明确保证同构时才允许这么做。

当前 x86-64 的实际情况：

- AP 在 `kmain` 的 `smp_init` 中启动，晚于 BSP 的 Direct Map 建立；
- AP 加载 BSP 的 `current_cr3_pa()`，共享同一棵 bootstrap root；
- `supports_huge_page` 只探测执行探针的 BSP；
- AP 路径没有页大小能力校验；
- `cpu_identify` 的 AP 身份表填充仍属于后续 SMP 工作。

因此当前支持边界必须明确写成：

> 只支持共享页表所需的页大小能力、有效物理地址上界和翻译配置同构的 SMP 平台。异构组合不在本次改动范围内。

“同构”不是“所有 CPU 型号相同”，而是“所有 CPU 对共享页表使用的页大小给出相同能力答案，并且对 `phys_addr_limit()`、页表格式和翻译配置给出兼容答案”。平台固件、虚拟化配置和 CPU 混合拓扑都必须满足这个条件。

如果未来要支持异构平台，不能只在 AP 启动后逐 CPU 查询。那时需要：

1. 在建立共享大页之前收集所有 CPU 的能力交集；
2. 用交集决定内核映射和 Direct Map 的页大小；
3. 或者在 AP 入口拒绝加载包含不支持叶子的页表根，并明确降级为少 CPU 运行。

第 3 种做法只能避免 AP 走坏页表，不能挽回“本来可以用 2 MiB 让所有 CPU 上线”的选择。因此它不是系统级能力协商的替代品。

测试上，至少要有一次 `-smp4` 的真实启动，确认 BSP 建立的页表能被所有 AP 正常走过。这个测试只覆盖同构 SMP：它能发现“AP 没有正确走共享页表”的实现错误，不能证明异构 CPU 下的能力安全。当前没有能力异构的硬件测试环境，这项限制应在文档和后续测试计划中保留。

### 6.10 显式化页大小前提与策略验证

当前三种架构都满足：

```rust
Paging::PAGE_SIZE == 4096
HugePages::FALLBACK_HUGE_PAGE_SIZE == PageSize::MIB_2
HugePages::HUGE_PAGE_SIZES.contains(&PageSize::MIB_2)
```

目标设计不再让这些前提藏在调用参数里：

- `DmPagePolicy` 携带 `base_page_size`、`fallback_page_size` 和可选的 `preferred_page_size`；
- `plan_range_units` 从策略读取三个尺寸，不写死 1 GiB/2 MiB/4 KiB；
- `install_boot_dm_range` 不再出现裸 `4096`；
- `BootPageSupport::validate()` 统一检查静态页大小配置，`plan_dm_policy` 只处理能力事实和窗口对齐；
- 每个架构模块增加 DM 布局编译期断言；`IDENTITY_MAP_END` 的整除断言放在 kernel 的启动规划模块。

验证规则分成架构静态配置、本次启动策略、单次叶子安装三类，另加编译期不变量。类型不变量、静态配置、策略和执行边界不能混成一张“都返回错误”的表：

| 验证类别 | 规则 | 失败结果 |
|---|---|---|
| 架构静态配置 | `A::Paging::PAGE_SIZE` 能转换为合法 `PageSize`，且与快照一致 | `InvalidBasePageSize` |
| 架构静态配置 | `phys_addr_limit > 0`，且由架构保证不超过其 PTE 编码能力 | `InvalidPhysAddrLimit` |
| 架构静态配置 | 回退尺寸等于 `FALLBACK_HUGE_PAGE_SIZE`、在 `HUGE_PAGE_SIZES` 内且大于基础页 | `InvalidFallback` |
| 架构静态配置 | 首选尺寸等于 `HUGE_PAGE_SIZE`、在 `HUGE_PAGE_SIZES` 内且大于回退尺寸 | `InvalidPreferred` |
| 本次启动策略 | 内核布局的虚拟基址、物理基址和长度加和不溢出，虚拟区间通过 `validate_virtual_range` | `InvalidKernelLayout` |
| 本次启动策略 | Direct Map 布局与架构换算常量逐项相等，容量按基础页对齐、终点和 VA 上界自洽，两个 VA 区间通过 `validate_virtual_range` 且互不重叠 | `InvalidDmLayout` |
| 本次启动策略 | 内核镜像物理末端不超过 `phys_addr_limit` | `InvalidKernelLayout` |
| 本次启动策略 | `IDENTITY_MAP_END` 不超过 `phys_addr_limit`（4 GiB 恒等窗口完整落在有效 PA 上界内） | 启动拒绝（`boot_validate_and_prepare`） |
| 本次启动策略 | 首选尺寸不受支持，或布局不满足首选对齐 | 正常降级到回退 |
| 本次启动策略 | 首选和回退都不满足内核布局 | `NoKernelPageSize` |
| 本次启动策略 | 回退尺寸不受硬件支持 | `FallbackUnsupported` |
| 本次启动策略 | `kernel_base % fallback != 0` 或 `vm_base % fallback != 0` | `InvalidDmAlignment` |
| 本次启动策略 | `preferred_page_size` 不受硬件支持 | 正常降级为 `None` |
| 本次启动策略 | `kernel_base` 或 `vm_base` 不按首选尺寸对齐 | 正常降级为 `None` |
| 单次叶子安装 | 每个叶子的 `VA`/`PA` 对齐、区间包含、窗口边界和加和不溢出 | `PageTableError::InvalidAddress` |
| 单次叶子安装 | 叶子末端的物理地址不超过 `phys_addr_limit` | `PageTableError::InvalidAddress` |
| 编译期 | `Kernel DM` 的基址、`base + size` 和 `KERNEL_DM_VA_END_LIMIT` 都落在架构 VA 合法范围，且 `base + size` 不超过该上限 | 编译期断言失败 |
| 编译期 | `IDENTITY_MAP_END` 能同时被首选和回退尺寸整除 | 编译期断言失败 |

`PageSize` 的“数值合法”由私有构造器保证，`BootPageSupport::validate()` 不再重复检查 2 的幂；两个合法 `PageSize` 只要前者大于后者，前者就一定能被后者整除，所以 `plan_dm_policy` 也不检查整除。为这些不可能失败的检查写测试只会掩盖真正的配置错误。需要测试非法尺寸时，应测试 `PageSize` 构造器拒绝非 2 的幂。

静态配置错误由 `BootPageSupport::validate()` 在能力检测和每个规划入口统一返回；首选尺寸不受支持或布局不满足首选约束是正常降级；回退尺寸不受支持或回退窗口不对齐，则必须在启动前失败，因为 `plan_range_units` 最终必须落到回退或基础页。

编译期断言示例：

```rust
const _: () = assert!(<X86_64Paging as Paging>::PAGE_SIZE == 4096);
const _: () = assert!(
    <X86_64Paging as HugePages>::FALLBACK_HUGE_PAGE_SIZE.bytes()
        == PageSize::MIB_2.bytes()
);
const _: () = assert!(
    IDENTITY_MAP_END % <X86_64Paging as HugePages>::HUGE_PAGE_SIZE.bytes() == 0
);
const _: () = assert!(
    IDENTITY_MAP_END
        % <X86_64Paging as HugePages>::FALLBACK_HUGE_PAGE_SIZE.bytes() == 0
);
```

`plan_range_units` 的契约因此变成：

- 首选大页由策略给出，当前通常是 1 GiB，但算法不再假设它一定是 1 GiB；
- 回退大页大小由策略给出，当前是 2 MiB；
- 基础页大小由策略给出，当前是 4 KiB；
- 每个发射单位必须完整落在候选区间内，不能超出区间，也不能跳过区间内的字节；
- 发射单位只能是策略允许的三个尺寸之一。

如果未来某个架构没有 2 MiB 回退，`detect_boot_page_support` 记录回退不支持，`plan_dm_policy` 必须在启动前返回错误，而不是让 `plan_range_units` 用错误尺寸继续写页表。

### 6.11 运行期地址换算的窗口边界

启动规划已经让“超出窗口容量的映射”在写表前失败，但窗口边界还必须延伸到运行期换算：`kernel_phys_to_virt`/`vm_phys_to_virt` 目前是无条件加法，`PA < phys_addr_limit` 而 `PA >= 窗口容量` 时，换算仍返回一个没有被该窗口覆盖的虚拟地址——算术成功不等于地址有效。仓库里有两个直接证据说明这不是理论风险：

- `virt_to_phys` 的反向加法/减法已经产生过真实事故：内核镜像高半区和内核栈的 VA 不在两个 DM 窗口内，`virt_to_phys` 对它们产出伪物理地址（NK4-C 1.51 判例，`os/kernel/src/misc.rs`、`os/kernel/src/syscall_copy.rs`、`os/kernel/src/syscall_signal.rs` 等多处调用点被迫改走 `AddressRef::Process` 通道）；
- `os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl` 的安全注释以「DM 窗口覆盖全部物理内存」为前提直接解引用——这是平台假设而不是代码保证；同时 `os/kernel/src/vm.rs:fn cross_space_read` 已经在逐 chunk 解引用前用 `physical_range_in_dm_window`（`os/kernel/src/vm.rs:fn physical_range_in_dm_window`）守卫 DM 别名的有效性，说明守卫需求真实存在，只是目前散落在个别调用点。

先把三种不同的保证分开——它们经常被混成一句“地址有效”，但证明责任完全不同：

| 保证 | 含义 | 由谁证明 |
|---|---|---|
| 窗口内 | VA 落在配置好的 Direct Map 地址区间内 | 窗口常量即可判定 |
| PA 可表达 | PA 不超过系统有效物理地址上界 | `phys_addr_limit()`；Kernel DM 的窗口容量（x86-64/ARM64 约 127 TiB、Sv39 约 255 GiB）可以大于它，VM DM 的窗口容量（1–16 GiB）通常小于它 |
| 实际可访问 | 对应物理范围的叶子确实已安装，且调用方有权访问 | 只有来源证明链或显式映射检查能证明；窗口常量与 `try_*` 都不提供 |

第三层不能省略的原因：Direct Map 按候选区间建立、刻意保留资源空洞，窗口容量内两个候选之间的空洞没有叶子；窗口容量与 `phys_addr_limit` 之间的缝隙同理。

**纯地址计算不是 unsafe 边界**。`*_unchecked` 换算只做整数加减并返回 `VirBytes`/`PhysBytes`，不形成引用、不触碰内存——“结果将来可能被解引用”不是本次计算自身的安全前提（`slice::get_unchecked` 之所以是 `unsafe`，是因为它直接构造引用，与本处不同）。因此快速换算是**安全函数**：名字里的 `_unchecked` 表达“不做任何验证”——不验证窗口、PA 上界或映射存在性，返回值不得在缺少第三层证明时解引用。真正承担安全前提的是形成指针并访问内存的操作：各架构 `phys_to_ptr` 与按 DM 别名读写的拷贝路径是 `unsafe` 边界所在，调用方在那里出示来源证明链或显式映射检查。`try_*` 与 `_unchecked` 的区别只是“是否检查前两层”，两者都不提供第三层——检查版返回 `Some` 不构成解引用的充分条件。

**检查换算的归属层与架构身份**。可换算上界需要 `phys_addr_limit()`，那是 `Paging` 的事实；`DirectMapArch` 只管布局，不应隐式引用某个具体 `Paging` 实现，也不该让调用方随手传入一个来历不明的上界。检查换算因此不放在 `DirectMapArch` 上，而是放在 `BootArch` 绑定层，且**运行期 API 绑定当前构建目标**：入口是非泛型函数，窗口常量取 `CurrentBootArch` 的投影，上界读启动期一次写入的运行期配置。不提供 `<A: BootArch>` 泛型的运行期入口——运行期配置是全局单值，不带架构身份，把它泛型化会让「任意 A 共用同一个未标注身份的上界」与 `BootArch` 的绑定承诺冲突。判定逻辑本体是显式参数的纯函数，测试与 mock 架构以任意窗口/上界组合直接调用它，不读写全局配置。不新增 trait，布局 trait 保持纯布局，能力查询留在 `Paging`。

```rust
// DirectMapArch：纯布局，安全算术，不做任何验证。
pub trait DirectMapArch {
    // ……窗口常量不变……

    /// 纯算术换算：`wrapping_add` 加法。安全函数；`_unchecked` 表示
    /// 不验证窗口、PA 上界或映射存在性，返回值不得在缺少来源证明
    /// 链或显式映射检查时解引用。用 wrapping 语义是有意的：无效
    /// 输入在 debug 与 release 下行为一致（不因溢出检查 panic 或
    /// 因 release 环绕而分叉），它是定义良好的原始整数换算；地址
    /// 合法性全部交给 `try_*`。
    fn kernel_phys_to_virt_unchecked(phys: PhysBytes) -> VirBytes;
    fn vm_phys_to_virt_unchecked(phys: PhysBytes) -> VirBytes;

    /// 反向纯算术：`wrapping_sub` 减法，窗口由函数名显式指定。
    /// 按地址大小分发的单一 `virt_to_phys` 定为**删除对象**：它对
    /// 窗口外 VA 产出伪物理地址（NK4-C 1.51 判例），而生产活调用
    /// 只剩 VM 服务器内部包装一处（`os/servers/vm/src/direct_map.rs:
    /// fn virt_to_phys`，输入只可能是 VM 窗口内 VA；kernel 侧命中
    /// 均为事故注释与测试）。包装迁到显式窗口版本后，通用入口从
    /// trait 删除，不给后续调用者留误用空间。
    fn kernel_virt_to_phys_unchecked(virt: VirBytes) -> PhysBytes;
    fn vm_virt_to_phys_unchecked(virt: VirBytes) -> PhysBytes;
}

// BootArch 绑定层：检查换算，正反向共用同一可换算上界。
// 运行期入口绑定当前构建目标：窗口常量 = CurrentBootArch 投影，
// 上界 = 启动期一次写入的运行期配置（不逐次重查 CPUID）。

/// 正向：pa < 可换算上界才返回 Some。
pub fn try_kernel_phys_to_virt(phys: PhysBytes) -> Option<VirBytes>;
pub fn try_vm_phys_to_virt(phys: PhysBytes) -> Option<VirBytes>;

/// 正向范围：[pa, pa + len) 经 checked 加法完整落在可换算上界内。
pub fn try_kernel_region(phys: PhysBytes, len: u64) -> Option<VirBytes>;
pub fn try_vm_region(phys: PhysBytes, len: u64) -> Option<VirBytes>;

/// 反向：va 必须落在对应窗口的半开区间内，且偏移 va - base
/// 小于可换算上界——窗口容量可以大于 phys_addr_limit，只查
/// 窗口会放行不可表达的 PA，与正向契约不对称。
pub fn try_kernel_virt_to_phys(virt: VirBytes) -> Option<PhysBytes>;
pub fn try_vm_virt_to_phys(virt: VirBytes) -> Option<PhysBytes>;

/// 反向范围：[va, va + len) 完整落在窗口内，且对应的 PA 区间
/// [va - base, va - base + len) 完整落在可换算上界内。
pub fn try_kernel_virt_region(virt: VirBytes, len: u64) -> Option<PhysBytes>;
pub fn try_vm_virt_region(virt: VirBytes, len: u64) -> Option<PhysBytes>;

/// 判定逻辑本体：显式参数的纯函数。生产包装以
/// （CurrentBootArch 窗口常量, 运行期已验证上界）调用它；
/// 测试与 MockBootArch 以任意窗口/上界组合调用它，不碰全局。
fn convertible_bound(window_capacity: u64, verified_limit: Option<u64>)
    -> Option<u64>
{
    verified_limit.map(|limit| core::cmp::min(window_capacity, limit))
}
```

运行期配置的写入与读取语义：`boot_validate_and_prepare` 在两个启动入口（`arch_boot_impl` 与 `arch_boot_resume_high_half`）都先于建表执行，校验成功后把已验证的 `phys_addr_limit` 一次写入。两个入口的互斥性有源码依据：它们是同一内核镜像的两种启动形态——`arch_boot_impl`（`os/kernel/src/lib.rs:fn arch_boot_impl`）从加载器直接启动并进入 `kmain`，`arch_boot_resume_high_half`（`os/kernel/src/lib.rs:fn arch_boot_resume_high_half`）承接 boot-shim 已建好的表并跳过高半入口，二者都以不返回的方式进入后续阶段，同一执行只走其一（启动协议选择，不共存）；因此 `boot_validate_and_prepare` 在一次启动中恰好执行一次，两次调用本身就是控制流错误。写入发生在 BSP 单线程启动段；AP 在 `kmain` 之后才由 `smp_init` 启动，所有运行期读取者都在初始化之后，无并发窗口。二次初始化或以不同值再写保持 panic（违约信号，不放宽为覆盖）；未初始化读取返回 `None`——`None` 统一表示「无法证明可换算」，不区分地址越界与上界不可得，诊断由启动期校验（`PageSupportError::InvalidPhysAddrLimit`）承担，不在热路径上引入错误体系。首次可能的运行期换算远晚于初始化（`kmain` 之后的 IPC/拷贝路径）；`establish_boot_dm` 用的是策略快照里的 `phys_addr_limit`，不依赖这份配置，启动建表不因它未初始化而受影响。

返回类型用 `Option`：`None` 统一表示「无法证明可换算」——可能是地址越出可换算上界，也可能是未取得可信上界；热路径不需要区分这两种成因，诊断由启动期校验承担，因此不需要错误枚举，也不让布局层依赖页表操作的错误类型。**`try_*` 的成功含义必须写准：它只证明前两层（可换算），不证明 PTE 存在**；把 `try_*_region` 返回 `Some` 当成实际映射已建立的证明是误用。检查用的窗口容量就是 `A::DirectMap` 的常量——§6.6 已经要求布局与常量逐项相等，因此对常量检查等价于对已验证布局检查，不会出现“布局验证过的窗口”与“换算检查的窗口”两套口径。

边界规则正反两个方向统一为半开区间语义，不存在两套行为：单地址转换要求地址严格位于区间内（正向 `pa < 上界`；反向 `va` 落在窗口半开区间内且偏移 `< 上界`）；非空范围要求整体落在有效区间内，末端可以恰好等于排他边界（`pa + len == 上界` 接受，`va + len == 窗口终点` 接受）；零长度范围不豁免起点检查（`len == 0` 仍要求起点严格位于区间内）；起点等于排他边界一律拒绝。往返性质作为对称性的验收：可换算范围内的地址，正反检查换算往返得到原地址；越过 `phys_addr_limit` 的输入在正反两个方向都返回 `None`。

三个直接解引用场景的第三层证明链，按来源逐一核对：

- **IPC 拷贝**（`os/kernel/src/ipc.rs:fn copy_via_root_pages`）：PA 来自调用者页表的 walk，该页 present 且 USER_ACCESSIBLE；用户可映射页只能来自 VM PMM 管理的常规内存，PMM 内存 ⊆ memmap 候选 ⊆ DM 覆盖。这是间接证明链，成立依赖“用户页必然来自常规内存”的分配器契约——链条成立，但它是分配器性质，不是换算函数能检查的。
- **跨空间读取**（`os/kernel/src/vm.rs:fn cross_space_read`、`fn nk4a_copy_verify`）：范围来自用户页表 walk 与 handoff 边界（root/bump 已被 `validate_bootstrap_tree` 约束在 DM 可表达范围内），且拷贝主循环在解引用前对 DM 别名逐 chunk 调用 `physical_range_in_dm_window`。该函数的现状要如实定级：它对范围内每页执行活动根的 walk（`walk(root, va)`，`os/kernel/src/vm.rs:fn physical_range_in_dm_window`），等价于 MMU 即将执行的同一遍翻译——但它目前只判 `walk(...).is_none()`，**只证明翻译存在，不证明翻译指向预期 PA，也不检查权限**（`walk` 返回的 `(PhysBytes, PageFlags)` 被丢弃；消息缓冲直写循环里同样 `_fl` 忽略权限）。因此它现在只能称为**映射存在性检查**。第三层证明的完整契约是三条：范围内每个 DM 别名页存在翻译；walk 返回的 PA 与预期物理区间逐页一致；权限满足本次访问（写路径要求可写）。目标设计中该守卫升级为三条全查——walk 已返回 PA 与 flags，逐页比对 `pa == base + offset` 与权限位是纯增检查，不需要新机制。这是应对窗口内空洞与错译的正确守卫形态。
- **消息缓冲解引用**（`os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl`）：缓冲页映射在调用者页表里，但「页表里 present」不足以锚定来源——用户地址空间除了 PMM 页，还容纳经 `vm_map_phys` 建立的 DIRECT 设备映射（`os/servers/vm/src/map_phys.rs:fn handle_map_phys`：接受任意 `phys_addr`、只做权限检查、没有 DM 资格约束）。证明链只对 PMM 页闭合：VM 的 PMM 自由链是「常规内存 ∩ DM 可表达」的交集（`os/servers/vm/src/boot.rs` 的 A2 自由链构建注释），PMM 页 ⊆ memmap 候选（来源 1；跨来源过滤只丢弃低优先级候选，memmap 永远存活）⊆ Kernel DM 覆盖；候选整段丢弃只影响 reserved/module（来源 3/4），不影响这条链。设备映射页不在链上（MMIO 是刻意保留的洞）。关于物理连续性，源码形态是：直写循环**逐页 walk、按页界分块**（`chunk = min(bytes − off, 0x1000 − (va & 0xfff))`，每页重新翻译），整缓冲区的连续 PA 假设不存在、也不得引入。迁移契约据此固定：保持逐页 walk + 页界 chunk 结构；`try_kernel_region` 只应用于不跨页界的单页 chunk（页内天然连续）；第三层检查升级为上面三条全查（存在 + PA 一致 + 写权限）——「消息缓冲必须位于普通内存」同时写为 IPC 契约，设备映射当消息缓冲属于调用方违约，内核侧兜底是显式检查拒绝。

现有生产调用点的分类与归宿（按 PA 来源归纳，不是抽查）：

| 调用点类别 | 代表位置 | PA 来源 | 前提如何成立 | 目标归宿 |
|---|---|---|---|---|
| 页表访问通道 | 各架构 `phys_to_ptr`（`os/arch/src/x86_64/paging.rs:fn phys_to_ptr` 等）、`os/kernel/src/pte_walk.rs` | PTE 内容与 `pt_alloc` 分配的页——内核自己写入的值 | 写入侧（安装器、`map_huge`）已按窗口容量与 PA 上界验证 | 换算改用 `_unchecked`（安全函数）并注明依据；unsafe 边界在指针形成/解引用处 |
| IPC / 跨空间拷贝 | `os/kernel/src/ipc.rs:fn copy_via_root_pages`、`os/kernel/src/vm.rs:fn nk4a_copy_verify`、`os/kernel/src/vm.rs:fn cross_space_read` | 用户页表 walk 的结果或 VM handoff 范围 | 间接证明链（PMM ⊆ memmap 候选 ⊆ DM 覆盖）；`cross_space_read` 另有逐 chunk 显式守卫 | 保留既有守卫或改用 `try_*`；不允许“裸换算加裸解引用” |
| 直接解引用 | `os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl`（安全注释以「窗口覆盖全部物理内存」为由） | 消息缓冲页 | 证明链只对 PMM 页闭合；`vm_map_phys` 的 DIRECT 设备映射是链外例外（设备帧不经 PMM 分配，显式映射自证） | `try_kernel_region` + 保留显式映射检查；IPC 契约写明消息缓冲须为普通内存 |
| 反向 `virt_to_phys` | 生产活调用仅 VM 内部包装一处（`os/servers/vm/src/direct_map.rs:fn virt_to_phys`；kernel 侧命中均为注释与测试） | 两个 DM 窗口内的 VA | 仅当输入确在窗口内才成立；窗口外输入产出伪物理地址 | 通用入口删除：VM 包装迁到显式窗口版本；新调用点一律 `try_*_virt_to_phys` 或 `_unchecked` |

VM 服务器侧（`os/servers/vm/src/direct_map.rs`）遵守同一契约；VM DM 窗口容量只有 1–16 GiB，越界换算在那里风险更高。新增调用点的规则：PA 来源属于“内核写入的页表值”或“已验证范围”两类的，用 `_unchecked` 并在注释里写明依据；其余一律 `try_*`。无论走哪条路，形成指针并访问内存的那一行才是 `unsafe` 边界，那里必须持有第三层证明（来源证明链或显式映射检查）。迁移时先改「直接解引用」类（注释理由已错），页表通道只做改名加注释。

### 6.12 为什么不是合并两个 trait

一个自然的问题是：既然 `HugePages::map_huge` 和 `BootDmInstaller::install_leaf` 都在写页表叶子，为什么不合并？

因为两者的差异不是“实现细节不同”，而是“执行条件不同”：

| 维度 | 运行期 `map_huge` | 启动期 `install_leaf` |
|---|---|---|
| 根 | 任意活动页表根或新建根 | 固定 bootstrap root |
| 写通道 | Kernel DM / VM DM，按句柄绑定 | 恒等映射 `VA = PA` |
| 中间页分配 | 运行期页表页分配器 | boot bump allocator |
| 并发 | 可能多 CPU 并发 | 单线程启动 |
| 已有映射的处理 | 保留、拆分或返回冲突 | 对 VM 窗口的恒等残留做整体替换 |
| TLB 处理 | 运行期刷新策略 | 启动期整表 CR3 重载 |
| 失败语义 | 运行期错误可返回给调用者 | 启动布局错误，通常直接拒绝启动 |

如果合并，这些差异会变成参数或 `if` 分支。调用方需要知道“现在是启动还是运行”，trait 也就失去了约束力。共享的选页策略已经提取为 `plan_range_units`，这已经足够；再抽一个“通用页表写入器”会把多种策略塞进一个函数，复杂度高于收益。

### 6.13 为什么不是新增能力 trait

另一个方案是新增 `HugePageCapability` 或 `PageSizeSupport`，让两个旧 trait 都转发过去。这样做的问题是：

- 它没有消除重复入口，只是把重复从两个变成三个；
- 当前只有一个动态能力需要查询，新增 trait 没有第二个消费者；
- 能力查询本来就可以放在 `HugePages` 里，因为 `HugePages` 的职责就是“大页能力与操作”。

所以短期建议是“删掉安装器上的能力方法，把 `HugePages` 的查询补完整”，而不是“再加一个能力 trait”。

另外三个看似简单的替代方案也不合适：

| 替代方案 | 问题 |
|---|---|
| 把能力放进 `DirectMapArch` | 地址布局是“窗口放在哪里”，大页能力是“硬件能映射多大”；两者变化原因不同，放在一起会让布局 trait 承担硬件探测 |
| 删除 `HugePages` 的能力，只保留启动安装器上的能力 | 运行期迟早需要知道大页能力；把唯一入口放在启动安装器上，会让运行期代码依赖一个 boot-only 类型，生命周期方向反了 |
| 保留两个入口，只加一致性测试 | 测试只能证明当前实现一致，不能阻止新增架构或默认实现漂移；重复入口本身仍然存在 |

### 6.14 最终结构

目标设计可以压缩成六步：

1. **架构身份层**：`BootArch` 把 `Paging`、`DirectMap` 和 `BootDmInstaller` 绑定到同一个 `A`。
2. **架构能力层**：`PageSize`、`HugePages::supports_huge_page`，每个架构显式实现；`PageSize` 只保证数值合法。
3. **事实获取层**：`detect_boot_page_support<A>() -> BootPageSupport<A>`，只查询能力，同一页大小只有一份答案。
4. **纯规划层**：`plan_kernel_mapping`、`plan_dm_policy`、`plan_boot_pages`，只消费 `A` 绑定的能力和布局。
5. **已验证策略层**：`KernelMapPolicy<A>`、`DmPagePolicy<A>`、`BootPagePolicy<A>`，字段私有、手动实现 `Copy`/`Clone`；`KernelMapPolicy` 携带已验证的 `KernelLayout`，`DmPagePolicy` 携带已验证的 `DirectMapLayout<A>`。
6. **执行层**：`map_huge` 和 `BootDmInstaller<A::Paging>`，只接受已验证的 `PageSize` 和策略，不查询能力；启动入口是 `unsafe`，安全前提写在契约里。

这套结构没有引入新的能力 trait，也没有把四个职责合并成一个“大而全”的 paging 接口。它只是把已经有的事实放到唯一位置，把已经存在的策略从隐式前提变成显式类型，并用一个零大小标记阻止跨架构拼接。运行期换算的窗口边界由 `try_*` 换算与无检查版本的前提契约共同表达（§6.11）：与启动期的窗口验证共用同一组常量，建表阶段与访问阶段执行的是同一套边界。

## 7. 行为、边界与测试

### 7.1 行为变化

| 场景 | 改动前 | 改动后 | 是否期望 |
|---|---|---|---|
| CPU 支持 1 GiB，布局按 1 GiB 对齐 | 启动选 1 GiB；DM 选 1 GiB | 启动选 1 GiB；DM 选 1 GiB | 否，行为不变 |
| CPU 支持 1 GiB，布局只按 2 MiB 对齐 | 启动选 2 MiB；DM 选 1 GiB 或 2 MiB（取决于候选范围） | 启动选 2 MiB；DM 选 1 GiB 或 2 MiB（策略不变） | 否，行为不变 |
| CPU 不支持 1 GiB，布局按 1 GiB 对齐 | 启动选 1 GiB，可能启动失败；DM 选 2 MiB | 启动选 2 MiB；DM 选 2 MiB | 是，修复 |
| CPU 不支持 1 GiB，布局只按 2 MiB 对齐 | 启动选 2 MiB；DM 选 2 MiB | 启动选 2 MiB；DM 选 2 MiB | 否，行为不变 |
| 未来架构不支持 1 GiB，未实现能力 | 默认实现可能返回真，选 1 GiB | 缺少方法无法通过编译；显式实现返回假，选 2 MiB | 是，防御性修复 |
| 2 MiB 回退也不受支持 | 可能在 Direct Map 规划阶段用错误尺寸写表 | `plan_dm_policy` 返回错误；`plan_boot_pages` 因而失败；若内核映射本身用首选尺寸，`plan_kernel_mapping` 仍可成功 | 是，显式失败 |
| 实际架构的 DM 基址不按首选尺寸对齐 | 可能在首选策略下写非法 VA | 架构布局编译期断言失败；布局常量本身不对齐的 mock 架构在 `plan_dm_policy` 降级 | 是，静态不变量和降级路径分层 |
| Kernel DM 候选 PA 超出虚拟窗口容量 | 用 `u64::MAX` 充当无上界，可能继续尝试映射 | 返回 `InvalidAddress`，启动失败 | 是，边界显式化 |
| 物理地址超出系统有效 PA 上界 | 编码器掩码后静默截断 | 通用预检返回 `InvalidAddress`；架构编码器另行拒绝无法编码的高位 | 是，PA 边界显式化 |
| 布局偏离架构换算常量（基址上移、容量缩小、终点不同） | 策略按布局安装叶子，运行期按常量换算地址，两者分家 | `DirectMapLayout::validate()` 要求逐项相等，偏离即 `InvalidDmLayout` | 是，布局与换算一致 |
| 平台有效 PA 上界低于 4 GiB 恒等窗口 | 恒等叶子可能写出硬件可寻址范围且无检查 | `boot_validate_and_prepare` 校验 `phys_addr_limit >= IDENTITY_MAP_END`，拒绝启动 | 是，边界显式化 |
| 安装中途遇到页表项冲突 | 可能继续使用部分建立的映射 | 返回错误并终止启动，不继续使用部分映射 | 是，失败契约显式化 |
| x86-64 最大扩展 CPUID 叶不足 | 仍可能读取未定义的 GBPAGES 位 | 探针返回 false，回退 2 MiB | 是，探测契约补全 |
| 泛型策略返回策略副本 | 派生 `Copy` 可能要求 `A: Copy`，导致无法编译 | 手写 `Copy`/`Clone`，不给 `A` 加 bound | 否，编译期修复 |
| SMP 平台 CPU 页大小能力不同 | 无检测，AP 可能无法走共享页表 | 当前仅支持满足同构前提的平台；实现不主动检测或拒绝异构组合，违反同构前提的系统不在安全保证范围内 | 是，边界显式化 |

### 7.2 五类不变量

这次改动的测试围绕五类不变量组织，而不是围绕函数名组织。先摆清楚三条容易互相冒充的验收边界，代码评审与测试报告按行分开，不合并陈述：

| 验收项 | 能证明什么 | 不能据此宣称什么 |
|---|---|---|
| 全局预检与安装重放同一候选序列 | 两遍执行的输入集合一致 | 候选集合完整且正确 |
| 资源空洞不被吞入映射 | 映射生成没有把已识别空洞扩大成有效范围 | 所有需要访问的有效页都被映射 |
| `try_*` 范围检查成功 | 地址位于可换算边界内 | 对应 PTE 存在并可安全解引用 |

**第一类：能力事实**

- 每个架构显式实现 `supports_huge_page`，没有默认实现；
- x86-64 探针先检查 `CPUID.80000000H.EAX`，再读 GBPAGES；
- `detect_boot_page_support` 只读取能力，不读取内核布局；
- `BootPageSupport<A>` 对同一个页大小只有一份答案，不保存内核/DM 两份偏好；
- `BootPageSupport::validate()` 要求首选和回退尺寸都来自架构页大小集合，并且基础页小于回退、回退小于首选；
- `phys_addr_limit` 是平台有效 PA 上界，与 `PTE_PHYS_ADDR_BITS` 格式上限分开；RISC-V 不从内存图最高地址推导，ARM64 以当前 IPS 为准；
- 同一组能力事实被内核规划和 DM 规划共同消费；
- 测试只能通过 test-support 构造器注入事实，不依赖 `MockPaging` 的全局默认值。

**第二类：策略验证**

- 能力为假时，即使布局完全对齐，也不能选该尺寸；
- 回退尺寸必须受支持；
- 首选尺寸必须大于回退，回退必须大于基础页；不满足是静态配置错误；
- 首选尺寸不受支持或基址不满足首选对齐时，正常降级为 `None`；
- DM 基址必须按回退尺寸对齐；
- `DirectMapLayout::validate()` 在策略构造前检查布局与架构换算常量逐项相等、容量（含基础页对齐）、终点一致性、VA 上界、窗口互斥，以及两端点的 canonical 合法性（判据按架构逐端点判定，见 §6.8 的 `validate_virtual_range` 契约）；偏离常量的布局即使几何自洽也被拒绝；
- 由 `PageSize` 二次幂不变量保证的整除关系不再在策略层重复检查；
- 静态配置在 `BootPageSupport::validate()` 失败，而不是在写表时失败。

**第三类：区间与地址**

- 每个发射单位完整落在 `DmRange` 内；
- 相邻单位连续覆盖，不重叠、不遗漏；
- 空洞不会被大页吞掉；
- 回退尺寸和基础页大小来自策略，不来自隐藏常量；
- `VA = va_base + PA` 按尺寸对齐；
- `va_base + pa`、`pa + size`、`va + size`、`range.base + range.len` 不溢出；
- Kernel DM 不越过 `kernel_pa_capacity` 和 `kernel_va_end`，VM DM 不越过 `vm_pa_capacity` 和 `vm_va_end`；
- 每个叶子末端的物理地址不超过 `Paging::phys_addr_limit()`，架构编码器不能静默截断高位；
- Kernel DM 超出容量时失败，不静默裁剪；
- 同一窗口的候选物理区间两两不重叠（跨来源重叠由归一化整段丢弃；Kernel DM 与 VM DM 映射同一 PA 不算冲突）；
- `try_*` 换算（`Option` 返回）拒绝可换算上界 `min(窗口容量, phys_addr_limit)` 之外的 PA；范围版要求 `[pa, pa + len)` 完整落在上界内（零长度起点仍须可换算、`pa == 上界` 拒绝、末端恰等上界接受）；反向换算同样以可换算上界判定（窗口内且偏移小于上界），正反在可换算范围内往返互逆；检查换算的窗口常量经封口 `BootArch` 取得，上界来自启动期一次写入的运行期配置；`try_*` 成功只证明可换算，不证明 PTE 存在，实际可访问由来源证明链或显式映射检查承担；
- 范围端点和长度按基础页对齐；
- 所有叶子的几何检查在第一个页表写入之前完成。

**第四类：叶子安装**

- `DmPagePolicy.preferred_page_size == None` 时，安装器不产生首选大页叶；
- `preferred_page_size == Some(size)` 时，完整落在候选区间内的对齐段产生该尺寸叶；
- 多个窗口共用同一个 `DmPagePolicy<A>`，不会出现 Kernel DM 用 1 GiB、VM DM 用 2 MiB 的能力差异；
- 安装器本身不调用 `supports_huge_page`；
- `install_boot_dm_range` 的调用点都在显式的 `unsafe` 块内，并带启动上下文安全注释；
- 底层安装错误发生在部分叶子写入之后时，调用者必须终止启动，不能继续使用部分 Direct Map。

**第五类：启动集成**

- `arch_boot_impl` 使用 `plan_boot_pages`，同时规划内核映射和 DM；
- `arch_boot_resume_high_half` 使用 `plan_dm_policy`，只规划自身需要的 DM；
- 内核映射和 DM 共享 `BootPageSupport<A>`，但不强制共享页大小；
- `KernelMapPolicy<A>` 携带规划时验证过的布局，执行入口不再重新读取另一份布局；
- 当前 `KernelLayout` 只覆盖连续、统一权限的内核映像；如果未来出现段级权限或空洞，规划必须按真实映射片段拆分；
- `DmPagePolicy<A>` 携带验证过的 `DirectMapLayout<A>`，两个 DM 窗口从同一份布局生成；
- `boot_validate_and_prepare` 在建表前校验 `phys_addr_limit >= IDENTITY_MAP_END`；
- 全局预检与实际安装两遍遍历同一个候选生成器，发射序列逐项相同；
- `BootPageSupport<A>`、`DirectMapLayout<A>`、`DmPagePolicy<A>` 和 `DmWindow<A>` 的架构参数一致，跨架构组合无法编译；
- `CurrentPaging`、`CurrentDirectMap`、`CurrentBootDmInstaller` 都投影自 `CurrentBootArch`，没有第二套架构选择；
- `-smp4` 启动时所有 AP 能加载并走过 BSP 建立的页表；同构条件包含页大小、有效 PA 上界和翻译配置，而不只是页大小；
- 不支持的 1 GiB 平台在真实启动中不会写入 1 GiB 叶。

### 7.3 测试矩阵

| 测试 | 目的 | 形式 |
|---|---|---|
| 每个架构显式实现 `supports_huge_page` | 没有默认实现兜底 | 编译测试 + `rg` |
| `supports_huge_page(1 GiB)` 与私有探针一致 | x86-64 能力单源 | 架构实现测试 |
| 最大扩展叶 `< 0x80000001` 时 1 GiB 返回 false | CPUID 叶存在性 | 注入 `(max_extended, edx)` 的探针测试 |
| 最大扩展叶 `< 0x80000008` 时 `phys_addr_limit()` 返回错误且启动失败 | CPUID PA 位宽叶存在性 | 注入 max-extended 的探针测试 |
| 注入 `EAX[7:0]` 位宽为 0 或不小于 64 时 `phys_addr_limit()` 返回错误；位宽超过编码上限时上界取编码上限 | 结构性验证先于移位 + 统一取小规则 | 注入 `(max_extended, bits)` 的探针测试 |
| 三架构 `phys_addr_limit()` 不超过各自 PTE 编码上界 | 上界与编码能力闭合 | 架构实现测试/编译期断言 |
| `PageSize::try_as_usize` 对不可表示值返回 `None` | 不做静默截断 | 未来增加 32 位目标时的测试门（§10 第 10 项）；当前 64 位目标只覆盖合法值 |
| `PageSize::try_from_bytes` 拒绝 0、小于 4 KiB 和非 2 的幂 | 数值合法性 | `PageSize` 单元测试 |
| `BootPageSupport<A>` 不保存 kernel/DM 重复字段 | 事实去重 | 结构字段 `rg` + 构造测试 |
| `BootPageSupport<A>::validate` 拒绝回退不大于基础页、首选不大于回退、尺寸不在集合 | 静态配置边界 | 规划测试 |
| `BootPageSupport::from_parts` 在普通生产构建不可见 | 能力事实不可伪造 | test-support feature/编译失败测试 |
| 架构参数不匹配的 `BootPageSupport<A>`/`DirectMapLayout<A>` 无法组合 | 架构身份 | 编译失败测试 |
| `X86_64BootArch`/AArch64/RISC-V 的关联类型与各自架构类型一致 | 注册表一致性 | 类型相等编译测试 |
| `CurrentPaging`/`CurrentDirectMap`/`CurrentBootDmInstaller` 等于 `CurrentBootArch` 的关联类型 | 唯一架构选择点 | 类型相等测试 |
| `KernelMapPolicy` 保存规划时验证的 `KernelLayout`，执行入口使用该布局 | 规划与执行输入不漂移 | 纯函数测试 + 代码审查 |
| `DmPagePolicy` 保存验证过的 `DirectMapLayout`，窗口从策略布局生成 | 规划与执行输入不漂移 | 纯函数测试 + mock 窗口测试 |
| `DirectMapLayout` 容量为零（Kernel/VM 窗口各自一例）、容量不按基础页对齐、`base + cap` 与终点不一致、终点越过 `KERNEL_DM_VA_END_LIMIT` 或 VM 上界时被拒绝 | 布局验证边界 | 注入布局的 `plan_dm_policy` 测试 |
| `DirectMapLayout` 任一字段偏离架构换算常量（基址上移、容量改变、终点不同）被拒绝 | 布局与运行期地址换算一致 | 注入布局的 `DirectMapLayout::validate` 测试 |
| ARM64 当前 IPS=5 的有效 PA 上界为 48 位 | 当前格式与有效上限一致 | 架构实现测试 |
| RISC-V 的 `phys_addr_limit()` 来自平台配置（48 位），不从内存图最高地址推导 | 资源范围与可寻址范围分离 | 架构实现测试 |
| 首选可用但回退不受支持时，`plan_kernel_mapping` 成功，`plan_dm_policy`/`plan_boot_pages` 失败 | 各规划器只验证自己真正依赖的条件 | 纯函数测试 |
| `install_boot_dm_range` 拒绝不属于策略的窗口 | 策略与窗口绑定 | 注入错配窗口的 mock 测试 |
| `MockPaging` 关闭 1 GiB 后选 2 MiB | 降级路径 | 可配置 mock 能力 |
| 1 GiB 不支持但布局按 1 GiB 对齐 | 选页先看能力 | 注入 `BootPageSupport<A>` 的纯函数测试 |
| 1 GiB 支持且对齐 | 不回退过度 | 注入事实的纯函数测试 |
| 只有 2 MiB 对齐 | 布局约束 | 注入事实的纯函数测试 |
| 都不满足 | 错误可诊断 | 返回错误或 `#[should_panic]` |
| 2 MiB 回退不受支持 | Direct Map 显式启动失败；内核若用首选仍可规划 | `plan_dm_policy`/`plan_boot_pages` 测试 |
| 首选不大于回退，或回退不大于基础页 | 静态配置校验 | `BootPageSupport::validate()` 测试 |
| 首选或回退尺寸不在 `HUGE_PAGE_SIZES` | 静态配置校验 | `BootPageSupport::validate()` 测试 |
| 范围端点或长度不按基础页对齐 | 写表前几何预检 | mock 安装器记录零叶子 |
| `plan_range_units` 直接收到未对齐范围 | 规划函数自包含输入契约 | 纯函数测试，回调零次 |
| `plan_range_units` 收到对齐的空区间 | 空区间无操作 | 返回 `Ok`，回调零次 |
| `plan_range_units` 收到未对齐的空区间 | 空区间也检查起点对齐 | 返回 `InvalidAddress` |
| 范围终点使末尾叶子越界 | 写表前几何预检 | mock 安装器记录零叶子 |
| VM DM 的几何错误出现在 Kernel DM 之后 | 跨窗口预检先于第一张叶子 | mock 安装器记录零叶子 |
| `install_leaf` 中途返回 `AlreadyMapped` | 失败后不得继续 | mock 安装器 + 启动失败契约测试 |
| `Kernel DM` 末尾小于一个首选/回退叶的尾部按基础页处理 | 不因尾部小页越界或漏映射 | mock 安装器 |
| 纯规划函数拒绝静态配置错误，不等待第二个规划入口 | 统一验证边界 | `plan_kernel_mapping`/`plan_dm_policy` 测试 |
| DM 基址不按首选尺寸对齐（mock 架构常量本身不对齐） | 虚拟地址对齐降级 | 布局常量不对齐的 `MockBootArch` 纯函数测试 |
| `install_boot_dm_range(preferred = None)` 不产生首选叶 | 策略与机制分离 | mock 安装器 |
| `install_boot_dm_range(preferred = Some(1 GiB))` 对完整候选产生 1 GiB 叶 | 策略传入生效 | mock 安装器 |
| 两个窗口共用同一个策略 | 防止窗口之间不一致 | mock 安装器记录参数 |
| `detect_boot_page_support` 每次启动只调用一次 | 能力事实共享 | 记录型 `HugePages` 实现 |
| 三个架构的 DM 基址断言 | 按架构首选页大小对齐，且窗口终点不越过 `KERNEL_DM_VA_END_LIMIT` | 编译期断言 |
| `IDENTITY_MAP_END` 能同时被首选和回退尺寸整除 | 身份映射尾部不产生半个叶子 | 编译期断言 |
| `map_huge` 不支持尺寸返回 `NotSupported` 且不写表 | 能力防御先于写表 | mock 页表或记录型实现 |
| `map_huge` 未对齐返回 `InvalidAddress` | 错误分类 | 架构实现测试 |
| 架构内部的 PTE 编码器遇到无法编码的高位 | 编码格式边界 | 架构实现测试，确认零写入 |
| `map_huge` 的物理地址越过 `phys_addr_limit` | 有效 PA 上界 | 架构实现测试，确认零写入 |
| `map_huge` 的 VA 终点溢出或区间不合法 | 完整 VA 区间检查先于分配 | 架构实现测试，确认零分配/零写入 |
| `validate_virtual_range` 在 `enable()` 前后调用结果一致 | 规划按声明模式，不读活动寄存器 | 架构实现测试 + `enable()` 后断言 |
| riscv64 `validate_virtual_range` 拒绝历史事故值 `0xFFFF_FC00_0000_0000`（Sv39 非规范：载荷 0 落在不存在的顶级槽位），接受规范值 `0xFFFF_FFC0_4000_0000`；x86-64 与 ARM64 各自的 canonical 判据同法各测一例 | canonical 判据逐架构落实，事故值钉死成回归门 | 架构实现测试 |
| `map_huge` 的 PA 终点溢出 | 完整 PA 区间检查先于分配 | 架构实现测试，确认零分配/零写入 |
| `BootPageSupport::validate` 拒绝 `phys_addr_limit == 0` | 能力快照有效性 | 纯函数测试 |
| `install_boot_dm_range` 的叶子越过策略 `phys_addr_limit` | 安装前 PA 预检 | mock 安装器记录零叶子 |
| 内核镜像物理末端越过 `phys_addr_limit` | 内核布局与平台 PA 上界 | `plan_kernel_mapping` 测试 |
| 注入 `phys_addr_limit < IDENTITY_MAP_END` 的能力事实 | 恒等映射窗口 PA 上界 | `boot_validate_and_prepare` 校验测试 |
| 全局预检与实际安装的候选发射序列逐项相同 | 两遍遍历确定性 | mock 安装器记录两遍序列 |
| 跨来源候选重叠：高优先级保留、低优先级整段丢弃、无 `AlreadyMapped` | 集合归一化 | 合成 `KernelInfo` 的生成器测试 |
| 候选集合不吞资源空洞、高优先级候选完整保留 | 现行归一化不变量（较弱的承诺） | 合成 `KernelInfo` 的生成器测试 |
| 被整段丢弃的低优先级尾部不含内核/启动需经 DM 访问的页 | 覆盖损失定界（独立立项的验收项，闭合前不作为现行断言） | 区间差集归一化实现 + 真机证明 |
| 内核虚拟区间不合法 | 内核布局 VA 合法性 | `Paging::validate_virtual_range` + `plan_kernel_mapping` 测试 |
| mock 架构的 DM VA 区间不合法 | 架构 VA 边界 | mock 架构 + `DirectMapLayout::validate` 测试 |
| Kernel DM 与 VM DM 两个合法区间互相重叠（mock 架构常量重叠） | 窗口互斥不变量 | mock 架构 + `DirectMapLayout::validate` 测试 |
| 每个架构私有 PTE 格式上限满足 12 ≤ bits < 64（半开区间，不含 64） | 移位与编码前提 | 架构内部编译期断言 |
| `plan_range_units` 不越界、不重叠、不遗漏 | 区间正确性 | 纯函数属性测试 |
| `install_boot_dm_range` 拒绝 VA/PA 溢出和越界 | 地址边界 | mock 安装器 |
| `try_*_phys_to_virt` 对可换算上界之外的 PA 返回 `None`，之内与 `_unchecked` 结果一致 | 可换算上界 | BootArch 绑定层测试 |
| `try_*_region` 接受末端恰等于上界与零长度（起点可换算），拒绝起点等于上界与 `pa + len` 溢出/越界 | 范围换算边界规则 | BootArch 绑定层测试 |
| `try_*_virt_to_phys` 对窗口外 VA 或偏移越出可换算上界返回 `None`；范围版要求 VA 区间与对应 PA 区间都完整落在边界内，零长度/末端规则与正向一致 | 反向换算边界与正反对称 | BootArch 绑定层测试 |
| 可换算范围内正反往返换算互逆；越过 `phys_addr_limit` 的输入正反两向都返回 `None` | 正反对称验收 | BootArch 绑定层测试 |
| 检查换算的窗口常量取自当前构建目标，上界读启动期一次写入的运行期配置；mock 架构经显式参数注入窗口/上界，不读写全局配置 | 缓存与架构身份一致 | 接口测试 + mock 注入测试 |
| PA 上界配置二次初始化或不同值再写触发启动契约违约 panic；未初始化读取返回 `None` | 一次写入语义 | 单元测试 |
| riscv64 Kernel DM 容量常量 = 基址到 `KERNEL_DM_VA_END_LIMIT` 的距离（约 255 GiB），与现行 `u64::MAX` 窗口的 VA 可表示边界一致；超界候选从静默回绕变为响亮失败 | 容量常量与现行行为对账 | 编译期断言 + 单元测试 |
| DM 别名指针形成与内存访问处（`phys_to_ptr`、拷贝路径）持有第三层证明 | unsafe 边界落在内存访问处 | 代码审查 + `rg` |
| DM 别名守卫三条全查：翻译存在、walk 返回 PA 与预期逐页一致、权限满足访问方向（现状只查存在性，目标补齐后两条） | 第三层检查完整语义 | mock walk 注入错 PA/缺权限的守卫测试 |
| ARM64 `PARange < 0b101` 时拒绝启动；`phys_addr_limit()` 为 `1 << 48` 仅在前提通过后成立 | ARM64 平台能力前提 | 注入 PARange 的启动校验测试 |
| 消息缓冲直写循环保持逐页 walk + 页界 chunk；`try_kernel_region` 只应用于单页 chunk | 物理连续性不引入整缓冲区假设 | 代码审查 + 跨页消息缓冲 mock 测试 |
| `_unchecked` 在 debug/release 下行为一致（wrapping 语义） | 纯算术语义无构建模式分叉 | 单元测试 |
| 运行期上界配置由启动校验后一次写入，检查换算不逐次重查 CPUID | 热路径上界来源 | 代码审查 + mock 配置注入测试 |
| `BootArch`/`BootDmInstaller` 的外部实现无法编译 | 架构注册封口 | 编译失败测试 |
| Kernel DM 候选 PA 超过 `kernel_pa_capacity` 时失败 | 有限窗口 | 注入超过容量的 `DmRange` + mock 安装器 |
| `install_boot_dm_range` 的调用点都带 `unsafe` 块 | 启动上下文契约 | 代码审查 + `rg` |
| 恢复入口只规划 DM | 内核布局不阻塞恢复 | 注入不满足 1 GiB 的 kernel layout + 对齐 DM |
| `-smp4` 真实启动 | AP 共享页表 | QEMU 真机门；只覆盖同构 |
| 禁用 GBPAGES 的 CPU 模型或能力注入 | 真实降级路径 | QEMU / 纯函数组合 |
| 重命名后旧标识符清零 | 机械迁移完整 | `cargo test` + `rg` |

其中“启动选页”的测试不需要构造完整页表环境，也不碰全局分配器。`plan_kernel_mapping`、`plan_dm_policy`、`plan_boot_pages` 都应该是纯函数。

### 7.4 测试替身的要求

`MockPaging` 不能再无条件返回“支持 1 GiB”。能力方法接收 `PageSize`，测试开关也应使用 `PageSize`：

```rust
impl HugePages for MockPaging {
    fn supports_huge_page(size: PageSize) -> bool {
        // 默认保持现状；降级测试显式关闭 1 GiB
    }
}
```

如果使用全局静态开关，降级测试必须持有现有启动全局锁，测试结束恢复默认值。更好的做法是让需要不同能力的测试直接注入 `BootPageSupport<A>`，避免共享状态。`minix-arch` 需要提供一个 `test-support` feature，只在测试构建中暴露 `BootPageSupport::from_parts` 和 `DirectMapLayout::from_parts`；内核测试通过 dev-dependency 启用它，生产构建不启用。

`MockBootDmInstaller<A::Paging>` 只记录叶子，不查询能力。它接收 `DmPagePolicy<A>` 和窗口；由于 `DmPagePolicy` 已保存 `DirectMapLayout<A>`，测试不需要再把另一份布局传进来。测试用的 `MockBootArch` 必须把 `Paging`、`DirectMap` 和 `DmInstaller` 绑到同一套 mock 类型上，不能只 mock 策略；需要驱动“首选对齐不满足”的降级分支时，让 mock 架构的 `DirectMapArch` 常量本身相对其首选页大小不对齐，而不是伪造偏离真实架构的布局。

### 7.5 非目标

这次改动不做以下事情：

- 不改变 Direct Map 的虚拟地址布局；
- 不改变 Kernel DM 与 VM DM 的建立顺序；
- 不改变候选区间并集、洞排除、窗口裁剪算法；
- 不改变 VM 交接协议；
- 不新增运行期 1 GiB 页分配器；
- 不新增通用 `HugePageCaps`/`PageSizeSupport` 能力 trait；
- 不把 2 MiB、1 GiB 的能力策略推广到所有调用方；
- 不为了统一而重写 x86-64 的恒等映射拆分逻辑；
- 不支持异构页大小能力的 SMP 平台。

## 8. 迁移步骤

迁移顺序的原则是：**先修行为，再统一接口，最后改名**。当前启动选页有实际正确性问题；如果先做机械重命名，会把行为差异埋在大面积改名噪音里，评审和回归都更难定位。

### 第一阶段：修复启动选页

**目标**：让启动选页先看能力，再看布局；暂不改变 trait 结构。

**内容**：

- 在 `os/kernel/src/lib.rs` 提取临时纯函数 `choose_kernel_page_size`；
- `boot_validate_and_prepare` 调用它，暂时使用现有的 `P::supports_1gb_page()` 作为 1 GiB 能力输入；
- 对“1 GiB 不支持但布局按 1 GiB 对齐”添加回归测试；
- 对“支持且对齐”“只按 2 MiB 对齐”“都不满足”补测试；
- 不改 trait 名、不改安装器名、不改 `establish_dm_range` 签名。

**验证**：

- 新增选页测试全部通过；
- `cargo test -p minix-kernel --features mock`；
- x86-64 QEMU 常规启动；
- 行为变化只发生在“不支持 1 GiB 但按 1 GiB 对齐”的平台。

**风险**：中。它改变启动路径的实际页大小选择，必须用真实启动确认。

### 第二阶段：引入 `PageSize` 并统一能力 API

**目标**：删除两个 `supports_1gb_page` 公开入口和 `supports_huge_page` 默认实现，让每个架构显式承担能力契约。

**内容**：

- 在 `os/arch/src/arch/paging_ext.rs` 增加 `PageSize`，把 `HUGE_PAGE_SIZES`、`HUGE_PAGE_SIZE`、`FALLBACK_HUGE_PAGE_SIZE` 和 `map_huge` 的参数改为 `PageSize`；
- `PageSize` 提供 `try_from_bytes`、`try_from_usize` 和 `try_as_usize`；禁止新增 `as_usize` 这类静默截断入口；
- 删除 `HugePages::supports_1gb_page` 的默认方法和 x86-64 覆盖；
- 删除 `DmCoverageArch::supports_1gb_page`；
- `HugePages::supports_huge_page(PageSize)` 改为无默认实现；x86-64、ARM64、RISC-V 都显式实现；
- x86-64 在 `supports_huge_page` 内部调用私有 `probe_1gb_page_support()`；探针先检查 `CPUID.80000000H.EAX >= 0x80000001`，再读 GBPAGES；
- 启动选页改用 `P::supports_huge_page(PageSize::GIB_1)` 和 `P::supports_huge_page(PageSize::MIB_2)`；
- `establish_dm_range` 暂时保留现有的 `one_gb: bool` 参数；`establish_boot_dm` 临时从 `CurrentPaging::supports_huge_page(PageSize::GIB_1)` 取一次能力，传给所有窗口；
- `MockPaging` 增加可配置能力；
- 删除“两个 trait 能力面一致”的测试，改成 x86-64 内部一致性测试；
- 更新 `map_huge` 调用点，传入 `PageSize`。

**验证**：

- `rg -n 'supports_1gb_page' os` 只剩历史注释或文档，不再有生产代码命中；
- `rg -n 'fn supports_huge_page' os/arch/src` 三个架构都有显式实现；
- x86-64 的 `supports_huge_page(PageSize::GIB_1)` 真正受 CPUID 控制；
- 最大扩展叶不足时探针返回 false，且不会读取未定义的 GBPAGES 位；
- ARM64 和 RISC-V 的显式实现返回各自当前翻译模式支持的结果；
- 启动选页和 DM 入口使用同一个查询；
- 宿主测试覆盖 1 GiB 关闭路径。

**风险**：中低。支持 1 GiB 的机器行为不变；不支持 1 GiB 的机器得到修复。`PageSize` 会带来一次较大范围的机械参数替换，但类型变化是编译期可验证的。

### 第三阶段：拆分能力事实与纯规划

**目标**：把“获取能力事实”和“规划启动策略”拆开，消除裸 `one_gb`、隐藏的 4096/2 MiB 和跨架构策略误用。

**内容**：

- 在 `os/arch/src/arch/boot.rs`（建议新增）放 `BootArch`；`os/arch/src/lib.rs` 增加 `CurrentBootArch`，并让 `CurrentPaging`/`CurrentDirectMap`/`CurrentBootDmInstaller` 全部投影自它；每个架构加关联类型一致性测试；
- 在 `os/arch/src/arch/paging_ext.rs` 增加 `PageSupport`、`BootPageSupport<A>`、`detect_boot_page_support<A>` 和 `BootPageSupport::validate`；`from_parts` 只在 `test-support` feature 下可见；删除按内核/DM 分别保存的重复能力字段；
- 在 `os/kernel/src/boot_policy.rs`（建议新增）放 `KernelLayout`、`KernelMapPolicy<A>`、`BootPagePolicy<A>`、`plan_kernel_mapping`、`plan_boot_pages`；`KernelMapPolicy` 保存已验证的 `KernelLayout`；
- 在 `os/arch/src/arch/direct_map.rs` 放 `DirectMapLayout<A>`、`KERNEL_DIRECT_MAP_SIZE`、`KERNEL_DM_VA_END_LIMIT` 和静态断言；在 `os/arch/src/arch/dm_coverage.rs` 放 `DmPagePolicy<A>`、`DmWindow<A>`、`plan_dm_policy`、`plan_range_units`；`DmPagePolicy` 保存已验证的 `DirectMapLayout<A>`；
- `boot_validate_and_prepare` 返回 `BootPageSupport<A>`，只负责校验、分配器注册、能力检测和 `phys_addr_limit >= IDENTITY_MAP_END` 恒等窗口校验；
- `arch_boot_impl` 调用 `plan_boot_pages`，把 `KernelMapPolicy<A>` 传给 `build_bootstrap_root_and_enable`，把 `DmPagePolicy<A>` 传给 `establish_boot_dm`；前者从策略取布局和页大小，后者只从策略取布局，不再接收另一份 `DirectMapLayout<A>`；
- `build_bootstrap_root_and_enable` 改收 `root_page` 和 `&KernelMapPolicy<A>`，删除裸 `kern_huge`；`establish_boot_dm` 改收 `kernel_info`、`root` 和 `&DmPagePolicy<A>`，返回可传播的安装错误；
- `arch_boot_resume_high_half` 只调用 `plan_dm_policy`，不因内核镜像布局不满足大页对齐而失败；
- `establish_dm_range`（第五阶段后改名 `install_boot_dm_range`）接收 `DmPagePolicy<A>` 和 `DmWindow<A>`，删除裸 `one_gb: bool`；入口改为 `unsafe`，并更新所有调用点；
- `plan_range_units` 从策略读取 `preferred_page_size`、`fallback_page_size`、`base_page_size`，删除 1 GiB 特殊分支；终点使用 `checked_add`；
- `install_boot_dm_range` 在第一次写表前完成范围端点/长度、所有叶子 VA/PA、窗口边界和溢出预检；`establish_boot_dm` 在写第一张叶子前对所有 Kernel DM/VM DM 候选调用同一个 `preflight_boot_dm_range`，随后走私有的 `install_prevalidated_boot_dm_range`，避免逐区间重复预检；底层安装失败后的契约写成“启动必须终止”；
- Kernel DM 使用 `KERNEL_DIRECT_MAP_SIZE` 计算有限窗口，并用项目配置的 `KERNEL_DM_VA_END_LIMIT` 限制窗口终点，同时确认它不超过当前 MMU 模式的合法范围；
- `Paging` 只增加语义化的 `phys_addr_limit()` 和 `validate_virtual_range()`；PTE 格式上限保留在架构内部（x86 52、ARM64 当前 48、RISC-V 56）。`phys_addr_limit()` 的来源是 x86 CPUID（先查最大扩展叶）、ARM64 当前 TCR_EL1.IPS、RISC-V 平台配置的保守上界（当前 48 位，不读内存图最高地址）；`map_huge` 和安装器预检只依赖语义上界；
- `DirectMapArch` 换算重整（§6.11）：无检查加法改名 `kernel/vm_phys_to_virt_unchecked` 并补反向 `*_virt_to_phys_unchecked`（安全纯算术，wrapping 语义）；检查换算放在 `BootArch` 绑定层且运行期入口绑定当前构建目标（非泛型），正反向共用可换算上界 `min(窗口容量, 已验证 phys_addr_limit)`，`Option` 返回，上界来自启动校验后一次写入的运行期配置（`boot_validate_and_prepare` 保存；二次写入违约 panic；判定本体是显式参数的纯函数，mock 注入不碰全局）；通用 `virt_to_phys` 删除——唯一生产活调用（VM 内部包装 `os/servers/vm/src/direct_map.rs:fn virt_to_phys`）迁到显式窗口版本；`os/kernel/src/dm_coverage.rs` 模块注释的 riscv64「16 GiB」讹误随迁移订正为 255 GiB；`os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl` 这类直接解引用调用点改用 `try_kernel_region` 并保留显式映射检查（`physical_range_in_dm_window` 的逐页 walk；`vm_map_phys` 设备映射是证明链外例外），安全注释理由改为来源证明链的适用范围；IPC/跨空间路径保留既有逐 chunk 守卫或改用 `try_*`；目标布局/窗口字段的 `pa_limit` 命名改为 `pa_capacity`（现有 `establish_dm_range` 的 `window_pa_limit` 参数随迁移机械改名）；
- 每个架构增加 DM 基址按首选页大小对齐、Kernel DM 容量可表示且不越过 `KERNEL_DM_VA_END_LIMIT`、基础页 4 KiB、回退 2 MiB 的编译期断言；
- 在 kernel 启动规划模块增加 `IDENTITY_MAP_END` 同时被首选和回退尺寸整除的编译期断言；
- 补 `DirectMapLayout::validate` 的容量基础页对齐/终点/上界/偏离换算常量拒绝测试、`plan_kernel_mapping` 的条件回退测试、`plan_dm_policy` 的硬回退测试、x86 CPUID 叶存在性测试、ARM64 当前 48 位测试（含 `PARange >= 0b101` 前提的注入测试）和 RISC-V 平台 PA 上界测试，以及策略构造、事实去重、窗口共用、布局随策略传播、DM 基址对齐、Kernel DM 容量、地址溢出、写表前预检、失败终止、恒等映射 PA 上界校验和两遍遍历序列一致测试。

**验证**：

- `arch_boot_impl` 只调用一次 `plan_boot_pages`，`arch_boot_resume_high_half` 只调用一次 `plan_dm_policy`；
- `boot_validate_and_prepare` 在进入 `build_bootstrap_root_and_enable` 前已完成 `phys_addr_limit >= IDENTITY_MAP_END` 校验；
- `os/kernel/src` 每处换算调用点都可归类为 `_unchecked`（注释写明依据）、带守卫或 `try_*`；DM 别名指针解引用处持有第三层证明（代码审查清单，§6.11 的分类表）；
- `rg -n 'fn virt_to_phys' os/arch os/servers` 生产代码只剩显式窗口版本，通用入口零命中；运行期检查换算不逐次重查 CPUID；
- `rg -n 'supports_1gb_page|supports_huge_page\(1 << 30\)' os` 只允许出现在历史注释或测试；
- `rg -n 'plan_range_units\(' os` 不再出现裸 `4096`；
- `establish_dm_range` 的参数列表里没有 `one_gb: bool`；
- `rg -n 'u64::MAX' os/arch/src/arch/dm_coverage.rs` 不再出现在 Kernel DM 窗口；
- `KernelMapPolicy` 的执行路径使用 `policy.layout()`，`DmPagePolicy` 的窗口使用 `policy.layout()`；
- Kernel DM 与 VM DM 的 mock 叶子使用同一个 `DmPagePolicy<A>`；
- `BootPageSupport<A>`、`DirectMapLayout<A>`、`DmPagePolicy<A>`、`DmWindow<A>` 都带同一个 `A`，跨架构误用会编译失败；
- `CurrentPaging`、`CurrentDirectMap`、`CurrentBootDmInstaller` 都由 `CurrentBootArch` 投影；
- `BootPageSupport::from_parts` 在普通生产构建不可见；
- `install_boot_dm_range` 和 `establish_boot_dm` 的调用点都在 `unsafe` 块内；
- 范围几何错误不会留下任何新叶子；底层安装失败按契约终止启动；
- `-smp4` 启动通过。

**风险**：中。调用链变长，但行为应保持不变；主要风险是漏传策略、恢复路径绕过策略，或 DM 基址/VA 上界断言发现现有布局不满足架构首选页大小或当前 MMU 模式。

### 第四阶段：补 `map_huge` 防御和 SMP 启动验证

**目标**：让绕过选页的调用也安全，并验证共享页表在 AP 上可用。

**内容**：

- 每个 `map_huge` 在写表前检查 `supports_huge_page`；
- 不支持尺寸返回 `NotSupported`；
- 如一并补齐对齐检查，未对齐返回 `InvalidAddress`；
- 增加“能力错误不写表”的测试；
- 确认所有 `install_boot_dm_range` 和 `establish_boot_dm` 调用都在 `unsafe` 块内，且安全注释说明 bootstrap root、恒等映射和单线程前提；
- 补写表前几何预检测试：范围端点/长度不对齐、末尾越界、VA/PA 溢出、PA 超出有效 `phys_addr_limit` 都记录零个叶子；
- 增加 Kernel DM 超出 `KERNEL_DIRECT_MAP_SIZE` 的失败测试，并确认失败发生在写表之前；
- 补一个底层 `install_leaf` 中途返回冲突的 mock 测试，确认调用者按契约终止启动，不继续使用部分映射；
- 增加 `-smp4` 启动测试，确认所有 AP 能加载 BSP 的页表根；
- 如果测试环境允许，增加禁用 GBPAGES 的 QEMU CPU 模型测试。

**验证**：

- 不支持尺寸的 `map_huge` 调用没有留下新页表项；
- Kernel DM 超容量路径返回错误且没有写叶子；
- 范围几何错误路径没有写任何叶子；
- 物理地址超出有效 `phys_addr_limit` 的路径没有写任何叶子；
- 底层安装失败的路径被启动入口视为致命错误；
- `rg -n 'unsafe.*install_boot_dm_range|unsafe.*establish_boot_dm' os` 能覆盖全部调用点；
- `-smp4` 启动无 AP 页错误；
- 降级路径在能力注入测试和真实启动中至少各覆盖一次。

**风险**：低到中。检查发生在入口，正常路径行为不变；SMP 验证依赖 QEMU 配置。

### 第五阶段：机械重命名

**目标**：只改名字，不改行为。

**内容**：

- `DmCoverageArch` → `BootDmInstaller`
- `dm_install_leaf` → `install_leaf`
- `establish_dm_range` → `install_boot_dm_range`
- `CurrentDmCoverage` → `CurrentBootDmInstaller`
- `X86_64DmCoverage`、`AArch64DmCoverage`、`Riscv64DmCoverage`、`MockDmCoverage` 跟随改名
- `arch/dm_coverage.rs` → `arch/boot_dm.rs`

**验证**：

- `cargo test`；
- `rg -n 'DmCoverage|dm_install_leaf|establish_dm_range' os` 只允许出现在明确保留的历史说明中；
- 真机启动门至少跑一次，确认只是改名。

**风险**：低。主要是跨文件引用和注释。

### 为什么不是先重命名

先重命名会把“启动选页没有检查能力”这个行为修复藏进大面积符号变更里。审查者很难从 diff 中区分“名字变了”和“页大小变了”；回归失败时也更难定位。

先修行为、再统一 API、最后改名，可以让每个阶段只有一类变化：

| 阶段 | 主要变化 | 行为是否变化 |
|---|---|---|
| 第一阶段 | 启动选页 | 是，只修错误路径 |
| 第二阶段 | 能力 API | 是，只修错误答案 |
| 第三阶段 | 策略对象 | 否，行为保持 |
| 第四阶段 | 防御检查 + SMP 验证 | 正常路径否，错误路径更安全 |
| 第五阶段 | 重命名 | 否 |

## 9. 外部系统对照

### 9.1 Linux

Linux 把“CPU 是否支持 1 GiB 页”放在 CPU 特性位里，例如 `X86_FEATURE_GBPAGES`，启动时通过 `boot_cpu_has` 查询。Direct Map、HugeTLB、透明大页是不同消费者，但它们共享同一个 CPU 特性事实，不各自定义一套能力查询。

这对应本文的原则：**一个硬件事实，一个来源；不同消费者各自决定策略**。Linux 也不会在每个映射调用里重新定义“1 GiB 能力”。

### 9.2 Redox

Redox 的 `rmm` 把页表映射器和物理帧分配器分开，映射器按表类型参数化。内核线性映射是映射器的一种使用方式，不是另一套“启动期能力接口”。这与本文“安装机制按生命周期分开，但能力不跟着分裂”的方向一致。

### 9.3 seL4

seL4 把页帧按大小做成不同类型的对象（4 KiB、2 MiB、1 GiB 等），能力查询与对象类型绑定。它强调的是“页大小是对象的属性”，而不是“页大小是某个生命周期阶段的属性”。这支持本文把能力放回 `HugePages`、不放在 `BootDmInstaller` 的选择。

### 9.4 Mach

Mach 的 `pmap` 层提供硬件页表机制，VM 层决定映射策略。`pmap` 不负责回答“现在是启动还是运行”，它只负责“给定根和页大小，怎么操作页表”。这与本文把策略放在调用方、机制放在实现方的分工一致。

这些系统不需要照搬。它们共同说明一件事：**启动期与运行期的差异应该体现在机制的安全前提上，而不是体现在硬件能力的定义上。**

## 10. 开放问题

以下问题适合在冻结前继续讨论。已经确定的设计包括：`install_boot_dm_range` 采用 `unsafe` 启动契约；架构身份统一由 `BootArch` 绑定；能力事实按页大小去重；Kernel DM 使用有限容量；策略 `Copy`/`Clone` 手写；`BootPageSupport` 的生产构造收窄到 `detect_boot_page_support`；静态页大小配置由 `validate()` 统一验证；`DirectMapLayout::validate()` 与架构换算常量逐项相等；恒等映射窗口在建表前通过 `phys_addr_limit >= IDENTITY_MAP_END` 检查；错误类型按 crate 分层；全局预检与安装两遍遍历同一个候选生成器；运行期换算按三层保证分层，`try_*` 返回 `Option` 并检查可换算上界 `min(窗口容量, phys_addr_limit)`、正反两向对称、快速版为安全纯算术 `_unchecked`，反向检查版成对；`phys_addr_limit()` 是硬件事实查询或「能力标识前提 + 声明配置值」（ARM64 以 `ID_AA64MMFR0_EL1.PARange >= 0b101` 为启动前提；能力标识寄存器是只读事实，不属活动控制寄存器）、`validate_virtual_range()` 才是纯验证，上界统一按 `min(报告上界, 编码上限)` 取值；`BootArch`/`BootDmInstaller` 封口且封口只防外部实现、不防对公开低层方法的直接调用；窗口容量字段命名用 `pa_capacity` 不用 `limit`，与 `phys_addr_limit` 区分；unsafe 边界放在 DM 别名指针形成与内存访问处；通用 `virt_to_phys` 删除（唯一生产活调用随迁移改显式窗口版）；检查换算的运行期入口绑定当前构建目标，上界来自启动期一次写入的运行期配置（判定本体是显式参数的纯函数，mock 注入不碰全局；两个启动入口按启动协议互斥，二次初始化 panic，未初始化读 `None`），`None` 统一表示无法证明可换算；riscv64 Kernel DM 容量按基址到 VA 顶取约 255 GiB，`dm_coverage.rs` 模块注释的「16 GiB」是讹误、迁移时订正；消息缓冲路径保留第三层检查并升级为三条全查（翻译存在 + walk PA 逐页一致 + 权限；现状只查存在性；`vm_map_phys` 设备映射是证明链外例外；直写循环保持逐页 walk + 页界 chunk，不引入整缓冲区连续 PA 假设）；布局验证返回布局层自己的 `DmLayoutError`，规划层负责转换。

**已关闭的支持边界**：本次明确只支持页大小能力、有效 PA 上界和翻译配置同构的 SMP 平台，不在启动时收集所有 CPU 能力或做交集协商；AP 在共享大页页表建立后才启动，事后检查不足以补救。异构平台如果将来要支持，必须作为独立设计，在建立共享页表之前完成能力收集；`-smp4` 只作为同构集成测试门，不证明异构安全。

1. **`test-support` feature 如何打包？**
   设计已经确定生产构建只允许 `detect_boot_page_support` 构造 `BootPageSupport<A>`，`from_parts` 只在 `#[cfg(test)]` 或 `test-support` feature 下可见。剩余问题只是 feature 名称、`minix-arch` 的 feature 声明和 kernel dev-dependency 的配置方式；这不影响能力事实不可伪造的结论。

2. **`DmPagePolicy<A>` 的构造边界放在哪里？**
   本文要求只能由 `plan_dm_policy` 构造，字段私有。这样能防止生产代码随意构造 `preferred_page_size = Some(1 GiB)`；测试通过注入 `BootPageSupport<A>` 和测试用的 `DirectMapLayout<A>` 构造间接覆盖。是否还需要一个只读的 `from_validated_parts` 给跨 crate 测试，需要确认；无论选哪种，不能把策略构造器暴露给生产代码。

3. **2 MiB 回退是否永远是三种架构的硬性前提？**
   当前是，且这是支持策略边界，不是技术必然：三种受支持架构的安装器都能安装基础页叶子（§6.6 已逐架构对账），仅靠基础页建表在技术上可行；把它写成硬前提，是不为缺少 2 MiB 大页的平台维护第二条建表与测试路径。本文要求 `plan_dm_policy` 在回退不受支持时启动前失败；`plan_kernel_mapping` 只有在首选尺寸也不可用、确实需要回退时才失败。完整启动仍会因 `plan_boot_pages` 中的 DM 规划失败而拒绝。如果未来要支持没有 2 MiB 大页的架构，应作为独立设计重新定义“回退”档位，而不是放宽这条前提。

4. **`supports_huge_page` 是否应该改名？**
   如果要强调“当前实现能力”，可以叫 `supports_page_size`；如果要强调“大页”，保留现名也可以。关键是语义要写清楚，不能只查静态列表。

5. **`map_huge` 的 CPUID 结果是否需要缓存？**
   当前启动路径调用次数很少，不需要提前优化。如果未来运行期频繁查询，可以在 x86-64 内部缓存，但不能改变系统级语义。

6. **`BootDmInstaller` 是否应该封口？（已决定：封口）**
   与 `BootArch` 一同封口（§6.1）：可实现的集合限定在 `minix-arch` 内部，mock 安装器经 `test-support` 提供。封口控制的是**实现集合**——安装器实现经过项目维护者审查；它不保证生产代码只能经 `install_boot_dm_range` 安装叶子：`install_leaf` 仍是公开 `unsafe` 方法，直接调用由调用点审查、搜索检查与测试维持。

7. **`KernelLayout` 是否应该从 `KernelInfo` 直接构造？**
   模块归属已经确定为 `os/kernel/src/boot_policy.rs`，但 `KernelLayout` 的构造入口还没有定：是让调用方拆字段，还是提供 `KernelLayout::from_info(&KernelInfo)` 并把校验留在 `plan_kernel_mapping`。需要确认哪种方式更容易测试，且不会把 `KernelInfo` 的字段语义复制到规划层。

8. **Kernel DM 的 VA 窗口与当前 MMU 模式（已确认，模式变更时重做）**
   当前 x86-64 使用 4 级 PML4，ARM64 使用 48 位 VA/TTBR1，RISC-V 使用 Sv39；§6.6 的 `KERNEL_DM_VA_END_LIMIT` 是项目为 Kernel DM 选择的上界，已确认落在这些模式的合法范围内，并用它断言窗口终点。如果未来切换到 5 级页表或 Sv48，必须重新计算 base、size 和 limit，不能沿用当前常量。

9. **物理地址编码上限与有效上界（已决定：路线 A + 统一取小）**
   架构内部的 PTE 格式上限只约束编码器（x86-64 52，ARM64 当前 48，RISC-V Sv39 56）；通用层只依赖 `Paging::phys_addr_limit()`。x86-64 从 `CPUID.80000008H` 读取（先确认最大扩展叶，叶不可用则启动失败），ARM64 按声明的目标翻译配置取值（IPS=5，即 48 位；`enable()` 写入同一配置并做写后断言），RISC-V 使用平台配置声明的保守上界（当前 `1 << 48`），不从内存图最高地址推导。上界统一按 `min(报告上界, 编码上限)` 取值（§6.8）：报告值超过编码上限时取编码上限，只有结构性非法（零位宽、移位不可安全、叶缺失）才失败。`map_huge` 和安装器预检只检查语义上界；架构编码器自己拒绝无法编码的高位。

10. **`PageSize` 的 `usize` 转换测试是否需要覆盖 32 位目标？**
    当前三种目标架构都是 64 位，`PageSize::GIB_1.try_as_usize()` 不会失败。如果未来增加 32 位目标，应增加 `try_as_usize` 返回 `None` 的测试，并确认所有调用点都处理了 `None`，而不是假设转换总能成功。

11. **候选区间整段丢弃的覆盖损失（独立正确性立项，不随本次迁移闭合）**
    reserved 描述符与 source-2/3 候选部分重叠时，其非重叠尾部随整段丢弃一起失去 kernel-DM 叶子。该尾部是否包含内核、VM 或启动需要经 DM 访问的有效页，目前没有源码或测试证据定界（DM 访问落在候选间隙即触发 #PF 的形态在 riscv64 真链上出现过，说明这类间隙确实能被踩到）。修复方向是按优先级做区间差集归一化：保留高优先级区间，恢复低优先级区间中不重叠的部分，配套真机证明。在本项闭合前，集合级不变量只承诺「资源空洞不被吞进映射、高优先级候选完整保留」，不承诺「所有有资格映射的字节都被保留」；本次迁移不触碰候选算法（行为兼容边界），也不把该缺口当作已解决。交付状态声明：本次冻结（若达成）只覆盖接口迁移与选页修复；Direct Map 覆盖完整性保持为**未关闭验收门**，不因本次冻结而视为已验证——现行测试只证明资源空洞不被吞、优先级规则按设计运行，不证明所有需经 DM 访问的有效页都被保留。「行为兼容」不替代「行为正确」。

## 附录 A：代码地图

| 路径 | 作用 |
|---|---|
| `os/arch/src/arch/boot.rs` | 建议新增：`BootArch`，把分页实现、地址布局和启动安装器绑定到同一架构；承载检查换算（`try_*`、可换算上界）与启动期一次写入的运行期 PA 上界配置 |
| `os/arch/src/arch/direct_map.rs` | `DirectMapArch`：窗口基址、`KERNEL_DIRECT_MAP_SIZE`、`KERNEL_DM_VA_END_LIMIT`、方向明确的纯算术换算（`*_unchecked`；通用 `virt_to_phys` 删除）、`DirectMapLayout<A>` 及其 `validate` 与 `DmLayoutError`；检查换算（`try_*`）不在此 trait，见 `arch/boot.rs` |
| `os/arch/src/arch/paging.rs` | `Paging`，运行期页表操作、`PAGE_SIZE`、`phys_addr_limit`、`validate_virtual_range` 与 `PageFlags`；PTE 格式上限留在各架构实现内部 |
| `os/arch/src/arch/paging_ext.rs` | `PageSize`、`HugePages`、`PageSupport`、`BootPageSupport<A>`、`detect_boot_page_support<A>`、`map_huge`、`BootPageSupport::validate`、`PageSupportError` |
| `os/arch/src/arch/dm_coverage.rs` | `BootDmInstaller<P>`、`DmRange`、`DmPagePolicy<A>`（携带 `DirectMapLayout<A>`）、`DmWindow<A>`、`plan_dm_policy`、`plan_range_units`、`install_boot_dm_range`、`DmPolicyError` |
| `os/kernel/src/boot_policy.rs` | 建议新增：`KernelLayout`、`KernelMapPolicy<A>`（携带 `KernelLayout`）、`BootPagePolicy<A>`、`plan_kernel_mapping`、`plan_boot_pages`、`KernelMapError`、`BootPagePolicyError` |
| `os/arch/src/x86_64/paging.rs` | x86-64 的 `HugePages`、私有 CPUID 探针、`X86_64BootDmInstaller`、`X86_64BootArch` |
| `os/arch/src/arm64/paging.rs` | ARM64 的 `HugePages`、`AArch64BootDmInstaller`、`AArch64BootArch` |
| `os/arch/src/riscv64/paging.rs` | RISC-V 的 `HugePages`、`Riscv64BootDmInstaller`、`Riscv64BootArch` |
| `os/arch/src/lib.rs` | `CurrentBootArch` 及其投影出的 `CurrentPaging`/`CurrentDirectMap`/`CurrentBootDmInstaller`、启动分配边界 |
| `os/kernel/src/lib.rs` | `boot_validate_and_prepare`、`plan_boot_pages`/`plan_dm_policy` 调用点、`arch_boot_impl`、`build_bootstrap_root_and_enable`、两个 `establish_boot_dm` 调用点 |
| `os/kernel/src/dm_coverage.rs` | `establish_boot_dm`、候选区间并集、两个窗口的建立顺序 |
| `os/kernel/src/vm_handoff.rs` | `kernel_dm_pa_end` 与 VM 交接页 |
| `os/kernel/src/smp.rs` | `smp_init`、AP 启动顺序、`cpu_identify` 的 BSP/AP 覆盖状态 |

## 附录 B：术语

| 术语 | 含义 |
|---|---|
| PA | 物理地址 |
| VA | 虚拟地址 |
| PTE | 页表项 |
| PML4 / PDPT / PD / PT | x86-64 的四级页表：页映射四级表、页目录指针表、页目录、页表 |
| 大页 | 由页表中间层直接指向物理块的叶条目，例如 2 MiB 或 1 GiB |
| PS 位 | x86-64 页表项中的 Page Size 位，标识该条目是大页叶 |
| CPUID | x86 的 CPU 特性查询指令；GBPAGES 位表示 1 GiB 页能力 |
| BSP | Bootstrap Processor，启动时首先执行内核代码的 CPU |
| AP | Application Processor，由 BSP 启动的其他 CPU |
| 同构 SMP | 所有 CPU 对共享页表的页大小、有效物理地址上界和翻译配置给出兼容答案的 SMP 平台 |
| 系统级能力 | 在当前支持平台前提下，所有可能访问共享页表的 CPU 都支持某个页大小 |
| `BootArch` | 架构标记，把 `Paging`、`DirectMap` 和 `BootDmInstaller` 绑定到同一个 `A` |
| `PageSize` | 轻量页大小新类型，只保证值是不小于 4 KiB 的 2 的幂；不表示硬件能力 |
| `PageSupport` | 某个页大小及其能力答案的事实对 |
| `BootPageSupport<A>` | 一次启动中由 `detect_boot_page_support` 获取的、绑定架构 `A` 的能力事实；同一尺寸只有一份答案 |
| `KernelMapPolicy<A>` | 已验证的内核镜像页大小策略，包含规划时的 `KernelLayout` 和选中的页大小，绑定架构 `A` |
| `DmPagePolicy<A>` | 已验证的 Direct Map 页大小策略，包含首选大页 `Option<PageSize>`、回退大页、基础页大小和规划时的 `DirectMapLayout<A>` |
| `BootPagePolicy<A>` | 内核映射策略与 Direct Map 策略的组合，绑定架构 `A` |
| `DirectMapLayout<A>` | 同一架构的 Kernel DM 基址/容量、VM 窗口范围和上界的集中表示 |
| `DmWindow<A>` | 一次 DM 安装使用的虚拟基址、窗口物理容量（`pa_capacity`）、虚拟上界和标志，绑定架构 `A` |
| `KERNEL_DIRECT_MAP_SIZE` | Kernel DM 窗口能覆盖的物理跨度；不是系统可映射 PA 上界 |
| `pa_capacity` | Direct Map 窗口的物理容量（窗口能覆盖的 PA 偏移上界）；刻意不用 `limit`，与系统有效 PA 上界 `phys_addr_limit` 区分 |
| `KERNEL_DM_VA_END_LIMIT` | 项目为 Kernel DM 选择的虚拟地址排他终点上限；不是硬件整个 canonical 空间的上界，必须落在当前 MMU 模式合法范围内 |
| `PTE_PHYS_ADDR_BITS` | 架构内部常量：当前页表项格式能编码的物理地址位数；x86-64 52，ARM64 当前 48，RISC-V Sv39 56 |
| `phys_addr_limit` | 当前系统（含同构平台契约）可安全映射的物理地址排他上界；不是 Kernel DM 窗口容量 |
| 可换算上界 | `min(窗口容量, phys_addr_limit)`；`try_*` 换算的判定边界，成功只证明地址可换算，不证明映射存在 |
| `try_*` 换算 | Direct Map 地址换算的检查版（`Option` 返回），按可换算上界判定；含物理到虚拟、虚拟到物理与范围版 |
| bootstrap root | 启动期建立的初始页表根 |
| Direct Map | 用固定偏移把物理地址线性映射到虚拟地址的软件模式 |
| Kernel DM | 内核使用的 Direct Map 窗口，监督态可访问 |
| VM DM | VM 服务器使用的 Direct Map 窗口，用户态可访问 |
| 恒等映射 | `VA = PA` 的映射，启动期用作页表自身的写通道 |
| canonical 地址 | 地址有效载荷按最高有效位做符号扩展的虚拟地址；判据按架构（Sv39 `VA[63:39]==VA[38]`、x86-64 四级 `VA[63:47]==VA[46]`、ARM64 48 位 `VA[63:48]` 全 0 或全 1）。非规范地址不对应任何合法顶级槽位，页表遍历行为不可依赖；同一数值在不同架构的结论可能相反 |
| ZST | 零大小类型，不携带字段，只用来实现静态操作 |
| 叶子 | 页表遍历的最后一层条目，直接指向物理页或物理块 |

## 附录 C：一句话版本

`DmCoverageArch` 是启动期安装器，不是运行期服务；`supports_huge_page` 只应有一个系统级平台来源，每个架构显式实现；`PageSize` 只保证数值合法，不冒充硬件能力；`BootArch` 把能力事实、布局、策略、窗口和安装器绑到同一架构，`Current*` 别名再从它投影；`detect_boot_page_support` 获取唯一事实，纯规划函数只消费事实；`KernelMapPolicy`/`DmPagePolicy` 携带各自验证过的布局，执行入口从策略取布局；`DirectMapLayout::validate()` 与架构换算常量逐项相等，运行期地址换算永远与已验证布局一致；恒等映射窗口在建表前通过 `phys_addr_limit >= IDENTITY_MAP_END` 检查；运行期换算按三层保证分层：检查换算在 `BootArch` 绑定层、正反两向共用可换算上界 `min(窗口容量, phys_addr_limit)` 且成功不证明 PTE 存在，快速版是安全纯算术 `_unchecked`，反向检查版成对，unsafe 边界在指针形成与内存访问处；`phys_addr_limit()` 是 `min(报告上界, 编码上限)`、位宽结构性验证先于移位；`BootArch`/`BootDmInstaller` 封口（只防外部实现），mock 经 `test-support` 提供；Kernel DM 使用有限容量和项目配置的 `KERNEL_DM_VA_END_LIMIT`；PTE 格式上限留在架构编码器内部，通用层只依赖 `Paging::phys_addr_limit()`；安装器先预检几何再写叶子，失败后必须终止启动；`install_boot_dm_range` 和 `establish_boot_dm` 是 `unsafe` 启动入口，安全前提由调用者承担。
