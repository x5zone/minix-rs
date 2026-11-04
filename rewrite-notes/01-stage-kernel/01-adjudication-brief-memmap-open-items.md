# 引导期内存政策：今天的保证、未决问题与候选方案

> **读者**：看不到代码仓库的外部评审人。本文不要求你读代码，但要求你能判断两件事——问题是否真实成立、候选方案是否覆盖了失败形态。
> **怎么读**：§2 是代码**今天真实**保证的行为，每条都有测试名；§5 是**尚未裁定**的问题，其中出现的公式与方案一律是**候选**，不是合同。**不要把 §5 的任何一行当成实现依据。**
> **更新方式**：按代码切片全量重写；没有代码变化的评审轮只产出 delta。
> **历史**：二十八轮的完整论证与历史公式冻结在 `04-design-backlog-round28.md`，本文不重复。

## 1. 问题（自包含）

### 1.1 三条腿、四种描述来源

机器把控制权交给内核之前，必须先回答四个问题：哪些物理页可以分配、哪些可以映射、哪些必须禁止访问、每个对象在哪个阶段以什么方式可达。答案来自四类描述：

| 来源 | 谁提供 | 描述什么 | 今天的代码 |
|---|---|---|---|
| UEFI 内存图 | 固件 `GetMemoryMap` | 每段物理范围的类型与属性 | 解析并分类（x86_64、aarch64 两条 UEFI 腿） |
| DTB `/memory` | 设备树 | 系统 RAM 区间 | 解析（riscv64 自引导、U-Boot、aarch64 直启） |
| DTB `/reserved-memory` | 设备树 | 保留区域，可带 `no-map` | 解析（三条设备树入口；UEFI 腿尚未接） |
| 文件头保留映射块（MRB） | 设备树 | 禁止访问的保留区间 | 解析（三条设备树入口） |

启动入口共**四条**，各自的输入权威不同：

| 入口 | RAM 权威来源 | 保留声明来源 | 今天的解析状态 |
|---|---|---|---|
| x86_64 UEFI | UEFI 内存图 | 无设备树来源 | 已实现 |
| aarch64 UEFI | UEFI 内存图 | 配置表 DTB（设计目标，未接入） | 只登记地址 |
| aarch64 直启 | 启动 DTB `/memory` | 启动 DTB `/reserved-memory` + MRB | 已实现 |
| riscv64 自引导 / U-Boot | 启动 DTB `/memory` | 启动 DTB `/reserved-memory` + MRB | 已实现 |

### 1.2 为什么不能只信一份来源

- **UEFI 启动时 `/memory` 必须被忽略**：RAM 范围由固件内存图决定，设备树不是第二份 RAM 来源。
- **但静态保留区仍要进 UEFI 内存图**：`no-map` 对应固件保留类型，其它静态保留对应引导服务数据。注意范围：**所有 UEFI 路径都以内存图兑现保留区的固件保护**；只有提供权威 DTB 的路径才进一步解析 DTB 保留声明（`/reserved-memory` 与 MRB）并与内存图交叉验证。没有 DTB 来源的路径（x86）不得伪造或推断这份来源身份。
- **aarch64 UEFI 腿有两份设备树，只有一份与内存政策有关**：RAM 唯一来源是固件内存图；配置表 DTB 是保留政策的权威来源（设计目标，今天尚未接入，见 O-11）；磁盘镜像 DTB **不是内存政策输入**——它的 `/memory` 只用于覆盖对照，且该对照**只打印诊断**：不改变启动结果、映射、PMM 或任何政策结论（实现上只打印警告，不参与裁决）。
- **MRB 的强度高于普通保留**：规范要求客户端在启动程序给出明确用途前不得访问。它不能和「保留但可读」混为一谈。

### 1.3 四个维度不能压成一个

| 维度 | 问的问题 | 谁兑现 | 今天的状态 |
|---|---|---|---|
| 分配资格 | PMM 能不能把它交出去 | 可用表 / 排除集合 | 可用表有，排除集合只有两张表之间的差集，没有独立集合 |
| 映射禁止 | 页表里能不能有它的叶子 | 两棵树各自的禁止集合 | **不存在** |
| 访问义务 | 某个对象在某个阶段实际怎么访问它 | 对象登记表 + 设备义务表 | **不存在** |
| 生命周期 | 什么时候可以释放、什么时候必须保留 | 释放处置 + 阶段模型 | 只有「永不回收」一条隐含规则 |

这四者正交：一页可以「不许分配但可映射」（保留映射区），也可以「可分配但不许映射」（待定，见 O-02）。

### 1.4 两棵树：设计意图，不是今天的代码

设计意图是每棵项目页表树各有一条路径：**引导根**（交接前执行，恒等 `VA = PA`）与**最终根**（内核直接映射）。每个对象在每棵树、每个阶段都可能有不同的要求。

**今天只有一棵根**：引导阶段建立的根在 Step 4 追加直接映射覆盖后继续使用；没有独立的最终根，也没有针对它的审计与冻结（§2.3）。所有涉及「两棵树」的合同（R13/R15/R19/R21）今天都没有代码消费者。

### 1.5 三架构的差异

| 架构 | 普通主存的兑现方式 | 额外准入 |
|---|---|---|
| x86-64 | 内核自建 `PAT`，普通页显式取写回索引；`MTRR` 只读分析 | **有效类型 = PTE 的 PAT 类型 × 该区间的 MTRR 类型组合判定**，只看 PTE 选择 WB 不足；每核 `CR0.CD=0`、`CR0.NW=0`；多核有效类型一致 |
| AArch64 | `MAIR` 两个索引（普通/设备）；第一版锁定 4 KiB translation granule | 两棵树各自遍历实际叶子；每核读回 `MAIR` |
| RISC-V | 按物理区间的 PMA 能力（宽度/对齐、取指、页表读写、原子、排序、一致性）；**能力值来源必须写明**：平台静态表 / 固件或 M-mode 信息 / SoC 寄存器 / 已验证来源，不存在通用查询接口 | PMP 是另一层：约束 S 模式取指、数据访问**与页表遍历** |

## 2. 今天代码保证什么

### 2.1 设备树采集器（`os/libs/minix-boot/src/memmap_source.rs`）

`DtbMemoryMap::new(&fdt, firmware_reserve_end).collect()` 产出**两张表**：

| 表 | 内容 | 语义 |
|---|---|---|
| conventional | `/memory` 声明，扣除固件前缀与所有保留声明 | PMM 可直接分配 |
| reserved | 固件前缀 + 非 `no-map` 的保留声明（与 `/memory` 相交的部分） | 可达、永不再分配 |

解析与拒绝条件：

| 来源 | 解析内容 | 拒绝条件 |
|---|---|---|
| `/memory` | 全树枚举 + `reg()`，支持 unit-address 与多段 | 单表超过 8 段（`TooManyFreeRegions` / `TooManyReservedRegions`）；回绕（`RangeOverflow`） |
| `/reserved-memory` | 路径查找 + 可见性交叉检查；父节点的 `#address-cells`/`#size-cells` 必须存在**且与根节点取值相同**；`ranges` 必须存在且为空；子节点必须有可解析 `reg`；`no-map`/`reusable` 必须是**空属性**；两者互斥 | 缺 `reg`、宽 cells、缺 cells 属性（`UnsupportedReservedDeclaration`）；`ranges` 非空或缺失（`ReservedMemoryRangesNotEmpty`）；同时声明 `no-map` 与 `reusable`（`ConflictingReservationAttributes`）；标记属性带非空值（`NonEmptyReservationAttribute`）；声明重叠（`OverlappingReservedRanges`）；节点可见却取不到（`ReservedMemoryUnreachable`） |
| MRB | `memory_reservations()` 条目（地址+长度，`{0,0}` 终止） | 回绕（`RangeOverflow`）；**条目之间重叠**（`OverlappingMemoryReservations`）；**超过 16 条**（`TooManyMemoryReservations`，缺失终止项也会落到这里） |

规则细节：

- 声明按 4 KiB **向外**量化，再与 `/memory` 全景求交；越出 `/memory` 的部分不产生 RAM 主张，也不进入两张表。
- 固件前缀已覆盖的声明不重复登记。
- `reusable` 不作区分，按保留处理。

测试（`cargo test -p minix-boot --lib`，81 通过）中的采集用例：`dtb_single_memory_region_fw_end_zero`、`dtb_multi_region_and_unit_address_variant`、`dtb_more_than_capacity_is_rejected_not_truncated`、`dtb_declared_reservation_is_never_conventional`、`dtb_no_map_reservation_leaves_both_tables`、`dtb_reservation_inside_the_firmware_prefix_is_not_duplicated`、`dtb_reservation_outside_memory_claims_no_ram`、`dtb_memory_reservation_block_is_carved_from_free`、`dtb_overlapping_reservations_are_rejected`、`dtb_reservation_with_wrapping_end_is_rejected`、`dtb_reserved_memory_with_wide_cells_is_rejected`、`dtb_carving_that_exceeds_capacity_is_rejected`、`dtb_reserved_memory_without_empty_ranges_is_rejected`、`dtb_reservation_with_no_map_and_reusable_is_rejected`、`dtb_reservation_with_nonempty_marker_is_rejected`、`dtb_reserved_memory_cells_must_match_root`、`dtb_overlapping_memory_reservations_are_rejected`、`dtb_too_many_memory_reservations_are_rejected`。

**不保证什么**：不产出禁止集合、不产出 PMM 排除集合、不区分 `reusable`、不处理 UEFI 来源。

**三项项目限制（不是规范语义）**：① `status` 不参与筛选——`/memory` 全树枚举，`disabled` 节点仍被当作声明；② `hotpluggable` 不识别——第一版没有内存移除机制，带该属性的区间仍会进入可用表；③ `reusable` 不回收，按保留处理。三项都登记为后续实现项，而不是「规范要求如此」。

### 2.2 UEFI 分类器（`os/boot-shim/src/ebs_map.rs`）

唯一解析器（按 40 字节 v1 前缀、固定偏移读字段，支持固件给出的更大 stride）+ 六步裁决 + 三份输出：

| 输出 | 内容 | 容量与超限行为 |
|---|---|---|
| 可用表 | `CONVENTIONAL`（写回、无独立属性） | 128 段，超限拒绝启动 |
| 保留表 | 引导服务代码/数据、`ACPI_RECLAIM`、`LOADER_*`（写回） | 256 段，超限拒绝启动 |
| 诊断成员 | `RUNTIME_*`、`ACPI_NVS`、`PERSISTENT` 及其它不建普通映射的类型 | 64 段，超限拒绝启动；**每个成员保存完整区间**，不截断 |

六步裁决（压缩版）：

1. 未知类型 / 未知属性位 / ISA 属性位 → 分类错误；
2. `UNACCEPTED` / `PAL_CODE` → 拒绝启动；
3. 类型强制处置：`RESERVED`/`UNUSABLE` → 排除；类型 11/12 → 设备路径（不产出区间）；
4. 独立属性（`RUNTIME`/`NV`/`SP`）→ 诊断成员；
5. 无写回位 → 诊断成员；
6. 写回准入后按类型分派：`CONVENTIONAL` → 可用表；引导服务类 / `ACPI_RECLAIM` → 保留表；其余 → 诊断成员。

测试（`cargo test -p boot-shim --lib`，66 通过）：`types_that_earn_no_ordinary_mapping_go_to_the_side_list`、`excluded_and_device_types_stay_out_of_every_list`、`rejects_side_list_beyond_its_buffer`、`the_four_ambiguous_combinations_land_as_adjudicated`、`geometry_is_checked_for_ranges_that_are_emitted` 等 32 条。

**不保证什么**：类型 11/12 与 `RESERVED`/`UNUSABLE` 只是一种「不产出区间」的处置，它们**没有**进入任何禁止集合（禁止集合不存在）；诊断成员没有跨界载体。

**结构校验的范围**：今天的几何检查（起点对齐、页数为零、末端回绕）只作用于**会输出区间的处置**（`geometry_is_checked_for_ranges_that_are_emitted`）。不输出区间的类型（`RESERVED`/`UNUSABLE`、类型 11/12）在结构上是否合法没有检查——这是 O-12h 要补的。

**一条必须说清的项目策略**：把引导服务代码/数据与 `ACPI_RECLAIM` 放进保留表并永不回收，是**第一版的选择**，不是 UEFI 的规范语义。规范允许它们在 `ExitBootServices` / ACPI 启用之后用于一般用途（见 O-01）。

### 2.3 引导根与直接映射（`os/kernel/src/lib.rs`、`os/kernel/src/dm_coverage.rs`）

- 建立一棵项目根：恒等 4 GiB（设备窗口按 2 MiB 粒度打 `DEVICE`）+ 内核高半映射 + aarch64 设备窗口高半别名。
- Step 4 的直接映射覆盖是**在同一棵根上追加 PTE**，候选来源是 `可用表 ∪ 自举页表树 ∪ 启动模块 ∪ 保留表` 的并集过滤。
- **没有独立最终根、没有审计遍历、没有禁止集合、没有页表冻结。**

**不保证什么**：恒等段无差别覆盖低 4 GiB，因此「不在可用表」**推不出**「不会被映射」。这一点是 O-02 的核心。

### 2.4 交接与消费

两张表经固定布局交接给内核（可用表 128、保留表 256 段，另有模块表与标量）。内核 PMM 用可用表；保留表供诊断与查询接口使用。引导区相交守卫使用 `checked_add`，回绕输入直接失败（`test_bootstrap_outside_memmap_rejects_wrapping_region` / `_allocation`）。

### 2.5 四条启动入口现状

| 入口 | 采集器 | 现状 |
|---|---|---|
| x86_64 UEFI | `ebs_map` | 已实现 |
| aarch64 UEFI | `ebs_map` | 已实现；配置表 DTB 只登记物理地址，不解析、不复制 |
| aarch64 直启 | `DtbMemoryMap`（`firmware_reserve_end = 0`） | 已实现；无独立 QEMU 用例 |
| riscv64 自引导 / U-Boot | `DtbMemoryMap`（`= KERN_PHYS_BASE`） | 已实现；QEMU 启动用例 PASS |

### 2.6 明确不存在的机制

`grep -rn "Deny" os/kernel/src os/arch/src`（排除测试）只命中权限语义；`grep -rln "fn audit" os/kernel/src os/arch/src` 为空。对象登记表、设备义务表、恒等 VA 检查、PMA/PMP 准入、固定布局 ABI 均无代码。

### 2.7 关键代码位置（供复核 §2 的主张）

行号对应当前提交；复核时以**函数名**为准，行号只作定位。

| 主张 | 代码位置 |
|---|---|
| 两张表由同一次采集产出 | `os/libs/minix-boot/src/memmap_source.rs:282` `DtbMemoryMap::collect` |
| `/reserved-memory` 的查找与可见性检查 | 同文件 `:484` `lookup_reserved_memory` |
| UEFI 六步裁决 | `os/boot-shim/src/ebs_map.rs:479` `adjudicate` |
| 分类计数与提交 | 同文件 `:551` `classify`、`:619` `commit` |
| 恒等映射循环 | `os/kernel/src/lib.rs:934`（`while addr < IDENTITY_MAP_END`） |
| 引导根构造入口 | 同文件 `:894` `build_bootstrap_root_and_enable` |
| Step 4 直接映射（追加） | `os/kernel/src/dm_coverage.rs:97` `establish_boot_dm` |
| 配置表 DTB 的取得 | `os/boot-shim/src/uefi_helpers.rs:111` `find_platform_sources` |

复核方式：读这八个函数即可核对 §2 的每一条主张；不需要整仓。

## 3. 五条不变量

任何改动不得违反，§5 的候选方案也受它们约束。

1. **来源有身份，原始范围先保留**；量化在**每种政策投影内各执行一次**（同一来源可分别生成 PMM 排除与映射禁止投影），不得先合并成无身份并集再归一化。
2. **PMM 域 = 可用 ⊍ 排除**，只对 `RamSourceDomain` 成立。`RamSourceDomain` = 该启动入口的权威来源识别出的物理主存区间，**尚未应用保留声明与对象生命周期**：DTB 入口取 `/memory`；UEFI 入口按明确策略从类型中识别（不是所有描述符）；MMIO、端口、地址空洞、未描述区域都不属于它。同一页有多条保留来源时，排除只计一次、来源身份不丢。

**必须分开的两个概念**：`RamSourceDomain` 回答「权威输入是否声称这段是物理 RAM」；`RamBacking` 回答「当前阶段能否把它当普通主存映射、访问或分配」。`EfiReservedMemoryType` 是典型反例：aarch64 UEFI 腿的 DTB `no-map` 保留区由固件标成该类型，它可能**属于物理 RAM**（DTB 声称过），但**不是当前 backing**。x86_64 没有 DTB 来源，它的 `EfiReservedMemoryType` 只能按固件类型政策处置，不能套用 aarch64 的交叉核对。
3. **两棵树各一个禁止集合；禁止与分配正交**。
4. **义务键 =（对象｜设备窗口，树，阶段，路径）**；缺任一维不构成义务。
5. **设备访问 ≠ 设备映射**：MMIO 与端口 I/O 是两个地址域，映射义务与访问合同是两份记录。

## 4. 已裁定、但代码未实现

| 合同 | 内容 | 为什么现在还不能当现状 |
|---|---|---|
| 三架构普通主存准入 | 见 §1.5 表；x86 的 `PAT`/`MTRR` 分析与每核检查、AArch64 的双树遍历与 `MAIR` 读回、RISC-V 的区间能力清单与 PMP | 没有代码，也没有真实板卡证据 |
| 恒等 VA 合法性 | `VA = PA` 必须落在该树分页配置的合法虚拟地址范围，且不与既有布局冲突 | 未实现；`effective_pa_limit` 不能替代它 |
| `effective_pa_limit` 的含义 | **物理地址可表示上界**（格式位宽 ∩ 平台位宽），**不承诺**上界以下可访问 | 已裁定；PMP 与 PMA 是独立准入维度 |
| 保留映射区的权限 | 第一版一律 `R`（只读、NX）；需要写的对象要么拒绝启动、要么独立登记 | 未实现；今天保留表没有权限字段 |
| 设备访问边界 | 原始 MMIO/端口操作只允许出现在逐个列举的包装函数；调用点带稳定 `AccessUseId`；CI 静态审计 + 两枚负控 | 未实现；今天 raw 端口 asm 只在 `arch/`、`plat/`、`boot-shim/` 共 5 个文件，`kernel/`、`servers/` 零命中 |

## 5. 未决问题与候选方案

每条给出：状态、问题、失败形态、候选（**未裁定**，除标注「已裁定」者）、判据、第一个消费者。评审人请判断问题是否成立、候选是否覆盖失败形态；**不要把候选当实现依据**。

### O-01 保留表与生命周期

**问题**：今天的保留表是静态的，没有生命周期维度。UEFI 的几种类型在规范里**不是永久保留**：`EfiBootServicesCode/Data` 在 `ExitBootServices` 之后可供一般使用；`EfiACPIReclaimMemory` 在 ACPI 启用后可回收；只有 `EfiRuntimeServicesCode/Data` 必须留给运行时服务。第一版一律「永不再分配」是**项目策略**。

**失败形态**：一台机器的大部分内存在 `EfiBootServicesData` 里。第一版永不回收，这段内存永远不可分配；文档若写成「保留映射」，读者会以为这是规范要求。

**候选**：

- **A（生命周期处置）**：每条来源记录带处置 `Permanent` / `Until(event)` / `ReclaimableNow`；两张表按处置与阶段派生。**最终处置是三层叠加**：UEFI 类型生命周期 + DTB 保留声明叠加（同一 `EfiBootServicesData` 可能对应一条静态保留声明）+ 对象存活区间；**不得只按类型决定释放事件**。第一版可全取 `Permanent`，但必须标注是项目选择。
- **B（静态表 + 显式损失声明）**：保留两张表，明确写「第一版不回收 BootServices 与 ACPI_RECLAIM，损失不计入可用表」。

**逐类型保留期限（不能简化成「只有 Runtime 必须保留」）**：

| 类型 | 规范规定的保留期限 |
|---|---|
| `RUNTIME_SERVICES_CODE/DATA` | 工作状态及 ACPI S1–S3 |
| `ACPI_NVS` | 工作状态及 ACPI S1–S3 |
| `PAL_CODE` | 工作状态及 ACPI S1–S4 |
| `ACPI_RECLAIM` | ACPI 启用之前 |
| `BOOT_SERVICES_CODE/DATA` | `ExitBootServices` 后可一般使用 |
| `LOADER_CODE/DATA` | `ExitBootServices` 后可用，但当前 loader 可能仍在使用 |
| `PERSISTENT` | 按 Conventional 方式使用，但数据非易失 |

**`reusable` 不能落成 `Permanent`**：规范允许 OS 使用，但所属驱动必须能收回——它带所有者关系。候选 A 将来需要表达 `ReclaimableUnder(owner)`；第一版不实现回收，但不能把这个语义从模型里抹掉。

**判据**：每种 UEFI 类型都有完整阶段处置（`Permanent` / `Until(event)` / `ReclaimableNow` / `Unsupported`，`Permanent` 必须注明是对本项目支持的运行阶段而言）；文档不得把项目策略写成规范语义。

**第一个消费者**：最终直接映射的构造。

### O-02a 恒等映射的授权完整性

**问题**：引导根无差别覆盖低 4 GiB。每一页是否都有「允许映射」的来源授权，今天没有证明；「不在可用表」推不出「不得映射」。

**失败形态**：一段没有来源授权的地址被恒等段建立叶子，没有任何集合记录它的存在。

**候选**：

- **A（授权集合）**：每类映射只允许覆盖有授权的页——合法 RAM、内核映像、启动对象、设备窗口；其余不得建叶子。
- **B（事后审计）**：保留宽映射，逐叶审计来源，无来源即失败。

**判据**：任意一页恒等叶子都能指回一个授权来源（四类之一）；**每个实际叶子覆盖的全部页都必须具有兼容的授权、权限与属性**——大页覆盖「有授权 + 空洞 + `no-map`」时不能只看起始页，必须拆分叶子或拒绝。授权记录至少含：PA 范围、VA 范围、用途、R/W/X、内存属性、共享性、树与阶段。

**x86 的叶子属性判据**：`EffectiveMemoryType = Resolve(PAT_Entry, MTRR_Type(physical_range))`，不是简单二元运算；大页覆盖 MTRR 类型不同的区间时不能按起始地址判定，必须拆分或拒绝。**所有逻辑处理器的 `IA32_PAT` 必须一致**（或逐核读回验证），不能只查 `CR0.CD/NW`；同一物理页的多个 VA 别名不得产生不兼容的有效内存类型。

**第一个消费者**：`build_bootstrap_root_and_enable`。

### O-02b 普通保留声明是否产生映射禁令

**问题**：`/reserved-memory` 的普通声明只要求「从正常内存用途中排除」；只有 `no-map` 禁止标准系统映射，MRB 才禁止访问。三者强度不同。

**失败形态**：一条普通保留声明被当成 `no-map`，产生规范不要求的映射禁令，后续合法的设备窗口映射被判失败。

**候选**：

- **A**：普通保留声明**只产生 PMM 排除**，不自动产生映射禁止或访问禁止；若某对象需要映射，必须另有对象/设备映射授权。`no-map`（禁止标准系统映射与非受控推测访问，**设备驱动受控使用例外**）、MRB（未授权不得访问）、类型政策分别生成各自的禁止语义；`reusable` 是另一种语义且与 `no-map` 互斥。
- **B**：所有保留声明都进禁止集合（更保守，会误禁）。

**判据**：每条禁止项都能指回 `no-map` / MRB / 类型政策之一；不存在「因为没 backing 所以禁止」。

### O-03 禁止项的来源分级

**问题**：禁止映射与禁止访问是两个集合，来源不同：`no-map` 禁止标准映射；MRB 禁止访问；类型政策禁止映射；对象义务禁止某个对象在某个阶段映射或访问。

**失败形态**：把保留声明整类塞进禁止集合（普通保留被误禁），或只处理 `no-map`（MRB 的访问禁止丢失）。

**候选**：

- **A**：每条禁止项带来源标签（`no-map` / MRB / 类型 / 对象），两份集合各自生成、互不代替。
- **B**：单一集合 + 来源位图，由消费者过滤。

**判据**：每条禁止项可追溯到一个来源条目；两份集合不能互相替代。**并需回答优先级**：正向授权与禁止声明覆盖同一范围时谁优先（例如 `no-map` 允许设备驱动受控访问，不能等价于无条件映射禁令）。语义至少分成：禁止标准映射 / 禁止非受控推测访问 / 禁止特定对象·树·阶段·路径的映射 / 禁止未授权访问 / 允许特定驱动受控访问。

### O-04 对象负向义务的投影

**问题**：`MustNotMap` 与 `MustNotAccess` 不同：前者禁止某棵树在某个阶段建立映射，后者禁止访问路径。今天的禁止集合构造式里没有它们的位置。

**失败形态**：某对象被裁定「在引导根、S2 禁止映射」，但不进任何集合，宽恒等映射照旧覆盖它。

**候选**：

- **A**：两个投影名（映射禁止 / 访问禁止），流式贡献，保留对象、树、阶段、路径四维。
- **B**：第一版规定对象矩阵不出现负向态，并加空集断言。

**判据**：R19a 的输入清单与两份禁止集合的构造式逐项对得上；`DenyMapping` 不被 `DenyAccess` 代替。

### O-05 审计时点与「冻结」的含义

**问题**：按字面「T1 前完成最终根审计」不可能（最终根到 S3 才建）；「冻结 PTE」若理解为字节不变，会与硬件 `Accessed`/`Dirty` 更新冲突。

**失败形态**：实现把最终根的空遍历当作已审计；或把硬件置 A/D 判成权限变化。

**候选 A（已裁定，待实现）**：按时点拆开——

| 时点 | 必须完成 |
|---|---|
| T1 激活引导根前 | 引导根的覆盖、权限、语义、别名与禁止集合；含设备映射验收（R13-D） |
| T2 激活最终根前 | 最终根的同类审计 + 三类义务合并目标（R21） |
| 激活后 | 对**映射定义字段**持续验证 |

冻结的字段与例外：

| 字段 | 激活后 |
|---|---|
| VA 覆盖、PA 对应、页大小、有效权限、内存属性、共享性 | 不允许变化 |
| `Accessed` / `Dirty` 等硬件维护位 | 按架构规则允许特定变化；审计必须定义如何屏蔽或验证 |

**候选 B**：不冻结，每次修改后重跑对应审计——可作为后续动态页表阶段的机制，不能替代切根前审计。

**无论 A/B，后续页表修改必须「先审后用」**：在未激活的副本上修改并审计后再发布，或经受控更新接口在映射对硬件可见之前完成验证；A/D 位的屏蔽必须逐字段定义，不能笼统忽略一组 PTE 位。

**判据**：每个时点能列出「必须已完成的审计」与「允许的硬件更新」。

**第一个消费者**：根切换代码（今天只有一次根建立，没有切换）。

### O-06 `RamBacking` 的阶段化定义

**问题**：UEFI 分支不能拿来源域当 backing——它含类型排除、诊断成员与设备类描述符。

**失败形态**：一条 `RESERVED` 描述符被当作「完整 RAM 支撑」，某段保留声明因此通过 backing 检查并被映射成普通内存。

**候选 A（已裁定，待实现）**：每个来源记录回答三个问题——是否具备普通主存属性、当前阶段是否允许普通映射、当前阶段是否允许 PMM 分配。逐类型至少：

| 类型 | 属 `RamSourceDomain` | 普通主存 backing | 当前可映射 | 当前可分配 | 释放事件 | 来源叠加 |
|---|---|---|---|---|---|---|
| `CONVENTIONAL` | 是 | 是 | 是 | 是 | 无 | 可能被 DTB 静态保留声明覆盖 |
| `LOADER_CODE/DATA` | 是 | 是 | 是（存活期需可执行） | 退出前否 | `ExitBootServices` + loader 最后使用点 | 可能被保留声明覆盖 |
| `BOOT_SERVICES_CODE/DATA` | 是 | 是 | 是 | 退出前否 | `ExitBootServices` | **必须叠加 DTB 静态保留声明** |
| `ACPI_RECLAIM` | 是 | 是 | 是 | ACPI 启用前否 | ACPI 启用 | 可能被保留声明覆盖 |
| `ACPI_NVS` | 是 | 否（第一版） | 否 | 否 | 无 | — |
| `PERSISTENT` | 是 | 否（第一版） | 否 | 否 | 无（第一版保守） | 是否支持需明确 |
| `RUNTIME_SERVICES_CODE/DATA` | 是 | 否 | 否 | 否 | 保留给运行时服务 | — |
| `RESERVED` | **待裁定**（A：按固件描述纳入来源域但不进 backing；B：域外） | 否 | 否 | 否 | 无 | — |
| `UNUSABLE` | **待裁定**（同上） | 否 | 否 | 否 | 无 | — |
| 类型 11 / 12 | 否 | 否 | 设备路径另定 | 否 | 无 | — |
| `PAL_CODE` | 否 | 否 | 否 | 否 | 拒绝启动（项目兼容性限制） | — |
| `UNACCEPTED` | 是（尚未接受） | 否 | 否 | 否 | 接受前不得当作普通可用内存 | — |

**已知属性位的处置**（不能只写「未知位拒绝」）：`RUNTIME`/`NV`/`SP` 已按独立属性进入六步裁决；`RO`/`RP`/`XP`/`WP`/`MORE_RELIABLE`/`CPU_CRYPTO` 第一版作为**来源元数据保留**，不改变分类——它们描述能力或状态，不改变「能否分配/映射」。任何**档外位**仍按第 1 步拒绝启动。

**候选 B（不可接受，列出以说明为何否）**：只看平台物理内存描述、不看类型分类——会把不可用内存当成 backing。

**判据**：每个 backing 页能指回一个「普通主存 = 是」的来源记录，且阶段允许。

**边界**：`RamBacking` 只描述来源与 OS 政策资格，**不证明某架构实际能访问该区间**——x86 还要 PAT/MTRR 有效类型、AArch64 还要 MAIR 与逐 PE 配置、RISC-V 还要 PMA 能力与逐 hart 的 PMP。硬件准入不得被 `ordinary_main_memory = true` 代替。

**`LOADER_CODE` 不是一律 R/NX**：只要它仍是当前执行中的 loader 代码，就需要可执行映射；可执行权限来自仍存活的 loader 对象义务，不因「被分类为保留内存」自动变成 R/NX。最后一次执行点之后才撤销执行映射或按生命周期处置——这也是对象存活区间必须参与映射权限计算的原因。

**第一个消费者**：R4b-c3 检查器。

### O-07 R20 的「实际访问需求」与完整性来源

**问题**：R20 检查「禁止访问集合」与「实际访问需求」不相交，但后者没有定义；而且静态清单本身不能证明「没有遗漏的访问」。

**失败形态**：某对象用未登记的裸指针访问了 MRB 区域；清单自身完整，R20 通过，禁令形同虚设。

**候选**：

- **A**：从对象登记表与使用点派生 `StartupObjectAccessRequirements`（对象身份、阶段、范围、路径、读写/取指），第一版是封闭静态清单；**并追加一条完整性来源**：受控访问包装、编译期可检查的访问 API、或明确承认裸指针不在证明范围。
- **B**：把 R20 限定为「已登记访问点」，并在合同里写明证明边界。

**判据**：每条实际访问点能指回登记项；R20 的通过条件写明它证明了什么、没证明什么。**第一版取 A 的渐进形式**：封闭清单 + 代码审计或 API 约束提高覆盖率；完整性未证明前，合同范围只覆盖已登记访问点，**不得宣称系统没有访问 MRB**——裸指针、`copy_nonoverlapping`、`volatile`、内联汇编与硬件隐式访问都可能落在清单外。

**第一个消费者**：对象登记表。

### O-08 端口 I/O 的地址域（已裁定：采用 A）

端口范围用独立的半开区间与 checked 运算，不做物理页量化、不进 PMM、不进页表、不进 MMIO 记录、不进物理地址跨界 ABI；`PortWindowPolicy` 与端口访问需求都是编译期静态输入。确需跨界时另立 ABI（候选 B，暂不设计）。

**判据**：端口与物理地址在任何集合与取整函数里都不混用。**消费者**：端口访问审计。

### O-09 诊断成员与样本的存储登记（已裁定：采用 A）

完整诊断成员表与操作者可见的有界样本表各自登记：存储范围、容量、写入者、读取者、最后使用阶段。防止切根后继续引用已失效的引导期缓冲。

**判据**：容量清单里每一项都能在对象目录里找到位置与生命周期。**消费者**：容量清单。

### O-10 R13-A 的第二枚负控（已裁定：采用 A）

逐个登记允许出现原始操作的函数；对函数体内原始操作的数量与位置做检查；新增原始操作必须显式更新审计合同。**两枚负控**：① 在已批准函数体内注入第二条原始操作；② 在**已有的 `asm!` 块内**加入第二条 `in`/`out` 指令。审计器必须检查展开后或结构化语法层面的真实操作，不能只数 `asm!` 宏调用次数。

**判据**：该负控生效。**消费者**：CI 审计器。

### O-11 配置表 DTB 的首次访问授权

**问题**：四条启动入口都会收到一个「启动程序提供的 DTB 地址」，随后读 header、解析 MRB。若 DTB 本体落在 MRB 内，读取发生在「知道它是 MRB」之前。

| 输入路径 | 初始授权来源 |
|---|---|
| aarch64 UEFI | UEFI 配置表中的 DTB 指针 |
| aarch64 直启 | 架构启动 ABI 传入的 DTB 指针 |
| riscv64 自引导 | OpenSBI / 启动 ABI 传入的 DTB 指针 |
| riscv64 U-Boot | U-Boot 启动 ABI 传入的 DTB 指针 |

授权必须是启动输入合同，而不是只给 UEFI 配置表路径特批。

**失败形态**：固件把 DTB 放在 MRB 内；内核在知道它是 MRB 之前已经读了它。

**候选**：

- **A（两步授权）**：① 配置表指针授权对 DTB **固定 header 区域**的首次只读访问；② 校验 magic、版本、`totalsize`、回绕与范围合法性后，才授权对 `[dtb, dtb + totalsize)` 的完整读取，并复制到自有缓冲。授权范围与失效点写进启动输入合同。
- **B**：不承认该授权，则必须定义不读取原始地址也能取得 DTB 的其他机制（例如固件提供副本），否则该布局拒绝启动。

**另需裁定一致性策略**：配置表 DTB 的保留声明与 UEFI 内存图之间——(A) 解析权威 DTB，保留来源身份并与内存图交叉验证；(B) 只信内存图，接受来源身份丢失并对无法区分的区域取更保守的统一政策。第一版倾向前者，但 B 也可以是明确的简化，不是规范要求。

**四条入口统一两阶段**：① 固定 header 只读授权；② 校验 magic/版本/`totalsize`/回绕/范围后，授权完整对象范围并复制；③ 之后政策代码只消费副本。边界条件：header 固定读取长度写明；`totalsize` 有项目上限；`dtb + totalsize` 必须 checked；完整范围必须证明可安全读取（不能只验证起始地址）。

**重叠时的 EFI 类型裁决**：同一 EFI 页被多条来源覆盖时，先求兼容处置——`EfiReservedMemoryType` 的要求可覆盖普通的 `EfiBootServicesData` 要求，但**来源身份分别保留**；无法同时满足所有来源约束则拒绝启动。规范强度写明：静态 `/reserved-memory` 入图与类型对应是明确要求；MRB 入图是明确要求，但用 `EfiReservedMemoryType` 的措辞是 `should`（项目可提升为严格合同，不得写成 must）。

**交叉验证覆盖两种来源**（不能只验 `/reserved-memory`）：

| 权威 DTB 来源 | UEFI 内存图预期 |
|---|---|
| 普通静态 `/reserved-memory` | `EfiBootServicesData` |
| 带 `no-map` 的静态 `/reserved-memory` | `EfiReservedMemoryType` |
| MRB 条目 | `EfiReservedMemoryType` |
| 动态 `size` 保留区 | 不要求出现在内存图 |

**MRB 与普通声明重叠时**：交叉验证只核对覆盖与兼容性；政策层保留两个独立来源身份，不得合并成一个更大的区间后丢掉语义差异。

**判据**：该腿能指出保留声明的实际读取点；若 DTB 在 MRB 内，能指出授权依据（候选 A）或拒绝依据（候选 B）。

**第一个消费者**：aarch64 UEFI 腿的保留政策。

### O-12 集合构造与来源完整性（总括，拆成可独立签字的子合同）

**问题**：禁止映射集合、禁止访问集合、设备窗口集合、PMM 累积器今天的构造式、消费者与容量都没有定义。它们来源不同，不应捆绑成一个裁决。

| 子项 | 内容 | 独立判据 |
|---|---|---|
| O-12a | 引导根禁止映射集合的构造式 | 每条禁止项可追溯到一个来源（`no-map`/MRB/类型/对象） |
| O-12b | 最终根禁止映射集合的构造式 | 同上，且包含对象负向义务的投影 |
| O-12c | 设备窗口集合的来源完整性 | 集合里的每一页都能指回一条窗口政策记录 |

**设备窗口来源矩阵**（缺这张表 O-12c 不能签）：

| 入口 | 设备窗口来源 | 今天的实现 |
|---|---|---|
| x86_64 UEFI | ACPI（第一版未解析）或平台静态表 | 内核内静态常量表 |
| aarch64 UEFI | 同上 | 内核内静态常量表（`identity_device_windows()`） |
| aarch64 直启 | DTB 设备节点 `reg`，按父总线 `ranges` 转换 | 静态常量表 |
| riscv64 | DTB 设备节点 `reg` / OpenSBI 平台信息 | 静态常量表 |
| 端口 I/O | **不属于物理设备窗口**，走 O-08 | 独立地址域 |
| UEFI 类型 11/12 | **不作为设备授权来源**（11 是运行时服务 MMIO 请求；12 有 Itanium 限定） | 不产生窗口 |

端口范围不做物理页量化；`ranges` 转换需要在 O-12g 的原始记录层完成，不能先合并总线地址。
| O-12d | PMM 排除累积器的构造式 | 与可用表互补（对来源域）；每来源贡献一次 |
| O-12e | 容量、两遍扫描与失败形状 | 超限拒绝启动；两遍扫描；每份输出独立容量 |
| O-12f | `RamSourceDomain` 与 `RamBacking` 分开定义（四条入口各一组来源规则） | 不变量 2 的分区式成立；`EfiReservedMemoryType` 可属来源域但不是 backing；MMIO/端口/空洞不在域内；重叠来源排除只计一次且身份不丢 |
| O-12h | UEFI 描述符的**结构校验与政策分类分开**：版本、步长、起点对齐、页数为零、末端回绕对**所有**描述符检查；政策分类只在其后 | `RESERVED` 起点未对齐、`UNUSABLE` 页数为零、类型 11/12 末端回绕都必须拒绝；不得因为不输出区间就跳过结构校验 |
| O-12g | 采集器重构：保留原始来源记录（UEFI 描述符 / `/memory` / `/reserved-memory` / MRB / **固件前缀** / 启动对象 / 设备窗口），再派生投影；两张表降为兼容性交接输出 | 原始记录保留**完整 byte range、来源身份与属性**；PMM 投影只取与 `RamSourceDomain` 的交集；映射/访问政策按各自语义处理完整声明范围；**不得在原始层先裁剪到 `/memory`**，也不得先合并重叠来源再丢身份 |

**判据**：每个子项单独可签，不被其它子项阻塞。

### O-13 固定布局 ABI（第三层）

字段编码、偏移、容量、版本与失败形状都未定；现在只有两张区间表的布局。等 O-12 的子项定型后，按真实消费者设计，不在集合未定时冻结。

**判据**：每个跨界记录有唯一消费者、容量核算与失败形状。**消费者**：内核消费方。

## 6. 证据与复核

| 证据 | 内容 |
|---|---|
| 提交 `38668f843` | DTB 保留来源落码：`/reserved-memory`、MRB、容量/回绕/重叠/形状拒绝、引导区 checked 守卫 |
| `cargo test -p minix-boot --lib` | 81 通过（采集器用例见 §2.1） |
| `cargo test -p boot-shim --lib` | 66 通过（分类器用例见 §2.2） |
| 交叉编译 | `x86_64-unknown-none`、`aarch64-unknown-none`（`fw-none-image`）、`riscv64gc-unknown-none-elf` |
| QEMU | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh` PASS；串口 memmap 行不变 |

**证据基线**：本轮代码提交 `413ff9265`；`git rev-parse HEAD` 与 `git status --short` 应显示该提交且工作树除用户自有文件外干净；81/66 条测试是该基线上的读数。基线之后如有新提交，应重新跑并更新本段，而不是沿用旧数字。

复核方式：对 §2 的每条主张，文中给出了测试名或代码位置；对 §5 的每条问题，文中给出了失败形态——评审人可以只凭本文判断「问题是否成立」。

**测试通过不等于输入完整性**：81 与 66 条测试说明的是「已覆盖的行为」，不能证明所有 DTB 输入都被解析、所有原始访问点都被登记、每条启动路径都用了正确的权威来源。这几项各自需要独立的完整性论证（O-07、O-11、O-12）。

## 7. 签字状态

| 层 | 结论 | 依据 |
|---|---|---|
| 第一层：语义与方向 | **可签** | 五条不变量；§4 的准入合同方向明确 |
| 第二层：集合与逐路径判据 | **不签** | 三个实质阻塞：① `RamSourceDomain`/`RamBacking`/`EfiReservedMemoryType` 的边界（O-12f）；② UEFI 权威 DTB 与内存图的交叉验证未覆盖 MRB、磁盘 DTB 自检刚降为诊断（O-11）；③ 现有两张表丢失 `no-map`/MRB/`reusable` 来源差异，需按 O-12g 重构 |
| 第三层：固定布局 ABI | **不可签** | 未开始（O-13） |

**第二层重新评审的条件**：O-01～O-12 中至少「禁止集合编译 + 引导根审计」一组有代码与测试，本文按新代码重写后，才有可复核的对象。在那之前，对 §5 的评审只需回答两个问题：问题是否成立、候选是否覆盖失败形态。
