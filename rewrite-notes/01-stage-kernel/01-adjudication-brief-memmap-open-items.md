# 引导期内存政策 —— as-built 合同

> **本文的写法**：每个「已实现」条目都指向代码与测试；每个「未设计」条目只写问题、判据与第一个消费者，不写实现方案。
> **输入来源**：代码、测试名、ADR。**不是**上一版文档。
> **上一版**（含二十八轮论证与历史公式）冻结在 `04-design-backlog-round28.md`，只增不改。
> **更新方式**：按切片全量重写；没有代码变化的评审轮只产出 delta。

## 0. 怎么读

| 标记 | 含义 | 读者能拿它做什么 |
|---|---|---|
| **已实现** | 代码今天保证的行为 | 可引用测试名复核；可作为合同签署 |
| **合同（未实现）** | 已裁定的设计约束，代码还没有 | 只能作为实现目标，不能当作现状 |
| **未设计** | 只有问题定义；触发它的第一个消费者已写明 | 不能签；不能按名字推断行为 |

五条不变量对三者都成立，任何改动不得违反。

## 1. 五条不变量

1. **来源有身份，量化只做一次，方向由政策角色定**——不得先合并成无身份并集再归一化。
2. **PMM 域 = 可用 ⊍ 排除**，对「来源域」成立；来源域之外的地址不参与。
3. **两棵树各一个禁止集合；禁止与分配正交**——「不许分配」推不出「不许映射」，反之亦然。
4. **义务键 =（对象｜设备窗口，树，阶段，路径）**；缺任一维不构成义务。
5. **设备访问 ≠ 设备映射**：MMIO 与端口 I/O 是两个地址域，映射义务与访问合同是两份记录。

## 2. 已实现（as-built）

### 2.1 设备树腿的采集（`os/libs/minix-boot/src/memmap_source.rs`）

`DtbMemoryMap::new(&fdt, firmware_reserve_end).collect()` 产出**两张表**：

| 表 | 内容 | 语义 |
|---|---|---|
| conventional | `/memory` 声明，扣除固件前缀与所有保留声明 | PMM 可直接分配 |
| reserved | 固件前缀 + 非 `no-map` 的保留声明（与 `/memory` 相交的部分） | 可达、永不再分配 |

解析范围与拒绝条件：

- `/memory`：全树枚举 + `reg()`，支持 unit-address 与多段；单表容量 8 段，超出返回 `TooManyFreeRegions` / `TooManyReservedRegions`（**不再静默截断**）。
- `/reserved-memory`：路径查找 + 可见性交叉检查（找得到节点却取不到即拒绝）；要求父节点 `#address-cells`/`#size-cells`；每个子节点必须有可解析的 `reg`；`no-map` 按属性存在与否判定；**`reusable` 不区分**，按保留处理。
- 文件头保留映射块（MRB）：`memory_reservations()`，条目从两张表移除（禁止访问语义）。
- 取整：声明按 4 KiB **向外**量化，再与 `/memory` 全景求交；越出部分不产生 RAM 主张。
- 错误：`RangeOverflow`（回绕）、`OverlappingReservedRanges`（声明重叠，含 `no-map` 与普通混合）、`UnsupportedReservedDeclaration`（缺 `reg`、宽 cells、缺 cells 属性）、`ReservedMemoryUnreachable`。

测试（`cargo test -p minix-boot --lib`，75 通过）中的采集用例：`dtb_single_memory_region_fw_end_zero`、`dtb_multi_region_and_unit_address_variant`、`dtb_more_than_capacity_is_rejected_not_truncated`、`dtb_declared_reservation_is_never_conventional`、`dtb_no_map_reservation_leaves_both_tables`、`dtb_reservation_inside_the_firmware_prefix_is_not_duplicated`、`dtb_reservation_outside_memory_claims_no_ram`、`dtb_memory_reservation_block_is_carved_from_free`、`dtb_overlapping_reservations_are_rejected`、`dtb_reservation_with_wrapping_end_is_rejected`、`dtb_reserved_memory_with_wide_cells_is_rejected`、`dtb_carving_that_exceeds_capacity_is_rejected`。

**边界**：两张表就是今天全部的政策输出。禁止集合、PMM 累积器、设备窗口集合都还不存在（见 §4）。

### 2.2 UEFI 腿的分类（`os/boot-shim/src/ebs_map.rs`）

唯一解析器 + 六步裁决 + 三份输出：

| 输出 | 内容 | 容量与超限行为 |
|---|---|---|
| 可用表 | `CONVENTIONAL`（写回、无独立属性） | 128 段，超限拒绝启动 |
| 保留表 | 引导服务代码/数据、`ACPI_RECLAIM`、`LOADER_*`（写回） | 256 段，超限拒绝启动 |
| 诊断成员（side list） | `RUNTIME_*`、`ACPI_NVS`、`PERSISTENT` 及其它不建普通映射的类型 | 64 段，超限拒绝启动；**每个成员保存完整区间** |

- 不产出区间的处置：`RESERVED`/`UNUSABLE`（Excluded）、类型 11/12（DevicePath）——它们不进入任何表，也**没有**进入禁止集合的能力（禁止集合未实现）。
- 属性处理：保留原始 `Attribute`，在适配器内做准入；ISA 位、未知位、档外位一律 fail-closed。
- 描述符前缀按固定偏移读取（40 字节 v1 前缀），支持固件给出的更大 stride。

测试（`cargo test -p boot-shim --lib`，66 通过）：`types_that_earn_no_ordinary_mapping_go_to_the_side_list`、`excluded_and_device_types_stay_out_of_every_list`、`rejects_side_list_beyond_its_buffer`、`the_four_ambiguous_combinations_land_as_adjudicated`、`geometry_is_checked_for_ranges_that_are_emitted` 等 32 条。

### 2.3 引导根（`os/kernel/src/lib.rs`、`os/kernel/src/dm_coverage.rs`）

- **今天只有一棵项目根**：`build_bootstrap_root_and_enable` 建立恒等 4 GiB（设备窗口按 2 MiB 粒度打 `DEVICE`）+ 内核高半映射 + aarch64 设备窗口高半别名。
- Step 4 的 DM 覆盖是**在同一棵根上追加 PTE**（`establish_boot_dm`），来源是 `memmap ∪ 自举页表树 ∪ 启动模块 ∪ 保留表` 的并集过滤。
- **没有独立的「最终根」**，也没有审计遍历、禁止集合或页表冻结。后续进程根由 VM 侧按 `kern_dm_pages` 重建。

### 2.4 交接与消费

- 两张表经固定布局交接给内核（可用表 128、保留表 256 段；另有模块表与标量）。
- 内核 PMM 用可用表；保留表供诊断与查询接口使用（`kinfo.reserved_regions()`）。
- 引导区的相交守卫使用 `checked_add`（`assert_bootstrap_outside_memmap`），回绕输入直接失败。

### 2.5 三条启动腿

| 腿 | 采集器 | 现状 |
|---|---|---|
| x86_64 UEFI | `ebs_map` | 已实现 |
| aarch64 UEFI | `ebs_map` | 已实现；配置表 DTB 只登记物理地址，不解析、不复制 |
| riscv64 自引导 / U-Boot | `DtbMemoryMap` | 已实现；QEMU 启动用例 PASS |

### 2.6 尚未存在的机制（读数为证）

`grep -rn "Deny\|deny" os/kernel/src os/arch/src`（排除测试）无禁止集合；`grep -rln "fn audit" os/kernel/src os/arch/src` 为空。对象登记表、设备义务表、恒等 VA 检查、PMA/PMP 准入、固定布局 ABI 均无代码。

## 3. 合同（已裁定、未实现）

| 合同 | 一句话 | 现状 |
|---|---|---|
| 三架构普通主存语义 | x86：自建 PAT + 只读 MTRR 分析 + 每核 `CR0.CD=0/NW=0`；AArch64：MAIR + 两棵树各自遍历；RISC-V：按区间的 PMA 能力清单 + PMP 准入 | 全部未实现，属平台准入 |
| 恒等 VA 合法性 | `VA = PA` 必须落在该树分页配置的合法虚拟地址范围，且不与既有布局冲突 | 未实现 |
| `effective_pa_limit` | **物理地址可表示上界**（格式位宽 ∩ 平台位宽），**不承诺**上界以下可访问 | 未实现；PMP/PMA 是独立维度 |
| 保留映射区权限 | 第一版一律 `R`（只读、NX）；需要写的对象拒绝启动或另立对象 | 未实现 |
| 设备访问边界 | 原始 MMIO/端口操作只允许出现在逐个列举的包装函数；调用点带 `AccessUseId`；CI 静态审计 + 负控 | 未实现；现状 raw 端口 asm 仅 5 个文件，`kernel/`、`servers/` 零命中 |

## 4. 未设计（问题 + 第一个消费者）

每条只定义问题与判据；**不允许按名字推断行为**。

| # | 问题 | 第一个消费者 |
|---|---|---|
| O-01 | `ReservedMappedRanges` 的来源合成：UEFI 类型来源（如 `ACPI_RECLAIM`）+ DTB 普通声明 + 固件前缀 − `no-map` − MRB − 其它禁止页；UEFI 分支的「完整 RAM 支撑页集合」需逐类型列出 | 最终直接映射的构造 |
| O-02 | `DenyOnly`（声明页 − RAM 页）落在哪：进两份禁止集合，还是证明不可达。今天的引导根无差别恒等 4 GiB，不能靠「不在可用表」证明未映射 | 引导根审计 |
| O-03 | 域外保留声明的处置：只约束 PMM 表，还是同时约束禁止集合；`/reserved-memory` 与 MRB 语义不同（前者不自动获得访问禁止） | 同上 |
| O-04 | 对象矩阵的 `MustNotMap` 如何进入两份禁止集合（`ObjectBootMapDenyRanges` / `ObjectFinalMapDenyRanges`）；若第一版为空必须显式断言 | 对象登记表 |
| O-05 | T1/T2 的审计集合与冻结语义：T1 前只可能审计引导根（含 `R13-D`）；「冻结」应指映射定义字段（覆盖、PA、页大小、权限、属性、共享性），架构允许的 A/D 硬件更新另定 | 引导根切换代码 |
| O-06 | `RamBackingPages(boot_mode)` 的定义：UEFI 分支不能直接拿 `SourceDomain` 当 backing；`TypeExcluded`/`Diagnostic`/设备类描述符不得提供普通主存支撑 | R4b-c3 检查器 |
| O-07 | `StartupObjectAccessRequirements`：对象身份、阶段、范围、路径、读写/取指类别；映射存在不等于实际访问 | R20 |
| O-08 | 端口 I/O 的地址域：不参与 4 KiB 物理归一化，不产生页表与 PMM 义务；`PortWindowPolicy`/`PortIoAccessRequirements` 是否跨界（今天的答案：不跨界） | 端口审计 |
| O-09 | 诊断成员与样本的存储登记：完整区间表与有界样本数组各自的位置、容量归属、最后使用阶段 | 容量清单与对象目录 |
| O-10 | R13-A 的第二枚负控：在**已批准的包装函数体内**新增裸操作必须被 CI 拒绝 | CI 审计器 |
| O-11 | 配置表 DTB 的复制链与保留语义（aarch64 UEFI 腿）：复制目标、容量、生命周期，以及内核改读副本 | UEFI 腿的保留政策 |
| O-12 | 两份禁止集合、设备窗口集合与 PMM 累积器的**构造式与来源完整性**（本轮评审列出的全部公式问题都在这里，尚无代码） | 页表规划的规划阶段 |
| O-13 | 固定布局 ABI（第三层）：字段编码、偏移、容量、版本、失败形状 | 内核消费方 |

## 5. 证据索引

| 证据 | 内容 |
|---|---|
| 提交 `38668f843` | DTB 保留来源落码：`/reserved-memory`、MRB、容量/回绕/重叠/形状拒绝、引导区 checked 守卫 |
| `cargo test -p minix-boot --lib` | 75 通过 |
| `cargo test -p boot-shim --lib` | 66 通过 |
| 交叉编译 | `x86_64-unknown-none`、`aarch64-unknown-none`（`fw-none-image`）、`riscv64gc-unknown-none-elf` |
| QEMU | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh` PASS；串口 memmap 行不变 |

## 6. 签字状态

| 层 | 结论 | 依据 |
|---|---|---|
| 第一层：语义与方向 | **可签** | 五条不变量已由代码或明确合同支撑 |
| 第二层：集合与逐路径判据 | **不签** | §4 的 O-01～O-12 全部没有代码；任何公式都只是方案 |
| 第三层：固定布局 ABI | **不可签** | 未开始（O-13） |

**第二层重新评审的条件**：O-01～O-12 中至少「禁止集合编译 + 引导根审计」这一组有代码与测试，本文按新代码重写后，才有可复核的对象。
