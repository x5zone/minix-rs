# 引导期内存政策：今天的保证、未决问题与候选方案

> **重写**：2026-11-06 04:56 CST（`date` 实测）；**内容轮次：第九十九轮评审后的修改**（排除定义拆成贡献与页集合、MRB 单列、`RejectBoot` 与页排除分开；`MemoryKind` 改枚举并统一第一版静态处置；扣减容量证到 47；首次分配补根页与结果侧测试；启动分配页补授权链）。
> **本文件是送审正文，也是唯一的「现行规则」来源**。体积收缩时被移出正文的内容分两处存放，逐字保留、未改写：取证引文、用例穷举、逐轮读数与带行号的定位表在同目录 `03`，按批次从第五十四节递增到第六十八节（正文写「见 `03`」即指这些节；代码坐标底账在六十四节，移交专项的冻结读数在六十五节，第六十六节存 §2/§7 收紧替换下来的段落，第六十七节存本轮三条分配路径与两跳 ABI 的行号读数）。另有两份配套：**硬件准入与用途资格模型**在同目录 `01-design-note-hardware-admission-model.md`（第九十六轮逐字迁出，非本项验收条件）；本方自设又被推翻的判断与过程叙述在 `02` 第七节（标注了撤销理由，不构成合同）。两处与本文冲突时以本文为准。`04-design-backlog-round28.md` 是第二十八轮的冻结快照，不再更新。
> **送审前请对一下本行时间**：低于你正在评审的那一轮即说明材料不是最新。
> **读者**：看不到代码仓库的外部评审人。本文不要求你读代码，但要求你能判断两件事——问题是否真实成立、候选方案是否覆盖了失败形态。
> **怎么读**：§0 是现行模型的名字与它们之间的等式；§2 是代码**今天真实**保证的行为，每条都有测试名；§5 是**尚未裁定**的问题，其中出现的公式与方案一律是**候选**，不是合同。**不要把 §5 的任何一行当成实现依据。**

## 0. 现行模型速览（读 §5 前先拿住这些名字）

本节**不新增规则**，只把 §5 里反复出现的名字与等式集中一处，方便逐条评审时定位。下表里的名字目前**全部是候选合同，一行代码都没有**（代码现状见 §2）。

| 名字 | 它回答的问题 | 展开位置 |
|---|---|---|
| `RamSourceDomain` | 本入口的权威输入是否声称这段是物理主存（尚未应用保留声明与对象生命周期） | §3 第 2 条、O-12f |
| `RamSourcePageDomain` | 上述主张先经兼容性裁决、再并集、再取完整页 | O-12i |
| `PmmSourceExclusionContributions` / `PmmSourceExcludedPages` | 前者是**贡献记录**（`evidence_ref`／`reason`／`raw_range`／`page_projection`／`phase`，MRB 单列、类型政策只取 `ExcludeRange`），后者是这些贡献的页并集——归因与集合分开，页只扣一次 | O-12i |
| `PmmSourceCandidatePages` | **本项的输出边界**：来源域减去上述页集合；它不回答「这台机器允许这次访问吗」 | O-12i |
| `HardwareAdmissionExcluded` | 架构与平台物理访问能力不满足而排除的页（缓存可表达性／内存序／与用途无关的访问准入／Profile 义务）——**专项侧**，本项不实现 | 专项册 |
| `PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx)` | 最终基底层的可用与排除互补且**只对页域成立**；`PmmBaseExcluded = PmmSourceExcludedPages ∪ HardwareAdmissionExcluded` | O-12i |
| `PmmRoleEligible(page, U, ctx)` | 把一页交给用途集合 `U` 的分配资格；用途未知时保守取 `AllRoles` | 专项册（非本项验收） |
| `RoleAdmit` ≡ `RolePageEligible` | 用途特有准入**只有一个展开式**，且它是分支：`PageTable` 走 backing，其余走通用体。三个消费者共用同一个判定核，各喂自己的需求集合——基底 `BaseAccessRequirements`、通用用途 `RoleSpecificRequirements`、页表用途 `PageTableAccessRequirements` | 专项册（非本项验收） |
| `PageTableBackingEligible` | 页表用途的唯一能力合同；新分配页与引导期既有页表共用（既有页表不经过 PMM，因此排序、Profile 区域义务、内存类别与幂等政策都必须自带在 backing 里） | 专项册（非本项验收） |
| `AccessRequirementsEligible`（唯一全称判定核） | `requirements ≠ ∅ ∧ ∀ req ∈ requirements : LegalStarts(page, req, role, ctx) ≠ ∅ ∧ ∀ a ∈ LegalStarts ∀ cpu ∈ ctx.ParticipatingCpuSet : PhysicalAccessPermits(cpu, req, Footprint(a, req))`，其中 `PhysicalAccessPermits = PmpPermits ∧ PmaPermits ∧ PlatformProtectionPermits`；`LegalStarts` 第一版锁**合同 A**（架构起始对齐 ∧ 该用途规定的完整 footprint 约束）。空需求集合与空起始集合都不允许让全称量词真空通过 | 专项册（非本项验收） |
| `VirtualAccessInstance → TranslatedAccessFragments → ActualMemoryOperations` | 对象访问的三层：源级一次访问、完整翻译片段、硬件实际执行的操作——**不得合并** | 专项册（非本项验收） |
| `ObjectPolicyAccessEligible` / `ObjectPhysicalAccessEligible` / `ObjectMappedAccessEligible` | 对象可用性的三面：来源授权、硬件准入、映射路径兑现 | 专项册（非本项验收） |
| `MemoryAdmissionContext` | 一切投影的输入：阶段、参与 CPU 集合身份、平台内存 profile、ISA profile、UEFI 属性位 profile、起始地址合同、证据代次、来源快照集合 | 专项册（非本项验收） |
| `SystemHartSet` / `ParticipatingHartSet` | 前者验证 A/D 更新方案的**全系统**一致性，后者计算普通物理访问准入的**逐 hart 交集**，不得互相替代 | 专项册（非本项验收） |
| `CommonBackingCapabilityEligible` + `PageTableSpecificAccessEligible` | 页表 backing 的两半：与用途无关的公共能力（含**缓存可表达性**）与页表特有的三类访问 | 专项册（非本项验收） |
| `AtomicCapabilityPermits` | 共享层唯一的原子能力谓词：只问「这些 CPU 在该范围上能否满足该算法的族／宽度／原子性／进展要求」。预约集是 RISC-V LR/SC 的承载形式，其几何与证据留在 adapter 侧；共享进展成员叫 `EventualAtomicProgress`，由 RISC-V adapter 兑现为 `RsrvEventual` | 专项册（非本项验收） |
| `SegmentsCorrespondToExpectedTranslation` / `OperationsRealizeAccess` | 对象访问三层之间的对应关系：逐段 VA→PA 对应（不是比物理并集）、操作集合是源级访问的忠实展开（adapter 证明） | 专项册（非本项验收） |
| `NoMapControlledAccessEligible` | 受控映射是必要非充分：谁能经哪条 VA 实际碰这些字节，另立访问侧合同 | O-03 |
| 甲案：授权驱动建表（**已裁定，属分页专项背景，非 I-29 交付**） | 恒等循环的输入换成四类已裁决授权对象（合法普通 RAM、内核映像、启动对象、设备窗口），外加根激活瞬间的 `EarlyExecutionClosure`；具名过渡映射 `BootTransitionMapping` 被否决 | O-02a |

> 上表标注「**专项册（非本项验收）**」的行，展开位置是同目录的 `01-design-note-hardware-admission-model.md`（第九十六轮逐字迁出）。它们描述硬件准入与用途资格，不是内存图交棒的签字条件；本项只保留页域、两层划分与「用途拒绝不等于基底排除」这三条与交接直接相关的事实（见 §5 末节的保留表）。

**共享层与架构 adapter 的分工**贯穿上表：共享层只表达 OS 需要的语义（普通物理访问、页表读取与更新、原子操作的宽度/原子性/进展、执行访问、参与访问的 CPU 集合、证据代次），机制名（PMP/PMA、`Svade`/`Svadu`/`ADUE`、AMO/LR/SC、`RsrvEventual`、MAIR、PAT/MTRR）留在各架构 adapter。本文的 RISC-V 段落因此是 **adapter 侧的展开示例**，不决定三架构共享接口的形状。

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

- **UEFI 启动时 `/memory` 必须被忽略**：RAM 范围由固件内存图决定，设备树不是第二份 RAM 来源。但静态保留区仍要进内存图——`no-map` 对应固件保留类型，其它静态保留对应引导服务数据。范围要说清：**所有 UEFI 路径都以内存图作为固件内存处置的权威描述来源**，而内存图本身不是硬件访问保护（它不建页表、不拦访问）；内核据类型与属性建立来源记录，由后续映射与分配政策兑现。只有提供权威 DTB 的路径才进一步解析 DTB 保留声明并与内存图交叉验证；没有 DTB 来源的路径（x86）不得伪造或推断这份来源身份。
- **MRB 的强度高于普通保留**：规范要求客户端在启动程序给出明确用途前不得访问，不能与「保留但可读」混为一谈。
- **aarch64 UEFI 腿有两份设备树，只有一份与内存政策有关**：RAM 唯一来源是固件内存图；配置表 DTB 是保留政策的权威来源（设计目标，今天尚未接入，见 O-11）；磁盘镜像 DTB **不是内存政策输入**——它的 `/memory` 只用于覆盖对照，且该对照只打印诊断，不改变启动结果、映射、PMM 或任何政策结论。

### 1.3 四个维度不能压成一个

| 维度 | 问的问题 | 谁兑现 | 今天的状态 |
|---|---|---|---|
| 分配资格 | PMM 能不能把它交出去 | 可用表 / 排除集合 | 可用表有，排除集合只有两张表之间的差集，没有独立集合 |
| 映射禁止 | 页表里能不能有它的叶子 | 两棵树各自的禁止集合 | **不存在** |
| 访问义务 | 某个对象在某个阶段实际怎么访问它 | 对象登记表 + 设备义务表 | **不存在** |
| 生命周期 | 什么时候可以释放、什么时候必须保留 | 释放处置 + 阶段模型 | 只有「永不回收」一条隐含规则 |

这四者正交：一页可以「不许分配但可映射」（保留映射区），也可以「可分配但不许映射」——**后者是模型允许表达的状态，第一版还没有这样的对象**；保留该维度是为了不让两种政策耦合，不代表第一版必须使用它。

### 1.4 两棵树：设计意图，不是今天的代码

设计意图是每棵项目页表树各有一条路径：**引导根**（交接前执行，恒等 `VA = PA`）与**最终根**（内核直接映射），每个对象在每棵树、每个阶段都可能有不同要求。今天只有一棵根：引导阶段建立的根在 Step 4 追加直接映射覆盖后继续使用，没有独立最终根，也没有针对它的审计与冻结（§2.3）——所有涉及「两棵树」的合同（R13/R15/R19/R21）今天没有代码消费者。

### 1.5 三架构的差异

| 架构 | 普通主存的兑现方式 | 额外准入 |
|---|---|---|
| x86-64 | 内核自建 `PAT`，普通页显式取写回索引；`MTRR` 只读分析 | **有效类型 = `Resolve(PAT_Entry, MTRR_Type(physical_range))`**，只看 PTE 选择 WB 不足；大页跨不同 MTRR 区间必须拆分或拒绝；每核 `CR0.CD=0`、`CR0.NW=0`；**MTRR 读取结果也进入逐核有效类型一致性证明**，不能假定所有处理器配置天然一致 |
| AArch64 | `MAIR` 两个索引（普通/设备）；第一版锁定 4 KiB translation granule | 两棵树各自遍历实际叶子；每核读回 `MAIR` |
| RISC-V | 按物理区间的 PMA 能力（宽度/对齐、取指、页表读写、原子、排序、一致性）；**能力值来源必须写明并绑定 machine/SoC 型号与版本**：平台静态表 / 固件或 M-mode 信息 / SoC 寄存器 / 已验证来源，不存在通用查询接口；**不得把「未发现限制」当成「支持普通主存访问」** | PMP 是另一层：约束 S 模式取指、数据访问**与页表遍历** |

### 1.6 三个相邻合同：本项只裁决第一个

| 合同 | 它回答的问题 | 谁负责 |
|---|---|---|
| **物理内存图** | 哪些物理范围存在、可用、保留、不可访问，各带什么属性；bootloader 自己的占用是否全部反映 | **本项（I-29）** |
| **平台资源图** | 设备资源在哪、范围多大、来源身份是什么 | 本项只负责**资源发现与描述**（下表），不负责驱动访问点审计与映射政策 |
| **页表与引导页表切换** | 虚拟地址如何映射到物理、用 4 KiB／2 MiB／1 GiB、权限与属性怎么设、根切换怎么过渡 | **BootEntry 与分页侧专项**，不作本项验收条件 |

**平台资源图这一栏必须再切一刀**，否则 O-07 的实际访问完整性会悄悄变成第 7 项阻塞，而 §7 的六项里没有它：

| 内容 | 是否属 I-29 |
|---|---|
| DTB 设备 `reg` 与多级 `ranges` 解析 | **是**（资源发现；今天未开始，见 O-12j-c） |
| ACPI 或平台静态资源描述的来源与身份 | **是**，按第一版支持范围 |
| `DeviceResource` 与平台 profile 的对应关系 | **是** |
| 每个驱动使用点的 VA、宽度、读写方向、参与 CPU | **否**——属设备访问审计（O-12c 已把它与资源描述分开） |
| `DeviceWindowPolicy`（哪些资源允许在哪棵树、哪个阶段以什么属性映射） | **否**——平台映射政策，不是资源发现结果 |
| `DeviceMappingCoverage`／实际 PTE 审计 | **否**——分页专项 |

判据是一句可检验的话：**OS 决定完全不开大页，也不应要求 bootloader 换一种内存图交接方式**。页大小是分页策略，不是物理资源描述的组成部分；因此凡以「某种叶子粒度／某次 PTE 修改／某个硬件访问能力」为条件的条款，都不属于本项的验收集合。

本轮据此撤销此前被列为阻断项的四类要求（叶子粒度与 `split_huge`、PTE 的 A/D 更新机制、页表访问的 PMP/PMA 证明、LR/SC／AMO／预约集等原子能力，以及 `FrozenBootMappingSet` 的页大小冻结）：它们的**内容继续留在正文作为设计上下文**，但**不再是 I-29 的签字条件**，逐段归属见 §7。这不等于说它们不重要——它们各自成立，只是回答的是另一个合同。

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
| conventional | `/memory` 声明，扣除固件前缀与所有保留声明 | **旧式候选投影**——它不等于目标 `PmmSourceCandidatePages`（后者还要纳入来源记录、存活对象与类型／生命周期政策），也不等于 §3 与 O-12i 里那个经过缓存可表达性、内存序、参与 CPU 普通访问、Profile 区域义务与全部排除项证明的 `PmmBaseAvailable(ctx)`；后者今天没有代码消费者 |
| reserved | 固件前缀 + 非 `no-map` 的保留声明（与 `/memory` 相交的部分） | 可达、永不再分配 |

**来源记录层（DTB 侧，第一阶段）**：`collect_with_sources()` 在同样的遍历里为每条声明留一条 `SourceRecord { id, kind, origin, range: PhysicalRange }`。

| 字段 | 合同 |
|---|---|
| `kind` | 六类：`Memory`、`FirmwarePrefix`、`MrbEntry`、`ReservedOrdinary`、`ReservedReusable`、`ReservedNoMap`；`SourceKind::class()` 把它们分成**输入声明**（`Memory`/`MrbEntry`/`Reserved*` 五类）与**派生保留**（`FirmwarePrefix`） |
| `range` | **字段名是中性词**：输入声明的 `range` 就是声明范围；派生前缀的 `range` 是派生结果 `M_i ∩ [0, F)`（`F = firmware_reserve_end`），所以不叫 `raw_*`。记录**不携带页范围**：`/memory` 投影向内取整（`FullPages`），保留声明与 `no-map` 向外取整，MRB 访问禁止保留精确字节。`range.end_exclusive` 是 `u128` 独占端，因此记录层能表达 `2^64` 本身：`physical_end` 只在 `start + len > 2^64` 时报 `RangeOverflow`，恰好等于 `2^64` 是合法顶端边界；拒绝发生在**投影**侧——`declared_page_range` 向外取整后达到 `2^64` 时报 `UnsupportedPhysicalAddressEnd`，`collect()` 构表时用 `projectable_end()` 复查同一条件。`PhysicalRange` 是两条腿共用的类型，因此 UEFI 描述符顶端页将来接入记录层时不会再靠 `u64` 加法偶然放过；但 UEFI 侧的 `SourceRecord` **今天仍未产出**（见 O-12g），这句只说类型合同 |
| `origin` | `MemoryNode { node, reg }`、`ReservedMemoryChild { node, reg }`、`MrbEntry(n)`（`node` 只数匹配节点、`reg` 只数该节点 `reg` 属性内的元组，均从 0 起、按文档顺序）；前缀的 `origin` 是 `FirmwarePrefix { memory_id, cut }`，既指回被切分的 `/memory` 记录，又保留派生参数 `F`（原 DTB 释放后仍能解释「为什么截在这里」） |
| `id` | **输入对象内的局部 ID**；跨输入引用须用 `(InputObjectId, LocalSourceId)`，不能单用 `id` |

容量 40 由**输入上限**推出（`/memory` 段与声明各受 `DTB_MEMMAP_MAX`、`no-map` 同样计数、前缀不多于 `/memory` 段、MRB 受 `DTB_MRB_MAX`），按 **reg 元组**计数而非节点数；超限拒绝（`TooManySourceRecords`）。`collect()` 语义不变、只取前两张表。来源记录层的用例（16 条，含 `dtb_source_records_cover_every_projected_span`、`dtb_collect_and_collect_with_sources_agree`、`dtb_every_input_declaration_has_exactly_one_record`、`dtb_identical_mrb_entries_are_rejected`、`dtb_same_range_with_different_semantics_is_a_conflict`）全清单见 `03`。

解析与拒绝条件：

| 来源 | 解析内容 | 拒绝条件 |
|---|---|---|
| `/memory` | 全树枚举 + `reg()`，支持 unit-address 与多段 | 单表超过 8 段（`TooManyFreeRegions` / `TooManyReservedRegions`）；`address + size` 越过 `2^64`（`RangeOverflow`，畸形）；末端恰为 `2^64`：记录层接受、投影层拒绝（`UnsupportedPhysicalAddressEnd`，见上表 `range` 行） |
| `/reserved-memory` | 路径查找 + 可见性交叉检查；父节点 `#address-cells`/`#size-cells` 必须存在**且与根节点相同**；`ranges` 必须存在且为空；子节点必须有可解析 `reg`；只有 `size` 无 `reg` 是**动态分配请求**；`no-map`/`reusable` 必须是**空属性**且互斥 | 规范依据（DTSpec 3.5 与父节点属性表，逐字原文见 `03`）：父节点属性表把 `#address-cells`/`#size-cells`/`ranges` 三项 Usage 都标为 R=Required 且逐字写「should use the same values as for the root node」——因此**缺失是格式错误**，而「不同/非空」只是 *should*；本项目要求相同与为空属**项目输入子集**，不是规范要求。子节点可用 `reg` 指定静态范围，或用 `size`（可带 `alignment`/`alloc-ranges`）请求动态分配；**`reg` 与 `size` 并存时按 `reg` 静态处理、`size` 与 `alignment`/`alloc-ranges` 一并忽略是规范规定**，不是本方选择。错误分流：`size`-only 动态请求 → `UnsupportedDynamicReservation`（**合法输入的不支持子集**——规范允许该形状，本启动没有可为其选址的动态分配器，静默忽略会把页交给 PMM）；无 `reg` 且无 `size` → `UnsupportedReservedDeclaration`；缺 cells → `MissingReservedMemoryCells`；cells 与根不同 → `MismatchedReservedMemoryCells`（项目子集）；`ranges` 缺失 → `MissingReservedMemoryRanges`（格式错误）；`ranges` 非空 → `UnsupportedReservedMemoryRanges`（输入合法但尚无转换路径）；同时声明 `no-map` 与 `reusable` → `ConflictingReservationAttributes`；标记属性带非空值 → `NonEmptyReservationAttribute`；声明重叠 → `OverlappingReservedRanges`；节点可见却取不到 → `ReservedMemoryUnreachable` |
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

`/reserved-memory` 的重复容忍是**项目级输入策略**，不是规范授权；MRB 没有这层容忍。`reusable` 在去重时保留的是**声明身份**（两条记录各有 `id`），owner 关系尚未建立，不能把记录数量当成 owner 数量——`memory-region` 引用是多对多（一个设备可引用多个区域、一个区域可被多个设备引用），模型应是 `DeviceMemoryRegionReference` 的引用集合而非单个 `OwnerId` 字段。**引用不等于所有权**：只有平台 binding 或驱动注册信息进一步证明回收职责时，才提升为 `ReclaimAuthority`。`/memory` 的**部分重叠**拒绝同样是项目策略，**不是数学必然**——§3 允许纯集合层对兼容区间 `Union`，部分重叠在集合模型里可以安全归一化；拒绝的理由是「严格输入策略：部分重叠视为固件异常」，它不依赖今天 PMM 的实现，也不声称 DTSpec 禁止 `/memory` 重叠。若将来改为并集，来源记录仍须各留一条。

测试：`cargo test -p minix-boot --lib` **146 通过**，其中采集器用例 30 条（全清单见 `03`）；形状分流的四条专名用例钉住三种拒绝形状互不混淆——`dtb_size_only_reservation_is_rejected_as_dynamic`、`dtb_child_without_reg_and_without_size_is_an_unsupported_declaration`、`reg_and_size_together_uses_reg_and_ignores_size`、`dtb_memory_range_ending_at_2_64_is_refused_as_unsupported_end`。
**不保证什么**：不产出禁止集合、不产出 PMM 排除集合、不实现 `reusable` 的 owner 回收、不处理 UEFI 来源。**`collect()` 的两张表无法表达 MRB 来源**：MRB 范围会从 conventional 扣除，但不会进入 reserved（reserved 的语义是「可达、永不再分配」，MRB 是「不得访问」）；调用方若不持有 `SourceRecord`，排除原因与访问禁止来源都不可追溯。这是旧接口的**有意取舍**，修复方向是让政策所有者持有记录，而不是把 MRB 塞进 reserved（那会把「不得访问」误表达成「可达但不分配」）。因此 `dtb_collect_and_collect_with_sources_agree` 只证明两条接口的**投影一致**，不证明旧接口保留来源完整性。
**输入准入（已实现，不是「已知限制」）**：`/memory` 的 `status` 缺失或为 `okay` 才采集，其它值一律拒绝启动（`UnsupportedMemoryNodeStatus`）——节点自己说不可用的内存不得交给 PMM；带 `hotpluggable` 的 `/memory` 拒绝启动（`HotpluggableMemoryUnsupported`），因为内核没有内存移除机制。`/reserved-memory` 子节点的 `status` 不筛选——保留声明即使 `disabled` 也按声明处理（保守方向，只会多留不会误放行）。**仍是项目限制**：`reusable` 不回收，记录保留其类型，回收语义留给后续生命周期层。

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
3. 类型强制处置：`RESERVED`/`UNUSABLE` → 排除；类型 11/12 → 设备路径（`Disposition::DevicePath`，**既不产出区间、不计数，也不记录 `Attribute`**——而该文件的模块头注释写着「它们的 attribute 字被记录为诊断」，**注释与实现不一致**，已登记为待修，不得反过来拿注释当现状）；
4. 独立属性 → 诊断成员：**当前可达的只有 `RUNTIME`**；`NV`（`0x8000`）与 `SP`（`0x4000` 低位 / `0x4_0000` 高位）在当前冲突位拒绝与 profile 门控下 fail-closed，不进入输出表。`PERSISTENT` 是独立的 UEFI 内存类型，不等于 `NV` 属性位，不能用它的测试证明 `NV` 分支可达；
5. 无写回位 → 诊断成员；
6. 写回准入后按类型分派：`CONVENTIONAL` → 可用表；引导服务类 / `ACPI_RECLAIM` → 保留表；其余 → 诊断成员。

测试：`cargo test -p boot-shim --lib` **70 通过**，其中 32 条属本切片；钉住分流与门控的代表用例——`types_that_earn_no_ordinary_mapping_go_to_the_side_list`、`excluded_and_device_types_stay_out_of_every_list`、`the_four_ambiguous_combinations_land_as_adjudicated`、`every_contested_attribute_bit_fails_closed`、`profile_only_attributes_fail_closed_without_a_pinned_profile`、`geometry_is_checked_for_ranges_that_are_emitted`、`rejects_side_list_beyond_its_buffer`（全清单见 `03`）。

### 2.3 引导根与直接映射（`os/kernel/src/lib.rs`、`os/kernel/src/dm_coverage.rs`）

- 建立一棵项目根：恒等低 4 GiB（`os/arch/src/lib.rs` 的 `BOOT_IDENTITY_MAP_END = 0x1_0000_0000`，三架构共用同一个常量）+ 内核高半映射 + aarch64 设备窗口高半别名。
- 恒等循环（`os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable`）的实际形状是 `while addr < IDENTITY_MAP_END`，标志位取 `kernel_read_write() | EXECUTABLE`，只有落在**静态设备窗口表**（`fn identity_device_windows`：aarch64 用 `MMIO_PA_BASE`/`MMIO_WINDOW_LEN`，riscv64 用 `kernel_identity_mmio_regions()`）内的块按 2 MiB 粒度打 `DEVICE`，其余一律按 `kern_huge` 建叶子；`map_huge` 的失败被显式忽略。循环**不查 memmap、不查类型、不查任何禁止集合**——memmap 只在两处被消费：`support.phys_addr_limit() >= IDENTITY_MAP_END` 的断言，以及 boot allocator 候选区的挑选。
- 代码注释写明这是**有意的宽映射**：UEFI loader 代码与数据可能位于非 `CONVENTIONAL` 类型的描述符里，若恒等段不覆盖它们，切页表后立刻缺页。所以「低 4 GiB 全映射」是引导过渡的现实需要，不是漏写的一个过滤器。
- Step 4 的直接映射在**同一棵根上**建立，候选来源是 `可用表 ∪ 自举页表树 ∪ 启动模块 ∪ 保留表` 的并集过滤；它在根激活**之后**才运行（`os/kernel/src/dm_coverage.rs:fn establish_boot_dm` 有四个调用点，全部在 `fn build_bootstrap_root_and_enable` 返回之后），失败一律 `panic`。三架构不是同一种写法：arm64 与 riscv64 只新增叶子、遇已存在槽立即失败（两架构的 `paging.rs:fn map_huge`，arm64 侧注释逐字写着「没有降级路径」），**x86-64 会改写既存叶子**——整槽替换既存叶子、把已存在的 1 GiB 恒等叶子降级成空表再细填、并在既存 PML4[0] 上置 USER 位（`os/arch/src/x86_64/paging.rs`，三处读数见 `03` 六十四节）。
- **没有独立最终根、没有审计遍历、没有禁止集合、没有页表冻结**；今天的启动流程也**没有任何对低 4 GiB 恒等段执行拆分或撤销的调用路径**——通用能力是存在的（trait 的 `unmap`、`unmap_range` 与 `split_huge`（`os/arch/src/arch/paging.rs`，三架构都实现了 `split_huge`）），只是从未在该区间被调用；现有的 `unmap`／`split_huge` 调用点都落在别处：heap 窗口驱逐与 ELF 加载窗口（`fn load_elf_into`），两处读数见 `03` 六十四节。宿主表征测试 `identity_piece_policy_covers_low_4gib_and_marks_device_windows` 断言的是「覆盖到 `IDENTITY_MAP_END`、含 `EXECUTABLE`、不含 `USER_ACCESSIBLE`」。

**由此产生的不是「缺审计」，而是现状与目标合同的直接冲突**。恒等循环**不按来源、类型或禁止政策筛选**，对覆盖范围逐块尝试建立普通 RW+X 叶子；**每次调用成功时，落在该块内的 `no-map`、MRB、`EfiReservedMemoryType`、`UNUSABLE` 范围就一并被覆盖**。由于 `map_huge` 的返回值被丢弃，当前代码既没有证明每个块都必然建立成功，也**没有任何实际 PTE 遍历证据**——宿主测试 `identity_piece_policy_covers_low_4gib_and_marks_device_windows` 证明的是策略形状（循环覆盖到 `IDENTITY_MAP_END`、标志位含 `EXECUTABLE`、不含 `USER_ACCESSIBLE`），不是运行中的页表内容。因此本文的主张是这一条，且它已经足够：**一个不消费禁止政策、失败又不向上传播的宽映射建表路径，不能被认定为满足目标禁止语义**；在完成映射裁决之前，不得声称当前引导根满足这些禁止语义。被比较的审计对象见 O-02a 的 `ObservedMappingAudit`。
**同一棵根上的第二个建表点也必须一起收**：Step 4 的 `establish_boot_dm` 候选来自 `kernel_info.memmap()`（文档逐字写着 **not filtered**：每条 conventional RAM 都算候选，含内核映像与启动模块占用的部分）加上自举页表树与 bump 区。只修恒等循环而不管 Step 4，边界会在追加直接映射时被绕过——因此第一阶段对**实际 PTE** 审计时必须同时覆盖两个建表点。**建表机制本身不在本项**：见 O-02a 的边界登记。该冲突与实施合同的裁决全文见 O-02a（甲案：授权驱动建表，四段依赖链）。

### 2.4 交接与消费

**交棒有两跳，本项的合同要覆盖两跳**：固件与 bootloader → 内核（固定布局），内核 → VM 服务器（`VmBootHandoff`）。

第一跳：两张表经固定布局交接（可用表 128 项、保留表 256 段，另有模块表与标量）。内核 PMM 用可用表；保留表供诊断与查询接口使用。引导区相交守卫使用 `checked_add`，回绕输入直接失败（`test_bootstrap_outside_memmap_rejects_wrapping_region` / `_allocation`）。

第二跳今天**已有实现且有测试**，它正是「bootloader／内核自己占了什么」这份记录的载体（`os/kernel/src/vm_handoff.rs:fn build_vm_handoff`）。扣减记录逐条可枚举，五类范围各有明确来源：内核映像（`KernelInfo.kern_phys_base()/kern_size()`）、VM 自己的根页（`arch_boot` 参数，即交接里的 `root_paddr`）、**启动 bump 区整段**（`os/kernel/src/boot_alloc.rs:fn boot_alloc_region`，已用页与剩余页都算内核保留）、引导期已用帧（`VmBootAllocator` 各池区间从尾部向前的已用后缀，**页表页就在这里**）、保留模块 blob（`KernelInfo.boot_modules()` 的**全部** blob，含 VM 自己的——因为 VM 的段帧在进入前就取自这份空闲表）。消费者侧的不变量写成 `LiveBootstrap ∩ VM-free = ∅`，且是在**消费边界**上核对，而不是相信内核自己的记账；PMM 可用集表达为 `conventional ∩ DM-representable − LiveBootstrap`。容量是固定的（`VM_BOOT_HANDOFF_MAX_DEDUCTED`），超出即当场失败并报「扣减记录不可在容量内枚举」——这是设计行为，不许放宽。测试：`test_classification_deducts_all_record_sources`、`test_classification_free_list_is_exact_complement`、`test_classification_clips_to_dm_window`、`test_classification_drops_out_of_window_region`、`test_boot_image_table_layout`。

**这一跳与本项的关系**：交棒信息必须完整且无歧义，所以五类范围的来源清单、容量核算与失败形状属 I-29 的合同；至于 VM 拿到之后怎么映射、开不开大页，属分页侧。缺口见 O-13（固定布局 ABI 未开始）与 §7。

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

### 2.7 复核方式：本节只留一条会影响重构决策的事实

§2 每条主张的代码位置已写在各自小节标题与正文里，**行号会随代码漂移，一律以函数名为准**；带行号的主张-位置对照表在 `03`。这里只保留一条不读代码就会做错的事实：容器校验 `fdt_layout::validate` 在解析库之前执行，今天共五处调用点——三条 DTB 政策入口（riscv `bootface()`、aarch64 直启 `bootface_a64()`、U-Boot `RiscvUbootBootShim::prepare_boot`）**参与内存政策**；另两处读取的是磁盘镜像里的 `dumpdtb.dtb`（`UefiBootShim::prepare_boot` 的磁盘 DTB 分支与 `load_dumpdtb_free`），它**不进入内存政策**，校验只保护诊断读取路径，**重构时不得把它接进政策链**。

### 2.8 第一次把普通物理页交出去的时刻，以及那之前扣了什么（代码读数）

**先把三个动作分清**：`fn init_proc_and_boot` 里的 `VmBootRegion::select_multi(kernel_info.memmap(), &exclusions)` 只是**选择候选区域**；`VmBootAllocator::new(regions)` 建立 VM 专用的启动分配器；真正把普通物理页交给具体对象，是 `fn load_vm_elf` 从这个分配器取页的那次调用。把「选了一片内存」写成「交付了一页内存」会掩盖问题。

**首次实际分配用的是哪份区域集合——时间线按实例钉死（第九十八轮）**

| 顺序 | 调用点／对象 | 输入区域 | 排除项 | 是否发生物理分配 |
|---|---|---|---|---|
| 1 | `VmBootRegion::select_multi`（`os/kernel/src/lib.rs`，`#[cfg(feature = "mock")]` 分支里的那一处） | — | — | 否，**且真实构建根本不编译这一处** |
| 2 | 同一函数真实分支里的 `select_multi(kernel_info.memmap(), &exclusions[..n_excl])` | **未裁剪的** `kernel_info.memmap()` | 只有内核映像 + 启动模块 | 否 |
| 3 | `VmBootAllocator::new(regions)` | 上一步选出的 regions | 无（分配器自身不做重叠检测） | 否 |
| 4 | `load_vm_elf(module, kernel_info, &mut paging, &mut vm_alloc, &access)` → `load_elf_into` → 第一次 `alloc_page` | 上一步的 allocator 实例 | 同上 | **是——这是第一次真实交付** |
| 5 | `build_vm_handoff(...)` | 读取 allocator 状态做分类 | 五类 `LiveBootstrap` | 否（只读，不再分配） |

**所以「候选集合是否就是首次分配用的那份」这个问题已被回答：是，而且是未裁剪的那份。** bump 池落在 DTB `/memory` 之内、`select_multi` 的排除清单里又只有内核映像与模块 → **候选集合包含 bump 池这一点已证明**；`build_vm_handoff` 在第 5 步才扣减，**对第 4 步没有任何保护作用**。

**仍未证明的是「实际分配页是否真的碰到了 bump 池」**：今天没有任何测试覆盖这个相交（`dm_coverage` 里那几枚「bump inside memmap」测的是直接映射候选过滤，`vm_handoff` 的 disjoint 断言用的是合成输入、且只针对分类后的空闲表）。因此正文结论保持两个层次，不得合并成一句话：

- **已证明**：候选集合与 bump 池相交，且首次分配早于扣减；
- **未证明**：首次分配的实际页与 bump 池相交（需要真实分配结果断言）。

**根页必须单列，不能靠「可能与其它对象重叠」推断安全**：`root_page` 在四条路径上都是**独立分配**——UEFI 腿是单独的 `allocate_pages(LOADER_DATA, 1)`，riscv／aarch64 直启是从 shim 池里单独切一页、与页表 bump 区间不相交。而第一次 allocator 的排除清单里没有它，第二跳的扣减清单却把它单列为一类。所以候选侧要**三枚**测试：

- `BootBumpRange ∩ FirstVmBootAllocatorCandidates = ∅`
- `BootRootPage ∩ FirstVmBootAllocatorCandidates = ∅`
- `FirstVmAllocatedPages ∩ (BootBumpRange ∪ BootRootPage) = ∅`

第三枚验证分配器**实际行为**，不能被前两枚替代。**第一次分配前仍存活的启动对象完整性清单**（候选侧必须逐项核对）：allocator 元数据自身、根页、bump 池（含自举页表页）、内核映像、12 个启动模块、第一跳 arena、DTB 原始对象（最后使用点已结束则明确记为「保护义务已终止」）、仍在使用的启动期临时缓冲。

**「`FREE_MEMMAP` 的扣除能不能保护实际分配路径？」——不能。**** 它不是候选区域的输入；候选来自未经裁剪的 `kernel_info.memmap()`。因此「模块、内核、DTB 都在 `FREE_MEMMAP` 里被扣掉了」这句话对第一次交付的安全性不提供任何证据。这是一条**写而不读**的并行副本（是否存在别的读者，读数见 `03` 六十七节），本身就值得单独立项处理。

**DTB 原始对象在第一次交付时还活着吗？——不活着，我上一轮写错了。** 最后一个读取点是 Phase A.5 的 `minix_platform::init_from_kinfo` → `DeviceTreeDesc::parse(source.phys_addr())` → `Fdt::from_ptr`，解析后即释放借用；`fn dtb_blob_len` 只读 header 取 `totalsize`。所以正确的说法不是「仍存活但没有保护」，而是：**DTB 的字节长度只在 `FREE_MEMMAP` 那条路径上被 `cut_memmap` 隐式向外取整（起点向下、终点向上），没有任何页级排除记录进入实际分配路径**。评审人问的「精确 byte range、向外量化的整页、还是已被更早的页级分配器排除」——答案是第一种加第二种发生在一张没有读者的表里，第三种不存在。

**启动 bump 区会不会重新落回候选范围？——两条腿会，一条腿由断言挡住。**

| 入口 | bump 区来源 | 与 conventional 候选的关系 |
|---|---|---|
| UEFI 两条腿 | `fn alloc_bump_region` 分配 `LOADER_DATA` 后 `init_boot_pt_alloc` | 由 `fn assert_bootstrap_outside_memmap` 三处守卫（bump、根页、arena）相交即 panic → **不相交由 shim 断言保证** |
| riscv64 `bootface` / aarch64 直启 | 从固定 `MODULE_REGION_BASE` 池（注释写着「DRAM+32M 起 32 MiB」）切出 | 该池**落在 DTB `/memory` 之内**，而 `select_multi` 的排除只有内核 + 模块 → **相交按构造成立**，且分配器没有第二道防线 |

`os/kernel/src/dm_coverage.rs` 里那几枚「bump inside memmap → dropped」的测试是**直接映射候选的过滤**，不是分配器排除，不能拿来当保护证据。VM 侧的文本（`os/servers/vm/src/boot.rs` 注释：整个 32 MiB bump 池都在扣减之列）说明风险在**分类之后**被补偿，但第一次真实交付发生在分类之前。

**结论与判据**：本轮据此把 §7 第 4 项写成可验收的两件事——① 一个具名的区间关系测试：`BootBumpRange ∩ VmBootRegionCandidates = ∅`，或在 `select_multi` 的排除清单里显式加入 bump 池与根页；② `FREE_MEMMAP` 与 `kernel_info.memmap()` 谁是权威必须裁决，不得两条路径各自扣各自的。**这不等于已经证明当前运行路径存在内存破坏**：是否真的破坏取决于 bump 池与 VM 段页的实际相交情况，而今天没有测试覆盖它——所以它是**未证明的正确性风险**，不是已确认的 bug。

**分配成功不等于有权写入——启动期新分配页缺一条授权链**（第九十九轮登记，属 BootRoot-3 与 Paging-4 的交界）：`load_elf_into` 拿到 `alloc_page()` 的物理页后，是通过**内核直接映射的 VA**（`phys_to_virt`，加 `KERNEL_DIRECT_MAP_BASE`）写入的，写入前没有任何权限或授权检查；而 O-02a 同时声明 `OrdinaryRamMappingAuthorizations = ∅`。两者并列就意味着：PMM 候选允许分配、allocator 成功交付、但这次写入**没有对应的对象级授权记录**——它依赖直接映射窗口「恰好」覆盖该页。最小闭合顺序应当是「分配 → 登记为启动对象 → 生成范围受限的写授权 → 建立满足该授权的映射 → 写入 → 所有权按 handoff 传递」，其中对象级授权来源是 `BootstrapAllocatedObjectAuthorization { ObjectId, AllocatedPhysicalRange, WriteAccessPath, Phase, Lifetime }`。这是**对象级启动授权，不是重新开放普通 RAM 通用映射**；授权在分配前生成还是分配后追加都可以，但顺序约束必须保证「先访问、后登记」不可能发生。

**未设计（ADR）**：`ConventionalCandidateRanges`（候选，不承诺可立即分配）与 `VmFreeRanges`（扣除全部仍存活对象后的交付集合）之间的转换点没有具名接口；第一次可分配之前对所有仍存活对象的保护也没有。把交接的可用表称作最终可分配集合即是本合同的失败形态。

### 2.9 来源记录今天走到哪一步就停了（代码读数）

**已实现**：`os/libs/minix-boot/src/memmap_source.rs:struct SourceRecord`（`id / kind / origin / range`）与 `fn collect_with_sources`，后者返回两张表加一份 `&'static [SourceRecord]`；记录实际驻留在同一文件的静态存储里。唯一的生产者调用点是 `os/kernel-image/src/bootface.rs` 中 `collect_with_sources()` 之后紧接 `minix_boot::audit_source_coverage(...)`。

**这条链今天到此为止——准确地说，丢的是引用，不是底层字节**：`collect_with_sources()` 返回 `&'static [SourceRecord]`，记录本身体面地驻留在同一文件的静态数组里（容量 40 = `/memory` 8 + 保留 8 + MRB 16 + 派生前缀 8，溢出走 `TooManySourceRecords`，并有 `dtb_source_capacity_is_reachable_at_exactly_forty` 钉住边界）。但 `sources` 这个引用在 `audit_source_coverage()` 之后再未被使用，**没有任何字段或 ABI 把记录交给内核的政策所有者**：静态存储仍在，后续消费者与交接所有权都没有。没有任何结构把它交给内核的政策所有者，`KernelInfo` 也没有承载字段（它只有 `platform_sources: &[PlatformDescSource]`，即「来源类别标签 + 物理地址」，不是来源记录）。其余三条 DTB 路径（`os/boot-shim/src/uefi_helpers.rs` 两处、`os/kernel-image/src/bootface_a64.rs`、`os/boot-shim/src/opensbi_helpers.rs`）调的是 `collect()`，**根本不生成记录**；UEFI 侧也没有描述符级来源记录。

所以「`collect_with_sources()` → `audit_source_coverage()`」证明的是**采集器内部自洽**（记录能覆盖当前投影），不是交棒。今天每条记录只有**输入对象内的局部 `id`**；同时接入 EFI 内存图、配置表 DTB 与其它启动对象之后，必须由 `SourceRef { input_object, local_source }` 才能唯一区分。**第一阶段的目标是让每条记录跨第一跳后仍有唯一身份，不是先把记录做厚**（节点路径、多级 `ranges` 转换链、`reusable` owner 关系都不在本阶段）。本项的核心交付是「来源记录跨过第一跳、由内核的政策消费者持有」，见 O-12g 与 O-13a。

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

**三层各自的说法不得串层**：第一层只有「Not usable」这一个事实，它既不承诺硬件会拦访问，也不等于本项目的 `DenyOsUse`；第二层才是项目政策的具名主张；第三层是「类型本身不兑现保护，映射隔离与访问阻断由内核建立」。两个方向的越层都要防：把第一层写成「硬件禁止访问」是拔高，把第三层「类型本身不兑现保护」反过来读成「所以第一层其实没那么强」是压低——后者尤其危险，因为 `DenyOsUse` 的强度来自第二层政策，不依赖硬件是否拦得住。UEFI 原文在本环境抓取返回 403，「Not usable」措辞与 `GetMemoryMap()` 的两段区分都由**评审人按 UEFI 2.11 核对**，本方独立取证仍是缺口（登记在 §6 的事实分级段），不升级成本方已验证的引用。
**与 `no-map` 的驱动例外组合时，第一版选「绝对禁止」**：`EfiReservedMemoryType` 区域即使同时带 DTB `no-map`，也不开放 `DriverControlledUse`——**理由是 UEFI 的「OS 不应访问」政策与本项目第一版不开放受控例外的合同共同成立**；它不是「UEFI 已给出与 MRB 同强度的硬件禁令」。组合测试（`EfiReservedMemoryType` + `no-map` + 受控使用）列为待补；另一个可选边界是「一般用途禁止、专属驱动例外可用」，但第一版不选它，避免让单一类型名隐式决定组合语义。
**`UNUSABLE` 产生独立的类型禁止项**：第一版至少阻止普通映射与对象访问，不能只靠 `TypeAllocatablePages` 把它挡在 PMM 之外——否则一旦另有对象或设备映射授权，类型本身「不得使用」就缺少负向裁决。它与 `EfiReservedMemoryType` 的 `DenyOsUse` 强度相近但**来源身份必须分开**：前者是「物理内存中检测到错误的区域」，后者是「物理支撑存在但 UEFI 声明 OS 不应访问」。`TypePolicy` 必须能独立生成 `DenyOsUse` 及其派生主张，**不要求存在对应的 DTB `SourceRecord`**（x86_64 没有 DTB，不能因此丢掉 UEFI 自己的禁止语义）。两者都可能导致普通直接映射被拒，但来源语义与例外规则不同，不得合并成一个无差别 `DenyMapping`。同理，`UNUSABLE` 是域内排除（来源身份存在、类型资格排除），不与域外的 `RESERVED` 合并。
**`reusable` 不能落成 `Permanent`**：规范允许 OS 使用，但所属驱动必须能收回——它带所有者关系。候选 A 将来需要表达 `ReclaimableUnder(owner)`；第一版不实现回收，但不能把这个语义从模型里抹掉。**owner 不能从节点层级推导**：DTSpec 用设备节点的 `memory-region` phandle 引用 `/reserved-memory` 子节点，所有者关系必须来自这条引用（或其它权威平台信息），并记录引用身份；`ReservedReusable` 目前只有声明身份，没有可兑现的回收授权。
**第九十六轮裁决：第一版采用 B（静态处置表 + 明确的损失声明），不引入事件驱动的回收框架**。理由不是生命周期不重要，而是**今天没有任何回收消费者**：BootServices 区域第一版不回收、`ACPI_RECLAIM` 在没有 `AcpiEnabled` 事件时不回收、Runtime Services 第一版不提供、`reusable` 第一版不回收。此时搭 `Permanent / Until(event) / ReusableUnder(owner)` 的事件框架，等于为零个消费者维护状态机。A 方案保留为目标合同。**但有一条不许随 B 简化掉**：`ReservedReusable` 必须继续是独立类型——即使第一版不回收，也不能把它并进普通永久保留，否则将来根本不知道哪些区域原本带驱动回收语义。

**判据**：每种 UEFI 类型都有完整阶段处置（`Permanent` / `Until(event)` / `ReclaimableNow` / `Unsupported`，`Permanent` 必须注明是对本项目支持的运行阶段而言）；文档不得把项目策略写成规范语义。
**第一个消费者**：最终直接映射的构造。

### O-02a 恒等映射的授权完整性

**问题**：引导根无差别覆盖低 4 GiB（现状事实与代码位置见 §2.3）。每一页是否都有「允许映射」的来源授权，今天没有证明；更严重的是**已经建立的叶子与目标禁止政策直接冲突**——`no-map`、MRB、`EfiReservedMemoryType`、`UNUSABLE` 只要落在低 4 GiB 内就都已有叶子。这不是「审计还没写」，而是「审计要拒绝的东西已经在页表里」。
**失败形态**：一段没有来源授权的地址被恒等段建立叶子，没有任何集合记录它的存在；驱动或内核任意代码经 `VA = PA` 读到 `no-map` 资源；`UNUSABLE` 页被普通访问；恒等叶子带 `EXECUTABLE`，于是低地址里任何内容都可被取指。
**裁决：采用甲案（授权驱动建表）**，乙案（把宽映射具名为 `BootTransitionMapping`）**否决**。否决不靠保守倾向，而靠两条无法用命名解决的事实：今天没有任何撤销恒等叶子的路径，所以「临时」不成立；覆盖事实不因改名而改变，而 `no-map` 的豁免只能来自它自己允许的受控例外（O-03），不能来自映射的自称用途。第三个事实由本轮补上：普通叶子还带 `EXECUTABLE`，等于把「启动阶段需要少数代码可执行」放大成整个低 4 GiB 可读写可执行。**「必需叶子走授权、普通 RAM 继续走旧宽映射」这种半拉子形态同样被否决**——那等于让两个互相矛盾的建表政策并存，而实际页表不会因为旁边写了「未闭合」就变窄。
**甲案的建表输入是四类已裁决授权对象，不是「只遍历可用表」**——那仍然不够，因为启动确实需要访问不属于 `CONVENTIONAL` 的 loader 代码与其它存活对象，它们可以有**对象级**映射授权，但不能因此把整个固件保留描述符或整个低 4 GiB 映射进来：

| 授权来源 | 建表时如何处理 |
|---|---|
| 合法普通 RAM | 先过来源、阶段、缓存属性与架构准入，再按完整叶子覆盖建映射 |
| 内核映像 | 代码、只读数据、可写数据**分别**登记权限与生命周期，不是一张表 |
| 启动对象 | 按实际存活对象及其访问需求登记：loader 代码、栈、自举页表、启动模块、DTB 原始对象与副本等 |
| 设备窗口 | 独立消费 `DeviceWindowPolicy`，逐页验证授权与设备属性（O-12c） |
**实施顺序是四段依赖链，责任归属以 §7 为唯一口径**（第九十八轮修正：本表此前仍把第 2、3 段写成 I-29，与同一节后文的「五项归专项」及 §7「只交付第一段」三处互相矛盾）。**段名固定为 I29-1／BootRoot-2／BootRoot-3／Paging-4**，避免「第一阶段」这种一词两义的写法：

| 段 | 交付内容 | 责任 |
|---|---|---|
| **I29-1** 来源事实进入交接 | 每条声明的身份（`SourceRef { input_object, local_source }`）、原始范围、原始语义、快照身份；跨来源兼容性裁决；类型／保留／生命周期的原始政策含义 | **I-29** |
| **BootRoot-2** 引导根负向投影 | 把上述事实**编译**成 `BootstrapRootDenyProjection(tree, phase)` | BootEntry／映射政策专项 |
| **BootRoot-3** 必需对象正向授权 | 把对象事实**编译**成 `RequiredObjectAuthorizations`（`ObjectId + PA + VA + R/W/X + Tree + Phase + AccessPath`）与 `BootstrapRootAuthorizationPlan`、`EarlyExecutionClosure` | BootEntry／映射政策专项 |
| **Paging-4** 建表消费与验收 | 两个建表点消费计划、替换旧宽映射、遍历实际 PTE 验收 | 分页专项 |

**I-29 交付对象事实，不交付对象映射授权计划**——这条细分是上表的关键：某个启动对象的身份、占用范围、最后使用阶段、来源是 `EfiLoaderData`／内核映像／启动模块，属本项；把这些事实转换成带 `VA`、`R/W/X`、树与访问路径的授权项，属 BootRoot-3。

I29-1 的最小要求与 BootRoot-2 的输入清单各在其处定义（`SourceRef` 定型与记录进政策见 O-12g；`BootstrapRootDenyProjection` 消费的五类来源见 O-12a），此处不重复展开；两者共同的硬约束是：某种输入没有接入时**必须拒绝该启动形态**，不得假定没有这种政策，且负向投影**不是把四种来源压成一个布尔**。

**两层合同分开写，避免把建表机制带回政策层**：

```
BootstrapRootAuthorizationPlan =
    RequiredObjectAuthorizations
    - BootstrapRootDenyProjection

ObservedMappingAudit(observed) =
    observed ⊆ AuthorizedMapping          ← 实际存在的映射不得超出授权，也不得命中禁止
    ∧ RequiredMapping ⊆ observed          ← 政策要求兑现的义务不得缺失
    ∧ 有精确计划处取 observed = PlannedMapping
```

第一式是 **BootRoot-2／BootRoot-3 的政策交付物**，不是本项的——I29-1 只提供它的来源事实、身份与原始政策含义；第二式是 **Paging-4** 对实际 PTE 的验收谓词。两者都不降低验收标准，而是把「政策决定允许什么」与「实际是否兑现」分成可分别追责的两份合同。集合差只在映射记录之间做：`RequiredObjectMappingAuthorizations − BootstrapRootMappingDenials`；MRB 的**访问**禁止仍由独立访问侧合同处理，若分页侧选择用「完全不映射」兑现它，那是访问禁止的一种保守实现投影，不得把原始 MRB 语义改名为 `DenyMapping`。
**被比较的对象不是 PA 区间，而是带映射关系的记录**，每条至少含 `(VA 区间, PA 区间, 权限, 有效内存属性, 共享性, 树, 阶段, 映射意图)`。只比 PA 并集会漏掉这类真实错误：对象要求经 `VA₁` 到达 `PA₁`，实际页表把 `VA₁` 映射到 `PA₂`，而某别名 `VA₂` 恰好映射到 `PA₁`——两个 PA 并集仍相等，路径却已经错。O-12i 的 `SegmentsCorrespondToExpectedTranslation` 已给出处置方向，本处的简写不得被读成较弱版本；同理设备窗口侧的 `Coverage ⊆ AuthorizedRange`、`RequiredAccess ⊆ Coverage` 只是便于阅读的物理投影，不能取代逐条访问需求的 VA→PA 对应检查。
**`EarlyExecutionClosure` 是必需覆盖集合，不是第四种授权来源**。它回答「根激活瞬间继续执行所需的访问集合是否都有映射覆盖」，不回答「这些物理字节来自哪里、是否允许 OS 映射」。因此它不出现在计划构造式的授权项里，而是出现在 `RequiredMapping` 侧：闭包内的每个对象（代码、`.rodata` 依赖含编译器生成的跳转表、可写状态、当前栈、仍存活的启动对象）仍须各自取得来源授权并通过禁止投影——**「执行所必需」绝不产生穿过 `no-map` 或 `EfiReservedMemoryType` 的许可**。
**普通 RAM 的映射授权必须独立于 PMM 可分配集合**：第一版策略 `OrdinaryRamMappingAuthorizations = ∅`，这是一个有意设计的接口状态，不是临时凑数。后续它必须由独立的映射政策投影产生，**不得复用 `PmmBaseAvailable(ctx)` 或 `conventional` 表**——来源资格、阶段资格、硬件可访问性、映射属性兑现分别检查；一页可以「不许 PMM 分配却因已登记对象获准映射」，也可以「允许分配但当前阶段尚未获准映射」。
**第二消费者不能漏**：§2.3 已写明 Step 4 的 `establish_boot_dm` 在同一棵根上追加直接映射。如果它继续从 `conventional` 或其它候选表生成叶子，第一步建立的边界会在 Step 4 被绕过——所以第一阶段必须同时保证恒等循环不再越权、`establish_boot_dm` 不再引入未授权或命中禁止项的叶子，并对 Step 4 之后的同一棵根做实际叶子审计。
**前置条件：根激活瞬间的执行闭包必须被映射覆盖**——这与 O-12j 的 `EarlyExecutionClosure` 是同一个问题，不另造概念：当前执行代码、继续执行所需的只读数据依赖、必须访问的可写状态、当前栈、以及切换之后仍要使用的页表与启动对象。**只映射当前指令所在页不够**：实证就在本仓库——预扫逻辑的代码页都在已映射范围内，但 `switch` 被编译成 `.rodata` 跳转表 + `jr` 绝对地址，照样触发 instruction access fault（见 O-12j 跳转表段）。编译器生成的跳转表、静态数据与调用路径依赖都属于该闭包。
**建表机制与映射政策都不在本工作项内**。I-29 的交付终点是**交出一份可被消费的来源事实与政策含义**：每条声明的身份与原始范围、跨来源的兼容性裁决结果、类型／保留／生命周期的原始政策含义、可供后续消费者使用的物理范围与来源记录、启动对象的实际存活范围与最后使用点。**下列五项属 BootEntry／分页专项**：`BootstrapRootDenyProjection`、`RequiredObjectAuthorizations`、`BootstrapRootAuthorizationPlan`、`EarlyExecutionClosure`、把授权计划变成实际 PTE 并检查实际映射。因此本项的合同拆成两块读：**O-02a-src（引导来源完整性，I-29）**与 **O-02a-map（引导根映射政策与消费，专项）**；O-12a/b、O-03、O-04 同守这条边界——来源语义与其身份归本项，映射禁止集、访问禁止集与对象访问路径的**投影**归消费者。这不是要删掉 `no-map`、MRB 或 `EfiReservedMemoryType` 的语义：恰恰相反，本项必须把它们准确保留，否则专项没有可靠输入。恒等段与直接映射**怎样**变成 PTE——叶子粒度怎么选、降级要几张中间表页、根激活后分配器自身的地址通道、`split_huge` 在各架构的可用性——属引导分页侧的专项，登记到 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 与 `07-paging-probe.md` 那条线，本轮已从正文撤出（原文逐字见 `02` 第七节）。**这条边界不是回避冲突**：§2.3 与 O-02a 已把「宽恒等映射不消费禁止政策」写成现状冲突，第四段依赖链也保留为政策验收条件；被撤出的只是分页机制的取证细节。
**四页交接 arena 是本阶段最关键的对象，登记字段如下**（它的物理上属于普通 RAM，但授权对象是这个有身份、有范围、有使用者与生命周期的具体对象，不是它所在的整段 RAM——通用 RAM 授权集合为空时，对象级授权的并集仍可非空）：

| 字段 | 今天已核实的事实（代码位置） | 第一阶段还缺什么 |
|---|---|---|
| 身份 | 交接 v1 的 arena，容量常量 `minix_boot::ARENA_CAP = 4 * 4096`（`os/libs/minix-boot/src/handoff_v1.rs`） | 还没有 `ObjectId`，只有裸物理地址常数 |
| 物理范围 | 由 `alloc_arena()` 现分配恰好 `ARENA_CAP` 字节的 `LOADER_DATA`，且被约束在恒等界以下（`os/boot-shim/src/uefi_helpers.rs`），以 `x0`/`rbx` 递进内核 | 范围只在寄存器里传递，未进入任何来源记录 |
| 来源 | UEFI `EfiLoaderData` 类型分配（`ebs_map.rs` 的类型表把 `LOADER_DATA` 判为 `ReservedMapped`） | 没有对应的 `SourceRecord`，x86/aarch64 UEFI 腿记录本就未产出（O-12g） |
| 存活期 | 内核在根激活后立即 `patch_runtime_fields_in` 写入，之后 `arch_boot_resume_v1` 再读取 | 没有「最后一次使用」的登记，也没有释放点 |
| 访问需求 | 写者=内核 patch 阶段；读后续消费者=`arch_boot_resume_v1` | 未按使用点登记，无法证明范围与路径 |
| 映射要求 | 今天靠恒等段的宽叶子顺带覆盖，权限 RW+X | 需要 `VA = PA`、要 `W`、无执行需求时应 `NX`；具体以多大叶子兑现属分页侧专项 |
| 负向检查 | 无任何禁止项参与（§2.3） | 必须过 `BootstrapRootDenyProjection` |

**因此第一阶段的启动形态**是：`KernelImageAuthorizations ∪ EarlyExecutionClosure ∪ LiveStartupObjectAuthorizations ∪ ExplicitDeviceWindowAuthorizations`，再减去禁止投影与不兼容覆盖；其余普通 RAM 不建通用直接映射，`establish_boot_dm` 也只能追加上述已授权范围。若某个启动对象在该边界之前就要用到一块任意普通 RAM，就必须把它逐对象登记进来或调整执行顺序，不得让旧宽映射代替授权。
**建表失败必须分档处理**，不得再用「忽略错误」把「没有建立完整授权覆盖」延迟成之后的不可预测缺页：① 必需叶子（执行闭包、内核映像、启动对象、页表）映射失败 → **拒绝启动**；② 可选映射未建立 → 只有政策明确允许不建立时才继续；③ 其余一律拒绝。代码里已有正确先例：同一个函数中，高半内核映射用的是 `.expect("kernel map: map_huge failed — kernel image mismatch")`，而恒等循环用的是 `let _ =`（`map_huge` 返回 `Result<(), PageTableError>`；错误种类的处置属分页侧专项）。**甲案把循环输入换成已裁决的授权集合之后，「地址没有物理内存支撑」这一类可容忍失败的理由随之消失**，因此忽略错误的写法失去依据。
**判据**：每个实际叶子都能反查到一个授权来源（四类之一），且叶子覆盖的**每一页**都通过来源、权限、内存属性、共享性与禁止项检查——大页覆盖「有授权 + 空洞 + `no-map`」时不能只看起始页，必须拆分叶子或拒绝。授权记录至少含：PA 范围、VA 范围、用途、R/W/X、内存属性、共享性、树与阶段。
**x86 的叶子属性判据**：`EffectiveMemoryType = Resolve(PAT_Entry, MTRR_Type(physical_range))`，不是简单二元运算；大页覆盖 MTRR 类型不同的区间时不能按起始地址判定，必须拆分或拒绝。**所有逻辑处理器的 `IA32_PAT` 必须一致**（或逐核读回验证），不能只查 `CR0.CD/NW`；同一物理页的多个 VA 别名不得产生不兼容的有效内存类型。
**消费者有两个，缺一即边界不成立**：`build_bootstrap_root_and_enable` 的恒等循环，以及同一棵根上追加直接映射的 `establish_boot_dm`（§2.3）。第一阶段对实际 PTE 审计时，两处产出的叶子都要过同一条判据：有授权来源、不命中禁止投影、逐页权限与属性兼容。

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
**`no-map` 要拆成三项，不能简化成一个无条件 `DenyMapping`**：`DenyStandardSystemMapping`（禁止进入标准系统内存映射）、`DenyUncontrolledSpeculativeAccess`（禁止非受控推测访问）、`DriverControlledUse`（专属设备身份 + 专属访问路径的受控使用）。第三项的第一版状态必须写死：`NoMapControlledUse { ModelCapability = ReservedForFuture, FirstEditionPolicy = Reject }`——**第一版没有任何可通过的 `DriverControlledUse` 政策实例**，实现者不得自行开放这条尚未兑现的路径；规范允许的未来扩展保留在模型能力里，不等于当前政策已开放。「有驱动申请」不是通用豁免，不能让 `no-map` 页进入内核直接映射。
**受控映射是必要条件，不是受控访问的充分证明**——访问侧另立一份合同：

```
NoMapControlledAccessEligible(use, device_id) =
    指定设备身份 ∧ 指定对象身份 ∧ 指定访问路径 ∧ 指定阶段
    ∧ 不存在绕过该受控路径的别名或其它访问主体
    ∧ 不违反非受控推测访问限制
    ← 映射侧只回答「这个叶子允许存在吗」；本式回答「谁能经哪条 VA 实际碰这些字节」。
      一个叶子带 DriverControlledMapping 标记，不能自动证明全部访问都由指定驱动控制
```

**MRB 的部分页**：若靠页表兑现 MRB 访问禁止，则该页整体不得出现在普通映射中；同页还有其它对象需要映射时，拒绝冲突或提供独立受控机制。精确 byte-range 检查不能代替页表兑现。
**`no-map` 与 MRB 在页表粒度上的保守投影相似，但原始语义与例外机制不同**：`no-map` 的普通系统映射禁止按相交页向外投影；MRB 是「未经授权不得访问」，两者不能共用一套例外规则。`DriverControlledUse` **不自动解除整页限制**；若专用映射的实际叶子覆盖超出 `no-map` 资源范围，必须证明整叶覆盖区的属性、权限、别名与访问控制均兼容，否则拒绝启动或改用更细页粒度。测试要求：`no-map` 只覆盖 4 KiB 页的部分字节时，普通直接映射不得出现该页，驱动例外也不得未经整页审查放行。

### O-04 对象负向义务的投影

**问题**：`MustNotMap` 与 `MustNotAccess` 不同：前者禁止某棵树在某个阶段建立映射，后者禁止访问路径。今天的禁止集合构造式里没有它们的位置。
**失败形态**：某对象被裁定「在引导根、S2 禁止映射」，但不进任何集合，宽恒等映射照旧覆盖它。
**候选**：

- **A**：两个投影名（映射禁止 / 访问禁止），流式贡献，保留对象、树、阶段、路径四维。
- **B**：第一版规定对象矩阵不出现负向态，并加空集断言。

**DTB 首读与 `no-map` 的先后关系必须裁定（第九十五轮：采用启动输入例外，不采用 ABI 前置保证）**：自引导形态的顺序是「ABI 递入 DTB 指针 → 读 header → 复制完整 DTB → 解析 `/reserved-memory` → 才知道有没有 `no-map`」。若 DTB 自己的物理范围恰好落在它自己声明的 `no-map` 区里，内核在**读出声明之前已经访问了该区域**——单靠 DTB 的内容无法在首读前证明不相交，所以「要求启动程序保证 DTB 不与 `no-map` 相交」这种 ABI 前置保证在第一版拿不到，拿不到就不受支持不是可选方案。裁定：`BootInputReadUse` 是**独立具名的一次性启动读取合同**——严格限定（指定对象、指定字节范围、只读、指定阶段与路径、复制到自有副本即失效），它**不属于标准系统映射，也不等价于 `DriverControlledUse`**，因此与 `FirstEditionPolicy = Reject` 不冲突；随后发现 DTB 与 `no-map` 冲突时拒绝继续启动。三条边界不许含糊：MRB 的启动读取依据启动程序明确授权的用途；`no-map` 的读取依据本项目自己规定的一次性启动读取合同；**两者不得因为都发生在 DTB 读取阶段就共用一个宽泛的 `BootInputProvenance` 例外**。

**`BootInputReadUse` 的准入式与它的证明责任分层（第九十七轮）**：一次性只读本身**不能**证明没有非受控推测访问——DTSpec 对 `no-map` 的要求是「不得进入标准系统映射」且「不得允许非设备驱动控制下的推测访问」，两者都是访问侧条件。所以准入式必须写成五项，且最后一项不许只是布尔标签：

```
BootInputReadEligible(use, source_policy, platform_profile) =
    provenance_valid(use, platform_profile)                 ← 输入对象身份与来源快照成立
    ∧ reachability_valid(use, platform_profile)             ← 端到端可读到这段字节
    ∧ explicit_access_is_bounded(use)                       ← 范围、宽度、方向、失效点写死
    ∧ no_standard_system_mapping_is_created(use)            ← 不因这次读取建立标准映射
    ∧ uncontrolled_speculative_access_is_prevented(use, platform_profile)
```

前四项由本项负责（都是启动输入合同的文字条款）；**第五项是每条启动入口与架构的实际保证**，本项只负责把它列为准入条件并要求具名依据；给出保证的机制（是否依赖地址转换未开启、是否受 PMP/PMA 约束、是否需要在读取前建立某种隔离）属硬件准入，见专项册。证明不出来时的正确动作是**拒绝这种输入布局**，而不是因为「这是启动读取」就默认豁免。

**MRB 一侧的四问必须逐条 ABI 回答**（DTSpec 允许在启动程序给出明确用途信息时访问 MRB，未授权前不得访问）：① 该启动程序是否明确说明递交的地址就是 DTB；② 是否明确授权内核读取该对象的**全部内容**；③ 授权是否只覆盖这一个对象、而不覆盖 MRB 其余字节；④ 读取授权是否在复制到自有副本之后失效。**单纯存在一个地址寄存器，不自动等价于它所指向的所有物理范围都可任意访问**；若启动 ABI 的语义确实是「把这份 DTB 交给内核消费」，这句话要写进 ABI 合同并给出来源（今天四条入口的这条合同仍是假设，见 §6）。

**判据**：R19a 的输入清单与两份禁止集合的构造式逐项对得上；`DenyMapping` 不被 `DenyAccess` 代替。

### O-05 审计时点与「冻结」的含义

**问题**：按字面「T1 前完成最终根审计」不可能（最终根到 S3 才建）；「冻结 PTE」若理解为字节不变，会与硬件 `Accessed`/`Dirty` 更新冲突。
**失败形态**：实现把最终根的空遍历当作已审计；或把硬件置 A/D 判成权限变化。
**候选 A（已裁定，待实现）**：按时点拆开——

| 检查点 | 时点 | 责任 | 归属 |
|---|---|---|---|
| T0 `RootActivationPreflight` | `paging.enable()` **之前** | 初始根的实际叶子、禁止项命中情况、根激活瞬间的执行闭包是否已覆盖 | 引导页表切换合同（BootEntry 侧） |
| T1 `FrozenBootMappingSet` | 引导根上**全部结构修改完成之后** | 最终引导映射集合的覆盖、权限、属性、别名与禁止集合；含设备映射验收（R13-D） | 分页侧专项 |
| T2 `FinalRootPreflight` | 最终根激活之前 | 另一棵树的同类审计 + 三类义务合并目标（R21） | 分页侧专项 |

**这张表替代此前的「T1 在激活引导根前」**：第九十三轮已裁定 `enable()` 不是 T1，时点表若继续写「激活前」就与裁定自相矛盾。拆开之后各点责任不再互相冒充——**T0 不能被 T1 的事后通过替代**，因为 T1 检查的是经过修改后的稳定集合，而 T0 检查的是「激活这一刻必须成立」的条件。T0 有一条落地前必须回答的问题：根尚未激活时如何遍历实际 PTE——若只能审计构造计划而不能检查已写入的 PTE，该阶段的证明强度必须如实降低，不得写成「已观测」。

**本项在这三个检查点里只有一件事**：交出可枚举的物理范围清单（§2.4 的扣减记录 + O-02a 的授权输入）。哪个叶子粒度、A/D 由谁更新、PMP/PMA 是否放行页表访问、临时映射的结构影响范围是否触及冻结字段——全部由 BootEntry 与分页侧专项承接，第九十三轮那张「激活前后修改路径」读数表已随重划移入 `03` 六十五节（逐字保留，作为该专项的输入，不再是本项验收条件）。

**冻结字段本身（VA 覆盖、PA 对应、页大小、有效权限、内存属性、共享性与 `MutableMappingMetadata`）属 T1/T2 的合同内容，随射程重划整体移入 `03` 六十五节**——本项不裁决「哪种修改算破坏冻结」，只保证交接清单里每个物理范围的来源可枚举。

**判据**：每个时点能列出「必须已完成的审计」与「允许的硬件更新」。
**第一个消费者**：根切换代码（今天只有一次根建立，没有切换）。

### O-06 `OrdinaryRamMappingEligibility` 的阶段化定义

**问题**：UEFI 分支不能拿来源域当 backing——它含类型排除、诊断成员与设备类描述符。
**失败形态**：一条 `RESERVED` 描述符被当作「完整 RAM 支撑」，某段保留声明因此通过 backing 检查并被映射成普通内存。
**候选 A（已裁定，待实现）**：每个来源记录回答三个问题——是否具备普通主存属性、当前阶段是否允许普通映射、当前阶段是否允许 PMM 分配。逐类型至少：

`MemoryKind` 是**枚举**而不是布尔：`VolatileRam | PersistentMemory | AcpiReclaim | AcpiNvs | UnacceptedRam | UnusableRam | NonRamPhysicalResource | Unsupported`。它只表达物理性质；「是否属来源域」「类型是否允许普通映射」「类型是否允许普通分配」「生命周期」各是独立列——一列一个语义，不许在单元格里重新耦合。

| 类型 | 属 `RamSourceDomain` | `MemoryKind` | 类型允许普通映射 | 类型允许普通分配 | 规范生命周期 → **第一版处置** |
|---|---|---|---|---|---|
| `CONVENTIONAL` | 是 | `VolatileRam` | 是（**缓存属性另行判定**，不在类型列） | 是（阶段／属性／排除另行判定） | 无事件 → `Permanent` |
| `LOADER_CODE` | 是 | `VolatileRam` | 是（存活期需 X，由对象义务提供） | 否 | `ExitBootServices` → **`Permanent`（第一版不回收）** |
| `LOADER_DATA` | 是 | `VolatileRam` | 是（默认 NX；写需对象义务） | 否 | 同上 |
| `BOOT_SERVICES_CODE/DATA` | 是 | `VolatileRam` | 是（固件活跃期按义务） | 否 | 规范：`ExitBootServices` 后可通用 → **第一版 `Permanent`** |
| `ACPI_RECLAIM` | 是 | `AcpiReclaim` | 是 | 否 | 规范：ACPI 启用后可回收 → **第一版 `Permanent`**（没有 `AcpiEnabled` 消费者） |
| `ACPI_NVS` | 是 | `AcpiNvs` | 否 | 否 | 无 → `Permanent` |
| `PERSISTENT` | 是 | `PersistentMemory` | 否（第一版普通 volatile 策略） | 否 | 无 |
| `RUNTIME_SERVICES_CODE/DATA` | 是 | `VolatileRam` | 否 | 否 | 第一版永久排除 |
| `RESERVED` | 否（采用 B）：仅凭类型名推不出是系统 RAM | 不承诺 | 否 | 否 | 无 |
| `UNUSABLE` | 是 | `UnusableRam` | 否 | 否 | 无 |
| `UNACCEPTED` | 是 | `UnacceptedRam` | 否 | 否 | **`RejectBoot`**（不是页排除） |
| `PAL_CODE` | 不支持 | `Unsupported` | — | — | **`RejectBoot`** |
| 类型 11 / 12 | 否 | `NonRamPhysicalResource` | 否 | 否 | 无 |

**资格是分层的，不是三个布尔值**：`RamSourceDomain`（权威来源是否声明这是物理主存）、`MemoryKind`（普通易失 / 持久 / NVS / 未接受 / 不可用）、`OrdinaryRamMappingEligibility(tree, phase)`、`PmmEligibility(phase)`、`AccessRequirement/AccessAuthorization` 各自回答不同问题，不能互相推导——普通 RAM 可能因 `no-map` 禁止普通映射；设备 MMIO 不是普通 RAM 却有设备访问授权；普通 RAM 可能因 MRB 禁止访问；保留对象可能允许特定对象读却不许 PMM 分配。表头「普通主存性质」只描述 `MemoryKind`，不是「能否使用」的总结论。
**「来源叠加」不是无条件成立（兼容矩阵）**：DTSpec 要求静态 `/reserved-memory` 出现在 UEFI 内存图里——`no-map` 对应 `EfiReservedMemoryType`，其它静态保留对应 `EfiBootServicesData`。所以表里「按兼容矩阵裁决」指的就是下面三种组合：

| UEFI 描述符 | 与之重叠的 DTB 静态保留声明 | 结果 |
|---|---|---|
| `EfiBootServicesData` | 普通 / `reusable` | 兼容（规范给出的预期组合） |
| `EfiReservedMemoryType` | `no-map` | 兼容 |
| `EfiReservedMemoryType` | 普通 / `reusable` | **冲突，拒绝启动**——这不是待裁事项，O-11 权威表一已给结论 |
| 其它类型 | 任意静态保留 | 与预期类型不符：拒绝启动，除非有明确列出的项目扩展规则 |

三张表（本矩阵、O-11 权威表一、O-11 权威表二）里**只有 O-11 权威表一是裁决来源**；本表是它在 UEFI 类型一侧的投影，后续修改必须先改权威表一再同步本表，不得两处各写一套结论。

同一页同时被普通静态保留与 MRB 覆盖时，**两种类型要求冲突、第一版拒绝启动**（见 O-11 的同页裁决表）；不存在「较强类型覆盖较弱类型」的泛化规则，来源记录在拒绝诊断中分别保留。动态 `size` 保留：规范要求它**不出现在** UEFI 内存图（不是「不要求出现」）；`reg` 与 `size` 同时存在时按静态 `reg` 处理，`size` 不决定分配。
**第一版处置与统一模型语义分开**：「属 `RamSourceDomain`」是按统一模型填的，不等于今天的分类器已经支持它继续启动：

| 类型 | 统一模型中的含义 | 第一版处置 |
|---|---|---|
| `UNACCEPTED` | 有物理来源、尚未接受、无普通 backing | **拒绝启动**，不进入阶段化分配 |
| `PAL_CODE` | 固件/处理器专用的代码内存 | 拒绝启动（项目兼容性限制） |
| `PERSISTENT` | 非易失物理内存，有 backing | 不接入普通 volatile PMM 与直接映射，保留来源记录 |

**属性位：语义三类 + 当前处置**（不能只写「未知位拒绝」）。语义分三类：缓存能力（`WB`/`WT`/`WC`/`UC`/`UCE`）；物理保护能力（`WP`/`RP`/`XP`/`RO`）——`WP` 归此类：规范先称它 physical memory protection attribute，再说明它通常被用作 write-protected cacheability policy，所以它有特殊缓存语义，但既不是页表写权限，也不能与 `RO` 混为一谈；其它特征（`MORE_RELIABLE`、`CPU_CRYPTO`、`SP`、`NV`、`RUNTIME`）。当前处置：模型保留 `RUNTIME`/`NV`/`SP` 三种独立属性语义，但**分类器实际可达的只有 `RUNTIME`**——`NV` 属无条件冲突拒绝域，`SP` 的低位编码属冲突拒绝域、高位编码在 profile 未钉住时被 `ProfileAttribute` 拒绝，二者都不进入输出表；`PERSISTENT` 是独立的 UEFI 内存类型，不等于 `NV` 属性位，不能用它的测试证明 `NV` 分支可达。`RO`/`RP`/`XP`/`WP`/`CPU_CRYPTO` 是 **capability 能力位**（`MORE_RELIABLE` 因 `0x1_0000` 的官方文本歧义已改为 fail-closed），第一版只作来源元数据——**不直接等同于当前已生效的访问权限**，也不改变 RAM 来源域成员资格；后续映射策略可用其中的能力信息验证目标属性是否可兑现，**`WP` 不得误解为页表写权限**。**`SP` 不是 no-map**：第一版把它排除出普通 PMM 是项目保守选择，不得由 `SP` 自动生成映射禁令。任何**档外位**仍按第 1 步拒绝启动。
**「规范生命周期」与「第一版处置」必须分两段读**：`BOOT_SERVICES_*` 在规范里 `ExitBootServices` 之后可作一般用途、`ACPI_RECLAIM` 在 ACPI 启用后可回收，这是**规范事实**；而 O-01 已裁定第一版采用静态处置表、没有任何回收消费者，所以**第一版处置一律 `Permanent`**。把两者写成一句「退出前否／退出后是」会被误读成第一版真的会在该事件后回收。

**列名为什么不叫「普通主存性质」**（第九十八轮按评审人的拆分重写）：

| 列 | 它回答的问题 | 反例 |
|---|---|---|
| 属 `RamSourceDomain` | 权威来源是否声称这段是物理主存 | `UNUSABLE` 属域内但类型禁止使用 |
| `MemoryKind` | 易失 RAM／Persistent／NVS／Unaccepted／Unusable 等**物理性质** | `PERSISTENT` 有 backing，但不是第一版普通易失 PMM |
| 类型允许普通映射 | 类型政策本身是否允许把它作为普通内存映射 | `RUNTIME_SERVICES_*` 第一版永久排除 |
| 类型允许普通分配 | 类型政策本身是否允许进普通 PMM | `ACPI_NVS` 第一版不许普通分配 |
| 生命周期 | 哪个事件之后可能改变处置 | `UNACCEPTED` 需要先被接受 |


**`CONVENTIONAL` 是有条件的**：它可以构成 `RamSourceDomain` 与普通主存资格，但 `OrdinaryRamMappingEligibility`（类型层面）**不隐含缓存属性检查**；最终普通映射 = 类型资格 ∧ 缓存属性资格（第一版要求 WB），最终 PMM 分配 = 阶段资格 ∧ 缓存资格 ∧ 全部排除项。无 WB 的描述符不进入普通可分配集合，但**保留其来源身份**。
**术语边界**：这里读的是 `GetMemoryMap()` 的 `EFI_MEMORY_DESCRIPTOR.Attribute`（能力位掩码），与 `EFI Memory Attributes Table` 反映的保护配置不是一回事；第一版不解析后者，也不宣称已获知固件实际施加的全部权限。
**`EfiReservedMemoryType` 在域外但来源身份仍有效**：它不进入 `RamSourceDomain`，因此不参与 `PmmBaseAvailable`/`PmmBaseExcluded` 的域内分割；但与其重叠的 DTB `/reserved-memory`、MRB 原始记录仍然保留，并可独立产生映射禁止与访问禁止。没有独立平台证据时，不把它重新解释为普通主存。
**UEFI Runtime Services 的支持边界（明确声明）**：第一版**不调用 `SetVirtualAddressMap()`、不提供 UEFI Runtime Services**；`RUNTIME` 属性与 `RUNTIME_SERVICES_*` 类型只保留诊断与来源信息，不产生任何普通或运行时映射义务。将来支持时，必须**单独增加**运行时对象与 MMIO 映射义务，不得混入普通直接映射授权。
**属性位编码存在两套官方取值，政策后果如下**（逐位矩阵、版本读数与两组编码对照见 `03` 六十二节）：

- **`0x1_F000` 整个冲突位域无条件拒绝（已实现）**：`WP`/`RP`/`XP`/`NV`/`MORE_RELIABLE` 在 UEFI 2.10 原始版与 2.10 Errata A／2.11 PDF 里位值不同，任一读法都可能把必须排除出 PMM 的内存伪装成元数据，因此在已知位闸之前单独拒绝（`PolicyError::AmbiguousAttribute`，负控 `every_contested_attribute_bit_fails_closed`、`the_ambiguous_bit_fails_closed_under_either_name`）。
- **高位 `RO`/`SP`/`CPU_CRYPTO` 改为 profile 门控（已实现）**：这两组位值在两个正式发布版本族里就不同，内核无法从 `uefi-raw` 常量判断启动固件用哪一版，故 profile 未钉住前直接拒绝（`PolicyError::ProfileAttribute`，测试 `profile_only_attributes_fail_closed_without_a_pinned_profile`）。
- **代价与恢复路径**：`NV` 位因此也落进拒绝域——模型认识 NV 语义，但当前分类器不接受带 NV 位的输入，该分支在真实输入上**不可达**；恢复需两步（先钉带版本证据的固件编码 profile，再把无条件拒绝改成按 profile 解释），只加 profile 记录解不开。`SPECIAL_PURPOSE` 同理不可达。
- **`UefiMemoryAttributeProfile` 的钉法**：必须绑定**实际启动固件镜像身份**（x86 与 aarch64 分别记录，两组固件不必同源），并**同时钉住低位组与高位组的完整映射**；只授权三个高位会得到「部分可识别、其余永久拒绝」的半套 profile。升级 `uefi-raw` 时必须**重核全部已识别属性位**。EDK2/Linux 的具体 commit/tag 尚未钉死（`main` 会漂移），`HOT_PLUGGABLE` 在 `uefi-raw 0.9` 无常量、走未知位闸。

**候选 B（不可接受，列出以说明为何否）**：只看平台物理内存描述、不看类型分类——会把不可用内存当成 backing。
**判据**：每个 backing 页能指回一个「普通主存 = 是」的来源记录，且阶段允许。
**PAT/MTRR、MAIR、PMA、PMP 与逐 CPU 的物理访问准入不在本项**：它们决定「这台机器允许这次访问吗」，属硬件准入，已随 §1.6 的重规划入 `01-design-note-hardware-admission-model.md`。本项只回答「这段范围的来源事实、类型与生命周期是什么」。**名字也要拆开**：`OrdinaryRamMappingEligibility` 把来源分类与映射决定重新绑在一起，本文保留该名字只用于**类型层面**的映射资格，最终映射资格还需叠加缓存属性与硬件准入（专项侧）。

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

端口范围用独立的半开区间与 checked 运算，不做物理页量化、不进 PMM、不进页表、不进 MMIO 记录、不进物理地址跨界 ABI；`PortWindowPolicy` 与端口访问需求都是编译期静态输入。确需跨界时另立 ABI（候选 B，暂不设计）。**判据**：端口与物理地址在任何集合与取整函数里都不混用。**消费者**：端口访问审计。

### O-09 诊断成员与样本的存储登记（已裁定：采用 A）

完整诊断成员表与操作者可见的有界样本表各自登记存储范围、容量、写入者、读取者、最后使用阶段，防止切根后继续引用已失效的引导期缓冲。**判据**：容量清单里每一项都能在对象目录里找到位置与生命周期。**消费者**：容量清单。

### O-10 R13-A 的第二枚负控（已裁定：采用 A）

逐个登记允许出现原始操作的函数，检查函数体内原始操作的数量与位置，新增原始操作必须显式更新审计合同。**两枚负控**：① 在已批准函数体内注入第二条原始操作；② 在**已有 `asm!` 块内**加入第二条 `in`/`out`——审计器必须看展开后的真实操作，不能只数宏调用次数。**判据**：该负控生效。**消费者**：CI 审计器。
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

- **A（两步授权，第九十五轮裁定为「具名一次性启动读取合同」）**：① 对应启动 ABI 提供的 DTB 指针授权对**固定 header 区域**的一次性只读访问（授权主体与传递方式按每条入口分别定义）；② 校验 magic、版本、`totalsize`、回绕与范围合法性后，才授权对 `[dtb, dtb + totalsize)` 的完整读取，并复制到自有缓冲。授权范围与失效点写进启动输入合同。
- **B**：不承认该授权，则必须定义不读取原始地址也能取得 DTB 的其他机制（例如固件提供副本），否则该布局拒绝启动。

**配置表 DTB 存在却没被解析时怎么办（第九十六轮裁定，不留给实现者猜）**：

| 配置表 DTB 状态 | 第一版行为 |
|---|---|
| 不存在 | 允许仅按 UEFI 内存图启动；**不回退**到磁盘镜像 DTB（那不是内存政策输入） |
| 存在且已完整解析并与内存图交叉验证 | 按两份权威来源共同处理 |
| 存在但尚未接入，或解析失败 | **拒绝启动**，不得静默丢弃其中的 `no-map`／MRB 声明 |

理由很具体：今天走的是第三行的情形却被当成第一行处理——只登记地址不解析。若配置文件里带着 `no-map` 或 MRB，而 boot-shim 按「没有设备树来源」的宽松路径继续，aarch64 UEFI 就会以不完整的政策启动。

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
**统一物理范围类型 `PhysicalRange { start: u64, end_exclusive: u128 }`**（`start < 2^64`、`start < end_exclusive ≤ 2^64`）：DTB 的 `address + size` 与 UEFI 描述符都能表达 `[0xffff_ffff_ffff_f000, 2^64)`，统一来源记录不得因 Rust 字段类型丢掉这个合法边界。原始输入字段保留各自编码（DTB 的 address+size、UEFI 的 start+pages），规范化层才转成 `PhysicalRange`；`UnsupportedPhysicalAddressEnd` 只在项目明确选择拒绝该边界时使用，**不能成为 `u64` 加法的副作用**。这条与 O-12h 结构校验、O-12i `FullPages` 是同一条边界合同；DTB 腿已按此实现（分名规则见 §2.1 与 O-12i），UEFI 侧的 `SourceRecord` 尚未接入。
**顶端那一页：集合层能表示，第一版仍拒绝，而拒绝是政策不是算术**。`FullPages` 的页编号范围是 `0 ≤ n < 2^52`，`n = 2^52 − 1` 的独占末端恰好是 `2^64`，所以纯集合层完全能表示最后一个 4 KiB 页；今天 `declared_page_range` 在投影末端到达 `2^64` 时返回 `UnsupportedPhysicalAddressEnd` 是**实现处置**。第一版的支持边界写明为：允许集合层表示该页，能否进入 PMM 由平台物理地址准入决定，而投影侧在拿到逐平台物理地址上界证据之前一律拒绝——理由是「该端点在本项目支持的任何一个 profile 上都没有真实 RAM 或设备窗口落在其上，接受它等于接受一条无法验证的路径」。错误码不得再被读成「`u64` 无法表达合法范围」。
**副本与源的相交检查绑定实际分配结果**：按 PA 判断不重叠之后，仍须在分配器返回副本物理地址后，用**实际分配范围**再作一次相交检查。「分配器通常不会从 ACPI reclaim 区分配」不是证据——分配资格与源对象的生命周期登记是两份独立证据。
**`BootInputReachability` 是端到端可读性，不是「PTE 存在」**：在指定阶段与访问路径下，完整对象的只读访问所需的地址翻译、页表访问、物理访问保护与平台准入条件必须全部成立——RISC-V 上 PMP 可能拒绝 S 模式访问、PMA 还可能限制访问方式；AArch64/x86 各有对应门控。具体由各架构实现验证，通用层只表达「可读」语义，不暴露 PMP/页表遍历等机制名。
**输入合同**：启动程序交接后、复制完成前，DTB 剩余字节不得被并发修改（固件或其它代理都不写）。这条不能证明时必须定义一致性检测或拒绝策略——否则副本可能是不同时刻拼出的混合状态。今天代码没有复制，自然也不做这项检测（见 §2.1 的「就地消费」）。
**UEFI ↔ DTB 交叉验证是覆盖合同，不是重叠检查**：静态 `/reserved-memory` 与 MRB 在 UEFI 启动时须出现在 `GetMemoryMap()` 中，且类型须匹配（`no-map`/MRB → `EfiReservedMemoryType`，普通静态 → `EfiBootServicesData`；MRB 在规范里是 `should`，本项目可提升为强制并写明是项目策略）。验证式：

```
Outward(R_reserved) ⊆ ⋃ { 类型兼容的 EFI 描述符覆盖的页 }
```

**权威表一：逐页 EFI 类型主张裁决**（本表是该主题的唯一裁决来源，O-06 与 O-11 其余各处只引用它、不再另立结论）。输入是 `PageSourceClaims(page) = { (SourceRef, RequiredEfiType, 规范强度) }`，裁决函数 `ResolvedEfiType(page) = ResolveCompatibleClaims(...)`；**「同页」一律指 `Outward(原始范围)` 相交，不是原始字节相交**。

| 该页上的主张组合 | 要求的 EFI 类型 | 结果 |
|---|---|---|
| 普通静态保留（`ReservedOrdinary`） | `EfiBootServicesData`（规范 `must`） | 兼容 |
| `ReservedReusable` | 同上——它是非 `no-map` 的静态保留 | 兼容；生命周期与 owner 关系另行裁决，不改变类型要求 |
| `no-map` | `EfiReservedMemoryType`（规范 `must`） | 兼容 |
| MRB | `EfiReservedMemoryType`（规范 `should`，本项目升为强制） | 兼容（按项目强化） |
| `no-map` + MRB | `EfiReservedMemoryType` | 兼容 |
| 普通 / `reusable` + MRB | BootServicesData(`must`) vs Reserved(项目强化) | **冲突 → 拒绝启动**：一条描述符无法同时满足两者 |
| 普通 / `reusable` + `no-map`（原始字节不相交但同页） | BootServicesData vs Reserved | **冲突 → 拒绝启动**：两条声明各自向外量化到同一 4 KiB 页，原始范围不重叠不豁免页级冲突 |
| 普通静态 + `no-map`（原始字节重叠） | — | DTB 侧已是冲突（同范围异语义），见 §2.1 |
| 仅 EFI DTB 对象 | `EfiACPIReclaimMemory`（UEFI 2.11 §4.6.1.3 对标准 `EFI_DTB_TABLE_GUID` 的专属要求） | 兼容 |
| EFI DTB + MRB（**原始字节相交**） | `EfiACPIReclaimMemory`：DTB 的 `must` 压过 MRB 的 `should` | 允许首读，但必须走具名 `BootInputReadUse`；MRB 的来源身份、访问禁止与 PMM 排除**不因该页取 ACPIReclaim 而消失** |
| EFI DTB + MRB（**只有向外量化后同页**） | 同上 | 首读**不属于 MRB 原始字节访问**；若第一版仍要求授权，必须标成项目额外施加的**页级**启动读取限制，不得说成 MRB 规则 |
| EFI DTB + 普通 / `reusable` 静态保留 | `must` vs `must` | **冲突 → 拒绝启动**：静态保留的类型对应是规范要求，不能被另一条 `must` 静默覆盖 |
| EFI DTB + `no-map` | `must` vs `must` | **冲突 → 拒绝启动**；第一版**不区分原始字节是否真的不相交** |
| EFI DTB + MRB + `no-map` | — | **冲突 → 拒绝启动**：MRB 例外只解决 DTB 与 MRB 的类型冲突，不解除 `no-map` 的类型主张，两条例外不能相加抵消 |
| EFI DTB + 动态 `size` 保留 | 动态保留按规范不入内存图 | 拒绝：`UnsupportedDynamicReservation`（与「DTB 必须在图内」不能同时成立） |
| 其它 UEFI 类型 × 任意静态保留 | 与预期类型不符 | 拒绝启动，除非有明确列出的项目扩展规则 |

**为什么不是「Reserved 更强所以覆盖」**：DTSpec 对普通静态保留指定的是类型本身，不只是「受保护」；用 Reserved 覆盖等于悄悄牺牲一条来源要求。若将来要接受普通保留 + MRB，唯一自洽的做法是**放弃 MRB 的 `should`→must 强化**（回到规范级别，MRB 的访问禁止由独立政策兑现），而不是让 `ResolveCompatibleClaims` 返回一个隐藏牺牲项的结果。跨来源身份照旧各留一条；覆盖公式分别定义 `R_static` 与 `R_MRB`，经页级裁决后再比对。表中的「同页冲突即拒绝」是**保守规则**，它会拒绝掉原始字节其实不相交的合法配置，换来的是不必证明「固件给出的页内字节布局恰好不重叠」——第一版接受这个代价。
**粒度只有两条，不得互相扩张（这是对上表的解释，不是第三份裁决）**：EFI 类型裁决按 **4 KiB 页**向外量化——部分覆盖即构成该页的类型主张，不能因为原始字节只占半页就把同页另一半当成「无主张」；MRB 的访问禁止与受限授权按声明的**原始字节** `[start, start+len)`，**不取整**——不能因交集页取了 `EfiACPIReclaimMemory` 就把字节授权扩成整页可读，也不能因某页类型合规就允许越出原始字节的访问。`no-map` 与普通保留的**分配排除**按页向外，但访问禁止的**强度**仍来自各自来源语义，不因取整而改变。DTB 首读范围本身是原始字节（header 40 B、body `totalsize`），只有**类型验证按页**。页级类型裁决成立**不**意味着该页全部字节可访问；反之亦然。
**交叉验证是覆盖合同，不是重叠检查**（验证式 `Outward(R_reserved) ⊆ ⋃ { 类型兼容的 EFI 描述符覆盖的页 }`）：只证明「有重叠」或「首类正确」不够——一个正确的首页描述符 + 其后的 `EfiConventionalMemory` 会让保留区大部分页未被保护；反向也不成立，一个兼容描述符只覆盖部分不能把整条来源标记为已验证。**必测三种形态（未实现）**：① 仅首页类型正确、后续页类型错误 → 拒绝；② 多个连续同类描述符共同完整覆盖 → 通过；③ 中间一页类型不兼容 → 拒绝。MRB 与普通声明重叠时，交叉验证只核对覆盖与兼容性，政策层保留两个独立来源身份，不得合并成一个更大区间后丢掉语义差异。规范强度照旧分开记录：静态 `/reserved-memory` 入图与类型对应是明确要求；MRB 入图是明确要求，但用 `EfiReservedMemoryType` 的措辞是 `should`（项目可提升为严格合同，不得写成 must）。`reusable` 与 `no-map` 互斥、MRB 之间不得重叠（规范 *shall*）两种强度不因本表改变。
**权威表二：DTB 一次性首读授权**（启动读取是一个具名用途 `BootInputReadUse { ObjectId, PA/VA, 读取范围, 只读, 阶段, 访问路径, 失效点 }`，不是全局正向授权；每个负向来源分别判断是否允许它，`BootInputProvenance` 不能覆盖所有禁止项）。标准 DTB 表有专属规则，不能套用一般配置表允许的四种类型：`RuntimeServicesData`/`BootServicesData`/`ACPINVS` 对标准 DTB 表不成立，`LoaderData` 更不在其中。

| 原始 DTB 所在区域 | 一次性启动读取的裁决 | 授权来源与失效点 |
|---|---|---|
| 普通 RAM | 允许 | 启动 ABI 授权 + 完整范围端到端可达 |
| 普通 `/reserved-memory` | 允许 | 由启动输入对象授权决定；**分配排除仍然保留** |
| MRB | 允许，但走具名受限授权 | MRB 规则自身允许的、启动程序明确用途对应的一次性访问授权（带对象身份、阶段与只读路径）；授权不覆盖所在 MRB 区域的其它访问，也不授权复制完成后继续访问原地址 |
| `no-map` | **首读允许、解析后发现冲突即拒绝继续启动** | DTSpec 的例外是「由使用该区域的设备驱动控制」的访问，**首读本身允许**（走 O-03 具名 `BootInputReadUse`）；此处拒绝的是「把它说成规范定义的驱动受控例外」。两个决定必须分开：**首读是否允许**属启动输入授权，**解析后发现源对象落在 `no-map` 里是否允许启动继续**属解析后的支持政策；除非实现并验证专用受控路径 |
| `EfiReservedMemoryType` | **第一版拒绝** | 配置表里的指针只建立来源身份，不能推翻内存图对该范围的禁止处置；除非有专门且明确的启动 ABI 例外与规范依据 |
| `EfiACPIReclaimMemory` | 允许（唯一合规存放类型） | 整段 DTB 对象所在描述符必须是该类型才允许首读；ACPI 启用前的保留处置不变 |
| `EfiUnusableMemory` / `EfiUnacceptedMemoryType` / MMIO / 端口空间 / `PAL_CODE` | 拒绝，或走各自独立准入规则 | 不得借用普通 DTB 读取通道 |

**类型验证分两个时点**（读完 header 之前并不知道 `totalsize`）：① **首读前**用 ABI 给出的物理起始地址验证 `[dtb, dtb+40)` 覆盖的全部页都落在 `EfiACPIReclaimMemory`，并完成 `BootInputProvenance` 与端到端 `BootInputReachability`；② **header preflight 后**由 snapshot 得到 `totalsize`，验证 `Outward([dtb, dtb+totalsize))` 的**全部页**都由该类型描述符完整覆盖（可多个连续同类型共同覆盖）；覆盖不完整、类型不一致或中途落入其它类型 → 立即拒绝，**不得继续复制 body**。**必测（未实现）**：header 跨两个连续 ACPI reclaim 描述符；header 在 ACPI reclaim 而 body 跨入其它类型（读 body 前失败）；header 跨入不兼容页（读 body 前失败）。
**这两张表的射程**：它们**只**决定「该页的 EFI 类型主张是什么」与「一次性只读能否发生」，**不**修改 `RamSourceDomain`、`PmmBaseAvailable` 或 `OrdinaryRamMappingEligibility`。
**实现状态：未实现**。见 §2.1——今天校验与解析都在固件提供的原始地址上就地执行，没有 preflight / 复制 / 副本校验三段。
**副本生命周期**：原始 DTB 只在复制期间可读；自有副本登记进对象目录；切根前不得回收或覆盖；副本的最终释放必须有明确消费者与阶段。边界条件：header 固定读取长度写明；`totalsize` 有项目上限；`dtb + totalsize` 必须 checked；完整范围必须证明可安全读取（不能只验证起始地址）。
**MRB 与 EFI DTB 的页集合差分**（上表第一行的例外只作用于交集页，不是整条 MRB）：

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
- **O-12i** 每种政策投影的页粒度规则（见下）；其**后半节**（普通访问准入、页表用途、原子能力、对象访问三层展开、整页全称判定）已在第九十六轮迁到 `01-design-note-hardware-admission-model.md`。
- **O-12j** 三层：**a 容器与 MRB（已实现）**——magic、`totalsize`（≤64 MiB）、blob/`off_mem_rsvmap` 8 字节对齐、`struct`/`strings` 块 checked 边界与互不重叠、结构块 4 字节对齐并以 `FDT_END` 结尾、MRB 终止项落在 `min(off_dt_struct, totalsize)`、**MRB 条目上限 16**、FDT 版本 `17/16`（项目子集）、块顺序 MRB → structure → strings（项目输入限制）。**b 结构块预扫（已实现，消费能力已验）**——无分配 token 预扫：token 合法性、单根、嵌套平衡、节点名/属性长度与名称偏移边界、**属性必须先于子节点**、深度 ≤63、`FDT_END` 唯一且最后。**c 目标节点语义（未闭合）**——`/memory`、`/reserved-memory` 与设备 `reg` 的解析与地址转换仍需明确失败形状（含动态 `size` 声明的拒绝形状）。

**O-12j-b 的两条边界必须与解析库实现对齐，不能只按格式规范写**：

- **属性序**：`fdt 0.1.5` 的 `properties()` 在首个非属性 token 处停止，`children()` 与 `all_nodes()` 在「子节点之后的属性」处**直接结束迭代**——这种布局不是「容忍即可」，而是会把子树静默截断成部分树，因此预扫拒绝（`PropertyAfterChild`）。「QEMU/OpenSBI 会产出这种布局」的说法**不成立**：QEMU 8.2.2 `dumpdtb` 全树 0 处违规，当时的观测来自有缺陷的临时扫描器。引用早期提交上的双 PASS 读数时必须带提交与脚本，不能当作当前基线证据（当前 aarch64 shim 腿被既有阻断挡住，详见 `03`）。

**O-12j 状态须按代码状态与合同状态两列读**：

| 子项 | 代码状态 | 合同状态 |
|---|---|---|
| a 容器与 MRB | 已实现 | 已实现；后续输入变化仍需复审 |
| b 结构块预扫 | 已实现 | 部分闭合：跳转表与「解析器可消费」已证；**预编译依赖闭包中是否含跳转表未证**（`-C jump-tables=no` 只作用于本项目编译的 crate） |
| c `/memory` 语义解析 | 已实现（status/hotplug 准入、重复去重、部分重叠拒绝） | 部分闭合 |
| c `/reserved-memory` 静态 `reg` | 已实现（三语义、`ranges` 空、cells 一致、重复/重叠/冲突三分） | 部分闭合 |
| c 动态 `size` / `alignment` / `alloc-ranges` | **已实现**：`size`-only → `UnsupportedDynamicReservation`；无 `reg` 且无 `size` → `UnsupportedReservedDeclaration`；`reg`+`size` → 按 `reg` 处理、`size` 忽略 | 形状分流与规范依据**均已闭合**：三条规定都是 DTSpec 3.5 逐字规定（原文见 `03`），因此 `UnsupportedDynamicReservation` 拒绝的是**规范允许、本项目不支持**的输入，不是非法 DTB；`alignment`/`alloc-ranges` 只在有 `reg` 时被忽略、未单独设错误，因为规范把它们定义为动态分配约束 |
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

**数值必须带证据状态，四种强度不得混在同一列里**（`ObservedInputResource` 本次启动输入实际解析所得 / `ProfileExpectedResource` 由钉住的 `PlatformMemoryProfile` 给出 / `StaticPolicyRange` 项目当前静态政策 / `ExpectedMappingCoverage` 从现有建表代码推导、非 PTE 实测）。按代码读数：**`DtbMemoryMap` 今天不解析设备节点的 `reg`**——只处理 `/memory`、`/reserved-memory` 与 MRB；`DeviceAperture` 这个名字在代码里不存在。因此 PLIC 的 `[0x0C00_0000, 0x0C60_0000)` 是 `StaticPolicyRange`（编译期常量）＋一份**离线** DTB dump 读数（`reg = 0xc000000 + 0x600000`、`interrupts-extended = <0x02 0x0b 0x02 0x09>`，见 `03`），**不是** `ObservedInputResource`；运行时解析只有 `minix-platform` 取到 PLIC 基址且 context 数硬编码为 1，长度未取。设备 `reg` 与多级 `ranges` 转换仍属 O-12j-c 未开始项。

QEMU 侧今天能给的读数：`rewrite-notes/concepts/qemu-environments.md` 记录 8.2.2 / 8.2.10 / 9.2.4 / 10.0.13 四个版本；`os/qemu-tests/run_qemu.sh` 里 aarch64 是 `-machine virt,gic-version=3 -cpu max -smp 4`，riscv64 是 `-machine virt,acpi=off -smp 4`（或 `-machine virt -smp 4 -bios default`），x86_64 只带 `-smp 4`、**没有 `-machine`/`-cpu`**；三条腿都**未记录 accelerator 参数**，DTB 由 QEMU `dumpdtb` 生成而非外部递交。所以「逐 profile 钉住」缺的是运行参数取证，不是模型问题；在它补齐之前，6 MiB 授权与 2.25 MiB 需求都保持候选状态。

**静态表必须作为今天的事实源被清点**。
**四层关系**（不是两层）：

| 概念 | 含义 | PLIC 例子 |
|---|---|---|
| `DeviceAperture` | 平台/设备描述声明的完整地址窗口 | DTB `reg`：`[0x0C00_0000, 0x0C60_0000)`（6 MiB） |
| `DeviceRequiredAccessEnvelope` | 当前内核实际需要访问的寄存器范围 | `[0x0C00_0000, 0x0C24_0000)`（按钉住的配置推导） |
| `DeviceWindowPolicy` | 授权某个范围以指定属性映射的政策 | 本项目静态表 |
| `DeviceMappingCoverage` | 页表实际叶子覆盖的范围 | `[0x0C00_0000, 0x0C40_0000)`（2 MiB 叶子包络） |

约束是：**每个 `DeviceAccessRequirement` 的地址都落在对应 `DeviceMappingCoverage` 内**；**coverage 的每一页都要有兼容的 `DeviceWindowPolicy` 授权**；未使用的 aperture 可以不映射；包络超出所需访问范围的部分仍要逐页通过授权与属性兼容检查。**不写 `DeviceMappingRange ⊇ DeviceResourceRange`**——那会把「整个设备 aperture 必须映射」错当成普遍不变量。
**架构寄存器窗口、设备声明的 aperture 与页对齐映射包络是三件事**，不能因为某个访问点落在 4 KiB 页里就把周围数 MiB 自动变成设备窗口授权。xAPIC 是最清楚的例子：寄存器接口只有 **4 KiB**，而当前政策授权是 `[0xFEE0_0000, 0xFF00_0000)`（**2 MiB**，即 `0xFF00_0000 − 0xFEE0_0000 = 0x20_0000`），IO-APIC 授权同样是 2 MiB。要回答的不是「授权范围太大」，而是**为什么 4 KiB 的实际访问需求要用一个 2 MiB 的设备映射叶子**：只要 profile 能证明整个 2 MiB 区域可按设备属性映射（不覆盖普通 RAM、地址空洞或属性不同的区域），设计就成立；在 QEMU profile、实际 MMIO 资源描述与映射覆盖证据钉住之前，这个授权范围只是**待证平台假设**。第一版若拿不出包络证明，就只映射实际需要的 4 KiB、用更细粒度叶子。

```
ArchitecturalRegisterAperture（架构规定的寄存器窗口，如 xAPIC 4 KiB）≠ PlatformDeviceAperture（平台/固件声明的窗口）≠ PageAlignedMappingEnvelope（实际叶子的对齐包络）
```

三者不能因为某个访问点落在 4 KiB 页里，就把周围数 MiB 自动变成设备窗口授权。

| 窗口 | `DeviceAperture`（设备声明） | `DeviceRequiredAccessEnvelope`（实际访问点） | `DevicePolicyAuthorizedRange`（政策授权） | `DeviceMappingCoverage`（预期；观测待实现） |
|---|---|---|---|---|
| LAPIC（**xAPIC MMIO 分支**） | **未知**（ACPI 未解析） | `[0xFEE0_0000, 0xFEE0_1000)` | `[0xFEE0_0000, 0xFF00_0000)` | 同授权范围 |
| LAPIC（**x2APIC 分支**） | 不适用：x2APIC 用 MSR 访问，MMIO 接口不可用 | **不产生 MMIO 访问需求，也不得伪造包络** | 无 MMIO 授权 | 无 |
| IO-APIC | **未知**（同上） | `[0xFEC0_0000, 0xFEC0_1000)` | `[0xFEC0_0000, 0xFEE0_0000)`（**与 LAPIC 相邻，不是同一个包络**） | 同授权范围 |
| 平台外设窗口 | 32 MiB（QEMU virt profile） | 从已登记的设备访问点派生；**未登记的设备不产生访问需求** | `[0x0800_0000, 0x0A00_0000)`（有意授权整段） | 32 MiB |
| PLIC | `[0x0C00_0000, 0x0C60_0000)`（DTB `reg`，6 MiB） | `[0x0C00_0000, 0x0C24_0000)` | `[0x0C00_0000, 0x0C60_0000)`（**整段 aperture，第九十三轮裁决**） | `[0x0C00_0000, 0x0C40_0000)`（预期覆盖，不等于授权） |
| `RiscvVirtPlatformMmioWindow` | 平台窗口（见下） | UART 控制台寄存器偏移（资源 256 B） | `[0x1000_0000, 0x1020_0000)`（2 MiB 平台窗口） | 同授权范围 |

**2 MiB 包络的授权对象是平台窗口，不是 UART**：QEMU virt 的 `[0x1000_0000, 0x1020_0000)` 同一段里还有 VirtIO MMIO（`0x1000_1000` 起）与 fw_cfg（`0x1010_0000`），所以这条政策登记为 `RiscvVirtPlatformMmioWindow`，UART 只是窗口内的一个资源条目（`DeviceAperture` = 256 B）。**通用规则：大页包络的授权对象必须是产生该包络的完整平台内存窗口，而不是包络里恰好存在的一个设备资源。**
**`RiscvVirtPlatformMmioWindow` 的资源清单**（QEMU virt，按 profile 记录）：UART0 `0x1000_0000`+`0x100`、VirtIO MMIO 8 个窗口自 `0x1000_1000` 起每 4 KiB、UART1 `0x1000_A000`+`0x100`（**仅在使用第二个串口时实例化，属 profile 条件资源**）、fw_cfg `0x1010_0000`+`0x18`。资源存在不等于访问需求：没有 `AccessUseId` 的设备不产生 `DeviceRequiredAccessEnvelope`。
**每条 `DeviceWindowPolicy` 的完整记录**（缺任一字段就只是地址白名单）：允许映射的整页范围、OS 语义属性（普通主存/设备）、`R/W/X`、共享性、适用树、适用阶段、`ProfileId`、证据出处；记录 OS 语义，不写 PAT/MAIR/PBMT 这类架构编码。
**四个数据不是同一件事**：`Aperture` 来自设备描述；`RequiredAccess` 来自已登记的访问点（`reg` 不能证明实际访问）；`PolicyAuthorizedRange` 来自预先审查的平台政策；`Coverage` 来自实际 PTE 遍历。审计公式：`Coverage ⊆ AuthorizedRange` 且 `RequiredAccess ⊆ Coverage`。
**观测记录必须带 VA，不能只有 PA**：`DeviceMappingCoverage` 若只是物理区间，就回答不了「这些页映射到哪个 VA」「访问点用的 VA 是否落在里面」「同一 PA 是否有多个别名、权限/属性/共享性是否兼容」。观测单元定义为 `ObservedDeviceMapping { VaRange, PaRange, LeafSize, Permissions, MemoryAttribute, Shareability, Tree, Phase }`，由它派生纯物理覆盖 `ObservedPaCoverage = Union(PaRange)`，审计仍写 `ObservedPaCoverage ⊆ DevicePolicyAuthorizedPaRange`；每条 `DeviceAccessUse` 要能证明其访问 VA 经对应叶子解析到预期 PA，且宽度、读写与属性兼容。这与 O-05 的别名审计是同一条要求——不能在要求审计别名的同时让覆盖集合丢掉 VA。
**AArch64 的 64 KiB 分组属性规则：按物理地址分组、只比分组属性、限定 UEFI profile**：UEFI 规定若一个 64 KiB 物理区间含任一 4 KiB 子页属于 `RuntimeServicesCode`/`RuntimeServicesData`/`ReservedMemoryType`/`ACPIMemoryNVS`，则该区间内全部 4 KiB 子页必须采用相同的 ARM memory page attributes。规范位置 UEFI 2.11 §2.3.6，评审人已按该节核对：引用 Table 2.5，权限映射在 Table 2.6；本方抓取 uefi.org 返回 403，**未直接取得原文**，两侧分开记。文本里的 `EfiReserved` 与本项目的 `EfiReservedMemoryType` 是同一类型的两种写法。四条精度约束：

- **分组键是物理地址**：`AttrGroup(PA) = floor(PA / 65536) * 65536`。同一 VA 在不同树里的别名不改变分组；按 VA 分组会漏掉跨别名违例。
- **比较字段只限分组属性**：memory type（Normal/Device 及其子型）、inner/outer cacheability、shareability。`R/W/X`、`PXN`/`UXN`、`AF` 等**访问权限逐页独立**（Table 2.6 一支），不进一致性比较——把权限塞进分组会禁掉合法的「同组不同权限」拆分，反向也会放过属性不一致。
- **比较对象是叶子最终生效的属性，不是 `AttrIndx` 数字**：写成 `EffectiveArmMemoryAttributes = (ResolvedMAIRAttrByte, Shareability)`。两个不同 `AttrIndx` 若解析出完全相同的有效属性，不因索引编号不同而判冲突；反过来，同一 `AttrIndx` 在不同 PE 上对应不同 MAIR 内容**不算通过**（只读回索引相等证明不了任何事）。
- **适用性按 profile 判定**：这条约束随 AArch64 **UEFI 启动 profile** 的属性递交而来；非 UEFI 入口（`-kernel` + DTB）没有 EFI memory map 属性递交，规则不自动适用，而架构侧是否也按 64 KiB 粒度取属性属另一项取证，未取到之前记为 profile 条件而非普适事实。第一版锁定 4 KiB granule **不等于**规则不适用。
- **未映射子页要有组级覆盖登记**：一致性检查的对象是同一 `AttrGroup` 内**已建立叶子**的属性集合，组内其余子页尚未映射不构成违例，但审计须记 `GroupCoverage`。任一后续新映射（低 4 GiB 宽恒等映射拆分、改属性尤其）都受既有分组属性约束；若某子页必须取不同的分组属性，只能整组改属性或拒绝，不能局部改。

它与 ACPI reclaim/NVS 描述符自身的 4 KiB 对齐约束是两条独立规则，不能混用。
**三个完整性主张强度不同，不能互相代替**：`Coverage ⊆ AuthorizedRange`（已映射区域未越权）、`RegisteredRequiredAccess ⊆ Coverage`（已登记需求可映射）、`ActualDeviceAccess ⊆ RegisteredRequiredAccess`（**未证明**：没有登记的设备访问是否存在，当前无任何证据）。最终报告必须分别陈述，不能拿前两项冒充第三项。

**数值相等 ≠ 来源相等**：表中若干行的授权范围与覆盖范围数值相同，但授权是**独立政策输入**，覆盖是**独立测量**；不能由实际叶子反推授权。PLIC 是显式例外：它的授权（6 MiB）比预期覆盖（4 MiB）**更宽**，两者不得再写成「同授权范围」。而且今天**没有页表审计遍历**，所以今天的覆盖只能叫 `ExpectedDeviceMappingCoverage`（从构造代码推导），`ObservedDeviceMappingCoverage`（PTE 遍历）等 O-02a 的引导根审计落地后才存在。表中「覆盖」列在观测实现前一律标为**预期**。

**x2APIC 是第三种访问域**：xAPIC 走 MMIO（`0xFEE0_0000` 页），x2APIC 走架构寄存器（RDMSR/WRMSR）——既不属于 MMIO 窗口模型，也不属于 O-08 的端口 I/O 地址域。**MSR 编号不进共享层**：共享层只表达 `ApicRegisterAccessRequirement`（APIC 逻辑寄存器、读写方向、访问者、阶段、CPU 范围、适用模式），x86 架构层用 `X2ApicMsrUse` 记录具体 MSR 编号、指令位置与包装函数。同一 OS 语义（读 Local APIC ID、写 EOI）在 xAPIC 映射到 MMIO、在 x2APIC 映射到 MSR。**第一版若不实现 x2APIC 访问路径**，必须锁定 xAPIC profile 并在检测到 x2APIC 时拒绝启动，不得「允许 x2APIC 模式却保留 xAPIC 的 MMIO 访问需求」。O-10 的 in/out 负控不覆盖 RDMSR/WRMSR：**架构寄存器访问必须进入独立的原始操作审计**——允许出现原始 MSR 访问的函数与指令位置清单、MSR 编号与共享层逻辑寄存器的对应关系、在已批准的 `asm!` 块中加第二条 `rdmsr`/`wrmsr` 必须使审计失败、x2APIC 模式下不得残留 xAPIC LAPIC MMIO 访问义务。

**x86 的 APIC 地址不是无条件硬件事实，而且核对是逐逻辑处理器的**：表中 `0xFEE0_0000`/`0xFEC0_0000` 只属于「本项目支持的某个 x86 QEMU profile」。`IA32_APIC_BASE` 是**每处理器 MSR**，xAPIC/x2APIC 模式也按处理器启用，因此不能只在 BSP 上核对一次（这条按「评审人已核的架构事实」使用，本方未取 Intel SDM 原文，登记为待核）。启动与次级处理器入场时，对**每个参与内存访问与调度的逻辑处理器**分别核对：当前模式（xAPIC MMIO 还是 x2APIC MSR）、该处理器的 `IA32_APIC_BASE` 基址与政策表一致、其 LAPIC 寄存器窗口是否已按对应模式获得映射或 MSR 路径准入——一个 AP 的基址不同，「政策授权一个 4 KiB 页」这条主张就不再对所有参与者成立。若解析 ACPI，MADT 的 Local APIC 地址、每个处理器条目的 ID 与 enabled flags、override 与 I/O APIC 项都要与政策表兼容；任一参与处理器不符合当前 profile 就**拒绝启动**或**阻止该处理器入场**。riscv 侧同形态：PLIC 的 context 按 hart 列出（`interrupts-extended`），逐 hart 核对，不能由 hart 总数乘 2 推出来。

**PLIC 的访问集合与包络**：`DeviceAccessUseSet` 的每条记录至少含 `AccessUseId`、`DeviceId`、`ObjectId`、**`VirtualByteRange`（或其可静态解析出实际 VA 的访问点标识）**、`PhysicalByteRange`（预期 PA）、访问宽度、读/写、阶段与路径。**缺 VA 就无法证明访问需求**：指针实际落在哪个别名上只有 VA 能回答；不能先由现有 PTE 推出 PA 再充作访问需求输入（那是循环证明）。`DeviceRequiredAccessEnvelope = Envelope(DeviceAccessUseSet)`，**不表示包络内每个 byte 都被访问**；没有完整访问清单时包络只是保守上界，不能宣称访问需求已完整（与 O-07 相连）。

**静态窗口必须绑定一个 `PlatformMemoryProfile`**：每个 profile 至少含 `ProfileId`、QEMU 构建版本、machine type/version、accelerator、CPU 型号、SMP 拓扑、窗口政策集合、预期资源集合与不支持的配置。表格只引用 profile ID；x86 APIC、aarch64 32 MiB、riscv PLIC/UART 四类窗口都适用——今天四者的 QEMU 构建版本都**尚未记录**，属于待补证据。

**aarch64 的 32 MiB 是 `DeviceWindowPolicy` 窗口，不是一个设备的资源**：它表示钉住的 QEMU virt profile 对该整段授予 `DEVICE` 映射资格；其中哪些设备、哪些空隙、哪些区段当前没有访问点，由资源清单与访问需求另行解释，不能由设备名称反推。

**`PLIC_RESOURCE_LEN` 里的 2 倍关系属于 TCG 形状，不是通用资源长度**（因此 2.25 MiB 至今是候选值，不得称「内核实际所需资源长度」，除非该 profile 的 context 编号布局被证明）：代码常量写成 `0x0020_0000 + 2 * PLIC_MAX_HARTS * 0x1000`（`os/arch/src/lib.rs`），配套注释逐字说的是「M-mode 与 S-mode context of every supported hart」，claim 寄存器按 `0x20_0000 + (2N+1)*0x1000` 定位——奇数 context ID 正是 QEMU TCG 路径的形状，KVM 用连续编号。所以通用合同只有一条：`RequiredEnd = ContextBase + (MaxActualContextID + 1) × Stride`，`2 × hart_count` 只能作为**具名 TCG profile 的推导结果**出现在该 profile 的记录里。另两点要分清：`PLIC_MAX_HARTS = 32` 是本项目 SMP 上限（注释要求「至少与内核 SMP 上限一致」），不是 QEMU 的最大 CPU 数；上游 QEMU virt 的 PLIC 窗口按最大 CPU 配置可达 6 MiB 这一条按「评审人已核」使用，**本方未取 QEMU 源码**。窗口长度 `PLIC_WINDOW_LEN` 是资源范围按 2 MiB 向上取整的结果并有 `PLIC_WINDOW_LEN <= 0x60_0000` 的编译期断言——它落在 aperture 内，但 aperture 本身不因它而被授权。

**PLIC 范围的推导与 profile 条件**：`N_context = hart_count × 2` 不是通用公式——QEMU TCG 用奇数 context ID（`2·cpu+1`），KVM 用连续编号，不支持 S-mode 的 hart 还会造成 context 数差异。所需范围写成 `ContextBase + (max(实际使用的 context ID)+1) × Stride`；context ID 不能由 hart 数直接乘 2，也不能把 `interrupts-extended` 的 interrupt specifier 当成 PLIC 的 MMIO `addrid`。内核实际需要的资源长度是 `PLIC_RESOURCE_LEN = 0x0020_0000 + 2 × PLIC_MAX_HARTS × 0x1000`（32 hart 时 `0x24_0000`）。

**本项「平台授权范围」的数值来源已裁决（第九十三轮）：否决按叶子粒度反推的 4 MiB 授权**。代码注释逐字写着 Step 1 按 2 MiB 出设备叶子，所以授权范围取资源范围的 2 MiB 包络，`PLIC_WINDOW_LEN = (PLIC_RESOURCE_LEN + 0x1F_FFFF) & !0x1F_FFFF`（并有 `is_multiple_of(0x20_0000)` 的编译期断言，`os/arch/src/lib.rs`）。先看到 2 MiB 叶子、再把所需范围向上取整成 4 MiB，这种生成方式不属于授权政策，正是本文自己确立的 `DeviceWindowPolicy` 与 `DeviceMappingCoverage` 分离原则所禁止的推导。第一版 PLIC 改按**完整 aperture 授权**：

| 层次 | 第一版裁决（钉住的 riscv QEMU virt profile） | 依据状态 |
|---|---|---|
| `DeviceAperture` | `[0x0C00_0000, 0x0C60_0000)`，6 MiB | DTB `reg` 的资源位置描述，不是映射许可 |
| `DeviceRequiredAccessEnvelope` | `[0x0C00_0000, 0x0C24_0000)`，2.25 MiB | 仍须按实际 context 证据确认（见上条推导规则） |
| `DevicePolicyAuthorizedRange` | `[0x0C00_0000, 0x0C60_0000)`，6 MiB | **须由 `PlatformMemoryProfile` 独立证明**，不是 `reg` 自动给的许可 |
| `DeviceMappingCoverage` | `[0x0C00_0000, 0x0C40_0000)`，4 MiB | **预期覆盖**，等实际 PTE 观测；它是 2 MiB 粒度的产物，不是授权 |

三条包含式必须同时成立：`DeviceRequiredAccess ⊆ DeviceMappingCoverage ⊆ DevicePolicyAuthorizedRange`，且 `DevicePolicyAuthorizedRange ⊆ ProfileVerifiedDeviceAperture`。**`DevicePolicyAuthorizedRange` 不得由 `PageSize`／`LeafSize` 推导**：换成 4 KiB 或别的粒度，授权范围不随之变化——这就是它与建表粒度脱钩的检验。两个边界：① 6 MiB 不因 DTB 写了 `reg` 就自动获得映射许可，必须由 profile 整段确认这些页都属 PLIC 设备窗口、每页都能以目标设备属性映射、不与 RAM／地址空洞／其它不兼容区冲突；**证据不成立就拒绝该 profile**，不得为了保住当前 2 MiB 建表方案强行放行（届时要么分页侧改用更细粒度，要么平台侧拿出独立子窗口证据）。② 不把「授权范围必须等于 aperture」提升为所有设备的普遍定律——普遍规则只有上面那三条包含式与「不得由叶子粒度推导」；若有独立平台证据证明某设备只需较小子窗口，可以只授权该子窗口，但**不能用 `AlignOut(RequiredAccess, LeafSize)` 代替那份证据**。本项 PLIC 数值取**6 MiB 授权、4 MiB 预期覆盖**。这项裁决不替代 PLIC context 的完整性证明：`max(实际 context ID)`、hart 与 context 的对应关系、QEMU 构建版本与 machine version 仍需钉住，`2 × hart_count` 不得回到通用公式。


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
  → RebuildSourceRecords → RevalidateBootInputObjectsAgainstFinalMap → RebuildPolicyProjections → Audit
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
| 顶端边界 | **结构合法性与平台准入分开（已裁定，未实现）**：`[0xffff_ffff_ffff_f000, 2^64)` 是合法的最后一页，UEFI 只约束「页起始地址不超过 `0xffff_ffff_ffff_f000`」，不要求独占末端能用 `u64` 表示。结构校验用 `u128` 接受 `end == 2^64`；是否可用由平台地址准入层判定（`effective_pa_limit`、PMA/PMP 等），拒绝时用独立错误码。DTB 腿的三层落定见 §2.1 与 O-12i，UEFI 描述符路径本就接受 `end == 2^64`；测试 `physical_range_expresses_the_top_page` 与四个边界用例 |
| 版本策略 | 只接受 `DescriptorVersion = 1`；其它版本明确拒绝（不是「受支持」的模糊说法） |
| 顶端页测试（待补） | `PhysicalStart = 0xffff_ffff_ffff_f000, Pages = 1` 通过；同起点 `Pages = 2` 拒绝；`VirtualStart` 同两点；`VirtualStart = 0` 允许但页数仍须满足虚拟上界；**以上对不进入任何分类输出的类型同样执行** |

#### O-12g 的附加要求

原始记录保留：声明所在节点、**每一级 `ranges` 转换的输入/输出与引用节点**、最终 CPU 物理地址、属性与来源身份——只保留最终地址会让审计无法解释地址是怎么来的。
**四条补充要求**：① **排除原因多重归属**——同一页可同时被普通保留、MRB、存活对象覆盖，PMM 只扣一次，但审计能列出全部 `SourceId`；② 固件前缀、内核镜像、boot shim、自举页表、栈、启动模块、DTB 原始对象与副本、登记表**不能合并成一个 Reserved 标记**，它们各自有阶段化读写/执行需求；③ **重叠规则**：**同一权威来源内部的同语义重复**集合去重、保留多个 `SourceId`；**跨来源证据**（UEFI 类型、平台分类、固件保留与 RAM 来源）对同一物理区域提出**不兼容主张**时进入冲突裁决，不能靠并集掩盖——不兼容即拒绝启动。`/memory` 本身只有 RAM 语义（`status` 非 `okay` 与 `hotpluggable` 已被准入拒绝），不存在「同源一条说 RAM、一条说不可用」的形态；④ **原始记录必须保留不输出区间的描述符**（含 `RESERVED`/`UNUSABLE`）——否则 O-06 的「`UNUSABLE` 属于来源域」没有可实现的输入；诊断成员表是面向内核的另一层，不能替代原始记录。

**状态按六层记（O-12g，第九十六轮按代码读数重述——审计自洽不等于交棒）**：

| 层 | 现状判断 |
|---|---|
| DTB 采集器能生成带身份的记录 | **已实现**（`fn collect_with_sources`） |
| RISC-V 自引导能用记录做覆盖审计 | **已实现**（`audit_source_coverage`），但 `sources` 在审计后再未被使用，函数返回即丢弃（§2.9） |
| 四条 DTB 路径均生成并保留记录 | **未实现**（其余三条走 `collect()`，不生成记录） |
| UEFI 描述符全部生成带身份的记录 | **未实现**（连 `Attribute` 也未记录，且注释与实现矛盾，见 §2.2） |
| 来源记录进入跨第一跳的 ABI | **未证明**（`KernelInfo` 无承载字段，只有 `platform_sources` 的类别标签＋地址） |
| 内核政策消费者实际消费来源记录 | **未实现** |

**容量上限必须按「解析器接受数」而不是「缓冲物理容纳量」算（第九十八轮更正）**：候选缓冲的容量由 `fn probe_map_buffer_size` 给出——`max(初始需求, 16 KiB) + max(DescriptorSize, 64) × 8 + PAGE_SIZE`；在最小 `DescriptorSize = 40`、初始需求 ≤ 16 KiB 时是 `16384 + 512 + 4096 = 20992` 字节，**能装 524 条 40 字节描述符**。但解析器只接受 256 条（`MapStorage.descs: [DescriptorRef; 256]`），所以真正的接受上限是：

```
ActualDescriptorCount    = MemoryMapSize / DescriptorSize
MaxAcceptedDescriptorCount = min(MAX_MAP_DESCRIPTORS, floor(MapBufferCapacity / MinimumSupportedDescriptorSize))
                           = 256（当前配置）
```

正文此前那句「来源记录数组容量必须 ≥ 最终 Map 缓冲可容纳的最大描述符数」若按字面执行会要求 524 槽，而 257 条就已经被拒绝——**这两个上限不是同一个数，必须分开写**。另外「容量不足必须在最终取图前发现」也不成立：真实条数只能在最终 `GetMemoryMap()` 返回后才知道。正确的两步是：**取图前预分配足够大的固定容量**（按 `MaxAcceptedDescriptorCount` 算）；**取图后若 `ActualDescriptorCount > 256` 立即以具名错误拒绝**，不得截断后继续启动。

**总容量公式**（不能各模块各自声明后依赖偶然的数组大小）：

```
TotalSourceCapacity = UefiDescriptorSourceCapacity   ← ≥ 256
                    + DtbSourceCapacity              ← 40（8 + 8 + 16 + 8，已有测试钉住）
                    + DerivedSourceCapacity          ← 固件前缀等派生记录
                    + StartupObjectCapacity          ← 内核映像、模块、bump、自举页表、arena
                    + PlatformResourceCapacity       ← 配置表 DTB、其自有副本、资源描述
```

五项各自的输入上限与超限行为都要写明；派生记录与启动对象记录今天**没有独立容量**（与主记录共用），是未闭合项。

**容量闭合：五个问题按代码读数回答（不再只留「固定容量工作区」一句话）**

| 问 | 代码事实 |
|---|---|
| Map 缓冲实际能容纳多少描述符 | **不是固定常数**：`fn probe_map_buffer_size` 取 `size.max(16 KiB) + margin`，`margin = desc_size.max(64) × 8 + PAGE_SIZE`，再 `allocate_pool(LOADER_DATA, bytes)` |
| 最小接受的 `DescriptorSize` | `MEMORY_DESCRIPTOR_SIZE = 40`；更小 → `DescriptorTooSmall`；不是 8 的倍数 → `DescriptorSizeNotAligned` |
| 256 是拒绝上限还是缓冲推导 | **是解析器的存储上限**：`MapStorage.descs: [DescriptorRef; 256]`，`entries > 256` 直接 `TooManyDescriptors`；与缓冲大小无关 |
| 来源记录数组是否 ≥ 256 槽 | **不存在**：`os/boot-shim/src` 里没有任何 per-descriptor 来源记录结构（连 `SourceRecord` 的引用都没有） |
| 派生记录与启动对象记录容量 | 与主记录共用同一容量，未单独开列 |

`MaxDescriptorCount = floor(MapBufferCapacity / MinimumSupportedDescriptorSize)` 这条可计算关系仍是**未实现**要求；但「stale-key 重试后描述符数超上限必须整体拒绝、不得截断」这一条**已实现**：超限走 `TooManyDescriptors`，shim 侧 `panic!("boot-shim: memory map rejected: …")`，是整体拒绝而非截断。**待补测试两枚**：恰好 256 条通过、257 条以具名错误拒绝（DTB 侧已有对等测试 `dtb_source_capacity_is_reachable_at_exactly_forty`，上限 `DTB_SOURCE_MAX = 8 + 8 + 16 + 8 = 40`）。

**arena 载荷预算（现有 v1 真实数字，不是待算）**：`WireMemoryRegion` 16 B、`WireBootModule` 32 B，所以可用表 128×16 = 2,048 B、保留表 256×16 = 4,096 B、模块表 12×32 = 384 B、平台描述 4×16 = 64 B、参数字节 1,024 B；编译期算术给出 `WIRE_PARAM_OFFSET = 6848`、`WIRE_END = 7872`、`SEMANTIC_END = 14016`，而 `ARENA_CAP = 4 × 4096 = 16384` → **剩余额度只有 2,368 B**。结论可计算且明确：**UEFI 侧 256 条来源记录不可能塞进现有四页 arena**（哪怕每条只按 16 B 计也要 4,096 B）。所以要么另开一块有界载荷区（谁分配、经哪个指针/偏移交接、谁持有、何时释放都要写进 O-13a），要么把来源记录压缩到「对象级身份 + 索引」这种量级并给出可核算的编码长度。

**`#[repr(C)]` 不等于完整 ABI 合同——六项核对的当前读数**

| 核对项 | 第一跳 `BootHandoffV1` | 第二跳 `VmBootHandoff` |
|---|---|---|
| 结构大小／对齐 | 编译期断言（`size_of == 168`、`align_of == 8`、`<= ENVELOPE_CAP`） | **只有** `size_of <= 4096` |
| 逐字段偏移 | **有**（`offset_of!` 逐个钉：magic 0、version 4、size 8、arena_len 16、`kern_virt_base` 24…`param_buf_byte_len` 160） | **缺失**（无偏移断言） |
| 地址字段固定宽度 | 有（`WirePhysAddr`/`WireVirtAddr` 是 `#[repr(transparent)] u64`） | 有（`u64`/`u32`） |
| 是否含 `usize`／Rust 枚举／`bool`／引用 | 无（纯标量与定长数组；VM 侧解码后的 `BootParams` 才引入 `bool`/`Option`/切片） | 线上结构无，解码后有 |
| 载荷偏移相对 arena 起点 | 有算术断言 | 未定义（用固定 VA `VM_BOOT_HANDOFF_VA` 直读） |
| 版本／未知字段拒绝行为 | 头部有 magic/version/size | 消费侧 `assert_eq!` 相等才通过，**不等即 panic**，没有优雅拒绝路径 |

第二跳的容量推导按**最终 wire 记录数**算，不是按语义来源数；两个前提在本轮取到代码证据（第九十九轮更正为 **47**）：`MAX_BOOT_REGIONS = 32` 是**分配器的聚合上限**（`VmBootRegions { regions: [VmBootRegion; 32] }`，超限即 `TooManyRegions`，不是每池 32）；`NR_BOOT_MODULES = 12`，且 VM 自己的 blob **也在**扣减记录里（内核侧有 `boot_modules().len() == NR_BOOT_MODULES` 断言）。`deducted` 数组里存的是**与候选页域相交后实际参与扣减的范围片段**：内核映像、自身根页、bump 整段、模块 blob 各是一条**整段**记录，而**每个被触碰的池区间各产生一条已用后缀记录**。所以最坏 `3 + 32 + 12 = 47` 条，**大于 `VM_BOOT_HANDOFF_MAX_DEDUCTED = 32`**；散文里那句「逐项相加 22」只在「每个池只碰一个区间」时成立。容量目标先按保守上界写成 `ConservativeDeductionBound = 3 + 32 + 12 = 47`；若要降到 46，必须证明第 12 条模块记录不可能成为扣减项。当前 `record.push` 在超容量时直接命中 `deduction record overflow — LiveBootstrap is not enumerable within capacity` 的断言——**这是启动期 panic 路径**，不是可接受的失败形状。O-13b 的验收：把容量提到该上界（或把记录按语义合并后重新推导），外加恰好达上限与超一条的两枚测试。另有一处**代码注释与实现矛盾**：`os/kernel/src/lib.rs` 里「VM 的 blob 不进扣减记录」的注释与 `vm_handoff.rs` 的实际循环相反，已登记待修。外加恰好达上限与超一条的两枚测试。 `MAX_MAP_DESCRIPTORS = 256`、`SIDE_CAP = 64`、`WIRE_MEMMAP_CAP = 128`、`WIRE_RESERVED_CAP = 256`。O-12g 要求**每个通过结构校验的描述符**都有原始记录，所以来源记录数组容量必须 ≥ 最终 Map 缓冲可容纳的最大描述符数（按 `descriptor_count = MemoryMapSize / DescriptorSize` 计算），且必须在最终取图**之前**就发现容量不足；`ExitBootServices` 重试导致描述符数量增加时，只能安全失败，不得截断记录（截断即违反「每个描述符都有身份」）。现有失败形状已具名：`TooManyDescriptors`、`TooManySideEntries`、`Capacity(TooManyMemmap/TooManyReservedRegions)`。

**状态按三层记（O-12g）**：

| 层次 | 状态 | 证据 |
|---|---|---|
| 库接口 + 首个启动消费者 | **已实现（第一阶段）** | `audit_source_coverage(free, reserved, sources)`：每条 free 投影必须被 `Memory` 记录链覆盖，每条 reserved 投影必须被保留类记录链覆盖，否则 `SourceAuditError`；riscv bootface 已在启动路径上调用它 |
| 库接口：能产出带身份的记录 | **已实现（第一阶段）** | `SourceRecord { id, kind, origin, range: PhysicalRange }`；`class()` 区分输入声明与派生 `FirmwarePrefix`；前缀带 `{ memory_id, cut }` 派生链；精确重复声明去重投影、保留两条来源记录；容量由输入上限推出，恰好 40 可达 |
| 启动路径：真实启动生成并保留记录 | **部分接入** | riscv64 bootface 已走 `collect_with_sources()` 并调用 `audit_source_coverage`；其余三个 DTB 调用点仍走 `collect()`，记录被丢弃（§2.5） |
| 消费者：政策投影实际引用记录 | **未接入** | `PolicyProjection` 尚无代码 |

**已解决的旧冲突（按来源分治）**：`/reserved-memory` 的精确重复按**三种语义**（普通/`reusable`/`no-map`）分别去重、来源各留一条，跨语义同范围一律冲突；MRB 的精确重复**必须拒绝**（DTSpec 5.3.2）；`/memory` 的部分重叠按项目策略拒绝。六条测试见 §2.1。

**合法记录与拒绝诊断记录分开**（未实现）：结构校验失败的描述符不进入来源记录层，但必须留下**拒绝诊断记录**（`DescriptorIndex`、原始 `Type`、`PhysicalStart`、`VirtualStart`、`NumberOfPages`、失败检查项）——结构错误发生时恰恰最需要来源信息，不能因为「没通过校验」就只剩一句错误码。

**统一 `SourceRef` 必须在接通启动路径前定型**（未实现）：`SourceRef { input_object: InputObjectId, local_source: LocalSourceId }`，`FirmwarePrefix` 对 `Memory` 的引用也用它，不再保留裸 `memory_id`。**UEFI 的 `InputObjectId` 是快照实例身份，不是来源类别**：每次成功的 `GetMemoryMap()` 分配新的 `MapSnapshotId`（`MapKey` 只是该快照的固件版本凭据）；`PreflightMap` 不产生最终政策记录，`FinalMap` 的全部描述符同属一个快照 ID，stale-key 重试产生新 ID 且旧记录与旧投影整体废弃——否则三份快照里同号的 `DescriptorIndex` 会被当成同一条来源。这样每条政策、排除原因、禁止项、对象义务共用同一身份。节点路径文本与逐级 `ranges` 转换链可分阶段补。

**UEFI 原始描述符记录的最小字段**（未实现）：`DescriptorIndex`、原始 `Type`、原始 `Attribute`、`PhysicalStart`、`VirtualStart`、`NumberOfPages`、**`address_domain`**、结构校验结果。地址域必须是显式类型，不能靠字段名推断：

```
DescriptorAddressDomain = PhysicalAddress | UefiMmioPortSpaceAddress | UnsupportedOrOpaque
    ← **地址域只表达事实，是否支持由另一字段表达**（第九十六轮改）：`EfiMemoryMappedIO`（类型 11）
      描述的是物理 MMIO 区域，`PhysicalStart` 的地址域就是 PhysicalAddress；把它记成
      UnsupportedOrOpaque 会把两个不同问题混起来，将来新增一种暂不支持的物理资源类型，就会把
      已知物理地址误标成不透明地址。政策侧改记 `PolicyDisposition = UnsupportedUefiRuntimeMmio`：
      不产生 `DeviceWindowPolicy`、不进 OS 普通设备资源授权，但来源事实保留
      （UEFI 在 `ExitBootServices` 之后不再用该类型，系统 MMIO 信息应从 ACPI 取；
      这条按「评审人已核的 UEFI 2.10/2.11 表述」使用，本方环境抓不到 uefi.org）。
      **类型 12（`EfiMemoryMappedIOPortSpace`）用第三个成员 `UefiMmioPortSpaceAddress`**：它是架构相关的 I/O 空间，
      规范对 Itanium 另有一句「只定义一个这样的区域」的特别说明，**不是 x86 的 `IN`/`OUT` 端口地址**，因此不得默认与
      O-08 的 `PortWindowPolicy` 共用类型、也不得转成可被 x86 端口消费的范围。第一版若不支持类型 12，保留原始字段
      与来源身份即可。这些规范表述按「评审人已核（UEFI 2.10/2.11）」使用，本方环境抓不到 uefi.org，未复现。
    ← EfiMemoryMappedIO 可以产生物理地址域记录；
      EfiMemoryMappedIOPortSpace（类型 12）**不得**因为描述符字段叫 PhysicalStart 就规范化成
      PhysicalRange——§3 不变量 5 已规定端口 I/O 不属于物理地址集合，不进 PMM、页表或物理范围集合。
      只有 PhysicalAddress 才允许进入 PhysicalRange；类型 11（运行时服务 MMIO 请求）按项目政策
      不产生窗口，记 PolicyDisposition = UnsupportedUefiRuntimeMmio 并保留原始字段
```

每个通过结构校验的描述符先成为带身份的记录，再进入类型裁决——裁决可以拒绝启动，但不能在来源层消灭 `RESERVED`/`UNUSABLE`/MMIO/类型 11/12；导致拒绝的 `PAL_CODE`/`UNACCEPTED` 至少要在错误上下文保留索引、类型与范围。

**仍缺的字段**：节点路径（今天只记 `origin` 类别 + 序号，多个 `/memory` 节点靠 `id` 区分，没有路径文本；序号的意义绑定输入对象与遍历顺序，跨对象不可解释）、每级 `ranges` 转换链、`SourceRef` 的统一类型（今天只有顶层 `id` 与 `FirmwarePrefix{memory_id,cut}` 的内嵌引用，尚无统一类型）、UEFI 侧未输出描述符的记录。顺序不变：先把记录接到启动路径并由政策层持有，再谈 O-02/O-03 的禁止集合编译；否则 DenyStandardSystemMapping、DenyUncontrolledSpeculativeAccess、MRB 访问禁令与「同页多重排除理由」都没有输入来源。

**记录层与原始输入对象分开**：原始记录保真**不等于**永久保留整份 DTB 或 EFI 内存图缓冲区。`SourceRecord` 是规范化记录（保留到政策投影与审计不再需要）；原始 DTB 副本只在解析与转换期间需要，最后一个消费者结束后可以释放。`ranges` 转换链（每级输入/输出地址、引用节点、转换属性）属于记录层，不能只留最终物理地址。

#### O-12i 每种投影的页粒度规则

| 投影 | 边界处理 |
|---|---|
| `RamSourceDomain` | 保留原始 byte range；只有构造页域时才说明边界处理 |
| `PmmSourceCandidatePages` | 只交付完整落入合法 RAM、且通过保留／存活对象／类型与生命周期排除的整页（向内收缩） |
| `HardwareAdmissionExcluded` | 架构与平台物理访问能力不满足而排除的页（专项侧，不在本项） |
| `PmmBaseAvailable` | 上面两项组合后的最终基底可用页 |
| `PmmBaseExcluded` | 对保留范围**向外覆盖整页**，部分相交的页不得进入基底可用 |
| `PmmRoleRejected` | 不做量化：它标注的是基底内某页缺哪条用途证明，不产生页集合边界 |
| `no-map` 映射禁止 | 向外覆盖所有相交页，防止大页/小页叶子跨入禁止区域 |
| MRB 访问禁止 | 保留精确 byte range；若通过页表落实，另做页粒度投影 |
| 设备窗口映射 | 叶子覆盖的**全部页**都要有兼容授权，不能只看起始页 |

**用途集合是分配时的凭证，不是事后可以随意扩大的标签**：分配交付的是 `(page, U_verified, p)`。① `U_verified` 记录交付时已证明的用途集合，页表 / 可执行 / 原子用途必须显式在里面；② 承担新用途（数据页改作页表、页表页改作可执行、任意页首次做 AMO）要**先重新准入再使用**——对 `U_verified ∪ {新用途}` 重跑 `PmmRoleEligible`，且用当下的 `H`（参与 hart 集合）与 `p`（阶段）重取证据，因为 hart 入场与阶段切换都可能使旧证明失效；③ **撤回用途不释放容量**：`U` 变小不产生新的可用页，一页能否回到 `PmmBaseAvailable` 只由生命周期决定（存活对象是否解除、保留是否撤销、`UNACCEPTED` 是否接受），不由用途标签变化决定——否则「改个标签」就凭空多出内存。

**所有 `PMM*` 集合都是 4 KiB 页集合**：`TypeAllocatablePages(p)` 表示「整页都满足阶段 `p` 的类型资格」的页；`PlatformAllocationDenials(p)` 也必须是页集合（或先向外量化）。来源原始范围仍是 byte range，只有进入指定投影时才量化；**阶段资格变化必须由显式事件触发**，不得随时间推进自动释放。

**`FullPages` 以页编号定义**：

```
PageBytes(n) = [ u128(n) * 4096, (u128(n) + 1) * 4096 )   ← 右端独占；FullPages(R) = { n | PageBytes(n) ⊆ R }
```

参数是页编号而不是物理地址或 byte range；`0 ≤ n < 2^52`（4 KiB 页、64 位物理地址），边界先转 `u128` 再运算，最高页的独占末端可以是 `2^64`。**两端都是半开区间**，与全文集合模型一致。只有整页 byte range 都被 RAM 来源覆盖才进入页域，不是「起点向上、终点向下后拼接」。
**页域与排除贡献的正式定义**（基底用补集，用途用子集——用途层不构成第二次划分）。第九十九轮把三件事分开：**贡献记录**（谁、为什么、原始范围、页投影、阶段）、**页集合**（并集）、**致命拒绝**（不是集合贡献）：

```
ExclusionContribution { evidence_ref, reason, raw_range, page_projection, phase }
    evidence_ref ∈ SourceRef | ObjectRef | PolicyRuleRef     ← 不强迫对象与政策贡献伪装成原始来源记录
    reason ∈ StaticReservation | LiveObject | TypePolicy | Lifecycle

PmmSourceExclusionContributions(ctx) =
    静态保留类：FirmwarePrefix | ReservedOrdinary | ReservedReusable | ReservedNoMap | MrbEntry
      ← **MRB 必须单列**：它的分配排除与访问禁止是两份合同、两条贡献，不因同出一源就合并；
        与 EFI DTB 相交时，DTB 的类型特例只改类型主张，MRB 的排除仍然存在
    ∪ LiveObject（以 ObjectRef 为身份）∪ TypePolicy(ExcludeRange) ∪ LifecycleRule（以 PolicyRuleRef 为身份）

PmmSourceExcludedPages(ctx) = Union(c.page_projection for c in 上述贡献)
    ← 页只扣一次由并集保证；「为什么扣、哪些来源共同扣」由贡献记录保证——裸 Union 只留成员关系，
      不留归因，所以两个产物必须分开写

TypePolicyDecision = AcceptWithDisposition | ExcludeRange | RejectBoot
    ← **拒绝启动不是页集合贡献**：只有 ExcludeRange 进 PmmSourceExcludedPages；
      UNACCEPTED 与 PAL_CODE 属 RejectBoot，整个启动输入不被接受，
      绝不能写成「排除若干页」后仍然产出一份看起来合法的候选集合

PmmSourceCandidatePages(ctx) = RamSourcePageDomain \ PmmSourceExcludedPages(ctx)
    ← **本项的输出边界就到这一层**：它回答「来源与生命周期上这页能不能进 PMM 候选」，
      不回答「这台机器允许这次访问吗」。
```

**页粒度表同时要改**（原表把 `PmmBaseAvailable` 写成「包含用途无关准入域的整页」，那是旧模型）：`PmmSourceCandidatePages` 负责来源、保留声明、对象存活与类型／生命周期政策；`HardwareAdmissionExcluded` 负责架构与平台的物理访问能力；`PmmBaseAvailable` 是两者组合后的最终基底可用页。

**来源排除与硬件准入是两次相减，不是一次**：`PmmSourceExcluded` 由本项定义（四类贡献、逐条可归因），`HardwareAdmissionExcluded` 由专项定义；两次相减的顺序不影响结果，但**谁贡献了哪一条排除理由**必须分别可查。；用途资格属于消费者在分配时附加的约束，不应与基底排除合并；硬件准入、用途资格与映射路径审计见 `01-design-note-hardware-admission-model.md`。随本轮一并移出的还有 `MemoryAdmissionContext` 的架构证据槽位、`PolicyProjectionKey` 的完整签名、参与 hart 集合变化导致投影失效的推导、`MappingIntent`／`MappingAudit` 与 `DenyStandardSystemMapping`／`DenyUncontrolledSpeculativeAccess` 的叶子级判定。**本项只保留与交棒直接相关的一条**：快照身份、阶段与证据代次必须进投影签名，否则同一份来源记录会产出两个都叫「可用表」的结论。

- 任何「可用」主张必须同时带用途集合与准入上下文：裸写 `PMMAvailable(·)` 不是良定义项。
- 用途集合**可组合、不互斥**，不是页的分类；分配时用途未知只能保守取 `U = AllRoles`——这两条约束的是消费者接口，本项保证交接信息足以支撑它们。

#### 用途准入与硬件准入模型已迁出本项（第九十六轮）

从这里起的位置，属于另一份文档：那一段「普通访问准入、页表用途、原子能力、对象访问三层展开、整页全称判定」的模型。它回答的是**硬件能否以某次操作触碰某段字节，以及 OS 该不该把这一页交给某个用途**——按 §1.6 的划界属 BootEntry 与分页侧专项，不是内存图交棒的验收条件。第九十六轮把它**逐字**迁到同目录的 `01-design-note-hardware-admission-model.md`；迁移动机与两份未闭合项写在那一份的篇头，公式本身一条没改。

本项只保留三条与交棒直接相关的事实，它们留在 O-12i 前半节：

| 保留项 | 它对交棒合同的意义 |
|---|---|
| 页域与两层划分的定义（`RamSourcePageDomain`、`PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx)`） | 交接信息必须能被消费者无歧义地量化到页；量化顺序（先兼容性裁决、再并集、再取完整页）本身是来源归一化的一部分 |
| 「用途拒绝不等于基底排除」这条不变量（§3 第 2 条） | 两类结论一旦压成同一个集合，「这一页为什么被排除、被谁排除」就不可追溯 |
| 用途集合是**分配时的凭证**（交付 `(page, U_verified, p)`，不得事后扩大） | 说明交接清单必须能支撑这种凭证；凭证本身的实现属内核侧，不由本项兑现 |
### O-13 固定布局 ABI（第三层）

**先更正一句过头的话**：第三层不是「完全没有现状」。今天已有一份 `#[repr(C)] struct BootHandoffV1`（`os/libs/minix-boot/src/handoff_v1.rs`）：头部含 magic/version/size/arena_len，标量含内核映像与引导区各段地址长度，切换后回填 `root_page/bump_base/bump_next/bump_end`，载荷按**字节长度**给出（`memmap_byte_len` 等），容量是 128/256/12/4/1024，`ARENA_CAP = 4 * 4096`，并有编译期布局门；地址经 `x0`（AArch64）、`rbx`（x86_64）、`a0`（RISC-V）递交。第二跳的 `#[repr(C)] struct VmBootHandoff` 同样是固定数组＋magic/version，扣减记录容量 `VM_BOOT_HANDOFF_MAX_DEDUCTED = 32`，并有 `deducted_count <= 32` 的运行期断言——**它是真二进制 ABI，不是同一镜像内的普通 Rust 结构**。

未定的是**扩展**，不是基线。因此拆成两项：**O-13a（boot-shim → 内核）**回答现有 v1 布局如何承载来源记录、平台资源描述、启动对象身份与生命周期（今天 `KernelInfo` 只有 `platform_sources` 的类别标签＋地址，无来源记录字段）；**O-13b（内核 → VM）**回答扣减记录、自由页集合、容量与所有权的传递与失败形状。`test_boot_image_table_layout` 钉的是 boot_image 表的算术（5 个内核任务 + 12 个模块 = `NR_BOOT_PROCS`），**不覆盖内存图语义**，别把它当交棒语义的证据。等 O-12 的子项定型后，按真实消费者设计，不在集合未定时冻结。
**判据**：每个跨界记录有唯一消费者、容量核算与失败形状。**消费者**：内核消费方。

## 6. 证据与复核

**三个提交身份必须分开写**（第九十七轮；「本文对应提交 `fe47471f1`」这句话容易被读成「当前全文快照就是那一版」，而正文已经走到第九十七轮、`03` 已到六十七节）：

| 身份 | 记录什么 | 当前值 |
|---|---|---|
| `CodeEvidenceCommit` | 146/70 两条宿主读数与交叉编译实际对应的代码提交 | `53c44e846`（其后各批均为纯文档，无代码改动，故读数未重跑） |
| `DocumentRevision` | 本轮正文与档案的版本 | 第九十九轮；`03` 至六十八节、`01-design-note-hardware-admission-model.md` v1 |
| `ReviewedRepositoryCommit` | 评审人应拿去核对的完整仓库提交（文档 + 代码） | 每轮在横幅里写**本轮合并提交的完整 SHA**；分支头是移动引用，不能当评审身份。第九十九轮：`a7d9ab4fa78f3b635d819844b9d0480ad5d08937`（正文 sha256 `6628418e62755d6c150476ddf2154bfc2a0257c79d610e0db5a4d04d7624a4f9`、档案 sha256 `e32bb7e30770f49c0e5c501436348adbc7c887544e38be82f325118dc683e5df`，同值存档于 `03` 六十九节；第九十八轮的值同时保留在那里，便于回溯） |
| `DocumentFileHash` / `03ArchiveFileHash` | 正文与档案的字节级身份 | 按上述合并提交计算，列在 `03` 六十八节；未提交的工作树版本另记 `WorkingTreeDiffHash` |

横幅时间与本表由第四道门对账；三者任一不符即说明材料不是同一版。

每条主张的可核对形式是测试名、代码位置的函数名、以及下表的读数。规范逐字引句、锚点清单与版本钉法在同目录 `03`：RISC-V 取 `riscv/riscv-isa-manual@418bdc4990ae2d60a86f1e11e8d7795aa279da61` 的 `src/priv/supervisor.adoc` 与 `src/priv/machine.adoc`，设备树取 `devicetree-org/devicetree-specification@8314f1e1c9965482e5c1080d4118f52fef827fce` 的第三、五章，Profile 取 `riscv/riscv-profiles` 的 `src/rva23-profile.adoc`（blob `4fce3ebf34393d7a52564dfab53b69730fab514d`）。**引用规范要钉发布版与源 commit，活动分支 `main` 会漂移，不能当长期冻结证据**；RVA20/RVA22/RVA23 各自的版本号与能力清单仍需另行钉住。

| 证据 | 读数 |
|---|---|
| 宿主单测 | `cargo test -p minix-boot --lib` **146 通过**；`cargo test -p boot-shim --lib` **70 通过**（用例见 §2.1、§2.2） |
| 落码提交 | `38668f843` 起：DTB 保留来源、来源记录层、`audit_source_coverage`、边界端点分名 |
| 交叉编译 | `x86_64-unknown-none`、`aarch64-unknown-none`、`riscv64gc-unknown-none-elf`、`x86_64-unknown-uefi`、`aarch64-unknown-uefi` |
| QEMU 启动腿 | riscv64 bootface PASS（三点位齐全，仅覆盖该入口）；aarch64 PASS（串口读到 `conv=13 dtb_regions=1` 与内核进入行）；x86 PASS 但该腿 ESP 上没有 `dumpdtb.dtb`，**采集器根本没被进入**，只证明启动未回归 |
| 输入准入 | `fdt_layout::validate` 五处调用点见 §2.7；容器与结构预扫的测试清单在 `03` |

**证据分层：编译通过 ≠ 启动验证 ≠ 新分支被覆盖**。三条腿都在边界改动之后重跑，但 `size`-only 拒绝与末端 `2^64` 分名都不改变 QEMU/OpenSBI 与 EDK2 实际递交的设备树（真实例子节点都带 `reg`，也没有范围触到顶端页），所以腿上的 PASS 是「未观察到变化」，这两个分支目前只有宿主测试覆盖。aarch64 日志里的 `dtb /memory cross-checked, diagnostic only` 也**不代表** O-11 的配置表 DTB 首读授权已验证——配置表 DTB 尚未进入政策链，该测试证明的是今天实际接入的路径。

**事实分级的两侧口径**：RISC-V 与设备树的规范原文由本方从固定 commit 的源文本直取。UEFI 侧三项——`EfiReservedMemoryType` 在类型表的字面「Not usable」、`GetMemoryMap()` 对「没有物理硬件支撑的地址空间不作为物理内存范围返回」与「有物理支撑但不应由 OS 访问的范围必须以该类型返回」的区分、以及 §4.6.1.3 要求标准 `EFI_DTB_TABLE_GUID` 位于 `EfiACPIReclaimMemory` 并按物理地址解释——由**评审人按 UEFI 2.11 独立核对**；本方环境抓取 uefi.org 返回 403，**本方未复现**，因此按第三方核对引用，不升级成本方已验证。AArch64 Table 2.5/2.6 的分工同样只有评审人核对，无原文引用。规范保证「固件不修改 DTB」不等于「所有 UEFI 应用与驱动都不修改」，复制期间源对象的一致性仍由当前 loader 保证。

**下一轮最该补的证据**（可复核事实，不是新公式）：x86_64 OVMF 与 aarch64 AAVMF 的实际镜像身份、版本与哈希，并附该固件实际返回的代表性描述符及其 `Attribute` 原始值与项目接受的完整位解释表——要证明的是运行固件**实际用了哪套编码**，不是它自报符合哪个规范版本，两条腿的固件也不必同源；逐 `PlatformMemoryProfile` 的 QEMU 构建版本、machine version、accelerator、CPU 型号与 SMP 配置（设备窗口与 PLIC 公式目前仍是 profile 候选值）；`PreflightMap` 与首次 `FinalMap` 的缓冲容量策略及 `EFI_BUFFER_TOO_SMALL` 处理；四条 DTB 输入路径的启动 ABI 合同，特别是「复制完成前原 DTB 不被并发修改」的依据——O-11 的内容一致性条件目前是假设。

**复核命令**（`git rev-parse HEAD`、`git status --short` 与两条 `cargo test -p <pkg> --lib`）及干净基线的期望输出在 `03`。146 与 70 是该基线、该工作树状态下的读数，§2.1、§2.2 与本节三处数字必须一致；基线之后如有新提交必须重跑。

**测试通过不等于输入完整性**：146 与 70 条说明的是已覆盖的行为，不能证明所有 DTB 输入都被解析、所有原始访问点都被登记、每条启动路径都用了正确的权威来源——这几项各自需要独立的完整性论证（O-07、O-11、O-12）。
## 7. 签字状态

| 层 | 结论 | 理由 |
|---|---|---|
| 第一层：语义与方向 | **可签** | §1.6 三合同划界；来源事实／冲突裁决／纯集合投影三层分离；保留语义（普通／`reusable`／`no-map`、MRB、UEFI 类型）各自独立 |
| 第二层：来源完整性与交接判据 | **不签** | 六项阻塞见下 |
| 第三层：固定布局 ABI | **不签** | 现有 `BootHandoffV1` 与 `VmBootHandoff` 是已存在的基线，缺的是承载来源记录与对象信息的**扩展**（O-13a／O-13b） |

**第二层的验收内容已按 §1.6 收紧**，只保留真正属于「资源发现与交接」的六项：

1. **输入真实性**——**provenance 与 reachability 分开**：I-29 负责各启动 ABI 的**对象身份、地址域、授权范围、阶段与失效点**，以及可复核的来源依据（provenance）；「该地址在当前阶段是否端到端可达、是否满足非受控推测访问限制」（reachability）由 BootEntry 与各架构 adapter 负责。**配置表 DTB 指针存在，不能推出它可读**——即便 UEFI 规定该指针按物理地址解释且 DTB 必须位于 `EfiACPIReclaimMemory`，可达性仍要单独证明。aarch64 UEFI 配置表 DTB 的三态规则已裁定（不存在→仅按内存图；已解析→双来源交叉验证；存在但未接入或解析失败→拒绝启动），实现为零。
2. **来源保真**：每条启动路径的 `SourceRecord` 生产、保存与**传递**。今天的实况见 §2.9——只有 riscv bootface 生成记录，且审计后即丢弃；`KernelInfo` 没有承载字段；UEFI 侧连 `Attribute` 都没记录，而模块头注释声称记录了。
3. **冲突与生命周期**：DTB／UEFI 静态保留的交叉验证、MRB、`no-map`、类型政策各自语义独立；首读与解析后拒绝两个决定已分开（O-04／O-11 的冲突已消）；O-01 第一版采静态处置表 + 损失声明，`ReservedReusable` 保持独立类型。
4. **分配所有权**：候选集与交付集不得混称（`ConventionalCandidateRanges`／`VmFreeRanges`）。§2.8 的时间线已把问题收到两句话——**候选集合包含 bump 池（已证明）**，**首次分配早于任何扣减（已证明）**，**实际分配页是否碰到 bump 池（未证明）**。验收：两枚具名测试 `BootBumpRange ∩ FirstVmBootAllocatorCandidates = ∅` 与 `FirstVmAllocatedPages ∩ BootBumpRange = ∅`（前者靠显式排除满足，但替代不了后者），外加 `FREE_MEMMAP` 的去留裁决——它零读者、也不是候选输入，**建议删除或改造成唯一权威输入**，留着只会让维护者以为保护已经做过。
5. **平台资源描述**：设备 `reg` 与多级 `ranges` 采集未开始（O-12j-c）；数值必须带证据状态四标签（`ObservedInputResource`／`ProfileExpectedResource`／`StaticPolicyRange`／`ExpectedMappingCoverage`），PLIC 的 6 MiB 与 2.25 MiB 目前都不是运行时采集结果。**不含**映射覆盖审计与访问准入证明。
6. **容量与 ABI**：接受上限是 `min(256, floor(缓冲/40)) = 256`，**不是**缓冲能装的 524；来源记录总容量按五项公式分列（UEFI 256、DTB 40、派生、启动对象、平台资源），后两项今天没有独立容量。第一跳 arena 只剩 2,368 B，256 条记录塞不进，必须另开有界载荷区或改成 `(MapSnapshotId, DescriptorIndex)` 引用（O-13a）。第二跳的 `deducted` 存的是**裁剪后的片段**，最坏 `3 + 32 + 12 = 47 > 32`（`MAX_BOOT_REGIONS = 32` 是分配器聚合上限、模块 12 条含 VM 自身，两项已取到代码证据），超容量即断言 panic——这是启动期失败路径，必须扩容或按语义合并后再推导（O-13b）；同时补字段偏移／尺寸／对齐断言、`VM_BOOT_HANDOFF_VA` 的地址与生命周期、线上字段字节序、以及 magic/version/size 不匹配的**明确失败语义**（fail-stop 可以，但要写成合同而不是实现细节）。「超限整体拒绝不截断」**已实现**。

**已登记的实施清理（不在本轮动代码，按纪律 6 只登记）**：① 把 `collect_with_sources()` 设为唯一正式结果，`DtbMemorySnapshot { conventional, reserved, sources }`，旧两表视图只许显式派生——现在三条路径调 `collect()` 会天然丢掉来源身份；② `FREE_MEMMAP` 删除或改造成唯一权威输入；③ `VmBootAllocator` 的构造参数改成「经过校验的候选区集合」，让排除不依赖每个调用点记得追加；④ `ebs_map.rs` 模块头注释与实现统一（类型 11/12 的 attribute 今天既不记录也不诊断）；⑤ `VM_BOOT_HANDOFF_MAX_DEDUCTED` 按分片数重新推导并补边界测试；⑥ 来源记录总容量五项分列。

**驱动访问点审计（每个使用点的 VA／宽度／读写方向／参与 CPU）不在本项**：§1.6 的资源表已把它与 `DeviceResource` 分开，O-07 的实际访问完整性不再算作本项第七项阻塞。**列为外部依赖、不影响本项签字状态**：叶子粒度与拆分／降级路径、PTE 的 A/D 更新方案、页表访问的 PMP/PMA 证明、LR/SC／AMO／预约集等原子能力、根激活前后的审计时序与 `FrozenBootMappingSet` 的冻结字段。这些材料在 `01-design-note-hardware-admission-model.md` 与 `03` 六十五节里继续存在，其中两条已知未闭合（共享原子合同仍按 LR/SC 分支；跨页源级访问的整体覆盖规则未进入逐操作判定式）已写在那一册篇头。

**现状与合同相反的一条（不是缺口，裁决已下）**：低 4 GiB 恒等映射不消费 `no-map`、MRB 与类型禁止政策，普通叶子带 `EXECUTABLE`，且今天的启动流程没有对该区间执行拆分或撤销的调用路径（§2.3）。O-02a 裁定甲案（授权驱动建表）、否决乙案与半拉子形态；其四段依赖链中，**本项只交付第一段（来源进入政策）**，第二、三段属映射政策消费者，第四段属分页专项——这条归属划分不改变「输入边界与来源完整性是它的前置证明」。

「禁止集合编译 + 引导根审计」即便有代码，也只能作为第一轮实现复审的起点。在那之前，对 §5 的评审只需回答两个问题：问题是否成立、候选是否覆盖失败形态。
