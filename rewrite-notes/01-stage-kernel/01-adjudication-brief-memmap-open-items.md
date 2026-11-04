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
| 内存保留块（Memory Reservation Block，MRB） | 设备树 | 禁止访问的保留区间 | 解析（三条设备树入口） |

启动入口共**四条**，各自的输入权威不同：

| 入口 | RAM 权威来源 | 保留声明来源 | 今天的解析状态 |
|---|---|---|---|
| x86_64 UEFI | UEFI 内存图 | 无设备树来源 | 已实现 |
| aarch64 UEFI | UEFI 内存图 | 配置表 DTB（设计目标，未接入） | 只登记地址 |
| aarch64 直启 | 启动 DTB `/memory` | 启动 DTB `/reserved-memory` + MRB | 已实现 |
| riscv64 自引导 / U-Boot | 启动 DTB `/memory` | 启动 DTB `/reserved-memory` + MRB | 已实现 |

### 1.2 为什么不能只信一份来源

- **UEFI 启动时 `/memory` 必须被忽略**：RAM 范围由固件内存图决定，设备树不是第二份 RAM 来源。
- **但静态保留区仍要进 UEFI 内存图**：`no-map` 对应固件保留类型，其它静态保留对应引导服务数据。注意范围：**所有 UEFI 路径都以内存图作为固件内存处置的权威描述来源**——内存图本身不是硬件访问保护（它不建页表、不拦访问）；内核据类型与属性建立来源记录，由后续映射与分配政策兑现。只有提供权威 DTB 的路径才进一步解析 DTB 保留声明（`/reserved-memory` 与 MRB）并与内存图交叉验证。没有 DTB 来源的路径（x86）不得伪造或推断这份来源身份。
- **aarch64 UEFI 腿有两份设备树，只有一份与内存政策有关**：RAM 唯一来源是固件内存图；配置表 DTB 是保留政策的权威来源（设计目标，今天尚未接入，见 O-11）；磁盘镜像 DTB **不是内存政策输入**——它的 `/memory` 只用于覆盖对照，且该对照**只打印诊断**：不改变启动结果、映射、PMM 或任何政策结论（实现上只打印警告，不参与裁决）。
- **MRB 的强度高于普通保留**：规范要求客户端在启动程序给出明确用途前不得访问。它不能和「保留但可读」混为一谈。

### 1.3 四个维度不能压成一个

| 维度 | 问的问题 | 谁兑现 | 今天的状态 |
|---|---|---|---|
| 分配资格 | PMM 能不能把它交出去 | 可用表 / 排除集合 | 可用表有，排除集合只有两张表之间的差集，没有独立集合 |
| 映射禁止 | 页表里能不能有它的叶子 | 两棵树各自的禁止集合 | **不存在** |
| 访问义务 | 某个对象在某个阶段实际怎么访问它 | 对象登记表 + 设备义务表 | **不存在** |
| 生命周期 | 什么时候可以释放、什么时候必须保留 | 释放处置 + 阶段模型 | 只有「永不回收」一条隐含规则 |

这四者正交：一页可以「不许分配但可映射」（保留映射区），也可以「可分配但不许映射」——**后者是模型允许表达的状态，第一版还没有这样的对象**；保留该维度是为了不让两种政策耦合，不代表第一版必须使用它。

### 1.4 两棵树：设计意图，不是今天的代码

设计意图是每棵项目页表树各有一条路径：**引导根**（交接前执行，恒等 `VA = PA`）与**最终根**（内核直接映射）。每个对象在每棵树、每个阶段都可能有不同的要求。

**今天只有一棵根**：引导阶段建立的根在 Step 4 追加直接映射覆盖后继续使用；没有独立的最终根，也没有针对它的审计与冻结（§2.3）。所有涉及「两棵树」的合同（R13/R15/R19/R21）今天都没有代码消费者。

### 1.5 三架构的差异

| 架构 | 普通主存的兑现方式 | 额外准入 |
|---|---|---|
| x86-64 | 内核自建 `PAT`，普通页显式取写回索引；`MTRR` 只读分析 | **有效类型 = `Resolve(PAT_Entry, MTRR_Type(physical_range))`**，只看 PTE 选择 WB 不足；大页跨不同 MTRR 区间必须拆分或拒绝；每核 `CR0.CD=0`、`CR0.NW=0`；**MTRR 读取结果也进入逐核有效类型一致性证明**，不能假定所有处理器配置天然一致 |
| AArch64 | `MAIR` 两个索引（普通/设备）；第一版锁定 4 KiB translation granule | 两棵树各自遍历实际叶子；每核读回 `MAIR` |
| RISC-V | 按物理区间的 PMA 能力（宽度/对齐、取指、页表读写、原子、排序、一致性）；**能力值来源必须写明并绑定 machine/SoC 型号与版本**：平台静态表 / 固件或 M-mode 信息 / SoC 寄存器 / 已验证来源，不存在通用查询接口；**不得把「未发现限制」当成「支持普通主存访问」** | PMP 是另一层：约束 S 模式取指、数据访问**与页表遍历** |

## 2. 今天代码保证什么

### 2.1 设备树采集器（`os/libs/minix-boot/src/memmap_source.rs`）

今天的真实调用路径是：

```
Raw DTB bytes → fdt_layout::validate → bounded blob slice → FDT parser
              → DtbMemoryMap::collect → conventional / reserved
```

**这段 blob 仍是固件提供的原地内存**：校验、解析、MRB 读取都直接在原始地址上进行，**没有复制到内核自有缓冲**。因此 O-11 的三段式（header 可达性 → 完整读取授权 → 自有副本上校验与解析）目前只是合同，不是实现；今天的授权事实是「启动程序给了指针，内核就近检验并在原位消费」。`DtbMemoryMap` 自身不做容器校验，校验器是外层输入边界；不要绕过它直接采集。`collect()` 产出**两张表**：

| 表 | 内容 | 语义 |
|---|---|---|
| conventional | `/memory` 声明，扣除固件前缀与所有保留声明 | PMM 可直接分配 |
| reserved | 固件前缀 + 非 `no-map` 的保留声明（与 `/memory` 相交的部分） | 可达、永不再分配 |

**来源记录层（DTB 侧，第一阶段）**：`collect_with_sources()` 在同样的遍历里为每条声明留一条 `SourceRecord { id, kind, origin, range_start, range_end }`——`kind` 六类：`Memory`、`FirmwarePrefix`、`MrbEntry`、`ReservedOrdinary`、`ReservedReusable`、`ReservedNoMap`；`SourceKind::class()` 把它们分成**输入声明**（`Memory`/`MrbEntry`/`Reserved*` 五类）与**派生保留**（`FirmwarePrefix`）。**字段名是中性词**：输入声明的 `range_*` 就是声明范围；派生前缀的 `range_*` 是派生结果 `M_i ∩ [0, F)`（`F = firmware_reserve_end`），所以不叫 `raw_*`。`origin` 给出位置：`MemoryNode { node, reg }`、`ReservedMemoryChild { node, reg }`、`MrbEntry(n)`（`node` 只数匹配节点、`reg` 只数该节点 `reg` 属性内的元组，均从 0 起、按文档顺序）；前缀的 `origin` 是 `FirmwarePrefix { memory_id, cut }`，既指回被切分的 `/memory` 记录，又保留派生参数 `F` （原 DTB 释放后仍能解释「为什么截在这里」）。`id` 是**输入对象内的局部 ID**；跨输入引用须用 `(InputObjectId, LocalSourceId)`，不能单用 `id`。**记录不携带页范围**：`/memory` 投影向内取整（`FullPages`），保留声明与 `no-map` 向外取整，MRB 访问禁止保留精确字节。`range_end` 为独占端；DTB `reg` 回绕在采集时即拒绝，故不会出现 `range_end == 2^64`（`u128` 端点规则属 UEFI 描述符路径）。容量 40 由**输入上限**推出（`/memory` 段与声明各受 `DTB_MEMMAP_MAX`、`no-map` 同样计数、前缀不多于 `/memory` 段、MRB 受 `DTB_MRB_MAX`），按 **reg 元组**计数而非节点数；超限拒绝（`TooManySourceRecords`）。`collect()` 语义不变、只取前两张表。测试：`dtb_no_map_reservation_keeps_a_source_record`、`dtb_memory_reservation_record_keeps_the_raw_range`、`dtb_reservation_outside_memory_keeps_its_record`、`dtb_source_records_cover_every_projected_span`、`dtb_collect_and_collect_with_sources_agree`、`dtb_no_map_declarations_count_against_the_declaration_cap`、`dtb_declaration_cap_is_shared_across_reservation_kinds`、`dtb_source_capacity_is_reachable_at_exactly_forty`、`dtb_firmware_prefix_links_to_its_memory_record`、`dtb_identical_reservations_are_deduplicated_not_rejected`、`dtb_identical_mrb_entries_are_rejected`、`dtb_same_range_with_different_semantics_is_a_conflict`、`dtb_every_input_declaration_has_exactly_one_record`、`dtb_identical_memory_tuples_are_deduplicated_not_rejected`。

解析与拒绝条件：

| 来源 | 解析内容 | 拒绝条件 |
|---|---|---|
| `/memory` | 全树枚举 + `reg()`，支持 unit-address 与多段 | 单表超过 8 段（`TooManyFreeRegions` / `TooManyReservedRegions`）；回绕（`RangeOverflow`） |
| `/reserved-memory` | 路径查找 + 可见性交叉检查；父节点的 `#address-cells`/`#size-cells` 必须存在**且与根节点取值相同**；`ranges` 必须存在且为空；子节点必须有可解析 `reg`；`no-map`/`reusable` 必须是**空属性**；两者互斥 | 缺 `reg`、宽 cells、缺 cells 属性（`UnsupportedReservedDeclaration`）；`ranges` 非空或缺失（`ReservedMemoryRangesNotEmpty`）；同时声明 `no-map` 与 `reusable`（`ConflictingReservationAttributes`）；标记属性带非空值（`NonEmptyReservationAttribute`）；声明重叠（`OverlappingReservedRanges`）；节点可见却取不到（`ReservedMemoryUnreachable`） |
| MRB | `memory_reservations()` 条目（地址+长度，`{0,0}` 终止） | 回绕（`RangeOverflow`）；**零长度非终止项**（`ZeroLengthMemoryReservation`）；**条目之间重叠**（`OverlappingMemoryReservations`）；**超过 16 条**（`TooManyMemoryReservations`，缺失终止项也会落到这里）。对齐、`totalsize` 边界与终止项**由 `fdt_layout::validate` 在解析库之前检查**（五条输入路径接入）；`DtbMemoryMap` 自身不做头部校验，也不声称做过 |

规则细节：

- 声明按 4 KiB **向外**量化，再与 `/memory` 全景求交；越出 `/memory` 的部分不产生 RAM 主张，也不进入两张表。
- 固件前缀已覆盖的声明不重复登记（投影不重复，来源记录仍保留——见下一条）。
- **重复与重叠按来源分别裁决**：

| 来源 | 完全相同、相同语义 | 部分重叠 | 同范围不同语义 |
|---|---|---|---|
| `/memory` | 精确重复 → 投影去重、记录各留一条 | **拒绝**（`OverlappingMemoryRanges`）：重页会进 PMM 两次——项目输入限制 | —（`/memory` 只有 RAM 语义） |
| `/reserved-memory`（普通/`reusable`/`no-map` 三种语义） | 同语义重复 → 投影去重、记录各留一条（测试：普通 `dtb_identical_reservations_are_deduplicated_not_rejected`、`reusable` `dtb_reusable_duplicates_are_deduplicated_not_rejected`） | 拒绝（`OverlappingReservedRanges`） | 拒绝（`ConflictingReservationAttributes`：普通+`reusable` 生命周期不同、普通+`no-map` 访问不同、`reusable`+`no-map` 规范互斥；测试 `dtb_ordinary_and_reusable_same_range_is_a_conflict`、`dtb_reusable_and_no_map_same_range_is_a_conflict`、`dtb_same_range_with_different_semantics_is_a_conflict`） |
| MRB | **拒绝**（`OverlappingMemoryReservations`）：DTSpec 5.3.2「These given regions shall not overlap each other」，重复也是重叠（`dtb_identical_mrb_entries_are_rejected`） | 拒绝 | —（MRB 无 `no-map` 语义） |

**页级生命周期规则（为将来 `reusable` 回收设防）**：普通保留与 `reusable` 原始字节可不相交却落在同一页；EFI 类型相同不构成冲突，但只有该页上**全部分配排除来源都已解除**，页才能进入 `PMMAvailable`。第一版两种声明都不回收，结果自然是继续排除——这条规则防止将来启用回收时出现「部分页释放」。

`/reserved-memory` 的重复容忍是**项目级输入策略**，不是规范授权；MRB 没有这层容忍。`reusable` 在去重时保留的是**声明身份**（两条记录各有 `id`）；owner 关系尚未建立，不能把记录数量当成 owner 数量——`memory-region` 引用是多对多（一个设备可引用多个区域、一个区域可被多个设备引用），模型应是 `DeviceMemoryRegionReference`（或 `MemoryRegionAssociation`）的引用集合，而非单个 `OwnerId` 字段。**引用不等于所有权**：只有平台 binding 或驱动注册信息进一步证明回收职责时，才提升为 `ReclaimAuthority`。`/memory` 的**部分重叠**拒绝同样是项目策略：**不是数学必然**——§3 允许纯集合层对兼容区间 `Union`，部分重叠在集合模型里可以安全归一化；拒绝的理由是「严格输入策略：部分重叠视为固件异常」，它不依赖今天 PMM 的实现，也不声称 DTSpec 禁止 `/memory` 重叠。若将来改为并集，来源记录仍须各留一条。`reusable` 的 owner 关系在去重时不得合并（来源记录各留一条正是为此）。
- `reusable` 在记录层已独立成 `ReservedReusable`；两张投影表仍把它按保留处理（不实现 owner 关联、回收能力或生命周期处置），第一版保守地不回收，但不丢原始声明语义。

测试（`cargo test -p minix-boot --lib`，132 通过）中的采集用例：`dtb_single_memory_region_fw_end_zero`、`dtb_multi_region_and_unit_address_variant`、`dtb_more_than_capacity_is_rejected_not_truncated`、`dtb_declared_reservation_is_never_conventional`、`dtb_no_map_reservation_leaves_both_tables`、`dtb_reservation_inside_the_firmware_prefix_is_not_duplicated`、`dtb_reservation_outside_memory_claims_no_ram`、`dtb_memory_reservation_block_is_carved_from_free`、`dtb_overlapping_reservations_are_rejected`、`dtb_reservation_with_wrapping_end_is_rejected`、`dtb_reserved_memory_with_wide_cells_is_rejected`、`dtb_carving_that_exceeds_capacity_is_rejected`、`dtb_reserved_memory_without_empty_ranges_is_rejected`、`dtb_reservation_with_no_map_and_reusable_is_rejected`、`dtb_reservation_with_nonempty_marker_is_rejected`、`dtb_reserved_memory_cells_must_match_root`、`dtb_overlapping_memory_reservations_are_rejected`、`dtb_too_many_memory_reservations_are_rejected`、`dtb_disabled_memory_node_is_rejected`、`dtb_hotpluggable_memory_is_rejected`、`dtb_zero_length_memory_reservation_is_rejected`。

**不保证什么**：不产出禁止集合、不产出 PMM 排除集合、不实现 `reusable` 的 owner 回收、不处理 UEFI 来源。**`collect()` 的两张表无法表达 MRB 来源**：MRB 范围会从 conventional 扣除，但不会进入 reserved（reserved 的语义是「可达、永不再分配」，MRB 是「不得访问」）；调用方若不持有 `SourceRecord`，排除原因与访问禁止来源都不可追溯。这是旧接口的**有意取舍**，修复方向是让政策所有者持有记录，而不是把 MRB 塞进 reserved（那会把「不得访问」误表达成「可达但不分配」）。因此 `dtb_collect_and_collect_with_sources_agree` 只证明两条接口的**投影一致**，不证明旧接口保留来源完整性。

**输入准入（已实现，不是「已知限制」）**：`/memory` 的 `status` 缺失或为 `okay` 才采集；其它值一律拒绝启动（`UnsupportedMemoryNodeStatus`）——节点自己说不可用的内存不得交给 PMM。带 `hotpluggable` 的 `/memory` 拒绝启动（`HotpluggableMemoryUnsupported`）：内核没有内存移除机制。`/reserved-memory` 子节点的 `status` 不筛选——保留声明即使 `disabled` 也按声明处理（保守方向，只会多留不会误放行）。**仍是项目限制**：`reusable` 不回收；记录保留其类型，回收语义留给后续生命周期层。

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

测试（`cargo test -p boot-shim --lib`，69 通过）：`types_that_earn_no_ordinary_mapping_go_to_the_side_list`、`excluded_and_device_types_stay_out_of_every_list`、`rejects_side_list_beyond_its_buffer`、`the_four_ambiguous_combinations_land_as_adjudicated`、`geometry_is_checked_for_ranges_that_are_emitted` 等 32 条。

**不保证什么**：类型 11/12 与 `RESERVED`/`UNUSABLE` 只是一种「不产出区间」的处置，它们**没有**进入任何禁止集合（禁止集合不存在）；诊断成员没有跨界载体。**输出分流相同 ≠ 语义相同**：`RESERVED` 在来源域之外（类型本身推不出普通 RAM 资格），`UNUSABLE` 在来源域之内、由类型资格排除；未来不能让实现者把两者合并成一个 `Excluded`。两者的差异要等 UEFI 原始描述符记录落地后才能兑现（O-12g 第二半）。

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

**先说清「目标路径」与「今天接入的路径」**：§1.1 的四条是**目标输入路径**；今天真正参与内存政策裁决的是三条 DTB 解析路径 + x86 的 UEFI 分类器。aarch64 UEFI 的配置表 DTB **尚未接入**（只登记地址），磁盘 DTB 只做诊断。**aarch64 UEFI 没有配置表 DTB 时的第一版规则**：允许仅依据 UEFI 内存图启动（来源身份仅 UEFI 侧），**不做 DTB 交叉验证**；绝不回退到磁盘诊断 DTB 充当权威来源。

**三个层次必须分开读**：库接口已实现、启动路径已接入、消费者已接入。今天只有第一个成立：

| 入口 | 采集器 | 采集接口 | `SourceRecord` 去向 |
|---|---|---|---|
| x86_64 UEFI | `ebs_map` | 无 DTB 记录接口 | 无（UEFI 描述符记录未做） |
| aarch64 UEFI | `ebs_map` + 磁盘 DTB 覆盖对照 | `collect()`（`uefi_helpers.rs:283`、`:837`） | **被丢弃**；配置表 DTB 只登记物理地址 |
| aarch64 直启 | `DtbMemoryMap`（`firmware_reserve_end = 0`） | `collect()`（`bootface_a64.rs:199`） | **被丢弃**；无独立 QEMU 用例 |
| riscv64 自引导 / U-Boot | `DtbMemoryMap`（`= KERN_PHYS_BASE`） | `collect()`（`bootface.rs:203`、`opensbi_helpers.rs:275`） | **被丢弃**；QEMU 用例 `test-kernel-image-riscv64-bootface.sh` **只覆盖 bootface 入口**，不覆盖 `RiscvUbootBootShim::prepare_boot` |

`grep -rn 'collect_with_sources' --include=*.rs os`（排除 `memmap_source.rs`）零命中：记录层目前是**库接口 + 测试消费者**，没有任何启动路径持有它。要让来源记录进入政策流程，须先决定由谁持有、在哪个阶段传递、何时释放——这是下一刀，不是今天的事实。

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

**FDT 容器校验的五个调用点**（`fdt_layout::validate` 在解析库之前执行）：

| 调用点 | 输入来源 | 参与内存政策 | 校验结果用于解析 |
|---|---|---|---|
| `bootface()`（`kernel-image/src/bootface.rs`） | riscv 启动 ABI 递入的 DTB | 是 | 是 |
| `bootface_a64()`（`kernel-image/src/bootface_a64.rs`） | aarch64 直启 DTB | 是 | 是 |
| `RiscvUbootBootShim::prepare_boot`（`boot-shim/src/opensbi_helpers.rs`） | OpenSBI/U-Boot a1 | 是 | 是 |
| `UefiBootShim::prepare_boot`（`uefi_helpers.rs` 的磁盘 DTB 分支） | ESP 上的 `dumpdtb.dtb` | **否**（只作覆盖对照诊断） | 是（只用于诊断读取） |
| `load_dumpdtb_free`（`uefi_helpers.rs`） | 同上 | **否** | 是（只用于诊断读取） |

最后两行是磁盘镜像 DTB：它**不进入内存政策**，校验只保护诊断读取路径；重构时不得把它接进政策链。

复核方式：`os/libs/minix-boot/src/fdt_layout.rs` 的 `validate`（容器）与本节的函数和映射构造位置即可核对 §2 的每一条主张；`validate` 的五处实际调用点见本节表格。不需要整仓。

## 3. 五条不变量

任何改动不得违反，§5 的候选方案也受它们约束。

1. **来源有身份，原始范围先保留**；量化在**每种政策投影内各执行一次**（同一来源可分别生成 PMM 排除与映射禁止投影）。**「不得合并」只约束来源记录层，不约束纯集合计算**：

| 层次 | 规则 |
|---|---|
| 来源事实层 | 每条声明保留自己的 `SourceRef`、原始范围与语义，来源记录不合并 |
| 冲突解析层 | 对重叠主张做兼容性裁决，保留全部参与裁决的来源身份 |
| 纯集合投影层 | 允许对已裁决的兼容区间做 `Union`，再按该投影规定的顺序量化 |
| 归因层 | 每个投影结果仍可反查贡献它的全部 `SourceRef` |

各投影的量化顺序：`RamSourcePageDomain` 先 `Union` 再 `FullPages`；`TypeAllocatablePages` 先裁决类型/属性兼容、再合并、再 `FullPages`；PMM 显式保留逐来源 `Outward` 后合并页集合；`no-map` 逐声明 `Outward`（映射禁止）；MRB 访问审计保留精确 byte range、页表落实时另行 `Outward`；设备映射按实际叶子逐页检查授权。
2. **PMM 域 = 可用 ⊍ 排除**，只对 `RamSourceDomain` 成立。`RamSourceDomain` = 该启动入口的权威来源识别出的物理主存区间，**尚未应用保留声明与对象生命周期**：DTB 入口取 `/memory`；UEFI 入口按明确策略从类型中识别（不是所有描述符）；MMIO、端口、地址空洞、未描述区域都不属于它。同一页有多条保留来源时，排除只计一次、来源身份不丢。

**必须分开的两个概念**：`RamSourceDomain` 回答「权威输入是否声称这段是物理 RAM」；`OrdinaryRamEligibility`（原 `RamBacking`）只回答「当前阶段是否允许把它作为普通主存**映射**」；能否分配由 `PmmEligibility(phase)` 表达，某个对象能否经某条路径访问由 `AccessRequirement/AccessAuthorization` 表达，物理内存性质由 `MemoryKind` 表达。四者不能互相推导。**术语说明**：`backing` 容易被读成「物理支撑」，而本项说的是 OS 政策资格——`RUNTIME_SERVICES_*`、`ACPI_NVS`、`PERSISTENT` 都有物理支撑却不具普通资格，`UNACCEPTED` 有物理来源但未接受。物理支撑性质、OS 政策资格、硬件准入是三个层次，不能压成一个布尔值。`EfiReservedMemoryType` 是典型反例：aarch64 UEFI 的权威 DTB `/reserved-memory` 与 MRB 提供**保留来源身份**，UEFI 内存图提供物理存在与固件处置；两者可交叉验证保留声明，但**都不能单独推出 `RamSourceDomain` 成员资格**——只有独立平台证据能证明属于系统 RAM 时才纳入。x86_64 没有 DTB 来源，它的 `EfiReservedMemoryType` 只能按固件类型政策处置，不能套用 aarch64 的交叉核对。
3. **两棵树各一个禁止集合；禁止与分配正交**。
4. **义务键 =（对象｜设备窗口，树，阶段，路径）**；缺任一维不构成义务。
5. **设备访问 ≠ 设备映射**：MMIO、端口 I/O 与 CPU 架构寄存器访问是**三种不同的访问接口/命名域**（第三种不属于物理地址集合，不得进入 PMM、页表或端口范围集合）；映射义务与访问合同是两份记录。

## 4. 已裁定、但代码未实现

| 合同 | 内容 | 为什么现在还不能当现状 |
|---|---|---|
| 三架构普通主存准入 | 见 §1.5 表；x86 的 `PAT`/`MTRR` 分析与每核检查、AArch64 的双树遍历与 `MAIR` 读回、RISC-V 的区间能力清单与 PMP | 没有代码，也没有真实板卡证据 |
| 恒等 VA 合法性 | `VA = PA` 必须落在该树分页配置的合法虚拟地址范围，且不与既有布局冲突 | 未实现；`effective_pa_limit` 不能替代它 |
| `effective_pa_limit` 的含义 | **物理地址可表示上界**（格式位宽 ∩ 平台位宽），**不承诺**上界以下可访问 | 已裁定；PMP 与 PMA 是独立准入维度 |
| 保留映射区的权限 | **已经获得普通 RAM 映射授权的保留对象**默认 `R`、`NX`（不适用于 `no-map`、MRB、类型政策禁止映射的区域，也不适用于走独立 `DEVICE` 属性的 MMIO 窗口）；需要 `W` 或 `X` 的存活对象（loader 代码、仍被读取的数据）必须独立登记用途、生命周期与权限，并通过对应树的叶子审计；第一版不支持的权限需求**拒绝启动** | 未实现；今天保留表没有权限字段 |
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

**`EfiReservedMemoryType` 的禁止语义分两层，且不依赖 DTB**：

| 层 | 主张 |
|---|---|
| 规范语义 | `DenyOSAccess`——UEFI 规定 OS 不得访问，但不等于 DTSpec `no-map` 的「禁止标准系统映射 + 非受控推测访问」 |
| 项目兑现 | 第一版额外禁止其进入普通系统映射（`DenyOrdinarySystemMapping`）——保守选择，写明是项目策略 |

**与 `no-map` 的驱动例外组合时，第一版选「绝对禁止」**：`EfiReservedMemoryType` 区域即使同时带 DTB `no-map`，也不开放 `DriverControlledUse`——UEFI 类型说该区域 OS 不应访问，而第一版没有实现「专属设备身份 + 范围 + 阶段 + 访问路径」的受控路径。组合测试（`EfiReservedMemoryType` + `no-map` + 受控使用）列为待补；另一个可选边界是「一般用途禁止、专属驱动例外可用」，但第一版不选它，避免让 `DenyOSAccess` 这个名字隐式决定组合语义。

`TypePolicy` 必须能独立生成这两条主张，**不要求存在对应的 DTB `SourceRecord`**（x86_64 没有 DTB，不能因此丢掉 UEFI 自己的禁止语义）。两者都可能导致普通直接映射被拒，但来源语义与例外规则不同，不得合并成一个无差别 `DenyMapping`。同理，`UNUSABLE` 是域内排除（来源身份存在、类型资格排除），不与域外的 `RESERVED` 合并。

**`reusable` 不能落成 `Permanent`**：规范允许 OS 使用，但所属驱动必须能收回——它带所有者关系。候选 A 将来需要表达 `ReclaimableUnder(owner)`；第一版不实现回收，但不能把这个语义从模型里抹掉。**owner 不能从节点层级推导**：DTSpec 用设备节点的 `memory-region` phandle 引用 `/reserved-memory` 子节点，所有者关系必须来自这条引用（或其它权威平台信息），并记录引用身份；`ReservedReusable` 目前只有声明身份，没有可兑现的回收授权。

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

**判据**：每条禁止项可追溯到一个来源条目；两份集合不能互相替代。

**冲突组合规则（按评审人给的六行，本项目采纳为候选规则）**：

| 情况 | 结果 |
|---|---|
| 普通映射授权 vs `no-map` | 拒绝普通映射 |
| 普通访问授权 vs MRB | 拒绝访问 |
| 设备驱动受控使用 vs `no-map` | 只有满足规范允许的受控用途且具备专属授权时才允许例外 |
| `EfiReservedMemoryType` + `no-map` + `DriverControlledUse` | **拒绝**：该组合额外命中 UEFI 类型的 `DenyOSAccess`，本项目第一版不开放此驱动例外。这不等于「所有 `no-map` 一律禁止驱动受控使用」——只是这一来源组合多了一条更强的类型政策 |
| 未获启动程序授权的访问 vs MRB | 拒绝访问 |
| 类型政策绝对禁止 vs 对象授权 | 拒绝 |
| 对象 `MustNotMap` vs 一般 `MustMap` | 拒绝启动，除非设计明确列出例外 |

原则不是「全局优先级数字」，而是：**一般正向授权不能覆盖具体的负向限制；例外必须具有更具体的对象身份、范围、路径与阶段；来源语义未明确允许的例外一律拒绝启动**。`no-map` 的例外与 MRB 的例外不是同一机制。

**`no-map` 要拆成三项，不能简化成一个无条件 `DenyMapping`**：`DenyStandardSystemMapping`（禁止进入标准系统内存映射）、`DenyUncontrolledSpeculativeAccess`（禁止非受控推测访问）、可选的 `DriverControlledUse`（专属设备身份 + 专属访问路径的受控使用）。「有驱动申请」不是通用豁免，不能让 `no-map` 页进入内核直接映射。

**MRB 的部分页**：若靠页表兑现 MRB 访问禁止，则该页整体不得出现在普通映射中；同页还有其它对象需要映射时，拒绝冲突或提供独立受控机制。精确 byte-range 检查不能代替页表兑现。

**`no-map` 与 MRB 在页表粒度上的保守投影相似，但原始语义与例外机制不同**：`no-map` 的普通系统映射禁止按相交页向外投影；MRB 是「未经授权不得访问」，两者不能共用一套例外规则。`DriverControlledUse` **不自动解除整页限制**；若专用映射的实际叶子覆盖超出 `no-map` 资源范围，必须证明整叶覆盖区的属性、权限、别名与访问控制均兼容，否则拒绝启动或改用更细页粒度。测试要求：`no-map` 只覆盖 4 KiB 页的部分字节时，普通直接映射不得出现该页，驱动例外也不得未经整页审查放行。

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

**冻结对象是 `FrozenBootMappingSet`，不是整棵页表**：T1/T2 审计过的是启动政策映射集合，该集合内的映射定义不得被未授权修改；其它页表区域（未来的 VM、动态映射、进程地址空间）经受控更新接口变化——准备修改 → 校验政策 → 发布 → 架构同步——这不违反冻结。**不接受的表述**：「启动后页表永久不可改」。

**候选 B**：不冻结，每次修改后重跑对应审计——可作为后续动态页表阶段的机制，不能替代切根前审计。

**无论 A/B，后续页表修改必须「先审后用」**：在未激活的副本上修改并审计后再发布，或经受控更新接口在映射对硬件可见之前完成验证；A/D 位的屏蔽必须逐字段定义，不能笼统忽略一组 PTE 位。

**政策审计与硬件同步是两份独立合同**：审计通过不等于新映射已在所有执行单元生效。架构层必须负责 PTE 写入的发布顺序、相应地址空间的 TLB 失效、以及多核之间的同步失效（shootdown）；共享层只表达「新映射必须在所有相关执行单元生效」这个语义，不出现 `INVLPG`/`SFENCE.VMA`/`TLBI` 之类机制。

**判据**：每个时点能列出「必须已完成的审计」与「允许的硬件更新」。

**第一个消费者**：根切换代码（今天只有一次根建立，没有切换）。

### O-06 `OrdinaryRamEligibility` 的阶段化定义

**问题**：UEFI 分支不能拿来源域当 backing——它含类型排除、诊断成员与设备类描述符。

**失败形态**：一条 `RESERVED` 描述符被当作「完整 RAM 支撑」，某段保留声明因此通过 backing 检查并被映射成普通内存。

**候选 A（已裁定，待实现）**：每个来源记录回答三个问题——是否具备普通主存属性、当前阶段是否允许普通映射、当前阶段是否允许 PMM 分配。逐类型至少：

| 类型 | 属 `RamSourceDomain` | 普通主存性质 | 当前可映射 | 当前可分配 | 释放事件 | 来源叠加 |
|---|---|---|---|---|---|---|
| `CONVENTIONAL` | 是 | 是 | **条件**：类型层面是；第一版还要求缓存属性为 WB | **条件**：阶段资格 + 缓存资格 + 全部排除项通过 | 无 | 按兼容矩阵裁决 |
| `LOADER_CODE` | 是 | 是 | 是（存活期需 X；由对象义务提供） | 退出前否 | `ExitBootServices` + loader 最后使用点 | 按兼容矩阵裁决 |
| `LOADER_DATA` | 是 | 是 | 是（**默认 NX**；写需对象义务授权 W） | 退出前否 | 同上 | 按兼容矩阵裁决 |
| `BOOT_SERVICES_CODE` | 是 | 是 | 是（固件活跃期按代码义务） | 退出前否 | `ExitBootServices` | 见下方兼容矩阵 |
| `BOOT_SERVICES_DATA` | 是 | 是 | 是（默认 NX；仍被使用时按读写义务） | 退出前否 | 同上 | 同上 |
| `ACPI_RECLAIM` | 是 | 是 | 是 | ACPI 启用前否 | ACPI 启用 | 按兼容矩阵裁决 |
| `ACPI_NVS` | 是 | 否（第一版） | 否 | 否 | 无 | — |
| `PERSISTENT` | 是 | **是（非易失字节可寻址物理内存，存在 backing）** | 否（第一版普通 volatile 策略） | 否 | 无（第一版保守） | 是否支持需明确；「无普通资格」≠「无 backing」 |
| `RUNTIME_SERVICES_CODE/DATA` | 是 | 否 | 否 | 否 | 保留给运行时服务 | — |
| `RESERVED` | **否（采用 B）**：仅凭类型名推不出是系统 RAM；有额外平台证据时另行纳入 | 否 | 否 | 否 | 无 | — |
| `UNUSABLE` | **是**（固件描述它包含错误，但它仍是物理内存的描述） | 否 | 否 | 进 PMM 排除 | 无 | — |
| 类型 11 / 12 | 否 | 否 | 设备路径另定 | 否 | 无 | — |
| `PAL_CODE` | 不支持（发现即拒绝启动，不做成员资格承诺） | 否 | 否 | 否 | 拒绝启动（项目兼容性限制） | — |
| `UNACCEPTED` | 是（尚未接受） | 否 | 否 | 否 | 接受前不得当作普通可用内存 | — |

**资格是分层的，不是三个布尔值**：`RamSourceDomain`（权威来源是否声明这是物理主存）、`MemoryKind`（普通易失 / 持久 / NVS / 未接受 / 不可用）、`OrdinaryMappingEligibility(tree, phase)`、`PmmEligibility(phase)`、`AccessRequirement/AccessAuthorization` 各自回答不同问题，不能互相推导——普通 RAM 可能因 `no-map` 禁止普通映射；设备 MMIO 不是普通 RAM 却有设备访问授权；普通 RAM 可能因 MRB 禁止访问；保留对象可能允许特定对象读却不许 PMM 分配。表头「普通主存性质」只描述 `MemoryKind`，不是「能否使用」的总结论。

**「来源叠加」不是无条件成立（兼容矩阵）**：DTSpec 要求静态 `/reserved-memory` 出现在 UEFI 内存图里——`no-map` 对应 `EfiReservedMemoryType`，其它静态保留对应 `EfiBootServicesData`。所以表里「按兼容矩阵裁决」指的就是下面三种组合：

| UEFI 描述符 | 与权威 DTB 静态 `/reserved-memory` 重叠时 |
|---|---|
| `EfiBootServicesData` | 普通保留声明兼容（规范给出的预期组合） |
| `EfiReservedMemoryType` | `no-map` 兼容；普通声明需按来源组合裁决 |
| 其它类型 | 与静态保留的预期类型不符：拒绝启动，除非有明确的项目扩展规则 |

同一页同时被普通静态保留与 MRB 覆盖时，**两种类型要求冲突、第一版拒绝启动**（见 O-11 的同页裁决表）；不存在「较强类型覆盖较弱类型」的泛化规则，来源记录在拒绝诊断中分别保留。动态 `size` 保留：规范要求它**不出现在** UEFI 内存图（不是「不要求出现」）；`reg` 与 `size` 同时存在时按静态 `reg` 处理，`size` 不决定分配。

**第一版处置与统一模型语义分开**：「属 `RamSourceDomain`」是按统一模型填的，不等于今天的分类器已经支持它继续启动：

| 类型 | 统一模型中的含义 | 第一版处置 |
|---|---|---|
| `UNACCEPTED` | 有物理来源、尚未接受、无普通 backing | **拒绝启动**，不进入阶段化分配 |
| `PAL_CODE` | 固件/处理器专用的代码内存 | 拒绝启动（项目兼容性限制） |
| `PERSISTENT` | 非易失物理内存，有 backing | 不接入普通 volatile PMM 与直接映射，保留来源记录 |

**已知属性位的处置**（不能只写「未知位拒绝」）：`RUNTIME`/`NV`/`SP` 已按独立属性进入六步裁决；`RO`/`RP`/`XP`/`WP`/`CPU_CRYPTO` 是 **capability 能力位**（`MORE_RELIABLE` 因 `0x1_0000` 的官方文本歧义已改为 fail-closed，见下），第一版只作为来源元数据保留——**不直接等同于当前已生效的访问权限**，也不改变 RAM 来源域成员资格。后续映射策略可用其中的能力信息验证目标属性是否可兑现；**`WP` 不得误解为页表写权限**。任何**档外位**仍按第 1 步拒绝启动。

**属性按语义分三类**（不能统称「能力位」）：缓存能力（`WB`/`WT`/`WC`/`UC`/`UCE`）；物理保护能力（`WP`/`RP`/`XP`/`RO`）；其它特征（`MORE_RELIABLE`、`CPU_CRYPTO`、`SP`、`NV`、`RUNTIME`）。**`WP` 归入物理保护能力**：规范先称它为 physical memory protection attribute，再说明它通常被用作 write-protected cacheability policy——它有特殊缓存语义，但既不是页表写权限，也不能与 `RO` 混为一谈。**`SP` 不是 no-map**：第一版把它排除出普通 PMM 是项目保守选择，不得由 `SP` 自动生成映射禁令。

**`CONVENTIONAL` 是有条件的**：它可以构成 `RamSourceDomain` 与普通主存性质，但 `OrdinaryRamEligibility`（类型层面）**不隐含缓存属性检查**；最终普通映射 = 类型资格 ∧ 缓存属性资格（第一版要求 WB），最终 PMM 分配 = 阶段资格 ∧ 缓存资格 ∧ 全部排除项。无 WB 的描述符不进入普通可分配集合，但**保留其来源身份**。

**术语边界**：这里读的是 `GetMemoryMap()` 的 `EFI_MEMORY_DESCRIPTOR.Attribute`（能力位掩码），与 `EFI Memory Attributes Table` 反映的保护配置不是一回事；第一版不解析后者，也不宣称已获知固件实际施加的全部权限。

**`EfiReservedMemoryType` 在域外但来源身份仍有效**：它不进入 `RamSourceDomain`，因此不参与 `PMMAvailable`/`PMMExcluded` 的域内分割；但与其重叠的 DTB `/reserved-memory`、MRB 原始记录仍然保留，并可独立产生映射禁止与访问禁止。没有独立平台证据时，不把它重新解释为普通主存。

**UEFI Runtime Services 的支持边界（明确声明）**：第一版**不调用 `SetVirtualAddressMap()`、不提供 UEFI Runtime Services**；`RUNTIME` 属性与 `RUNTIME_SERVICES_*` 类型只保留诊断与来源信息，不产生任何普通或运行时映射义务。将来支持时，必须**单独增加**运行时对象与 MMIO 映射义务，不得混入普通直接映射授权。

**`EFI_MEMORY_HOT_PLUGGABLE` 与语义歧义位（两个不同的位）**：

- **冲突位域整体 fail-closed（已实现）**：`0x1000..=0x1_0000` 在两套官方编码里含义不同——`0x4000` 在一套里是 `SP`（必须排除出 PMM）、另一套里是 `XP`（仅保护能力元数据）；`0x8000` 是 `CPU_CRYPTO` vs `NV`；`0x1000` 是 `MORE_RELIABLE` vs `WP`；`0x2000` 是 `RO` vs `RP`；`0x1_0000` 是 `HOT_PLUGGABLE` vs `MORE_RELIABLE`。任一读法都可能把必须排除的内存伪装成元数据，因此整个 `0x1_F000` 在已知位闸之前单独拒绝（`PolicyError::AmbiguousAttribute`，负控 `every_contested_attribute_bit_fails_closed` 与 `the_ambiguous_bit_fails_closed_under_either_name`）。
- `0x0010_0000`（仓库编码的 `HOT_PLUGGABLE`）：`uefi-raw 0.9` 无该常量，走未知位拒绝（`hot_pluggable_bit_fails_closed_as_unknown`）。
- **代价**：`NON_VOLATILE` 位现在也落进拒绝域，`TYPE_INDEPENDENT` 里的 NV 分支在真实输入上不可达——这是没钉固件 profile 时的保守选择；将来钉住带版本证据的 profile 后，可以为该编码赋予单一含义并恢复 NV 的排除路径。**位值证据按「规范仓库源文件 + EDK2」这一支取，不按 2.11 PDF 表取**：三个已发布文本与 EDK2 的属性位编码互不一致（见 03 号文档第十五节矩阵：2.11 PDF 为 `0x10000`、EDK2 为 `0x100000`，`WP`/`MORE_RELIABLE` 同样整体错位）。钉住的是「当前 `uefi-raw` 与 Linux 头文件一致的编码」这一事实，而不是某份 PDF 的表格；将来升级 `uefi-raw` 时**必须重核全部已识别属性位**，不能只迁移 hot-pluggable 一位。

**属性位证据矩阵（自包含）**：本项目取「规范仓库源文件 + EDK2 + Linux 一致」这一支，不按已发布 PDF 的表：

| 来源 | 版本/修订 | `HOT_PLUGGABLE` | `WP` | `MORE_RELIABLE` |
|---|---|---|---|---|
| UEFI 2.10 PDF | 2022-08 发布 | 未定义 | `0x1000` | `0x10000` |
| UEFI 2.10 Errata A PDF | 2024-08 | 未定义 | `0x100` | `0x1000` |
| UEFI 2.11 PDF | 2024-12 | `0x10000` | `0x100` | `0x1000` |
| EDK2 头文件 | 主干，commit 待钉 | `0x100000` | `0x1000` | `0x10000` |
| Linux 头文件 | 版本待钉 | `0x100000` | `0x1000` | `0x10000` |
| `uefi-raw 0.9` | 0.9 | 未定义常量 | `WRITE_PROTECT = 0x1000`（crate 常量） | `MORE_RELIABLE = 0x10000`（crate 常量） |

**待补证据**：EDK2 与 Linux 的具体 commit/tag 尚未钉死（`main` 会漂移，不能作为长期冻结证据）。`uefi-raw` 升级时必须重核全部已识别属性位，不能只迁移 hot-pluggable 一位。

**候选 B（不可接受，列出以说明为何否）**：只看平台物理内存描述、不看类型分类——会把不可用内存当成 backing。

**判据**：每个 backing 页能指回一个「普通主存 = 是」的来源记录，且阶段允许。

**边界**：`OrdinaryRamEligibility` 只描述来源与 OS 政策资格，**不证明某架构实际能访问该区间**——x86 还要 PAT/MTRR 有效类型、AArch64 还要 MAIR 与逐 PE 配置、RISC-V 还要 PMA 能力与逐 hart 的 PMP。硬件准入不得被 `ordinary_main_memory = true` 代替。

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

- **A（两步授权）**：① 对应启动 ABI 提供的 DTB 指针授权对**固定 header 区域**的一次性只读访问（授权主体与传递方式按每条入口分别定义）；② 校验 magic、版本、`totalsize`、回绕与范围合法性后，才授权对 `[dtb, dtb + totalsize)` 的完整读取，并复制到自有缓冲。授权范围与失效点写进启动输入合同。
- **B**：不承认该授权，则必须定义不读取原始地址也能取得 DTB 的其他机制（例如固件提供副本），否则该布局拒绝启动。

**另需裁定一致性策略**：配置表 DTB 的保留声明与 UEFI 内存图之间——(A) 解析权威 DTB，保留来源身份并与内存图交叉验证；(B) 只信内存图，接受来源身份丢失并对无法区分的区域取更保守的统一政策。第一版倾向前者，但 B 也可以是明确的简化，不是规范要求。

**四条 DTB 输入路径统一两阶段**（x86_64 UEFI 没有 DTB 输入，不属于这四条）：

- **两个独立前置条件，缺一不可**（不能合成一个「授权」）：
  - **`BootInputProvenance`**：源授权——哪个启动 ABI 把这个对象交给内核、授权读哪些字节、哪个阶段读、何时失效。启动 ABI 递指针建立的是这一条。
  - **`BootInputReachability`**：可达性——给定 PA/VA、当前地址转换状态与页表，`[dtb, dtb+40)` 这 40 字节**整段**确实可读（起点对齐不保证不跨 4 KiB 页）。
  - 两者不能互推：有指针不等于可解引用；有可读映射不等于这个地址就是启动程序授权的 DTB。
- **`ValidatedObjectReadAuthorization`**：校验 magic/版本/`totalsize`/回绕/范围后，授权 `[dtb, dtb+totalsize)` 整段只读并复制；之后政策代码只消费副本。
- **全范围可达是第二个独立检查**：拿到 `totalsize` 后必须**重新证明** `[dtb, dtb + totalsize)` 在当前转换下整段可读（覆盖所有跨页、地址转换与末端），不能拿 header 的 40 字节证明代替；复制只能在这次证明之后开始。

**复制目标不得与源重叠**：原始 DTB 不一定在 MRB 里，也未必已被分配器排除。分配自有副本前，先把原始 DTB 的完整范围登记为**存活输入对象**；源与目标范围不相交才允许复制（DTSpec 要求客户端在 DTB 使用完之前不得覆盖它，即使它不在保留区内）。于是 O-11 有四个互相独立的条件：**来源授权、地址可达、内容一致（不得并发修改）、自有副本**。

**`OwnedCopyValidation`（第三段，不能省）**：`fdt_layout::validate` 并非只读 40 字节 header——它还要扫 MRB、结构块、属性长度与字符串名称。因此完整容器校验**必须在自有副本上执行**：原始 DTB 的 **header 只在 preflight 阶段读一次**，**body `[dtb+40, dtb+totalsize)` 只在复制阶段读一次**；两部分都进入自有副本后，校验与解析只访问副本，不再访问原始地址。三段的顺序是：provenance + reachability（`[dtb, dtb+40)` 整段）→ header preflight → 完整读取授权 + 对齐副本 → 在副本上 `validate` → 解析 → `collect`。

**副本必须是一份一致快照（复制算法写死）**：`totalsize` 本身来自 header，若「先读 40 字节、再整段复制」就会把 header 读两次。算法：

1. 一次读 40 字节到局部 header snapshot；
2. 只从 snapshot 校验 magic/版本/`totalsize` 上限与加法；
3. 分配对齐的自有缓冲区；
4. 把 snapshot 写入副本前 40 字节，**只从原地址复制 `[dtb+40, dtb+totalsize)`**；
5. 在副本上执行完整 `fdt_layout::validate`。

**ABI 地址、源物理范围与读取 VA 是三个概念**：`BootInputAddress`（ABI 递交的地址及其地址域——四条路径实际交付的都是**物理地址**：AArch64 `x0`、UEFI 配置表 DTB、RISC-V `a1`/`satp=0`）、`SourcePhysicalRange`（DTB 对象实际占用的物理范围）、`ReadVirtualRange`（当前阶段实际用于读取的 VA 范围）。不构造概括性的 `SourceVirtualRange`——那会把物理地址误包装成虚拟地址。`LiveInputReservation = Outward(SourcePhysicalRanges)`——**PMM 排除的是物理页，不是虚拟区间**；目标副本与源的不重叠也必须**按物理地址**判断：源 VA 与目标 VA 数值不同不代表不落在同一 PA（直接映射、恒等映射等别名都可能重合）。

**`BootInputReachability` 是端到端可读性，不是「PTE 存在」**：在指定阶段与访问路径下，完整对象的只读访问所需的地址翻译、页表访问、物理访问保护与平台准入条件必须全部成立——RISC-V 上 PMP 可能拒绝 S 模式访问、PMA 还可能限制访问方式；AArch64/x86 各有对应门控。具体由各架构实现验证，通用层只表达「可读」语义，不暴露 PMP/页表遍历等机制名。

**输入合同**：启动程序交接后、复制完成前，DTB 剩余字节不得被并发修改（固件或其它代理都不写）。这条不能证明时必须定义一致性检测或拒绝策略——否则副本可能是不同时刻拼出的混合状态。今天代码没有复制，自然也不做这项检测（见 §2.1 的「就地消费」）。

**UEFI ↔ DTB 交叉验证是覆盖合同，不是重叠检查**：静态 `/reserved-memory` 与 MRB 在 UEFI 启动时须出现在 `GetMemoryMap()` 中，且类型须匹配（`no-map`/MRB → `EfiReservedMemoryType`，普通静态 → `EfiBootServicesData`；MRB 在规范里是 `should`，本项目可提升为强制并写明是项目策略）。验证式：

```
Outward(R_reserved) ⊆ ⋃ { 类型兼容的 EFI 描述符覆盖的页 }
```

**先做页级类型裁决，再验覆盖**：同一页可能同时被普通静态保留（规范 `must` → `EfiBootServicesData`）与 MRB（规范 `should` → `EfiReservedMemoryType`）覆盖，两种预期类型不同。裁决按页进行：

```
PageSourceClaims(page)  = { (SourceRef, RequiredEfiType, 规范强度) }
ResolvedEfiType(page)   = ResolveCompatibleClaims(PageSourceClaims(page))
```

**同页主张裁决表（第一版）**：

| 该页的 DTB 主张 | 解析结果 |
|---|---|
| 普通静态保留（`ReservedOrdinary`） | 要求 `EfiBootServicesData`（规范 `must`） |
| `ReservedReusable` | 同样要求 `EfiBootServicesData`（它是非 `no-map` 的静态保留）；生命周期与 owner 关系另行裁决，不改变类型要求 |
| `no-map` | 要求 `EfiReservedMemoryType` |
| MRB | 项目选择要求 `EfiReservedMemoryType`（把 `should` 升为项目强制） |
| `no-map` + MRB | 兼容，要求 `EfiReservedMemoryType` |
| **普通静态 + MRB** | **冲突，拒绝启动**：一条描述符无法同时满足普通保留的 `must`（BootServicesData）与本项目的 MRB 强化（Reserved） |
| **普通 / `reusable` + `no-map`（原始字节不相交但同页）** | **冲突，拒绝启动**：两条声明各自向外量化到同一 4 KiB 页，页上同时要求 BootServicesData 与 Reserved——DTB 原始范围不重叠并不豁免页级冲突 |
| **`ReservedReusable` + MRB（同页）** | **冲突，拒绝启动**：`reusable` 是非 `no-map` 静态保留（BootServicesData），MRB 按项目强化要求 Reserved；与 `ReservedOrdinary + MRB` 同构 |
| 普通静态 + `no-map`（原始字节重叠） | DTB 侧已是冲突（同范围异语义） |

**为什么不是「Reserved 更强所以覆盖」**：DTSpec 对普通静态保留指定的是类型本身，不只是「受保护」；用 Reserved 覆盖等于悄悄牺牲一条来源要求。若将来要接受普通保留 + MRB，唯一自洽的做法是**放弃 MRB 的 `should`→must 强化**（回到规范级别，MRB 的访问禁止由独立政策兑现），而不是让 `ResolveCompatibleClaims` 返回一个隐藏牺牲项的结果。跨来源身份照旧各留一条；覆盖公式分别定义 `R_static` 与 `R_MRB`，经页级裁决后再比对。

只证明「有重叠」或「首类正确」不够：一个正确的首页描述符 + 其后的 `EfiConventionalMemory` 会让保留区大部分页未被保护。反向也成立：一个兼容描述符只覆盖部分，不能把整条来源标记为已验证。必测三种形态（**未实现**）：① 仅首页类型正确、后续页类型错误 → 拒绝；② 多个连续同类描述符共同完整覆盖 → 通过；③ 中间一页类型不兼容 → 拒绝。

**启动读取是一个具名的用途，不是全局正向授权**：`BootInputReadUse { ObjectId, PA/VA, 读取范围, 只读, 阶段, 访问路径, 失效点 }`，由每个负向来源分别判断是否允许它——`BootInputProvenance` 不能覆盖所有禁止项：

| 原始 DTB 所在区域 | 一次性启动读取的裁决 |
|---|---|
| 普通 RAM | 启动 ABI 授权 + 完整范围可达即可 |
| 普通 `/reserved-memory` | 读取由启动输入对象授权决定；分配排除仍然保留 |
| MRB | 走 MRB 规则允许的、启动程序明确用途对应的受限访问授权 |
| `no-map` | **第一版拒绝**：DTSpec 的例外是「由使用该区域的设备驱动控制」的访问，一次性启动读取不属于该例外；除非实现并验证专用受控路径 |
| `EfiReservedMemoryType` | **第一版拒绝**：配置表里的指针只建立来源身份，不能推翻内存图对该范围的禁止处置；除非有专门且明确的启动 ABI 例外与规范依据 |
| `EfiACPIReclaimMemory` | **标准 `EFI_DTB_TABLE_GUID` 的唯一合规存放类型**：整段 DTB 对象所在的描述符必须是该类型才允许首读；ACPI 启用前的保留处置不变 |
| `EfiUnusableMemory` / `EfiUnacceptedMemoryType` / MMIO / 端口空间 / `PAL_CODE` | 拒绝，或走各自独立准入规则；不得借用普通 DTB 读取通道 |

**标准 DTB 表有专属规则，不能套用一般配置表的四种类型**：UEFI 2.11 §4.6.1.3 规定 DTB 必须位于 `EfiACPIReclaimMemory`，地址按物理地址解释、运行时不作地址修正。一般配置表允许的 `RuntimeServicesData`/`BootServicesData`/`ACPINVS` 对标准 DTB 表**不成立**，`LoaderData` 更不在其中。首读前必须先证明完整 DTB 对象落在 `EfiACPIReclaimMemory` 描述符内，否则第一版拒绝；其它类型只能作为明确的非标准平台扩展处理，不能写成 UEFI 标准允许的普通情况。

**EFI DTB 对象本身也是页级主张来源**：它要求 `EfiACPIReclaimMemory`。与静态 `/reserved-memory`（BootServicesData）、`no-map`（Reserved）向外量化后同页 → **冲突，拒绝**。与 MRB 同页时第一版**采用例外**：该页按 UEFI DTB 的强制要求取 `EfiACPIReclaimMemory`，MRB 的访问禁止与 PMM 排除照旧保留，一次性读取走具名 MRB 授权——这是对 MRB 类型 `should` 的**有意偏离**（遵守两条规范中更强的类型要求），不是把两种来源合并。

这张表**只**决定「一次性只读能否发生」，不修改 `RamSourceDomain`、`PMMAvailable` 或 `OrdinaryMappingEligibility`。

**与 O-03 的接口**：若 DTB 本体落在 MRB 覆盖范围内，上面这次只读访问必须登记为**MRB 规则自身允许的、由启动程序明确提供的一次性对象访问授权**（带对象身份、阶段与只读路径）。要否定的是「无边界豁免」——授权不覆盖所在 MRB 区域的其它访问，也不授权复制完成后继续访问原地址；但也不应把 DTB 读取说成「不属于任何例外」，那与规范允许的显式授权相矛盾。

**实现状态：未实现**。见 §2.1——今天校验与解析都在固件提供的原地址上就地执行。

**副本生命周期**：原始 DTB 只在复制期间可读；自有副本登记进对象目录；切根前不得回收或覆盖；副本的最终释放必须有明确消费者与阶段。边界条件：header 固定读取长度写明；`totalsize` 有项目上限；`dtb + totalsize` 必须 checked；完整范围必须证明可安全读取（不能只验证起始地址）。

**重叠时的 EFI 类型裁决**：同一 EFI 页被多条来源覆盖时，先按来源语义生成逐页类型主张，只有裁决表明确认兼容的主张才能合并——普通静态保留（`must` → BootServicesData）与 MRB 强化（Reserved）同页时**拒绝启动**，规范强制要求不得被另一个来源的项目强化静默覆盖；来源身份分别保留；无法同时满足所有来源约束则拒绝启动。规范强度写明：静态 `/reserved-memory` 入图与类型对应是明确要求；MRB 入图是明确要求，但用 `EfiReservedMemoryType` 的措辞是 `should`（项目可提升为严格合同，不得写成 must）。

**交叉验证覆盖两种来源**（不能只验 `/reserved-memory`）：

| 权威 DTB 来源 | UEFI 内存图预期 |
|---|---|
| 普通静态 `/reserved-memory` | `EfiBootServicesData` |
| 带 `no-map` 的静态 `/reserved-memory` | `EfiReservedMemoryType` |
| MRB 条目 | `EfiReservedMemoryType` |
| 动态 `size` 保留区 | 不应列入内存图（规范要求）；`reg` 与 `size` 并存时按静态 `reg` 处理 |

**MRB 与普通声明重叠时**：交叉验证只核对覆盖与兼容性；政策层保留两个独立来源身份，不得合并成一个更大的区间后丢掉语义差异。

**判据**：该腿能指出保留声明的实际读取点；若 DTB 在 MRB 内，能指出授权依据（候选 A）或拒绝依据（候选 B）。

**第一个消费者**：aarch64 UEFI 腿的保留政策。

### O-12 集合构造与来源完整性（总括，拆成可独立签字的子合同）

**问题**：禁止映射集合、禁止访问集合、设备窗口集合、PMM 累积器今天的构造式、消费者与容量都没有定义；它们来源不同，不应捆绑成一个裁决。

- **O-12a** 引导根禁止映射集合：每条禁止项可追溯到一个来源（`no-map` / MRB / 类型 / 对象）。
- **O-12b** 最终根禁止映射集合：同上，且包含对象负向义务的投影。
- **O-12c** 设备窗口授权：见下节；判据是**每页都能指回一条有效的设备映射政策授权**。
- **O-12d** PMM 排除累积器：与可用表互补（对来源域）；每来源贡献一次。
- **O-12e** 容量与失败形状：超限拒绝启动；两遍扫描；每份输出独立容量。
- **O-12f** `RamSourceDomain` 与 `OrdinaryRamEligibility` 分开定义（四条入口各一组规则）。
- **O-12g** 采集器重构：保留原始来源记录，再派生投影。
- **O-12h** UEFI 描述符结构校验与政策分类分开（见下）。
- **O-12i** 每种政策投影的页粒度规则（见下）。
- **O-12j** 三层：**a 容器与 MRB（已实现）**——magic、`totalsize`（≤64 MiB）、blob/`off_mem_rsvmap` 8 字节对齐、`struct`/`strings` 块 checked 边界与互不重叠、结构块 4 字节对齐并以 `FDT_END` 结尾、MRB 终止项落在 `min(off_dt_struct, totalsize)`、**MRB 条目上限 16**、FDT 版本 `17/16`（项目子集）、块顺序 MRB → structure → strings（项目输入限制）。**b 结构块预扫（已实现，消费能力已验）**——无分配 token 预扫：token 合法性、单根、嵌套平衡、节点名/属性长度与名称偏移边界、**属性必须先于子节点**、深度 ≤63、`FDT_END` 唯一且最后。**c 目标节点语义（未闭合）**——`/memory`、`/reserved-memory` 与设备 `reg` 的解析与地址转换仍需明确失败形状（含动态 `size` 声明的拒绝形状）。

**O-12j-b 的两条边界必须与解析库实现对齐，不能只按格式规范写**：

- **属性序**：`fdt 0.1.5` 的 `properties()` 在首个非属性 token 处停止；`children()` 与 `all_nodes()` 在「子节点之后的属性」处**直接结束迭代**。因此这种布局不是「容忍即可」，而是会把子树静默截断成部分树。预扫改为拒绝（`PropertyAfterChild`）。此前的「QEMU/OpenSBI 会产出这种布局」结论**已撤回**：QEMU 8.2.2 `dumpdtb` 全树 0 处违规，且 riscv（OpenSBI 打补丁链）与 aarch64 UEFI 两条 QEMU 腿在「拒绝」规则下均 PASS——当时的观测来自有缺陷的临时扫描器，不是固件事实。**该「双 PASS」是早期提交上的历史读数**（riscv `test-kernel-image-riscv64-bootface.sh` 只覆盖 bootface；aarch64 `test-shim-bootmarks-aarch64.sh` 自 `2d292a06b` 起被 `E0046` 阻断，当前基线只有 riscv PASS），引用时必须带提交与脚本，不能当作当前基线证据。
- **深度**：`all_nodes()` 的父栈是 `[&[u8]; 64]`，但根节点存在下标 1（`parent_index += 1` 后才写入），所以可安全走完的最大深度是 **63**（含根）。上限从 64 收到 63，并用 `the_parser_consumes_the_deepest_tree_this_gate_accepts` 证明「门接受的最深树，解析器确实能走完」。

**O-12j 状态须按代码状态与合同状态两列读**：

| 子项 | 代码状态 | 合同状态 |
|---|---|---|
| a 容器与 MRB | 已实现 | 已实现；后续输入变化仍需复审 |
| b 结构块预扫 | 已实现 | 部分闭合：跳转表与「解析器可消费」已证；**预编译依赖闭包中是否含跳转表未证**（`-C jump-tables=no` 只作用于本项目编译的 crate） |
| c `/memory` 语义解析 | 已实现（status/hotplug 准入、重复去重、部分重叠拒绝） | 部分闭合 |
| c `/reserved-memory` 静态 `reg` | 已实现（三语义、`ranges` 空、cells 一致、重复/重叠/冲突三分） | 部分闭合 |
| c 动态 `size` / `alignment` / `alloc-ranges` | 未实现 | 未闭合：今天落入统一 `UnsupportedReservedDeclaration`，缺专用错误与 `size`-only 测试（DTSpec：`reg` 与 `size` 并存按 `reg`，动态区不入 UEFI 内存图） |
| c 设备 `reg` / 多级 `ranges` 转换 | 未开始 | 未闭合 |

**早期执行闭包（EarlyExecutionClosure）合同**：一段代码在某阶段执行之前，必须证明它的 `Instructions ∪ ReadOnlyDataDependencies ∪ RequiredWritableState` 在该阶段的地址转换与权限下全部可达；只证明入口指令可执行不够。实证：预扫逻辑位于已映射页内，但 `switch` 被编译成 `.rodata` 跳转表 + `jr` 绝对地址，触发 RISC-V `instruction access fault`（`cause=1`，`epc` 落在 `validate_structure` 内）。

**跳转表禁用的落地方式**：本工具链（rustc 1.94.1）的选项拼写是 **`-C jump-tables=no`**（`-C no-jump-tables` 在该版本不存在）；`RUSTFLAGS` 会**覆盖** target config，因此镜像构建器（xtask）在注入的 `RUSTFLAGS` 中同时带上它，config 另为 riscv64 与 aarch64 直启目标保留一份。反汇编确认 `validate_structure` 只剩直接 `jalr`；去掉该选项时原故障复现。**边界**：预编译的 `core` 不受该选项影响，其代码路径仍需单独核对。

#### O-12c 的输入：三个概念要分开

| 概念 | 回答的问题 | 允许的来源 |
|---|---|---|
| `DeviceResource` | 设备资源在哪里 | DTB 设备节点 `reg`（经每级 `ranges` 转换）、ACPI、平台信息 |
| `DeviceWindowPolicy` | 哪个窗口允许被哪棵树、哪个阶段映射 | **可审计的平台/设备政策**，不能由一段地址自动推导 |
| `DeviceAccessRequirement` | 哪个使用点经什么路径访问 | 对象/设备使用点登记 |

**`reg` 只是资源位置描述，不是映射或访问授权**：它不推出「必须映射」「属性一定是 DEVICE」「任意代码可访问」「应进入恒等映射」。

| 入口 | 设备资源来源 | 今天实际生效的授权事实源 |
|---|---|---|
| x86_64 UEFI | ACPI（未解析）或平台静态表 | 内核内静态常量表 |
| aarch64 UEFI | 同上 | 静态常量表 |
| aarch64 直启 | DTB 设备节点 `reg` + 每级 `ranges` | 静态常量表 |
| riscv64 | DTB 设备节点 `reg` / OpenSBI 平台信息 | 静态常量表 |
| 端口 I/O | **不属于物理设备窗口**，走 O-08 | 独立地址域 |
| UEFI 类型 11/12 | **不作为设备授权来源**（11 是运行时服务 MMIO 请求；12 有 Itanium 限定） | 不产生窗口 |

**静态表必须作为今天的事实源被清点**。

**四层关系**（不是两层）：

| 概念 | 含义 | PLIC 例子 |
|---|---|---|
| `DeviceAperture` | 平台/设备描述声明的完整地址窗口 | DTB `reg`：`[0x0C00_0000, 0x0C60_0000)`（6 MiB） |
| `DeviceRequiredAccessEnvelope` | 当前内核实际需要访问的寄存器范围 | `[0x0C00_0000, 0x0C24_0000)`（按钉住的配置推导） |
| `DeviceWindowPolicy` | 授权某个范围以指定属性映射的政策 | 本项目静态表 |
| `DeviceMappingCoverage` | 页表实际叶子覆盖的范围 | `[0x0C00_0000, 0x0C40_0000)`（2 MiB 叶子包络） |

约束是：**每个 `DeviceAccessRequirement` 的地址都落在对应 `DeviceMappingCoverage` 内**；**coverage 的每一页都要有兼容的 `DeviceWindowPolicy` 授权**；未使用的 aperture 可以不映射；包络超出所需访问范围的部分仍要逐页通过授权与属性兼容检查。**不再写 `DeviceMappingRange ⊇ DeviceResourceRange`**——那会把「整个设备 aperture 必须映射」错当成普遍不变量。

| 窗口 | `DeviceAperture`（设备声明） | `DeviceRequiredAccessEnvelope`（实际访问点） | `DevicePolicyAuthorizedRange`（政策授权） | `DeviceMappingCoverage`（预期；观测待实现） |
|---|---|---|---|---|
| LAPIC（**xAPIC MMIO 分支**） | **未知**（ACPI 未解析） | `[0xFEE0_0000, 0xFEE0_1000)` | `[0xFEE0_0000, 0xFF00_0000)` | 同授权范围 |
| LAPIC（**x2APIC 分支**） | 不适用：x2APIC 用 MSR 访问，MMIO 接口不可用 | **不产生 MMIO 访问需求，也不得伪造包络** | 无 MMIO 授权 | 无 |
| IO-APIC | **未知**（同上） | `[0xFEC0_0000, 0xFEC0_1000)` | `[0xFEC0_0000, 0xFEE0_0000)`（**与 LAPIC 相邻，不是同一个包络**） | 同授权范围 |
| 平台外设窗口 | 32 MiB（QEMU virt profile） | 从已登记的设备访问点派生；**未登记的设备不产生访问需求** | `[0x0800_0000, 0x0A00_0000)`（有意授权整段） | 32 MiB |
| PLIC | `[0x0C00_0000, 0x0C60_0000)`（DTB `reg`，6 MiB） | `[0x0C00_0000, 0x0C24_0000)` | `[0x0C00_0000, 0x0C40_0000)` | 同授权范围 |
| `RiscvVirtPlatformMmioWindow` | 平台窗口（见下） | UART 控制台寄存器偏移（资源 256 B） | `[0x1000_0000, 0x1020_0000)`（2 MiB 平台窗口） | 同授权范围 |

**2 MiB 包络的授权对象是平台窗口，不是 UART**：QEMU virt 的 `[0x1000_0000, 0x1020_0000)` 同一段里还有 VirtIO MMIO（`0x1000_1000` 起）与 fw_cfg（`0x1010_0000`），所以这条政策登记为 `RiscvVirtPlatformMmioWindow`，UART 只是窗口内的一个资源条目（`DeviceAperture` = 256 B）。**通用规则：大页包络的授权对象必须是产生该包络的完整平台内存窗口，而不是包络里恰好存在的一个设备资源。**

**`RiscvVirtPlatformMmioWindow` 的资源清单**（QEMU virt，按 profile 记录）：UART0 `0x1000_0000`+`0x100`、VirtIO MMIO 8 个窗口自 `0x1000_1000` 起每 4 KiB、UART1 `0x1000_A000`+`0x100`（**仅在使用第二个串口时实例化，属 profile 条件资源**）、fw_cfg `0x1010_0000`+`0x18`。资源存在不等于访问需求：没有 `AccessUseId` 的设备不产生 `DeviceRequiredAccessEnvelope`。

**每条 `DeviceWindowPolicy` 的完整记录**（缺任一字段就只是地址白名单）：允许映射的整页范围、OS 语义属性（普通主存/设备）、`R/W/X`、共享性、适用树、适用阶段、`ProfileId`、证据出处；记录 OS 语义，不写 PAT/MAIR/PBMT 这类架构编码。

**四个数据不是同一件事**：`Aperture` 来自设备描述；`RequiredAccess` 来自已登记的访问点（`reg` 不能证明实际访问）；`PolicyAuthorizedRange` 来自预先审查的平台政策；`Coverage` 来自实际 PTE 遍历。审计公式：`Coverage ⊆ AuthorizedRange` 且 `RequiredAccess ⊆ Coverage`。

**观测记录必须带 VA，不能只有 PA**：`DeviceMappingCoverage` 若只是物理区间，就回答不了「这些页映射到哪个 VA」「访问点用的 VA 是否落在里面」「同一 PA 是否有多个别名、权限/属性/共享性是否兼容」。观测单元定义为 `ObservedDeviceMapping { VaRange, PaRange, LeafSize, Permissions, MemoryAttribute, Shareability, Tree, Phase }`，由它派生纯物理覆盖 `ObservedPaCoverage = Union(PaRange)`，审计仍写 `ObservedPaCoverage ⊆ DevicePolicyAuthorizedPaRange`。访问需求不能只拿 PA 包络比覆盖：每条 `DeviceAccessUse` 要能证明其访问 VA 经对应叶子解析到预期 PA，且宽度、读写与属性兼容。这与 O-05 的别名审计是同一条要求——不能在要求审计别名的同时让覆盖集合丢掉 VA。

**数值相等 ≠ 来源相等**：表中若干行的授权范围与覆盖范围数值相同，但授权是**独立政策输入**，覆盖是**独立测量**；不能由实际叶子反推授权。而且今天**没有页表审计遍历**，所以今天的覆盖只能叫 `ExpectedDeviceMappingCoverage`（从构造代码推导），`ObservedDeviceMappingCoverage`（PTE 遍历）等 O-02a 的引导根审计落地后才存在。表中「覆盖」列在观测实现前一律标为**预期**。

**x2APIC 是第三种访问域**：xAPIC 走 MMIO（`0xFEE0_0000` 页），x2APIC 走架构寄存器（RDMSR/WRMSR）——它既不属于 MMIO 窗口模型，也不属于 O-08 的端口 I/O 地址域。**MSR 编号不进共享层**：共享层只表达 `ApicRegisterAccessRequirement`（APIC 逻辑寄存器、读写方向、访问者、阶段、CPU 范围、适用模式）；x86 架构层用 `X2ApicMsrUse` 记录具体 MSR 编号、指令位置、包装函数与访问约束。同一 OS 语义（读 Local APIC ID、写 EOI）在 xAPIC 映射到 MMIO、在 x2APIC 映射到 MSR。**第一版若不实现 x2APIC 访问路径**，必须锁定 xAPIC profile 并在检测到 x2APIC 时拒绝启动；不得「允许 x2APIC 模式却保留 xAPIC 的 MMIO 访问需求」。O-10 现有的 in/out 负控不覆盖 RDMSR/WRMSR，需另列架构寄存器访问审计。

**x86 的 APIC 地址不是无条件硬件事实**：表中 `0xFEE0_0000`/`0xFEC0_0000` 只属于「本项目支持的某个 x86 QEMU profile」。启动时必须核对：Local APIC 当前模式（xAPIC MMIO 还是 x2APIC MSR 路径）、`IA32_APIC_BASE` 的基址与政策表一致；若解析 ACPI，MADT 的 Local APIC 地址、override 与 I/O APIC 项必须与政策表兼容；不符合当前 profile 就**拒绝启动**。不解析 ACPI 时，支持范围严格锁定在明确的 machine profile。

**PLIC 的访问集合与包络**：`DeviceAccessUseSet` 的每条记录至少含 `AccessUseId`、`DeviceId`、`ObjectId`、**`VirtualByteRange`（或其可静态解析出实际 VA 的访问点标识）**、`PhysicalByteRange`（预期 PA）、访问宽度、读/写、阶段与路径。**缺 VA 就无法证明访问需求**：指针实际落在哪个别名上，只有 VA 能回答；不能先由现有 PTE 推出 PA 再充作访问需求输入（那是循环证明）。`DeviceRequiredAccessEnvelope = Envelope(DeviceAccessUseSet)`，**不表示包络内每个 byte 都被访问**。没有完整访问清单时，包络只是保守上界，不能宣称访问需求已完整（与 O-07 相连）。PLIC 的 profile 还必须钉住具体的 QEMU 构建版本（`qemu-system-riscv64 --version` 读数与 machine version）；今天尚未记录该版本，属于待补证据。

**静态窗口必须绑定一个 `PlatformMemoryProfile`**：每个 profile 至少含 `ProfileId`、QEMU 构建版本、machine type/version、accelerator、CPU 型号、SMP 拓扑、窗口政策集合、预期资源集合与不支持的配置。表格只引用 profile ID；x86 APIC、aarch64 32 MiB、riscv PLIC/UART 四类窗口都适用——今天四者的 QEMU 构建版本都**尚未记录**，属于待补证据。

**aarch64 的 32 MiB 是 `DeviceWindowPolicy` 窗口，不是一个设备的资源**：它表示钉住的 QEMU virt profile 对该整段授予 `DEVICE` 映射资格；其中哪些设备、哪些空隙、哪些区段当前没有访问点，由资源清单与访问需求另行解释，不能由设备名称反推。

**PLIC 范围的推导与 profile 条件**（合并上一版两段重复表述）：`N_context = hart_count × 2` 不是通用公式——QEMU TCG 用奇数 context ID（`2·cpu+1`），KVM 用连续编号；不支持 S-mode 的 hart 还会造成 context 数差异。所需范围写成 `ContextBase + (max(实际使用的 context ID)+1) × Stride`；context ID 不能由 hart 数直接乘 2，也不能把 `interrupts-extended` 的 interrupt specifier 当成 PLIC 的 MMIO `addrid`。推导链：specifier → 解析 `interrupt-parent` phandle → 识别 hart 与特权模式 → 确定 context 排列 → 枚举稠密 `addrid` → 最大偏移。本项目的 `0x24_0000` **只在以下配置下成立**：QEMU `virt` + 固定 machine version + TCG + 单 socket + AIA 关闭 + 明确 CPU 型号 + `-smp ≤ 32` 且全部支持 S-mode；其它配置必须拒绝或重新推导。

**PLIC 窗口的教训（本轮修正）**：context 区从 `+0x20_0000` 开始，窗口大小应为 `0x200000 + N_context×0x1000`，**不能写固定 2 MiB**——原来的 2 MiB 恰好结束在 context 区起点，claim/complete 寄存器拿不到 `DEVICE` 映射。QEMU 实测 DTB：`plic@c000000` 的 `reg` 为 `0xc000000 + 0x600000`。窗口按内核 SMP 上限 32 hart 取 `0x24_0000`，仍在 QEMU 的 PLIC 区间内。

这些条目今天来自编译期常量（`kernel_identity_mmio_regions()` 与 `MMIO_PA_BASE/MMIO_WINDOW_LEN`），**不是** DTB/ACPI 解析结果。

**每条窗口必须绑定可验证的版本与动态规则**：QEMU 版本或 machine version、固定地址依据、动态范围的计算式、与 DTB `reg` 的一致性检查、不支持的配置范围。窗口表复制自某个 QEMU 版本时，不能继续当作另一个版本的硬件事实。**DTB 与静态表不一致时的规则**（拒绝启动 / 以哪一方为准）必须写明；第一版倾向拒绝。

#### O-12k UEFI 内存图快照与 MapKey 一致性（未实现）

`GetMemoryMap()` 返回的 `MapKey` 随内存图变化而变化，`ExitBootServices()` 必须使用**当前**内存图的 key；key 失效时须重新取图、重试。这与 O-11 的自有副本、对象登记表、诊断缓冲区的分配直接冲突：每次 Boot Services 分配都可能改变内存图。

**合同**：

1. 所有可能改变 UEFI 内存图的 Boot Services 分配（DTB 自有副本、引导对象缓冲、登记表、诊断区）**先完成**；
2. 最后取一次 `GetMemoryMap()`，保存 `MapKey`、`DescriptorSize`、`DescriptorVersion` 与**完整描述符快照**；
3. 用这份快照生成来源记录与政策投影；从取图到成功 `ExitBootServices()` 之间**不再执行会改变内存图的操作**；
4. `ExitBootServices()` 因 key 失效失败 → **重新取图、重建来源记录与政策投影、重新审计**后再重试；不得沿用旧快照的 `PMMAvailable`；
5. 重取后描述符数量与分段可能变化：容量不足、描述符增多、重试耗尽都要有明确失败形状，不能假设「只差一个 MapKey」。

**重试阶段的执行环境必须预先约束**（规范允许首次 `ExitBootServices()` 失败后固件已部分关闭 Boot Services）：

- 首次调用**之前**，重试所需的一切已分配：内存图缓冲区、分类记录、政策投影、审计工作区、诊断状态；
- 首次失败后，重试路径**只使用预留缓冲区与纯内存计算**：不调用文件系统、控制台、设备协议、事件服务或任何依赖完整 Boot Services 的功能；
- 复用同一内存图缓冲区，并预留描述符增长余量（规范明确：分配新缓冲区本身可能增大内存图）；重取返回 `EFI_BUFFER_TOO_SMALL` 时第一版**直接以明确错误终止**，不在部分关闭状态下扩容；
- 新快照必须**重建**全部依赖描述符的结果（来源记录、资格集合、PMM 投影、审计意见）；`MapKey`、记录与投影属于同一快照版本，不得混用；
- 只对 `EFI_INVALID_PARAMETER` 走 stale-key 重试；其它错误走独立失败路径；重试次数有上限；
- **重试期间不打印日志**：错误路径不得调用 Console 或协议服务取诊断——那正是首次失败后被限制的环境。诊断输出只在交接成功后由自有设施进行。

重试内核是纯函数式阶段，逐级重建：

```
GetMemoryMap(reused_buffer) → ValidateDescriptorSnapshot
  → RebuildSourceRecords → RebuildPolicyProjections → Audit
  → ExitBootServices(new_map_key)
```

**必测负控**（未实现）：① stale key 后新快照描述符分段变化（一分为二/类型变化）→ 投影必须按新快照重建，不得沿用旧 `PMMAvailable`；② 重试缓冲不足 → 明确错误；③ 非 `EFI_INVALID_PARAMETER` 错误 → 独立失败路径；④ 重试次数耗尽 → 明确错误；⑤ 失败后的审计路径没有调用 Boot Services 协议或动态分配器。

**为什么单列**：它约束的是 UEFI 输入快照的**时间一致性**，不是描述符几何合法性（O-12h）；两者失败形态不同。

#### O-12h 的两级结构校验

| 层级 | 检查 |
|---|---|
| 内存图元数据 | `DescriptorVersion` 受支持；`DescriptorSize` 不小于已知前缀；`MemoryMapSize` 能被 `DescriptorSize` 整除**且不超过调用方缓冲区长度**；按 `DescriptorSize` 步进；字段读取不越出该条前缀 |
| 每个描述符 | `PhysicalStart` 对齐且不超上界；`VirtualStart` 按规范校验（**`0` 可接受**，它满足对齐；第一版验证后忽略）；`NumberOfPages` 非零，页数约束**分别对物理与虚拟起点**计算；两个末端都用 **u128**；**不论类型是否输出区间都执行** |
| 跨描述符 | **物理区间不得重叠**：两个描述符覆盖同一页却给出不同政策时，分类器无法给出唯一结果，必须拒绝启动 |
| 顶端边界 | **结构合法性与平台准入分开（已裁定，未实现）**：`[0xffff_ffff_ffff_f000, 2^64)` 是合法的最后一页，UEFI 只约束「页起始地址不超过 `0xffff_ffff_ffff_f000`」，不要求独占末端能用 `u64` 表示。结构校验用 `u128` 接受 `end == 2^64`；是否可用由平台地址准入层判定（`effective_pa_limit`、PMA/PMP 等），拒绝时用独立错误码。**今天的代码仍把它当结构错误拒绝（`RangeOverflow`），这是要改掉的行为**——不能把合法 UEFI 输入伪装成算术回绕 |
| 版本策略 | 只接受 `DescriptorVersion = 1`；其它版本明确拒绝（不是「受支持」的模糊说法） |
| 顶端页测试（待补） | `PhysicalStart = 0xffff_ffff_ffff_f000, Pages = 1` 通过；同起点 `Pages = 2` 拒绝；`VirtualStart` 同两点；`VirtualStart = 0` 允许但页数仍须满足虚拟上界；**以上对不进入任何分类输出的类型同样执行** |

#### O-12g 的附加要求

原始记录保留：声明所在节点、**每一级 `ranges` 转换的输入/输出与引用节点**、最终 CPU 物理地址、属性与来源身份。只保留最终地址会让审计无法解释地址是怎么来的。

**四条补充要求**：① **排除原因多重归属**——同一页可同时被普通保留、MRB、存活对象覆盖，PMM 只扣一次，但审计能列出全部 `SourceId`；② 固件前缀、内核镜像、boot shim、自举页表、栈、启动模块、DTB 原始对象与副本、登记表**不能合并成一个 Reserved 标记**，它们各自有阶段化读写/执行需求；③ **重叠规则**：**同一权威来源内部的同语义重复**（例如两个 `/memory` 节点描述同一段 RAM，或同一节点多个 `reg` 重复）集合去重、保留多个 `SourceId`；**跨来源证据**（UEFI 类型、平台分类、固件保留与 RAM 来源）对同一物理区域提出**不兼容主张**时进入冲突裁决，不能靠并集掩盖——不兼容即拒绝启动。`/memory` 本身只有 RAM 语义（`status` 非 `okay` 与 `hotpluggable` 已被准入拒绝），不存在「同源一条说 RAM、一条说不可用」的形态；④ **原始记录必须保留不输出区间的描述符**（含 `RESERVED`/`UNUSABLE`）——否则 O-06 的「`UNUSABLE` 属于来源域」没有可实现的输入；诊断成员表是面向内核的另一层，不能替代原始记录。

**状态按三层记（O-12g）**：

| 层次 | 状态 | 证据 |
|---|---|---|
| 库接口：能产出带身份的记录 | **已实现（第一阶段）** | `SourceRecord { id, kind, origin, range_start, range_end }`；`class()` 区分输入声明与派生 `FirmwarePrefix`；前缀带 `{ memory_id, cut }` 派生链；精确重复声明去重投影、保留两条来源记录；容量由输入上限推出，恰好 40 可达 |
| 启动路径：真实启动生成并保留记录 | **未接入** | 四条 DTB 调用点全部走 `collect()`，记录被丢弃（§2.5） |
| 消费者：政策投影实际引用记录 | **未接入** | `PolicyProjection` 尚无代码 |

**已解决的旧冲突（按来源分治）**：`/reserved-memory` 的精确重复按**三种语义**（普通/`reusable`/`no-map`）分别去重、来源各留一条，跨语义同范围一律冲突；MRB 的精确重复**必须拒绝**（DTSpec 5.3.2）；`/memory` 的部分重叠按项目策略拒绝。六条测试见 §2.1。

**合法记录与拒绝诊断记录分开**（未实现）：结构校验失败的描述符不进入来源记录层，但必须留下**拒绝诊断记录**（`DescriptorIndex`、原始 `Type`、`PhysicalStart`、`VirtualStart`、`NumberOfPages`、失败检查项）——结构错误发生时恰恰最需要来源信息，不能因为「没通过校验」就只剩一句错误码。

**UEFI 原始描述符记录的最小字段**（未实现）：`DescriptorIndex`、原始 `Type`、原始 `Attribute`、`PhysicalStart`、`VirtualStart`、`NumberOfPages`、结构校验结果；每个通过结构校验的描述符先成为带身份的记录，再进入类型裁决——裁决可以拒绝启动，但不能在来源层消灭 `RESERVED`/`UNUSABLE`/MMIO/类型 11/12；导致拒绝的 `PAL_CODE`/`UNACCEPTED` 至少要在错误上下文保留索引、类型与范围。

**仍缺的字段**：节点路径（今天只记 `origin` 类别 + 序号，多个 `/memory` 节点靠 `id` 区分，没有路径文本；序号的意义绑定输入对象与遍历顺序，跨对象不可解释）、每级 `ranges` 转换链、`SourceRef` 的统一类型（今天只有顶层 `id` 与 `FirmwarePrefix{memory_id,cut}` 的内嵌引用，尚无统一类型）、UEFI 侧未输出描述符的记录。顺序不变：先把记录接到启动路径并由政策层持有，再谈 O-02/O-03 的禁止集合编译；否则 DenyStandardSystemMapping、DenyUncontrolledSpeculativeAccess、MRB 访问禁令与「同页多重排除理由」都没有输入来源。

**记录层与原始输入对象分开**：原始记录保真**不等于**永久保留整份 DTB 或 EFI 内存图缓冲区。`SourceRecord` 是规范化记录（保留到政策投影与审计不再需要）；原始 DTB 副本只在解析与转换期间需要，最后一个消费者结束后可以释放。`ranges` 转换链（每级输入/输出地址、引用节点、转换属性）属于记录层，不能只留最终物理地址。

#### O-12i 每种投影的页粒度规则

| 投影 | 边界处理 |
|---|---|
| `RamSourceDomain` | 保留原始 byte range；只有构造页域时才说明边界处理 |
| `PMMAvailable` | 只交付完整落入合法 RAM 的整页（向内收缩） |
| `PMMExcluded` | 对保留范围**向外覆盖整页**，部分相交的页不得进入 PMM |
| `no-map` 映射禁止 | 向外覆盖所有相交页，防止大页/小页叶子跨入禁止区域 |
| MRB 访问禁止 | 保留精确 byte range；若通过页表落实，另做页粒度投影 |
| 设备窗口映射 | 叶子覆盖的**全部页**都要有兼容授权，不能只看起始页 |

**所有 `PMM*` 集合都是 4 KiB 页集合**：`TypeAllocatablePages(p)` 表示「整页都满足阶段 `p` 的类型资格」的页；`PlatformAllocationDenials(p)` 也必须是页集合（或先向外量化）。来源原始范围仍是 byte range，只有进入指定投影时才量化；**阶段资格变化必须由显式事件触发**，不得随时间推进自动释放。

**`FullPages` 以页编号定义**：

```
PageBytes(n) = [ u128(n) * 4096, (u128(n) + 1) * 4096 )   ← 右端独占
FullPages(R) = { n | PageBytes(n) ⊆ R }
```

参数是页编号而不是物理地址或 byte range；`0 ≤ n < 2^52`（4 KiB 页、64 位物理地址），边界先转 `u128` 再运算，最高页的独占末端可以是 `2^64`。**两端都是半开区间**，与全文集合模型一致。只有整页 byte range 都被 RAM 来源覆盖才进入页域，不是「起点向上、终点向下后拼接」。

**页域的正式定义**：

```
RamSourcePageDomain = FullPages(Union(RamSourceDomain))      ← 先并集，再取完整页

PMMAvailable(p) = RamSourcePageDomain
    ∩ TypeAllocatablePages(p)
    ∩ CacheAttributeEligiblePages(p)
    \ Outward(ExplicitAllocationReservations(p))
    \ Outward(LiveObjectRanges(p))
    \ PlatformAllocationDenials(p)

PMMExcluded(p) = RamSourcePageDomain \ PMMAvailable(p)
```

**`CacheAttributeEligiblePages(p)` 必须单独成项**：第一版要求 WB，无 WB 的 `CONVENTIONAL` 描述符不进普通可分配集合——如果只写 `TypeAllocatablePages`，这类页仍会出现在公式结果里，与文字结论矛盾。UEFI 路径的该项来自描述符属性资格；DTB 路径来自平台内存属性政策（不得为共用公式伪造 UEFI 属性记录）。普通映射授权同样要有自己的属性兼容检查，不能只看 `OrdinaryRamEligibility`。

**四类排除原因必须分开，不能塞进一个未定义的 `AllocationReservations`**：① 类型资格（`UNUSABLE`/`UNACCEPTED`/`ACPI_NVS`/`Runtime` 等在阶段 `p` 是否允许分配）——构造规则是**先裁决、再取整页**：

```
ResolvedTypeEligibleRanges(p) = ResolveCompatibleTypeClaims(SourceRecords, p)
TypeAllocatablePages(p)       = FullPages(Union(ResolvedTypeEligibleRanges(p)))
```

不能对每条描述符单独 `FullPages` 再并集：两条相邻、语义兼容的来源应能合成一个整页；一条普通 RAM 与一条不可用类型相邻或重叠，则整页不合格。`TypeAllocatablePages(p)` 由「普通主存 backing 资格 + 阶段分配资格」共同派生，不是只看 EFI 类型数字；② 显式分配保留（`/reserved-memory`、MRB、固件前缀）；③ 存活对象（启动模块、页表、DTB 副本、栈、登记表）；④ 平台拒绝。反例：一条 `UNUSABLE` 描述符没有 DTB 声明，若只看显式保留，它会被算进可用表——与 O-06 冲突。

先并集再取整页，避免两个非页对齐的相邻 RAM 区间各自向内收缩时白丢一页；来源记录与身份**不合并**，只在纯计算层构造集合。同一页被多条排除原因覆盖时，PMM 只排除一次，但**所有排除理由都可追溯**（见 O-12g 的多重归属）。

**原始声明、RAM 交集与政策投影是三个范围**：`RawDeclaredRange` 是输入声明的完整字节（不裁剪、不取整）——DTB 侧已落为 `SourceRecord` 的 `range_start/range_end`（派生记录的该字段是派生结果，不是原始声明），记录本身**不携带任何页范围**；`RamIntersection` 是与 RAM 来源域相交、按页向外量化的部分，用于 PMM 投影；`PolicyProjection` 是按 `no-map`、MRB、对象各自语义生成的禁止范围。**MRB 即使超出 `/memory` 也不能丢**：它的访问禁止语义不因为范围不在 RAM 内而消失。`no-map` 与标准系统映射相交时由独立的映射禁止投影处理；若项目只支持与 RAM 相交的 `no-map`，超出部分必须是显式拒绝条件，不能静默裁剪。今天两张投影表只表达 `RamIntersection`，`PolicyProjection` 尚无消费者——这正是 O-12g 记录层要先行的原因。**端点统一**：DTB 记录用 `u64` 独占端，`reg` 回绕目前以 `RangeOverflow` 拒绝；UEFI 描述符路径用 `u128` 接受 `end == 2^64`。两条腿的差别应是显式项目策略（单列 `UnsupportedPhysicalAddressEnd`），而不是数据类型偶然造成的差异——列为待改。

**MRB 的两种粒度**：精确 byte range 用于访问义务与审计；要通过页表保证不可访问时，另算相交页级范围，并说明「只覆盖一页中部分字节」时该页在其它对象的普通路径中是否允许映射。

**设备窗口的边界页负控（必须有测试）**：资源 `[0x1003, 0x1fff)` 若直接向外量化成 `[0x1000, 0x2000)`，授权就扩张到了相邻地址。边界页只能三选一：拒绝非页对齐资源；明确授权整页并证明邻接区的属性/权限/别名兼容；拆分后仍无法表达则拒绝启动。

### O-13 固定布局 ABI（第三层）

字段编码、偏移、容量、版本与失败形状都未定；现在只有两张区间表的布局。等 O-12 的子项定型后，按真实消费者设计，不在集合未定时冻结。

**判据**：每个跨界记录有唯一消费者、容量核算与失败形状。**消费者**：内核消费方。

## 6. 证据与复核

| 证据 | 内容 |
|---|---|
| 提交 `38668f843`（**功能落码提交，不是测试基线**） | DTB 保留来源落码：`/reserved-memory`、MRB、容量/回绕/重叠/形状拒绝、引导区 checked 守卫 |
| `cargo test -p minix-boot --lib` | 132 通过（采集器用例见 §2.1） |
| `cargo test -p boot-shim --lib` | 69 通过（分类器用例见 §2.2） |
| 交叉编译 | `x86_64-unknown-none`、`aarch64-unknown-none`（`fw-none-image`）、`riscv64gc-unknown-none-elf`、`x86_64-unknown-uefi`、`aarch64-unknown-uefi` |
| QEMU | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh` PASS（exit 0，**仅覆盖 bootface 入口**）；`test-shim-bootmarks-aarch64` **仍被外部红阻断**：`servers/rs`（`b71bce1d0`）与 `servers/sched`（`ae4ef5c76`）已修，`os/servers/vfs/src/main_loop.rs:7635`（`impl SefIpc for VfsIpc`）仍缺 `sendrec`（`E0046`）。aarch64 启动行为**未验证** |
| 来源记录层 | `collect_with_sources()` 交出六类 `SourceRecord`（容量 40）；`no-map`、RAM 外声明、MRB 出表后仍可追溯：`dtb_no_map_reservation_keeps_a_source_record`、`dtb_memory_reservation_record_keeps_the_raw_range`、`dtb_reservation_outside_memory_keeps_its_record`、`dtb_source_records_cover_every_projected_span`；`collect()` 语义不变；`dtb_collect_and_collect_with_sources_agree` 钉住两条入口不漂移；`dtb_no_map_declarations_count_against_the_declaration_cap` 钉住「RAM 外的 no-map 不进任何表仍占声明容量」；`dtb_reusable_duplicates_are_deduplicated_not_rejected`、`dtb_ordinary_and_reusable_same_range_is_a_conflict`、`dtb_reusable_and_no_map_same_range_is_a_conflict`、`dtb_partial_overlap_of_memory_ranges_is_rejected` 钉住三分裁决 |
| FDT 容器校验 | `fdt_layout::validate` **28 条测试**（`accepts_a_canonical_layout`、`accepts_an_unaligned_strings_block`、`accepts_a_property_before_children`、`rejects_a_property_after_a_child`、`rejects_an_unknown_token`、`rejects_a_second_root`、`rejects_unbalanced_end_nodes`、`rejects_an_end_token_in_the_middle`、`rejects_a_property_running_past_the_block`、`rejects_a_property_name_outside_the_strings_block`、`rejects_an_unterminated_property_name`、`rejects_nesting_beyond_the_parser_limit`、`the_parser_consumes_the_deepest_tree_this_gate_accepts`、`rejects_misaligned_blob_address`、`rejects_misaligned_mrb_offset`、`rejects_mrb_outside_the_object`、`rejects_missing_terminator_before_the_struct_block`、`rejects_a_terminator_only_beyond_totalsize`、`rejects_more_reservations_than_the_collector_accepts`、`rejects_struct_block_past_totalsize`、`rejects_overlapping_struct_and_strings_blocks`、`rejects_totalsize_larger_than_the_buffer`、`rejects_a_structure_block_without_the_end_token`、`rejects_unbacked_structure_alignment`、`rejects_unsupported_fdt_versions`、`rejects_a_layout_whose_mrb_follows_the_struct_block`、`rejects_an_mrb_that_starts_inside_the_header`、`rejects_strings_before_the_structure_block`）；五处调用点：`bootface.rs`、`bootface_a64.rs`、`opensbi_helpers.rs`、`uefi_helpers.rs` 两处 |
| 解析库依赖事实 | `fdt 0.1.5` `node.rs`：`properties()` 首个非属性 token 即停；`children()`/`all_nodes()` 遇「子节点后属性」即结束；`all_nodes()` 父栈 64 槽、根在下标 1（最大安全深度 63）。QEMU 8.2.2 `dumpdtb` 全树 0 处属性后置 |

**证据分层（编译通过 ≠ 启动验证）**：

| 层级 | 本轮状态 |
|---|---|
| `minix-boot` 宿主单测 | 132 PASS |
| `boot-shim` 宿主单测 | 69 PASS |
| riscv64 启动路径 QEMU | PASS（exit 0） |
| aarch64 镜像组装 | **BLOCKED**：`E0046` @ `servers/vfs/src/main_loop.rs:7635`（`VfsIpc` 缺 `sendrec`） |
| aarch64 启动行为 | 本切片**未验证** |

**证据基线**：本轮代码提交 `cdfa1637c`。复核命令与顺序（缺一不可）：

```
git rev-parse HEAD
git status --short
cd os && cargo test -p minix-boot --lib
cd os && cargo test -p boot-shim --lib
```

132/69 条测试是**该 HEAD、该工作树状态**下的读数（§2.1、§2.2、§6 三处数字必须一致）。`git status --short` 的期望输出只有用户自有文件 `AI-chats/daily.todo.md` 一行；出现其它改动即说明测试不是在干净基线上跑的。基线之后如有新提交，必须重跑并更新本段。

复核方式：对 §2 的每条主张，文中给出了测试名或代码位置；对 §5 的每条问题，文中给出了失败形态——评审人可以只凭本文判断「问题是否成立」。

**测试通过不等于输入完整性**：132 与 69 条测试说明的是「已覆盖的行为」，不能证明所有 DTB 输入都被解析、所有原始访问点都被登记、每条启动路径都用了正确的权威来源。这几项各自需要独立的完整性论证（O-07、O-11、O-12）。

## 7. 签字状态

| 层 | 结论 | 依据 |
|---|---|---|
| 第一层：语义与方向 | **可签** | 五条不变量；§4 的准入合同方向明确 |
| 第二层：集合与逐路径判据 | **不签** | 按完整性边界分为六组（见下） |
| 第三层：固定布局 ABI | **不可签** | 未开始（O-13） |

**第二层的六个阻塞组**：

| 阻塞组 | 对应合同 |
|---|---|
| 输入真实性与边界 | O-11（含 EFI DTB 专属类型合同）、O-12h、**O-12k（MapKey 快照）**、O-12j（a/b 已实现，b 的预编译依赖闭包未证；c 未闭合）、DTB 节点准入 |
| 来源保真、RAM 资格与生命周期 | O-01、O-06（属性位冲突域已 fail-closed，完整固件 profile 仍待钉）、O-12f、O-12g（DTB 记录层已落，UEFI 描述符记录未落） |
| PMM 投影与页粒度 | O-12d、O-12i |
| 设备资源、映射和访问 | O-12c、O-08 |
| 禁止集合与逐树审计 | O-02a/b、O-03、O-04、O-05、O-12a/b |
| 对象与访问完整性 | O-07、O-09、O-10、O-12e |

**「禁止集合编译 + 引导根审计」有代码只能作为第一轮实现复审的起点，不是第二层签字的条件**：输入边界、来源完整性、PMM 资格与设备映射覆盖都是它的前置证明。

**第二层第一轮实现复审的起点**：禁止集合编译与引导根审计具备代码和测试后，可以开始复核其实现；这不代表其它阻塞组已满足，也不构成第二层签字。在那之前，对 §5 的评审只需回答两个问题：问题是否成立、候选是否覆盖失败形态。
