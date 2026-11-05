# 引导期内存政策：今天的保证、未决问题与候选方案

> **重写**: 2026-11-05 21:50 CST（由 `date "+%Y-%m-%d %H:%M %Z"` 实测；准确提交时间以 `git log -1 --format=%ci -- rewrite-notes/01-stage-kernel/01-adjudication-brief-memmap-open-items.md` 为准）；**内容轮次：第七十九轮评审后的修改**。
> **本文件是唯一的「现行规则」来源**：`02`（审阅历史）与 `03`（证据材料）是**历史档案**，其中出现的旧公式描述与旧符号名（例如把 Svade 实现状态与 `ADUE` 做无条件 OR 的旧布尔中间量，以及两个已被替换的旧字段/符号名）都是**已被撤销的历史文本**，不代表现行规则。只把本文件当作当前合同。
> 送审前请对一下本行时间：低于你正在评审的那一轮即说明材料不是最新。
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

**来源记录层（DTB 侧，第一阶段）**：`collect_with_sources()` 在同样的遍历里为每条声明留一条 `SourceRecord { id, kind, origin, range: PhysicalRange }`——`kind` 六类：`Memory`、`FirmwarePrefix`、`MrbEntry`、`ReservedOrdinary`、`ReservedReusable`、`ReservedNoMap`；`SourceKind::class()` 把它们分成**输入声明**（`Memory`/`MrbEntry`/`Reserved*` 五类）与**派生保留**（`FirmwarePrefix`）。**字段名是中性词**：输入声明的 `range` 就是声明范围；派生前缀的 `range` 是派生结果 `M_i ∩ [0, F)`（`F = firmware_reserve_end`），所以不叫 `raw_*`。`origin` 给出位置：`MemoryNode { node, reg }`、`ReservedMemoryChild { node, reg }`、`MrbEntry(n)`（`node` 只数匹配节点、`reg` 只数该节点 `reg` 属性内的元组，均从 0 起、按文档顺序）；前缀的 `origin` 是 `FirmwarePrefix { memory_id, cut }`，既指回被切分的 `/memory` 记录，又保留派生参数 `F` （原 DTB 释放后仍能解释「为什么截在这里」）。`id` 是**输入对象内的局部 ID**；跨输入引用须用 `(InputObjectId, LocalSourceId)`，不能单用 `id`。**记录不携带页范围**：`/memory` 投影向内取整（`FullPages`），保留声明与 `no-map` 向外取整，MRB 访问禁止保留精确字节。`range.end_exclusive` 是 `u128` 独占端，因此记录层能表达 `2^64` 本身：`physical_end` 只在 `start + len > 2^64` 时报 `RangeOverflow`，恰好等于 `2^64` 是合法顶端边界。拒绝发生在**投影**侧——`declared_page_range` 向外取整后达到 `2^64` 时报 `UnsupportedPhysicalAddressEnd`，`collect()` 构表时用 `projectable_end()` 复查同一条件。`PhysicalRange` 是两条腿共用的类型，因此 UEFI 描述符顶端页将来接入记录层时不会再靠 `u64` 加法偶然放过；但 UEFI 侧的 `SourceRecord` **今天仍未产出**（见 O-12g），这句只说类型合同，不说 UEFI 腿已实现。容量 40 由**输入上限**推出（`/memory` 段与声明各受 `DTB_MEMMAP_MAX`、`no-map` 同样计数、前缀不多于 `/memory` 段、MRB 受 `DTB_MRB_MAX`），按 **reg 元组**计数而非节点数；超限拒绝（`TooManySourceRecords`）。`collect()` 语义不变、只取前两张表。测试：`dtb_no_map_reservation_keeps_a_source_record`、`dtb_memory_reservation_record_keeps_the_raw_range`、`dtb_reservation_outside_memory_keeps_its_record`、`dtb_source_records_cover_every_projected_span`、`dtb_collect_and_collect_with_sources_agree`、`dtb_no_map_declarations_count_against_the_declaration_cap`、`dtb_declaration_cap_is_shared_across_reservation_kinds`、`dtb_source_capacity_is_reachable_at_exactly_forty`、`dtb_firmware_prefix_links_to_its_memory_record`、`dtb_identical_reservations_are_deduplicated_not_rejected`、`dtb_identical_mrb_entries_are_rejected`、`dtb_same_range_with_different_semantics_is_a_conflict`、`dtb_every_input_declaration_has_exactly_one_record`、`dtb_identical_memory_tuples_are_deduplicated_not_rejected`、`dtb_missing_reserved_memory_ranges_is_rejected`、`dtb_missing_reserved_memory_cells_is_rejected`。

解析与拒绝条件：

| 来源 | 解析内容 | 拒绝条件 |
|---|---|---|
| `/memory` | 全树枚举 + `reg()`，支持 unit-address 与多段 | 单表超过 8 段（`TooManyFreeRegions` / `TooManyReservedRegions`）；`address + size` 越过 `2^64`（`RangeOverflow`，畸形）；末端恰为 `2^64`：**记录层接受**（`SourceRecord.range` 已是 `PhysicalRange`，`end_exclusive: u128`），拒绝发生在**投影层**（`projectable_end()` 返回 `None` → `UnsupportedPhysicalAddressEnd`），投影改用 `u128` 后才可能交付最高页 |
| `/reserved-memory` | 路径查找 + 可见性交叉检查；父节点的 `#address-cells`/`#size-cells` 必须存在**且与根节点取值相同**（DTSpec 的父节点属性表把 `#address-cells`/`#size-cells`/`ranges` 三项 Usage 都标为 R=Required，另逐字写「should use the same values as for the root node, and `ranges` should be empty so that address translation logic works correctly」——因此**缺失是格式错误**，而「不同/非空」只是 *should*；本项目要求相同与为空属**项目输入子集**，不是规范要求）；`ranges` 必须存在且为空（同为**项目输入子集**，非空 `ranges` 不代表非法，而是尚无转换路径）；子节点必须有可解析 `reg`；只有 `size` 无 `reg` 是**动态分配请求**——DTSpec 3.5 逐字：「Each child node may either use a `reg` property to specify a specific range of reserved memory, or a `size` property with optional constraints to request a dynamically allocated block of memory」，且「A reserved memory node requires either a `reg` property for static allocations, or a `size` property for dynamics allocations. Dynamic allocations may use `alignment` and `alloc-ranges` properties to constrain where the memory is allocated from. If both `reg` and `size` are present, then the region is treated as a static allocation with the `reg` property taking precedence and `size` is ignored.」。因此 `reg`+`size` 并存时按 `reg` 静态处理、`size` 与 `alignment`/`alloc-ranges` 一并忽略是**规范规定**，不是本方选择；`no-map`/`reusable` 必须是**空属性**；两者互斥 | `size`-only 动态请求（`UnsupportedDynamicReservation`：**合法输入的不支持子集**——规范允许该形状，本启动没有可为其选址的动态分配器，静默忽略会把页交给 PMM；见 O-12g 行 c）；缺 `reg` 且无 `size`（`UnsupportedReservedDeclaration`）；缺 cells（`MissingReservedMemoryCells`）、cells 与根不同（`MismatchedReservedMemoryCells`，项目子集）；`ranges` 缺失（`MissingReservedMemoryRanges`，格式错误）与 `ranges` 非空（`UnsupportedReservedMemoryRanges`，输入合法但暂不支持转换）分开；同时声明 `no-map` 与 `reusable`（`ConflictingReservationAttributes`）；标记属性带非空值（`NonEmptyReservationAttribute`）；声明重叠（`OverlappingReservedRanges`）；节点可见却取不到（`ReservedMemoryUnreachable`） |
| MRB | `memory_reservations()` 条目（地址+长度，`{0,0}` 终止） | `address + size` 越过或落到 `2^64`（`RangeOverflow` / `UnsupportedPhysicalAddressEnd` 分名；后者是今天的临时拒绝，目标在记录层接受）；**零长度非终止项**（`ZeroLengthMemoryReservation`）；**条目之间重叠**（`OverlappingMemoryReservations`）；**超过 16 条**（`TooManyMemoryReservations`，缺失终止项也会落到这里）。对齐、`totalsize` 边界与终止项**由 `fdt_layout::validate` 在解析库之前检查**（五条输入路径接入）；`DtbMemoryMap` 自身不做头部校验，也不声称做过 |

规则细节：

- 声明按 4 KiB **向外**量化，再与 `/memory` 全景求交；越出 `/memory` 的部分不产生 RAM 主张，也不进入两张表。
- 固件前缀已覆盖的声明不重复登记（投影不重复，来源记录仍保留——见下一条）。
- **重复与重叠按来源分别裁决**：

| 来源 | 完全相同、相同语义 | 部分重叠 | 同范围不同语义 |
|---|---|---|---|
| `/memory` | 精确重复 → 投影去重、记录各留一条 | **拒绝**（`OverlappingMemoryRanges`）：重页会进 PMM 两次——项目输入限制 | —（`/memory` 只有 RAM 语义） |
| `/reserved-memory`（普通/`reusable`/`no-map` 三种语义） | 同语义重复 → 投影去重、记录各留一条（测试：普通 `dtb_identical_reservations_are_deduplicated_not_rejected`、`reusable` `dtb_reusable_duplicates_are_deduplicated_not_rejected`） | 拒绝（`OverlappingReservedRanges`） | 拒绝（`ConflictingReservationAttributes`：普通+`reusable` 生命周期不同、普通+`no-map` 访问不同、`reusable`+`no-map` 规范互斥；测试 `dtb_ordinary_and_reusable_same_range_is_a_conflict`、`dtb_reusable_and_no_map_same_range_is_a_conflict`、`dtb_same_range_with_different_semantics_is_a_conflict`） |
| MRB | **拒绝**（`OverlappingMemoryReservations`）：DTSpec 5.3.2「These given regions shall not overlap each other」，重复也是重叠（`dtb_identical_mrb_entries_are_rejected`） | 拒绝 | —（MRB 无 `no-map` 语义） |

**页级生命周期规则（为将来 `reusable` 回收设防）**：普通保留与 `reusable` 原始字节可不相交却落在同一页；EFI 类型相同不构成冲突，但只有该页上**全部分配排除来源都已解除**，页才能进入 `PmmBaseAvailable`。第一版两种声明都不回收，结果自然是继续排除——这条规则防止将来启用回收时出现「部分页释放」。

`/reserved-memory` 的重复容忍是**项目级输入策略**，不是规范授权；MRB 没有这层容忍。`reusable` 在去重时保留的是**声明身份**（两条记录各有 `id`）；owner 关系尚未建立，不能把记录数量当成 owner 数量——`memory-region` 引用是多对多（一个设备可引用多个区域、一个区域可被多个设备引用），模型应是 `DeviceMemoryRegionReference`（或 `MemoryRegionAssociation`）的引用集合，而非单个 `OwnerId` 字段。**引用不等于所有权**：只有平台 binding 或驱动注册信息进一步证明回收职责时，才提升为 `ReclaimAuthority`。`/memory` 的**部分重叠**拒绝同样是项目策略：**不是数学必然**——§3 允许纯集合层对兼容区间 `Union`，部分重叠在集合模型里可以安全归一化；拒绝的理由是「严格输入策略：部分重叠视为固件异常」，它不依赖今天 PMM 的实现，也不声称 DTSpec 禁止 `/memory` 重叠。若将来改为并集，来源记录仍须各留一条。`reusable` 的 owner 关系在去重时不得合并（来源记录各留一条正是为此）。
- `reusable` 在记录层已独立成 `ReservedReusable`；两张投影表仍把它按保留处理（不实现 owner 关联、回收能力或生命周期处置），第一版保守地不回收，但不丢原始声明语义。

测试（`cargo test -p minix-boot --lib`，146 通过）中的采集用例：`dtb_single_memory_region_fw_end_zero`、`dtb_multi_region_and_unit_address_variant`、`dtb_more_than_capacity_is_rejected_not_truncated`、`dtb_declared_reservation_is_never_conventional`、`dtb_no_map_reservation_leaves_both_tables`、`dtb_reservation_inside_the_firmware_prefix_is_not_duplicated`、`dtb_reservation_outside_memory_claims_no_ram`、`dtb_memory_reservation_block_is_carved_from_free`、`dtb_overlapping_reservations_are_rejected`、`dtb_reservation_with_wrapping_end_is_rejected`、`dtb_reserved_memory_with_wide_cells_is_rejected`、`dtb_carving_that_exceeds_capacity_is_rejected`、`dtb_reserved_memory_without_empty_ranges_is_rejected`、`dtb_reservation_with_no_map_and_reusable_is_rejected`、`dtb_reservation_with_nonempty_marker_is_rejected`、`dtb_reserved_memory_cells_must_match_root`、`dtb_overlapping_memory_reservations_are_rejected`、`dtb_too_many_memory_reservations_are_rejected`、`dtb_disabled_memory_node_is_rejected`、`dtb_hotpluggable_memory_is_rejected`、`dtb_zero_length_memory_reservation_is_rejected`、`dtb_size_only_reservation_is_rejected_as_dynamic`、`dtb_child_without_reg_and_without_size_is_an_unsupported_declaration`、`reg_and_size_together_uses_reg_and_ignores_size`、`dtb_memory_range_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_reservation_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_top_partial_page_reservation_is_refused_as_unsupported_end`、`dtb_memory_range_past_2_64_is_range_overflow`。

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
4. 独立属性 → 诊断成员：**当前可达的只有 `RUNTIME`**；`NV`（`0x8000`）与 `SP`（`0x4000` 低位 / `0x4_0000` 高位）在当前冲突位拒绝与 profile 门控下 fail-closed，不进入输出表。`PERSISTENT` 是独立的 UEFI 内存类型，不等于 `NV` 属性位，不能用它的测试证明 `NV` 分支可达；
5. 无写回位 → 诊断成员；
6. 写回准入后按类型分派：`CONVENTIONAL` → 可用表；引导服务类 / `ACPI_RECLAIM` → 保留表；其余 → 诊断成员。

测试（`cargo test -p boot-shim --lib`，70 通过）：`types_that_earn_no_ordinary_mapping_go_to_the_side_list`、`excluded_and_device_types_stay_out_of_every_list`、`rejects_side_list_beyond_its_buffer`、`the_four_ambiguous_combinations_land_as_adjudicated`、`every_contested_attribute_bit_fails_closed`、`profile_only_attributes_fail_closed_without_a_pinned_profile`、`geometry_is_checked_for_ranges_that_are_emitted` 等 32 条。

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
| riscv64 自引导 | `DtbMemoryMap`（`= KERN_PHYS_BASE`） | **`collect_with_sources()`（`bootface.rs:203`）** | **被持有并审计**：`audit_source_coverage` 在启动路径上校验「每条投影字节都有来源记录」，失败即停 |
| riscv64 U-Boot | `DtbMemoryMap`（`= KERN_PHYS_BASE`） | `collect()`（`opensbi_helpers.rs:275`） | **被丢弃**；QEMU 用例不覆盖 `RiscvUbootBootShim::prepare_boot` |

`grep -rn 'collect_with_sources' --include=*.rs os`（排除 `memmap_source.rs`）现为 **1 处**：`os/kernel-image/src/bootface.rs`——riscv 自引导入口已持有记录并调用 `audit_source_coverage`。其余三条入口（aarch64 直启、U-Boot shim、UEFI shim）仍调用 `collect()` 并丢弃记录。**持有 ≠ 进入政策**：审计只用记录做「投影有来源」的自洽检查，还没有派生禁止集或 PMM 排除；谁在哪个阶段传递、何时释放仍是下一刀。

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
2. **PMM 基底层 = 可用 ⊍ 排除**，且只对 `RamSourcePageDomain` 成立（`PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx)`，见 O-12i）。用途层**不是第二个划分**：`PmmRoleRejected(role, ctx) ⊆ PmmBaseAvailable(ctx)`，同一页可以「普通数据可用、页表用途拒绝」；把用途拒绝并进 `PMMExcluded` 会让同一页同时出现在可用与排除两侧，「一页只排除一次、排除理由全部可追溯」的记账立刻失效。`RamSourceDomain` = 该启动入口的权威来源识别出的物理主存区间，**尚未应用保留声明与对象生命周期**：DTB 入口取 `/memory`；UEFI 入口按明确策略从类型中识别（不是所有描述符）；MMIO、端口、地址空洞、未描述区域都不属于它。同一页有多条保留来源时，排除只计一次、来源身份不丢。

**必须分开的两个概念**：`RamSourceDomain` 回答「权威输入是否声称这段是物理 RAM」；`OrdinaryRamMappingEligibility`（原 `RamBacking`）只回答「当前阶段是否允许把它作为普通主存**映射**」；能否分配由 `PmmEligibility(phase)` 表达，某个对象能否经某条路径访问由 `AccessRequirement/AccessAuthorization` 表达，物理内存性质由 `MemoryKind` 表达。四者不能互相推导。**术语说明**：`backing` 容易被读成「物理支撑」，而本项说的是 OS 政策资格——`RUNTIME_SERVICES_*`、`ACPI_NVS`、`PERSISTENT` 都有物理支撑却不具普通资格，`UNACCEPTED` 有物理来源但未接受。物理支撑性质、OS 政策资格、硬件准入是三个层次，不能压成一个布尔值。`EfiReservedMemoryType` 是典型反例：aarch64 UEFI 的权威 DTB `/reserved-memory` 与 MRB 提供**保留来源身份**，UEFI 内存图提供物理存在与固件处置；两者可交叉验证保留声明，但**都不能单独推出 `RamSourceDomain` 成员资格**——只有独立平台证据能证明属于系统 RAM 时才纳入。x86_64 没有 DTB 来源，它的 `EfiReservedMemoryType` 只能按固件类型政策处置，不能套用 aarch64 的交叉核对。
3. **每棵树各有独立的映射禁止集合；访问禁止另有独立集合，按对象、阶段与访问路径裁决。禁止映射与禁止访问不得互相代替；禁止与分配正交**。`no-map` 的两项子语义——`DenyStandardSystemMapping` 与 `DenyUncontrolledSpeculativeAccess`——也各自独立，不因为都会影响页表映射就被压平成一条；受控驱动例外只属于后者，不能顺带解除前者。
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

**没有 ACPI 启用事件的 profile：`EfiACPIReclaimMemory` 视为 `Permanent`**：它的规范生命周期是「ACPI 启用前保留、启用后可回收」，而标准 EFI DTB 又必须放在这种内存里。若某个受支持 profile 从不进入 `AcpiEnabled`，就不能把 `Until(AcpiEnabled)` 当成已满足——否则一旦复制完 DTB 就会把原页交给 PMM。规则：只有存在明确的 ACPI 启用事件，才允许该类型转为可回收；否则在该 profile 中永久保留。

**`EfiReservedMemoryType` 的禁止语义分三层，且不依赖 DTB**：

| 层 | 主张 |
|---|---|
| UEFI 类型事实 | 内存类型表的可用性列对该类型写的就是字面 **「Not usable」**——项目不得把它改写成一个更像政策标识符的驼峰名，也不得写成「保留但仍可用」或「受保护内存」；配套事实是 `GetMemoryMap()` 侧区分两种情况：没有物理硬件支撑的地址空间不作为物理内存范围返回，有物理支撑但不应由 OS 访问的范围必须以该类型返回 |
| OS 政策语义 | `DenyOsUse`：不得作为 OS 可用内存，也不得让普通内核对象访问它；普通 PMM 分配与普通系统映射都归它派生 |
| 硬件兑现 | UEFI 类型本身**不建立页表、不配置 PMP、不保证 CPU 访问会硬件故障**；映射隔离与访问阻断由内核兑现。它不是 DTSpec `no-map`（后者另有标准映射限制、非受控推测访问限制与驱动受控使用例外） |

**三层各自的说法不得串层**：第一层只有「Not usable」这一个事实，它既不承诺硬件会拦访问，也不等于本项目的 `DenyOsUse`；第二层才是项目政策的具名主张；第三层是「类型本身不兑现保护，映射隔离与访问阻断由内核建立」。两个方向的越层都要防：把第一层写成「硬件禁止访问」是拔高，把第三层「类型本身不兑现保护」反过来读成「所以第一层其实没那么强」是压低——后者尤其危险，因为 `DenyOsUse` 的强度来自第二层政策，不依赖硬件是否拦得住。UEFI 原文在本环境抓取返回 403，「Not usable」措辞与 `GetMemoryMap()` 的两段区分都由**评审人按 UEFI 2.11 核对**，本方独立取证仍是缺口，登记在 §6，不升级成本方已验证的引用。

**与 `no-map` 的驱动例外组合时，第一版选「绝对禁止」**：`EfiReservedMemoryType` 区域即使同时带 DTB `no-map`，也不开放 `DriverControlledUse`——**理由是 UEFI 的「OS 不应访问」政策与本项目第一版不开放受控例外的合同共同成立**；它不是「UEFI 已给出与 MRB 同强度的硬件禁令」。组合测试（`EfiReservedMemoryType` + `no-map` + 受控使用）列为待补；另一个可选边界是「一般用途禁止、专属驱动例外可用」，但第一版不选它，避免让单一类型名隐式决定组合语义。

`TypePolicy` 必须能独立生成 `DenyOsUse` 及其派生主张，**不要求存在对应的 DTB `SourceRecord`**（x86_64 没有 DTB，不能因此丢掉 UEFI 自己的禁止语义）。两者都可能导致普通直接映射被拒，但来源语义与例外规则不同，不得合并成一个无差别 `DenyMapping`。同理，`UNUSABLE` 是域内排除（来源身份存在、类型资格排除），不与域外的 `RESERVED` 合并。

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
| `EfiReservedMemoryType` + `no-map` + `DriverControlledUse` | **拒绝**：该页命中 UEFI 的 `DenyOsUse`（OS 不应访问）与项目不开放驱动例外的合同；不是「UEFI 类型已提供硬件访问屏障」。这不等于「所有 `no-map` 一律禁止驱动受控使用」——只是这一来源组合多了一条更强的类型政策 |
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
| 架构 adapter 列出的受控元数据字段（`MutableMappingMetadata { field, update_agent, permitted_transition, synchronization_contract }`） | 按 `update_agent` 允许特定变化；RISC-V 的 A/D 随 `PteUpdateScheme` 取 `Hardware` 或 `FaultHandler`，AArch64/x86 由各自 adapter 定义；审计必须说明如何屏蔽或验证 |

**冻结对象是 `FrozenBootMappingSet`，不是整棵页表**：T1/T2 审计过的是启动政策映射集合，该集合内的映射定义不得被未授权修改；其它页表区域（未来的 VM、动态映射、进程地址空间）经受控更新接口变化——准备修改 → 校验政策 → 发布 → 架构同步——这不违反冻结。**不接受的表述**：「启动后页表永久不可改」。

**候选 B**：不冻结，每次修改后重跑对应审计——可作为后续动态页表阶段的机制，不能替代切根前审计。

**无论 A/B，后续页表修改必须「先审后用」**：在未激活的副本上修改并审计后再发布，或经受控更新接口在映射对硬件可见之前完成验证；A/D 位的屏蔽必须逐字段定义，不能笼统忽略一组 PTE 位。

**政策审计与硬件同步是两份独立合同**：审计通过不等于新映射已在所有执行单元生效。架构层必须负责 PTE 写入的发布顺序、相应地址空间的 TLB 失效、以及多核之间的同步失效（shootdown）；共享层只表达「新映射必须在所有相关执行单元生效」这个语义，不出现 `INVLPG`/`SFENCE.VMA`/`TLBI` 之类机制。

**判据**：每个时点能列出「必须已完成的审计」与「允许的硬件更新」。

**第一个消费者**：根切换代码（今天只有一次根建立，没有切换）。

### O-06 `OrdinaryRamMappingEligibility` 的阶段化定义

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

**资格是分层的，不是三个布尔值**：`RamSourceDomain`（权威来源是否声明这是物理主存）、`MemoryKind`（普通易失 / 持久 / NVS / 未接受 / 不可用）、`OrdinaryRamMappingEligibility(tree, phase)`、`PmmEligibility(phase)`、`AccessRequirement/AccessAuthorization` 各自回答不同问题，不能互相推导——普通 RAM 可能因 `no-map` 禁止普通映射；设备 MMIO 不是普通 RAM 却有设备访问授权；普通 RAM 可能因 MRB 禁止访问；保留对象可能允许特定对象读却不许 PMM 分配。表头「普通主存性质」只描述 `MemoryKind`，不是「能否使用」的总结论。

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

**已知属性位的处置**（不能只写「未知位拒绝」）：模型保留 `RUNTIME`/`NV`/`SP` 三种独立属性语义，但**当前分类器实际可达的只有 `RUNTIME`**：`NV` 位属于无条件冲突拒绝域；`SP` 的低位编码属于冲突拒绝域、高位编码在 profile 未钉住时被 `ProfileAttribute` 拒绝——二者当前都不进入输出表；`RO`/`RP`/`XP`/`WP`/`CPU_CRYPTO` 是 **capability 能力位**（`MORE_RELIABLE` 因 `0x1_0000` 的官方文本歧义已改为 fail-closed，见下），第一版只作为来源元数据保留——**不直接等同于当前已生效的访问权限**，也不改变 RAM 来源域成员资格。后续映射策略可用其中的能力信息验证目标属性是否可兑现；**`WP` 不得误解为页表写权限**。任何**档外位**仍按第 1 步拒绝启动。

**属性按语义分三类**（不能统称「能力位」）：缓存能力（`WB`/`WT`/`WC`/`UC`/`UCE`）；物理保护能力（`WP`/`RP`/`XP`/`RO`）；其它特征（`MORE_RELIABLE`、`CPU_CRYPTO`、`SP`、`NV`、`RUNTIME`）。**`WP` 归入物理保护能力**：规范先称它为 physical memory protection attribute，再说明它通常被用作 write-protected cacheability policy——它有特殊缓存语义，但既不是页表写权限，也不能与 `RO` 混为一谈。**`SP` 不是 no-map**：第一版把它排除出普通 PMM 是项目保守选择，不得由 `SP` 自动生成映射禁令。

**`CONVENTIONAL` 是有条件的**：它可以构成 `RamSourceDomain` 与普通主存性质，但 `OrdinaryRamMappingEligibility`（类型层面）**不隐含缓存属性检查**；最终普通映射 = 类型资格 ∧ 缓存属性资格（第一版要求 WB），最终 PMM 分配 = 阶段资格 ∧ 缓存资格 ∧ 全部排除项。无 WB 的描述符不进入普通可分配集合，但**保留其来源身份**。

**术语边界**：这里读的是 `GetMemoryMap()` 的 `EFI_MEMORY_DESCRIPTOR.Attribute`（能力位掩码），与 `EFI Memory Attributes Table` 反映的保护配置不是一回事；第一版不解析后者，也不宣称已获知固件实际施加的全部权限。

**`EfiReservedMemoryType` 在域外但来源身份仍有效**：它不进入 `RamSourceDomain`，因此不参与 `PmmBaseAvailable`/`PmmBaseExcluded` 的域内分割；但与其重叠的 DTB `/reserved-memory`、MRB 原始记录仍然保留，并可独立产生映射禁止与访问禁止。没有独立平台证据时，不把它重新解释为普通主存。

**UEFI Runtime Services 的支持边界（明确声明）**：第一版**不调用 `SetVirtualAddressMap()`、不提供 UEFI Runtime Services**；`RUNTIME` 属性与 `RUNTIME_SERVICES_*` 类型只保留诊断与来源信息，不产生任何普通或运行时映射义务。将来支持时，必须**单独增加**运行时对象与 MMIO 映射义务，不得混入普通直接映射授权。

**`EFI_MEMORY_HOT_PLUGGABLE` 与语义歧义位（两个不同的位）**：

- **冲突位域整体 fail-closed（已实现）**：`0x1000..=0x1_0000` 在两套官方编码里含义不同——`0x4000` 在一套里是 `SP`（必须排除出 PMM）、另一套里是 `XP`（仅保护能力元数据）；`0x8000` 是 `CPU_CRYPTO` vs `NV`；`0x1000` 是 `MORE_RELIABLE` vs `WP`；`0x2000` 是 `RO` vs `RP`；`0x1_0000` 是 `HOT_PLUGGABLE` vs `MORE_RELIABLE`。任一读法都可能把必须排除的内存伪装成元数据，因此整个 `0x1_F000` 在已知位闸之前单独拒绝（`PolicyError::AmbiguousAttribute`，负控 `every_contested_attribute_bit_fails_closed` 与 `the_ambiguous_bit_fails_closed_under_either_name`）。
- `0x0010_0000`（仓库编码的 `HOT_PLUGGABLE`）：`uefi-raw 0.9` 无该常量，走未知位拒绝（`hot_pluggable_bit_fails_closed_as_unknown`）。
- **代价**：`NON_VOLATILE` 位现在也落进拒绝域——模型仍认识 NV 语义（`TYPE_INDEPENDENT` 保留该分支），但**当前分类器不接受带 NV 位的输入，该分支在真实输入上不可达**；恢复它需要**两步**：先钉住带版本证据的固件编码 profile，再把 `0x1_F000` 的无条件拒绝改为按 profile 解释——只要无条件拒绝还在，仅添加 profile 记录仍然解不开 `NV`。若决定后续版本也不支持该属性，应明说，不要让读者以为 profile 一到手就自动解锁。`SPECIAL_PURPOSE`（`0x4_0000`）属于 `ProfileAttribute` 拒绝域、低位 `0x4000` 属于冲突拒绝域，**当前同样不可达**。**位值证据不是「三个已发布文本 vs EDK2」**：UEFI 2.10 原始发布版与 EDK2/Linux 同组，2.10 Errata A 与 2.11 PDF 是移位后的另一组（见下方完整矩阵）。钉住的是「当前 `uefi-raw` 与 Linux 头文件一致的编码」这一事实，而不是某份 PDF 的表格；将来升级 `uefi-raw` 时**必须重核全部已识别属性位**，不能只迁移 hot-pluggable 一位。

**高位属性改为 profile 门控（已实现），理由要按两版发布文本写对**：`RO`/`SP`/`CPU_CRYPTO` **在 UEFI 2.10 原始发布版里就是 `0x2_0000`/`0x4_0000`/`0x8_0000`**；2.10 Errata A 与 2.11 PDF 把它们移到低一组（`0x2000`/`0x4000`/`0x8000`）。所以问题不是「已发布规范没有这些位置」，而是**两个正式发布版本族对同一属性采用了不同位值**，而内核无法从 `uefi-raw` 常量判断启动固件采用哪一版。三者因此在 profile 未钉住前**直接拒绝**（`PolicyError::ProfileAttribute`，测试 `profile_only_attributes_fail_closed_without_a_pinned_profile`）；`0x10_0000` 仍由未知位闸拒绝。将来 `UefiMemoryAttributeProfile` 必须绑定**实际启动固件镜像身份**（x86 与 aarch64 分别记录，不能假设同源），并**同时钉住低位组与高位组的完整映射**——只授权三个高位会得到「部分属性可识别、其余冲突位永久拒绝」的半套 profile。

**属性位完整矩阵（两组编码的全部相关位）**：

| 属性 | UEFI 2.10 原始版 | 2.10 Errata A / 2.11 PDF | EDK2 / Linux / `uefi-raw 0.9` | 本项目当前处置 |
|---|---|---|---|---|
| `WP` | `0x1000` | `0x100` | `0x1000` | 冲突域 → 拒绝 |
| `RP` | `0x2000` | `0x200` | `0x2000` | 冲突域 → 拒绝 |
| `XP` | `0x4000` | `0x400` | `0x4000` | 冲突域 → 拒绝 |
| `NV` | `0x8000` | `0x800` | `0x8000` | 冲突域 → 拒绝 |
| `MORE_RELIABLE` | `0x1_0000` | `0x1000` | `0x1_0000` | 冲突域 → 拒绝 |
| `RO` | `0x2_0000` | `0x2000` | `0x2_0000` | `ProfileAttribute` → 拒绝 |
| `SP` | `0x4_0000` | `0x4000` | `0x4_0000` | `ProfileAttribute` → 拒绝 |
| `CPU_CRYPTO` | `0x8_0000` | `0x8000` | `0x8_0000` | `ProfileAttribute` → 拒绝 |
| `HOT_PLUGGABLE` | 未定义 | `0x1_0000` | `0x10_0000` | `0x1_0000` 冲突域；`0x10_0000` 未知位 |

PDF 低位组的 `0x100`–`0x800` 在本项目里落入已知位集合之外，同样被未知位闸拒绝。规范仓库 `main` 是活动分支，不能当固定版本。

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

**边界**：`OrdinaryRamMappingEligibility` 只描述来源与 OS 政策资格，**不证明某架构实际能访问该区间**——x86 还要 PAT/MTRR 有效类型、AArch64 还要 MAIR 与逐 PE 配置、RISC-V 还要 PMA 能力与逐 hart 的 PMP。硬件准入不得被 `ordinary_main_memory = true` 代替。

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
- **对象身份在首读前建立**：`BootInputObjectId` 由启动 ABI 交付时即分配，不等到 `totalsize` 才知道——首读前已经存在一个具 40 字节读取授权的对象。同一身份随阶段扩充范围：

```
BootInputObject { id, provenance, initial_read_range /*40B*/, validated_full_range, owned_copy_range, lifecycle }
```

header 读取、完整对象授权、自有副本、生命周期记录都继承同一 ID，避免出现「header 是对象 A、完整 DTB 是对象 B、副本是对象 C」只能靠地址猜测关联。

**全范围可达是第二个独立检查**：拿到 `totalsize` 后必须**重新证明** `[dtb, dtb + totalsize)` 在当前转换下整段可读（覆盖所有跨页、地址转换与末端），不能拿 header 的 40 字节证明代替；复制只能在这次证明之后开始。

**复制目标不得与源重叠**：原始 DTB 不一定在 MRB 里，也未必已被分配器排除。分配自有副本前，先把原始 DTB 的完整范围登记为**存活输入对象**；源与目标范围不相交才允许复制（DTSpec 要求客户端在 DTB 使用完之前不得覆盖它，即使它不在保留区内）。于是 O-11 有四个互相独立的条件：**来源授权、地址可达、内容一致（不得并发修改）、自有副本**。

**`OwnedCopyValidation`（第三段，不能省）**：`fdt_layout::validate` 并非只读 40 字节 header——它还要扫 MRB、结构块、属性长度与字符串名称。因此完整容器校验**必须在自有副本上执行**：原始 DTB 的 **header 只在 preflight 阶段读一次**，**body `[dtb+40, dtb+totalsize)` 只在复制阶段读一次**；两部分都进入自有副本后，校验与解析只访问副本，不再访问原始地址。三段的顺序是：provenance + reachability（`[dtb, dtb+40)` 整段）→ header preflight → 完整读取授权 + 对齐副本 → 在副本上 `validate` → 解析 → `collect`。

**副本必须是一份一致快照（复制算法写死）**：`totalsize` 本身来自 header，若「先读 40 字节、再整段复制」就会把 header 读两次。算法：

1. 一次读 40 字节到局部 header snapshot；
2. 只从 snapshot 校验 magic/版本/`totalsize` 上限与加法；
3. 分配对齐的自有缓冲区；
4. 把 snapshot 写入副本前 40 字节，**只从原地址复制 `[dtb+40, dtb+totalsize)`**；
5. 在副本上执行完整 `fdt_layout::validate`。

**ABI 地址、源物理范围与读取 VA 是三个概念**：`BootInputAddress`（ABI 递交的地址及其地址域——四条路径实际交付的都是**物理地址**：AArch64 `x0`、UEFI 配置表 DTB、RISC-V `a1`/`satp=0`）、`SourcePhysicalRange`（DTB 对象实际占用的物理范围）、`ReadVirtualRange`（当前阶段实际用于读取的 VA 范围）。不构造概括性的 `SourceVirtualRange`——那会把物理地址误包装成虚拟地址。`LiveInputReservation = Outward(SourcePhysicalRanges)`——**PMM 排除的是物理页，不是虚拟区间**；目标副本与源的不重叠也必须**按物理地址**判断：源 VA 与目标 VA 数值不同不代表不落在同一 PA（直接映射、恒等映射等别名都可能重合）。

**统一物理范围类型 `PhysicalRange { start: u64, end_exclusive: u128 }`**（`start < 2^64`、`start < end_exclusive ≤ 2^64`）：DTB 的 `address + size` 与 UEFI 描述符都能表达 `[0xffff_ffff_ffff_f000, 2^64)`，统一来源记录不得因 Rust 字段类型丢掉这个合法边界。原始输入字段保留各自编码（DTB 的 address+size、UEFI 的 start+pages），规范化层才转成 `PhysicalRange`；`UnsupportedPhysicalAddressEnd` 只在项目明确选择拒绝该边界时使用，**不能成为 `u64` 加法的副作用**——DTB 腿本轮已按这条实现（`physical_end` 用 `u128` 求和并分名两类拒绝，见 O-12i 的三层表）。这条与 O-12h 结构校验、O-12i `FullPages` 是同一条边界合同；DTB 记录层**已按此落地**，UEFI 侧的 `SourceRecord` 尚未接入。

**副本与源的相交检查绑定实际分配结果**：按 PA 判断不重叠之后，仍须在分配器返回副本物理地址后，用**实际分配范围**再作一次相交检查。「分配器通常不会从 ACPI reclaim 区分配」不是证据——分配资格与源对象的生命周期登记是两份独立证据。

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

**类型验证分两个时点**（读完 header 之前并不知道 `totalsize`）：① **首读前**用 ABI 给出的物理起始地址验证 `[dtb, dtb+40)` 覆盖的全部页都落在 `EfiACPIReclaimMemory`，并完成 `BootInputProvenance` 与端到端 `BootInputReachability`；② **header preflight 后**由 snapshot 得到 `totalsize`，验证 `Outward([dtb, dtb+totalsize))` 的**全部页**都由该类型描述符完整覆盖（可多个连续同类型共同覆盖）；覆盖不完整、类型不一致或中途落入其它类型 → 立即拒绝，**不得继续复制 body**。

**必测**（未实现）：header 跨两个连续 ACPI reclaim 描述符；header 在 ACPI reclaim 而 body 跨入其它类型（读 body 前失败）；header 跨入不兼容页（读 body 前失败）。

**EFI DTB 对象本身也是页级主张来源**：它要求 `EfiACPIReclaimMemory`。与静态 `/reserved-memory`（BootServicesData）、`no-map`（Reserved）向外量化后同页 → **冲突，拒绝**。与 MRB 同页时第一版**采用例外**：该页按 UEFI DTB 的强制要求取 `EfiACPIReclaimMemory`，MRB 的访问禁止与 PMM 排除照旧保留，一次性读取走具名 MRB 授权——这是对 MRB 类型 `should` 的**有意偏离**（遵守两条规范中更强的类型要求），不是把两种来源合并。

**标准 DTB 表有专属规则，不能套用一般配置表的四种类型**：UEFI 2.11 §4.6.1.3 要求 DTB 位于 `EfiACPIReclaimMemory`、按物理地址解释、运行时不修正；一般配置表允许的 `RuntimeServicesData`/`BootServicesData`/`ACPINVS` 对标准 DTB 表不成立，`LoaderData` 更不在其中。**三方组合的完整裁决（EFI DTB 对象 × MRB × `no-map`）**：上面各处给的都是两两结论，容易被读成三者叠加也各自兼容，这里收成一张表。表中的「同页」一律指 `Outward(原始范围)` 相交，**不是**原始字节相交：

| 该页同时被主张 | EFI 类型结论 | 一次性首读 | 依据 |
|---|---|---|---|
| 仅 EFI DTB 对象 | `EfiACPIReclaimMemory` | 允许 | 标准 DTB 表的专属类型要求 |
| EFI DTB + MRB（**原始字节相交**：`DtbRawRange ∩ MrbRawRange ≠ ∅`） | `EfiACPIReclaimMemory`：DTB 的类型 `must` 压过 MRB 的类型 `should` | 允许，但必须走具名 `BootInputReadUse`——这次读取确实触碰 MRB 字节 | 有意偏离：项目对 MRB 类型的 `should`→must 强化在此让位；MRB 的来源身份、访问禁止与 PMM 排除**不因该页取 ACPIReclaim 而消失** |
| EFI DTB + MRB（**只有向外量化后同页**：原始字节不相交） | 同上，按页级规则裁决 | **不属于 MRB 原始字节访问**；若第一版仍要求 MRB 授权，必须标成**项目额外施加的页级启动读取限制**，不得说成 MRB 原始字节规则 | 两种粒度不能互相扩张：页级类型裁决不产生字节级访问授权 |
| EFI DTB + 普通静态保留 | **冲突 → 拒绝启动** | 不读取 | 静态保留的类型对应是规范要求（`must` → `EfiBootServicesData`），不能被另一条 `must` 静默覆盖 |
| EFI DTB + `no-map` | **冲突 → 拒绝启动** | 不读取 | `no-map` 向外量化到同一 4 KiB 页即冲突；第一版**不区分原始字节是否真的不相交** |
| EFI DTB + MRB + `no-map` | **冲突 → 拒绝启动** | 不读取 | MRB 例外只解决 DTB 与 MRB 的**类型**冲突，不解除 `no-map` 的类型主张；两条例外不能相加抵消 |
| EFI DTB + 动态 `size` 保留 | 不适用 | 拒绝：`UnsupportedDynamicReservation` | 动态保留按规范不列入内存图，与 DTB 必须在图内的类型要求不能同时成立 |

**「EFI 类型按页、MRB 访问按原始字节」是两条粒度，不得互相扩张**：

| 维度 | 粒度 | 取整方向 | 不能做什么 |
|---|---|---|---|
| EFI 描述符类型裁决 | 4 KiB **页** | 向外：部分覆盖即构成该页的类型主张 | 不能因为原始字节只占半页，就把同页另一半当成「无类型主张」 |
| MRB 的访问禁止与受限授权 | 声明的**原始字节** `[start, start+len)` | **不取整** | 不能因交集页取了 `EfiACPIReclaimMemory` 就把 MRB 的字节授权扩成整页可读；也不能因某页类型合规就允许越出原始字节的访问 |
| `no-map` / 普通保留的分配排除 | 页 | 向外 | 排除范围可扩到整页，但访问禁止的**强度**仍来自各自来源语义，不因取整而改变 |
| DTB 首读范围 | 原始字节（header 40 B、body `totalsize`） | 读取范围本身不取整，**类型验证按页** | 不能拿「页类型合规」替代「该字节范围此刻可达」——这正是 `BootInputProvenance` 与 `BootInputReachability` 不互推的那条分工 |

上表两行的区分条件是 `DtbRawRange ∩ MrbRawRange` 是否为空，**不是** `Outward` 交集是否为空——后者只决定该页的 EFI 类型主张与 PMM 排除。保守选择（同页即要求授权）可以保留，但归属必须写对：它是项目页级限制，不是 MRB 规则。

两条禁止互推由此固定：页级类型裁决成立**不**意味着该页全部字节可访问；MRB 的字节级访问禁止也**不**改变该页在内存图里的类型主张。表里「同页冲突即拒绝」是**保守规则**，它会拒绝掉原始字节其实不相交的合法配置，换来的是不必证明「固件给出的页内字节布局恰好不重叠」——第一版接受这个代价。`reusable` 与 `no-map` 的互斥、MRB 之间的不重叠（规范 *shall*）与 MRB 类型（规范 *should*，项目升为强制）三种强度照旧分别记录，不因本表改变。

这张表**只**决定「一次性只读能否发生」，不修改 `RamSourceDomain`、`PmmBaseAvailable` 或 `OrdinaryRamMappingEligibility`。

**与 O-03 的接口**：若 DTB 本体落在 MRB 覆盖范围内，上面这次只读访问必须登记为**MRB 规则自身允许的、由启动程序明确提供的一次性对象访问授权**（带对象身份、阶段与只读路径）。要否定的是「无边界豁免」——授权不覆盖所在 MRB 区域的其它访问，也不授权复制完成后继续访问原地址；但也不应把 DTB 读取说成「不属于任何例外」，那与规范允许的显式授权相矛盾。

**实现状态：未实现**。见 §2.1——今天校验与解析都在固件提供的原地址上就地执行。

**副本生命周期**：原始 DTB 只在复制期间可读；自有副本登记进对象目录；切根前不得回收或覆盖；副本的最终释放必须有明确消费者与阶段。边界条件：header 固定读取长度写明；`totalsize` 有项目上限；`dtb + totalsize` 必须 checked；完整范围必须证明可安全读取（不能只验证起始地址）。

**重叠时的 EFI 类型裁决**：同一 EFI 页被多条来源覆盖时，先按来源语义生成逐页类型主张，只有裁决表明确认兼容的主张才能合并——普通静态保留（`must` → BootServicesData）与 MRB 强化（Reserved）同页时**拒绝启动**，规范强制要求不得被另一个来源的项目强化静默覆盖；来源身份分别保留；无法同时满足所有来源约束则拒绝启动。规范强度写明：静态 `/reserved-memory` 入图与类型对应是明确要求；MRB 入图是明确要求，但用 `EfiReservedMemoryType` 的措辞是 `should`（项目可提升为严格合同，不得写成 must）。

**交叉验证覆盖两种来源**（不能只验 `/reserved-memory`）：

| 权威 DTB 来源 | UEFI 内存图预期 |
|---|---|
| 普通静态 `/reserved-memory` | `EfiBootServicesData` |
| 带 `no-map` 的静态 `/reserved-memory` | `EfiReservedMemoryType` |
| MRB 页（**不与 EFI DTB 对象页相交**） | `EfiReservedMemoryType`（项目强化） |
| MRB 页（**与 EFI DTB 对象页相交**） | `EfiACPIReclaimMemory`（DTB 专属 `must` 优先；MRB 的类型 `should` 在此让位） |
| 动态 `size` 保留区 | 不应列入内存图（规范要求）；`reg` 与 `size` 并存时按静态 `reg` 处理 |

**MRB 与普通声明重叠时**：交叉验证只核对覆盖与兼容性；政策层保留两个独立来源身份，不得合并成一个更大的区间后丢掉语义差异。

**MRB 与 EFI DTB 的页集合差分**（例外只作用于交集页，不是整条 MRB）：

```
DtbPages = Outward(EfiDtbObjectPhysicalRange)
MrbPages = Outward(MrbPhysicalRange)

MrbPages \ DtbPages  → EfiReservedMemoryType
MrbPages ∩ DtbPages  → EfiACPIReclaimMemory
```

交集页上的 MRB 来源身份、访问禁令与 PMM 排除仍然存在，只是该页的 EFI 类型要求服从 DTB 专属 `must`。因此一条多页 MRB 若只与 DTB 相交其中一页，描述符必须能按页分段：其余页仍按项目强化规则取 `EfiReservedMemoryType`。**必测**（未实现）：MRB 覆盖多页、DTB 只落在中间页 → 只允许中间页为 ACPIReclaim；DTB 横跨两页 → 不能只检查起始页。

**判据**：该腿能指出保留声明的实际读取点；若 DTB 在 MRB 内，能指出授权依据（候选 A）或拒绝依据（候选 B）。

**第一个消费者**：aarch64 UEFI 腿的保留政策。

### O-12 集合构造与来源完整性（总括，拆成可独立签字的子合同）

**问题**：禁止映射集合、禁止访问集合、设备窗口集合、PMM 累积器今天的构造式、消费者与容量都没有定义；它们来源不同，不应捆绑成一个裁决。

- **O-12a** 引导根禁止映射集合：每条禁止项可追溯到一个来源（`no-map` / MRB / 类型 / 对象）。
- **O-12b** 最终根禁止映射集合：同上，且包含对象负向义务的投影。
- **O-12c** 设备窗口授权：见下节；判据是**每页都能指回一条有效的设备映射政策授权**。
- **O-12d** PMM 排除累积器：与可用表互补（对来源域）；每来源贡献一次。
- **O-12e** 容量与失败形状：超限拒绝启动；两遍扫描；每份输出独立容量。
- **O-12f** `RamSourceDomain` 与 `OrdinaryRamMappingEligibility` 分开定义（四条入口各一组规则）。
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
| c 动态 `size` / `alignment` / `alloc-ranges` | **已实现**：`size`-only → `UnsupportedDynamicReservation`（`dtb_size_only_reservation_is_rejected_as_dynamic`）；无 `reg` 且无 `size` → 仍走 `UnsupportedReservedDeclaration`（`dtb_child_without_reg_and_without_size_is_an_unsupported_declaration`）；`reg`+`size` → 按 `reg` 处理、`size` 忽略（`reg_and_size_together_uses_reg_and_ignores_size`） | 形状分流与规范依据**均已闭合**（本轮取到主源原文）：`size` 独有=动态请求、「`reg`+`size` 按 `reg` 且忽略 `size`」、「Dynamic reserved memory regions must not be listed in the UEFI memory map because they are allocated by the OS after exiting firmware boot services」都是 DTSpec 3.5 逐字规定。因此 `UnsupportedDynamicReservation` 拒绝的是**规范允许、本项目不支持**的输入，不是非法 DTB。`alignment`/`alloc-ranges` 只在有 `reg` 时被忽略、未单独设错误——规范把它们定义为动态分配约束，静态处理时忽略是规定行为 |
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

**架构寄存器窗口、设备声明的 aperture 与页对齐映射包络是三件事**（x86 APIC 是最清楚的例子）：xAPIC 的寄存器接口只有 **4 KiB**，而当前政策授权是 `[0xFEE0_0000, 0xFF00_0000)`（**2 MiB**——上一版这里写 18 MiB，是我算错：`0xFF00_0000 − 0xFEE0_0000 = 0x20_0000`），IO-APIC 授权 `[0xFEC0_0000, 0xFEE0_0000)` 同样是 2 MiB。真正要回答的因此不是「授权范围太大」，而是**为什么 4 KiB 的实际访问需求要用一个 2 MiB 的设备映射叶子**。大范围映射本身不一定错：只要 profile 能证明整个 2 MiB 区域可按设备属性映射（不覆盖普通 RAM、地址空洞或属性不同的区域），设计就成立；但在 QEMU profile、实际 MMIO 资源描述与映射覆盖证据钉住之前，这个授权范围只是**待证平台假设**，不是已证事实。第一版若拿不出包络证明，就只映射实际需要的 4 KiB 区域、用更细粒度叶子。

```
ArchitecturalRegisterAperture   ← 架构规定的寄存器窗口（xAPIC 4 KiB）
PlatformDeviceAperture          ← 平台/固件声明的设备窗口
PageAlignedMappingEnvelope      ← 实际页表叶子的对齐包络
```

三者不能因为某个访问点落在 4 KiB 页里，就把周围数 MiB 自动变成设备窗口授权。

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

**AArch64 的 64 KiB 分组属性规则：按物理地址分组、只比分组属性、限定 UEFI profile**：UEFI 规定若一个 64 KiB 物理区间含任一 4 KiB 子页属于 `RuntimeServicesCode`/`RuntimeServicesData`/`ReservedMemoryType`/`ACPIMemoryNVS`，则该区间内全部 4 KiB 子页必须采用相同的 ARM memory page attributes。规范文本里的 `EfiReserved` 就是本项目的 `EfiReservedMemoryType`，是同一类型的两种写法，不是两个类型。规范位置为 UEFI 2.11 §2.3.6（评审人已按该节核对：引用 Table 2.5，权限映射在 Table 2.6）；本方本环境抓取 uefi.org 返回 403，**未直接取证**——两侧分开记：规则内容有评审人核对支持，「本方独立取证」仍是缺口，因此不升级为跨入口的架构通则。四条精度约束：

- **分组键是物理地址**：`AttrGroup(PA) = floor(PA / 65536) * 65536`。同一 VA 在不同树里的别名不改变分组，不同 VA 映射到同一 `AttrGroup` 才需要比较；按 VA 分组会漏掉跨别名违例。
- **比较字段只限分组属性**：memory type（Normal/Device 及其子型）、inner/outer cacheability、shareability。`R/W/X`、`PXN`/`UXN`、`AF` 等**访问权限逐页独立**（Table 2.6 一支），不进一致性比较——把权限塞进分组会禁掉合法的「同组不同权限」拆分，反向也会放过属性不一致。
- **比较对象是叶子最终生效的属性，不是 `AttrIndx` 数字**：写成 `EffectiveArmMemoryAttributes = (ResolvedMAIRAttrByte, Shareability)`，或规范化成等价的内存类型 + 内/外层缓存属性 + 共享性语义。两个不同 `AttrIndx` 若解析出完全相同的有效属性，不因索引编号不同而判冲突；反过来，同一 `AttrIndx` 在不同 PE 上对应不同 MAIR 内容**不算通过**（只读回索引相等证明不了任何事）。权限仍逐页独立，这条与上一项合起来才既不放纵违例、也不误禁合法的同组不同权限映射。
- **适用性按 profile 判定，不是无条件全局**：这条约束随 AArch64 **UEFI 启动 profile** 的属性递交而来；非 UEFI 入口（`-kernel` + DTB）没有 EFI memory map 属性递交，规则不自动适用，但架构侧是否也按 64 KiB 粒度取属性属另一项取证，未取到之前记为 profile 条件而非普适事实。第一版锁定 4 KiB granule **不等于**规则不适用。
- **未映射子页要有组级覆盖登记**：一致性检查的对象是同一 `AttrGroup` 内**已建立叶子**的属性集合；组内其余子页尚未映射不构成违例，但审计须记 `GroupCoverage`（该组已映射哪些子页）。任一后续新映射（低 4 GiB 宽恒等映射拆分、改属性尤其）都受既有分组属性约束；若某子页必须取不同的分组属性，只能整组改属性或拒绝，不能局部改。

它与 ACPI reclaim/NVS 描述符自身的 4 KiB 对齐约束是两条独立规则，不能混用。

**三个完整性主张强度不同，不能互相代替**：`Coverage ⊆ AuthorizedRange`（已映射区域未越权）、`RegisteredRequiredAccess ⊆ Coverage`（已登记需求可映射）、`ActualDeviceAccess ⊆ RegisteredRequiredAccess`（**未证明**：没有登记的设备访问是否存在，当前无任何证据）。最终报告必须分别陈述，不能拿前两项冒充第三项。

**数值相等 ≠ 来源相等**：表中若干行的授权范围与覆盖范围数值相同，但授权是**独立政策输入**，覆盖是**独立测量**；不能由实际叶子反推授权。而且今天**没有页表审计遍历**，所以今天的覆盖只能叫 `ExpectedDeviceMappingCoverage`（从构造代码推导），`ObservedDeviceMappingCoverage`（PTE 遍历）等 O-02a 的引导根审计落地后才存在。表中「覆盖」列在观测实现前一律标为**预期**。

**x2APIC 是第三种访问域**：xAPIC 走 MMIO（`0xFEE0_0000` 页），x2APIC 走架构寄存器（RDMSR/WRMSR）——它既不属于 MMIO 窗口模型，也不属于 O-08 的端口 I/O 地址域。**MSR 编号不进共享层**：共享层只表达 `ApicRegisterAccessRequirement`（APIC 逻辑寄存器、读写方向、访问者、阶段、CPU 范围、适用模式）；x86 架构层用 `X2ApicMsrUse` 记录具体 MSR 编号、指令位置、包装函数与访问约束。同一 OS 语义（读 Local APIC ID、写 EOI）在 xAPIC 映射到 MMIO、在 x2APIC 映射到 MSR。**第一版若不实现 x2APIC 访问路径**，必须锁定 xAPIC profile 并在检测到 x2APIC 时拒绝启动；不得「允许 x2APIC 模式却保留 xAPIC 的 MMIO 访问需求」。O-10 现有的 in/out 负控不覆盖 RDMSR/WRMSR：**架构寄存器访问必须进入独立的原始操作审计**——允许出现原始 MSR 访问的函数与指令位置清单、MSR 编号与共享层逻辑寄存器的对应关系、在已批准的 `asm!` 块中加第二条 `rdmsr`/`wrmsr` 必须使审计失败、x2APIC 模式下不得残留 xAPIC LAPIC MMIO 访问义务。

**x86 的 APIC 地址不是无条件硬件事实，而且核对是逐逻辑处理器的**：表中 `0xFEE0_0000`/`0xFEC0_0000` 只属于「本项目支持的某个 x86 QEMU profile」。`IA32_APIC_BASE` 是**每处理器 MSR**、xAPIC/x2APIC 模式也按处理器启用，因此不能只在 BSP 上核对一次——这条按「评审人已核的架构事实」使用，本方未取 Intel SDM 原文，登记为待核，不写成本方已验证的引用。启动与次级处理器入场时，对**每个参与内存访问与调度的逻辑处理器**分别核对：该处理器的 Local APIC 当前模式（xAPIC MMIO 还是 x2APIC MSR 路径）、该处理器的 `IA32_APIC_BASE` 基址与政策表一致、该处理器的 LAPIC 寄存器窗口是否已按对应模式获得映射或 MSR 路径准入。只读 BSP 不构成全平台证明——一个 AP 的基址不同，「政策授权一个 4 KiB 页」这条主张就不再对所有参与者成立。若解析 ACPI，MADT 的 Local APIC 地址、每个处理器条目的 processor ID 与 enabled flags、override 与 I/O APIC 项都要与政策表兼容；任一参与处理器不符合当前 profile 就**拒绝启动**或**阻止该处理器入场**。riscv 侧是同一形态的问题：PLIC 的 context 按 hart 列出（`interrupts-extended`），逐 hart 核对，不能由 hart 总数乘 2 推出来（见 PLIC 推导段）。

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
4. `ExitBootServices()` 因 key 失效失败 → **重新取图、重建来源记录与政策投影、重新审计**后再重试；不得沿用旧快照的 `PmmBaseAvailable` 与用途准入证据；
5. 重取后描述符数量与分段可能变化：容量不足、描述符增多、重试耗尽都要有明确失败形状，不能假设「只差一个 MapKey」。

**两次快照、两种用途（与 O-11 的时间顺序）**：

| 顺序 | 操作 | 使用的内存图 |
|---|---|---|
| 1 | 预分配内存图缓冲区与首次读取所需的最小工作区 | 尚无快照 |
| 2 | `GetMemoryMap()` → 检查 `[dtb, dtb+40)` 全部页属于 `EfiACPIReclaimMemory` | **`PreflightMap`**，只用于首次读取授权 |
| 3 | 读取一次 40 字节 header snapshot，校验 magic/版本/`totalsize`/边界 | 不再读原 header |
| 4 | 分配 DTB 自有副本与其余工作区；此后不再进行 Boot Services 分配 | `PreflightMap` 不再作为政策依据 |
| 5 | 再次 `GetMemoryMap()` → 检查完整 DTB 页范围属于 `EfiACPIReclaimMemory` | **`FinalMap`**，取得最终 `MapKey` |
| 6 | header snapshot 写入副本、只复制 body；在副本上完成校验、解析、来源记录、政策投影与审计 | 所有结果绑定 `FinalMap` |
| 7 | 不再调用会改变内存图的服务，立即 `ExitBootServices(FinalMap.MapKey)` | 同一最终快照 |

三条硬约束：**`FinalMap` 的完整类型检查必须在读 body 之前**（不能用 `PreflightMap` 的 header 验证代替整段授权）；**步骤 5 与步骤 6 之间必须再确认一次完整范围的 `BootInputReachability`**——`FinalMap` 只证明物理范围的类型与覆盖，不证明当前 VA 翻译、PMP/PMA 与访问路径仍可读，两者不能互推；**最终取图之后的解析/采集/投影/审计必须是无分配路径**（隐藏的动态分配同样会让 `MapKey` 失效）。`PreflightMap` 不进入 PMM 投影，也不与 `FinalMap` 的记录混用。

**缓冲区不足分三种情况，不能共用一个规则**：

| 发生位置 | 第一版处理 |
|---|---|
| `PreflightMap` 太小 | 尚未读 header：可以扩容后重取，或明确拒绝启动 |
| 首次 `FinalMap` 太小 | 尚未调用 `ExitBootServices()`：可扩容后重跑最终取图与审计；若坚持「步骤 4 后不再分配」，则明确拒绝 |
| 首次 `ExitBootServices()` 失败后的重取太小 | **保持现有决定**：复用缓冲区、不扩容、明确失败——此时固件可能已部分关闭 Boot Services，约束不同 |

**扩容即失效**：只要为扩大缓冲区调用了可能改变内存图的 Boot Services 分配，当前 `MapSnapshotId`、`MapKey`、来源记录与全部政策投影**整体失效**——必须重新 `GetMemoryMap()`、生成新快照身份、从新快照重建依赖结果。`MapSnapshotId` 用预分配或单调递增的本地序号生成，**不能通过新的动态分配创建**（观察实例的身份不应反过来改变被观察的内存图）。

**必测**（未实现）：① 预检时 header 页是 ACPI reclaim、最终快照类型改变 → 读 body 前拒绝；② 预检跨两个连续同类型描述符、最终快照分段变化但仍完整覆盖 → 允许；③ 最终快照验证后，解析与审计期间无额外分配、无改变内存图的服务调用；④ 三种 `EFI_BUFFER_TOO_SMALL` 场景分别落到上表对应处理。

**重试阶段的执行环境必须预先约束**（规范允许首次 `ExitBootServices()` 失败后固件已部分关闭 Boot Services）：

- 首次调用**之前**，重试所需的一切已分配：内存图缓冲区、分类记录、政策投影、审计工作区、诊断状态；
- 首次失败后，重试路径**只使用预留缓冲区与纯内存计算**：不调用文件系统、控制台、设备协议、事件服务或任何依赖完整 Boot Services 的功能；
- 复用同一内存图缓冲区，并预留描述符增长余量（规范明确：分配新缓冲区本身可能增大内存图）；重取返回 `EFI_BUFFER_TOO_SMALL` 时第一版**直接以明确错误终止**，不在部分关闭状态下扩容；
- 新快照必须**重建**全部依赖描述符的结果（来源记录、资格集合、PMM 投影、审计意见）；`MapKey`、记录与投影属于同一快照版本，不得混用；
- 只对 `EFI_INVALID_PARAMETER` 走 stale-key 重试；其它错误走独立失败路径；重试次数有上限；
- **重试期间不打印日志**：错误路径不得调用 Console 或协议服务取诊断——那正是首次失败后被限制的环境。诊断输出只在交接成功后由自有设施进行。

**静态准备前移，缩短「取图 → 交接」窗口**：对象登记表、静态设备政策、固定容量工作区与静态审计规则不依赖本次 EFI 快照，应在最终取图**之前**构建；最终取图后只做必须依赖本次快照的验证与投影，然后立即 `ExitBootServices()`。这不改变正确性要求——首次失败后仍须用新快照完整重建，旧投影不得复用。

重试内核是纯函数式阶段，逐级重建：

```
GetMemoryMap(reused_buffer) → ValidateDescriptorSnapshot
  → RebuildSourceRecords → RebuildPolicyProjections → Audit
  → ExitBootServices(new_map_key)
```

**必测负控**（未实现）：① stale key 后新快照描述符分段变化（一分为二/类型变化）→ 投影必须按新快照重建，不得沿用旧快照的基底可用集合与用途准入证据；② 重试缓冲不足 → 明确错误；③ 非 `EFI_INVALID_PARAMETER` 错误 → 独立失败路径；④ 重试次数耗尽 → 明确错误；⑤ 失败后的审计路径没有调用 Boot Services 协议或动态分配器。

**为什么单列**：它约束的是 UEFI 输入快照的**时间一致性**，不是描述符几何合法性（O-12h）；两者失败形态不同。

#### O-12h 的两级结构校验

| 层级 | 检查 |
|---|---|
| 内存图元数据 | `DescriptorVersion` 受支持；`DescriptorSize` 不小于已知前缀；`MemoryMapSize` 能被 `DescriptorSize` 整除**且不超过调用方缓冲区长度**；按 `DescriptorSize` 步进；字段读取不越出该条前缀 |
| 每个描述符 | `PhysicalStart` 对齐且不超上界；`VirtualStart` 按规范校验（**`0` 可接受**，它满足对齐；第一版验证后忽略）；`NumberOfPages` 非零，页数约束**分别对物理与虚拟起点**计算；两个末端都用 **u128**；**不论类型是否输出区间都执行** |
| 跨描述符 | **物理区间不得重叠**：两个描述符覆盖同一页却给出不同政策时，分类器无法给出唯一结果，必须拒绝启动 |
| 顶端边界 | **结构合法性与平台准入分开（已裁定，未实现）**：`[0xffff_ffff_ffff_f000, 2^64)` 是合法的最后一页，UEFI 只约束「页起始地址不超过 `0xffff_ffff_ffff_f000`」，不要求独占末端能用 `u64` 表示。结构校验用 `u128` 接受 `end == 2^64`；是否可用由平台地址准入层判定（`effective_pa_limit`、PMA/PMP 等），拒绝时用独立错误码。**代码已落到三层**：`SourceRecord.range` 已是 `PhysicalRange { start: u64, end_exclusive: u128 }`，**记录层接受 `end == 2^64`**；拒绝发生在投影层——`PhysicalRange::projectable_end()` 对最高页返回 `None`，`/memory` 与 `declared_page_range` 在那里报 `UnsupportedPhysicalAddressEnd`。输入层只拒 `> 2^64`（`RangeOverflow`）。**仍未落地的部分**：投影层算术仍是 `u64`，最高页不交付给 PMM/保留表；UEFI 描述符路径本就接受 `end == 2^64`。测试：`physical_range_expresses_the_top_page` 与四个边界用例 |
| 版本策略 | 只接受 `DescriptorVersion = 1`；其它版本明确拒绝（不是「受支持」的模糊说法） |
| 顶端页测试（待补） | `PhysicalStart = 0xffff_ffff_ffff_f000, Pages = 1` 通过；同起点 `Pages = 2` 拒绝；`VirtualStart` 同两点；`VirtualStart = 0` 允许但页数仍须满足虚拟上界；**以上对不进入任何分类输出的类型同样执行** |

#### O-12g 的附加要求

原始记录保留：声明所在节点、**每一级 `ranges` 转换的输入/输出与引用节点**、最终 CPU 物理地址、属性与来源身份。只保留最终地址会让审计无法解释地址是怎么来的。

**四条补充要求**：① **排除原因多重归属**——同一页可同时被普通保留、MRB、存活对象覆盖，PMM 只扣一次，但审计能列出全部 `SourceId`；② 固件前缀、内核镜像、boot shim、自举页表、栈、启动模块、DTB 原始对象与副本、登记表**不能合并成一个 Reserved 标记**，它们各自有阶段化读写/执行需求；③ **重叠规则**：**同一权威来源内部的同语义重复**（例如两个 `/memory` 节点描述同一段 RAM，或同一节点多个 `reg` 重复）集合去重、保留多个 `SourceId`；**跨来源证据**（UEFI 类型、平台分类、固件保留与 RAM 来源）对同一物理区域提出**不兼容主张**时进入冲突裁决，不能靠并集掩盖——不兼容即拒绝启动。`/memory` 本身只有 RAM 语义（`status` 非 `okay` 与 `hotpluggable` 已被准入拒绝），不存在「同源一条说 RAM、一条说不可用」的形态；④ **原始记录必须保留不输出区间的描述符**（含 `RESERVED`/`UNUSABLE`）——否则 O-06 的「`UNUSABLE` 属于来源域」没有可实现的输入；诊断成员表是面向内核的另一层，不能替代原始记录。

**状态按三层记（O-12g）**：

| 层次 | 状态 | 证据 |
|---|---|---|
| 库接口 + 首个启动消费者 | **已实现（第一阶段）** | `audit_source_coverage(free, reserved, sources)`：每条 free 投影必须被 `Memory` 记录链覆盖，每条 reserved 投影必须被保留类记录链覆盖，否则 `SourceAuditError`；riscv bootface 已在启动路径上调用它 |
| 库接口：能产出带身份的记录 | **已实现（第一阶段）** | `SourceRecord { id, kind, origin, range: PhysicalRange }`；`class()` 区分输入声明与派生 `FirmwarePrefix`；前缀带 `{ memory_id, cut }` 派生链；精确重复声明去重投影、保留两条来源记录；容量由输入上限推出，恰好 40 可达 |
| 启动路径：真实启动生成并保留记录 | **部分接入** | riscv64 bootface 已走 `collect_with_sources()` 并调用 `audit_source_coverage`；其余三个 DTB 调用点仍走 `collect()`，记录被丢弃（§2.5） |
| 消费者：政策投影实际引用记录 | **未接入** | `PolicyProjection` 尚无代码 |

**已解决的旧冲突（按来源分治）**：`/reserved-memory` 的精确重复按**三种语义**（普通/`reusable`/`no-map`）分别去重、来源各留一条，跨语义同范围一律冲突；MRB 的精确重复**必须拒绝**（DTSpec 5.3.2）；`/memory` 的部分重叠按项目策略拒绝。六条测试见 §2.1。

**合法记录与拒绝诊断记录分开**（未实现）：结构校验失败的描述符不进入来源记录层，但必须留下**拒绝诊断记录**（`DescriptorIndex`、原始 `Type`、`PhysicalStart`、`VirtualStart`、`NumberOfPages`、失败检查项）——结构错误发生时恰恰最需要来源信息，不能因为「没通过校验」就只剩一句错误码。

**统一 `SourceRef` 必须在接通启动路径前定型**（未实现）：`SourceRef { input_object: InputObjectId, local_source: LocalSourceId }`；`FirmwarePrefix` 对 `Memory` 的引用也用它，不再保留裸 `memory_id`。**UEFI 的 `InputObjectId` 是快照实例身份，不是来源类别**：每次成功的 `GetMemoryMap()` 分配新的 `MapSnapshotId`（`MapKey` 只是该快照的固件版本凭据）；`PreflightMap` 不产生最终政策记录，`FinalMap` 的全部描述符同属一个快照 ID，stale-key 重试产生新 ID 且旧记录与旧投影整体废弃——否则三份快照里同号的 `DescriptorIndex` 会被当成同一条来源。这样每条政策、排除原因、禁止项、对象义务共用同一身份，也防止把两个输入对象里的 `SourceId(3)` 当成同一条来源。节点路径文本与逐级 `ranges` 转换链可分阶段补，不必为接通内存记录先做完整地址翻译。

**UEFI 原始描述符记录的最小字段**（未实现）：`DescriptorIndex`、原始 `Type`、原始 `Attribute`、`PhysicalStart`、`VirtualStart`、`NumberOfPages`、结构校验结果；每个通过结构校验的描述符先成为带身份的记录，再进入类型裁决——裁决可以拒绝启动，但不能在来源层消灭 `RESERVED`/`UNUSABLE`/MMIO/类型 11/12；导致拒绝的 `PAL_CODE`/`UNACCEPTED` 至少要在错误上下文保留索引、类型与范围。

**仍缺的字段**：节点路径（今天只记 `origin` 类别 + 序号，多个 `/memory` 节点靠 `id` 区分，没有路径文本；序号的意义绑定输入对象与遍历顺序，跨对象不可解释）、每级 `ranges` 转换链、`SourceRef` 的统一类型（今天只有顶层 `id` 与 `FirmwarePrefix{memory_id,cut}` 的内嵌引用，尚无统一类型）、UEFI 侧未输出描述符的记录。顺序不变：先把记录接到启动路径并由政策层持有，再谈 O-02/O-03 的禁止集合编译；否则 DenyStandardSystemMapping、DenyUncontrolledSpeculativeAccess、MRB 访问禁令与「同页多重排除理由」都没有输入来源。

**记录层与原始输入对象分开**：原始记录保真**不等于**永久保留整份 DTB 或 EFI 内存图缓冲区。`SourceRecord` 是规范化记录（保留到政策投影与审计不再需要）；原始 DTB 副本只在解析与转换期间需要，最后一个消费者结束后可以释放。`ranges` 转换链（每级输入/输出地址、引用节点、转换属性）属于记录层，不能只留最终物理地址。

#### O-12i 每种投影的页粒度规则

| 投影 | 边界处理 |
|---|---|
| `RamSourceDomain` | 保留原始 byte range；只有构造页域时才说明边界处理 |
| `PmmBaseAvailable` | 只交付完整落入合法 RAM **且落入用途无关准入域**的整页（向内收缩） |
| `PmmBaseExcluded` | 对保留范围**向外覆盖整页**，部分相交的页不得进入基底可用 |
| `PmmRoleRejected` | 不做量化：它标注的是基底内某页缺哪条用途证明，不产生页集合边界 |
| `no-map` 映射禁止 | 向外覆盖所有相交页，防止大页/小页叶子跨入禁止区域 |
| MRB 访问禁止 | 保留精确 byte range；若通过页表落实，另做页粒度投影 |
| 设备窗口映射 | 叶子覆盖的**全部页**都要有兼容授权，不能只看起始页 |

**用途集合是分配时的凭证，不是事后可以随意扩大的标签**：分配交付的是 `(page, U_verified, p)`。① `U_verified` 记录交付时已证明的用途集合，页表 / 可执行 / 原子用途必须显式在里面；② 承担新用途（数据页改作页表、页表页改作可执行、任意页首次做 AMO）要**先重新准入再使用**——对 `U_verified ∪ {新用途}` 重跑 `PmmRoleEligible`，且用当下的 `H`（参与 hart 集合）与 `p`（阶段）重取证据，因为 hart 入场与阶段切换都可能使旧证明失效；③ **撤回用途不释放容量**：`U` 变小不产生新的可用页，一页能否回到 `PmmBaseAvailable` 只由生命周期决定（存活对象是否解除、保留是否撤销、`UNACCEPTED` 是否接受），不由用途标签变化决定——否则「改个标签」就凭空多出内存。

**所有 `PMM*` 集合都是 4 KiB 页集合**：`TypeAllocatablePages(p)` 表示「整页都满足阶段 `p` 的类型资格」的页；`PlatformAllocationDenials(p)` 也必须是页集合（或先向外量化）。来源原始范围仍是 byte range，只有进入指定投影时才量化；**阶段资格变化必须由显式事件触发**，不得随时间推进自动释放。

**`FullPages` 以页编号定义**：

```
PageBytes(n) = [ u128(n) * 4096, (u128(n) + 1) * 4096 )   ← 右端独占
FullPages(R) = { n | PageBytes(n) ⊆ R }
```

参数是页编号而不是物理地址或 byte range；`0 ≤ n < 2^52`（4 KiB 页、64 位物理地址），边界先转 `u128` 再运算，最高页的独占末端可以是 `2^64`。**两端都是半开区间**，与全文集合模型一致。只有整页 byte range 都被 RAM 来源覆盖才进入页域，不是「起点向上、终点向下后拼接」。
**页域与两层 PMM 的正式定义**（基底用补集，用途用子集——用途层不构成第二次划分）：

```
MemoryAdmissionContext {
    phase,                        ← 阶段 p
    participating_cpu_set_id,     ← 参与访问的 CPU 集合身份（不是数量）；RISC-V 成员叫 hart，该术语留在 adapter
    platform_memory_profile_id,   ← 平台内存配置：QEMU/SoC 的实际窗口与硬件形状
    isa_profile_id,               ← ISA Profile（如 RVA23U64/RVA23S64）；「不声明」= 未作合规声明，不等于已合规
    uefi_attribute_profile_id,    ← 固件属性位编码 profile：实际采用哪一套位组解释
    evidence_generation,          ← 架构准入证据的代次
    source_snapshot_set_id,       ← 来源快照集合身份（O-12k 的 MapSnapshotId 属此），投影版本键的一部分
    legal_starts_contract,        ← Page | Object；进 AdmissionContextId，切换必须重建全部投影
    adapter_evidence              ← 各架构证据槽位：RISC-V 放 pte_update_scheme_id / PmpEvidence /
                                    SystemHartInventoryId；共享层只要求 adapter 回答同一语义问题
}
ParticipatingCpuSet = Resolve(participating_cpu_set_id, inventory_snapshot)
    ← 身份解析成成员集合：不可变，成员与 evidence_generation 绑定；共享公式只用 ParticipatingCpuSet，
      `ctx.H` 只是 RISC-V adapter 内的写法，不是第二个隐含字段

RamSourcePageDomain = FullPages(Union(ResolveCompatibleClaims(RamSourceDomain)))
                                                           ← 先裁决兼容性主张，再并集，再取完整页

PmmBaseAvailable(ctx) = RamSourcePageDomain
    ∩ TypeAllocatablePages(ctx.phase)
    ∩ OrdinaryCachePolicyRepresentablePages(ctx)      ← 预映射可行性，不读实际叶子
    ∩ MemoryOrderingCompatiblePages(ctx)              ← 内存序 PMA（见下）
    ∩ CommonCpuAccessEligiblePages(ctx)               ← 与用途无关的物理访问准入
    ∩ ProfileMemoryEligiblePages(ctx)                 ← 声明 ISA Profile 时该 profile 对主存区域的强制义务
    \ Outward(ExplicitAllocationReservations(ctx))
    \ Outward(LiveObjectRanges(ctx))
    \ PlatformAllocationDenials(ctx)

PmmBaseExcluded(ctx) = RamSourcePageDomain \ PmmBaseAvailable(ctx)   ← 补集：不相交且覆盖整个页域

RoleAdmit(role, page, ctx) = 该用途**在基底之上**的额外准入
                              （页表遍历与 PTE 更新方案 / 取指 / 原子指令族、进展与预约集几何）
                              ≡ RolePageEligible(page, role, ctx)   ← 唯一操作化定义，见下节

PmmRoleEligible(page, U, ctx) = page ∈ PmmBaseAvailable(ctx)
                                 ∧ U ≠ ∅                                   ← 空用途集合不得真空通过
                                 ∧ ∀ role ∈ U : RoleAdmit(role, page, ctx)
PMMAvailable(U, ctx)          = { page | PmmRoleEligible(page, U, ctx) }

PmmRoleRejected(role, ctx) = { page ∈ PmmBaseAvailable(ctx) | ¬RoleAdmit(role, page, ctx) }

MappingIntent = StandardSystemMapping | DriverControlledMapping(device_id, access_use_id) | OtherExplicitMapping(…)
MappingAudit(leaf, tree, phase, intent)（对每个实际叶子，不是对物理范围）：
    MappingAuthorizationCovers(leaf, tree, phase, intent)      ← 标准映射受 DenyStandardSystemMapping 约束
    ∧ DenyMappingDoesNotIntersect(leaf, tree, phase, intent)   ← 驱动受控映射须有设备身份、范围、阶段、路径与例外授权
    ∧ DenyUncontrolledSpeculativeAccessSatisfied(leaf, intent) ← 独立审计，不因一条受控授权整体豁免
    ∧ LeafCoverageHasCompatibleSources(leaf)
    ∧ 映射属性符合目标语义 ∧ 物理范围属性允许该映射 ∧ 各 PE 属性一致 ∧ 别名兼容
    ← 设备窗口 Coverage ⊆ AuthorizedRange 不能代替 Coverage ∩ DenyMapping = ∅；
      `EfiReservedMemoryType` 的 `DenyOsUse` 第一版绝对禁止，不走驱动分支
```

**投影版本键**：`PolicyProjectionKey = SourceSnapshotSetId + AdmissionContextId + EvidenceGeneration`。同一 `ctx` 下**不得**复用上一次 `FinalMap` 派生的记录或投影；新 `MapSnapshotId` 即使描述符数量与布局完全相同，也必须重建——否则 O-12k 的快照绑定只是文字。

**`H` 是输入，不是背景**：参与 hart 集合会改变结论，所以它必须进签名。失败形态很具体——参与集合原本只有 hart 0，它对某区域具备普通读写；hart 1 入场后其 PMP 拒绝该区域，`CommonCpuAccessEligiblePages` 的旧结果立刻失效；若投影只由阶段 `p` 标识，缓存就会继续交付过期结论。因此**任何参与 hart 集合变化、架构证据换代或 `PteUpdateScheme` 变更，都必须使依赖它的证据与全部投影失效**——这与 O-12k 的 `MapSnapshotId` 是同一条版本化原则：结果绑定生成它的证据快照。`participating_cpu_set_id` **不能取成员数量**：数量相同的两个集合既可能是不同的成员，也可能 PMP 配置完全不同。此前签名只写 `(p)`、正文却用 `H` 取交集，等于把一个真实输入维度藏进口头描述。

**公式里的第一步是裁决，不是并集**：`RamSourceDomain` 是未裁决的原始主张集合，冲突/不兼容来源必须先按 §3 的四层规则（来源事实层保留身份 → 冲突解析层裁决 → 纯集合投影层才允许 `Union`）处理，`Union` 只作用于**已裁决的兼容区间**。把公式写成「原样并集再取整页」会让读者以为冲突主张可以无条件合并，那是 §3 明令禁止的。

**为什么要拆两层**：用途化以后，「可用 ⊍ 排除 = 页域」不再对单个 `PMMAvailable(U, ctx)` 成立——同一页可以 `GeneralData` 可用、`PageTable` 拒绝，若仍写成一张 `PMMExcluded(p)`，这一页会同时出现在可用与排除里，§3 的「同一页只排除一次、排除理由全部可追溯」也就无从记账。因此只有**基底层**保留补集性质：

- `PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx) = RamSourcePageDomain`；排除的唯一性、多重归属的可追溯性都在这一层成立。
- `PmmRoleRejected(role, ctx) ⊆ PmmBaseAvailable(ctx)`：**用途拒绝不缩小基底可用集合**，也不得计入 `PmmBaseExcluded`；它只表示「这段页不能承担该用途」，不是「这段页被排除出内存」。
- 任何「可用」主张必须同时带用途集合与准入上下文：裸写 `PMMAvailable(·)` 不是良定义项。
- 用途集合**可组合、不互斥**，不是页的分类：`U ⊆ U′ ⇒ PmmRoleEligible(page, U′, ctx) → PmmRoleEligible(page, U, ctx)`（单调），同一页可同时属于多个 `U` 的可用集合。
- 分配时用途未知，只能保守取 `U = AllRoles`（等价于旧的「保守全局集合」）；这与用途化仍是二选一，**不得**同时声称「按用途准入」和「一页一用途类」。本项目选用途化 + 未知即 `AllRoles`。

**能力、硬件准入与平台拒绝是三项，不能合并**：

- `OrdinaryCachePolicyRepresentablePages(profile)`：**预映射可行性**（跨架构语义，不以 UEFI 术语命名）——物理范围与硬件配置是否存在**某种**合法映射编码可兑现普通主存缓存语义：x86 看 MTRR 状态与可用 PAT 项能否表达 WB；AArch64 看所有参与 PE 的 MAIR 中是否存在可用的 Normal Cacheable 属性；RISC-V 看平台 PMA/机器级配置能否满足目标。**它不能读实际叶子**——PMM 计算时叶子还不存在。
- `ObservedMappingEffectiveAttribute(mapping, arch_state)`：**实际兑现**——叶子选择的编码与硬件状态组合后的最终属性（x86 叶子 PAT 索引×MTRR、AArch64 AttrIndx×MAIR、RISC-V PBMT×PMA），只进 `MappingAudit`。把后者塞进 PMM 会造成「先有 PTE 才能决定能否分配、分配又要求先有 PTE」的循环。
- 物理访问准入按用途分成**两半**：`CommonCpuAccessEligiblePages(ctx)` 是与用途无关的基底部分，`RoleAdmit(role, page, ctx)` 是用途特有部分；四格内容以正文的 `RoleSpecificRequirements(role, ctx)` 正式定义为准，不另列表格。

**原子用途要把「指令可执行」与「算法进展假设」分开**：主源把 LR/SC 支持分三级——「RsrvNone indicates that no LR/SC operations are supported (the location is non-reservable). RsrvNonEventual indicates that the operations are supported (the location is reservable), but without the eventual success guarantee described in the unprivileged ISA specification. RsrvEventual indicates that the operations are supported and provide the eventual success guarantee.」因此光登记指令族与宽度不够，用途记录要带：

```
AtomicPageRoleRequirement {          ← 页级能力：候选页能否承担这类原子状态（**不含**具体物理地址）
    operation_family, access_width, progress_requirement, reservation_set_requirement, fallback_plan_id,
}
AtomicAccessInstance {               ← 对象级实际访问：已存在对象真正发生的原子访问
    access_use_id, va_range, expected_physical_footprints, operation_family, access_width,
    reservation_set_evidence, required_cpus,
}

ReservationSetRequirement { base_page_containment, profile_geometry }
    ← base_page_containment：页式 VM ∧ A 扩展 ⇒ 预约集整段落在单个基础 4 KiB 页内（`norm:lr_sc_rsrv_set_page`，第一版即成立）
    ← profile_geometry：声明 profile 时启用（Za64rs：连续、自然对齐、≤64 B）；未声明记「不适用」
ReservationSetGeometryPermits(cpu, physical_range, requirement, ctx.isa_profile_id)
    = requirement 为空（AMO-only）∨ ( 基础页内约束 ∧ profile 几何（若声明） )   ← 逐 CPU、逐实际访问范围判定

AtomicFallbackPlan {
    trigger, bounded_retry_policy,       ← 何时判定缺乏进展、如何避免无限重试
    required_operations, required_widths, required_memory,   ← fallback 自己用到的操作、宽度与内存区域
    progress_argument,                   ← 为什么 fallback 本身能完成工作
}
```

**`fallback` 必须是可追溯的计划引用，不是布尔值**：一个「有 fallback」的布尔标记可能只说明「源码里还存在另一个函数」，而那个函数自己可能也在用 LR/SC、或依赖该区域并不支持的 AMO。计划至少要证明四件事：① 能完成**同一算法操作**；② 不依赖刚被判为不足的 LR/SC 进展保证；③ 它实际使用的操作与宽度在该页上**确有 PMA 支持**（走同一条 `RoleAdmit`，不能用未经验证的操作替代另一个未经验证的操作）；④ 检测到缺乏进展时**真的切换**过去。合同必须显式覆盖两点：**如何检测需要 fallback**（`trigger` 与 `bounded_retry_policy`，不能把「存在另一个函数」当成检测机制），以及**fallback 自身依赖的操作、宽度与内存区域也要过准入**（`required_operations`/`required_widths`/`required_memory`）——包括 fallback 用到的锁变量、状态字与它们的访问路径，不能出现「fallback 依赖一块从未过准入的共享内存」。

```
AtomicRoleEligible(page, requirement, ctx) =        ← 页级：用 LegalStarts 全称判定，不携带具体物理地址
    ∀ req ∈ ExpandAtomicRequirement(requirement) : LegalStarts(page, req, AtomicSharedState, ctx) ≠ ∅
        ∧ ∀ a ∈ LegalStarts(...) ∀ cpu ∈ ctx.ParticipatingCpuSet : PhysicalAccessPermits(cpu, req, Footprint(a, req))
    ∧ ProgressRequirementSatisfied(requirement, ctx) ∧ ReservationSetGeometryPermits(page, requirement, ctx)
RoleAdmit(AtomicSharedState, page, ctx) = RegisteredAtomicAlgorithms(ctx) ≠ ∅
    ∧ ∀ algorithm ∈ RegisteredAtomicAlgorithms(ctx) : AtomicRoleEligible(page, algorithm.requirement, ctx)
    ← 对象级实际访问另由 `AtomicAccessInstance` 在 `ObjectAccessEligibility` 里判；两层不共用同一描述
```

**第一版策略（统一，不再两处说法不一）**：所有实际使用 LR/SC 的第一版算法一律要求 `RsrvEventual`；`RsrvNonEventual` 与 `AtomicFallbackPlan` 路径整体留待扩展，不进入第一版准入。规范允许「`RsrvNonEventual` + 检测到无进展时 fallback」，但那条要求 `FallbackPlanSatisfied` 真的检查计划完整性与 fallback 自身依赖的操作/宽度/内存——第一版没有这份证据，因此不开放该分支。

- **只用 AMO 的算法**：要求相应 AMO 家族与宽度即可，不因它要求 `RsrvEventual`；`reservation_set` 为空、不检查预约集。
- **LR/SC 的几何、进展与页内约束是三件事**：基础页内约束（`norm:lr_sc_rsrv_set_page`，第一版即使不声明 profile 也成立）∧ 进展要求（第一版一律 `RsrvEventual`）∧ 声明 profile 时的几何（`Za64rs`）。三项并列，缺一不得通过原子用途准入。**不依赖最终成功的 `RsrvNonEventual` + fallback 路径整体不开放**：规范允许它，但前提是 `FallbackPlanSatisfied` 有真证据，第一版没有。
- 「We recommend providing RsrvEventual support for main memory regions where possible」是规范的 NOTE（推荐，不是强制），不能当准入依据引用。
- **声明符合的 ISA Profile 会追加全局 PMA 义务，只查当前源码恰好用到的指令不够**。这里有两个不同的 profile，不能合并：

| 概念 | 记录什么 |
|---|---|
| `PlatformMemoryProfile` | QEMU / SoC / 硬件的实际配置（machine version、accelerator、PMP 布局、hart 启动方式…） |
| `ProfileMemoryObligations(isa_profile, region)` | **ISA Profile**（RVA20/RVA22/RVA23 的具体版本）对**指定主存范围**提出的 PMA 义务——义务只对具备相应 cacheability/coherence PMA 的区域成立，因此 profile 与 region 都是输入 |

  Profile 义务按**来源**分三类，不能混：

| 义务来源 | 处理 |
|---|---|
| 所声明 Profile 的 **Mandatory Extensions** | 必须全部兑现 |
| **Optional / Development Options** | 只有项目明确选择该扩展时才纳入 |
| **项目自身用途要求** | 不依赖 Profile 声明，单独准入 |

  与内存政策直接相关的条目（以 RVA23U64 的当前公开文本为例）：`Ziccif`、`Ziccrse`、`Ziccamoa`、`Zicclsm` 属**强制**；`Ziccamoc`（AMOCASQ 级支持）属 **Development Options**，**不是** RVA23U64 的强制基线；`Ssccptr`（硬件页表读取）是 **RVA23S64** 的强制义务——**U64 与 S64 的义务不能混写**。AMO 能力自身也分级（`AMONone` / `AMOLogical` / `AMOArithmetic` / `AMOCASW`/`AMOCASD`/`AMOCASQ`，CAS 级以 AMOArithmetic 为前提）。

  **取指要拆成两件事**：`InstructionFetchSupported(width)` 是**可执行用途的基础准入**（这页能不能按该宽度取指，进 `RoleSpecificRequirements(Executable)`）；`InstructionFetchAtomicity(width)` 是**Profile 义务**（是否满足某 Profile 或代码更新机制要求的取指原子性，进 `ProfileMemoryObligations`）。第一版不声明 Profile，不等于 `Executable` 的宽度条件失去定义——两者不在同一个集合里。

  规则：**`RoleAdmit` 检查项目实际用途的需求，`ProfileMemoryObligations` 检查所声明 Profile 的强制义务，两者取交集**。`Executable` 的取指准入不能替代 `Ziccif` 的取指原子性；`AtomicSharedState` 的用途准入也不能替代 `Ziccamoa` 对**整段**符合条件主存的约束。

  **第一版的项目决定：不声明 RVA20/RVA22/RVA23 等任何 RVA Profile**（列为待定事项）。`ProfileMemoryObligations` 因此是**空集合**——空集只表示「未声明」，**不得读成「已满足」**；一旦将来声明，必须按**钉住版本的 Profile 全文**导入该 Profile 的完整强制义务（含本节未列的条目），不能只挑一条提升为全局规则。RVA20/RVA22/RVA23 各自的确切版本与能力清单要分别钉住（见 §6 的证据边界段）。
**页表用途的准入不是 PMM 的私有步骤**：引导阶段已存在自举页表根、页表树与静态分配的页表页，它们**不来自 PMM**。若把页表准入只写在分配路径里，未来分配会被正确拒绝，而更早建立的那批页表页从未经过同一项检查。

**但上一版写成 `page ∈ PmmBaseAvailable(ctx) ∧ RoleAdmit(...)` 是错的**：`PmmBaseAvailable` 扣除了 `Outward(LiveObjectRanges(ctx))`，而既有引导页表恰恰是存活对象——**越是被使用的页表页，越不可能通过这个谓词**。这是把两个不同问题写进了同一个断言：① 该物理页的硬件能力是否允许承担页表用途；② 该页当前是否处于可供 PMM 分配的状态。拆成三个：

```
PageTableBackingEligible(page, ctx)
    = 来源与类型允许
      ∧ 普通访问准入通过
      ∧ 页表读取（ImplicitPageWalk / Read）准入通过
      ∧ 内核显式 PTE 读写准入通过              ← 与 A/D 方案无关，两种方案都必需
      ∧ 按 A/D 方案要求的 PTE 更新准入通过
        （HardwareManagedAd 另需 ImplicitHardwarePteUpdate：
         隐式 W 权限 + 硬件页表写能力 + 相对同一 PTE 的原子更新 + RsrvEventual）

PageTableAllocationEligible(page, ctx)
    = page ∈ PmmBaseAvailable(ctx) ∧ PageTableBackingEligible(page, ctx)

ExistingPageTableObjectEligible(object, ctx)
    = 对象身份与存活登记有效
      ∧ PageTableBackingEligible(object.page, ctx)
      ∧ 对象的映射、权限与访问路径通过审计
```

职责：`PageTableBackingEligible` 是**一份**硬件能力合同；PMM 只在交付新页表页时把它与分配资格合成 `PageTableAllocationEligible`；对象审计对**已存在**的页表对象用 `ExistingPageTableObjectEligible`——不要求对象重新成为可分配页，只要求它有来源、有身份、能力合格、路径可审计。两个消费者共用同一份 backing 能力，而不是共用一个带「可分配」语义的谓词。

若第一版不愿引入用途分类，则只能把上表全部并入基底、保留**保守的全局集合**（所有基底可用页都满足页表与全部通用用途），此时**不得**同时声称「按用途准入」和「一页一用途类」。
- `PlatformAllocationDenials(ctx)`：**平台策略显式拒绝**，不再兼任架构准入。
- `MemoryOrderingCompatiblePages(ctx)`：**内存序 PMA 的落点**（此前只在 RISC-V 讨论里被提到，没有归属集合）。Machine-Level ISA「Memory Ordering PMA」小节主源逐字规定：「Regions of the address space are classified as either *main memory* or *I/O* for the purposes of ordering by the FENCE instruction and atomic-instruction ordering bits.」以及「Coherent main memory regions always have either the RVWMO or RVTSO memory model. Incoherent main memory regions have an implementation-defined memory model.」内核的同步原语按其中一种模型书写，因此：① 该集合进入**基底层**（用途无关）——用途特有的原子要求仍归 `RoleAdmit(AtomicSharedState, ·, ctx)`；② **第一版不支持由软件维护一致性的非一致主存**：区间被声明为主内存但 Coherence PMA 显示非一致 → 无论其内存模型能否被证明，都不得进入 `PmmBaseAvailable(ctx)`；实现自定义排序的区域同理被排除。规范在「Coherence and Cacheability PMAs」小节逐字把两件事分开——「Coherence is a property defined for a single physical address, and indicates that writes to that address by one agent will eventually be made visible to other coherent agents in the system. **Coherence is not to be confused with the memory consistency model of a system**, which defines what values a memory read can return given the previous history of reads and writes to the entire memory system.」并写明「the use of hardware-incoherent regions is discouraged due to software complexity, performance, and energy impacts」。两条规则各答各的问题：

| 集合 | 回答的问题 |
|---|---|
| `CommonCpuAccessEligiblePages(ctx)` 的 coherence 条件 | 多 hart 对共享物理地址的访问能否满足内核的一致性要求 |
| `MemoryOrderingCompatiblePages(ctx)` | 区域实际使用的内存模型是否满足内核同步原语的假设 |

若将来要支持软件维护一致性，需要另立同步与访问合同，而不是让一份 memory-ordering profile 证明**顺便**替代 coherence——否则同一条准入规则会在两处表达不同含义；③ RVWMO 与 RVTSO 之争是**软件假设**问题：内核按 RVWMO 书写时 RVTSO 区域也满足（更强），反之不成立，故 profile 记录「内核假定模型」而非只记录区域模型；④ 「为 FENCE 与原子排序位目的」这句限定了该分类的用途——它约束的是排序语义，不是来源域成员资格，不能拿它替代 `RamSourceDomain`。
- **幂等性不能反过来证明是主内存**：规范的小节名是 **Idempotency PMAs**，属性对是 `Idempotent` / `Non-idempotent`。上一版写的 `Non-resident` 是**本方杜撰的名称**，本轮撤销（当前主源里根本没有该 PMA 名）。逐字规定：「Main memory regions are assumed to be idempotent」；对 I/O 区域，读与写的幂等性可**分别**规定（例如读幂等、写不幂等）；「If accesses are non-idempotent, i.e., there is potentially a side effect on any read or write access, then speculative or redundant accesses must be avoided」；以及「For non-idempotent regions, implicit reads and writes must not be performed early or speculatively」（隐式读写不得提前或推测执行，主源对该条另有页表遍历相关的例外分支）。方向因此仍是「先由来源域认定主内存，才谈得上排序/幂等属性适用」，「观察到读写无副作用」不是 RAM 来源资格证据；反过来，主存区域被**假定**幂等是规范的默认，不需要额外证据，也就不能拿它当成区分主存与 MMIO 的判据。非幂等区域禁止提前/推测隐式访问这一条，正是页表内存必须落在幂等区域的原因，但它属于 `PageTableBackingEligible` 的证据，不由幂等 PMA 单独决定准入。AArch64 与 x86 无对应 PMA 概念，其可缓存性等价要求由 `OrdinaryCachePolicyRepresentablePages` 与实际叶子审计承担，本节集合对非 RISC-V 架构按「由 profile 声明为兼容」处理，不得留空当作已通过。

**架构证据按问题分层，不能汇总成一个布尔值**：

| 资格 | 架构证据 | 何时验证 |
|---|---|---|
| `OrdinaryCachePolicyRepresentablePages` | UEFI `WB` 能力位 / DTB 平台属性政策 | 来源记录阶段 |
| `MemoryOrderingCompatiblePages` | RISC-V 内存序 PMA（一致主内存 = RVWMO/RVTSO；非一致 = 实现自定义）；其它架构由 profile 声明 | 来源记录阶段 |
| `CommonCpuAccessEligiblePages` | 参与 hart 集合的普通读写与一致性；RISC-V 有效 PMP 权限、PMA 主存分类 | 准入阶段，用途无关 |
| `RoleAdmit`（用途特有） | RISC-V 按**实际生效的 A/D 更新方案**条件化（见下表）；原子操作按 AMO / LR-SC 与宽度分别登记；其它架构的页表遍历、取指限制 | 准入阶段，按用途 |
| 映射属性兑现 | x86 `PAT` 项 × 物理范围 `MTRR` 有效类型；AArch64 `AttrIndx` × `MAIR_ELx`（读回 MAIR 不足以证明叶子选对了索引）；RISC-V `PBMT`/PMA | **逐实际叶子**（`MappingAudit`） |

**RISC-V 的 A/D 更新方案决定页表页准入**（不能无条件要求 `RsrvEventual`）。规范原文本轮已从主源 `riscv/riscv-isa-manual` 的 `src/priv/supervisor.adoc` 与 `src/priv/machine.adoc` 直接取到，逐字引句带规范锚点：

> Two schemes to manage the A and D bits are defined: … The *Svade* extension: when a virtual page is accessed and the A bit is clear, or is written and the D bit is clear, a page-fault exception is raised. … When the Svade extension is not implemented, the following scheme applies. … The page tables must be located in memory with hardware page-table write access and *RsrvEventual* PMA. … All harts in a system must employ the same PTE-update scheme as each other.（锚点 `norm:pte_ad_scheme_uniform`）

| 条件 | 硬件自动更新 A/D（分支结果为 `HardwareManagedAd`） | Svade 软件处理 A/D（分支结果为 `SupervisorFaultManagedAd`） |
|---|---|---|
| 硬件页表遍历读取 | 必须 | 必须 |
| 硬件页表写入能力 | 必须 | 不因 A/D 更新而要求 |
| `RsrvEventual` | 必须 | 不因 A/D 更新而要求 |
| S-mode 软件写 PTE | 内核更新页表时必须（PMP 仍检查写权限） | 同左 |
| PMP | 覆盖硬件隐式页表访问 | 覆盖隐式读取与软件显式写入 |

**`ADUE=0` 与「未实现 Svadu」都推不出软件 A/D 更新**。Machine-Level ISA 的 `menvcfg` 段（锚点 `norm:menvcfg_adue_op`、`norm:menvcfg_adue_rdonly0`）逐字规定：`ADUE=1` 时启用硬件更新且「behaves as though the Svade extension were not implemented」；`ADUE=0` 时「behaves as though Svade were implemented」；未实现 Svadu 时 `ADUE` 是只读 0，并附注「hence Svade is implemented when ADUE=0」。因此 **Svadu 存在时 `ADUE` 覆盖 Svade 的独立实现状态**，方案用分支表达：

```
PteUpdateScheme(hart) =
    if SvaduImplemented(hart):
        match EffectiveADUE(hart):
            1       => HardwareManagedAd        ← 即使 Svade 也已实现，ADUE=1 令其不生效
            0       => SupervisorFaultManagedAd
            Unknown => Unknown
    else:
        match SvadeImplemented(hart):
            true    => SupervisorFaultManagedAd
            false   => HardwareManagedAd        ← 两种扩展都没有时是硬件更新
            Unknown => Unknown
```

**上一版这里是一条布尔式，与真值表冲突，已撤销**：它在 `Svadu=已实现 ∧ ADUE=1 ∧ Svade=已实现` 时给出软件方案，而真值表与规范原文都要求硬件更新。正文**不再保留那条式子**（逐字记录留在 02 的第六十五轮块），原因有两个：同一段里表格与公式各说一套会互相矛盾；后来者检索到已撤销的式子，还会把它当成现行规则。能直接抓住它的反例见下面的负控表。

| Svadu | 有效 `ADUE` | Svade | 实际 A/D 行为 | 页表内存的额外要求 |
|---|---|---|---|---|
| 已实现 | 1 | **任意值**（被 `ADUE` 覆盖，含已实现） | 硬件自动更新 | 硬件页表写能力 + `RsrvEventual` |
| 已实现 | 0 | 等效启用 | A/D 需置位时产生页故障 | 无（S-mode 写 PTE 仍受 PMP 写权限检查） |
| 未实现 | 只读 0 | 已实现 | 软件故障处理 | 无 |
| 未实现 | 只读 0 | **未实现** | **硬件自动更新** | 硬件页表写能力 + `RsrvEventual` |

**第四行不能漏**：两种扩展**均被证据证明未实现**时是硬件更新，不是软件故障——注意这里的条件是「证据证明二者都未实现」，**不是**「设备树里没列出 `svade`/`svadu`」；后者按 Linux 扩展绑定的说法属于行为未知，必须按两边都准备处理。判定输入因此必须是**有效状态组合**，不是能力清单——设备树里出现 `svadu` 字符串、QEMU 声称支持某扩展，都不构成方案证明。分支形式顺带把**证据要求**也钉精确：

| 输入情况 | 必需证据 |
|---|---|
| Svadu 已实现 | 必须证明有效 `ADUE`；Svade 的独立实现状态不决定当前方案 |
| Svadu 未实现 | 必须证明 Svade 是否实现 |
| Svadu 状态未知 | **不能**拿 `ADUE` 的读数猜测方案——未实现 Svadu 时该位恒为只读 0，不携带信息 |
| Svade 状态未知且 Svadu 未实现 | 方案未知 → 拒绝启动 |

**撤销上一轮的第二条自设规则**：「hart 方案一致属于项目准入、不是规范强制」不成立。规范逐字要求系统内所有 hart 采用同一 PTE 更新方案（`norm:pte_ad_scheme_uniform`）。因此**「证据缺失时默认 `SupervisorFaultManagedAd`」也不安全**：若硬件实际自动更新而内核按软件方案建表等待 A/D 故障，故障永不到来，fault handler 不承担该职责；反过来该区间还需要硬件页表写能力与 `RsrvEventual`，按软件方案就会漏检。三层分开写：

| 层 | 内容 |
|---|---|
| 规范约束 | **系统内全部 hart**（含暂不启动的次级 hart）必须采用同一 PTE 更新方案 |
| 项目准入 | 只有通过普通内存访问、架构状态、既存对象可见性与启动条件检查的 hart 才能进入 `ctx.H` |
| 项目实现责任 | 次级 hart 上线前仍需单独通过运行准入；但**全系统方案一致性不能推迟成「它以后上线再说」** |

**两个 hart 集合必须分开命名**，否则规范约束会被偷偷缩到当前参与集合上：

```
SystemHartSet                = 该平台全部 hart（含暂不启动者）
ParticipatingHartSet         = 当前参与普通内存访问的 hart 集合，即 ctx.H

SystemPteUpdateScheme              = VerifyUniformPteUpdateScheme(SystemHartSet)
MemoryAdmissionContext.participating_cpu_set_id = Identity(ParticipatingCpuSet)   ← RISC-V adapter 内成员即 hart
```

**比较的是最终有效的 `PteUpdateScheme`，不是各 hart 的扩展位图**：两个 hart 的 Svade/Svadu 实现情况可以不同，只要各自的有效行为一致（例如一个走 `Svadu ∧ ADUE=1`、另一个走「两扩展皆未实现」，两者都得出 `HardwareManagedAd`），系统级一致性就成立；反过来，两个 hart 的扩展位图相同但 `ADUE` 不同，方案就不同。`SystemHartInventoryId` 标识那份**完整 hart 清单**（含暂不启动者）及其方案证明，`ParticipatingHartSetId` 标识当前参与集合——两个身份不能共用一个字段。

职责不重叠：`SystemHartSet` 只用来验证 PTE 更新方案的**全系统**一致性；`ParticipatingHartSet` 只用来计算普通物理访问准入的**逐 hart 交集**。反例（4 hart 平台）：hart 0/1/2 参与、hart 3 暂不启动且实际方案与其它三个不同——**不能**因为 hart 3 没进 `ctx.H` 就宣称 `norm:pte_ad_scheme_uniform` 已满足。该证明由 M-mode/固件按 SystemHartSet 逐 hart 给出，缺任一 hart 的证明即 `Unknown`。
**清单从哪来、永不启用的 hart 怎么算，必须写死**：`SystemHartSet` 不能由 `ctx.H` 反推，也不能只停留成一个名字——它要有一份可核对的清单与来源（固件启动 ABI 的 hart 清单 / 设备树 `/cpus` 的 `reg` 集合 / ACPI MADT 处理器条目，取其一并记录代次）。清单缺失、与实际启动 hart 不一致或来源不可证 → `Unknown` → 拒绝启动。**「永不启用」不等于「不属于系统」**：某个 hart 若只是本项目暂不调度，它仍属规范所说的 system hart，其方案仍须被证明；只有能给出该 hart 在平台上**不存在**的物理事实（不是「我们不启动它」）时，才可以把它从清单里去掉。把它从 `ctx.H` 删除，规范义务一点也不会消失。

**清单的完整性要按 CPU 节点状态语义判定**（DTSpec 逐字）：`"okay"` = The CPU is running；`"disabled"` = The CPU is in a quiescent state，且可由 `enable-method` 重新启用（「This property is required for CPUs with a status property with a value of `disabled`」）；`"fail"` = The CPU is not operational or does not exist，并且「A CPU with `fail` status does not affect the system in any way」。由此：`disabled` **不能**简单排除——它仍可能是未来可启用的 system hart，必须进入一致性证明；`fail` 可以按「不存在的 CPU 节点」处理，但要保留排除依据；**不能因为项目决定永不启用某个 hart 就把它从 `SystemHartSet` 删除**。每条启动入口都要给出：

```
HartInventoryEvidence {
    source, source_version, hart_ids        ← 固件启动 ABI / DT /cpus / ACPI MADT 三选一，并记代次
    completeness_claim                      ← 证明的是「平台完整清单」还是「当前可启动清单」
    disabled_hart_policy, nonexistent_hart_policy
}
```

`completeness_claim` 是最容易被跳过的一格：只有「平台完整清单」才支撑 `VerifyUniformPteUpdateScheme(SystemHartSet)`；只证明「当前可启动」时，一致性证明的覆盖面必须按该清单实际范围表述，不能宣称覆盖全系统。
**运行时不得单 hart 切换方案**：只递增 `pte_update_scheme_id` 不够——某个 hart 换了 `menvcfg.ADUE` 而其余 hart 仍按旧方案运行，规范一致性当场破掉。规范的 `norm:menvcfg_adue_fence` 只保证「改 `menvcfg.ADUE` 后，`SFENCE.VMA rs1=x0 rs2=x0` 足以同步**本 hart** 的地址转换缓存」，跨 hart 需要另行远程围栏，且页表内容语义也要同步重建。**第一版把方案在初始化后冻结**；若将来允许变更，必须规定全系统 quiescence → 统一写入新值 → 各 hart 地址转换同步 → 投影与存活页表重建 的顺序，作为一次显式的政策变更。

`PteUpdateScheme = Unknown` 时第一版只有两条出路：由 M-mode/固件逐 hart 给出有效状态证明，或**拒绝启动**——除非本项目另行实现并验证一条同时兼容两种行为的路径（这个兼容性本身是待证事项，不是默认能力）。Linux 上游的 RISC-V 设备树扩展绑定也提醒 `svade` 与 `svadu` 都未声明时软件不能断定行为、应两种都准备；那是**启动信息处理指导，不是 ISA 规范**，此处只用来佐证「扩展信息缺失有风险」。

`PteUpdateEvidence` 至少含：Svade 实现状态、Svadu 实现状态、Svadu 存在时的有效 `ADUE`、hart 身份、证据提供者、`evidence_generation`、`stability_until`——**只读 `ADUE` 不够**（未实现 Svadu 时它恒为 0，不携带方案信息）。而且 `ADUE=0` 必须记录**来源**：`Svadu 已实现 ∧ 控制位读回 0` 是真实开关值，`Svadu 未实现 ⇒ 只读 0` 不是开关值，两者不能混成一个「0」。写 `menvcfg` 或方案变更必须递增 `ctx.pte_update_scheme_id` 并使依赖它的全部投影失效。
**A/D 方案负控（第一版必须实现）**：

| 输入状态 | 预期 |
|---|---|
| Svadu 已实现、`ADUE=1` | 硬件更新；页表用途要求硬件写与 `RsrvEventual` |
| **Svadu 已实现、`ADUE=1`、Svade 也已实现** | **硬件更新**——这条就是上一版布尔式的反例，必须能失败旧公式 |
| Svadu 已实现、`ADUE=0` | 软件故障路径 |
| Svadu 未实现、Svade 已实现 | 软件故障路径 |
| Svadu 未实现、Svade 未实现 | 硬件更新路径 |
| Svade/Svadu 状态无法证明 | 拒绝启动，不得默认软件故障 |
| 两个 hart 的有效方案不同 | 拒绝该平台配置，或阻止不一致 hart 上线 |

**Profile 义务与有效方案是两个独立输入**：`SvadeImplemented` 是**扩展实现状态**，`PteUpdateScheme` 是**当前有效行为**。`Svadu ∧ ADUE=1` 可以把有效行为推到硬件更新，但这不证明底层实现了 Svade 的故障行为，也不能反过来用「方案是硬件更新」去证明「Svade 未实现」。声明 profile 时另加校验：

| 声明的 Profile | 额外验证 |
|---|---|
| 不声明任何 RVA Profile | 按平台实际的 `Svade`/`Svadu`/`ADUE` 证据计算，不附加 RVA 扩展义务 |
| RVA23S64 | 必须证明 `Svade` 已实现，并满足继承的 RVA23U64 强制内存义务（`Ziccif`/`Ziccrse`/`Ziccamoa`/`Zicclsm`/`Za64rs`） |
| RVA23S64 + `Svadu` 未实现 | `Svade` 必须存在，有效行为走软件故障路径 |
| RVA23S64 + `Svadu` 已实现 | `Svade` 仍须作为已实现扩展得到证明；当前有效方案由 `ADUE` 决定 |
| RVA23S64 + `Svadu` 未实现 + `Svade` 未实现 | **Profile 不合格**：不能因为通用公式会落到硬件更新就放行 |

**Profile 义务要有实际执行位置**：实际代码用了哪些指令是**项目用途**问题，声明某 ISA Profile 后该 profile 对主存区域提出哪些强制义务是**符合性**问题——`Ziccamoa` 这类义务不能靠「内核恰好用了这些 AMO」来证明满足。补两个判定并按义务范围分别合入：

```
AdmissionContextValid(ctx) = ProfileAdmissionValid(ctx) ∧ SystemPteUpdateSchemeUniform(ctx)   ← 准入前置；任一不成立则 ctx 不可用于任何投影
ProfileAdmissionState = NotDeclared | DeclaredAndSatisfied(id) | DeclaredButFailed(id, evidence) | InsufficientEvidence(id)
    ← 第一版 isa_profile_id 未声明 ⇒ **NotDeclared**：`ProfileAdmissionValid` 不得返回「通过」；
      `ProfileMemoryEligiblePages` 取**中性语义**（不施加 profile 额外限制），**不是空集**——否则基底被交成空集；
      声明后证据不足 ≠ NotDeclared；`SystemPteUpdateSchemeUniform` 在非 RISC-V 架构须返回有类型的 `NotApplicable`
ProfileAdmissionValid(ctx)          ← 系统级：ctx.isa_profile_id 声明的强制义务集合是否整体成立
ProfileMemoryEligiblePages(ctx)     ← 页级：义务覆盖的主存区间（与 BaseAccessRequirements 求交）
```

不是所有义务都是逐页属性，所以不把它们机械塞进 `RoleAdmit`。第一版 `isa_profile_id = 未声明`，两个判定的结果都是「**未作合规声明**」——这既不阻塞第一版，也**不得**被未来调用方读成「Profile 检查已通过」。
**`Za64rs` 的预约集几何要有着落**：`Za64rs` 要求「Reservation sets are contiguous, naturally aligned, and a maximum of 64 bytes」（`rva23-profile.adoc` 97–98 行）。`AtomicAlgorithmRequirement` 现在只有操作族、宽度与进展要求，需补 `ReservationSetRequirement { max_bytes, contiguous, naturally_aligned }`；只写 `RsrvEventual` 不足以表达 profile 的 LR/SC 义务。第一版不声明 profile，该字段记「不适用」，但结构不能缺。

**共享层与架构 adapter 的边界（本轮裁定，属重构项而非 RISC-V 算法错误）**：通用内存资格模型的接口只表达 OS 需要的**语义**——普通物理访问要求、页表读取与更新要求、原子操作的宽度/原子性/进展、执行访问要求、参与访问的 CPU 集合、证据代次与有效期；具体机制名属于各架构 adapter：参与访问的 CPU 集合（RISC-V hart / AArch64 PE / x86 逻辑处理器）、页表更新要求（`Svade`/`Svadu`/`ADUE` 与 `RsrvEventual` / 硬件 `AF`+dirty / 硬件 A/D）、原子能力（`AMO(family)`/`Lr`/`Sc` 与预约集几何 / `exclusive`+`LSE` / 锁定 `lock` 前缀族）、物理访问保护（PMP+PMA / MAIR 与层级保护 / PAT+MTRR 与分页）。

三条硬规则：① `MemoryAdmissionContext` 里不得出现只在一套架构上有意义的**必填**字段——`pte_update_scheme_id` 对 x86 无意义，它是 RISC-V adapter 的可选证据槽位，不是共享接口的核心维度；② 共享层枚举不得出现 `Amo/Lr/Sc` 这类指令名，改为中性的「原子操作族 + 宽度 + 进展要求」，由 adapter 提供映射；③ `hart_set_id` 这类术语统一为 `participating_cpu_set_id`，RISC-V 章节内可以继续叫 hart。`PmpPermits`/`PmaPermits`/`RequiredPermission` 等谓词名**属于 RISC-V adapter**，AArch64 与 x86 用各自实现满足同一层语义接口。**这些类型现在全是候选设计，重命名在落码前完成成本最低**；§5 的 RISC-V 细节照旧保留，只是不再决定三架构共享接口的形状。
**整页合格是「合法起始 × 整次访问 × 每个参与 hart」的全称判定**（此前写成一个「整页是否被可允许访问集合包住」的集合包含式，把起始地址与访问宽度丢了）：

```
AccessRequirement = { privilege, path, operation, width, alignment, atomicity, rsrv_eventual_required }
    privilege ∈ {S, U}        ← 有效访问特权级；隐式页表访问按 S-mode 计
    path ∈ {Explicit, ImplicitPageWalk}
    operation ∈ {NormalRead, NormalWrite, InstructionFetch, AtomicUpdate{width, progress}, CacheMaintenance{kind}}
        ← 共享层只用 OS 语义名；RISC-V adapter 展开为 Read/Write/Fetch/AMO(family)/LR/SC，
          以及 CMO 三类 CacheMaintenance{Management, Zero, Prefetch}——第一版三类均不支持，
          遇到即 `UnsupportedAccessOperation`（权限表能表示的操作，输入模型也必须能表示）

PmpImplementationState = Absent
                       | Implemented { entry_count, grain, effective_configuration, smepmp_state }

Footprint(a, req) =
    require req.width > 0 ∧ a < 2^64
    end = u128(a) + u128(req.width)
    require end <= 2^64        ← 恰好等于 2^64 合法：a = 0xffff_ffff_ffff_fff8 的 8 字节访问覆盖最后 8 个
    return [u128(a), end)      ← 物理字节，独占端点由 u128 表示；只有 a + width > 2^64 才越界

PmpPermits(hart, req, Footprint(a, req)) =
    match PmpImplementationState(hart):
      Absent      => true                        ← 仅表示标准 PMP 单元不施加额外限制，见「Absent 不等于没有保护」
      Implemented =>
        let e = 编号最低、命中该 footprint 内任意字节的条目
        e 覆盖 footprint 的全部字节 ∧ 按 (privilege, RequiredPermission(req)) 检查 L/R/W/X
        （无条目命中时：M 成功；S/U 在「至少实现一条」时失败）

PmaPermits(hart, req, Footprint(a, req)) = 该区间 PMA 允许这一类访问
    （访问宽度、取指、页表读/写、AMO/LR-SC 族与原子性粒度）
    ∧ req.rsrv_eventual_required ⇒ 该区间具备 `RsrvEventual`

PhysicalAccessPermits(hart, req, [a,b)) =
    PmpPermits(hart, req, [a,b)) ∧ PmaPermits(hart, req, [a,b)) ∧ PlatformProtectionPermits(hart, req, [a,b))

BaseAccessRequirements(ctx) = 任何普通数据页都必须满足的访问类别
    实际访问代码用到的 (Load/Store, 宽度) 集合（字节/半字/字/双字等，由 ISA + 编译目标 + 实际代码确定）
    完整性由 `GeneralDataCompleteness(ctx)` 保证；证不出完整即 `InsufficientEvidence`

RoleSpecificRequirements(role, ctx) = 该用途**在基底之上**的额外访问类别
    GeneralData        → ∅                                   ← 空集是定义，不是遗漏：基底已覆盖
    PageTable          → { (ImplicitPageWalk, Read,  PteSize, PteAlignment) }   ← 两种 A/D 方案都必需
                         ∪ { (Explicit, Read,  PteSize, PteAlignment) 若内核直接读 PTE }
                         ∪ { (Explicit, Write, PteSize, PteAlignment) }  ← 安装/修改/撤销 PTE：两种方案都必需
                         ∪ { ImplicitHardwarePteUpdate 若 HardwareManagedAd }
    Executable         → `InstructionFetchPattern` 展开的取指访问类别（见下，不能退化成单个宽度）
    AtomicSharedState  → 由 `RegisteredAtomicAlgorithms(ctx)` 展开的 `AtomicUpdate{width, progress}` 集合
                         （RISC-V adapter 再展开为 AMO(family)/LR/SC）+ 预约集几何要求

ImplicitHardwarePteUpdate = AccessRequirement { privilege=S, path=ImplicitPageWalk, operation=Write,
    width=PteSize(active_translation_mode), alignment=PteAlignment,
    atomicity=AtomicPteUpdate, rsrv_eventual_required=true }   ← 相对同一 PTE 的其它访问必须原子

BasePageEligible(page, ctx) =
    AdmissionContextValid(ctx) ∧ ctx.ParticipatingCpuSet ≠ ∅     ← 系统级前置；空参与集合会让 ∀ hart 真空通过
    ∧ GeneralDataCompleteness(ctx) = Complete ∧ BaseAccessRequirements(ctx) ≠ ∅   ← 非空还不够：漏掉 8 字节 Store 同样会通过
    ∧ ∀ req ∈ BaseAccessRequirements(ctx) :
        LegalStarts(page, req, GeneralData, ctx) ≠ ∅                     ← 页内无合法起始 ≠ 合格
        ∧ ∀ a ∈ LegalStarts(page, req, GeneralData, ctx)
          ∀ hart ∈ ctx.H : PhysicalAccessPermits(hart, req, Footprint(a, req))

RolePageEligible(page, role, ctx) =
    RoleRequirementsComplete(role, ctx)          ← 需求生成器没有因缺 profile / 缺代码扫描而「漏出」空集
    ∧ ( role = GeneralData ⇒ RoleSpecificRequirements(role, ctx) = ∅ )      ← 只有这一格按定义为空
    ∧ ( role ≠ GeneralData ⇒ RoleRequirementsApplicable(role, ctx)
                             ∧ RoleSpecificRequirements(role, ctx) ≠ ∅ )    ← 否则 ∀ req ∈ ∅ 仍为真
    ∧ ∀ req ∈ RoleSpecificRequirements(role, ctx) :
        LegalStarts(page, req, role, ctx) ≠ ∅
        ∧ ∀ a ∈ LegalStarts(page, req, role, ctx)
          ∀ hart ∈ ctx.H : PhysicalAccessPermits(hart, req, Footprint(a, req))

CommonCpuAccessEligiblePages(ctx) = { page | BasePageEligible(page, ctx) }   ← 仍要与其它基底交集条件求交
PmmRoleEligible(page, U, ctx)     = page ∈ PmmBaseAvailable(ctx) ∧ U ≠ ∅
                                    ∧ ∀ role ∈ U : RolePageEligible(page, role, ctx)
```

**`PteSize` / `PteAlignment` 绑定当前翻译模式**：显式读写同样带宽度，不留「无宽度的 Read/Write」。**软件故障方案只改变 A/D 位由谁更新，不取消硬件页表遍历本身**——地址转换算法仍会隐式读取 PTE，所以 `(ImplicitPageWalk, Read)` 与 `(Explicit, Write)` 都在「始终必需」那一列；硬件更新方案才在其上追加 `ImplicitHardwarePteUpdate`。

**名字与定义必须一一对应，不能一处加前置条件、另一处不加**：`RoleAdmit(role, page, ctx)` 是 O-12i 两层代数里的**声明式接口**，它的操作化定义就是 `RolePageEligible(page, role, ctx)`——同一个谓词，两个名字，不存在第二套展开式：

```
RoleAdmit(role, page, ctx)     ⇔ RolePageEligible(page, role, ctx)
PmmRoleEligible(page, U, ctx)  = page ∈ PmmBaseAvailable(ctx) ∧ U ≠ ∅ ∧ ∀ role ∈ U : RoleAdmit(role, page, ctx)
```

**基底与用途特有是两层，不是同一层的两个名字**：`CommonCpuAccessEligiblePages(ctx)` 只消费 `BaseAccessRequirements(ctx)`（普通 Load/Store 的宽度清单，含完整性证据），`RoleAdmit` 只消费 `RoleSpecificRequirements(role, ctx)`（用途在基底之上的额外类别）。`GeneralData` 的用途特有集合因此**定义为空集**——它不再同时扮演「基底要求」和「空的额外要求」两个角色，这正是上一版三句话互相矛盾的根源。`PageTable` 这一格的用途特有要求即 `PageTableBackingEligible` 的访问部分。**改判定式只允许改操作化那一侧**，声明式接口名保持稳定；把 `RoleAdmit` 存成一个布尔字段会丢掉 `req`/起始/hart 三个量词维度。

**只有 `GeneralData` 的用途特有集合可以按定义为空**：`PageTable` 因隐式遍历读与显式 PTE 写始终必需而非空，`Executable` 必须有完整取指模式（`instruction_lengths` 未登记即不完整），`AtomicSharedState` 必须有完整算法清单。任何其它用途出现空集，都是**证据不足**而不是「没有额外要求」——结论必须是 `InsufficientEvidence`，不能靠全称量词在空集上自动成立。形式化前置条件是 `RoleRequirementsComplete(role, ctx)`（清单完整）与 `RoleRequirementsApplicable(role, ctx)`（该用途在当前配置下适用），二者与 `≠ ∅` 并列写进 `RolePageEligible`。

**空集不得真空通过**：`BasePageEligible` 已把 `ctx.ParticipatingCpuSet ≠ ∅`、`BaseAccessRequirements(ctx) ≠ ∅` 与每条基底 `req` 的 `LegalStarts ≠ ∅` 写成前置条件，`PmmRoleEligible` 另要求 `U ≠ ∅`——否则「漏登记」会被全称量词读成「全部合格」。`GeneralData` 的宽度清单必须由**固定编译目标 + 编译器代码生成约束 + 已审计指令集合**三者合成 `GeneralDataCompleteness(ctx)`；证不出完整时结论是 `InsufficientEvidence`，不是「已通过」。`Executable` 的 `instruction_lengths` 为空集同理：那等于宣称该页没有取指需求。

**全称量词不能替换成「字节被覆盖」**：一页每个字节都被某次成功访问覆盖，并不代表从该页任一合法起始地址发起的最宽访问都能成功——编号最低的条目只需命中一个字节就要覆盖整次访问，跨边界的操作整体失败（规范那个覆盖 `0xC–0xF` 的条目遇到 `0x8–0xF` 的 8 字节访问的例子）。`AdmittedFootprints` / `AllowedOperationStarts` 这类「先把可允许访问收集成集合、再比包含」的写法可以当等价推导，但**判定式本身必须写成对起始地址全称量化**，否则实现里很容易退化成字节并集。PMP 粒度最细 4 字节（`norm:pmp_granularity`、`norm:pmp_a_field_napot_na4`「These modes support four-byte granularity」），页内边界是真实存在的，这正是必须量到起始地址的原因。**判定式的索引必须是 `AccessRequirement` 而不是页**：同一 hart 同一条 PMP 配置下，「普通读写可用但隐式页表读不可用」「页表可读但 A/D 隐式写不可用」「读写可用但取指不可用」「读写可用但某 AMO 宽度不可用」四类差异真实存在，字节集合会把它们抹平。**一个页不满足 `PageTable` 用途，不代表它不能作为 `GeneralData`**——把页表条件塞回基底会把用途层架空。

**`LegalStarts` 有两个互斥合同，实现者不得自行挑选**：

| 合同 | `LegalStarts(page, req, role, ctx)` | 成立前提 | 代价 |
|---|---|---|---|
| **A 页级保守** | 该页内**同时满足「该访问类别的架构起始对齐」与「该用途规定的完整 footprint 约束」**的起始地址：Load/Store 取自然对齐且宽度 ≤ 页大小；LR/SC/AMO 取自然对齐并满足预约集几何；Fetch 取满足指令起始对齐且**最大指令 footprint 不越出本页** | 用途资格独立于对象登记：页一旦合格，任何合法起始都能用 | 跨页对象无法只靠页级资格表达；PMP 边界附近判得更严；Fetch 的页尾若干字节不可作起点 |
| **B 对象级精确** | 该页上**已登记访问点**在该用途下实际使用的起始地址 | O-07 的访问点清单完整性已被证明，否则是循环证明 | 未登记的访问不构成合格证据；新增使用点须重新判定并重建投影 |

第一版选 **A**：B 依赖的完整性主张今天没有任何证据（见 O-07 第三项未证明）。选 A 之后**不得**再用「只检查登记起始」放宽任何结论；若将来改选 B，合同切换本身要显式记录、全部投影重建，并把「存在未登记访问」报告为独立失败形态。合同标识进入 `ctx`（`legal_starts_contract`），两种合同不得共用同一个 `evidence_generation`。
**跨页 footprint 不由页级资格覆盖——两个谓词各管一件事**：

```
PageRoleEligibility(page, role, ctx) = RolePageEligible(page, role, ctx)
    ← 分配资格：这一页将来能否承担该用途（面向 PMM 分配入口）

ObjectAccessUse {
    access_use_id, object_id,    ← 使用点与对象身份：政策授权按这两个定位
    va_range,                    ← 该使用点实际访问的 VA 区间
    expected_translation,        ← ExpectedTranslationSegments：分段登记，不得由当前 PTE 反推
    access_requirement,          ← AccessRequirement（取指见下：每个取指实例自带宽度）
    tree, phase, path, required_cpus, source_refs   ← required_cpus ⊆ ctx.ParticipatingCpuSet；
    source_refs 给出该对象的来源记录身份，政策授权层按它判定
}
ExpectedTranslationSegments = { TranslationSegment { va_range, expected_pa_range } }
    ← 连续 VA 区间不保证映射到连续 PA：跨虚拟页的对象/取指会落到多段物理范围，
      单一 expected_pa_range 表示不了这种映射

ActualAccesses(use) = { ActualAccessInstance { requirement, physical_footprints } }
    ← 普通 Load/Store 与 PTE 读写按 TranslationSegment 生成；Fetch 按**每个指令实例**生成，
      实例自带该指令的长度/宽度/片段（16/32/… 与跨页指令不同，固定宽度代表不了全部）
    ← 每个实例必须满足 AccessAlignmentSatisfied：起始地址满足该操作类别的架构对齐
      （Load/Store 自然对齐、Fetch 为 IALIGN、LR/SC/AMO 自然对齐），否则该使用点不合格
    ← 展开成**全部**物理片段：跨页访问必须覆盖涉及的所有片段，不是只看起始页

ObjectPhysicalAccessEligible(use, ctx) =
    RequiredCpus(use) ≠ ∅        ← 空集合会让 ∀ cpu 真空通过；作用域按 RequiredCpuScope，无 CPU 直接访问者须显式 NotApplicable
    ∧ ∀ instance ∈ ActualAccesses(use) : ∀ footprint ∈ instance.physical_footprints
      ∀ cpu ∈ RequiredCpus(use) : PhysicalAccessPermits(cpu, instance.requirement, footprint)

ObjectPolicyAccessEligible(use, ctx) =
    AccessAuthorizationCovers(use.object_id, use.source_refs, use.tree, use.phase, use.path, use.expected_translation)
    ∧ SourceSpecificDenialsSatisfied(use, ctx)
    ← 来源政策授权层：MRB 的具名一次性授权、no-map 的非受控访问限制、
      EfiReservedMemoryType 的 DenyOsUse 各按自己的来源规则解除，
      不得由一个通用 AccessAuthorization 布尔覆盖所有负向限制

ObjectMappedAccessEligible(use, ctx) =
    ObjectPolicyAccessEligible(use, ctx)
    ∧ Resolve(use.va_range, use.tree, use.phase) = ExpectedTranslationSegments(use)
         ← 逐段验证：整区间落到多段 PA 时，每一段都要对应，不是起点相等
    ∧ MappingPermissionsAllow(use) ∧ EffectiveMappingAttributeCompatible(use)
    ∧ AliasCompatibilityHolds(use) ∧ ObjectPhysicalAccessEligible(use, ctx)

ObjectAccessEligibility(object, ctx) =
    ObjectAccessInventoryState(object) ∈ { Complete, NoAccess }      ← 空清单不等于「没有访问需求」
    ∧ ∀ use ∈ ObjectAccessUses(object) : ObjectMappedAccessEligible(use, ctx)
ObjectAccessInventoryState(object) = Complete(nonempty uses) | NoAccess(proof_or_reason) | Incomplete
RequiredCpuScope = ParticipatingCpuSet(nonempty) | NotApplicable(reason)   ← 后者只表示无 CPU 直接访问者，其他合同仍独立判定
```

**三层必须分开，不能只留一层**：① 来源政策授权（这个对象有没有权访问这段来源范围）② 硬件物理准入（CPU 能不能以该操作访问这些字节）③ 映射路径兑现（这条 VA 路径是否确实逐段映射到预期 PA，权限/属性/别名是否兼容）。只查 ② 会让「PMP/PMA 都允许的 MRB 区域」「硬件仍允许 S-mode 读取的 `EfiReservedMemoryType`」「带 `no-map` 的非受控访问限制」全部绕过 §3 的不变量。

**物理可访问不等于该 VA 到达该 PA**：`ObjectPhysicalAccessEligible` 只回答「hart 能不能碰这段物理字节」，不回答「内核用的那个 VA 是否解析到这段字节、权限与属性是否满足合同、是否存在别名」。`Resolve` 必须是**区间级**判定：起点相等只证明一个点，而一次访问的中间页可能切换到别名或属性不兼容的映射——区间逐字节对应才排除这种情形。这与设备侧要求 `VirtualByteRange` 与 `PhysicalByteRange` 并存是同一条规则。`use.pa_start` **必须来自独立的对象访问输入**，不能先查当前 PTE 得出 PA 再把它当成预期 PA——那是循环证明。

`ExistingPageTableObjectEligible(object, ctx)` 因此是五项合取，缺一项都不成立：① 对象身份与存活登记；② `PageTableBackingEligible`（隐式遍历、PTE 更新与 A/D 方案）；③ 实际使用的 VA → PA 路径（`Resolve(va_range, tree, phase) = expected_pa_range`，区间级）；④ 显式 PTE 读/写与隐式遍历两类访问都登记；⑤ 权限、属性与别名审计。

一个对象可以跨过两页，其中一页不作为该用途的分配候选，但对象自己的既有访问点仍可能全部合法；反过来，整页合格也证明不了某个跨越页尾的对象访问合法。页级谓词与对象级谓词不得互相冒充：`PmmRoleEligible` 回答「能不能把这一页交给该用途」，`ObjectAccessEligibility` 回答「这个对象现在经这些 VA 是否真的可用」。
**第一版只准入「按架构对齐的整次访问」——三类对齐必须分开**：`norm:pmp_misaligned_access_behavior`「On some implementations, misaligned loads, stores, and instruction fetches may be decomposed into multiple memory operations, some of which may succeed before an access-fault exception occurs, as described in the RVWMO specification. PMP checking is performed on each memory operation independently.」以及紧随其后的「In particular, a portion of a misaligned store that passes the PMP check may become visible, even if another portion fails the PMP check.」——**半写可见**是真实失败形态：一次非对齐 store 可以把落在合格页内的那一半写进去，再在越界那一半上触发故障，页级资格完全拦不住。因此 `alignment` **不能**统一理解成「等于宽度」：

| 访问类别 | 对齐来源 | 会不会跨页 |
|---|---|---|
| `Load` / `Store` | 该宽度的自然对齐（第一版只准入这一类） | 自然对齐且宽度 ≤ 页大小时不跨页 |
| `Lr` / `Sc` / `Amo` | 该宽度的自然对齐 | 同上，但另受预约集几何约束（见原子用途） |
| `Fetch` | **架构的指令起始对齐**，不是指令宽度（RISC-V 为 IALIGN：基座 32 位、含 `C` 时放宽到 16 位） | **会跨页**：32 位指令可以从 2 字节对齐处开始 |

**取指不能套用「宽度 = 对齐」**：RISC-V 的逐字依据是 `src/unpriv/intro.adoc` 的「IALIGN is 32 in the base ISA, but some ISA extensions, including the compressed ISA extension, relax IALIGN to 16.」（该文件 452–460 行），以及「the instruction-fetch unit on machines with ILEN >32 report an illegal-instruction exception rather than an access-fault exception when such an instruction borders a protection boundary, complicating variable-instruction-length fetch and decode」（同文件 528–531 行，位于 NOTE 内，不是编号规范条目）——**规范明确承认一条指令可以 border 一个保护边界**。于是页尾 `page_end − 2` 处的 32 位指令 footprint 是 `[page_end−2, page_end+2)`，跨两页。x86 的可变长指令（最长 15 字节）同理，该条按架构事实使用，本方未取 Intel SDM 原文，登记为待核。

```
InstructionFetchPattern {
    instruction_lengths,                   ← 该 profile 实际可能出现的指令长度（RISC-V: 16/32/48/64…）
    architectural_start_alignment,         ← IALIGN（RISC-V 16 或 32；x86 为 1 字节）
    access_fragments(a, L), page_boundary_behavior   ← 实际触碰的字节片段，与跨页时的行为
}
FetchFootprints(a, pattern) = { access_fragments(a, L) | L ∈ instruction_lengths }
```

由此两条规则，**必须择一登记，不得含糊**：

1. **页级资格（`Executable`）用保守片段集**：`LegalStarts` 只保留「最大长度 footprint 仍落在本页内」的起始地址。页尾若干字节因此不能作为指令起点——这是**容量损失，不是安全漏洞**，且让页级资格保持为单页性质。
2. **对象级资格（真实代码）用 `FetchFootprints`**：实际存在的跨页指令必须让 `ObjectAccessEligibility` 同时证明**起始页与下一页**的取指访问，不能因为「起始页合格」就放行。
**这条判定是保守的，代价要写明**：`PmpPermits` 要求**一条**条目覆盖整次 footprint（`norm:pmp_full_match_required`）。若硬件把跨页取指拆成多次操作、逐次检查，那么落在两条不同条目里的两段各自合法也会成功——本判定式会拒绝这种配置。第一版选 fail-closed，实现复审时要用真实条目布局确认这种拒绝不会把支持的平台判死。

**非对齐 Load/Store**：要允许，必须先把它变成显式软件路径（陷阱模拟并保证不产生半写），或对该 footprint 涉及的**每一页**分别判定，二者都属新增合同，不能靠「整页合格」默认放行。`Zicclsm` 不是无条件事实（`norm:Zicclsm_misaligned_support` 只说「若实现该扩展，则一致且可缓存主存区间的非对齐 load/store 必须支持」）；平台未声明该扩展时，非对齐访问按不支持处理，不得假设硬件会拆开完成。
**准入向内收缩、排除向外量化，两者不对称**：向外量化只出现在**排除**侧（保留、存活对象、`no-map`、`PlatformAllocationDenials`），否则会出现「一页有一半被拒、整页却交出去」。

**PMP 不是逐字节权限的交集，而是「整次访问」的判定**——规范逐字规定：「PMP entries are statically prioritized. The lowest-numbered PMP entry that matches any byte of a memory operation determines whether that operation succeeds or fails.」（`norm:pmp_entry_priority`）以及「The matching PMP entry must match all bytes of a memory operation, or the operation fails, irrespective of the L, R, W, and X bits.」（`norm:pmp_full_match_required`）；规范自带的反例是：一条覆盖 `0xC–0xF` 的条目遇到 `0x8–0xF` 的 8 字节访问会**失败**，哪怕逐字节看只有一半越界。所以判定输入必须是**整次访问**（基址 + 宽度 + 对齐 + 特权级 + 路径）。

**`RequiredPermission(req)`：操作 → 许可位的显式映射**（**本表是 RISC-V adapter 的展开**；共享层 `operation` 用语义名，见上）——逐行钉住规范原文并给锚点。此前只写「按 `(privilege, operation)` 检查 L/R/W/X」，没有说每种操作查哪一位：

| 操作 | 查的位 | 有效特权级 | 依据 |
|---|---|---|---|
| `Load` / `Lr` | **R** | S 或 U | `norm:pmp_load_fault`：「Attempting to execute a load or load-reserved instruction that accesses a physical address within a PMP region without read permissions raises a load access-fault exception.」 |
| `Store` / `Sc` / `Amo(·)` / cache-block zero | **W** | S 或 U | `norm:pmp_store_fault`：「Attempting to execute a store, store-conditional, AMO, or cache-block zero instruction that accesses a physical address within a PMP region without write permissions raises a store access-fault exception.」 |
| `Fetch` | **X** | S 或 U | `norm:pmp_exec_fault`：无执行权限的取指触发 instruction access-fault |
| 隐式页表**读** | **R** | **S**（固定） | `norm:pmp_check_pagetable_access`：「PMP checks are also applied to page-table accesses for virtual-address translation, for which the effective privilege mode is S.」＋ `norm:pmp_cfg_permissions` |
| 硬件 A/D **隐式写** | **W** | **S**（固定） | 遍历算法把这一步直接写成对 PTE 的 **store**：「If a store to the PTE at address __a__+__va.vpn__[__i__]×PTESIZE would violate a PMA or PMP check, raise an access-fault exception corresponding to the original access type.」（`src/priv/supervisor.adoc` 地址翻译算法第 5 步的分支，该句**无独立 norm 锚点**，本方本轮直取原文）＋ `norm:pmp_store_fault` 的 store→W。**强度差别要写清**：条款存在，但它是算法步骤里的一句话而非编号规范条目，且同一步还要求「Compare…set a/d…store」原子完成——这正是硬件方案另要 `RsrvEventual` 与硬件页表写权限的原因 |
| 软件更新 PTE | 显式 `Write` 的 **W** | S | 同一张表，只是 `path = Explicit` |

三条配套（都是「一句话表达不出来」的地方）：

- **权限按写侧检查 ≠ 原子能力由 PMP 决定**。`norm:amo_op` 描述 AMO 为「atomically load a data value … then store the result back」，一次操作同时含 load 与 store，但规范的故障归类把 AMO 划到 store 侧；同时 `norm:pmp_rwx_warl` 规定 R/W/X 里「R=0 且 W=1」是**保留编码**，因此一条可写的合法条目必然也可读。**AMO 的族（`AMONone`/`AMOSwap`/`AMOLogical`/`AMOArithmetic`，`norm:pma_amo_levels`）与支持的宽度、以及 `Ziccamoa`/`Ziccamoc` 这类主存义务全部由 `PmaPermits` 承担**，拿 PMP 位代替会造出「PMP 可写就以为支持该 AMO」的错误准入。
- **`privilege` 是有效特权级，且合同只覆盖 `{S, U}`**。`norm:pmp_check_priv_modes`：「PMP checks are applied to all accesses whose effective privilege mode is S or U, including instruction fetches and data accesses in S and U mode, and data accesses in M-mode when the MPRV bit in `mstatus` is set and the MPP field in `mstatus` contains S or U.」由此三条结论：① `MPRV=1 ∧ MPP∈{S,U}` 的 M-mode 数据访问落在本合同内，但内核第一版不在 M-mode 用 MPRV 访问普通主存，所以这一格记为「合同覆盖、当前无消费者」；② `norm:pmp_l_bit_m_mode_enforcement` 说 L 清零时「any M-mode access matching the PMP entry will succeed」，**M-mode 能访问不构成 S/U 准入的证据**；③ 位判定本身按特权级分支：`norm:pmp_rwx_check`（L 置位或特权级为 S/U 时，只有对应访问类型的 R/W/X 置位才成功）与 `norm:pmp_no_entry_match`（无条目命中时 M 成功；S/U 在至少实现一条时失败）。
- **PMP 与 PMA 并行且都要通过**：PMP 管有效权限，PMA 管访问类型、宽度、取指、页表读写与原子能力。缺任一项，公式就表达不出「PMA 允许读写但不允许页表访问 → 数据页可用、页表页拒绝」这条已列入的负控。

**cache-block 三类操作的许可规则与普通读写不同**（`src/unpriv/cmo.adoc` 原文；prefetch 类 `norm:cbp_access` 在许可时按 load/store/fetch 任一，不许可时**不产生异常也不访问内存**，与另两类不同）：`norm:cbm_access` 说 cache-block management「is permitted to access the specified cache block whenever a load instruction **or store instruction** is permitted to access the corresponding physical addresses」——是「读**或**写许可」，两者都不允许而仅取指允许时行为 UNSPECIFIED；它**不是**「按读权限检查」。`norm:cbz_access` 说 cache-block zero 要求 **store** 许可**且** PMA 表明支持该访问类型。另有块级一致性约束 `norm:pmp_same` / `norm:pma_same`：整个 cache block 内 PMP 位与 PMA 必须一致，且「if write permission is granted by the PMP access control bits, read permission shall also be granted」，不满足时 `norm:cbo_rsv` 规定行为 UNSPECIFIED。因此将来启用这两类操作时，准入必须按**整个 cache block** 判并检查块内一致性，只按单次访问 footprint 判会漏掉 UNSPECIFIED 分支。

**Smepmp 的支持边界（第一版裁定）**：`mseccfg.MML`（Machine Mode Lockdown）「changes the interpretation of the `pmpcfg.L` bit」——置位后 `L` 不再表示「锁定且在所有模式强制」，而是 `L=1` → *M-mode-only*、`L=0` → *S/U-mode-only*，并且原本保留的 `pmpcfg.RW=01` 与 `pmpcfg.LRWX=1111` 变成 *Shared-Region* 编码（`norm:mseccfg_mml_pmpcfg_l_op`、`norm:mseccfg_mml_m_rule_op`、`norm:mseccfg_mml_soru_rule_op`、`norm:mseccfg_mml_shared_l0_op`、`norm:mseccfg_mml_shared_l1_op`、`norm:mseccfg_mml_shared_lrwx_1111_op`）；`MMWP` 把「无匹配条目的 M-mode 访问」的默认从 ignored 改成 denied（`norm:mseccfg_mmwp_op_warl`）；`RLB` 是临时绕过锁定的开关（`norm:mseccfg_rlb_op_warl`）；三者复位为 0「unless the platform mandates a different reset value」（`norm:mseccfg_mml_mmwp_rlb_rst`），`MML`/`MMWP` 还是 sticky 位。**同一张 R/W/X 表在 `MML=1` 下整张是错的**——查表的位含义已经变了。第一版因此只接受 `smepmp_state = {MML=0, MMWP=0}`（`RLB` 只登记不参与判定）；`MML=1`、`MMWP=1`，或 `mseccfg` 读不到/无法证明 → `SmepmpUnsupportedConfiguration`，在平台准入阶段拒绝，不做「按标准语义近似」。将来若要支持，得把 Shared-Region 的三种编码、`norm:mseccfg_mml_x_restrict`（不能新增可执行的 *M-mode-only* 或锁定的 *Shared-Region* 规则）与 `norm:pmp_rw01_rsv` 一并写进 `PmpPermits`，那是另一份合同。`mseccfg` 同 `pmpcfg*` 一样只能 M-mode 访问，所以这一格也归 `PmpEvidence.provider`，不能靠探测。

**`Absent` 不等于「没有物理内存保护」**：规范允许零条 PMP（`norm:pmp_entry_count`「Implementations may implement zero, 16, or 64 PMP entries」），但同一节的 NOTE 逐字写着「Platforms vary widely in demands for physical memory protection, and some platforms may provide other PMP structures in addition to or instead of the scheme described in this section.」因此 `PmpImplementationState = Absent` 只表示**标准 PMP 单元不施加额外限制**，它不证明平台没有别的保护结构。第一版两条出路，必须择一登记进 `PmpEvidence`：① 平台 profile 给出带实现名与版本的支持限制声明——「本 profile 下该区间除标准 PMP 外无其它物理访问限制」；② 拿不出该声明 → 证据不足 → fail-closed，不得把 `Absent` 当成整段通过。把 `Absent` 直接写成 `PmpPermits => true` 而不附支持限制，等于把「没观察到保护」写成「没有保护」。

**完整平台保护清单是独立前提，`Implemented` 同样需要**：规范允许平台在标准 PMP 之外另有保护结构，所以「标准 PMP 证据齐全」只覆盖它自己那部分。补：

```
PhysicalProtectionInventory {
    standard_pmp, additional_protection_structures,   ← 后者是平台声明的其它物理保护（若有）
    completeness,      ← 「无未建模结构」还是「仅覆盖已声明结构」
    scope,             ← 每个额外结构须绑定：适用 CPU 集合、物理范围、访问类别与权限、生效配置版本、稳定性承诺
}
PhysicalAccessPermits = PmpPermits ∧ PmaPermits ∧ PlatformProtectionPermits
```

第一版对每个 `PlatformMemoryProfile` 二选一：证明不存在未建模的额外保护结构（此时 `PlatformProtectionPermits` 由该证明通过），或把已声明结构纳入 `PlatformProtectionPermits` 逐区间判定。`completeness` 只能由 profile 提供，**不能默认为真**。
**证据稳定性：`RLB` 进稳定性证明，不是只登记**。`norm:mseccfg_rlb_op_warl` 说 `RLB` 提供「a mechanism to temporarily modify *Locked* PMP rules」，即 `RLB=1` 时 `L=1` 的条目仍可被改。于是「`RLB` 只登记不参与判定」只在**位解释**层面成立，不能当作证据有效期依据。完整合同四条：① 若依赖 `L=1` 条目保证稳定，必须证明 `RLB` 不允许绕过锁定（`RLB=0` 且无法再置位）；② 只要求 `RLB=0` 也不够——没有 `L` 的条目仍可由 M-mode 修改；③ 真正要证明的是**整份有效配置**在 `stability_until` 内不变——而且**事后通知不够**：provider 改完再通知，旧投影可能已被分配器或访问路径使用，事后递增代次无法撤销已经发生的非法访问。第一版只接受两种合同之一：
   **A 不可变配置合同**（第一版选它）：provider 承诺从提供证据到生命周期结束，不修改相关 PMP、Smepmp 与额外保护配置；
   **B 协调式变更合同**：变更**前**先通知内核，内核停止相关访问、完成受影响对象检查与必要的迁移或隔离，再允许配置变化；变化完成后递增代次、重建投影、重新开放访问。
   `stability_until` 因此不是单纯的时间期限，而是 provider 对「配置不可变」或「变更受协调」的承诺；没有该承诺的证据不得用于任何跨阶段分配或访问授权；④ 任何改变生效配置的事件（M-mode 改写 `pmpcfg*`/`pmpaddr*`、写 `mseccfg`、hart 上线使该 hart 的配置进入参与集合）都必须递增 `evidence_generation` 并使全部依赖投影失效。拿不到承诺与通知的 provider，`stability_until` 只能记「未声明」，其证据不得跨阶段缓存。
**待补测试（按用途区分，不做「全局原子/全局可访问」的粗暴结论）**：MAIR 配置正确但叶子选错 `AttrIndx` 必须失败；PAT 项正确但 MTRR 使有效类型非 WB 必须失败；任一参与 hart 的有效 PMP 拒绝该区间的**普通读写** → 不得进入 `PmmBaseAvailable`；仅拒绝**隐式页表访问** → 基底保留、`PmmRoleRejected(PageTable)` 命中；PMA 允许读写但不允许页表访问 → 数据页可用、页表页拒绝；PMA 无 AMO 但普通读写可用 → 按用途处理；一致主内存标为实现自定义排序 → 不得进入基底；A/D 方案的六行负控见上表（尤其「Svade/Svadu 均不可证 → 拒绝启动」与「两 hart 方案不同 → 拒绝该平台配置」）；参与 hart 集合新增一个 PMP 更严的 hart → 旧 `ctx` 的全部投影失效并重建；LR/SC 算法依赖最终成功而区域仅 `RsrvNonEventual` 且无 fallback → 该用途不合格，同页 `GeneralData` 仍合格；一页只有一半落在准入域内 → 该用途不合格；`ranges` 缺失与非空分别落到各自错误；`FinalMap` 扩容后旧快照与投影全部失效；**起始地址负控**——同一页内存在一个起始地址使 footprint 越过 PMP 条目边界 → 该页在此宽度下不合格，即使逐字节看全部被覆盖过；**非对齐负控**——合格页尾部发起的半字/字访问越过条目边界 → 必须拒绝或走显式软件路径，不得按整页合格放行；**合同负控**——同一 `evidence_generation` 下 `legal_starts_contract` 从 A 切到 B → 全部投影失效；**Smepmp 负控**——`mseccfg.MML=1` 或 `MMWP=1` 或 `mseccfg` 不可读 → `SmepmpUnsupportedConfiguration`，不得套用 R/W/X 表；`Absent` 且平台拿不出「无其它保护结构」支持限制 → fail-closed；**Footprint 顶端负控**——`a = 0xffff_ffff_ffff_fff8` 且宽度 8 必须合格（`end == 2^64`），`a = 0xffff_ffff_ffff_fff9` 必须越界；**取指跨页负控**——页尾 `page_end − 2` 处的 32 位取指：页级资格按保守片段集排除该起始，对象级按 `FetchFootprints` 同时判两页；**空集负控**——`BaseAccessRequirements` 为空、`U = ∅`、或某条 `req` 的 `LegalStarts` 为空 → `InsufficientEvidence`，不得真空通过；**稳定性负控**——`RLB=1` 或 provider 没有变更通知 → `stability_until` 不得用于跨阶段缓存，配置变更未递增 `evidence_generation` 必须失败；**保护清单负控**——`Implemented` 但 profile 既未证明「无未建模结构」也未列出其它结构 → `PlatformProtectionPermits` 不得默认为真；**profile 负控**——声明 RVA23S64 而 `Svade` 未实现 → 不合格，即使通用公式落到硬件更新；**基底真空负控**——`BaseAccessRequirements` 为空、`PmmRoleEligible` 的 `U = ∅`、或任一基底 `req` 的 `LegalStarts` 为空 → `InsufficientEvidence`，不得通过；**用途特有集合负控**——`RoleSpecificRequirements(GeneralData) = ∅` 必须**不**触发失败（空集是定义），但把它当成「基底要求」的写法必须失败；**硬件 PTE 更新负控**——`HardwareManagedAd` 下缺 `ImplicitHardwarePteUpdate` 的任一项（隐式 W、硬件页表写、原子更新、`RsrvEventual`）→ 页表用途不合格；**对象区间负控**——VA 起点正确但区间中段落到别名或属性不兼容映射 → `ObjectMappedAccessEligible` 失败；**跨页取指对象负控**——32 位指令起于页尾 2 字节、下一页取指不合格 → 失败；**稳定性负控**——provider 只承诺「变更后通知」→ 不接受，须给不可变或变更前协调承诺；**预约集几何负控**——支持 LR/SC 且 `RsrvEventual`，但预约集几何不满足声明 profile → 原子用途不合格；**cache-block 负控**——`CacheBlockManagement`/`CacheBlockZero` 在第一版一律 `UnsupportedAccessOperation`；**本轮新增负控**：硬件页表读未登记 → `PageTable` 用途不合格（即使显式写通过）；`RoleRequirementsComplete` 为假或 `Executable` 的 `instruction_lengths` 未登记 → `InsufficientEvidence`；`ctx.ParticipatingCpuSet = ∅` → 基底判定不得通过；对象物理准入通过但来源政策未授权（MRB / `no-map` / `EfiReservedMemoryType`）→ `ObjectPolicyAccessEligible` 失败；连续 VA 映射到两段不连续 PA 而 `expected_translation` 只登记一段 → 失败；叶子落在正向授权内但与某条 `DenyMapping` 相交 → `MappingAudit` 失败；LR/SC 算法在第一版遇到 `RsrvNonEventual` 区域 → 一律拒绝（fallback 分支未开放）。

**PMP 证据必须有提供者，不能靠探测**：标准 `pmpcfg*`/`pmpaddr*` 只能由 M-mode 读写；S-mode 内核既读不到，也不能把一次探测性 load 当成整段证明（它不覆盖写、取指、隐式页表访问，也不覆盖 PMP 边界）。因此需要类似 `PmpEvidence { provider, hart_id, privilege_profile, physical_ranges, access_permissions, evidence_version, stability_until }` 的合同：由 M-mode 固件经启动 ABI 提供，或由平台 profile 对固件版本/PMP 配置/hart 启动方式作出可验证承诺；两者都不可得时，不得把「未观察到故障」升级为「满足 PMP 准入」。

**SMP 入场门槛拆成三个判定，别再塞回基底**：`ctx.H` 不是初始化时的静态名单——每个次级 hart 在加入普通内存访问集合前，必须完成自己的准入；未通过的 hart 不得进入会访问普通 PMM 页的调度集合。

| 判定 | 检查对象 | 目的 |
|---|---|---|
| `HartRuntimeAdmission` | 当前执行工作集 + 活动页表及其遍历所需区域 + 上线后可能访问的全部既存共享对象 + 这些对象对应的映射与访问路径 | 决定该 hart 能否进入运行集合 |
| `CommonCpuAccessEligiblePages(ctx)` | 每个候选物理页 | 普通 S-mode 读写与必要的一致性 |
| `PageTableBackingEligible(page, ctx)` | 每个候选页表页——**含引导期已建立、未经 PMM 的那批** | 隐式遍历、PTE 更新与 A/D 方案要求；PMM 分配与对象审计共用同一份 backing 能力 |

**入场审计必须覆盖既存存活对象，不能只重算未来的可用集合**。失败形态：初始只 hart 0 参与 → PMM 按 `H={hart0}` 分出一批普通数据页 → 其中若干页位于 hart 0 可访问、hart 1 的 PMP 禁止的区域 → hart 1 加入 → 新的 `PmmBaseAvailable(ctx)` 正确地排除了这些区域。**第 5 步只阻止以后从这里分配，不能让已经交给内核对象的页忽然可访问。**因此新增 hart 的准入除「能运行当前启动工作集」外，还必须证明「能访问所有**已经分配且可能被该 hart 使用**的共享对象」，并且要说明这些对象经**哪些映射与访问路径**被该 hart 触及——只证明对象可读、不证明它经哪条 VA/页表路径到达，仍可能漏掉别名与高半映射：

| 情况 | 结果 |
|---|---|
| 所有可能访问的共享对象均可访问 | 允许入场 |
| 某些对象无法访问，但已完成迁移或隔离 | 满足迁移/隔离合同后允许入场 |
| 某些对象无法访问，且仍可能被调度路径触及 | 拒绝入场 |
| 对象访问范围或可达性无法证明 | 拒绝入场 |

与 `CommonCpuAccessEligiblePages(ctx)` 的分工：后者决定**未来可以分配哪些页**，入场审计决定**新 hart 能否安全加入现有内存使用体系**，两者不能互相替代；对应的负控是「旧 hart 已在某区域分配存活对象、新 hart 的 PMP 拒绝该区域 → 入场必须失败，除非对象已迁移或被明确隔离」。

`RsrvEventual` 不是基底要求，只在 `PageTableBackingEligible` 的 PTE 更新准入里、且方案为 `HardwareManagedAd`（或 Svade/Svadu 皆未实现）时被要求。分区内存留待后续。
**能力 ≠ 有效类型**：`WB` 能力通过不代表页表叶子与硬件配置共同得到的有效类型就是 WB；后者属硬件兑现层，不能拿能力位冒充。普通映射授权同样要有自己的属性兼容检查，不能只看 `OrdinaryRamMappingEligibility`。

**第五类：架构准入拒绝，且必须区分「不合格」与「证据不足」**：`PmmBaseExcluded = RamSourcePageDomain \ PmmBaseAvailable` 还包含三个基底交集条件的失败——`OrdinaryCachePolicyRepresentablePages`（缓存策略无法表达）、`MemoryOrderingCompatiblePages`（排序模型不兼容）、`CommonCpuAccessEligiblePages`（参与 hart 的普通读写不足）。这些页既没有显式保留、也不是存活对象、更不是平台政策拒绝；只列四类原因时，`PmmBaseExcluded` 的代数是闭合的，**排除原因的审计却不闭合**。补：

```
ArchitectureAdmissionRejected {
    cache_policy_evidence,
    memory_ordering_evidence,
    hart_access_evidence,
    profile_memory_obligation_evidence,   ← ProfileMemoryEligiblePages 拒绝时必须有这一项
    state: DefinitelyIneligible | InsufficientEvidence,
}
```

`DefinitelyIneligible` 是已有证据证明不能访问或不满足目标语义（平台事实）；`InsufficientEvidence` 是无法证明合格、按 fail-closed 排除（证据缺口）。两者都导致排除，但**诊断不能合并**——前者要做平台决策，后者要做补证，处理路径完全不同。
**四类排除原因必须分开，不能塞进一个未定义的 `AllocationReservations`**：① 类型资格（`UNUSABLE`/`UNACCEPTED`/`ACPI_NVS`/`Runtime` 等在阶段 `p` 是否允许分配）——构造规则是**先裁决、再取整页**：

```
ResolvedTypeEligibleRanges(p) = ResolveCompatibleTypeClaims(SourceRecords, p)
TypeAllocatablePages(p)       = FullPages(Union(ResolvedTypeEligibleRanges(p)))
```

不能对每条描述符单独 `FullPages` 再并集：两条相邻、语义兼容的来源应能合成一个整页；一条普通 RAM 与一条不可用类型相邻或重叠，则整页不合格。`TypeAllocatablePages(p)` 由「普通主存 backing 资格 + 阶段分配资格」共同派生，不是只看 EFI 类型数字；② 显式分配保留（`/reserved-memory`、MRB、固件前缀）；③ 存活对象（启动模块、页表、DTB 副本、栈、登记表）；④ 平台拒绝。反例：一条 `UNUSABLE` 描述符没有 DTB 声明，若只看显式保留，它会被算进可用表——与 O-06 冲突。

先并集再取整页，避免两个非页对齐的相邻 RAM 区间各自向内收缩时白丢一页；来源记录与身份**不合并**，只在纯计算层构造集合。同一页被多条排除原因覆盖时，PMM 只排除一次，但**所有排除理由都可追溯**（见 O-12g 的多重归属）。
**结构化候选（非阻断，已登记）**：`SourceRecord.range` 的语义要读 `kind` 才能解释——输入声明是原始范围，`FirmwarePrefix` 保存的是**派生范围**。可考虑拆成「输入声明」与「派生主张」两个变体，让 `declared_range` / `derived_range` 的差别在类型里表达；本轮不改，登记为重构候选，理由是不能为了形式统一在切片中途改公共字段。

**原始声明、RAM 交集与政策投影是三个范围**：`RawDeclaredRange` 是输入声明的完整字节（不裁剪、不取整）——DTB 侧已落为 `SourceRecord` 的 `range.start` / `range.end_exclusive`（派生记录的该字段是派生结果，不是原始声明），记录本身**不携带任何页范围**；`RamIntersection` 是与 RAM 来源域相交、按页向外量化的部分，用于 PMM 投影；`PolicyProjection` 是按 `no-map`、MRB、对象各自语义生成的禁止范围。**MRB 即使超出 `/memory` 也不能丢**：它的访问禁止语义不因为范围不在 RAM 内而消失。`no-map` 与标准系统映射相交时由独立的映射禁止投影处理；若项目只支持与 RAM 相交的 `no-map`，超出部分必须是显式拒绝条件，不能静默裁剪。今天两张投影表只表达 `RamIntersection`，`PolicyProjection` 尚无消费者——这正是 O-12g 记录层要先行的原因。**范围表示的三层与今天代码**（本轮把 DTB 腿的边界语义从算术副作用改成显式项目策略）：

| 层次 | 现状或目标 |
|---|---|
| DTB 原始输入 | `address + size` 两个 cell，保留原始编码（cell 数由父节点声明，规范化层才求和） |
| 今天的 `SourceRecord` | 已是 `range: PhysicalRange`（`{ start: u64, end_exclusive: u128 }`）：**记录层能表达 `2^64`**，能否交付由投影层判定 |
| 规范化物理范围（已落，DTB 腿） | `PhysicalRange { start: u64, end_exclusive: u128 }`，可表达 `[0xffff_ffff_ffff_f000, 2^64)`；UEFI 腿尚未接入 |

**代码已按这三层落定**：`physical_end` 在 `u128` 里算 `address + size`，只把 `> 2^64` 判为 `RangeOverflow`（真正的畸形输入）；`== 2^64` 一路被接受进 `PhysicalRange`。拒绝移到投影层——`PhysicalRange::projectable_end()` 对最高页返回 `None`，`/memory` 循环与 `declared_page_range`（同时服务 `/reserved-memory` 子节点与 MRB）在那里报 `UnsupportedPhysicalAddressEnd`，向外取整落到 `2^64` 同样归投影层。三条输入路径（`/memory` 的 `reg`、`/reserved-memory` 子节点的 `reg`、MRB 的地址+长度）现在共用同一条 `u128` 求和；`grep -c checked_add os/libs/minix-boot/src/memmap_source.rs` = 0。**剩下的一步**：投影层自身改用 `u128` 后即可交付最高页——那时 `UnsupportedPhysicalAddressEnd` 连投影层也不再需要。类型契约由 `physical_range_expresses_the_top_page` 钉住（最高页可表达、`projectable_end()` 为 `None`），边界行为仍由四个 `dtb_*_2_64_*` 用例守。

**MRB 的两种粒度**：精确 byte range 用于访问义务与审计；要通过页表保证不可访问时，另算相交页级范围，并说明「只覆盖一页中部分字节」时该页在其它对象的普通路径中是否允许映射。

**设备窗口的边界页负控（必须有测试）**：资源 `[0x1003, 0x1fff)` 若直接向外量化成 `[0x1000, 0x2000)`，授权就扩张到了相邻地址。边界页只能三选一：拒绝非页对齐资源；明确授权整页并证明邻接区的属性/权限/别名兼容；拆分后仍无法表达则拒绝启动。

### O-13 固定布局 ABI（第三层）

字段编码、偏移、容量、版本与失败形状都未定；现在只有两张区间表的布局。等 O-12 的子项定型后，按真实消费者设计，不在集合未定时冻结。

**判据**：每个跨界记录有唯一消费者、容量核算与失败形状。**消费者**：内核消费方。

## 6. 证据与复核

| 证据 | 内容 |
|---|---|
| 提交 `38668f843`（**功能落码提交，不是测试基线**） | DTB 保留来源落码：`/reserved-memory`、MRB、容量/回绕/重叠/形状拒绝、引导区 checked 守卫 |
| `cargo test -p minix-boot --lib` | 146 通过（采集器用例见 §2.1） |
| `cargo test -p boot-shim --lib` | 70 通过（分类器用例见 §2.2） |
| 交叉编译 | `x86_64-unknown-none`、`aarch64-unknown-none`（`fw-none-image`）、`riscv64gc-unknown-none-elf`、`x86_64-unknown-uefi`、`aarch64-unknown-uefi` |
| QEMU | `test-kernel-image-riscv64-bootface.sh` PASS（`### TEST_RESULT: PASS ###`，B1/B2/B3 三点位齐全，仅覆盖 bootface 入口）；`test-shim-bootmarks-aarch64.sh` PASS（串口读到 `boot-shim: memmap source: uefi-getmemorymap (dtb /memory cross-checked, diagnostic only; conv=13 dtb_regions=1)` 与内核进入行）；`test-shim-bootmarks-x86.sh` PASS（串口 `boot-shim: memmap source: uefi-getmemorymap (no dumpdtb.dtb on ESP)`——该腿没有 ESP 上的 `dumpdtb.dtb`，**DTB 采集器根本没被进入**，故它只证明 x86 启动未回归，不构成对本切片新分支的覆盖）。三条腿均在**本轮边界改动之后**重跑：第六十三轮的 `size`-only 与本轮的 `2^64` 分名都不改变 QEMU/OpenSBI 与 EDK2 实际递交的 DTB（其实例子节点都带 `reg`，也没有任何范围触到顶端页），故启动行为不变——这是「未观察到变化」，不是「已验证新分支」；两个新分支目前只有宿主测试覆盖。riscv 腿 B1/B2/B3 点位仍全；aarch64 腿读到 `conv=13 dtb_regions=1` 与内核进入行；x86 腿仍是 `no dumpdtb.dtb on ESP`（不进入采集器） |
| 来源记录层 | `collect_with_sources()` 交出六类 `SourceRecord`（容量 40）；`no-map`、RAM 外声明、MRB 出表后仍可追溯：`dtb_no_map_reservation_keeps_a_source_record`、`dtb_memory_reservation_record_keeps_the_raw_range`、`dtb_reservation_outside_memory_keeps_its_record`、`dtb_source_records_cover_every_projected_span`；`collect()` 语义不变；`dtb_collect_and_collect_with_sources_agree` 钉住两条入口不漂移；`dtb_no_map_declarations_count_against_the_declaration_cap` 钉住「RAM 外的 no-map 不进任何表仍占声明容量」；`dtb_reusable_duplicates_are_deduplicated_not_rejected`、`dtb_ordinary_and_reusable_same_range_is_a_conflict`、`dtb_reusable_and_no_map_same_range_is_a_conflict`、`dtb_partial_overlap_of_memory_ranges_is_rejected` 钉住三分裁决 |
| 本轮规范原文取证（主源直取，可复核） | RISC-V：`riscv/riscv-isa-manual` @ `418bdc4990ae2d60a86f1e11e8d7795aa279da61`（`main` 只作阅读入口，不作为冻结版本）`src/priv/supervisor.adoc`（两种 A/D 方案、`norm:pte_ad_scheme_uniform`「All harts in a system must employ the same PTE-update scheme as each other.」、硬件更新方案要求「hardware page-table write access and *RsrvEventual* PMA」）与 `src/priv/machine.adoc`（`norm:menvcfg_adue_op`、`norm:menvcfg_adue_rdonly0`、Memory Ordering PMA、Reservability PMA 三级、Idempotency PMAs）。设备树：`devicetree-org/devicetree-specification` 主分支 `source/chapter3-devicenodes.rst`（`/reserved-memory` 父子节点属性表、`reg`/`size` 优先关系、动态区不入 UEFI 内存图、`no-map` 与 `reusable` 互斥）与 `source/chapter5-flattened-format.rst`（MRB「shall not overlap each other」、以 `EfiReservedMemoryType` 列出是 *should*）。RISC-V 与设备树两处已改为本方直接取证；**UEFI 侧的证据等级本轮上调一档**：`EfiReservedMemoryType` 在类型表（Table 7.10）的字面「Not usable」、`GetMemoryMap()` 对「无物理硬件支撑」与「有物理支撑但 OS 不应访问」的区分、以及 §4.6.1.3 的「标准 `EFI_DTB_TABLE_GUID` 必须位于 `EfiACPIReclaimMemory`、按物理地址解释、运行时不修正、固件安装后不得修改」三项，已由**评审人独立取到 UEFI 2.11 原文并给出章节/表号**；本方环境抓取 uefi.org 仍返回 403，**本方未复现**。因此这三项按「第三方独立核对」引用，不写成「本方已验证」。**仍是缺口**：AArch64 Table 2.5/2.6 的分工只有评审人核对，没有本方或第三方的原文引用。**边界**：规范保证「固件不修改」不等于「所有 UEFI 应用/驱动都不修改」，复制期间源对象的一致性仍须由当前 loader 自己保证 |
| 保留声明形状分流 | `size`-only / 无 `reg` 无 `size` / `reg`+`size` 三种形状各有专名错误与专测（见 O-12g 行 c）；`UnsupportedDynamicReservation` 拒绝的是 **DTSpec 3.5 允许、但第一版不支持**的合法动态保留请求，规范语义已由本方直取原文核实 |
| 边界端点分流 | `address + size` 在 `u128` 里三分：`>2^64` → `RangeOverflow`；记录层的 `range.end_exclusive` 是 `u128`，**恰好等于 `2^64` 现在可表达**；投影侧 `declared_page_range` 向外取整达到 `2^64` 时报 `UnsupportedPhysicalAddressEnd`，`collect()` 构表前再用 `projectable_end()` 复查同一条件；四测试 `dtb_memory_range_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_reservation_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_top_partial_page_reservation_is_refused_as_unsupported_end`、`dtb_memory_range_past_2_64_is_range_overflow`；`grep -c 'checked_add' …/memmap_source.rs` = 0 |
| 近几轮规范取证（本方直取，锚点即条目名） | `src/priv/machine.adoc` 的 PMP 小节：`norm:pmp_cfg_permissions`（R/W/X 按访问类型许可）、`norm:pmp_rwx_warl`（R=0 且 W=1 为保留编码）、`norm:pmp_exec_fault`、`norm:pmp_load_fault`（load 与 load-reserved 查 R）、`norm:pmp_store_fault`（store、store-conditional、AMO、cache-block zero 查 W）、`norm:pmp_misaligned_access_behavior`（非对齐访问可被拆成多次操作，逐次独立检查；通过检查的那一半 store 可能已可见）、`norm:pmp_check_priv_modes`（有效特权级 S/U 全覆盖，含 M-mode 下 `MPRV=1 ∧ MPP∈{S,U}` 的数据访问）、`norm:pmp_check_pagetable_access`（页表访问的有效特权级为 S）、`norm:pmp_l_bit_m_mode_enforcement`、`norm:pmp_no_entry_match`、`norm:pmp_entry_count`（可零条）、`norm:pmp_granularity` 与 `norm:pmp_a_field_napot_na4`（4 字节粒度）、Smepmp 的 `norm:mseccfg_mml_warl`、`norm:mseccfg_mml_pmpcfg_l_op`、`norm:mseccfg_mml_m_rule_op`、`norm:mseccfg_mml_soru_rule_op`、`norm:mseccfg_mml_shared_l0_op`、`norm:mseccfg_mml_shared_l1_op`、`norm:mseccfg_mml_shared_lrwx_1111_op`、`norm:mseccfg_mml_x_restrict`、`norm:pmp_rw01_rsv`、`norm:mseccfg_mmwp_op_warl`、`norm:mseccfg_rlb_op_warl`、`norm:mseccfg_mml_mmwp_rlb_rst`，以及 PMP 小节那段**无编号 NOTE**「some platforms may provide other PMP structures in addition to or instead of the scheme described in this section」；原子侧 `src/unpriv/zaamo.adoc` 的 `norm:amo_op` 与 `norm:pma_amo_levels`；主存义务 `norm:Zicclsm_misaligned_support`、`norm:Ziccamoa_amo_pma`、`norm:Ziccrse_rsrv_eventual`。**第七十七轮评审人另独立核到** UEFI 2.11 属性位低位组（`WP=0x100`/`RP=0x200`/`XP=0x400`/`NV=0x800`）与 2.10 原始版高位组的差异，与本简报 O-06 的冲突位矩阵一致；本方仍未取到原文。**本轮新增（第七十七轮）**：`src/unpriv/cmo.adoc` 的 `norm:pmp_same`（块内 PMP 位一致且 W⇒R）、`norm:pma_same`、`norm:cbo_rsv`（不满足则行为 UNSPECIFIED）、`norm:cbm_access`（cache-block management 只要 load **或** store 许可）、`norm:cbz_access`（cache-block zero 需 store 许可且 PMA 支持）、`norm:cbp_access`；`src/priv/supervisor.adoc` 1745–1748（A/D 更新「Perform the following steps atomically … Then store _pte_」）。**本轮仍缺的**：`src/priv/supervisor.adoc` 地址翻译算法里「对 PTE 的 store 会先过 PMA/PMP 检查」这句话**没有 norm 锚点**（本方直取原文，按算法步骤引用），因此硬件 A/D 隐式写查 W 的依据强度低于命名规范条目；cache-block 的 PMA supported-access-type 读数需由平台 profile 提供，本方未取；UEFI 与 `EfiReservedMemoryType` 的「Not usable」措辞仍是评审人核对，本方未独立取到原文 |
| FDT 容器校验 | `fdt_layout::validate` **28 条测试**（`accepts_a_canonical_layout`、`accepts_an_unaligned_strings_block`、`accepts_a_property_before_children`、`rejects_a_property_after_a_child`、`rejects_an_unknown_token`、`rejects_a_second_root`、`rejects_unbalanced_end_nodes`、`rejects_an_end_token_in_the_middle`、`rejects_a_property_running_past_the_block`、`rejects_a_property_name_outside_the_strings_block`、`rejects_an_unterminated_property_name`、`rejects_nesting_beyond_the_parser_limit`、`the_parser_consumes_the_deepest_tree_this_gate_accepts`、`rejects_misaligned_blob_address`、`rejects_misaligned_mrb_offset`、`rejects_mrb_outside_the_object`、`rejects_missing_terminator_before_the_struct_block`、`rejects_a_terminator_only_beyond_totalsize`、`rejects_more_reservations_than_the_collector_accepts`、`rejects_struct_block_past_totalsize`、`rejects_overlapping_struct_and_strings_blocks`、`rejects_totalsize_larger_than_the_buffer`、`rejects_a_structure_block_without_the_end_token`、`rejects_unbacked_structure_alignment`、`rejects_unsupported_fdt_versions`、`rejects_a_layout_whose_mrb_follows_the_struct_block`、`rejects_an_mrb_that_starts_inside_the_header`、`rejects_strings_before_the_structure_block`）；五处调用点：`bootface.rs`、`bootface_a64.rs`、`opensbi_helpers.rs`、`uefi_helpers.rs` 两处 |
| 解析库依赖事实 | `fdt 0.1.5` `node.rs`：`properties()` 首个非属性 token 即停；`children()`/`all_nodes()` 遇「子节点后属性」即结束；`all_nodes()` 父栈 64 槽、根在下标 1（最大安全深度 63）。QEMU 8.2.2 `dumpdtb` 全树 0 处属性后置 |

**规范版本钉法（本轮补）**——引用规范要钉**发布版与源 commit**，不能引用活动页面：

```
RISC-V Privileged Architecture
  official release 入口标签: v20260120（docs.riscv.org/reference/isa/v20260120/priv/…）
  相关模块: Supervisor ISA 1.13、Svade 1.0、Svadu 1.0、Machine ISA 1.13
  本方本轮实际读取的源: github.com/riscv/riscv-isa-manual
      @ 418bdc4990ae2d60a86f1e11e8d7795aa279da61  src/priv/supervisor.adoc, src/priv/machine.adoc
  用到的锚点: norm:pte_ad_scheme_uniform、norm:menvcfg_adue_op、norm:menvcfg_adue_rdonly0、
      norm:menvcfg_adue_fence、norm:pma_rsrv_levels、norm:Ziccrse_pma_rsrv_req、
      norm:Ziccamoa_pma_amo_req、norm:Ziccamoc_pma_amo_req
  本轮新增读取（同一 commit）: src/priv/machine.adoc 的 PMP 与 Smepmp 小节、
      src/unpriv/zaamo.adoc、src/unpriv/zicclsm.adoc、src/unpriv/ziccamoa.adoc、
      src/unpriv/ziccif.adoc、src/unpriv/ziccrse.adoc

RISC-V Profiles
  本方本轮实际读取的源: github.com/riscv/riscv-profiles  src/rva23-profile.adoc
      blob sha 4fce3ebf34393d7a52564dfab53b69730fab514d
      该文件最近一次提交 2852ae1f1bfbc34334fcee8b8345472e4811dfd7
  读到的事实（只作 profile 义务表来源，第一版不声明合规）:
      RVA23U64 强制项含 Ziccif、Ziccrse、Ziccamoa、Zicclsm、Za64rs、Zic64b、Zicbom/Zicbop/Zicboz；
      Zacas 与 Ziccamoc 属 Development Options；强制的预约集条目是 Za64rs（≤64 字节），
      Za128rs 只出现在文末的术语对照表里，不是强制项；
      RVA23S64 强制项含 Svbare、Sv39、Svade、Ssccptr、Sstvecd、Ss1p13，
      Svadu 属新增 Expansion Options（即硬件 A/D 不是该 profile 的强制项）
  更正: 上一轮该清单是按评审人转述登记的，本轮改为本方直取，之前标注的「未独立核对」缺口撤销

Devicetree Specification
  本方本轮实际读取的源: github.com/devicetree-org/devicetree-specification
      @ 8314f1e1c9965482e5c1080d4118f52fef827fce  source/chapter3-devicenodes.rst,
                                                   source/chapter5-flattened-format.rst
  站点当前显示的 revision 为 v0.4-12-g8314f1e（评审人读出；本方只核到 commit 前缀 8314f1e 一致，未独立核该 tag 串）
  在线页面本身标 unknown-rev，只作阅读入口，不作为冻结版本引用
```

**Profile 与 UEFI 仍是缺口**：RVA20/RVA22/RVA23 各自的版本号与能力清单要**另行钉住**，不并入某一份 Privileged ISA；UEFI 2.11 的 §4.6.1.3 与 AArch64 平台绑定（Table 2.5/2.6 分工）本环境抓取返回 403，仍属**本方未取证**。

**下一轮最该补的证据（可复核事实，不是新公式）**：

| 材料 | 为什么 |
|---|---|
| x86_64 OVMF 与 aarch64 AAVMF 的实际镜像身份、版本、哈希 **+ 该固件实际返回的代表性描述符及其 `Attribute` 原始值 + 项目接受的完整位解释表** | 要证明的是运行固件**实际用了哪套编码**，不是它自报符合哪个规范版本；两条腿的固件也不必同源 |
| QEMU 构建版本、machine version、accelerator、CPU 型号、SMP 配置（逐 `PlatformMemoryProfile`） | 设备窗口与 PLIC 公式目前仍是 profile 候选值 |
| `PreflightMap` / 首次 `FinalMap` 的缓冲容量策略与 `EFI_BUFFER_TOO_SMALL` 处理 | 三种失败位置的合同已定，缺实现与测试 |
| 四条 DTB 输入路径的启动 ABI 合同，特别是「复制完成前原 DTB 不被并发修改」的依据 | O-11 的内容一致性条件目前是假设 |

**证据分层（编译通过 ≠ 启动验证）**：

| 层级 | 本轮状态 |
|---|---|
| `minix-boot` 宿主单测 | 146 PASS |
| `boot-shim` 宿主单测 | 70 PASS |
| riscv64 启动路径 QEMU | PASS（exit 0） |
| aarch64 镜像组装 | **PASS**（`b71bce1d0`/`ae4ef5c76`/`8dc30f75e` 依次补齐 `sendrec` 实现后） |
| aarch64 启动行为 | **PASS**：`test-shim-bootmarks-aarch64.sh` exit 0，串口读到 memmap 来源行与内核进入行。**但日志中的 `dtb /memory cross-checked, diagnostic only` 不代表 O-11 的配置表 DTB 首读授权已验证**——配置表 DTB 尚未进入政策链，该测试证明的是今天实际接入的路径 |

**证据基线**：本轮代码提交 `f52730bef`。复核命令与顺序（缺一不可）：

```
git rev-parse HEAD
git status --short
cd os && cargo test -p minix-boot --lib
cd os && cargo test -p boot-shim --lib
```

146/70 条测试是**该 HEAD、该工作树状态**下的读数（§2.1、§2.2、§6 三处数字必须一致）。`git status --short` 的期望输出只有用户自有文件 `AI-chats/daily.todo.md` 一行；出现其它改动即说明测试不是在干净基线上跑的。基线之后如有新提交，必须重跑并更新本段。

复核方式：对 §2 的每条主张，文中给出了测试名或代码位置；对 §5 的每条问题，文中给出了失败形态——评审人可以只凭本文判断「问题是否成立」。

**测试通过不等于输入完整性**：146 与 70 条测试说明的是「已覆盖的行为」，不能证明所有 DTB 输入都被解析、所有原始访问点都被登记、每条启动路径都用了正确的权威来源。这几项各自需要独立的完整性论证（O-07、O-11、O-12）。

## 7. 签字状态

| 层 | 结论 | 依据 |
|---|---|---|
| 第一层：语义与方向 | **可签** | 五条不变量；§4 的准入合同方向明确 |
| 第二层：集合与逐路径判据 | **不签** | 按完整性边界分为六组（见下） |
| 第三层：固定布局 ABI | **不可签** | 未开始（O-13） |

**第二层的六个阻塞组**：

**逐项签字视图（第七十九轮评审后）**：

| 项目 | 状态 | 未闭合的是什么 |
|---|---|---|
| A/D 更新方案 | 分支合同已闭合 | 缺平台证据与实现（固件逐 hart 方案证明） |
| 系统 hart 一致性 | 两集合分离已闭合 | 清单完整性证据缺失（`HartInventoryEvidence.completeness_claim`） |
| hart 入场 | 既存共享对象审计方向已闭合 | 缺对象登记表与实现 |
| 普通访问 / 内存序 | 非一致主存已裁定排除；PMP 无条目分支与 PMA 合成已写成谓词 | **本轮不闭合**：整页合格的起始量化刚改写，无实现、无负控 |
| 两层代数（基底 / 用途特有） | 已分离，`RoleAdmit ≡ RolePageEligible` 单一展开式，`U ≠ ∅` 与基底非空前置已写 | 无实现；`u128` 计算待落 |
| 准入上下文闭合（**P0 阻塞**） | `ParticipatingCpuSet = Resolve(id, inventory_snapshot)` 不可变并绑定 `evidence_generation`；`legal_starts_contract` 进 `ctx` 与 `AdmissionContextId`；`ProfileAdmissionState` 四态，第一版 `NotDeclared` 且页级取中性语义 | 无实现；`SystemPteUpdateSchemeUniform` 在非 RISC-V 的 `NotApplicable` 分支未落 |
| 整页合格判定式 | 全称谓词 + 合同 A 补「完整 footprint 约束」；`PageTable` 已补回始终必需的隐式页表读（形式化缺陷已修）；非空前置覆盖基底集合、`U`、参与 CPU 集合与非 `GeneralData` 用途；`GeneralDataCompleteness` 已进 `BasePageEligible` | `RoleRequirementsComplete`/`Applicable` 与完整性证据无实现；Fetch 页尾排除无实现 |
| 原子与 Profile 义务进入公式 | **已接入最终公式**：`RoleAdmit(AtomicSharedState, …)` 派发到 `AtomicRoleEligible`（逐 CPU、逐实际访问范围判几何）；`AdmissionContextValid` 进 `BasePageEligible`，`ProfileMemoryEligiblePages` 进 `PmmBaseAvailable`；第一版统一「所有 LR/SC 都要 `RsrvEventual`」且基础页内约束恒成立 | 全部无实现；`isa_profile_id = 未声明` 时结果必须记「未作合规声明」，不得被读成已通过 |
| 架构边界（P1，重构项） | 三条硬规则已落进类型：`operation` 改用语义名（`NormalRead`/`NormalWrite`/`InstructionFetch`/`AtomicUpdate`/`CacheMaintenance`），`ctx` 的 RISC-V 专有证据移入 `adapter_evidence` 槽位；`ctx` 身份三分（平台内存 / ISA / UEFI 属性） | 无代码；AArch64/x86 adapter 的语义→机制展开表尚未写 |
| 取指访问模型 | `InstructionFetchPattern` + `FetchFootprints` 已定义，「对齐 ≠ 宽度」且取指可跨页；对象级按**每个指令实例**的宽度与片段判定，不再用固定宽度代表全部 | 页级保守片段集与对象级多片段证明均无实现；x86 变长指令跨页仅按架构事实使用，未取 SDM 原文 |
| 对象访问资格（**P0 阻塞**） | 三层已分开且已闭合到同一判定：政策授权 `ObjectPolicyAccessEligible`、硬件准入按 `ActualAccessInstance` 的每个片段与 `instance.requirement` 判、映射兑现按分段 `Resolve`；实例必须满足 `AccessAlignmentSatisfied`，`RequiredCpus ≠ ∅` | 全部无实现；`NotApplicable` 分支（无 CPU 直接访问的对象）与别名审计未落 |
| PMP 证据稳定性与保护清单 | 同上；稳定性改为「不可变承诺（第一版）」或「变更前协调」二选一，`stability_until` 定义为承诺而非期限 | 无 provider、无协调协议、无 `evidence_generation` 递增路径 |
| Profile 与有效方案 | 两者已分离，五档校验表已写；`Za64rs` 几何与基础页内约束分成两层并都有判定位置 | 第一版不声明任何 RVA Profile，profile 几何为「不适用」，**但基础页内约束不适用这条捷径不成立**；几何事实的 profile 来源未实现 |
| PMP 权限映射与 Smepmp | 操作→许可位表逐行带规范锚点；第一版只接受 `MML=0 ∧ MMWP=0` | 硬件 A/D 隐式写查 W 的依据是遍历算法里那句无编号锚点的 store 条款，强度低于命名规范条目；`Absent` 的平台支持限制与 `mseccfg` 证据提供者未落地 |
| 原子算法进展 | 合同已定形：基础页内约束 ∧ 进展要求（第一版一律 `RsrvEventual`）∧ profile 几何三项并列；`RegisteredAtomicAlgorithms ≠ ∅` 也是前置 | `AtomicFallbackPlan` 与 `ReservationSetGeometryPermits` 为实现预留 |
| 页表用途准入 | backing / 分配 / 既有对象已三分 | 既有对象须按 `ObjectAccessEligibility` **逐使用点**判定，不得引用页级谓词冒充；缺实现 |
| ISA Profile 义务 | 第一版不声明任何 RVA Profile；义务表由本方直取核对，`Svade` 强制与 `Svadu` 扩展选项的关系已写进 profile 校验表 | 空集≠已满足；profile 合规判定与 `ReservationSetRequirement` 无实现 |
| UEFI DTB 与 MapKey | 方向认可；三方组合（EFI DTB × MRB × `no-map`）与「类型按页 / 访问按原始字节」的粒度分工已定形 | 无实现与负控；UEFI 侧证据为评审人核对，本方未独立取到原文 |
| AArch64 64 KiB 分组 | 方向可签 | 规则内容已核；本方未独立取证 |
| UEFI 属性位版本差异 | 策略可签，profile 未签 | 冲突域 fail-closed；固件镜像证据未钉 |
| 设备窗口 | 模型可签，数值未签 | 四层关系已定；APIC 核对改为**逐逻辑处理器**（BSP 一次核对不构成全平台证明）；QEMU 平台证据未齐 |

| 阻塞组 | 对应合同 |
|---|---|
| 输入真实性与边界 | O-11（含 EFI DTB 专属类型合同、三方组合裁决表、页/字节粒度分工）、O-12h、**O-12k（MapKey 快照）**、O-12j（a/b 已实现，b 的预编译依赖闭包未证；c 未闭合）、DTB 节点准入 |
| 来源保真、RAM 资格与生命周期 | O-01、O-06（属性位冲突域已 fail-closed，完整固件 profile 仍待钉）、O-12f、O-12g（DTB 记录层已落，UEFI 描述符记录未落） |
| PMM 投影与页粒度 | O-12d、O-12i。**P0 阻塞项已明确列出**：① 对象访问缺来源政策授权层；② `PageTable` 形式化漏隐式页表读；③ 用途特有集合与参与 CPU 集合仍可真空通过。此外 A/D 方案、起始量化、Smepmp 边界、原子与 Profile 义务的**证据合同与负控未闭合**，实现尚未开始 |
| 设备资源、映射和访问 | O-12c、O-08 |
| 禁止集合与逐树审计 | O-02a/b、O-03、O-04、O-05、O-12a/b |
| 对象与访问完整性 | O-07、O-09、O-10、O-12e |

**「禁止集合编译 + 引导根审计」有代码只能作为第一轮实现复审的起点，不是第二层签字的条件**：输入边界、来源完整性、PMM 资格与设备映射覆盖都是它的前置证明。

**第二层第一轮实现复审的起点**：禁止集合编译与引导根审计具备代码和测试后，可以开始复核其实现；这不代表其它阻塞组已满足，也不构成第二层签字。在那之前，对 §5 的评审只需回答两个问题：问题是否成立、候选是否覆盖失败形态。
