# 01 · 硬件准入与用途资格模型（BootEntry／分页专项的上下文材料）

> **版本**：v1（第九十六轮自 `01-adjudication-brief-memmap-open-items.md` 逐字迁出，公式与措辞未作任何修改；迁出区间是该文 O-12i 后半节，共 47,692 字符、672 行）。
> **射程**：本册记录的是「硬件能否以某次操作触碰某段物理字节」「某一页该不该交给某个用途」「对象的一次访问实际展开成哪些硬件操作」这三类判定。**它们不是 I-29（引导期物理内存图与平台资源的发现、归一化与无损交棒）的验收条件**，而是 BootEntry 与分页／物理内存管理专项的上下文。判据来自 I-29 简报 §1.6：OS 决定完全不开大页，也不应要求 bootloader 换一种内存图交接方式。
> **状态**：全部条款仍是**候选合同，零代码、零消费者**。本册不冻结，也不进入签字流程；立项时应当重新按代码与设计输入审一遍，而不是继承这里的编号。
> **两条已知未闭合（迁出时如实标注，勿当成已完成）**：
> ① 共享原子合同一面禁止 `algorithm.uses_lr_sc` 这类机制字段进入共享层，一面又在 `AtomicSharedStateEligible` 里写「若用 LR/SC 则要求 `EventualAtomicProgress`」——共享层不应再按 LR/SC 分支，进展要求应由 adapter 以语义级形式兑现。
> ② 对象级判定逐个检查 `ActualMemoryOperations`，而正文另有「跨页源级取指的完整 footprint 必须由同一条 PMP 条目覆盖」的项目要求；若一次源级访问被拆成多个硬件操作，逐操作公式并未表达「这些操作共用同一条目」这条额外规则。RISC-V 规范确实允许一条指令产生多次访问，且失败时其它访问可能已产生可见副作用——所以这条整体覆盖是**项目选择的更保守合同**，不是规范自带规则，公式与文字必须统一到其中一边。
> **术语档位**：本册内容全部属 Architectural Evolution（不存在对应 C 行为可复刻，也不改现有 Rust 语义），涉及架构机制的段落是各架构 adapter 侧的展开示例，不决定三架构共享接口的形状。
> **路径与标注约定**：见 `rewrite-notes/README.md` 的全局阅读约定；本册不重复。
> **读者**：与本专项相关的实现者与评审人。看不到仓库也能读，因为公式自带展开位置；但**不要**据此实现，除非该专项已立项。

**能力、硬件准入与平台拒绝是三项，不能合并**：

- `OrdinaryCachePolicyRepresentablePages(profile)`：**预映射可行性**（跨架构语义，不以 UEFI 术语命名）——物理范围与硬件配置是否存在**某种**合法映射编码可兑现普通主存缓存语义：x86 看 MTRR 状态与可用 PAT 项能否表达 WB；AArch64 看所有参与 PE 的 MAIR 中是否存在可用的 Normal Cacheable 属性；RISC-V 看平台 PMA/机器级配置能否满足目标。**它不能读实际叶子**——PMM 计算时叶子还不存在。
- `ObservedMappingEffectiveAttribute(mapping, arch_state)`：**实际兑现**——叶子选择的编码与硬件状态组合后的最终属性（x86 叶子 PAT 索引×MTRR、AArch64 AttrIndx×MAIR、RISC-V PBMT×PMA），只进 `MappingAudit`。把后者塞进 PMM 会造成「先有 PTE 才能决定能否分配、分配又要求先有 PTE」的循环。
- 物理访问准入按用途分成**两半**：`CommonCpuAccessEligiblePages(ctx)` 是与用途无关的基底部分，`RoleAdmit(role, page, ctx)` 是用途特有部分；四格内容以正文的 `RoleSpecificRequirements(role, ctx)` 正式定义为准，不另列表格。

**原子用途要把「指令可执行」与「算法进展假设」分开**：规范的 Reservability PMA 把 LR/SC 支持分三级——不可预约、可预约但不保证最终成功、可预约且保证最终成功（逐字引句与锚点见 `03`）。光登记指令族与宽度不够，用途记录要带下面的字段。

```
AtomicPageRoleRequirement {          ← 页级能力：候选页能否承担这类原子状态（**不含**具体物理地址）
    operation_family, access_width, atomicity, progress_requirement, fallback_plan_id,
}
    ← 共享结构只有操作语义四项加一个可追溯的 fallback 计划引用。**不含 `reservation_set_requirement`**：
      预约集是 RISC-V LR/SC 的承载形式——x86 的锁定前缀原子没有预约集模型，AArch64 的 exclusive monitor
      是另一套约束。把它做成三架构共用必填字段，等于让两个架构各填一个语义不自然的 NotApplicable
    （对象级原子访问不再另立实例类型：它就是带 `reservation_set_evidence` 的 `ActualAccessInstance`，见对象访问一节）
AtomicCapabilityPermits(arch, algorithm, scope, cpus, isa_profile_id)   ← 共享层唯一的原子能力谓词
    = 该架构 adapter 证明：这些 CPU 在该范围上以该算法要求的族/宽度/原子性可完成访问，且满足所声明的进展要求
    ← RISC-V adapter 在这里持有 ReservationSetRequirement { base_page_containment, profile_geometry }
      与 ReservationSetGeometryCore(scope, cpus, isa_profile_id)
      （AMO-only ⇒ 不检查预约集；LR/SC ⇒ 基础页内约束 ∧ 声明 profile 时的连续／自然对齐／≤64 B 几何）；
      AArch64 与 x86 由各自 adapter 给出对应证明。共享判定核不知道预约集、LR/SC 或指令前缀
    ← 页级入参 =（候选页, 算法要求, 全部参与 CPU, profile）；对象级入参 =（实例自带的证据与实际物理范围,
      执行该访问的 CPU, profile）——对象级必须真正消费它，页级通过不能替代对象级证明

AtomicFallbackPlan {
    trigger, bounded_retry_policy,       ← 何时判定缺乏进展、如何避免无限重试
    required_operations, required_widths, required_memory,   ← fallback 自己用到的操作、宽度与内存区域
    progress_argument,                   ← 为什么 fallback 本身能完成工作
}
```

**`fallback` 必须是可追溯的计划引用，不是布尔值**：一个布尔标记只说明「源码里还存在另一个函数」，而那个函数自己可能也在用 LR/SC，或依赖该区域并不支持的 AMO。计划要证明四件事：能完成**同一算法操作**；不依赖刚被判为不足的进展保证；它实际使用的操作与宽度在该页上**确有 PMA 支持**（走同一条 `RoleAdmit`）；检测到缺乏进展时**真的切换**过去。合同因此必须显式写明两件事：`trigger` 与 `bounded_retry_policy` 是检测机制（不能把「存在另一个函数」当检测），以及 `required_operations` / `required_widths` / `required_memory` 覆盖 fallback 自己的锁变量与状态字——不得出现「fallback 依赖一块从未过准入的共享内存」。

```
AtomicSharedStateEligible(page, ctx) =              ← 页级：只做「登记完整性 + 算法要求的进展等级 + 架构原子能力」
    AtomicRequirementsComplete(ctx) ∧ RegisteredAtomicAlgorithms(ctx) ≠ ∅
    ∧ ∀ algorithm ∈ RegisteredAtomicAlgorithms(ctx) :
        ProgressOf(algorithm) = 若用 LR/SC 则 EventualAtomicProgress，否则取该算法登记项 progress_requirement 显式给出的
                                OperationCompletion 或 NotRequired   ← 登记是合同输入，不允许实现者自行推断该选哪个
          ← 分型而非一个共用的 Eventual：AMO-only 不因它要求最终成功；其他架构的额外进展要求由 adapter 映射
        ∧ ∀ scope ∈ AlgorithmScopes(page, algorithm, ctx) :
              AtomicCapabilityPermits(algorithm, scope, ctx.ParticipatingCpuSet, ctx.isa_profile_id)
            ← **不加 `algorithm.uses_lr_sc` 这类前置条件**：AMO-only 不检查预约集，不等于 AMO-only 不经过
              原子能力判定——它的预约集要求可以是 NotRequired，但整个算法仍必须通过同一个语义接口，否则
              AArch64 的 exclusive／LSE 与 x86 的锁定前缀原子会绕过这个谓词，而它恰恰是全文唯一的共享原子入口。
              谓词由架构 adapter 的 trait 契约分派，不靠共享层对 `arch` 做枚举分支；
              「用的是 AMO 还是 LR/SC」是 RISC-V 的实现选择，不进共享算法模型的必填事实
    ← 操作族、宽度与逐起始的硬件准入**只在** RoleSpecificRequirements + PhysicalAccessPermits 那条路上查一次；
      这里不再重复 AccessRequirementsSatisfied，否则两边会各自演化出不同的「进展是否满足」
    ← 对象级实际访问复用同一个 adapter 谓词（入参换成实例证据与实际物理范围）：两层共享判定核心，不共享机制名
```

`AtomicProgressRequirement = NotRequired | OperationCompletion | EventualAtomicProgress`（AMO 的操作完成语义与 LR/SC 的预约最终成功不是同一件事）。

- **只用 AMO 的算法**：要求相应 AMO 家族与宽度即可，不因它要求 `RsrvEventual`；它的预约集要求是 NotRequired、不检查预约集，**但仍走 `AtomicCapabilityPermits` 这条唯一语义入口**。
- **LR/SC 的几何、进展与页内约束是三件事**：基础页内约束（`norm:lr_sc_rsrv_set_page`，第一版即使不声明 profile 也成立）∧ 进展要求 ∧ 声明 profile 时的几何（`Za64rs`），三项并列，缺一不得通过原子用途准入。第一版把所有实际使用 LR/SC 的算法一律要求语义级「最终成功」保证（RISC-V 即 `RsrvEventual`）；**不依赖最终成功的 `RsrvNonEventual` + fallback 路径整体不开放**——规范允许它，但前提是 `FallbackPlanSatisfied` 有真证据，第一版没有。
- 规范「建议在主存区域提供 `RsrvEventual`」是一句 NOTE（推荐，不是强制），不能当准入依据引用。

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

**页表准入不能写成 `page ∈ PmmBaseAvailable(ctx) ∧ RoleAdmit(...)`**：`PmmBaseAvailable` 扣除了 `Outward(LiveObjectRanges(ctx))`，而既有引导页表恰恰是存活对象——**越是被使用的页表页，越不可能通过这个谓词**。这是两个不同问题：① 该物理页的硬件能力是否允许承担页表用途；② 该页当前是否处于可供 PMM 分配的状态。拆成三个：

```
CommonBackingCapabilityEligible(page, ctx)          ← 与用途无关的公共能力，两条路径都必须过
    = 来源与类型允许
      ∧ OrdinaryCachePolicyRepresentablePages(ctx) 命中该页   ← 既有页表不经过 PmmBaseAvailable，缓存可表达性必须自带
      ∧ CommonCpuAccessEligiblePages(ctx) 命中该页            ← 普通读写准入
      ∧ MemoryOrderingCompatiblePages(ctx) 命中该页           ← 内核使用的内存排序模型
      ∧ ProfileMemoryEligiblePages(ctx) 命中该页              ← 已声明 Profile 的区域义务；未声明取中性语义
      ∧ 该页的物理内存类别属于该架构允许的页表 backing 区域
      ∧ 该页属于普通、幂等主存                                ← 项目政策（非规范无条件要求）

PageTableSpecificAccessEligible(page, ctx)          ← 页表用途特有的访问类别
    = PageTableRequirementsComplete(ctx)                          ← 专属完整性门槛，取代通用 RoleRequirementsComplete
      ∧ AccessRequirementsEligible(page, PageTable, PageTableAccessRequirements(ctx), ctx)
        ← 绕开通用「需求生成器」，但绝不绕过通用量词：逐需求 × 逐合法起始 × 逐参与 CPU × 完整 footprint
    = 页表读取（ImplicitPageWalk / Read）准入通过
      ∧ 内核显式 PTE 读写准入通过                              ← 与 A/D 方案无关，两种方案都必需
      ∧ 按 A/D 方案要求的 PTE 更新准入通过
        （HardwareManagedAd 另需 ImplicitHardwarePteUpdate：
         隐式 W 权限 + 硬件页表写能力 + 相对同一 PTE 的原子更新 + 语义级进展保证）

PageTableBackingEligible(page, ctx)
    = CommonBackingCapabilityEligible(page, ctx) ∧ PageTableSpecificAccessEligible(page, ctx)

PageTableAllocationEligible(page, ctx)
    = page ∈ PmmBaseAvailable(ctx) ∧ PageTableBackingEligible(page, ctx)

ExistingPageTableObjectEligible(object, ctx)
    = 对象身份与存活登记有效
      ∧ PageTableBackingEligible(object.page, ctx)
      ∧ 对象的映射、权限与访问路径通过审计

← backing 能力里不得出现 LiveObjectRanges 或「PMM 可分配」语义：能否用于页表与能否交给分配器是两件事
← RolePageEligible 的定义点只有一个，见下面判定式块里的分支；此处不再重复一份定义
```

**两条路径判定同一组条件，只是不重复计算**：新分配页经 `PmmRoleEligible → RolePageEligible(PageTable)`，而 `PmmBaseAvailable` 已经算过缓存可表达性、普通访问、排序与 Profile 区域义务；既有页表对象不经过 PMM，因此这四项必须由 `CommonBackingCapabilityEligible` 自带——「普通访问准入通过」**推不出**「缓存属性能按目标语义兑现」，两者在本文是独立层次，所以缓存可表达性必须逐字出现在 backing 里。职责划分：`PageTableBackingEligible` 是**一份**硬件能力合同，PMM 交付新页表页时把它与分配资格合成 `PageTableAllocationEligible`，对象审计对**已存在**的页表对象用 `ExistingPageTableObjectEligible`（不要求它重新成为可分配页，只要求有来源、有身份、能力合格、路径可审计）。两个消费者共用同一份 backing，而不是共用一个带「可分配」语义的谓词。

- `PlatformAllocationDenials(ctx)`：**平台策略显式拒绝**，不再兼任架构准入。
- `MemoryOrderingCompatiblePages(ctx)`：**内存序 PMA 的落点**。Machine-Level ISA 的 Memory Ordering PMA 小节按 `FENCE` 与原子排序位的目的把地址空间分为主内存与 I/O，并规定一致主内存总是 RVWMO 或 RVTSO、非一致主内存是实现自定义内存模型；另有两句把概念拆开——一致性是**单一物理地址**上的属性，不要与整个内存系统的内存一致性模型混淆，且规范**不鼓励**使用硬件非一致区域（软件复杂度、性能与能耗代价）。逐字引句见 `03`。内核的同步原语按其中一种模型书写，因此：① 该集合进入**基底层**（用途无关），用途特有的原子要求仍归 `RoleAdmit(AtomicSharedState, ·, ctx)`；② **第一版不支持由软件维护一致性的非一致主存**：区间被声明为主内存但 Coherence PMA 显示非一致 → 无论其内存模型能否被证明，都不得进入 `PmmBaseAvailable(ctx)`，实现自定义排序的区域同理；③ RVWMO 与 RVTSO 之争是**软件假设**问题：内核按 RVWMO 书写时 RVTSO 区域也满足（更强），反之不成立，故 profile 记录「内核假定模型」而非只记录区域模型；④ 那句「为 `FENCE` 与原子排序位目的」限定的是排序语义用途，不是来源域成员资格，不能替代 `RamSourceDomain`。若将来要支持软件维护一致性，需要另立同步与访问合同，而不是让一份 memory-ordering profile **顺便**替代 coherence。两条规则各答各的问题：

| 集合 | 回答的问题 |
|---|---|
| `CommonCpuAccessEligiblePages(ctx)` 的 coherence 条件 | 多 hart 对共享物理地址的访问能否满足内核的一致性要求 |
| `MemoryOrderingCompatiblePages(ctx)` | 区域实际使用的内存模型是否满足内核同步原语的假设 |

- **页表只允许普通、幂等主存是项目政策，不是规范无条件要求**：规范说的是主存区域被**假定**幂等，而对非幂等区域「隐式读写不得提前或推测执行」并**列了例外**。第一版选更保守的政策，是为了避免还要去证明非幂等区域上的隐式页表访问不会提前或推测执行——报告时必须写成项目政策。

**幂等性不能反过来证明是主内存**：规范的小节名是 Idempotency PMAs，属性对是 `Idempotent` / `Non-idempotent`。主内存区域被**假定**幂等（因此不需要额外证据，也就不能拿它区分主存与 MMIO）；I/O 区域的读幂等与写幂等可**分别**规定；访问可能有副作用时推测与冗余访问必须避免，非幂等区域的隐式读写不得提前或推测执行（主源对该条另列页表遍历相关的例外分支）。方向因此固定：先由来源域认定主内存，才谈得上排序与幂等属性适用——「观察到读写无副作用」不是 RAM 来源资格证据。「非幂等区域禁止提前/推测隐式访问」正是页表内存必须落在幂等区域的原因，但它是 `PageTableBackingEligible` 的证据之一，不由幂等 PMA 单独决定准入。AArch64 与 x86 无对应 PMA 概念，等价要求由 `OrdinaryCachePolicyRepresentablePages` 与实际叶子审计承担；本节集合对非 RISC-V 架构按「由 profile 声明为兼容」处理，**不得留空当作已通过**。

**架构证据按问题分层，不能汇总成一个布尔值**：

| 资格 | 架构证据 | 何时验证 |
|---|---|---|
| `OrdinaryCachePolicyRepresentablePages` | UEFI `WB` 能力位 / DTB 平台属性政策 | 来源记录阶段 |
| `MemoryOrderingCompatiblePages` | RISC-V 内存序 PMA（一致主内存 = RVWMO/RVTSO；非一致 = 实现自定义）；其它架构由 profile 声明 | 来源记录阶段 |
| `CommonCpuAccessEligiblePages` | 参与 hart 集合的普通读写与一致性；RISC-V 有效 PMP 权限、PMA 主存分类 | 准入阶段，用途无关 |
| `RoleAdmit`（用途特有） | RISC-V 按**实际生效的 A/D 更新方案**条件化（见下表）；原子操作按 AMO / LR-SC 与宽度分别登记；其它架构的页表遍历、取指限制 | 准入阶段，按用途 |
| 映射属性兑现 | x86 `PAT` 项 × 物理范围 `MTRR` 有效类型；AArch64 `AttrIndx` × `MAIR_ELx`（读回 MAIR 不足以证明叶子选对了索引）；RISC-V `PBMT`/PMA | **逐实际叶子**（`MappingAudit`） |

**RISC-V 的 A/D 更新方案决定页表页准入**（不能无条件要求 `RsrvEventual`）。规范原文由本方从主源 `riscv/riscv-isa-manual` 的 `src/priv/supervisor.adoc` 与 `src/priv/machine.adoc` 直取，逐字引句带规范锚点：

> 规范定义两种 A/D 管理方案：*Svade* —— 访问虚拟页时 A 位为 0、或写入时 D 位为 0，产生页故障异常；未实现 Svade 时适用另一方案，**页表必须位于具备硬件页表写能力与 `RsrvEventual` PMA 的内存**；并且「一个系统内所有 hart 必须采用同一 PTE 更新方案」（`norm:pte_ad_scheme_uniform`）。完整逐字原文见 `03`。

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

**为什么用分支而不是布尔式**：把三个输入写成一条布尔运算，会在 `Svadu=已实现 ∧ ADUE=1 ∧ Svade=已实现` 时给出软件方案，而规范原文与下面的真值表都要求硬件更新。分支形式顺带把**证据要求**也钉精确：

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

**hart 方案一致不是项目自设的准入，而是规范强制**：规范逐字要求系统内所有 hart 采用同一 PTE 更新方案（`norm:pte_ad_scheme_uniform`）。由此**「证据缺失时默认 `SupervisorFaultManagedAd`」也不安全**：若硬件实际自动更新而内核按软件方案建表等待 A/D 故障，故障永不到来，fault handler 不承担该职责；反过来该区间还需要硬件页表写能力与 `RsrvEventual`，按软件方案就会漏检。三层分开写：

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

职责不重叠：`SystemHartSet` 只用来验证 PTE 更新方案的**全系统**一致性，`ParticipatingHartSet` 只用来计算普通物理访问准入的**逐 hart 交集**，两者不得互相替代。反例（4 hart 平台）：hart 0/1/2 参与、hart 3 暂不启动且实际方案与其它三个不同——**不能**因为 hart 3 没进 `ctx.H` 就宣称 `norm:pte_ad_scheme_uniform` 已满足。该证明由 M-mode/固件按 `SystemHartSet` 逐 hart 给出，缺任一 hart 的证明即 `Unknown`。

**清单的完整性要按 CPU 节点状态语义判定**（DTSpec）：`okay` 表示该 CPU 在运行；`disabled` 表示它处于静默状态、可由 `enable-method` 重新启用（规范要求 `disabled` 的 CPU 必须提供该属性），因此**不能简单排除**——它仍可能是未来可启用的系统 hart；`fail` 的措辞是「not operational or does not exist」，两种读法对系统集合的后果不同，**不得伪称物理不存在**。

| CPU 状态 | 对 `SystemHartSet` 的处理 |
|---|---|
| `okay` | 纳入 |
| `disabled` | 纳入；quiescent 不等于不属于系统，且规范要求其提供 `enable-method` |
| `fail` 且证据证明**物理上不存在**对应 CPU | 排除，记 `PhysicallyAbsent` |
| `fail` 但证据证明**物理存在**（哪怕不可运行） | **纳入 `SystemHartSet`**——本文已选定「平台上所有物理存在的 hart」，不可运行不改变成员资格 |
| 只能证明 `fail` / 不可运行，既证不了物理存在也证不了不存在 | `InsufficientEvidence` → **拒绝启动**；`NotOperational` 只作诊断事实，不得当作成员资格裁决 |

**为什么这条不能含糊**：系统级 PTE 更新方案一致性是规范义务（`norm:pte_ad_scheme_uniform`），若允许用 `fail` 状态暗中缩小 `SystemHartSet`，等于用输入清单的措辞替规范减免义务。清单语义已在本文选定为「物理存在的 hart」，因此不再保留「取决于正式定义」这种未定分支。

| 状态或清单完整性未知 | `InsufficientEvidence` |

因此 `SystemHartSet` 的定义必须**先选定**是「平台上所有物理存在的 hart」还是「当前系统中可能运行/可重新启用的功能性 hart」——两者成员可以不同，本简报取前者（更保守）。无论取哪个，都不能用 `ParticipatingHartSet` 的成员变化偷偷改变全系统一致性义务。每条启动入口都要给出：

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
| **Svadu 已实现、`ADUE=1`、Svade 也已实现** | **硬件更新**——任何把三个输入做无条件 OR 的写法都会在这条上给出错误方案，负控必须能把它挡下来 |
| Svadu 已实现、`ADUE=0` | 软件故障路径 |
| Svadu 未实现、Svade 已实现 | 软件故障路径 |
| Svadu 未实现、Svade 未实现 | 硬件更新路径 |
| Svade/Svadu 状态无法证明 | 拒绝启动，不得默认软件故障 |
| `SystemHartSet` 内两个 hart 的有效方案不同 | **拒绝该平台配置**：两者都属于系统 hart，规范要求全系统一致。只有证明某 hart 物理上不存在或不属该系统才可排除；`disabled`、暂不调度、禁止其进入 `ParticipatingCpuSet` 都**不是**排除依据 |

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
AdmissionContextValid(ctx) = ProfileAdmissionUsable(ctx) ∧ SystemPteUpdateSchemeUniform(ctx)   ← 准入前置；任一不成立则 ctx 不可用于任何投影
ProfileDeclarationValid(ctx)         ← 声明的 Profile 身份、版本与适用范围是否合法
ProfileGlobalObligationsSatisfied(ctx) ← CPU/系统级义务：满足后才允许进入区域级检查
ProfileMemoryEligiblePages(ctx)      ← 区域级义务，逐物理主存区间判定（进 PmmBaseAvailable）
ProfileAdmissionUsable(ctx)  = state ∈ { NotDeclared, DeclaredAndGlobalChecksSatisfied }
                               ∧ ( state = NotDeclared ∨ ( ProfileDeclarationValid(ctx)
                                                          ∧ ProfileGlobalObligationsSatisfied(ctx) ) )
    ← 声明本身无效（Profile ID / 版本 / 适用范围不合法）却把全局检查标成通过，是可能的输入；
      上下文可用性必须同时消费声明有效性，不能只看状态标签
ProfileConformanceSatisfied(ctx) = ProfileDeclarationValid ∧ ProfileGlobalObligationsSatisfied
                                   ∧ AllRegionObligationsSatisfied(ctx)            ← 聚合三者；不得由一个系统级标签提前盖章
ProfileContextState = NotDeclared | DeclaredAndGlobalChecksSatisfied(id) | DeclaredButDeclarationInvalid(id, evidence)
                      | DeclaredButGlobalChecksFailed(id, evidence) | InsufficientGlobalEvidence(id)
    ← 上下文层只判**全局**检查；区域级义务此时尚未构造，不能盖章成「已符合」
    ← 第一版 isa_profile_id 未声明 ⇒ **NotDeclared**：`ProfileAdmissionValid` 不得返回「通过」；
      `ProfileMemoryEligiblePages` 取**中性语义**（不施加 profile 额外限制），**不是空集**——否则基底被交成空集；
      声明后证据不足 ≠ NotDeclared；`SystemPteUpdateSchemeUniform` 在非 RISC-V 架构须返回有类型的 `NotApplicable`
ProfileAdmissionValid(ctx)          ← 系统级：ctx.isa_profile_id 声明的强制义务集合是否整体成立
ProfileMemoryEligiblePages(ctx)     ← 页级：义务覆盖的主存区间（与 BaseAccessRequirements 求交）
```

不是所有义务都是逐页属性，所以不把它们机械塞进 `RoleAdmit`。第一版 `isa_profile_id = 未声明`，两个判定的结果都是「**未作合规声明**」——这既不阻塞第一版，也**不得**被未来调用方读成「Profile 检查已通过」。
**`Za64rs` 的预约集几何要有着落，但它不是共享结构的字段**：`Za64rs` 要求「Reservation sets are contiguous, naturally aligned, and a maximum of 64 bytes」（`rva23-profile.adoc` 97–98 行）。这条几何连同其证据、判定核心与失败形态都归 **RISC-V adapter** 的原子能力合同（见上节的 `AtomicCapabilityPermits`），共享层的 `AtomicAlgorithmRequirement` 只保留操作族、宽度、原子性与进展要求。理由是这三条约束（连续、自然对齐、≤64 B）在 x86 与 AArch64 上没有对应物；把它们塞进共用必填字段，换来的不是统一，而是两个架构各写一个 `NotApplicable` 来填空。RISC-V 的正确推导不应决定三架构共享数据模型的形状。

**共享层与架构 adapter 的边界（属重构项而非 RISC-V 算法错误）**：通用内存资格模型的接口只表达 OS 需要的**语义**——普通物理访问要求、页表读取与更新要求、原子操作的宽度/原子性/进展、执行访问要求、参与访问的 CPU 集合、证据代次与有效期；具体机制名属于各架构 adapter：参与访问的 CPU 集合（RISC-V hart / AArch64 PE / x86 逻辑处理器）、页表更新要求（`Svade`/`Svadu`/`ADUE` 与 `RsrvEventual` / 硬件 `AF`+dirty / 硬件 A/D）、原子能力（`AMO(family)`/`Lr`/`Sc` 与预约集几何 / `exclusive`+`LSE` / 锁定 `lock` 前缀族）、物理访问保护（PMP+PMA / MAIR 与层级保护 / PAT+MTRR 与分页）。

三条硬规则：① `MemoryAdmissionContext` 里不得出现只在一套架构上有意义的**必填**字段——`pte_update_scheme_id` 对 x86 无意义，它是 RISC-V adapter 的可选证据槽位，不是共享接口的核心维度；② 共享层枚举不得出现 `Amo/Lr/Sc` 这类指令名，改为中性的「原子操作族 + 宽度 + 进展要求」，由 adapter 提供映射；③ `hart_set_id` 这类术语统一为 `participating_cpu_set_id`，RISC-V 章节内可以继续叫 hart。`PmpPermits`/`PmaPermits`/`RequiredPermission` 等谓词名**属于 RISC-V adapter**，AArch64 与 x86 用各自实现满足同一层语义接口。**这些类型现在全是候选设计，重命名在落码前完成成本最低**；§5 的 RISC-V 细节照旧保留，只是不再决定三架构共享接口的形状。
**整页合格是「合法起始 × 整次访问 × 每个参与 hart」的全称判定**（此前写成一个「整页是否被可允许访问集合包住」的集合包含式，把起始地址与访问宽度丢了）：

```
AccessRequirement = { privilege, path, operation, width, alignment, atomicity, progress_guarantee }
    privilege: ExecutionPrivilege = Kernel | Unprivileged     ← 共享层的 OS 语义；由 adapter 解释：
        RISC-V Kernel=S-mode、AArch64 = 实际运行内核的 EL、x86 = 实际内核 CPL；M-mode/EL2/SMM 等
        不属于普通内核访问合同的情况由 adapter 的适用性规则处理，不写进共享枚举
    path ∈ {Explicit, ImplicitPageWalk}
    operation ∈ {NormalRead, NormalWrite, InstructionFetch, AtomicUpdate{family}, CacheMaintenance{kind}}
        ← 名称分三层，共享公式**只许出现第一层**（旧式把 `Read`/`Write` 直接写进共享合取，
          而 `Write` 不在本枚举里；下表是唯一的转换边界）：
          | 层 | 名称 |
          |---|---|
          | 共享需求 | `NormalRead`、`NormalWrite`、`InstructionFetch`、`AtomicUpdate{family}` |
          | RISC-V adapter | `Load`、`Store`、`Fetch`、`AMO`、`LR`、`SC` |
          | 硬件保护检查 | 由 adapter 映射到 PMP 的 R/W/X 与 PMA 许可，含实际访问宽度 |
          AArch64／x86 的映射同理，只在各自 adapter 文档里定义，不进共享判定式。
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
    ∧ match req.progress_guarantee:                       ← 只认 AtomicProgressRequirement 的三个成员，不留第四种写法
          NotRequired         => 不追加进展义务
          OperationCompletion => 按该架构 adapter 检查相应操作完成语义（RISC-V：AMO 家族与宽度可得操作完成）
          EventualAtomicProgress => 该区间满足语义级「最终成功」保证（RISC-V adapter 兑现为 `RsrvEventual`）

PhysicalAccessPermits(hart, req, [a,b)) =
    PmpPermits(hart, req, [a,b)) ∧ PmaPermits(hart, req, [a,b)) ∧ PlatformProtectionPermits(hart, req, [a,b))

BaseAccessRequirements(ctx) = 任何普通数据页都必须满足的访问类别
    实际访问代码用到的 (Load/Store, 宽度) 集合（字节/半字/字/双字等，由 ISA + 编译目标 + 实际代码确定）
    完整性由 `GeneralDataCompleteness(ctx)` 保证；证不出完整即 `InsufficientEvidence`

RoleSpecificRequirements(role, ctx) = 该用途**在基底之上**的额外访问类别
    GeneralData        → ∅                                   ← 空集是定义，不是遗漏：基底已覆盖
    PageTable          → { (ImplicitPageWalk, NormalRead,  PteSize, PteAlignment) }   ← 两种 A/D 方案都必需
                         ∪ { (Explicit, NormalRead,  PteSize, PteAlignment) 若内核直接读 PTE }
                         ∪ { (Explicit, NormalWrite, PteSize, PteAlignment) }  ← 安装/修改/撤销：两种方案都必需
                         ∪ { ImplicitHardwarePteUpdate 若 HardwareManagedAd }
    Executable         → `InstructionFetchPattern` 展开的取指访问类别（见下，不能退化成单个宽度）
    AtomicSharedState  → ExpandAtomic(RegisteredAtomicAlgorithms(ctx))   ← **等式而非巧合**：
                         登记表是唯一输入，每个算法展开成 (operation_family, width, progress, geometry)；
                         只生成 A 不生成 B 时 PhysicalAccessPermits 会漏掉 B，故
                         AtomicRequirementsComplete(ctx) 要求「每个登记算法都完整展开」而非「表非空」
                         RISC-V adapter 再把 family 展开为 AMO(...) / LR-SC；宽度与进展只存外层一处

ImplicitHardwarePteUpdate = AccessRequirement { privilege=Kernel, path=ImplicitPageWalk, operation=NormalWrite,
    width=PteSize(active_translation_mode), alignment=PteAlignment,
    atomicity=AtomicPteUpdate, progress_guarantee=EventualAtomicProgress }   ← 相对同一 PTE 的其它访问必须原子
        ← 硬件 A/D 更新要求的 RISC-V `RsrvEventual` 正是语义级 EventualAtomicProgress；本节 PmaPermits 只认
          NotRequired / OperationCompletion / EventualAtomicProgress 三个成员，写 Eventual 是未定义成员

AccessRequirementsEligible(page, role, requirements, ctx) =   ← 全文唯一的全称判定核：三个消费者共用同一份展开
    requirements ≠ ∅                                          ← 空需求集合不得让 ∀ 真空通过
    ∧ ∀ req ∈ requirements :
        LegalStarts(page, req, role, ctx) ≠ ∅                 ← 页内无合法起始 ≠ 合格
        ∧ ∀ a ∈ LegalStarts(page, req, role, ctx)
          ∀ cpu ∈ ctx.ParticipatingCpuSet : PhysicalAccessPermits(cpu, req, Footprint(a, req))

PageTableRequirementsComplete(ctx) =                ← 页表用途的完整性门槛（不借用通用生成器）
      当前翻译模式已知 ∧ PteSize / PteAlignment 已明确
    ∧ 架构 adapter 的页表遍历访问模型完整（覆盖全部层级与全部有效 PTE 起始位置）
    ∧ 显式 PTE 读访问清单完整：要么列出全部显式读，要么证明约定审计范围内没有此类读
    ∧ 显式 PTE 写要求完整（安装 / 修改 / 撤销三类都算）
    ∧ 有效 PteUpdateScheme 已知且适用，且该方案要求的隐式更新要求已完整展开
    ∧ PageTableAccessRequirements(ctx) ≠ ∅
    ← 它要拦住的失败形态：① 翻译模式不明却拿默认 8 字节当页表格式证明；② 只登记一个
      ImplicitPageWalk/Read 类别就以为覆盖了全部层级与起始位置；③ 内核确有显式 PTE 读却没进需求集；
      ④ 方案未知却默认软件故障、或 HardwareManagedAd 漏掉隐式写的任一项（隐式 W、硬件页表写、
      原子更新、EventualAtomicProgress）、或 A/D 方案变了却复用旧准入结果；⑤ 某个架构 adapter 返回空集合
      或 NotApplicable 就被当成「页表用途无额外要求」——三架构都必须兑现同一份共享语义

BasePageEligible(page, ctx) =
    AdmissionContextValid(ctx) ∧ ctx.ParticipatingCpuSet ≠ ∅     ← 系统级前置；空参与集合会让 ∀ hart 真空通过
    ∧ GeneralDataCompleteness(ctx) = Complete                     ← 清单本身要证完整，非空还不够：漏掉 8 字节 Store 同样会通过
    ∧ AccessRequirementsEligible(page, GeneralData, BaseAccessRequirements(ctx), ctx)

RolePageEligible(page, role, ctx) =                        ← 全文唯一定义点：分支，不是两份平行定义
    if role = PageTable then PageTableBackingEligible(page, ctx)
    else GenericRolePageEligible(page, role, ctx)
        ← PageTable 不走通用体：它的三类需求由 PageTableAccessRequirements 逐项列举、由
          PageTableRequirementsComplete 单独证明完整，不依赖需求生成器是否漏出空集

GenericRolePageEligible(page, role, ctx) =
    RoleRequirementsComplete(role, ctx)          ← 需求生成器没有因缺 profile / 缺代码扫描而「漏出」空集
    ∧ ( role = AtomicSharedState ⇒ AtomicSharedStateEligible(page, ctx) )   ← 原子用途的唯一展开入口
    ∧ if role = GeneralData then
          RoleSpecificRequirements(role, ctx) = ∅        ← 唯一定义为空的一格：它的基底需求已由 BasePageEligible 判完
      else
          RoleRequirementsApplicable(role, ctx)          ← 适用性由 adapter 证明
          ∧ AccessRequirementsEligible(page, role, RoleSpecificRequirements(role, ctx), ctx)
    ← **必须分支**。判定核第一条拒绝空需求集合，而 `GeneralData` 的用途特有集合按定义为空：写成无条件调用，
      `RolePageEligible(page, GeneralData, ctx)` 永不成立，任何包含 `GeneralData` 的用途集合都跟着一起失败——
      这不是保守程度问题，是公式没按设计意图展开。
    ← 也不许反过来把判定核的 `requirements ≠ ∅` 删掉来绕过矛盾：那会让「漏登记」的用途靠 ∀ ∈ ∅ 真空通过

CommonCpuAccessEligiblePages(ctx) = { page | BasePageEligible(page, ctx) }   ← 仍要与其它基底交集条件求交
PmmRoleEligible(page, U, ctx)     = page ∈ PmmBaseAvailable(ctx) ∧ U ≠ ∅
                                    ∧ ∀ role ∈ U : RolePageEligible(page, role, ctx)
```

**`PteSize` / `PteAlignment` 绑定当前翻译模式**：显式读写同样带宽度，不留「无宽度的 Read/Write」。**软件故障方案只改变 A/D 位由谁更新，不取消硬件页表遍历本身**——地址转换算法仍会隐式读取 PTE，所以 `(ImplicitPageWalk, Read)` 与 `(Explicit, Write)` 都在「始终必需」那一列；硬件更新方案才在其上追加 `ImplicitHardwarePteUpdate`。

**名字与定义必须一一对应，空集不得真空通过**：`RoleAdmit(role, page, ctx)` 是声明式接口，其操作化定义**只有** `RolePageEligible(page, role, ctx)` 一个展开式（分支形式），三个消费者共用同一份 `AccessRequirementsEligible` 判定核，但各喂自己的需求集合——`CommonCpuAccessEligiblePages` 喂 `BaseAccessRequirements(ctx)`，`GenericRolePageEligible` 的非 `GeneralData` 分支喂 `RoleSpecificRequirements(role, ctx)`，页表分支喂 `PageTableAccessRequirements(ctx)`。**空集合在两处含义相反，必须用分支把它们分开**：`GeneralData` 的用途特有集合按定义为空（基底已覆盖，这是唯一允许空的一格，走空分支直接合格），`Executable`／`AtomicSharedState`／其它用途出现空集是漏登记，走判定核那一支被 `requirements ≠ ∅` 挡住。`PageTable` 因隐式遍历读与显式 PTE 写始终必需而非空。基底侧另三条前置不得省：`ctx.ParticipatingCpuSet ≠ ∅`、`GeneralDataCompleteness(ctx) = Complete`、每条基底 `req` 的 `LegalStarts ≠ ∅`——宽度清单必须由**固定编译目标 + 编译器代码生成约束 + 已审计指令集合**三者合成，证不出完整就是 `InsufficientEvidence`，不是「已通过」；`Executable` 的 `instruction_lengths` 为空集同理，那等于宣称该页没有取指需求。把 `RoleAdmit` 存成一个布尔字段会丢掉 `req`、起始地址、CPU 三个量词维度。

**全称量词不能替换成「字节被覆盖」**：一页每个字节都被某次成功访问覆盖，并不代表从该页任一合法起始地址发起的最宽访问都能成功——编号最低的条目只需命中 footprint 内一个字节就要覆盖整次访问，跨边界的操作整体失败（规范自带的反例是一条覆盖 `0xC–0xF` 的条目遇到 `0x8–0xF` 的 8 字节访问）。PMP 粒度最细 4 字节（`norm:pmp_granularity`、`norm:pmp_a_field_napot_na4`），页内边界真实存在，这正是判定必须量到起始地址的原因。`AdmittedFootprints` / `AllowedOperationStarts` 这类「先把可允许访问收集成集合、再比包含」的写法可以当等价推导，但**判定式本身必须写成对起始地址全称量化**。判定式的索引必须是 `AccessRequirement` 而不是页：同一 hart 同一条 PMP 配置下，「普通读写可用但隐式页表读不可用」「页表可读但 A/D 隐式写不可用」「读写可用但取指不可用」「读写可用但某 AMO 宽度不可用」四类差异真实存在，字节集合会把它们抹平。**一个页不满足 `PageTable` 用途，不代表它不能作为 `GeneralData`**——把页表条件塞回基底会把用途层架空。

**`LegalStarts` 有两个互斥合同，实现者不得自行挑选**：

| 合同 | `LegalStarts(page, req, role, ctx)` | 成立前提 | 代价 |
|---|---|---|---|
| **A 页级保守** | 该页内**同时满足「该访问类别的架构起始对齐」与「该用途规定的完整 footprint 约束」**的起始地址：Load/Store 取自然对齐且宽度 ≤ 页大小；LR/SC 取自然对齐并满足基础页内约束（+ 声明 profile 时的几何）；AMO 只要求自然对齐与对应家族/宽度，不检查预约集；Fetch 取满足指令起始对齐且**最大指令 footprint 不越出本页** | 用途资格独立于对象登记：页一旦合格，任何合法起始都能用 | 跨页对象无法只靠页级资格表达；PMP 边界附近判得更严；Fetch 的页尾若干字节不可作起点 |
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
    access_instance_set_id,        ← 源级访问实例集合的引用：实例从哪来必须有登记，不能靠 Expand 猜
    access_requirement,          ← 需求模板；每个实例的 requirement 必须由它展开得到
    tree, phase, path, required_cpus, source_refs   ← 来源记录身份供政策授权层判定
}
ExpectedTranslationSegments = { TranslationSegment { va_range, expected_pa_range } }   ← 必须是 va_range 的非空、无重叠、无空洞分段
    ← 连续 VA 区间不保证映射到连续 PA：跨虚拟页的对象/取指会落到多段物理范围，
      单一 expected_pa_range 表示不了这种映射

VirtualAccessInstance {          ← 第一层：源级一次访问（原始宽度与架构对齐）
    instruction_or_access_id, virtual_address, original_width, architectural_alignment,
    architecture_access_evidence_ref,   ← 架构相关证据的**引用**：共享类型不认识预约集，字段名与类型都不出现它
}
TranslatedAccessFragments(i) = { TranslationFragment { va_range, pa_range } }
    ← 第二层：完整翻译结果，须无遗漏、无重叠并保持字节对应；它回答「字节映射到哪里」
ActualMemoryOperations(i, arch) = { MemoryOperation { requirement, footprint } }
    ← 第三层：由架构 adapter 按**实际执行规则**展开的硬件 memory operation，
      每个自带宽度、操作类别与 footprint；它回答「硬件实际执行了哪一次访问」
    ← 三层不可合并：一条 4 字节取指可能映射成两段 PA、再被硬件拆成两次操作；
      把翻译片段当 operation，就让两个片段分别命中不同 PMP 条目，绕过整次覆盖规则
ActualAccesses(use) = Resolve(use.access_instance_set_id).instances   ← 实例只来自登记的集合，Expand 不再凭空生成访问地址
InstanceSetComplete(use) =                     ← 对象访问的第二份完整性证明，被 ObjectAccessEligibility 消费
    ActualAccesses(use) ≠ ∅
    ∧ Resolve(use.access_instance_set_id).source_evidence 覆盖 use 声明的 audit_scope
    ∧ ∀ i ∈ ActualAccesses(use) : InstanceMatchesRequirement(i, use.access_requirement)
    ← Expand 的合法职责只有两个：验证实例符合需求模板，或把模板展开成硬件访问类别；
      它没有 virtual_address、指令身份与预约集证据，因此不能充当实例来源
path 只有一个来源：`use.path` 已删除，授权判定与物理准入都消费 `use.access_requirement.path`
    ← 同一访问点不得存在两套路径事实，否则可能出现授权走一条路径、准入走另一条路径
ObjectAccessSubject = CpuAccess(cpus)
    ← 第一版**只建模 CPU 访问主体**：cpus = Resolve(use.required_cpus, ctx) ≠ ∅ ∧ cpus ⊆ ctx.ParticipatingCpuSet。
      不设 NotApplicable(reason) 逃逸分支——reason 不是证明，任何调用方都能用它跳过整段 PMP/PMA 检查；
      非 CPU 主体（DMA/IOMMU、设备、固件）不属于第一版对象访问合同，遇到即 UnsupportedAccessActor，
      将来须由独立的访问主体合同承接，不得理解为「无需访问准入」

AccessInstanceSet {                                  ← 实例的产生者必须显式，Expand 只负责展开需求类别
    instances: NonEmptySet<VirtualAccessInstance>,
    source_evidence,                                 ← 这些实例来自哪份登记或工具链证据
    completeness_scope,                              ← 覆盖范围；裸指针与内联汇编在范围外即记 Incomplete
}
    ← 二选一，不得含糊：要么每条 ObjectAccessUse 就是一条具体访问实例，并规定严格对应
      i.virtual_address = use.va_range.start、i.original_width = use.access_requirement.width、
      i.instruction_or_access_id = use.access_use_id 的实例身份；要么一条 use 涉及多条指令或多个
      起点时，必须给出独立的实例列表。Expand 不能同时承担「生成访问类别」与「从源码恢复访问地址」

ObjectPhysicalAccessEligible(use, ctx) =
    ActualAccesses(use) ≠ ∅                    ← 只挡「空集合让 ∀ 真空通过」这一种；
        实例集合是否完整由 InstanceSetComplete 在对象级判定处负责（见 ObjectAccessEligibility）
    ∧ ∀ i ∈ ActualAccesses(use) :
        TranslatedAccessFragments(i) ≠ ∅ ∧ ActualMemoryOperations(i, ctx.arch) ≠ ∅
        ∧ FragmentsCover(i.virtual_address, TranslatedAccessFragments(i))   ← 片段的 VA 分区无重叠、无空洞，恰好覆盖 VirtualFootprint(i)
        ∧ SegmentsCorrespondToExpectedTranslation(i, use)          ← 逐段比对 VA→PA 对应，不是比物理并集
        ∧ ArchitecturalAlignmentSatisfied(i)                       ← 约束原始指令/操作数的有效地址
        ∧ OperationsRealizeAccess(i, TranslatedAccessFragments(i), ActualMemoryOperations(i, ctx.arch), ctx.arch)
        ∧ AtomicAccessesEligible(i, TranslatedAccessFragments(i), subject_cpus, ctx.adapter_evidence)
             ← adapter 内部分派：RISC-V 才用 ReservationSetRequirement / ReservationSetGeometryCore /
               ReservationSetEvidence（`RsrvEventual`），AArch64 用 exclusive monitor 约束，x86 用锁定前缀族。
               **不得默认用访问宽度代替预约集实际几何**：Union(TranslatedAccessFragments(i)) 是翻译后的实际
               物理片段，未必等于该算法需要证明的预约集范围
        ∧ ∀ op ∈ ActualMemoryOperations(i, ctx.arch) ∀ cpu ∈ subject_cpus :
              PhysicalAccessPermits(cpu, op.requirement, op.footprint)   ← 逐操作，用该操作自己的宽度与类别
    ← 对齐字段只有一处含义：原始访问的架构对齐；分解后每个 operation 另按整次覆盖规则判定

SegmentsCorrespondToExpectedTranslation(i, use) =
    ∀ f ∈ TranslatedAccessFragments(i) : ∃ 唯一的期望段 s ∈ ExpectedTranslationSegments(use) :
        f.va_range ⊆ s.va_range ∧ f.pa_range = TranslateByOffset(s, f.va_range)
    ← 只比物理并集会放过「两段 VA 的 PA 被互换」：并集相同而对应关系已错

OperationsRealizeAccess(i, fragments, operations, arch) =          ← adapter 契约，由架构侧证明
    操作集合覆盖该源级访问实际触碰的全部字节（不遗漏、不缩小）
    ∧ 跨页访问产生的每一次操作都在集合内（不得只报第一次）
    ∧ 每个操作的 footprint 是某个翻译片段的子集，且其物理地址取自该片段而不是另算
    ∧ 该架构若允许额外预取或进一步分解，必须在本契约内声明——不允许实现者自行缩小 footprint

ObjectPolicyAccessEligible(use, ctx) =
    AccessAuthorizationCovers(
        use.object_id, use.source_refs, ObjectAccessSubject(use, ctx), use.tree, use.phase,
        use.path, use.access_requirement.operation, use.access_requirement.width,
        use.access_requirement.privilege, use.expected_translation)
    ∧ SourceSpecificDenialsSatisfied(use, ctx)
    ← 授权逐操作、逐宽度、逐特权级消费：某对象拿到一段范围的**只读**授权，推不出它拥有同范围的
      写入、取指或原子访问授权。MRB 里那次一次性 DTB 读取的授权只覆盖（指定对象、指定 MRB 来源、
      指定字节范围、指定阶段、只读、指定 CPU 主体与访问路径、复制到自有副本为止的失效点），
      不得解释成该对象对整个物理区域的泛化访问许可
    ← 来源政策各自解除，不得由一个通用 AccessAuthorization 布尔覆盖所有负向限制：MRB 的具名一次性
      授权、no-map 的非受控访问限制、EfiReservedMemoryType 的 DenyOsUse 各有各的规则
    ← path 只有一个来源：规定 use.path = use.access_requirement.path（或删除其一），
      不得出现授权检查走一个 path、物理准入检查走另一个 path

ObjectMappedAccessEligible(use, ctx) =
    ObjectPolicyAccessEligible(use, ctx)
    ∧ Resolve(use.va_range, use.tree, use.phase) = ExpectedTranslationSegments(use)
         ← 逐段验证：整区间落到多段 PA 时，每一段都要对应，不是起点相等
    ∧ ∀ segment ∈ ExpectedTranslationSegments(use) :
          MappingPermissionsAllow(segment) ∧ EffectiveMappingAttributeCompatible(segment)   ← 逐片段/叶子
    ∧ AliasCompatibilityHolds(use) ∧ ObjectPhysicalAccessEligible(use, ctx)

ObjectAccessEligibility(object, ctx) =
    ObjectAccessInventoryState(object) ∈ { Complete(uses ≠ ∅, audit_scope), NoAccess(proof) }
    ∧ ( ObjectAccessInventoryState(object) = Complete(uses, audit_scope)
          ⇒ uses = ObjectAccessUses(object) )   ← 要验的是「状态携带的清单就是实际消费的清单」，不是非空标签
    ∧ ∀ use ∈ ObjectAccessUses(object) :
          InstanceSetComplete(use)              ← 非空不等于完整：部分实例集不得冒充完整实例集
          ∧ ObjectMappedAccessEligible(use, ctx)
ObjectAccessInventoryState(object) = Complete(NonEmptySet<ObjectAccessUse>, audit_scope)
                                     | NoAccess(machine_checkable_proof)
                                     | Incomplete
    ← 三态穷尽，不留混合入口：有访问点 → 必须 Complete 且清单**类型级非空**；确认没有访问点 → 必须走
      NoAccess 并给出机器可检查证明；没完成检查 → Incomplete。允许 `Complete(∅, scope)` 就等于让
      「一条访问都没登记」冒充「完整扫描后确认没有访问」，绕开特意设的 NoAccess 分支——这与
      `AccessInstanceSet.instances` 早已采用 NonEmptySet 是同一规矩
    ← NoAccess(proof) ⇒ ObjectAccessUses(object) = ∅，且 proof 必须覆盖适用阶段、适用树、全部访问路径与对象
      整个存活期间的访问面
    ← 两份完整性证明各自独立、都不许省：① `ObjectAccessInventoryState` 证明**给定 audit_scope 内**该对象的
      全部访问使用点都已登记；② `InstanceSetComplete` 证明**每个使用点**的实际实例集合完整且逐实例符合需求模板。
      O-07 已承认裸指针与内联汇编尚未全覆盖，所以 ① 的范围必须锁在具名 audit_scope 里，
      不得让 Complete 这个名字被读成全系统访问完整性证据
    ← 只写 `ActualAccesses(use) ≠ ∅` 会放过这种失败：某条 use 登记了一次合法访问、代码里还有未登记的其它访问，
      全称量词在那一次访问上成立就放行——那正是「部分实例集冒充完整实例集」
```

**三层必须分开，不能只留一层**：① 来源政策授权（这个对象有没有权访问这段来源范围）② 硬件物理准入（CPU 能不能以该操作访问这些字节）③ 映射路径兑现（这条 VA 路径是否确实逐段映射到预期 PA，权限/属性/别名是否兼容）。只查 ② 会让「PMP/PMA 都允许的 MRB 区域」「硬件仍允许 S-mode 读取的 `EfiReservedMemoryType`」「带 `no-map` 的非受控访问限制」全部绕过 §3 的不变量。

**物理可访问不等于该 VA 到达该 PA**：`ObjectPhysicalAccessEligible` 只回答「CPU 能不能碰这段物理字节」，不回答「内核用的那个 VA 是否解析到这段字节、权限与属性是否满足合同、是否存在别名」。`Resolve` 必须是**区间级**判定：起点相等只证明一个点，而一次访问的中间页可能切换到别名或属性不兼容的映射——区间逐字节对应才排除这种情形。这与设备侧要求 `VirtualByteRange` 与 `PhysicalByteRange` 并存是同一条规则。`use.pa_start` **必须来自独立的对象访问输入**，不能先查当前 PTE 得出 PA 再把它当成预期 PA——那是循环证明。

`ExistingPageTableObjectEligible(object, ctx)` 因此是五项合取，缺一项都不成立：① 对象身份与存活登记；② `PageTableBackingEligible`（隐式遍历、PTE 更新与 A/D 方案）；③ 实际使用的 VA → PA 路径（`Resolve(va_range, tree, phase) = expected_pa_range`，区间级）；④ 显式 PTE 读/写与隐式遍历两类访问都登记；⑤ 权限、属性与别名审计。

一个对象可以跨过两页，其中一页不作为该用途的分配候选，而对象自己的既有访问点仍可能全部合法；反过来，整页合格也证明不了某个跨越页尾的对象访问合法。**页级谓词与对象级谓词不得互相冒充**：`PmmRoleEligible` 回答「能不能把这一页交给该用途」，`ObjectAccessEligibility` 回答「这个对象现在经这些 VA 是否真的可用」。

| 访问类别 | 对齐来源 | 会不会跨页 |
|---|---|---|
| `Load` / `Store` | 该宽度的自然对齐（第一版只准入这一类） | 自然对齐且宽度 ≤ 页大小时不跨页 |
| `Lr` / `Sc` / `Amo` | 该宽度的自然对齐 | 同上，但另受预约集几何约束（见原子用途） |
| `Fetch` | **架构的指令起始对齐**，不是指令宽度（RISC-V 为 IALIGN：基座 32 位、含 `C` 时放宽到 16 位） | **会跨页**：32 位指令可以从 2 字节对齐处开始 |

**取指不能套用「宽度 = 对齐」**：RISC-V 的 `IALIGN` 在基座 ISA 是 32，含压缩扩展（`C`）的机器放宽到 16，因此 32 位指令可以从 2 字节对齐处开始——**一条指令可以紧贴保护边界**（规范在讨论 `ILEN > 32` 的取指单元时明确承认这一点，该句位于 NOTE 内，不是编号规范条目；逐字原文见 `03`）。于是页尾 `page_end − 2` 处的 32 位指令 footprint 是 `[page_end−2, page_end+2)`，跨两页。x86 的可变长指令（最长 15 字节）同理，该条按架构事实使用，本方未取 Intel SDM 原文，登记为待核。

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

**PMP 不是逐字节权限的交集，而是「整次访问」的判定**：条目静态定序，命中 footprint 内任意字节的**最低编号**条目决定该次操作成败，且该条目必须覆盖操作的**全部字节**，与 L/R/W/X 无关（`norm:pmp_entry_priority`、`norm:pmp_full_match_required`；逐字引句见 `03`）。所以判定输入必须是整次访问：基址 + 宽度 + 对齐 + 特权级 + 路径。

**`RequiredPermission(req)`：操作 → 许可位的显式映射**（**本表是 RISC-V adapter 的展开**；共享层 `operation` 用语义名，见上）——逐行钉住规范原文并给锚点。此前只写「按 `(privilege, operation)` 检查 L/R/W/X」，没有说每种操作查哪一位：

**`RequiredPermission(req)`：操作 → 许可位的显式映射**——**本表是 RISC-V adapter 的展开**，共享层 `operation` 用语义名。逐行结论：`Load`/`Lr` 查 R；`Store`/`Sc`/`Amo(·)`/cache-block zero 查 W（规范把 AMO 与 cache-block zero 归到 store 侧）；`Fetch` 查 X；隐式页表读查 R 且有效特权级固定为 S；硬件 A/D 隐式写查 W（依据是地址翻译算法里那句**无独立 norm 锚点**的 store 条款，强度低于编号规范条目，且同一步要求「比较—置 A/D—store」原子完成）；软件更新 PTE 同一张表、`path = Explicit`。逐行锚点、引文与三条配套论证（AMO 族与宽度归 `PmaPermits`、`privilege` 只覆盖 `{S,U}` 及其三条推论、PMP 与 PMA 并行）见 `03` 六十二节。

**两条必须留在此处的政策结论**：① **AMO 的族、宽度与 `Ziccamoa`/`Ziccamoc` 类主存义务全部由 `PmaPermits` 承担**——拿 PMP 可写当「支持该 AMO」是错误准入；② **cache-block 三类操作第一版一律 `UnsupportedAccessOperation`**（权限表能表示的操作，输入模型也必须能表示）；将来启用时准入必须按**整个 cache block** 判并检查块内 PMP 位与 PMA 一致、W⇒R，否则落入行为 UNSPECIFIED 分支。**Smepmp 第一版只接受 `MML=0 ∧ MMWP=0`**：`MML` 改变 `pmpcfg.L` 的解释、原保留编码变成 *Shared-Region*，`MMWP` 改默认拒绝方向，**同一张 R/W/X 表在 `MML=1` 下整张是错的**，故 `MML=1`、`MMWP=1` 或 `mseccfg` 不可读一律 `SmepmpUnsupportedConfiguration`，不做「按标准语义近似」；`mseccfg` 与 `pmpcfg*` 同样只能 M-mode 访问，因此这一格归 `PmpEvidence.provider`，不能靠探测。
**`Absent` 不等于「没有物理内存保护」**：规范允许零条 PMP（`norm:pmp_entry_count`），而同节的 NOTE 逐字写着平台**可能在标准 PMP 之外或代替它**提供其它物理内存保护结构。因此 `PmpImplementationState = Absent` 只表示**标准 PMP 单元不施加额外限制**，不证明平台没有别的保护结构。第一版两条出路，必须择一登记进 `PmpEvidence`：① 平台 profile 给出带实现名与版本的支持限制声明——「本 profile 下该区间除标准 PMP 外无其它物理访问限制」；② 拿不出该声明 → 证据不足 → fail-closed。把 `Absent` 直接写成 `PmpPermits => true` 而不附支持限制，等于把「没观察到保护」写成「没有保护」。

**完整平台保护清单是独立前提，`Implemented` 同样需要**：规范允许平台在标准 PMP 之外另有保护结构，所以「标准 PMP 证据齐全」只覆盖它自己那部分。补：

```
PhysicalProtectionInventory { standard_pmp, additional_protection_structures, completeness, scope }
    ← completeness：「无未建模结构」还是「仅覆盖已声明结构」；scope：每个额外结构绑定适用 CPU、物理范围、
      访问类别与权限、生效配置版本与稳定性承诺——否则证明不了 PlatformProtectionPermits 对特定访问为真
PhysicalAccessPermits = PmpPermits ∧ PmaPermits ∧ PlatformProtectionPermits
```

第一版对每个 `PlatformMemoryProfile` 二选一：证明不存在未建模的额外保护结构（此时 `PlatformProtectionPermits` 由该证明通过），或把已声明结构纳入 `PlatformProtectionPermits` 逐区间判定。`completeness` 只能由 profile 提供，**不能默认为真**。

**待补测试的覆盖要求**：必须按**用途**分别判定，不得收敛成「全局原子/全局可访问」的粗暴结论。以下十一组各至少一枚负控，逐组的具体断言与已有测试名清单见 `03` 六十二节——**属性兑现**（MAIR/`AttrIndx`、PAT/MTRR 有效类型）、**基底与用途分层**（普通读写被拒 vs 仅隐式页表访问被拒、PMA 允许读写不允许页表访问、AMO 缺失、实现自定义排序、硬件页表读未登记）、**起始地址与对齐**（页内某起始使 footprint 越条目边界、页尾非对齐访问、半页落域外）、**取指**（页尾 2 字节起的 32 位指令：页级排除该起始、对象级判两页）、**快照与合同版本**（`FinalMap` 扩容失效、`legal_starts_contract` 切换使投影重建、参与 CPU 集合新增更严 hart）、**PMP 证据与保护清单**（Smepmp 三态、`Absent` 缺支持限制、`Implemented` 缺完整性、provider 只承诺事后通知）、**顶端边界**（`end == 2^64` 合格、越界拒绝）、**原子与几何**（`RsrvNonEventual` 一律拒绝、几何不满足声明 profile、`HardwareManagedAd` 缺隐式更新任一项、cache-block 三类）、**Profile**（声明 RVA23S64 而 `Svade` 未实现即不合格）、**空集前置**（参与集合/需求集合/起始集合/算法清单任一为空即 `InsufficientEvidence`，而 `GeneralData` 的空特有集合不得失败）、**对象访问与映射**（物理准入通过但来源未授权、多段翻译只登记一段、区间中段落别名、叶子与 `DenyMapping` 相交）。


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
ArchitectureAdmissionDenial {      ← 归因记录，不是集合：PmmBaseExcluded 是集合，补集不会自动产生归因
    page_range,                          ← 按相同证据与原因可合并范围，但任意一页都必须能查出全部理由
    reason, state, cpu_scope, failed_requirement, evidence_id, admission_context_id, source_refs,
}
# （用途层归因记录 RoleAdmissionDenial 的字段清单移入 `03` 六十五节；它是分配器的诊断能力，不作交棒验收条件）
```

`DefinitelyIneligible` 是已有证据证明不能访问或不满足目标语义（平台事实）；`InsufficientEvidence` 是无法证明合格、按 fail-closed 排除（证据缺口）。两者都导致排除，但**诊断不能合并**——前者要做平台决策，后者要做补证，处理路径完全不同。
**四类排除原因必须分开，不能塞进一个未定义的 `AllocationReservations`**：① 类型资格（`UNUSABLE`/`UNACCEPTED`/`ACPI_NVS`/`Runtime` 等在阶段 `p` 是否允许分配）——构造规则是**先裁决、再取整页**：

```
ResolvedTypeEligibleRanges(p) = ResolveCompatibleTypeClaims(SourceRecords, p)
TypeAllocatablePages(p)       = FullPages(Union(ResolvedTypeEligibleRanges(p)))
```

不能对每条描述符单独 `FullPages` 再并集：两条相邻、语义兼容的来源应能合成一个整页；一条普通 RAM 与一条不可用类型相邻或重叠，则整页不合格。`TypeAllocatablePages(p)` 由「普通主存 backing 资格 + 阶段分配资格」共同派生，不是只看 EFI 类型数字；② 显式分配保留（`/reserved-memory`、MRB、固件前缀）；③ 存活对象（启动模块、页表、DTB 副本、栈、登记表）；④ 平台拒绝。反例：一条 `UNUSABLE` 描述符没有 DTB 声明，若只看显式保留，它会被算进可用表——与 O-06 冲突。

先并集再取整页，避免两个非页对齐的相邻 RAM 区间各自向内收缩时白丢一页；来源记录与身份**不合并**，只在纯计算层构造集合。同一页被多条排除原因覆盖时，PMM 只排除一次，但**所有排除理由都可追溯**（见 O-12g 的多重归属）。
**结构化候选（非阻断，已登记）**：`SourceRecord.range` 的语义要读 `kind` 才能解释——输入声明是原始范围，`FirmwarePrefix` 保存的是**派生范围**。可考虑拆成「输入声明」与「派生主张」两个变体，让 `declared_range` / `derived_range` 的差别在类型里表达；第一版不改，登记为重构候选，理由是不能为了形式统一在切片中途改公共字段。

**原始声明、RAM 交集与政策投影是三个范围**：`RawDeclaredRange` 是输入声明的完整字节（不裁剪、不取整，DTB 侧已落为 `SourceRecord.range`，派生记录的该字段是派生结果而非原始声明，记录本身**不携带任何页范围**）；`RamIntersection` 是与 RAM 来源域相交、按页向外量化的部分，用于 PMM 投影；`PolicyProjection` 是按 `no-map`、MRB、对象各自语义生成的禁止范围。**MRB 即使超出 `/memory` 也不能丢**：它的访问禁止语义不因为范围不在 RAM 内而消失。`no-map` 与标准系统映射相交时由独立的映射禁止投影处理；若项目只支持与 RAM 相交的 `no-map`，超出部分必须是显式拒绝条件，不能静默裁剪。今天两张投影表只表达 `RamIntersection`，`PolicyProjection` 尚无消费者——这正是 O-12g 记录层要先行的原因。范围表示的三层与今天代码：

| 层次 | 现状或目标 |
|---|---|
| DTB 原始输入 | `address + size` 两个 cell，保留原始编码（cell 数由父节点声明，规范化层才求和） |
| 今天的 `SourceRecord` | 已是 `range: PhysicalRange`（`{ start: u64, end_exclusive: u128 }`）：**记录层能表达 `2^64`**，能否交付由投影层判定 |
| 规范化物理范围（已落，DTB 腿） | `PhysicalRange { start: u64, end_exclusive: u128 }`，可表达 `[0xffff_ffff_ffff_f000, 2^64)`；UEFI 腿尚未接入 |


**MRB 的两种粒度**：精确 byte range 用于访问义务与审计；要通过页表保证不可访问时，另算相交页级范围，并说明「只覆盖一页中部分字节」时该页在其它对象的普通路径中是否允许映射。

**设备窗口的边界页负控（必须有测试）**：资源 `[0x1003, 0x1fff)` 若直接向外量化成 `[0x1000, 0x2000)`，授权就扩张到了相邻地址。边界页只能三选一：拒绝非页对齐资源；明确授权整页并证明邻接区的属性/权限/别名兼容；拆分后仍无法表达则拒绝启动。

