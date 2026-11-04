# 内存图政策评审简报 —— 审阅历史档案

> **创建**：2026-11-04（第八轮评审回填之后）
> **来源与完整性**：本文件的内容全部从对外评审简报 `01-adjudication-brief-memmap-open-items.md` **逐字移出**，未改写、未删减。移出原因只有一个：正文已经长到无法整份送交外部评审人，而下面这些部分属于**记录**，不属于**待审内容**。
> **与正文的分工**：正文保留当前政策状态、合同条款、待裁问题、事实分级与复核锚点；本档案保留二十轮各自改变了什么、双方各自撤回过什么、本方承认过哪些错误，以及被后续轮次替代的阻断表与取证表。
> **一条不可反悔的约定**：本档案里的条目**不会因为被移出正文而失效或被降级**。凡已被后续轮次撤销的判断，这里保留原文并就地标注撤销原因——这正是正文 §7.1 从第四轮起强制要求的「改动需要被看见」，现在同样适用于这份文档自身的结构变化。评审人若质疑「这条政策为什么长这样」，答案在这里，而不在正文里再写一遍。
> **阅读顺序**：不必通读。需要核某一条政策的形成过程时，按正文里的「第 N 轮已改」标记到本档案对应小节查。

---

## 一、各轮评审带来的正文变化（按轮次，最新在上）

**本轮（第五十三轮评审之后：歧义位 fail-closed、O-12k、组合规则）**

- **代码 `694b74386`**：`0x0001_0000` 在 2.11 PDF 是 `HOT_PLUGGABLE`、在规范仓库/EDK2/Linux 是 `MORE_RELIABLE`——此前按后者放行，按 PDF 构建的固件会把热插拔内存送进 PMM。现在该位在已知位闸之前单独拒绝（`PolicyError::AmbiguousAttribute`），负控 `the_ambiguous_bit_fails_closed_under_either_name`；`u64::MAX` 断言相应掩掉歧义位。boot-shim **68**、minix-boot **132**。
- **删除两处「Reserved 可覆盖 BootServicesData」的泛化表述**：普通静态保留 + MRB 同页一律拒绝；总规则改为「逐页生成主张，只有裁决表明确认兼容的才能合并，规范强制要求不得被另一个来源的项目强化静默覆盖」。
- **新增 O-12k**：UEFI 内存图快照与 `MapKey` 一致性——先完成所有会改变内存图的分配，再取最终快照；`ExitBootServices()` 失效重试须重建来源记录与政策投影，不能只换 key。
- **组合规则**：`EfiReservedMemoryType` + `no-map` 时第一版**不开放** `DriverControlledUse`（绝对禁止），组合测试列为待补。
- **其他**：UEFI 合法记录与拒绝诊断记录分开；aarch64 UEFI 无配置表 DTB 时「仅依 UEFI 内存图启动、不做 DTB 交叉验证」；riscv QEMU 用例明确只覆盖 bootface；O-12h 顶端页五点测试清单；`DeviceMemoryRegionReference`/`ReclaimAuthority` 命名与「引用≠所有权」。

**本轮（第五十二轮评审之后：纯文档——类型冲突裁决、缓存资格、拒绝策略）**

- **普通静态保留 + MRB 同页 = 拒绝**（第一版）：一条 EFI 描述符无法同时满足 `must`（BootServicesData）与本项目的 MRB 强化（Reserved）；并写明若要接受该组合，唯一自洽路径是放弃 MRB 的 `should`→must 强化，而不是让裁决函数隐藏牺牲项。
- **PMM 公式补 `CacheAttributeEligiblePages(p)`**：无 WB 的 `CONVENTIONAL` 不再可能出现在可用集合里；UEFI 路径取描述符属性资格、DTB 路径取平台属性政策，不伪造属性记录。
- **O-11 矩阵收紧**：`no-map` 与 `EfiReservedMemoryType` 中的 DTB 读取第一版**一律拒绝**（前者例外属设备驱动受控使用，后者指针不足以推翻 `DenyOSAccess`），不再写成「须证明」的开放条件。
- **`reusable` owner 措辞修正**：去重保留的是**声明身份**，owner 关系尚未建立；未来是引用关系集合（多对多），不是单个 `OwnerId`。
- **`collect()` 的 MRB 归因缺失**写入「不保证什么」：MRB 从 conventional 扣除却不进 reserved，旧接口无法表达其来源；`dtb_collect_and_collect_with_sources_agree` 只证投影一致，不证来源完整。
- 本轮**无代码改动**：纯文档提交走 lint/表格/行数三门；代码基线仍 `c5f1789b8`，132/67。

**本轮（第五十一轮评审之后：读取合同与类型裁决收口）**

- **代码 `c5f1789b8`（测试切片）**：新增 `build_fdt_multi_reg` 夹具与 `dtb_identical_memory_tuples_are_deduplicated_not_rejected`——单节点两个相同 `reg` 元组时，全景只留一份、两条来源记录各留（origin `{node:0, reg:0/1}`）。此为纯测试提交，生产代码未动。minix-boot **132**、boot-shim **67**。
- **O-11 文字矛盾修正**：原句「原件只读一次」与「复制 `[dtb+40, …)`」不能同时成立；改为「header 只在 preflight 读一次、body 只在复制读一次，之后只访问副本」。
- **UEFI↔DTB 类型裁决**：补 `PageSourceClaims`/`ResolvedEfiType`；明确本项目走**项目强化**路径（任一 `no-map`/MRB 主张都要求 `EfiReservedMemoryType`，把 MRB 的 `should` 升为项目强制），`R_static` 与 `R_MRB` 分开定义后再做页级裁决。
- **启动读取用途矩阵**：`BootInputReadUse` 具名化；`no-map` 不得借用 MRB 例外、`EfiReservedMemoryType` 不得因指针存在而解除 `DenyOSAccess`。
- **O-06**：`CONVENTIONAL` 的「可映射/可分配」改为条件列（类型资格 ∧ 缓存 WB ∧ 排除项），`OrdinaryRamEligibility` 不隐含缓存检查。
- **`/memory` 重叠理由**：改述为「严格输入策略」，不再暗示集合模型必然重复计页。
- **O-12j-c 状态表**：拆成 `/memory`（部分闭合）、静态 `/reserved-memory`（部分闭合）、动态 `size`（未闭合，缺专用错误与测试）、设备 `reg`/`ranges`（未开始）。

**本轮（第五十轮评审之后：保留语义三分与 RAM 重叠收口）**

- **代码 `59aa94cf3`**：`/reserved-memory` 的重复判定从「`no-map` 布尔」改为**三语义**（`ReservationIntent::Ordinary/Reusable/NoMap`）——上一版里普通与 `reusable` 会被误判成同语义去重，现在同范围跨语义一律 `ConflictingReservationAttributes`；`/memory` 的部分重叠新增 `OverlappingMemoryRanges` 拒绝（精确重复去重、记录各留一条）。minix-boot **131**、boot-shim **67**；riscv QEMU PASS。
- **顺带暴露的旧夹具问题**：`dtb_multi_region_and_unit_address_variant` 的 `0x8000_0000 + 0x4000_0000` 与 `0x8800_0000` 本来就是重叠 RAM（首段一直盖到 `0xC000_0000`），旧代码不检查所以没发现；夹具已改为不相交区间。
- **文档**：重复矩阵扩为三语义 × 三来源（含 `/memory` 行）；UEFI↔DTB 交叉验证改为**覆盖合同** `Outward(R_reserved) ⊆ ⋃类型兼容描述符页`，并列出三种必测形态（未实现）；`EfiReservedMemoryType` 拆规范语义（`DenyOSAccess`）与项目兑现（`DenyOrdinarySystemMapping`）；O-11 的 MRB 措辞改为「MRB 规则自身允许的显式一次性授权」；补 `LiveInputReservation = Outward(...)`；UEFI 描述符记录最小字段清单。

**本轮（第四十九轮评审之后：MRB 重复改判，四层投影合同）**

- **代码 `b3c93b83f`**：上一轮把「精确重复一律去重」套到了 MRB 上，与 DTSpec 5.3.2「These given regions shall not overlap each other」冲突——改为**拒绝**（`OverlappingMemoryReservations`），测试 `dtb_identical_mrb_entries_are_rejected`；`/reserved-memory` 的重复去重保留为**项目级**策略。minix-boot **127**、boot-shim **67**；riscv QEMU PASS。
- **不变量 1 与 O-12i 统一**：补四层表（来源事实层不合并 → 冲突解析保留身份 → 纯集合投影可 `Union` 后量化 → 归因层可反查），并列出各投影的量化顺序，消除「不得合并成无身份并集」的字面歧义。
- **O-11 四项独立条件**：来源授权、**全范围可达（拿到 `totalsize` 后重新证明整段可读）**、内容一致、自有副本；补源/目标不相交与原始 DTB 存活登记。
- **O-12c**：`DeviceAccessUseSet` 增 `VirtualByteRange`（或可静态解析 VA 的访问点标识）；缺 VA 无法证明访问需求，且不得用 PTE 反推 PA 充作输入。
- **§2.2/O-06/O-03**：`RESERVED`（域外）与 `UNUSABLE`（域内排除）输出分流相同但语义不同，不得合并；`EfiReservedMemoryType` 的禁止语义由 `TypePolicy` 独立产生，不依赖 DTB 记录；`WP` 归入物理保护能力（具特殊缓存语义）；`reusable` 的 owner 来自设备节点 `memory-region` 引用，不能从节点层级推导。

**本轮（第四十八轮评审之后：中立范围字段、派生参数与「重复 ≠ 重叠」）**

- **代码 `09e56a729`**：`SourceRecord.raw_*` 改名 `range_*`（派生记录的该字段是派生结果，不是原始声明）；`FirmwarePrefix { memory_id, cut }` 保留派生参数 `F = firmware_reserve_end`；精确重复的 `/reserved-memory` 声明与 MRB 条目改为**投影去重、来源记录各留一条**，部分重叠仍拒绝，同范围但语义不同（`no-map` vs 普通）按 `ConflictingReservationAttributes` 拒绝。minix-boot **127**、boot-shim **67**；riscv QEMU PASS；aarch64 仍被外部 `E0046` 阻断。
- **补上反向完整性**：`dtb_every_input_declaration_has_exactly_one_record`（每条输入元组恰好一条记录、记录不多不少），与既有的「每个投影区间都有来源」互补。
- **文档**：§2.1 写明前缀公式 `M_i ∩ [0, F)`、`node`/`reg` 序号计数规则、按 reg 元组计容量、重复/重叠/冲突三分裁决；O-12g 把「一律拒绝重叠」与去重规则的旧冲突标为已解决。

**本轮（第四十七轮评审之后：记录区分输入声明与派生前缀，身份补齐）**

- **代码 `b49afa97d`**：`SourceKind::class()` 区分**输入声明**（`Memory`/`MrbEntry`/`ReservedOrdinary`/`ReservedReusable`/`ReservedNoMap`）与**派生保留**（`FirmwarePrefix`）；`origin` 补齐 `MemoryNode { node, reg }`、`ReservedMemoryChild { node, reg }`，派生前缀用 `FirmwarePrefix { memory_id }` 指回它切分的那条 `/memory` 记录。minix-boot **123**、boot-shim **67**；riscv QEMU PASS；aarch64 仍被外部 `E0046` 阻断。
- **新测试**：`dtb_declaration_cap_is_shared_across_reservation_kinds`（普通/reusable/no-map 共用 8 条上限）、`dtb_source_capacity_is_reachable_at_exactly_forty`（40 条恰好可达）、`dtb_firmware_prefix_links_to_its_memory_record`（派生链可追）。
- **身份命名空间**：`id` 是输入对象内的局部 ID；跨输入（DTB + UEFI）引用必须是 `(InputObjectId, LocalSourceId)`——写进 §2.1，未落码。
- **文档矛盾修正**：六类 kind vs「五类」、「不区分 `reusable`」、§2.1 测试清单补新测试；`OrdinaryRamEligibility` 收窄为**普通主存映射资格**，分配/访问/性质各归其位；`uefi-raw 0.9` 行展开为 `WRITE_PROTECT = 0x1000`、`MORE_RELIABLE = 0x10000`（crate 常量）、`HOT_PLUGGABLE` 未定义；O-12i 补端点统一（DTB `u64` + `RangeOverflow` vs UEFI `u128`，待改为独立错误）；§6 增加**证据分层表**（宿主单测 / riscv 启动 / aarch64 组装 / aarch64 启动），aarch64 启动行为明确记为未验证。

**本轮（第四十六轮评审之后：记录层去掉预置页范围，补身份）**

- **代码 `9a4d0eb51`**：`SourceRecord` 改为 `{ id, kind, origin, raw_start, raw_end }`——删掉 `page_*`，因为 `/memory` 要内向 `FullPages`、保留声明要外向取整、MRB 访问禁止要精确字节，一份预置页范围会把某个消费者的规则写进所有消费者。`kind` 拆出 `ReservedReusable`（`reusable` 不再并进普通保留），MRB 更名 `MrbEntry`；`origin` 记录 `MemoryNode`/`FirmwarePrefix`/`MrbEntry(n)`/`ReservedMemoryChild(n)`。minix-boot **120**、boot-shim **67**；riscv QEMU PASS；aarch64 QEMU 被外部红阻断（见下）。
- **容量依据改写**：40 由**输入上限**推出（`/memory` 段与声明各受 `DTB_MEMMAP_MAX`、MRB 受 `DTB_MRB_MAX`；`no-map` 同样计数），不再用「投影表装得下」论证。新增负控 `dtb_no_map_declarations_count_against_the_declaration_cap`：9 条 RAM 外 `no-map` 不进任何表，第 9 条仍必须拒绝——这恰好推翻旧的容量论证。
- **一致性测试**：`dtb_collect_and_collect_with_sources_agree` 证明两条入口产出相同两张表。
- **启动路径如实登记**：四条 DTB 调用点全部走 `collect()`，记录被丢弃；§2.5 改为「三个层次」表（库接口 / 启动路径 / 消费者），`grep collect_with_sources` 零命中。
- **外部红登记**：`rewrite` 上 `2d292a06b` 给 `SefIpc` 加 `sendrec` 未同步 `servers/rs`，aarch64-unknown-none 编译失败（E0046 at `servers/rs/src/lib.rs:203`），aarch64 QEMU 腿本轮无法运行；与本切片无关，待其所有者修复。
- **O-06 分层**：新增 `MemoryKind` / `OrdinaryMappingEligibility(tree, phase)` / `PmmEligibility(phase)` / `AccessRequirement` 说明；表头「普通主存 backing」改「普通主存性质」；`CONVENTIONAL`/`LOADER_*`/`ACPI_RECLAIM` 的「可能被保留声明覆盖」改为「按兼容矩阵裁决」。O-11 动态 `size` 统一为「不应列入内存图；`reg` 与 `size` 并存按 `reg`」。O-12g 状态改三层表；O-12i 注明记录不携带页范围。

**本轮（第四十五轮评审之后：采集边界交出逐条来源记录）**

- **代码 `28e277d5b`**：`memmap_source` 增加 `SourceRecord` 层——五类（`Memory`/`FirmwarePrefix`/`MemoryReservation`/`ReservedOrdinary`/`ReservedNoMap`），容量 40，`raw_*` 不取整不裁剪、`page_*` 供投影；`collect_with_sources()` 交出三张表，`collect()` 语义与签名不变。minix-boot **118**、boot-shim **67**；riscv bootface 与 aarch64 UEFI QEMU 双 PASS。
- **为什么这是 O-12g 的第一步**：此前 `no-map`、RAM 之外的保留声明、MRB 原始条目出了两张投影表就无从追溯；后续禁止集合没有输入。记录层先证明「信息在边界上不丢」，再谈投影。
- **O-11 拆成两个前置条件**：`BootInputProvenance`（启动 ABI 授权）与 `BootInputReachability`（当前转换下 `[dtb, dtb+40)` 整段可读）不能互推；补「header snapshot 只读一次、只复制剩余字节」的复制算法与「交接后不得并发修改」的输入合同。
- **O-12h 改判**：`2^64` 独占末端是合法的最后一页，结构校验改用 `u128` 接受，拒绝下移到平台准入层。今天的代码仍以 `RangeOverflow` 拒绝，标为待改行为。
- **O-06 收紧**：「来源叠加」改为兼容矩阵（`EfiBootServicesData`↔普通保留、`EfiReservedMemoryType`↔`no-map`、其它类型不一致即拒绝）；新增第一版处置列（`UNACCEPTED`/`PAL_CODE`/`PERSISTENT`）；`RamBacking` 更名 `OrdinaryRamEligibility`，与物理支撑性质分开。
- **其他**：O-12c 观测单元补 VA/PA/叶/权限/属性/共享性/树/阶段；O-12i 拆 `RawDeclaredRange`/`RamIntersection`/`PolicyProjection`；O-05 冻结对象收窄为 `FrozenBootMappingSet`；UEFI 属性位给出自包含矩阵并标出待钉的 EDK2/Linux commit；§2.7 措辞改「本节的函数和映射构造位置」；§6 基线改 `28e277d5b`、118/67。

**本轮（第四十四轮评审之后：预扫与解析库消费能力对齐）**

- **代码 `d615d0190`**：`validate_structure` 增加**属性必须先于子节点**（`PropertyAfterChild`），深度上限 **64 → 63**；fdt_layout 测试 28 条；minix-boot **114**、boot-shim **67**；riscv bootface（OpenSBI 链）与 aarch64 UEFI QEMU 双 PASS。
- **撤回上一轮的一个事实判断**：上一轮写「属性位于子节点之后是真实固件形态（QEMU/OpenSBI 打补丁即如此）」，并据此让预扫显式接受。本轮读 `fdt 0.1.5` 源码：`properties()` 在首个非属性 token 停；`children()`/`all_nodes()` 在「子节点后属性」处**直接结束迭代**——接受该形态等于让树被静默截断。随后用 QEMU 8.2.2 `dumpdtb` 全树扫描（0 处违规）与两条 QEMU 腿在「拒绝」规则下 PASS 证明：上一轮的观测来自有缺陷的临时扫描器，不是固件事实。判断收回，测试 `accepts_a_property_after_a_child` 改为 `rejects_a_property_after_a_child`。
- **深度边界来自依赖实现**：`all_nodes()` 父栈 `[&[u8]; 64]`，根存下标 1，安全上限是 63 而非 64。新增 `the_parser_consumes_the_deepest_tree_this_gate_accepts`：门接受的最深树，`all_nodes()` 必须能走完。
- **O-11 补第三段**：`fdt_layout::validate` 不只读 40 字节 header，还要扫 MRB/结构块/字符串；完整容器校验只能在**自有副本**上做。§2.1 同时写明今天仍是原地址就地消费、未复制。
- **O-12g 前置**：原始来源记录（逐条声明、MRB 条目、未输出描述符）今天**没有跨过采集边界**，必须先落记录层再谈禁止集合编译。
- **UEFI 属性位证据改支**：按规范仓库源文件 + EDK2 的编码取 `HOT_PLUGGABLE = 1<<20`，不再写「即 2.11 PDF 的位值」；三份文本与 EDK2 编码互不一致（03 号文档第十五节）。
- **文档**：§6 测试清单按代码重新生成（28 条，去掉幽灵条目与重复）；基线改 `d615d0190`；`DeviceRequiredAccessRange` 统一为 `DeviceRequiredAccessEnvelope`；`PERSISTENT` 拆分「有 backing」与「第一版无普通资格」；§1.2「内存图兑现固件保护」改为「权威描述来源」。

**本轮（第四十三轮评审之后：预扫在禁用跳转表后落地）**

- **代码 `0464a1e5c`**：结构块预扫真正落地（token/树形/名称与属性边界/深度/END），27 条 fdt_layout 测试；minix-boot **113**、boot-shim **67**；riscv bootface 与 aarch64 UEFI QEMU 双 PASS。
- **跨过早期执行障碍**：评审人给的 `-C no-jump-tables` 在本工具链（rustc 1.94.1）不存在，实际拼写为 **`-C jump-tables=no`**；且 `RUSTFLAGS` 会覆盖 target config，故 **xtask 在注入的 RUSTFLAGS 中同时带上该选项**，config 另为 riscv64 与 aarch64 直启目标保留一份。反汇编确认 `validate_structure` 只剩直接 `jalr`；去掉选项时上一轮的原故障可复现（已留存读数）。
- **新增合同**：`EarlyExecutionClosure = Instructions ∪ ReadOnlyDataDependencies ∪ RequiredWritableState`——某阶段执行一段代码前，必须证明这三类在当前转换与权限下全部可达，只证明入口可执行不够。这是本轮故障的直接产物。
- **文档**：O-12j-a/b 均标已实现（c 仍开放）；O-12i 的 `PageBytes` 改为右端独占并写明 `0 ≤ n < 2^52`；§2.1 写清 `raw → validate → slice → parser → collect` 的真实调用链；O-12c 合并两段重复的 PLIC 表述；§6 统一 113/67 并把 fdt 测试名补全为 27 条；基线改 `0464a1e5c`。

**本轮（第四十二轮评审之后：结构预扫受阻的实证 + 容器校验收口）**

- **代码 `51b784cb3`**：容器校验补 **MRB 条目上限 16 条**（`TooManyReservations`，有界性在扫描函数自身）与 **strings 块非对齐的正向测试**；minix-boot **104**、boot-shim **67**；riscv 与 aarch64 UEFI QEMU 双 PASS。
- **O-12j-b 的预扫尝试与阻塞（实证）**：实现了无分配的 token 预扫（token 合法性、单根、嵌套平衡、属性/名称边界、深度 ≤64），riscv bootface 在真实 DTB 上 `fault_fetch`（QEMU `cause=1`，`epc` 落在 `validate_structure` 的 `jr` 处）；`match` 与比较链两种写法都被 LLVM 还原为 `.rodata` 跳转表，而该引导阶段执行的映射不覆盖跳转表的绝对地址。预扫已回退；可行方向：移到分页/高半生效后执行，或提供无跳转表的编译形态。
- **文档**：O-12j-a 更新为 18 条测试（含字符串块非对齐与 MRB 上限）；O-12j-b 记录上述阻塞；O-12c 补 `RiscvVirtPlatformMmioWindow` 的资源清单（8 个 VirtIO、条件 UART1、fw_cfg）与 `DeviceWindowPolicy` 完整字段；O-06 把属性分为缓存/保护/其它三类（`WP` ≠ `RO`、`SP` 不产生禁令）并给 `CONVENTIONAL` 加 WB 条件；O-12i 的 `PageBytes` 改为先转 `u128` 再加一、写明 `0 ≤ n < 2^52`；§2.7 复核口径纳入 `fdt_layout::validate`；§6 统一 104/67。

**本轮（第四十一轮评审之后：FDT 版本子集、结构块边界与平台窗口授权）**

- **代码 `23feb9bc0`**：FDT 版本收紧为 `version=17` + `last_comp_version=16`（明确是项目输入子集，规范要求客户端接受向后兼容版本）；新增 `off_dt_struct` 4 字节对齐、结构块以 `FDT_END` 结尾、`MRB → structure → strings` 完整顺序检查。测试 16 条；minix-boot **102**、boot-shim **67**；两个 QEMU 腿 PASS。
- **文档**：UART 的 2 MiB 包络改名为 `RiscvVirtPlatformMmioWindow`（窗口内还有 VirtIO 与 fw_cfg），并给出通用规则「包络授权对象是平台窗口，不是窗口里的某个设备」；PLIC context 推导链写明 specifier ≠ `addrid`；新增 `PlatformMemoryProfile` 记录（四类窗口的 QEMU 版本均标为待补）；`DeviceAccessUseSet` 补八个字段；删除一处空表头；`no-map`/MRB 措辞改为「页表投影相似、语义不同」；`FullPages` 改为页编号 + u128 定义；MRB 中文名改为「内存保留块」；§2.7 补五个调用点表（含「是否参与内存政策」一列）；§1.3 说明「可分配但不许映射」是模型状态、v1 无此对象；§6 测试名与计数统一为 16 / 102 / 67，基线改 `23feb9bc0`。

**本轮（第四十轮评审之后：O-12j 三分 + 设备覆盖的来源独立 + 地址可达性）**

- **代码 `3333d428c`**：`fdt_layout::validate` 增加 FDT 格式版本策略（只接受 17，`last_comp_version ≤ 17`）与**项目块顺序**（MRB 在 structure 之前；规范该顺序是 `should`，按输入限制执行）；新增 3 条测试（旧/未来版本、MRB 在结构块之后、MRB 落入 header）。minix-boot **99**、boot-shim **67**；riscv bootface 与 aarch64 UEFI QEMU 双 PASS。
- **O-12j 拆三层**：a 容器与 MRB（已实现，14 条测试）；b 结构块 token/属性解析（未闭合——`fdt 0.1.5` 含 `unwrap`/`expect` 与直接切片）；c 目标节点语义（未闭合）。
- **§2.1 归属改正**：`DtbMemoryMap` 自身不做头部校验，五条输入路径在解析库前由 `fdt_layout::validate` 完成；不再写成「未检查」。
- **O-11 改名并补地址域**：`BootInputReachabilityAuthorization`（PA/VA、当前转换状态、`[dtb, dtb+40)` 整段可读）与 `ValidatedObjectReadAuthorization`（完整对象只读并复制）。
- **O-12c**：覆盖改为 `ExpectedDeviceMappingCoverage`（构造推导）与 `ObservedDeviceMappingCoverage`（PTE 遍历，未实现）；APIC 地址绑定 x86 profile（xAPIC/x2APIC、`IA32_APIC_BASE`、MADT 一致性）；PLIC 拆 `DeviceAccessUseSet` 与 `Envelope`，并明确 QEMU 版本尚未钉住。
- **O-12i**：类型资格改为「先 `ResolveCompatibleTypeClaims`、再 `FullPages`」；O-12g 重叠措辞改为「同源去重 / 跨来源冲突」；§4 默认 R/NX 限定到「已获普通 RAM 映射授权的保留对象」；§6 列出 14 条测试名与五处调用点。

**本轮（第三十九轮评审之后：FDT 容器校验落地 + 范围四列化）**

- **代码 `1a9e2a14f`**：新增 `minix_boot::fdt_layout::validate`——解析前检查 magic、`totalsize`、blob 8 字节对齐、`struct`/`strings` 块 checked 边界与互不重叠、MRB 偏移对齐、`{0,0}` 终止项落在 `min(off_dt_struct, totalsize)`；返回块范围，调用方截到 `totalsize` 再解析。五处 DTB 入口接入（riscv/aarch64 bootface、OpenSBI、aarch64 UEFI 两处堆副本）。10 条测试；minix-boot **96**、boot-shim **67**；riscv bootface 与 aarch64 UEFI QEMU 双 PASS。
- **O-12j 关闭**：容器校验替代「只做 MRB 有界扫描」；O-12g 依赖解除。残留登记：节点遍历仍依赖库。
- **文档**：O-12c 改**五列**（补 `DevicePolicyAuthorizedRange`，写明「授权不能等于实际叶子」）；UART 资源按实测 DTB 改 **256 B**；PLIC profile 收紧为单 socket + AIA 关闭 + TCG + 明确 CPU/hart 条件；O-06 拆 `LOADER_CODE`/`LOADER_DATA`、`BOOT_SERVICES_CODE/DATA` 四行并写明默认 NX；§4 权限改为「普通保留默认 R/NX，W/X 由对象义务登记，不支持即拒绝」；O-06 补 `EfiReservedMemoryType` 域外但来源身份仍有效；O-12i 补 `FullPages` 定义、`TypeAllocatablePages` 派生与 `no-map` 部分页对称规则；O-12g 区分同语义重复与异语义冲突；O-12h 补版本策略与 `2^64` 独立错误码；§6 统一 **96/67** 并标注 `38668f843` 是功能提交；§7 把 O-01 归入来源保真组，改名为「第一轮实现复审的起点」。

**本轮（第三十八轮评审之后：DTB 首访边界 + 范围四层化 + 证据基线统一）**

- **代码 `a52cfee54`**：`minix-boot` 新增 `dtb_blob_aligned` / `dtb_len_ok`（40 B–64 MiB）；riscv/aarch64 bootface 与 OpenSBI 腿在首次解引用前检查 8 字节对齐；OpenSBI 腿补 `totalsize` 上限与 checked 加法（先验证 totalsize 再建切片）。minix-boot 86、boot-shim 67；riscv bootface QEMU PASS。
- **文档**：O-12c 改四层（`DeviceAperture` / `DeviceRequiredAccessRange` / `DeviceWindowPolicy` / `DeviceMappingCoverage`），IO-APIC 与 LAPIC 写成两个相邻包络，PLIC 范围绑定 QEMU/accel/CPU/`-smp`/S-mode 配置；O-12i 声明所有 `PMM*` 为 4 KiB 页集合、资格变化由显式事件触发；O-12g 补 `/memory` 重叠规则与「原始记录保留 `UNUSABLE` 等不输出区间描述符」；O-06 声明第一版不提供 Runtime Services；O-05 补「政策审计 ≠ 硬件同步」；§1.5 补 PMA 机器绑定与 MTRR 逐核；O-12h 补 `VirtualStart=0`、双末端 u128、缓冲区长度；O-12j 写入 `fdt 0.1.5` 的逐条核对结果（只查 magic/长度，不查对齐与偏移边界）；O-11 改「四条 DTB 输入路径」并写明 40 字节 header + 对齐先于读取；§6 基线统一为 `a52cfee54`/86/67 并列四条复核命令；§7 改为六个阻塞组。
- **过程更正**：第三十七轮的正文补丁（O-12j/O-12g/§6/§7）在「reset 后只重放 docA」的恢复操作中丢失，本轮已补回；教训记入：reset 后必须重放**全部**补丁，不能只重放最后一个。
- **门**：minix-boot 86、boot-shim 67；riscv/aarch64 目标编译；riscv bootface QEMU PASS。

**本轮（第三十七轮评审之后：PMM 分区公式补全 + 设备映射包络）**

- **代码 `9b2b09f1c`**：PLIC 授权范围改为 **2 MiB 叶子包络** `0x40_0000`（资源 `0x24_0000` 单独保留为 `PLIC_RESOURCE_LEN`），并加编译期断言「2 MiB 对齐 + 不越 QEMU 6 MiB aperture」。
- **文档**：O-12c 拆 `DeviceResourceRange` / `DeviceMappingRange` 两栏（4 KiB 资源实际映射为 2 MiB 包络，授权必须覆盖包络）；O-12i 的 `PMMAvailable(p)` 补类型资格、显式保留、存活对象、平台拒绝四类原因，`PMMExcluded(p)` 取差集；O-03 把 `no-map` 拆成禁止标准映射 / 禁止非受控推测 / 受控驱动使用三项，并写明 MRB 部分页规则；O-12j 写明 `fdt 0.1.5` 的实际行为与「不签」结论；O-12g 补排除原因多重归属与「对象不得合并成 Reserved」；§7 五个阻塞更新；§6 证据基线改为 `9b2b09f1c` 并列出四条复核命令。
- **门**：minix-boot 84、boot-shim 67；UEFI 双目标编译；riscv bootface QEMU PASS。

**本轮（第三十六轮评审之后：PLIC 窗口修正 + 页域公式 + 禁止冲突表）**

- **代码 `f98d4edff`**：riscv64 PLIC 窗口从 2 MiB 扩到 `0x24_0000`（`0x20_0000` 优先/待决/使能区 + 32 hart 的 M/S context）——原窗口恰好结束在 context 区起点，claim/complete 拿不到 `DEVICE` 映射；QEMU DTB 实测 `plic@c000000` 的 `reg` 为 `0xc000000 + 0x600000`。另按 EDK2 `UefiSpec.h` 把 `EFI_MEMORY_HOT_PLUGGABLE` 修正为 `0x0010_0000`（上一轮测试用的 `0x400` 不是该位）。
- **文档**：O-12c 窗口表更新 PLIC 与 aarch64「平台外设窗口」解释，并加版本绑定要求（QEMU/machine version、动态范围公式、与 DTB `reg` 一致性）；O-12i 补 `RamSourcePageDomain = FullPages(Union(...))`、`PMMExcluded/PMMAvailable` 公式与 MRB 两种粒度；O-03 采纳六行冲突组合规则；O-12j 写明「有界扫描仍是未闭合项」及其当前证据链（库按传入 blob 切片迭代、缺终止项由 16 条上限拒绝、对齐未查）。
- **门**：minix-boot 84、boot-shim 67；UEFI 双目标编译；riscv bootface QEMU PASS。

**本轮（第三十五轮评审之后：来源域叙述统一 + 量化合同 + 静态窗口清点）**

- **代码 `53755840b`**：MRB 的零长度非终止项拒绝（`ZeroLengthMemoryReservation`）；UEFI `EFI_MEMORY_HOT_PLUGGABLE`（bit 10, 0x400）负控测试——钉住的 `uefi-raw 0.9` 无常量，携带该位必须走未知属性拒绝。minix-boot 84、boot-shim 67；riscv bootface QEMU PASS。
- **文档**：§3 删掉「DTB `no-map` 可能属于物理 RAM」的矛盾句，统一为「权威 DTB 提供保留身份，UEFI 图提供物理存在与处置，二者都不能单独推出 `RamSourceDomain` 成员资格」；O-06 已知属性位改为 **capability 元数据**（不等于当前生效权限，`WP` ≠ 页表写权限），并区分 `EFI_MEMORY_DESCRIPTOR.Attribute` 与 `EFI Memory Attributes Table`；§2.1 MRB 补零长度拒绝与 **O-12j**（`off_mem_rsvmap` 对齐/边界未校验的缺口）；`PAL_CODE` 改为「不支持、不做成员资格承诺」；新增 **O-12i**（六种投影的页粒度规则 + 非页对齐设备窗口负控）；O-12h 补描述符重叠与 `2^64` 顶端规则（u128 计算、第一版明确拒绝）；O-12c 补**实际静态窗口清单**（LAPIC/IO-APIC/平台 MMIO/PLIC/UART）与「以静态表为准、DTB 不一致倾向拒绝」。
- **门**：minix-boot 84、boot-shim 67；UEFI 双目标编译；riscv bootface QEMU PASS。

**本轮（第三十四轮评审之后：DTB 节点状态成为准入条件 + O-12 重构）**

- **代码 `f0c518179`**：`/memory` 的 `status` 缺失或 `okay` 才采集，其它值拒绝（`UnsupportedMemoryNodeStatus`）；`hotpluggable` 拒绝启动（`HotpluggableMemoryUnsupported`）。+2 测试（83），riscv bootface QEMU PASS。
- **文档**：§2.1 把 status/hotpluggable 从「已知限制」升级为**输入准入**（`disabled` 内存不再交给 PMM）；O-06 裁定 `RESERVED` 域外（采用 B）、`UNUSABLE` 在来源域但进 PMM 排除；补 `EFI_MEMORY_HOT_PLUGGABLE`（`uefi-raw 0.9` 无该位 → 未知位 fail-closed）；O-11 改掉残留的「配置表指针」措辞并补副本生命周期；O-12 从被矩阵打断的表格改为子合同列表，O-12c 拆 `DeviceResource`/`DeviceWindowPolicy`/`DeviceAccessRequirement` 并列出静态表清点与冲突规则；O-12h 补内存图元数据层与 `VirtualStart`；O-12g 补每级 `ranges` 记录；§7 阻塞改为五项。
- **门**：minix-boot 83、boot-shim 66；x86_64/aarch64 UEFI 编译；riscv bootface QEMU PASS。

**本轮（第三十三轮评审之后：MRB 完整性切片 + 四项阻塞的文档收敛）**

- **代码 `413ff9265`**：MRB 条目**互不重叠**（`OverlappingMemoryReservations`）与**扫描上限 16 条**（`TooManyMemoryReservations`；缺 `{0,0}` 终止项也落在这里）；+2 测试（minix-boot 81），riscv bootface QEMU PASS。
- **文档**：§2.1 补 MRB 拒绝条件与三项项目限制（`status` 不筛选、`hotpluggable` 不识别、`reusable` 不回收）；O-11 扩展到**四条 DTB 入口**（授权来源表 + 统一两阶段读取 + 重叠类型裁决与规范强度）；O-06 表加 `RamSourceDomain` 列（`RESERVED`/`UNUSABLE` 标待裁定）、补已知属性位处置；O-12c 补设备窗口来源矩阵；O-12g 要求原始记录保留完整 byte range 与身份、PMM 投影才取交集；§3 不变量 1 改为「按投影各量化一次」；新增 **O-12h**（UEFI 结构校验与政策分类分开）；§6 增加证据基线（HEAD + 工作树状态）。
- **评审人指出的证据可复现性问题**：上一版引用的提交号与测试数不同步；本轮起 §6 固定写「证据基线：提交 + HEAD 校验 + 工作树状态 + 读数」。
- **门**：minix-boot 81、boot-shim 66；x86_64/aarch64 UEFI 与 riscv64 目标编译；riscv bootface QEMU PASS。

**本轮（第三十二轮评审之后：两处解析器校验 + 自检定性 + 来源域/backing 分层）**

- **代码 `431bf8461`**：`no-map`/`reusable` 必须是空属性（非空 → `NonEmptyReservationAttribute`）；`/reserved-memory` 的 cells 必须与根一致；**修复 aarch64-unknown-uefi 编译回归**（两处 `collect()` 未处理 `Result`，上一轮漏检该目标）；UEFI 磁盘 DTB 覆盖对照由 panic 降为诊断，QEMU 用例 MARK3 同步。
- **文档 `46cbef986`**：§1.2 收窄「UEFI 要读设备树保留声明」为「只有提供权威 DTB 的路径才解析并交叉验证」；§3 分出 `RamSourceDomain` 与 `RamBacking`，`EfiReservedMemoryType` 可属来源域但非 backing；O-01 补逐类型保留期限与 `reusable` 所有者回收；O-02a 补 x86 `Resolve(PAT, MTRR(range))`、PAT 多核一致与别名；O-06 补「backing 不证明硬件可访问」与 `LOADER_CODE` 可执行权限；O-11 补 header 边界与 `/reserved-memory`/MRB 交叉验证表；O-12f 拆分、新增 O-12g（采集器保留来源记录的重构）。
- **门**：minix-boot 79、boot-shim 66；x86_64/aarch64 UEFI 与 riscv64 目标编译；`test-shim-bootmarks-aarch64` QEMU PASS。

**本轮（第三十一轮评审之后：一个代码切片 + 定点语义修订）**

评审人确认结构可用，给出四项首要修改与逐条裁决。本轮先落一个代码切片，再改文档：

- **代码切片 `2209fb3e9`**：`/reserved-memory` 父节点必须声明**空 `ranges`**（非空/缺失 → `ReservedMemoryRangesNotEmpty`），子节点 `no-map` 与 `reusable` 互斥（→ `ConflictingReservationAttributes`）；夹具补 `ranges`/`reusable`，新增 2 条测试（77 通过），riscv bootface QEMU PASS。
- **修正 aarch64 UEFI 的磁盘 DTB 用法**：磁盘 DTB 的 `/memory` 不是内存政策输入，只作固件内存图的自检闸；RAM 唯一来源是 UEFI 内存图；配置表 DTB 是保留政策来源（设计目标，未接入）。启动入口矩阵改为四行。
- **O-01** 候选 A 补「三层叠加」：UEFI 类型生命周期 + DTB 保留声明 + 对象存活区间，不得只按类型决定释放事件。
- **O-02a** 补页粒度判据（大页覆盖混合区必须拆分或拒绝）与授权字段（PA/VA/用途/权限/属性/共享性/树与阶段）。
- **O-02b** 改写候选 A：普通保留声明**只产生 PMM 排除**，不自动产生映射/访问禁止；`no-map` 的设备驱动受控例外与 `reusable` 语义写明。
- **O-03/O-04** 补优先级问题（正向授权与禁止覆盖同段时谁优先）与五种禁止语义的区分。
- **O-05** 补「先审后用」：未激活副本上修改并审计后发布，或经受控接口在硬件可见前验证；A/D 逐字段屏蔽。
- **O-06** 类型表补全 `PERSISTENT`/`PAL_CODE`/`UNACCEPTED`/`ACPI_NVS`/`LOADER_*`，并加「释放事件/来源叠加」两列。
- **O-07** 明确 R20 只证明「已登记访问点与禁止集合不冲突」，不得宣称全局访问安全。
- **O-10** 加第二枚负控：已有 `asm!` 块内第二条原始操作也必须被拒绝。
- **O-11** 改为两步授权（header → 完整对象），并加「权威 DTB vs UEFI 内存图」一致性策略 A/B。
- **O-12** 增 O-12f（`RamSourceDomain` 的可计算定义）；§3 不变量 2 改用该定义。
- **§1.5** 补 x86「PAT × MTRR 组合有效类型」与 RISC-V「PMA 能力值来源必须写明」。

**本轮（第三十轮评审之后：定点语义修订，不重写）**

评审人确认结构可用，要求先修语义再写代码。逐条落笔，主文档 364 → 384 行：

- **O-01 重写**：UEFI 的 `ACPI_RECLAIM` / BootServices / Loader 有生命周期，不是永久保留；「第一版永不回收」改为项目策略。候选 A 引入 `Permanent / Until(event) / ReclaimableNow`。
- **O-02 拆分**：O-02a 恒等映射的授权完整性（「不在可用表」推不出「不得映射」）；O-02b 普通保留声明不自动产生映射禁令（与 `no-map`、MRB 分级）。
- **O-03 重写**为禁止项来源分级（`no-map` / MRB / 类型 / 对象），两份集合互不代替。
- **O-04** 明确 `MustNotMap` ≠ `MustNotAccess`，保留对象、树、阶段、路径四维。
- **O-05 采用候选 A**：审计时点表（T1 前引导根含 R13-D；T2 前最终根含 R21）＋冻结字段表（映射定义字段不允许变化；A/D 按架构规则允许并定义审计如何屏蔽）。候选 B 降为后续动态页表阶段的机制。
- **O-06 采用候选 A**：逐类型回答「普通主存 / 当前可映射 / 当前可分配」三问。
- **O-07** 补「访问点完整性来源」：受控包装、可检查的访问 API，或明确裸指针不在证明范围。
- **O-08 / O-09 / O-10 标记为已裁定（采用 A）**，保留编号等待实现。
- **O-11 重写**为首次访问授权的循环依赖：先裁定配置表指针是否构成授权，再谈复制。
- **O-12 拆成 O-12a～O-12e** 五个可独立签字的子合同。
- **文档级修正**：启动入口改为四条（补 aarch64 直启）；`/reserved-memory` 普通声明 / `no-map` / MRB 的强度分级落到 O-02b 与 O-03；`reusable` 明确是项目限制；新增 §2.7 代码位置表；§6 补「测试 ≠ 输入完整性」。

**本轮（第二十九轮之后：主文档第二次改写——从状态备忘改回自包含简报）**

评审人指出 137 行的 as-built 备忘对看不到仓库的读者不可用。同一纪律下改写为 364 行：

- §1 自包含背景：三条腿、四种来源、四个正交维度、两棵树的设计意图（并注明今天只有一棵根）、三架构差异；
- §2 as-built：每条主张带测试名或代码位置，并新增「不保证什么」；
- §3 五条不变量；§4 已裁定但未实现的合同；
- §5 十三条未决问题：每条给「问题 / 失败形态 / 候选 A、B（明确标注未裁定）/ 判据 / 第一个消费者」；
- §6 证据；§7 签字状态。

纪律不变：候选方案不是合同；第二层不签的理由仍是 O-01～O-12 没有代码。137 行的状态备忘被本版替换（未另行留存）。

**本轮（第二十九轮评审之后：意见三分，主文档改为 as-built 全量重写）**

本轮起执行新纪律：评审意见先三分，主文档只描述代码现状。

- **三分结果**：九项意见全部**成立**，但性质相同——它们都是「未设计集合的公式与时序」，不是代码缺陷。因此不再写成正文公式，统一登记为主文档 §4 的 O-01～O-12；两处旧文自相矛盾（R4b-c1 与 DenyOnly 的域外语义、Port I/O 混进 4 KiB 归一化表）随旧文冻结而消失。无「不成立」项（上一轮 G22/R20 重复计数的不成立结论继续有效，本轮未再提）。
- **主文档 1669 → 137 行**，按新结构重写：五条不变量、as-built（采集器 / 分类器 / 单根与附加 DM / 交接与消费 / 三条腿 / 尚未存在的机制）、已裁定未实现合同、O-01～O-13、证据索引、签字状态。
- **旧文冻结**为 `04-design-backlog-round28.md`（只增不改），保留二十八轮论证与历史公式供反查。
- **本轮核出的关键代码事实**：本仓今天只有**一棵**项目根；Step 4 的 DM 覆盖是**追加**（`establish_boot_dm` 注释：additive on the active bootstrap root），不是第二棵树。因此 R21/T1/T2 那套「两棵树」合同今天没有代码消费者，只能留在未设计。
- **评审人列的下一轮六项**（ReservedMapped 来源合成、DenyOnly 落点、对象 MustNotMap 投影、T1/T2 审计与冻结、Port I/O 地址域、effective_pa_limit 措辞）中，最后一项是本文措辞问题，已在 §3 改成「可表示上界」；其余五项与对象目录、R20 输入、A/D 硬件更新、诊断存储、R13-A 负控一起进 O 表。

**本轮（第二十八轮评审之后：集合边界、验收目标、事实状态）**

评审人仍判第二层不签，给出九项定点意见（三项阻断、四项关系式/证明责任、两项验收边界），并要求主文档压到 1700 行以内。逐条落笔，1745 → 1669 行：

- **非 UEFI `SourceDomain` 与保留声明分离**：`SourceDomain = NormalizeRamInward(RawRamSourceRanges)`；保留声明只在 `/memory` 交集内生效（`DeviceTreeReservedRanges = NormalizeReservedOutward(声明) ∩ SourceDomain`）；越出部分只进禁止域，不扩大 PMM 统计域。
- **`ReservedMapped` 派生式两步**：声明向外、backing 向内；新增 `DenyOnly = DeclaredPages − RamPages`，同时被要求映射又落在 `DenyOnly` 时构造期拒绝。角色表相应改成「两步」。
- **R21 补齐三类义务**：`FinalRootRequiredMappings` = Ordinary(S3/S4) ∪ Startup(FinalRoot,S3/S4) ∪ Device(FinalRoot,S3/S4)；审计点在 T2 之前，S4 只确认同一份根。
- **R18 定义 `RequiredPhysicalRanges`**：启动义务 ∪ 设备义务；Port I/O 不参与物理求交。
- **R13-A 收紧到函数级**：原始操作只允许在逐个列举的包装函数里，CI 同时拒绝白名单文件内新增的裸操作，并核对调用参数与 `AccessUseId` 一致。
- **T1 审计完成点 + S1 访问清单**：R13/R15a/R19b 必须在 T1 启用前完成；S1 的代码、栈、DTB、内存图缓冲、设备窗口逐项登记，按 hart 记录外部转换状态。
- **输入表**：`ReservedPrefixEnd` 划为表外政策标量；保留声明标为「来源身份独立」。
- **类型 12 定性**：改为项目支持边界（标准类型不属于架构专属），「目标固件不输出」仍待三条腿样本。
- **§4.4.1b 事实重写**：第十一轮「全仓零命中」作废（`memmap_source.rs` 已解析 `/reserved-memory` 与 MRB）；补四行事实表（解析入口/内容/测试/是否进政策）与三层状态（能解析 / 能转成政策集合 / 接入两棵树与 ABI）。
- **关系组复核**：评审人称 G22 与 G23 重复计入 R20——核对原文为 `G22 R18+R19a+R19b+R19c`、`G23 R20+R21`，**不成立**；仍补一句「每个断言恰好出现一次」的显式说明。

**本轮（第二十七轮评审之后：合同收尾，不扩集合）**

评审人确认第一层方向可签（保留映射区需补平台准入），第二层仍不签，给出五组收尾项。本轮逐条落笔，主文档 1745 → 1739 行：

- **输入域与输出责任**：三输入域与表头一致（原分隔行只有两列），`StartupObjectInventory` 只出现在义务域；`DeviceMappingRequirements` 不再叫「访问授权」，只引用 `AuthorityId`。
- **4 KiB granule 的架构边界**：AArch64 第一版锁定 4 KiB translation granule；代码锚点 `TCR_EL1` 的 `TG0/TG1 = 0` 与 `PhysFrame::SIZE = 0x1000`；16K/64K 不在支持集合。
- **归一化角色**：`NormalizeByType` 废弃，改为按六步裁决后的 `NormalizationRole` 六行表；`RamLikeSourceRanges` 改名 `MemoryTypeSourceRanges` 并注明「只表示描述内存」；补 R2 的包含前提（`DiagnosticMemberRanges ⊆ PmmPolicyUniverse`，构造式 + R17 页级拒绝）。
- **R13-A 完备性**：主键改为 `AccessUseId`，`(tree,phase,path)` / `(caller,phase)` 降为属性，映射侧引入 `MappingRequirementId`；调用点完备性改为「受控边界 + CI 静态审计 + 注入负控」的实现先决项，并把 raw 端口 asm 的现有分布写进合同。
- **保留映射区语义**：普通主存语义准入覆盖所有 `NormalMainMemory` 映射范围，不因是否进入 PMM 而豁免。
- **PERSISTENT / 诊断容量**：PERSISTENT 定为 `DiagnosticMemberRanges`（代码事实：type 14 落 side list）；诊断成员 range 表（完整、超限拒绝启动）与操作者样本（可截断、不否决启动）拆成两个合同。
- **计数与登记**：39 条按 `4+5+2+11+1+2+7+7` 重算，24 组给出 `G01–G24` 对照；§4.4.4 与 §6.8 的三处「请评审人回答」收敛进 §12.4（一项可回退决定、一项形状确认、一项审计项）。
- **措辞**：`AuthoritativeDtbSource` 的窄用途授权明确标为**项目解释**，不是规范自动授予。

**本轮（第二十六轮之后：先落码，再对该提交做双轴回归评审）**

按「先把可以落地的设计写进代码」执行。落码提交 `38668f843`（基线 `14682b699`）：

- 采集器（`os/libs/minix-boot/src/memmap_source.rs`）：`/reserved-memory` 静态 `reg`（含 `no-map`）与文件头保留映射块接入；声明按页向外取整并按 `/memory` 全景裁剪；固件前缀不重复登记；容量、回绕、声明重叠（含 `no-map` 与非 `no-map` 混合）、不可读声明形状、不可达保留节点各自具名拒绝。
- 修复两条登记缺陷：8 格静默截断（改为错误）；引导区重叠断言的裸加法（改 `checked_add`）。
- **撤销**缺陷表第五条：核对 `uefi 0.33` 源码后，`exit_boot_services` 失败时 reset、不返回，路径 A 封装没有可丢弃的状态。这是本方第二次「未读源码先登记」，撤销原因已进正文 §12.5。
- 验证读数：`cargo test -p minix-boot --lib` **75 passed**；`cargo test -p boot-shim --lib` **66 passed**；`cargo check` `x86_64-unknown-none` / `aarch64-unknown-none`（`fw-none-image`）/ `riscv64gc-unknown-none-elf` 全通过；`os/qemu-tests/test-kernel-image-riscv64-bootface.sh` **PASS**，串口 memmap 行不变（`1 first 0x80200000+0xfe00000`，OpenSBI 的保留节点全部落在被裁剪的固件前缀内）。
- 文档按落码事实修正：§4.1 采集能力、§4.3 两行现状、R4b-c1（撤回「必须整段在 /memory 内，否则拒绝」——落码按交集生效，帧缓冲式声明不拒绝）。

**第二十一至二十六轮档案回填说明**：这六轮的逐条变化此前只写进正文的「第 N 轮」标记与结论表，本档案缺块；细节以正文标记为准，标题级补记如下：第二十一轮端口 I/O 与 MMIO 分域、`AccessPath` / `MappingBanScope` 分离、`DeviceWindowPolicy` 入册、PMP 准入；第二十二轮 R19 拆政策编译与页表验收、设备访问单一验收输入、S3 释放与映射解耦；第二十三轮 `UNUSABLE` 进 L3 构造式、MMIO 双义务一一对应、对象来源覆盖、MRB 负向来源、恒等 VA 两路径；第二十四轮设备三记录职责与字段所有权；第二十五轮 `PortWindowPolicy`、`AccessUseId`、R23-P、R24b backing；第二十六轮三输入域、R21 并集目标、R19c、`TreePhasePairs`、计数 39 / 24。

**本轮（第二十轮评审之后）的变化**

评审人把「设备访问模型」列为第二层先决：`VA = PA` 覆盖不了 x86 端口 I/O。本轮修正七处：

1. **设备访问按腿拆开**：第一版把「设备访问授权」与「MMIO 映射义务」分成两个对象——`MemoryMappedDevice` 走 `DeviceMappingRequirements` + 页表验收；`PortIoDevice`（x86 控制台 COM1 `0x3F8` 的 `in`/`out`）走 `DeviceAccessRequirements`，**没有页表叶子、不进设备窗口、不参与恒等 VA 检查**。并写明 `LAPIC`/`IO-APIC` 不在第一版窗口清单里，§1.1 只是举例说明设备页必须显式不可缓存。
2. **原子要求改回 OS 语义**：`AtomicOperationRequirements` 记录内核实际依赖的操作种类、宽度、对齐、内存序与作用域；LR/SC 与 AMO 只是 RISC-V 的证明路径，不进跨架构字段；宽度也不再无条件要求 8/16/32/64。
3. **新增 `DeviceWindowPolicy` 记录**：`AuthorityId` 指向这条记录（声明者身份、允许范围、允许的树/阶段/路径、允许的访问语义），`DeviceWindowRanges` 由它派生——R22 的四条判据才有承载。
4. **R13-D 拆成两条断言**：`DeviceMappingSatisfies`（页表）与 `DeviceAccessSatisfies`（接口与调用路径）。
5. **`ACPI_RECLAIM` 的表格措辞改正**：回收前提标成「未来版本若引入回收机制需满足的条件」，不再暗示第一版有释放路径。
6. **R17 补非 UEFI 的 MRB × `/reserved-memory`**，并把「完全重复」的判据改成几何范围 + 类型 + 属性三者全同。
7. **DTB 源可读性按腿分支**：UEFI 腿给出允许读取类型的完整集合（含 DTB 落在 MRB 内的窄授权依据），非 UEFI 腿由启动协议保证并进平台假设；清理了「五问五答」与轮次编号。

另按评审人要求把断言口径分成**30 条顶层断言记录**与**独立数学不变量**两个数字，并明确 R3/R9/R9b/R5 是分阶段守卫、R14 是独立守卫。

**行数**：主文档 1752 行（上限 1780）。


**本轮（第十九轮评审之后）的变化**

评审人仍判第二、三层不签；第一层「普通主存方向可冻结、设备 I/O 需先解决 MMIO 与 x86 端口 I/O 的边界」。本轮修正六处：`ACPI_RECLAIM` 定为第一版 `NeverReclaim`（R1 无阶段参数，与「解析完可回收」直接冲突）；R21 五个语义字段填满（六类页共用一组取值，`ReservedMappedRange` 的 `R`/`RW` 由声明有无写需求决定）；R17 补五步顺序（原始字节判冲突 → 各来源分别量化 → 页级再判碰撞 → 通过才合并）与三条同源重叠规则；类型 11 × 设备窗口的拒绝标成项目支持限制；S1 改名「项目页表启用前的外部地址转换状态」并写 per-hart 与 SMP 停驻前提；两次转换 T1/T2 成为验收键、R13e 键扩成 `(物理页, 阶段|T1|T2)`，另补设备路径的 VA 布局冲突与 `MustNotAccess` 的模型层限定，计数统一为「30 条顶层断言记录」。正文 1729 行（上限 1780）。

**本轮（第十八轮评审之后）的变化**

评审人确认规范依据与分维度方向，第二、三层不签，第一层拆成「普通主存可签、设备 I/O 待闭合」。本轮修正七处：R17 去掉全部「按具名政策判定」（两处改为一律拒绝；MRB × `/reserved-memory` 按 UEFI 类型对照分两种）；R14 改回独立守卫；R4b-s 分两条腿写、覆盖交给 R4b-c1；计数 30 条；输出表改「两张区间表 + 四份记录」并把访问义务记录正名为 `StartupMappingRequirements` ABI；R21 六类页权限表填满并补 VA 可表示性进 R13b；`MustNotMap` 转换声明为有意扩大；`S3ReleaseDisposition` 进必填字段、`NoRequirement` 完整性落成矩阵五条验收、`DeviceIoContract` 读/写幂等分开、R13-D 拆两层；DTB 缓冲「先分配后取最终 MapKey」七步。

**本轮（第十七轮评审之后）的变化**

评审人确认第一层「架构中立语义方向」可冻结，但第一层完整合同条件未闭合，第二、三层不能签。本轮修正六处：

1. **R16 不再替政策做升级**：限定到 `L1RequiredDenyRanges`（除「只升 L2」之外的所有 L2 成员）；并补一条配套——第一版无专用路径，「只升 L2」的登记项其范围必须不在可用表里，否则登记即拒。
2. **R4b-c 拆成两条**：`R4b-c1` 在**原始字节坐标**上验证覆盖（避免「向外归一化的保留 vs 向内归一化的 RAM」误杀贴边合法声明），`R4b-c2` 在页级坐标上验证；并明确 MRB ⊆ /memory 是**项目支持子集**，不是规范强制。
3. **R22 补成四条可执行判据**（`AuthorityId` 解析、唯一或列举、范围包含、授权政策允许该树/阶段/路径/语义），堵住「落在并集里但无人授权」的假通过。
4. **新增 R9b**：`ReservedMappedRanges ∩ FinalMapDenyRanges = ∅`——R12 与 R15b 对纯 L2 成员本来不可能同时成立，必须在页表构造前失败。
5. **R21 给出页类别生成规则**（代码/只读数据/可写数据/自举页表/通用 PMM/保留映射区各一条记录；两要求同页取交集；L2 禁止页不进直接映射记录）。
6. **其余按他逐条落笔**：类型 11 残留句改为「相交即来源冲突、拒绝启动」；`FirmwareRuntimeMmioRanges` 消费者改为「无运行期服务消费者，有规划与政策审计消费者」；设备页改 `DeviceIoContract`；R13-D 覆盖设备专属字段；RISC-V 补硬件 A/D 与统一 PTE 方案；§1.2 撤销「返回零」；§4.4.1b 五项改六项；§8 的 R13d 行改成五个字段；四态义务的跨界转换写成三句（MustNotMap 完整编译并保 scope、MustNotAccess 由引导壳交接前执行、验收位置不在内核）；S3 回收写成三值字段并默认 `NeverReclaim`；计数按正文逐行重算为 **28 条原子断言 / 22 个关系组**。

**行数**：主文档 1691 行（上限 1750）。


**上一轮（第十六轮评审之后）的变化**

评审人把第一层从「可签」下调为「方向认可、条件待闭合」，理由是 RISC-V 通用池规则与设备 I/O 的跨架构语义要先补完；第二、三层不签。本轮修正八处：

1. **R3 加定义域**：`BootMapDenyRanges ∩ PmmPolicyUniverse ⊆ PmmExcludedRanges(p)`——类型 11 是域外成员，整集合进域内排除集是数学上不可能的。
2. **R4b-s 的恒真问题**：非 UEFI 腿的 `SourceDomain` 本来就把保留声明并进来，R4b-s 在那条腿上证明不了覆盖；新增 **R4b-c**（保留声明必须被 `/memory` RAM 向内归一化后完整覆盖），不满足即输入矛盾。
3. **R19 落到可执行 scope**：`映射范围` 取 `DirectMapOnly(树)` / `AllPaths(树)` / `NamedPath(树,路径)` 三个具名值，由 `ScopeToPaths` 展开成真实路径集合。
4. **新增 R22**：设备义务的每条物理范围必须落在已登记的设备窗口里、并能解析到授权来源。
5. **R21 补齐五面定义**：普通内存要求要逐维给出覆盖、固定偏移与溢出证明、**精确权限（通用 PMM 区与保留映射区可不同）**、五个语义字段、别名兼容。
6. **类型 11 的设备路径格改口**：R17 已规定类型 11 与设备窗口相交即拒绝，四层归属表里「按设备义务授权才可访问」是死代码，删掉。
7. **MustNotAccess 的证明边界**：改为「不存在已登记的访问义务或设备授权」，明确它不声称能证明裸指针与硬件推测访问；`DeniedFor` 对设备路径套用 L2/L3 的后果写全（只升 L2 的登记项也禁止设备恒等访问）。
8. **其余按他逐条落笔**：RISC-V 删掉「按区域限定」的准入例外并补跨 hart 共同能力；设备义务用**设备 I/O 语义**（读写/宽度/对齐/幂等/排序/推测/原子）替代「不可缓存」；R13e 审计**全部真实叶子**、大页按 4KiB 参与且不得跨界、无政策归属的叶子拒绝；S3 后回收补范围来源二选一与 `NeverReclaim`；`NoRequirement` 定为**不跨界**（方案一）并要求完整性证明；对象目录补 per-hart 栈、trap 向量、SMP 参数、切换期页表页四类冷读对象；计数按追溯表重算为 **26 条原子断言**，正文不再人工维护。

**行数**：主文档 1676 行（上限 1750）。


**上一轮（第十五轮评审之后）的变化**

评审人再次确认第一层可签、第二层暂不签，并指出六处「新补定义彼此不兼容」。正文变化六处：

1. **类型 11 与真实设备窗口改为互斥**：第十五轮裁决为「相交即拒绝启动」。理由是两条政策不能同时成立——类型 11 已完整进入 `BootMapDenyRanges`，若再允许设备义务使用同一段，R18 必然失败；规范表 7.10 也说退出后这类内存不由 OS 使用、MMIO 信息来自 ACPI，类型 11 不应成为设备窗口的授权来源。这一改同时消掉了「`DeviceWindowAccess` 复用禁止集合」的语义争议。
2. **显式禁令拆成 L2/L3 两份**：`ExplicitPolicyDeniedRanges` 拆成 `ExplicitBootMapDenyRanges` 与 `ExplicitFinalMapDenyRanges`，E-1 登记项从此能表达「只升 L2」；L3 ⊆ L2 由 R11 自然保证。
3. **`SourceDomain` 改成分语义归一化**：`NormalizeRamInward(RamSources) ∪ NormalizeReservedOutward(DtbReservedSources)`；并删掉 §4.4.2 里那条与语义表相反的「一律起点向下、末端向上」通用条款。
4. **RISC-V 取方案 A 并把例外删掉**：凡进入通用 PMM 的 RAM 必须满足内核可能赋予它的全部用途能力；不再保留「只对特定区域要求取指/页表访问」这种没有分配器约束的例外。
5. **状态机乘积表补 `NotApplicable`**，并把 S1 改名「项目页表启用前的外部根」（非 UEFI 腿的初始页表未必由固件建立）。
6. **四态与命名统一**：正文与术语表一律称四态；`StartupNoMapRequirements` / `StartupNoAccessRequirements` 统一。新增 **R21**（普通主存映射的正向验收）——此前 R13 实际只约束启动对象与设备窗口，不能声称覆盖整个最终直接映射。计数改为 **22 个关系组 + 25 条原子断言**，并新增 `assertion_id → test_id` 追溯表。

**规范分级修正两处**：同一 `/reserved-memory` 节点内多个 `reg` 重叠——规范对 MRB 有明确不重叠要求、对节点内 `reg` 没有，因此改标「第一版项目支持子集」；类型 12 继续写作「已知标准类型，但目标平台与本项目第一版不支持」，不引申为跨架构绝对禁令。

**边界说明**：权威 DTB 的窄用途授权与 R20 的时间边界写清——R20 约束禁止集合生效后、项目页表内的义务；S1 外部页表下复制前的读取是独立的 `DtbReadCopy` 授权，不构成后续访问义务。

**行数**：主文档 1663 行（上限本轮收紧到 1750）。


**上一轮（第十四轮评审之后）的变化**

评审人**签署了第一层**（语义结构与设计方向），拒绝第二层，并逐条给出可指出的内部矛盾；他同时明确下一轮只按八项清单做合同—代码对照。正文本轮变化七处：

1. **`BootMapDenyRanges` 从「空定义」补成构造式**：新增成员来源 × 四层归属表（`no-map`、MRB、`RESERVED/UNUSABLE`、类型 11、真实设备窗口），并把它写进 L2 公式同一代码块；**类型 11 的去向由定义给出**——第一版显式禁止进入两棵普通映射树，不再依赖「它不在候选来源里」这种偶然保护。
2. **`DeviceWindowAccess` 的虚拟关系定死为恒等**（`VA = PA`），因此它套用所在树的禁止集合是**定义结果**而不是「把 L2/L3 当全局禁令」；将来引入独立窗口须同时新增设备访问自己的禁止集合并按 ABI 变更登记。
3. **访问义务由三态扩成四态**：`MustNotMap(映射范围)` 与 `MustNotAccess(访问范围)` 分开，后者用新关系 **R20** 验收（不得有启动对象义务或设备义务与它相交）；MRB 走的是 R20 而不是只靠 R19。
4. **区间量化改为双向**：正向 RAM 候选向内取整，排除/保留/对象/设备窗口向外取整，四条仍在同一 4 KiB 坐标上——避免 `/memory` 把声明外的头尾页交出去。
5. **引导根 `RWX` 与 `W^X` 的裁决**：第一版改造引导根按对象拆权限（代码 `RX`、数据栈 `RW+NX`、设备不可缓存+NX），不把整段设为例外；确需例外必须具名五要素，例外表为空是目标；过渡窗口按 R13e 合并判定。
6. **`AvailableRanges(p)` 的跨界消费定死**：交接只传 `AvailableRanges(S3)`，之后结束生命周期的对象由明确的内核释放点逐项释放；`AvailableRanges(S4)` 只作数学结果，内核不得据此扩大可用池。
7. **其余按他逐条落笔**：`R4b` 拆成来源层 `R4b-s` 与政策层 `R4b-p`；R17 增加「先保留来源身份、后归一化」的硬约束并补三类同源重叠；计数改为 **22 个关系组 + 23 条原子断言**；`effective_pa_limit` 在 §8 如实降级（唯一输入源与设备窗口可达性属实现未完成）；RISC-V 取保守口径（进入通用 PMM 的 RAM 必须满足全部访问/页表/原子能力，不做能力分区分配器）；权威 DTB 落在 MRB 内时裁定为**窄用途授权**（只读该 DTB 对象、不改写、复制后失效，不接受则拒绝该布局）；§0.5 冻结门改成**三档**（第一层可签、第二层待核验、第三层 ABI 不可签）；§1 状态改为「方向已定、可签政策，硬件证据属实现门槛」。

**行数**：主文档 1656 行（上限 1780）。


**上一轮（第十三轮评审之后）的变化**

他这一轮的落点全部在「关系式自身是否闭合」上，并第一次把冻结拆成两层。正文因此重写了 §0.3、补齐了访问义务三态与路径判据，并把主文档压到 1600 行以内。八处变化：

1. **§0.3 整体重写为自包含的核心**（这一节现在自带集合定义、构造式、22 行关系式、三态访问义务、五个验收面与计数口径，读者不必翻材料册也能裁决）。重写同时解决了他点的三处旧名：`DeviceAddressRanges` 彻底除名（统计域写成两次减法）、`StartupRequiredObjectRanges` 全部改为现名、`MappingPath = NoMapping` 这个把两件事压在一起的值被拆掉。
2. **R17 补上合法的来源叠加并扩成封闭矩阵**：新增 `RAM × 设备树保留` 行（静态保留区落在 RAM 内是正常形态，两条腿的验证步骤不同），补 `类型 11 × 设备树保留`、`设备窗口 × 类型强制排除`、以及「同一 MRB 内条目重叠属格式非法、必须在归一化**之前**拒绝」。
3. **访问义务拆成三态**：`MustMap` / `NoRequirement` / `MustNotMap(范围)`；`StartupMappingRequirements` 只从 `MustMap` 派生，新增 `StartupNoMappingRequirements` 与关系 **R19**（真正的无叶子验收）。评审人的判据很直接：「不要求映射」推不出「禁止映射」。
4. **路径模型统一**：`DeniedFor` 逐条定义两条已支持路径（引导根恒等、最终直接映射）与 `DeviceWindowAccess`，`NoMapScope` 按记录范围解析；三条义务对象（启动映射、无映射、设备映射）各自对应 R10/R13、R19、R18/R13-D。
5. **设备树的两路保留语义不再被并集抹平**：`/reserved-memory` 的 `no-map` 与文件头保留映射块（MRB）分开写合同——MRB 是「未经用途授权不得访问」，第一版没有授权通道，因此它与设备窗口相交直接拒绝启动。
6. **DTB 复制前置补格式检查点**：基址 8 字节对齐、保留映射块 8 字节对齐、结构块 4 字节对齐、`version` 与 `last_comp_version` 的兼容范围，并逐项写明由本项目还是依赖库负责。
7. **状态机补允许组合表**（S1 允许三种服务状态、S2–S4 只允许 `BootServicesExited`）；非 UEFI 腿记 `NotApplicable` 而不是「已退出」。**§8 依赖图改成四段流水线**，解除「裁决二 ↔ 裁决六」的循环。
8. **RISC-V 访问能力补宽粒度**：按内核实际依赖的访问宽度与对齐登记，并按区域限定范围（取指只对代码区、页表读写只对承载页表的区域）。

**冻结门改成两层**（本节新增，取代第十二轮那版单一门）：第一层「语义结构与设计方向」可以签；第二层「每份集合的精确定义、每条关系式、交接 ABI 字段合同与失败形状」本轮已逐条落笔、请他核验后同上签署。实现门槛继续独立跟踪。

**行数**：主文档 1609 行（上限 1780，用户本轮把上限从 1800 收紧到 1780 并要求自包含）。

**回填之后的复核**：同一条意见再次送交时，本方把它的十四项逐条回读正文，另查出四处残留并就地改掉——「四个语义集合与那十三条集合关系」的旧计数、§6.7 分组叙述里的旧名、§6.8 仍引用「最小权限 + 允许上界」那句旧伪公式、§8 表格里「类型 11 → DeviceAddressRanges」的旧落点。四处都是同一类：**改名/拆分之后，引用它的地方没有全部跟着改**——这正是本方第十四轮给自己记的那条教训，所以本轮把「回填后逐项回读」也当成收口门的一部分。


**上一轮（第十二轮评审之后）的变化**

他这份标题写「第十一轮重写稿」，审阅的是本方第十一轮回填后的正文；本方记为第十二轮。判断仍未冻结，但他第一次给出**冻结路线**：先修完 先决，再冻结政策结构；**硬件证据、QEMU 验收与实现缺陷作为独立的实现门槛，不为等硬件而拖延概念冻结**。本轮正文变化六处：

1. **统计域公式漏掉了刚拆出来的一份域外集合**（他的 第一项先决）。第十一轮把 `DeviceAddressRanges` 拆成两份，**公式里只减了 `DeviceWindowRanges`**，于是类型 11 的描述符留在 `SourceDomain` 里、既不在可用侧也不在排除侧，**R5b 分区式直接不成立**。本方按他的建议不合并，而是两个都减；并写死 `FirmwareRuntimeMmioRanges` 的职责是「来源完整性记录 + 与设备窗口的交叉核验件」，不是义务。
2. **R17 升级成封闭兼容矩阵**（他的 第一项先决 后半）。一句「RAM ∩ 设备 = ∅」覆盖不了本方自己表里允许的两种交叉，所以按类别对列五条判定，并给出 `RamLikeSourceRanges` / `DeviceLikeSourceRanges` / `DtbReservedSourceRanges` 的定义，以及「类型 11 × 设备窗口」那条具名对应关系的五条可执行规则（允许的匹配形状、部分交叉不允许、只按页区间判定、重复按并集、不匹配即拒绝）。
3. **设备义务接进 R13，R18 补全量词**（他的 第二项先决）。新增 `R13-D`，与启动对象共用同一套五维判定但候选集合与失败记录分开；`DeniedFor(树,路径)` 只对两条路径有定义、其它路径必须显式登记；`MappingPath` 进义务记录，第一版四个取值；**普通启动对象的 `DedicatedPath` 是「未支持值」，出现即拒绝**，设备窗口走 `DeviceWindowAccess`（他的 11.1）。`R15a` 改名 `BootIdentityMappedRanges`——引导根实现的是恒等段，不叫 DirectMap。
4. **设备义务表自身的存储进对象目录**（他的 第三项先决）。补一行 ABI 存储与生命周期；补「暂存区是否真落在规划工作区」的包含关系检查；并回答他指出的递归问题——**登记表自身的符号地址取自链接期静态范围，不取自登记结果**。
5. **两条腿的 DTB 消费点闭合**（他的 第四项先决 与 第五项先决）。每条 `AuthoritativeDtbSource` 补 `PostShimConsumer` 三值（None / OriginalSource{last_use_phase} / CopyTarget{copy_object_id}），三条腿各选一个、不许留空；把「头部可读」拆成**来源合法**（类型允许读）与**当前可达**（S1 属固件页表下的外部前提、进平台假设清单）两组；**无 DTB 的准入条件**写死为「平台证据能证明没有需要额外表达的 no-map / 保留映射块语义，或已由其它来源完整承载」，否则拒绝启动，且平台证据不能是「QEMU 当前能启动」。
6. **计数、语义层与 RISC-V 访问面**（他的 第六项先决 / 第七 / 第八节）。计数改成四行并列（编号区间 / 21 行具名关系式 / R13 五个验收面 / 实际测试另计），并同步 §8；`MemoryClass = NormalWriteBack` 与 `CoherentShareable` 这两个示例值按他的判据改成五个字段（`AddressSpaceClass` / `CoherenceContract` / `SharingScope` / `OrderingRequirement` / `AtomicOperationRequirements`），可缓存性降为平台能力与页表证明资料；RISC-V 补 `SupportedAccess` 维度（取指、普通读写、硬件页表读、硬件页表写）。
7. **另外按他的 9.1 / 9.2 / 11.2 / 11.3 / 11.4 补齐**：`TypeExcludedRanges` 的流式贡献改成**两个下游**（PMM 排除累积器与 L2 累积器，各自计数、各自预检、两遍扫描）；§3.3 问题 D 改为直接引用 §5.3 的唯一清单；R13e 的 `W^X` 别名规则标为**项目映射安全合同**并强调跨对象跨页归并；`PrefixRangeView` 补「不保证页对齐 ⇒ 走统一外向归一化」与「超过 `effective_pa_limit` 即拒绝启动，不静默钳制」；**E-1 登记表的七个必填字段写进正文**。
8. **本轮首次给出冻结边界**（他的最终裁决那段建议）：正文新增一段「政策与合同结构：建议冻结；实现门槛独立跟踪」的声明，并请他确认边界——不再以「文档还很长」推迟，也不把硬件验证当冻结前置。

**行数**：主文档上限仍是 1800 行，本轮回填一度到 1857，按同一套纪律把 §0.5 的两张历史裁定表移入档案、§2.5 逐位处置表与 §2.3 矩阵完备性判据移入材料册，正文 1797 行。

**上一轮（第十一轮评审之后）的变化**

他这份标题写「第十轮回填版审阅意见」，审阅的是本方第十轮回填后的正文；本方按收到次数记为第十一轮，编号差异在正文文首登记。本轮判断仍是**不建议冻结**，但他把话说得更清楚：**政策方向不必重开，阻止冻结的只剩「新补出的形式化关系又把不同语义压回了一个集合关系」**，并点名两处最危险——R10 只按树不按路径，以及 `DeviceAddressRanges` 把刚分开的类型 11 与设备窗口又并回去。本轮正文变化八处，其中**四处是本方第十轮刚写下的东西被再拆**：

1. **排除集合的公式漏了自己新加的成员，R16 还缺量词**（他的第 1、2 项）。第十轮本方给 L2 补了五项构造式，其中 `ExplicitPolicyDeniedRanges` **没有同步进 `PmmExcludedRanges`**，而 R16 要求 L2 成员都在排除侧——凡是只进这一项的页都会违反 R16。本方按他的方案一把它加进公式，并接受他更狠的那句：**它不能成为换了名字的兜底桶**；本方因此给出一张封闭登记表（E-1 逐项登记的类型升级、E-2 由诊断成员承载不重复计入），并明确**第一版它就是空集**、将来只能经 E-1 增长、来源完整性测试要断言每个元素都能追溯到一条登记项。R16 补 `∀p`。
2. **统计域的减法本身成了新的掩盖器**（他的第 27 项，本方核对与本方第十轮修的是同一个病）。`PmmPolicyUniverse = SourceDomain − DeviceAddressRanges` 会把「同一页既是 RAM 候选又是设备寄存器」这种**两个来源互相打脸**的情况直接抹掉，公式照样闭合。本方按他的方向新增关系 **R17：先判来源自洽，再算域外减法**，并列出四类来源对各自的处理（普通 RAM 与设备窗口相交一律矛盾、拒绝启动；设备树保留声明与设备窗口相交要按具名政策判断）。
3. **前缀阈值一处话、三个口径**（他的第 5 项）。本方第十轮一边在 §0.3 写「阈值不进任何区间集合」，一边在构造式里放了 `ReservedPrefixRanges`，术语表还跟着加了一个词条。按他的区分改名并写死：**`ReservedPrefixEnd` 是唯一标量输入（不是集合、不进容量清单），`PrefixRangeView = [0, ReservedPrefixEnd)` 是派生区间视图**，并且明确它**没有独立存储**——这条正是本方自己第十轮那条「没有独立存储不得冒充缓冲区」纪律对本方的又一次适用。
4. **R10 把 L2 当成了绝对禁令**（他的第 11 项，他列为本轮语义层面最重要的一条）。本方第十轮只做到「按树选禁止集合」，可 L2 的定义是**不得进入最终直接映射**，不等于「这页在任何虚拟地址上都不能映射」；而本方自己给映射义务留了三种访问路径，于是「一个位于诊断成员里、但有经证明的专用路径的对象」会被无条件拒绝。本方按他给的表把 R10 升级为**按树与访问路径选判据**，并明确第一版态度：**不提供可绕过 L2 的专用映射例外**（对象必须在最终根访问而其在 L2 内 ⇒ 拒绝启动），§2.2 行 3 那半句「可经审查提供专用路径」限缩为**未来政策**；同时把实际验收拆成 **R15a（引导根直接映射 ∩ L3）** 与 **R15b（`FinalDirectMapRanges` ∩ L2）**，被测集合按映射种类分开，不混设备映射与专用路径。
5. **刚拆开的类型 11 又被并回域外集合**（他的第 8 项）。本方第十轮在 §2.1 把 11 与 12 分开，转头在 `DeviceAddressRanges` 里让 11 与架构设备窗口共用一个集合、还共用「设备映射义务的地址来源」这个消费者。本方按他给的两份集合拆开：`FirmwareRuntimeMmioRanges`（只装类型 11，**第一版没有消费者**，不自动生成驱动义务）与 `DeviceWindowRanges`（真实窗口，生成设备义务）。另外接受他一条纠正：本方写的「样本一旦显示固件输出类型 12 就改判域外保留」**不成立**——**样本只证明固件输出，不证明本项目已有正确的消费者与合同**；若真输出，先重新裁决该平台是否仍受支持，或另行设计完整处置，不得仅凭样本切换归属。
6. **设备映射义务从一句名字变成一份合同**（他的第 9、12、25、26 项，以及第九节）。本方第十轮只给了名字加一条「要能回答授权来源」。本轮补齐六个问题：记录里的语义字段（含**设备内存语义**而非缓存类主存语义）、生产者与消费者（**两棵树都消费**——中止路径在引导根写寄存器、内核早期控制台驱动在最终根写同一窗口）、是否跨界（**是**，独立容量、不得与映射义务共用余量）、怎样参与实际页表验收（R18 + R13 五个子判据）、与 `DeviceWindowRanges` 和 `identity_device_windows` 的关系（来源集合 vs 授权集合；后者标为待改造的来源件）。并按他的警告新增关系 **R18：设备义务也要过禁止冲突检查**——本方第十轮那句「串口窗口不属于启动对象登记表」如果不同步补 R18，就等于让最该拒绝启动的那类冲突无人检查。存储清单本轮定为**唯一权威**（§5.3 那张表，§3.3 问题 D 只引用不再另列），并裁定 `PmmExcludedRanges` **不跨界**（他第 12 项指出的三处口径不一致，本方取判据二）、`TypeExcludedRanges` **流式贡献不独立存储**（他第 26 项：本方第十轮那条纪律必须能应用到本方自己新加的名字上）。
7. **设备树那条链在两条腿上都还没有闭合点**（他的第 4、10、11、15 项）。本方核出并写下：UEFI 腿上两路设备树保留声明**必须先证明被固件内存图完整覆盖**再谈求交，否则 `∩ PmmPolicyUniverse` 为空会让 R4b 与 R5b 双双通过而这条声明哪儿也没进——**这正是本方第十轮修的那个假闭合换了位置**；覆盖对照拆成两档，范围覆盖属规范强制、reservation block 的类型属建议级（本项目升级为严格 ⇒ 标项目加强）。非 UEFI 腿的原始设备树（RISC-V `dtb_ptr()` 那份）此前**不在对象目录里**，本轮补 `AuthoritativeDtbSource(启动方式)`，并要求 DTB 本体落在保留映射块时**读取先于禁止生效**或复制到可访问位置。复制前置补**源可读性五问**（头部可读、checked 末端、整段可读、复制目标容量、各偏移限制在 totalsize 内）——他的判据本方原样收下：**区间算术正确不等于源内存真的可读**。退化规则从三态并成一格拆成**四态**（absent / 单一有效 / 重复冲突 / 损坏），并删掉本方上一版留在 §4.4.1b 末尾「是否解析配置表那份」的备选口径——他已裁定采用配置表那份，本方不能同时写「合同已定」与「请评审人决定合同是否存在」。
8. **R13d / R13e 从字段名落到语义，阶段模型拆成两个状态机**（他的第 13、14、16、17、18 项）。R13d 原来写「叶子属性索引与共享性符合平台合同」，而**只有 AArch64 同时有这两样东西**：x86 的有效类型要由页表选择与 PAT、MTRR 合成，RISC-V Sv39 普通页表项根本没有缓存属性索引。本轮改为核对**实际映射与平台属性共同兑现的 OS 内存语义**，三架构各自给证明方式；并按他的提醒把 `MemoryClass = NormalWriteBack` 这个示例值拆开——它和本仓 §1.1「普通主存语义不要求平台有物理缓存」顶牛，故拆成主存类别 / 一致性与共享性 / 排序 / 原子能力四维，**具体缓存策略只在架构证明层出现，不作跨架构统一必填**。R13e 从「整体兼容」这四个无法测试的字改成可判定关系：按（物理页, 阶段或转换窗口）归并，`RX`+`R` 允许、`RX`+`RW` 拒绝、跨阶段分别生效不算冲突、同步完成前新旧项一起判。阶段模型按他给的两个名字分开（`BootServicesState` 与 `MemoryLifecyclePhase`，本方核对确实存在 `BootServicesExited` × `S1` 这种真实组合），S4 改名「最终根生效阶段」、服务器就绪降为独立事件，表头「PMM 可用？」改成「物理页分配器是否已启用」。

**本方本轮另需单独登记的三处自身错误**（都写进 §12.5）：① **术语表落后正文一整轮**——他第 6 与第 24 项点的两处（`PmmPolicyUniverse` 词条仍是第九轮那个「先减掉待统计排除项」的旧公式，`归一化集合族` 词条还写着已被拆掉的 `StartupRequiredObjectRanges`）都是本方第十轮**改正文时漏改词条**，而他给的后果是本方最怕的那种：第三方读者照词条实现，而那个名字在正文里已不存在。② **「122 处原子用法」是把匹配行数当成了使用点数**——本方第十轮刚立「计数类断言不能只看计数」，第十一轮就在同一份文档里第二次犯；本轮改为「122 行匹配」并写明它既不是调用数也不是指令数。③ **`ReservedPrefixRanges` 这一处属于「同一节里两句话互相打脸」**，与第十轮那处「引导语说只有第一层参与匹配、下面的表自己写着不作匹配键」同因：本方逐段改、不做一次同节通读。

**行数与结构**：用户本轮给出硬上限「主文档不超过 1800 行」。第十一轮回填一度把正文推到 1986 行，本方按第八轮与第十轮同一套纪律再移一次：原 §0.2、§1.5、§2.4、§3.1、§5.1、§6.1、§7.2、§9 八段移入材料册（现第三至第八节），§4.4.1 的三案细节与 §6.5 的 C′ 完整形状、§8 的三层状态框架移入档案；正文 1792 行。**移出的都是形状事实、机制背景、取证面与词表，没有一条政策判断、待裁问题或合同条款被移出**；逐行核对被移出的 180 个非空行在目标文件里全部命中（0 缺失）。

**上一轮（第十轮评审之后）的变化**

评审人这份意见的标题写「第九轮审阅意见」，但它审阅的是本方第九轮回填之后的正文——本方按「第 N 次收到意见」计数，记为**第十轮**，并把编号差异如实登记在正文文首，不靠统一编号掩盖内容。他本轮的总判断仍是**不建议冻结**，但他明确改变了问题的性质：**政策方向基本收敛，阻止冻结的是数据模型与实现合同**，并要求下一轮不再泛读，改为围绕四条数据流逐条对照「谁生产、保存在哪、谁消费、容量如何预检、超限怎样失败、用什么测试证明完整」。本轮正文变化九处，其中三处是本方上一版刚写下的东西被再拆一次：

1. **统计范围构造式改成两层，因为第九轮的写法会造成「假闭合」**（他的第 1 项，本方核对为本轮最重要的一处形式纠错）。第九轮写 `PmmPolicyUniverse = normalize(被判为普通内存候选的区间) − 前缀阈值`，而 `TypeExcludedRanges` 装的正是被类型强制排除的那两类；非 UEFI 腿还把设备树保留范围提前减掉了。**于是「范围 ∩ 统计域」可能是空集，分区式照样通过，而那些范围从未进入统计域、从未被真正统计**——这比第九轮要修的那个漏收更糟，因为它会通过全部测试。本方按他的方向改成 `SourceDomain`（来源声明什么就收什么）→ `PmmPolicyUniverse`（只减设备地址类），其余一律作为域内具名成员；本方另外把「先减掉前缀阈值」也改成域内成员 `ReservedPrefixRanges`，那是同一个错误的另一半。**本方与他的一步分歧**：`DeviceAddressRanges` 必须留在域外，否则它要进排除侧才能满足分区式，而那会否证第九轮已裁定的「设备路径属政策域之外」；域外只留这一类，且它从此必须有完整的不可截断存储。同时按他的第 3 项给 R5b 补上 `∀p`，并把 `AvailableRanges(p)` 定死为「政策上有资格」而不是「此刻可分配」——后者是分配器状态，只由阶段表表达，否则 S1、S2 会被迫把整个统计域解释成排除成员。
2. **宣布了一个成员却忘了写进等式**（他的第 2 项）。第九轮正文说设备树保留范围「本方选单列具名」，构造式里却没有 `DeviceTreeReservedRanges`。本方按他给的三层补齐，并且**不合并两个原始来源**：`DtbMemoryReservationBlockRanges`（文件头保留映射块）与 `DtbReservedMemoryRegRanges`（`/reserved-memory` 子节点 `reg`）来自设备树的不同部位、可靠性不同，归一化后合成政策集合但要带来源标签，标签在容量预检与失败信息里不得丢失。
3. **L2 禁止集合此前只写了子集关系，等于没定义；而「不必新增交接协议」那句被否证**（他的第 4 项与第七节）。第九轮只有 `BootMapDenyRanges ⊆ FinalMapDenyRanges`——那只说明 L3 是 L2 的子集，没有说 L2 包含哪些页。本方按他列的五个问题逐项给出具名成员构造式，并**点名哪些排除范围仍然允许最终直接映射**（`ReservedMappedRanges` 与启动对象页必须不在 L2 里，否则就是本文反复反对的「排除即禁映射」压位）。更实的一步是本方撤回自己那句话：跨边界传递的只有 `BootMapDenyRanges`，而 L2 成员包含诊断成员与类型强制排除，这两类按 §0.3 根本不跨界，**内核手里没有重建所需的输入**——所以 `FinalMapDenyRanges` 改为独立的完整跨界集合，与禁止清单同一档容量合同。并按他给的两条补上 R15（最终根实际映射与 L2 不相交的验收关系，明确它不是 R11 的替代品）与 R14（L2 禁止页不许进可用表），本方另加 R16 写清 R14 成立的前提，使 R14 可标为冗余并保留独立报警。
4. **R13 从子集关系改回四维合同谓词**（他的第二节第一条，本方承认这是他这一轮最尖锐的一条）。正文第八轮就把映射合同扩成四维，第九轮仍把 R13 写成 `StartupMappingRequirements ⊆ MappedRanges`——**子集关系只能表达物理页出现过**，表达不了虚拟偏移、权限、属性索引、共享性与别名冲突。本方核对：这正是本文第八轮批评过的同一件事换了位置（正文扩到四维、形式化缩回一维，而实现者与测试照的是形式化那一行）。改为 `MappingSatisfies` 并拆 R13a–R13e 五个可独立失败的子判据、各配一条具名用例。他另指出一处术语漏洞本方照改：字段允许「不要求映射」的记录其实不是映射义务，上位概念改为 `StartupAccessRequirements(树,阶段)`，`StartupMappingRequirements` 降为它派生的「必须有叶子」子集；「不要求映射」那一半从此交给 R15 与禁止集合的负向检查，不再放进映射义务记录里的一个魔法值。R10 同时按他的 A 项改：第一版**不留「经审其它安全访问路径」这个实际不存在的例外通道**，并按树选择禁止集合（引导根查 L3、最终根查 L2），修掉本方自己造出的「用一条引导根冲突关系覆盖两棵树」的漏洞。
5. **对象目录补齐，并把他给的二选一落在设备窗口上**（他的第三节）。§6.8 的身份枚举还停在第七轮的清单，而正文此后又承诺登记了复制后的设备树缓冲区、两块静态数组、登记表自身存储、义务派生暂存区等——**本方只在散文里说「它也要登记」，没有同步目录**。本轮按他给的三类（固定镜像 / 启动期分配 / 派生协议）逐块列出真实存储。中止路径要写的**串口寄存器窗口**他要求二选一：本方取「设备窗口不属于登记表、由独立的设备映射义务表承载」，理由是两类存储的容量、生命周期与失败形状完全不同，混在一张表里会让「装不下」这件事失去确定含义；同时登记一处最近的同形物 `identity_device_windows`（本仓已有，回答哪些设备窗口进引导根恒等段，但没有义务、授权来源与容量合同）。
6. **阶段边界从「寄存器写入」推进到「架构切换完成」**（他的第六节，且他限定四档不增加）。本方按他的判据改写：新根已写入且该架构要求的地址转换同步与指令同步步骤完成后，此后访问才按新根验收；过渡窗口内若仍可能访问某对象，就要旧根新根双侧覆盖或证明不访问。本方由此自加两条：两次切换各一条跨架构用例（构造「在同步序列之内访问只在新根下有映射的对象」并断言失败），以及双侧覆盖只针对登记过义务的对象、禁止集合在两侧都成立——否则「双侧覆盖」会退化成「两棵树都映射一切」，把禁止集合抹掉。他还问了 S1 的活动树归属，本方裁定取第一案：**固件页表属外部平台前提，不作为第三棵树纳入四维验收**，代价如实写出——S1 期间的访问需求不能声称已被 R13 验收，要另进一份平台假设清单。
7. **本方核出一条比「该信哪一份」更硬的事实：本文描述的那次复制从来没有发生过**（他的第九节）。他要求用调用链确认内核是否也改用复制目标，本方照做，读数三连：引导壳**没有任何**与设备树相关的复制动作；登记成平台描述来源的只是一个物理地址；内核 `parse_by_kind` 就按那个原始地址解引用解析，其安全注释要求该地址在调用期间保持可读。**所以本文第七轮起写的「原始缓冲区最后使用点＝复制完成」是一句描述不存在动作的合同**。他点出的冲突因此是现在时：规范允许设备树本体出现在文件头保留映射块里，而本仓第一版要把无用途授权的保留映射块范围判为禁止映射——于是会出现同一份物理内存一边被禁止映射、一边被内核按原始地址解析。本方按他给的五步数据流定成合同，并**第一次承认「不动固定布局」这个倾派对某个对象失效**：栈那一格仍不改布局，设备树复制目标那一格必须变更交接记录的意义。退化规则也按他的建议写死：不回退到磁盘那份，没有权威配置表条目就明确记为「本次启动没有设备树保留语义输入」并打进输出，不许静默。
8. **类型 12 的定性错误被指出，本方改判**（他的第四节）。第九轮本方写「按未知类型处理」——他核对得对：§0.2 把 12 列为标准类型，冻-7 与第 1 步针对的是**未知数值**，本方那句话等于把「已知但本项目不支持」塞进「未知」这一格，是本文一直反对的一词指两件事。本轮按其裁定改为**第一版直接拒绝启动**，不新增域外集合。本方加一条他自己提示的风险：拒绝启动的前提「三个目标架构的实际固件不输出类型 12」**本仓没有样本证据**，所以列为取证门；样本若显示有输出，改判域外完整保留，且那次改动必须带样本出处。类型 11 本方给出与 12 **故意不对称**的处置——通用 MMIO 描述在本项目三个架构上常规出现，拒绝启动会让自己的腿起不来，故走域外完整保留，并按他要求的「载体、容量、生命周期」把 `DeviceAddressRanges` 的存储、消费者与超限行为写实；这同时补上了第九轮 §0.3 第五条欠的那个域外出口。
9. **其余按裁定与读数落笔**：退出状态码之外新增三条诊断与容量合同——完整诊断成员必须一次性贡献给排除结果与适用的最终禁止结果、不得靠可截断样本恢复（本方并区分开「不跨界」与「不需要完整存储」这两个此前混在一起的概念），§3.3 问题 D 的输出清单按真实存储重写并禁止「只有数学定义、没有独立存储的集合」冒充缓冲区；x86 的 `CR0.CD/NW` 标为**项目准入加强**而非跨架构必要条件（否则会被读成「x86 一关缓存就违反普通主存语义」，而 §1.1 明说不要求平台有物理缓存）；RISC-V 原子清单的主键从 API 名称换成**真实指令实现路径 × 宽度 × 对齐 × 区域 PMA 等级 × 排序要求**，比较交换要区分 LR/SC 序列与 Zacas 的 `AMOCAS`、8/16 位要区分 Zabha 原生 / 宽操作模拟 / 锁回退（本方读数：内核与库层原子用法 122 处，`Zacas` 与 `Zabha` 在本仓文本里零命名）；设备树规范里 `should` 与 `shall` 的分级、保留映射块类型条款属建议性而本项目升级为严格，都按**项目加强**标注；映射义务 ABI 改为携带 **OS 内存语义**（`MemoryClass` / `Shareability`）而不是 `PAT 索引 / AttrIdx / PBMT` 这类架构编码值，并由各架构页表实现各自证明兑现——本方承认第九轮写那张表时没有按本文自己的抽象原则检查过。他给的七个开放问题裁定全部落笔，§0.5 与 §12.4 相应关闭，另有两问本轮新提出（受控环境是否真有配置表 DTB、`FinalMapDenyRanges` 与义务 ABI 的容量与布局版本）。

**本方本轮必须单独登记的一处：在档案与台账里写下了一条正文里并没有的修改。**第九轮本方在本文第一节第八轮条目末尾与阶段台账里都写了「设备树两条硬性判据已标注为项目支持子集而非规范强制」，但正文 §4.3 那一行**从来没有那半句**——本方第十轮按他的第十节去核对时才发现。这条比前几轮的「写了当成闭合」更严重一档：那些是把未完成的说成完成，这条是**在一处记录里宣称已在另一处完成**，而两处都不自证。本文此后加一条硬自检：**凡在档案或台账登记「本轮已改 X」，必须同时给出一条能在正文里检索到 X 的关键词**，并在收口前用该关键词回读正文一次。

**上一轮（第九轮评审之后）的变化**

第九轮没有新原则，评审人自己也这么说：这份文档的问题已经从「抽象没有定义」变成「不同抽象之间的边界还没完全闭合」。他的判断是**仍不冻结**，并要求下一轮不再做泛泛审阅、改为围绕代码材料做合同对照。本轮正文变化八处，其中四处是本方上一版写下的东西被再拆一次：

1. **统计范围补上分区式，并按启动方式分别定义**（§0.3）。上一版本方写了 `PmmExcludedRanges` 是完整集合、给了构造式，但**构造式只说明排除集合由哪些来源并成，没有说明可用与排除两者盖住了候选范围**——评审人给的形状是：一个既没进可用表又漏进所有排除集合的有效区间，当时那十条等式可以全部通过。补 R5b（不相交并集），R5 仍单列以便测试报告区分「漏收」与「重叠」。同时他指出「由两个区间来源与前缀阈值共同形成」这句把两条腿说成一件事——**UEFI 腿按 A 案忽略设备树的 `/memory`，裸机腿却以它为 RAM 候选**，故统计范围给出两个构造式。另按他的两条收紧：去掉 `OtherPolicyExcludedRanges` 兜底桶（否则将来漏任何新类别都能塞进 `Other`，来源完整性审计就白做了），每一项先与统计范围求交（R3 的定义域问题），以及**登记对象落在统计范围之外时不许靠并入排除集合掩盖**——本方本轮第一次把它写成「一律报错」，随后按中止路径的追溯要求核出裸串口寄存器窗口不在候选 RAM 之内，才发现那样会把一个合法的设备地址判成启动失败，因此收窄成按来源分两类：内存类对象越界即来源矛盾、拒绝启动；设备地址类本就不在 PMM 政策域内，只承担映射义务并改由设备窗口政策校验授权来源。
2. **一个集合同时兼两件事，被拆成两个**（§0.3、§6.8）。评审人指出 `StartupRequiredObjectRanges(阶段)` 同时被用来表达「这一阶段仍需访问」与「第一版一律继续排除、不自动归还」，而这两件事**会在对象最后一次使用之后分叉**——他给的例子正是本仓的现场：引导壳原始设备树缓冲区在复制完成后不再需要映射，但第一版仍不把它的页交还分配器。拆成 `StartupObjectPmmReservedRanges(阶段)`（归 R4、R7 管）与 `StartupMappingRequirements(树, 阶段)`（归 R10、R13 管），并写明「最后一次使用归前一阶段」只撤销后者、不能自动撤销前者。
3. **L2 层此前没有完整禁止集合**（§0.3 新增 R11、R12）。本方此前只承诺了 L3 那一小撮（`BootMapDenyRanges`），而 L2「不许进最终直接映射」的范围大得多；更要紧的是本文实际是**靠最终直接映射那四份候选来源「恰好没包含」来满足 L2 的**——本仓自己就记录过一次「一条保留描述符跨越候选对象与无关页时尾部没有叶子」的区间减法缺口，正是这种「恰好」失效的形状。补 `FinalMapDenyRanges`（由已有政策集合在规划阶段派生，不必新增交接协议）与 R11 包含关系；并按他指出 R1 只证明保留区「不许分配」、没证明它「真的进了最终直接映射」，补 R12。
4. **阶段边界本方上一版定的规则被他自己否掉，本方接受**（§6.8）。本方第八轮才用控制流证明引导壳中止路径跨过了页表切换，第九轮评审人指出：同一条事实**否证**本方第九轮之前写的「边界上最后一次使用归前一阶段」——若按「切换辅助函数还没返回」判归属，就会把已经在新根之下的访问记到旧阶段。规则改为按体系结构状态真正切换的时点定义，并把四档落成一张转换表（进入条件 / 退出事件 / 活动树 / PMM 可用性 / 边界归属），档数本身按他的裁定保留四档不再讨论。
5. **两份不同的设备树**（新增 §4.4.1b）。评审人说这条不要靠文字润色、要用调用链确认，本方照做，核出他怀疑的方向正确：磁盘镜像里那份 `/dumpdtb.dtb` 只解析 `/memory`（A 案删的就是它的内存用途），固件配置表里那份引导壳**只登记地址、从不解析**。由此得到的结论比「有混淆」更糟——**设备树的保留语义在 aarch64 的 UEFI 腿上今天没有任何来源**，§4.2–§4.6 那套政策在这条腿上是悬空的。本方给出取舍：只信配置表那份（磁盘那份与当前机器拓扑无证明关系，是第五轮 A 案同一套论证的重演），并留一条待他裁的备选。
6. **设备窗口的两个类型拆开**（§2.1 第 3 步）。评审人核对规范后指出 `EfiMemoryMappedIO` 是「固件为运行期服务请求映射」的那段，而 `EfiMemoryMappedIOPortSpace` 与 Itanium 平台架构的区域定义挂钩；本仓今天用一行 `matches!(... | ...)` 把两者合并处置。本方接受拆分，并把「三个概念（PMM 政策域 / 固件运行期 MMIO 描述 / 架构与设备树给出的实际设备映射窗口）不因都与设备有关而成为同一个集合」写进正文。**同时记下这是一周内第二次被 Itanium 专属条款咬到**（上一次是 8 KiB 对齐），因此新增一条自检：规范断言除出版物标识与日期外，还要标注它是通用条款还是某架构专属条款，无法判定时按「不适用本项目」处理。
7. **方案 B 的运行时选档残留清干净**（§7.1）。他直言这不是 B 的问题，而是文档没完成 A→B 的迁移：上一版一边选了 B，一边留着「唯一命中 / 零命中 / 多命中」这套运行时匹配规则。本方整段撤下，改为四步单向流程（构建期绑定档 → 运行时只用绑定档解释描述符 → 流水线记录并验证产物与环境对应 → 样本只作审核）。并按他对本方第 3 问的裁定保留一条**引导壳可独立触发的拒绝**，但把对象表述改准：拒绝的是「描述符违反构建期绑定档」，不是「固件身份不在白名单」——本方上一版把两件事写在同一个「拒绝启动」动作里，读者会以为运行时已经具备身份验证。
8. **权限、虚拟路径与别名审计三处收紧**（§6.8）。权限按他裁定改为**默认精确 R/W/X**（「最小 + 上界」里的上界是开放解释口：代码页登记 `RX` 给 `RWX` 算不算合格，不能由最小权限自动决定），并引导根的宽权限不得扩散成最终根的默认要求；**新增页级权限冲突检查**（两个对象字节范围不同却同页，分别通过对象级检查仍不可能同时满足）；虚拟路径要比较具体关系而非种类（恒等即 `VA == PA`、直接映射是固定偏移且偏移计算不得溢出、专用路径要有区间与授权来源）；别名一致性审计从「属性与共享性」扩到含**权限**，否则同页一个别名只读可执行、另一个可写，逐条叶子合格而整体 `W^X` 不成立。评审人对引导壳映像那一格还要求把该路径依赖的字符串、静态数据、栈缓冲与**输出设备访问路径**一并追溯——本方照做后立刻撞上第 1 条末尾那条新判据（串口寄存器窗口不在统计范围内）。**本方本轮又把它收窄了一次**：一律报错会把一个合法的设备地址判成启动失败，所以判据按来源分两类（内存类越界即矛盾、拒绝启动；设备地址类本就不在政策域内，只承担映射义务并按设备窗口政策校验）。这是本文第一次出现「新合同差点制造一个新误判」的现场，正文 §0.3 与 §0.5 都按分两类改写。

另外四条按他的裁定直接落笔：`ExitBootServices` 的重试按状态码分三类（只有版本戳不匹配可重试，其它状态码直接终止——本方核出配对循环今天不区分，登记为现存缺陷第五条）；`CR0.CD / CR0.NW` 检查失败按 fail-closed，且**不得靠顺手清位解决**；RISC-V 平台清单落代码常量表、文档作证据附件，宿主测试断言识别串唯一命中与字段齐全；x86 普通页与大页分别验收且做「索引位 + 真实树遍历」两层。原子能力登记细化到**操作类别、宽度、对齐、排序强度与证据来源**。物理地址上界统一记号为 `effective_pa_limit = min(platform_pa_limit, translation_format_pa_limit)`，并纠正本方「Sv39 只有 48 位」的写法（Sv39 的页表项字段可表达 56 位，48 是当前平台配置值）。

本方本轮**没有反驳任何一条**：八处修改里有四处是本方上一版刚写下的东西被再拆一次（R4 的双重语义、阶段归属规则、统计范围的一句话定义、§7.1 的选档残留），这本身就是上一轮「把写了当成闭合」的续集。他最后那句关于不要扩大抽象数量的限制，本方当作本文的一条收尾纪律接受：**第九轮只拆已有集合之间的边界，没有新增任何顶层抽象。**


**本轮（第八轮评审之后）的变化**

第八轮没有新原则，全部是把第七轮刚立起来的两份合同推到「可被代码与测试证明」的程度，外加一处旧文复活与一条从第二轮起就漏掉的硬件状态。评审人的总体判断是**仍不建议冻结**，本方同意。本轮正文变化八处：

1. **PMM 排除集合的「完整」以前是空的**（评审人列为第一条先决项，本方核对为实质缺口）。第七轮把 `PmmExcludedRanges` 定义成「所有不许分配的页的完整集合」，但 §2.1 第 3 步判为「排除」与「设备路径」的那两类去向**在本仓根本不产生区间**——三处匹配（预检计数、写入分发、提交落盘）全部落在空分支，连长度都返回零。于是「所有来源经同一规划过程汇入」这句话对这两类来源是空的：**即使 R1 到 R10 全部通过，也证明不了排除集合覆盖了被类型强制排除的范围**，因为那些范围从未进入任何集合，等式两边都没有它们。本方按评审人的建议补三样：定义统计范围 `PmmPolicyUniverse`（取「进入本项目内存政策的物理范围」，不是全部地址空间——否则地址空洞与从未被声明的地址都要求存进一个有限容量集合）、给出**构造式**而不只给关系式、把验收拆成关系测试与**来源完整性测试**两组。`TypeExcludedRanges` 按评审人的说法**不必成为另一份跨交接边界的协议**，但必须有完整的生成过程；§5.1 那条「排除与设备路径不产生输出」的现状因此从政策结论改登记为**待修正的实现事实**。
2. **`StartupMappingRequirements ⊆ MappedRanges` 过弱**（评审人列为第二条先决项，本方完全同意并承认这与本文反复纠正的是同一个毛病）。他给的例子是一针见血的：一个内核代码页要求在最终根里通过直接映射别名访问，实际却映射到另一个虚拟地址、权限写成可写不可执行——纯物理覆盖看这一页确实在 `MappedRanges` 里，等式通过，**访问合同一点也没兑现**。本文从第三轮起一直在说「不要用一位压两件事」，这里恰恰是用「有映射」压了四件事。合同按**物理覆盖 / 虚拟访问路径 / 访问权限 / 内存属性与共享性**四维重写，每维各自可验收、各自有用例；并说明属性与共享性两维不是新要求，而是**第一次把裁决一的承诺接到具体对象上**——此前那份承诺只有全量遍历一种验收方式，没有可比对的清单。
3. **映射要求字段改为按 `(树, 阶段)` 索引**（评审人第 4 项）。单个四选一枚举无法表达同一个对象在引导根里靠恒等段存活、在最终根里要靠别名存活；本方核对 §6.8 自己那张表就写着两棵树要求不同，却给了一个全局字段——这是本文第六次把「同一时刻都为真的多件事」写成「一个对象」。
4. **方案 B 落成形状并关闭**（评审人第 7、7.2 项，且明确说这项本轮可以直接定、不必再问他）。配置档改为**编译期绑定到具体引导壳构建产物**，固件镜像身份与模拟器的版本 / 机器 / 处理器参数由测试流水线记录与固定，样本只作符合性检查，引导壳第一版不承诺运行时身份识别。本方特别采纳他那条边界：**不能把「外部支持前提」写成「运行时 fail-closed 已保证」**——引导壳能对不符合绑定档的属性组合与未知位失败，但无法判断一个身份未知的固件是否就是清单里那份。§7.1 的标题、身份表、§8 依赖图第 0 步、§13 两条取证项随之改；「登记项上限为 1」随运行时选择器一起作废。本方并把他提醒的三件事拆开写：档有标识、档怎么绑到产物、测试环境如何证明用了对应固件——此前本文把它们当成一件事。
5. **三条生命周期补入 §6.8**（评审人第 6 项），其中一条**修正了本方自己第七轮的推理**：设备树快照的复制目的地要作为独立对象登记（第七轮只登记了原始缓冲区，落下「原始指针已失效但消费者还在用复制对象」的另一半）；登记表**自己的存储**也要登记（本方据此推广为一条自检规则：任何要求「必须能访问」的清单，其自身存储都要进清单——本文对禁止清单已经用过一次，登记表是第二个例子）；引导壳中止处理路径的可达阶段**要用控制流证明而不是从注释读行为**。本方核到三处读数：进入内核那个函数返回 `!`、**但页表切换发生在这个函数内部**、且切换之后同一段里仍有会中止的调用。结论比第七轮更精确也不同：这段路径跨的是**引导根的启用**而不是切到最终根，因此它需要在引导根启用后到控制权真正转移前仍可取指与读写，**不要求**在最终根里保持映射——评审人举的「引导壳映像不应因被登记就自动成为最终直接映射候选」的例子，本方按控制流确认成立。本方第七轮给的理由（「注释写明它驻留」）是**从注释推行为**，是本文批评过三次的事，本方第四次踩在同一类上。
6. **栈的物理范围改法**（评审人第 5 项）：本方第七轮写「由分配时的类型与内存图快照推出」，他指出这条推法不成立——一条加载器数据描述符可能覆盖多个对象，说不出其中哪几页是栈；虚拟栈顶也推不出物理底部。本方核对交接结构确实只有虚拟栈顶、没有物理栈底字段，据此改为他给的三条：从**分配点**登记、或从**已有页表映射**取并验证覆盖、两者都取不到才考虑扩展交接结构。结论与第七轮相同（不改固定布局）但**理由完全不同**：第七轮是「不改布局更省事」，本轮是「布局里根本没有可推的字段」。
7. **来源收集缓冲超限一律拒绝启动**（评审人第 11 项，本方采纳并收紧自己此前「失败或至少打诊断」的措辞）：诊断样本截断可接受（不改变政策结果），**来源收集缓冲截断会改变政策结果**——漏收一条保留范围就等于把本不该分配的页交给分配器。本仓今天的静默丢弃因此是现存缺陷而不是可接受行为。同一项他还要求把父节点单元数与 `ranges` 为空这两条硬性判据标注为**项目支持子集**而非规范强制（规范的措辞是建议级），本方核改并加一句防错说明：不标注会让将来有人把它当兼容性缺陷去「修规范」。
8. **x86 补 `CR0.CD / CR0.NW` 一条**（评审人第 8 项，他说这条「实现成本相对低、最容易被遗漏」，本方核对完全同意）。本文从第二轮起要求自建 `PAT`、分析 MTRR、逐核核对有效类型，**却始终没有写缓存禁用状态**——而那张有效类型组合表是以 `CD = NW = 0` 为前提的。本方按读数补一条比「从未检查」更具体的事实：全仓 5 处写 `CR0` 的位置**每一处都是读出后只改自己关心的位再写回**【**第九轮标注**：「5 处」是逐文件数出来的，本方当时给的现量命令命中 6 行——多那一行是文档注释而非写点。正文已把命令与读数一并改写，并据此新增一条取证规则。】（改的是 PE/PG/TS/MP/NE/EM/WP），**没有任何一处读、检查或清零 CD 与 NW**，即固件留下的缓存禁用状态会被原样透传进内核。本方另核到一条有利的同形先例：分页启用那处注释明写「UEFI 固件不保证 WP 已启用」并显式置位——**说明本项目已经承认某些固件位不可依赖，只是同一认知没覆盖到缓存控制位**。
9. **形式问题**（评审人第 3、12 项）：R1–R10 的「阶段」量词补齐——第七轮把阶段加到两个集合上却没给其余条目量词，导致「R6 由 R2 与 R5 推出」在形式上不成立；本方按他的写法给每条加 `∀p`、把阶段无关的集合在记号约定里一次声明，并接受他由此引出的那条必须先定的约定：**阶段边界上的最后一次使用归属前一阶段**（本方给的取舍，若他认为归属后一阶段更安全可由他定）。另把「十条等式全部通过」明确降格为**只证明集合之间不矛盾**，覆盖面要由来源完整性测试证明。

本方另外核改他列的旧文四处：§12.3 第 5 条里复活的 8 KiB 句子**整句删除而不是加旁注**（他的判断准确：留着会让读者以为本方的单向立场依赖一条已撤销的规范事实，而它不依赖）；§11 的标题轮次与 §12 不一致（本方第七轮改了 §12 忘了 §11）；§12.4 里 RISC-V 平台清单那一行重复出现两次（本方第七轮插入时留下的）；§13 两处早期「做完这些就够了」的清单标注为历史判断。§0.2 对属性的整体定义也按他的第 9 项改写——原文「属性是那块区域支持的能力位图」把异质的四类东西压成一种，与本文对运行期位自己的用法矛盾。

**第八轮之后本文仍然不建议冻结**，并把范围收窄到「把这两份合同变成能被代码与测试证明的东西」+ 一次完整交叉审计；不推翻任何已签署的架构原则。

**上一轮（第七轮评审之后）的变化**

第七轮意见的开头是一条**规范事实纠错**，而不是一项设计偏好：本方第六轮写进 §2.1 的「带运行期属性位的描述符必须 8 KiB 对齐、长度为 8 KiB 整数倍」**不是本项目三个目标架构的规范要求**。评审人核对 UEFI 2.10 官方文本与其 Errata A 页面，两处都是 4 KiB；他又交叉检查 2.9A 的架构章节——x64、AArch64、RISC-V 对应要求同样是 4 KiB，**8 KiB 那一支出现在 Itanium 架构章节**，不能泛化。本方接受并**撤销该条判据本身**，而不是在「严格报错」与「记录后继续」之间替他选一个——被撤销的不是一条严格度选项，而是一个不属于本项目的要求。本方自己的检索读数与此冲突（同一位置，官方页面给 8 KiB、另一份公开翻译给 4 KiB），冲突如实登记成表，并承认这是本文对自己 §7.1 纪律的一次违反：本方当时没核对那句话属于哪一架构分支。

其余六条本方逐条核对，**四条采纳、一条采纳并部分修正前提、一条为计数纠错**。正文变化九处：

1. **8 KiB 判据撤销**（§2.1、§7.2 第 12 项、§8、§12.4、§13 同步）。现有解析器检查 4 KiB 起点对齐，与三个目标架构的该项要求一致，不需要改动；原始长度溢出、描述符步长、页数为零等校验各自独立成立，不受本条影响。
2. **登记表接上三个真实下游**（本轮最重要的结构性修正，§6.8）。第六轮它只有「与禁止范围冲突检查」一个消费者，而本方自己在 §6.8 记录的历史故障起因是内核镜像与栈**没进入最终直接映射的构造来源**——只接冲突检查等于没解决那次故障，还会让人以为已经解决。改为按字段派生三种结果：分配排除、阶段访问要求、**页表映射要求**，第三种写成合同 `StartupMappingRequirements(树,阶段) ⊆ MappedRanges(树,阶段)`，按两棵树与阶段两个轴分别验收。
3. **补上「活对象不得被分配」这条集合约束**（§0.3 的 R4，第七轮新增）。评审人给的错误形状本方核对成立：一段加载器数据进可用表、其中一页放着启动栈或页表页、内核起来后分配器把它重分配、而对象最后一次访问尚未发生。规范背景是加载器退出引导服务后仍在占用若干加载代码与数据区间（评审人核实并给出出处，本方未逐字核对）。
4. **集合等式由七条扩到十条**，并把「阶段」同时加到排除集合与可用集合上；第一版按保守档执行——**对象一经登记就在全部后续阶段保持排除、不自动归还**，因为本仓没有任何一处定义过释放点。
5. **物理地址上界单独立节（§4.4.4）**。评审人指出把格式上限当实际能力上限会接受当前分页配置根本访问不到的区间。本方核对结果比他预期的更微妙：**本仓不是没有这个概念，而是有格式上限与能力上限两份实现，且用错的那一处恰好是内存图准入**——内核侧三个架构各有一份「能力与格式取小」的访问器（x86 读 CPUID 扩展叶子、AArch64 读 `PARANGE` 且低于 48 位直接判不支持、RISC-V 取平台位数与编码位数之小），而引导壳分类器用的是编译期常量，其注释自己写着「将来由分页层同名访问器取代」。本方按此**修正评审人的一条前提**：RISC-V 不是没有取值（Sv39 下为 48），缺的是「那个平台位数是写死常量而非按平台登记的证据」。另按他的 4.1 给出越界处置的三档来源表，并规定**不得存在未区分来源的默认裁剪**。
6. **白名单「第一版只登记一条」作为解决方案撤回**（§7.1、§13）。评审人给的两种情况本方核对都致命：字段列为必需则本仓读不到 → 零命中 → **本文的算法拒绝本项目自己的三条腿**；声明不约束则那条登记项只按架构与启动方式匹配、命中任意同架构固件，「唯一命中」是虚假的唯一性；样本补不上，因为本方自己已规定样本不是选择器。改为必须在「来宾身份闭合」与「限制支持环境并诚实声明」之间显式二选一，本方选后者并保留 fail-closed。
7. **十条等式的计数纠错**（§0.3）。评审人指出本方「七条里有两条冗余」数错了——七条里只有一条可推出。本方补入「可用 ∩ 可映射保留区」使该说法在十条版本下成立，并把「编号写在左边、推出依据写在右边」定为本文的写法：数量与用例编号由表自己决定，不由散文里的数字决定。
8. **受限阶段禁止再读配置表**（§3.4，关闭第六轮本方留的追问）。本方同时补两点准确性说明：直接读系统表所在内存与调用引导服务函数是两件事，规范没把只读访问一律禁止，**因此这条是项目为可验证性选的加强约束而非合规要求**；并核出一条连带——本仓 aarch64 那条腿的设备树快照来自配置表查询且其原始缓冲区还被直接映射来源引用，所以「首次尝试前完成复制」与 §4.3 那条复制规则必须落在同一个动作上。
9. **登记表完备性改用可追溯性判据**（§6.8，采纳评审人给的问法替代本方的项数问题）：对每个会跨越「内存图改变 / 页表切换 / 分配器启用」三类边界的对象，能追溯其物理范围、最后使用阶段、以及在每棵树上的访问方式。按这条判据本方**自查出两类漏项**并补入：引导壳自身代码映像与栈（那段刻意不用堆、把诊断写进栈上定长缓冲的中止处理路径正跨越边界）、以及原始内存图缓冲区与归一化规划工作区（本方核对：与分类器输出存储**是两块不同存储**）。

**第七轮之后本文不再自称结构可冻结**（§8、§12.4、§13 同步撤销第六轮那句）：三个结构性缺口——登记表下游未接全、缺可执行的运行时身份键、物理地址上界语义未统一——按评审人的话留作下一轮范围，且本轮重心从「文档写清楚」移到「代码接得上、用例测得到」。本方对「什么算闭合」的标准也据此收紧：**一个抽象只写完字段与生产者不算闭合，它的所有下游消费者都要点名。**

**第六轮（评审之后）的历史变化**

第六轮意见的判据与前一轮一致：架构方向不再讨论，且明确认可 §0.3 把「区间数学 / 多个语义集合 / 集合之间的关系」分成三层。本轮提出八项问题，其中一项是本文档自己的内部矛盾（本方核对后确认成立）、一项是数据流的缺失输入、其余是关系不完备、合同不闭合与旧文未清。本方逐条核实后**七条采纳、一条部分采纳并给出反驳**（反驳的是评审人自己提出的那条双向判据，见下）。正文有九处实质变化：

1. **修掉一处本文档自己的分类矛盾，并因此改掉一个冻结条目**。第 4 步（旧编号）说候选类型带 `RUNTIME`/`NV`/`SP` 任一位就成诊断成员，而第五轮补的交叉合同又说「普通类型带 `RUNTIME`」判分类错误——同一个 `LOADER_DATA + RUNTIME` 两处判法相反。修法按评审人建议：把一致性校验单独成一步、排在属性裁决之前，属性裁决只处理通过校验之后的组合。**为此冻结条目冻-6 的措辞由「五步」改为「六步」**——这是本文第一次改写冻结条目，改的是步数与顺序，各步政策内容不变（§2.1 开头专段说明）。
2. **本方反驳评审人自己的「双向等价」判据**（第六轮唯一不同意的意见）。规范把「运行期内存」的身份落在**属性位**上（凡请求运行期虚拟映射即用该位表达），把「退出引导服务后必须保留」落在**类型**上；两句回答的问题不同，推不出双向必要。（本方第六轮在此处还多写了一句「且据此要求带该位的描述符 8 KiB 对齐、长度为 8 KiB 整数倍」——**第七轮撤销**，那一支属 Itanium 架构章节，本项目三个目标架构的要求是 4 KiB；撤销理由与两处读数冲突登记在 §2.1。本条反驳的结论不受影响，因为它依赖的是「身份落在属性位」这一句，不是那条几何要求。）若按字面实现「普通类型带 `RUNTIME` 即分类错误」，会误杀设备窗口类型带该位这种形态（本仓现在第 3 步就把设备窗口类型引走，所以今天不撞，但任何双向实现都会在设备窗口上炸）。第 4 步因此**只查一个方向**，并新增两条方向性用例专门防止有人改回双向。
3. **补上数据流缺失的第四个输入：`StartupObjectInventory`**（评审人称本轮最大新缺口，本方同意）。第三行输入删掉 phantom 之后，输出侧的「启动必需对象」没有任何来源——内核入口、启动栈、两棵页表树的页、交接结构、分类器输出数组、原始设备树缓冲区、禁止清单自己的存储，都不是固件或设备树声明出来的。新增 §6.8 定义它的五个字段、生产者（引导壳，组装于分类之前）、消费时机与容量纪律，并写明它**不是**第三种内存来源、**不是**另一份保留区间表。
4. **集合关系补全**（评审人指出五条不完备，本方核对为真漏）：`ReservedMappedRanges` 与诊断成员同样不许分配，等式里却没写。改为七条，并把 `PmmExcludedRanges` 明确定义成「不许分配」的**完整集合**（所有来源经同一规划过程汇入），同时强调汇入同一排除集合不代表映射政策相同。其中两条是可推出的冗余，本方有意保留以便逐条对应宿主用例，并请评审人若要求最小无关集时指明删哪几条。
5. **区间溢出合同与既有实现对照后改写**：本方核实既有的页对齐裁剪纯函数对 `start + len` 用的是**饱和加法**（注释自述夹到地址空间顶端），与新合同「溢出即拒绝」并不天然一致。改为明确**两步走**（先 `checked_add` 验原始长度、溢出即拒；再取整与钳制），并指出本仓已有一处正确样板可引为规范（分类器的长度检查就是连续两步 checked 运算、三种溢出各有独立错误码与用例）。取整方向同源、溢出策略不同源，两件事分开写。附带核出**一处现存脆弱性**：引导壳那条「常规内存不得与自举分配相交」的断言用裸加法算末端，今天因上游已校验而不可达，但任何绕过分类器的新调用都会拿到一个静默失效的守卫。
6. **上界表示与代码归属给出可执行结论**：独占上界本仓现有取值（AArch64 `1 << 48`、x86-64 `1 << 52`）都远小于 `u64::MAX`，因此 `end == 上界` 可表示、不需要 `+1`；若将来出现 64 位全地址空间平台，必须改「末元素」语义或提升到更宽中间量。**依赖图按读数核实**：引导壳**已经**依赖内核库，所以「不能反向依赖」这个前提在本仓不成立——但沿那条边去用内核里的区间函数仍是错的方向；两个消费方的交集是 `minix-boot` 与 `minix-types`，故新增的数学落在那里，不复制函数。
7. **RISC-V 合同补第二个独立维度**：评审人指出只有原子性不够，规范把内存排序也列为按物理区域描述的属性，一致主存与非一致主存、设备区的排序语义不同。本方采纳并加两列（区间是否为一致主存、采用哪种排序模型），把准入判据收紧为「一致性证据 + 排序模型覆盖内核依赖 + 两类原子能力达最低合同」三条同时成立。理由与本文一贯口径相同：本项目对普通主存语义的承诺本来就含「多处理器共享时保持一致」，而**支持原子指令不等于多核看到一致的顺序**。本方另核实这一列今天是空的（没有任何按物理区间登记的一致性证据）。
8. **白名单区分「运行时可读」与「流水线证据」，并修掉一处循环定义**。本方承认第五轮那张表把配置档标识同时写成输入与输出，按字面实现就是「要选档先知道档」。改为两层表：只有运行时可读的身份字段参与匹配，配置档标识由唯一命中**返回**；模拟器版本、机器与处理器参数、固件镜像哈希归测试流水线上线，不与运行时诊断混在一条行里。另补三条规则：缺必需身份字段即不匹配、未定义的通配一律不允许、外部证据不得冒充运行时身份。本方按读数加一条现实约束：**第一版实际可读的身份维度比表里列的更窄**（厂商与厂商版本在本仓零读取点），因此建议第一版只登记一条已核实组合。
9. **旧文清理（评审人点名四处，本方逐处核改）**：§4.4.1 删掉「先做 C、日后升 B」的落地顺序与那条请裁定问题（第五轮已定 A）；§4.7 两个剩余问题都改为已关闭并给出关闭方式；§8 第一张阻断表那行的三输入名单标注为过时并指向 §0.3 现行名单；§13 那条「出界闸改用固件交出的设备树」的取证项与 A 案冲突，整行重写为按 A 案拆除的判据。§4.4.1 与 §13 各补一条澄清：A 案删的是把 `/memory` 当 RAM 来源与准入判据，设备树快照本身仍要读（平台描述与保留子树都在树里）。

本方另外补三条口径统一：运行时类型与属性不一致的严格拒绝、`UNACCEPTED`/`PAL_CODE` 的整机拒绝、`HOT_PLUGGABLE` 与 `SP` 的排除处置，全部显式标注为**项目加强政策**，不得写成规范要求——评审人要求统一所有类似表述，本方逐行核改，并在 §2.1 加上一条前提：必须先确认没有合法固件布局被这条政策拒绝；重试阶段按评审人裁定定为**三档**（可用 / 受限重试 / 已退出），且「首次尝试后连内存分配也不用」标注为本项目为加强可验证性选的约束、不是规范允许的最严边界；§3.4 与 §12.4 的两处待裁定粒度据此关闭。

**第五轮（评审之后）的历史变化**

两份第五轮意见（一份聚焦缺口、一份通读全文）方向一致：架构方向不再讨论，缺的是几处**数据合同本身**。本方逐条采纳后，正文有八处实质变化：

1. **属性档要有自己的身份**：§2.5 那张表不是任何一份规范出版物的档，而是以依赖库头文件布局为基础的实现兼容档，改称 `MEMATTR_PROFILE_UEFI_RAW_0_9`（身份是项目内部的，不宣称规范归属）。本方核实：钉死的依赖库里**没有**「可热插拔」这一位，而主流实现头文件把它定义为 `0x100000`——所以它在本档里是**档外扩展位**，§2.5 现在把这一行明写出来（此前是漏，不是选择）。
2. **白名单的键与算法补齐**：厂商串与厂商版本都不是构建身份；条目补齐为八个维度（平台身份、架构与启动方式、固件构建身份、证据来源与日期这四维是本轮新增），并给出选档算法（唯一命中→用；零命中/多命中/样本违反该档→拒绝启动）。**样本是验证证据，不是选择器**——某个位没出现在样本里，不能证明固件以后不会给。
3. **归一化阶段产出多个语义集合，不是一个统一禁止集合**（这是本轮最重要的自我纠正）：上一版把「一份集合」写成了统一禁止范围，会把普通静态保留区（允许映射、不许分配）一起禁掉，或者反过来为了保住映射而弱化禁止。改成：唯一的区间数学与页级归一化 + 规划阶段的四行输出（PMM 排除 / 映射禁止 / 按阶段的启动必需对象 / 来源诊断），它们与分类器自己的两类输出合起来共六个集合，相互之间用**五条集合等式**约束（§0.3、§6.7）。
4. **「版本」拆成三个互不相关的字段**：系统表修订、**描述符版本**（读图服务自己返回的、规范单独定义为 1）、属性配置档标识。描述符版本 1 **不**蕴含属性位布局 1；三个都要进诊断。本方核实：本仓已经在校验描述符版本（不受支持即整图作废），但摘要行与致命行都不带它。
5. **一处隐式政策被抓出来**：`RUNTIME_SERVICES_CODE/DATA` 在正文里没有类型行，靠第 5 步那条兜底分支才没落进普通映射——本方核实它确实落到 `_ => 诊断成员`。这违反本文自己的验收判据（每个标准类型都要有明确政策），已在 §2.1 补显式行，并加上「类型与运行时属性位不一致即分类错误」。
6. **`PERSISTENT` 的「保留完整范围信息」承诺被删**：它和有界、可截断、不跨界的诊断样本没有共同载体。第一版改为只承诺总数、总字节与有界样本，将来实现持久内存支持时另立来源。
7. **出界闸的分歧关闭：选 A**。UEFI 腿不再读设备树的 `/memory`（无论是否参与准入）。本方上一版坚持的「先修输入、停在只做诊断」被两位评审人共同否定，理由成立：既然这条腿不该让 `/memory` 参与决策，留着它就要长期维护一套不属于本政策职责的跨来源一致性判断，而输入还是从磁盘镜像读来的。拓扑一致性检查若将来确实需要，作为独立的平台一致性验证另立。
8. **一处 §0.3 的输入画错了**：数据流图里的「启动信息保留输入」在本仓**不存在**（本仓只有一个前缀阈值与一份尚未读取的设备树保留映射块；交接结构里的 `reserved_regions` 是分类输出而不是输入）。已从图中删除。

另外采纳：§0.1 把 QEMU 启动称为「真机」是措辞错误，改成三档证据名称；设备树的 `reusable` 单独出现、多段 `reg`、`/memory` 的 `hotpluggable` 三条补上第一版政策；区间数学要一份能直接编码的合同（本仓已有一条同源实现可复用）；第一次退出尝试之后要有受限的重试阶段类型；诊断字段要按来源分层并区分两种计数口径；x86 的 PAT 与 MTRR 组合机制的表述纠正（不存在一个叫「跟随 MTRR」的类型）；MTRR 分析器要做成有独立输入输出与合成用例的纯计算件；RISC-V 的原子性从「平台有 A 扩展」推进到「按物理区间分级」（LR/SC 与 AMO 是两条独立能力）并升为平台准入合同而非验收细节；§6.6 与 §2.1 的两处内部矛盾修掉；§6.1 表里「被排除的区域不许访问」一句删除。

**本方在第五轮补的三条仓库事实**（评审人看不到代码）：设备树收集器确实遍历了 `reg` 的全部地址/长度对，**但超过 8 条时静默丢弃、既不失败也不记诊断**——多内存条的机器会少识别 RAM；`os/kernel/src/memmap.rs:cut_memmap` 已经是一条「按页向外取整 + 饱和处理末端」的既有纯函数（复刻 C 行为并有宿主用例），归一化数学应与它同源而不是再写一套；配对循环体内目前没有任何引导服务控制台写入，但「第一次退出尝试之后不得依赖固件协议」这条约束现在是**巧合成立**，没有类型或断言保证。

**第四轮（评审之后）的历史变化**

1. **§7.1 的选档机制被推翻**：`EFI_SYSTEM_TABLE.Hdr.Revision` **选不出配置档**——2.10、2.10 Errata A 与 2.11 都用同一个系统表修订值 `2.100`，而本仓钉死的依赖库把 `2.100` 直接打印成 `2.10`（本地可复核，见 §7.1）。改为**项目登记的固件组合白名单**（固件身份 + 配置档标识 + 原始属性样本），修订值与厂商字段降级为诊断记录。
2. **本方撤回上一轮「2.10 掩码已核实」的表述**：本方取到的是标注为 2.10 的 HTML 页面，无法确定它对应 2022 年原始发布还是 2024 年勘误版；评审人同时撤回上一轮对本方结论的确认。四份正式材料的读数并列登记在 §7.1，本文不推测哪一份写错。
3. **§2.5 逐位表改为按配置档标识键控**，并修正 `WP` 与 `SP` 两行：规范的分类陈述与本项目的加强政策不能混写。
4. **§6.3 补一条强制的页级归一化合同**：向外取整后的**同一份**区间集合必须同时用于可用表切除、禁止清单生成、启动对象冲突检查与页表覆盖验收——否则同一个物理页会一边「不许映射」、一边「可以分配」。合并时不得丢失来源证据，因此把「交接用的不相交区间集合」与「引导期来源诊断」分成两个对象。
5. **§0.3 的输出表正式列出第四种输出**，并写明它不是 UEFI 分类器的输出：三个来源汇入唯一的归一化与集合规划阶段，由该阶段产出跨边界的禁止清单。
6. **§3.3 补阶段对象的所有权不变量**：Rust 的类型不会因对象被丢弃而清零静态存储，所以「不可提交」必须配合有效长度、独占访问与每轮从零计数。
7. **§4.3 把「固件常驻区」重新归类**：它是一个**前缀上界阈值**，不是第三个保留区间集合，改名以免被当成可与任意保留范围取并集的东西；并补上保留映射块与设备树本体、启动模块存储相交时的生命周期规则。
8. **§1.3 与 §1.5 补两条验收维度**：AArch64 的共享性字段与属性表寄存器是两个维度；RISC-V 的平台清单必须含原子操作能力的证据——平台识别只回答「这是什么机器」，不回答「它能保证什么」。
9. §0.2 措辞修正：4 字节补齐位**已包含**在版本 1 的 40 字节长度里，不是「40 字节之外另有 4 字节」。

**第三轮的变化（仍然成立；只有其中第 1 条关于版本门控的写法被本轮再修正）**

1. **版本门控的写法被推翻重写**（§7.1）。上一版写的是「2.11 才出现的位按基线外处理」，这句话允许整张图继续进入后续分类——而同一个 `0x10000` 位在 2.10 配置档里是「更可靠」，在 2.11 的发布文本里被列为「可热插拔」，误读是静默的。规则改为**按固件声明的修订版本整体选档或整体拒绝**，且不得把 2.11 的属性字交给 2.10 分类器。评审人已撤回上一轮对本仓 ISA 掩码的质疑；本方另核实到 2.11 的正式变更清单把该项记为「新增热插拔内存属性」，没有记载既有属性位重新编号。
2. **§1.1 的 x86 机制描述本方写错并已修正**：选取 `IA32_PAT` 表项的是 `PAT`、`PCD`、`PWT` **三个位**（4 KiB 页与大页的 PAT 位位置不同），不是 `PCD`/`PWT` 两位。这条不是措辞问题——后面要分别改普通页与大页两条构造路径，把它当成两位会漏设或设错位置。
3. **§4.4 撤回一条本方结论**：「不拿 `/memory` 当来源、只当闸门」并不能宣称为符合规范的「忽略 `/memory`」——它参与了启动准入决策。改为三选一的显式项目策略，本方选「只做诊断、不改变准入」。
4. **§4.3 把保留来源拆成三个名字**（设备树自身的保留映射块 / `/reserved-memory` 子树 / 项目启动信息的保留区间）——第四轮把第三项再修正为前缀阈值，不是区间集合，并按仓库读数补两条：依赖库早已提供设备树保留映射块的解析接口，本仓**零调用点**；本仓现有的「固件常驻区」是一个**上界常量**而不是区间列表，因此表达不了高于该上界的保留。
5. **§3.3 的阶段类型补一个中间对象**：上一版的四条类型约束完整适合路径 A，但与路径 B 矛盾（路径 B 的分类发生在退出之前）。补入「预备分类结果」后两条路径才能在同一个阶段对象上汇合。
6. **§6 补三条页表级实现合同**：禁止范围必须按最小页粒度兑现（拆到 2 MiB 不够）、与启动必需对象相交时要做冲突检查、清单要作为第四种输出纳入第一遍预检的硬容量检查。并修正层次归属：规范的两条要求是 L2 + L4，L3 是本项目的加强。
7. **§2.5 新增全部已知属性位的逐位处置表**——「每个已知位都有明确处置」此前只是验收判据，本轮在正文里兑现。
8. **§6.7 新增一条建模纪律**：本轮已经出现六个语义不同的物理范围集合，禁止再用「往枚举里加一个值」的方式合并它们。

**第二轮的变化（仍然成立，只有上表第 1 条把其中的版本策略再收紧了）**

1. **推翻上一轮的一处事实断言**：上一轮称本仓的 ISA 属性掩码比规范多覆盖一个 nibble。本方取到 UEFI 2.10 规范原文、EDK2 头文件与 Linux 内核头文件三处比对，**本仓取值与三者一致**；该断言不成立，据此提出的「先修掩码」阻断项撤销，改为「把取值来源写成配置档」。证据见 §7.1。**本条的补充说明**：当时当作「规范原文」的那一份材料没有出版物标识，因此本条结论已在第四轮被本方自己撤回，只保留「四处读数一致」这一观察。
2. **裁决六的甲案被否**，本方接受并改推「收窄版乙」：诊断样本与「启动映射禁止清单」是两个对象，只有后者跨交接边界且不可截断。同时补一条只有读代码才能发现的边界：交接之前那段页表由固件建立，禁止清单管不到它。
3. 按仓库读数补两处事实：**aarch64 的 UEFI 腿今天已经同时使用两份内存来源**（固件内存图 + 从镜像盘读入的设备树 `/memory` 出界闸），所以「UEFI 腿要不要读设备树保留区」不是理论问题；`/reserved-memory` 目前在任何腿上都没有解析。
4. 属性位「未知」的测试哨兵取值与主流实现框架里真实的后续版本属性位重合，本方登记为下一批代码修正项。
5. 三架构合同补齐：x86 的设备页不可缓存属性**同批**落地，并新增各逻辑处理器 MTRR 一致性验收；AArch64 的验收改成「两棵树分别遍历 + 各处理器属性寄存器读回」；RISC-V 的清单收窄到实际验证过的环境，并澄清「普通主存语义」不等于「平台必须有物理缓存」。
6. 路径 A 的事务描述改为规范准确的表述：退出调用只接收版本戳、只返回状态码，所谓「最终图」是与成功版本戳配对的那次读图快照。

---

## 二、逐轮结论回填与签字状态（原正文第十一节）

### 11. 二十轮外部评审结论回填

#### 11.1 第一轮与第二轮的结论

| 条目 | 第一轮 | 第二轮 | 当前状态 |
|---|---|---|---|
| 裁决一 · 合同形态 | 丙 | 维持丙 | 方案可签 |
| 裁决一 · x86 | `PAT` 自建 + MTRR 只读 + 设备页显式 UC | 同批落地，不许分批 | 已并入；待硬件读数 |
| 裁决一 · x86 多核 | （未提） | **新增**各处理器 MTRR 一致性 | 已加入条款 ⑥ |
| 裁决一 · AArch64 | 静态审计 + 运行时遍历 | **两棵树分别遍历** + 各核 `MAIR` 读回 + 确认启用的根 | 已并入；待读数 |
| 裁决一 · RISC-V | 平台合同 + 平台清单 | 清单只登记实际验证过的环境；语义不隐含「必须有物理缓存」 | 已并入；待清单 |
| 裁决二 · 行 1 | 三类分级 | 「先修 ISA 掩码」为阻断项 | **分级原则可签；掩码断言经本方核实不成立**（§7.1，四处取值一致），阻断项改为「配置档」，第四轮再改成固件组合白名单五项（§7.1） |
| 裁决二 · 行 2 | 甲 | 维持 | 已签 |
| 裁决二 · 行 3 | 丙的加强版 | 继续与裁决六绑定；覆盖不能仅凭「引导根可达」成立 | 待裁决六 |
| 裁决二 · 行 4 | 维持本方 | 维持 | 已签 |
| 裁决二 · 行 5 | 拆两条生命周期政策 | **加保留区例外**（静态保留区可能以引导服务数据类型出现） | 政策已签；例外见 §2.2 行 5 + §4.2、§4.4 |
| 裁决二 · 行 6 | 不预建注册机制 | 不必再讨论 | 已签（本方补验收强度） |
| 裁决二 · 行 7 | 有界样本 + 截断标志 | 样本容量常量语义要改名或限定；配额按处置独立 | 已签 + 两条补充 |
| 裁决二 · 无空洞验收 | 七条判据 + 具名用例 | 维持 | 已签（本方补「逐位测试分保留位与已定义位」） |
| 裁决三 · 路径分工 | 夹具腿用 A、事务协议单独测 | 维持 | 已签 |
| 裁决三 · 事务表述 | （未纠） | **退出调用只返回状态码**；「最终图」=与成功键配对的读图快照 | 本文已改（§3.1） |
| 裁决三 · 输出生命周期 | 预分配存储 + 实际覆盖证明 | 构建期**与**运行期两道证明，不可互相替代 | 已改（§3.3 问题 A） |
| 裁决三 · 失败观测 | 结构化致命行 | 成功也出摘要行；摘要要回答四件事；错误码与政策原因**不要合成一个枚举** | 已改（§3.3 问题 B、§3.4 第 1 问） |
| 裁决三 · 阶段类型 | 类型化阶段对象 | **覆盖两条腿**，不只辅助/测试腿 | 已改；本方补「模拟固件装置也要产出阶段对象」 |
| 裁决三 · 容量 | 一次启动采全部读数；交接表超限失败、样本截断 | 维持 | 已签 |
| 裁决四 · 保留区 | 丙，重写重叠规则 | 静态 `reg`、`no-map`、先算集合再排除，均同意 | 已签方向 |
| 裁决四 · UEFI 边界 | 本方写「整节不适用 UEFI 腿」 | **纠正**：忽略 `/memory` 不等于忽略 `/reserved-memory`；保留映射表是第三输入 | 已改（§4.2、§4.3） |
| 裁决四 · `no-map` 用语 | 本方收窄为规范合同 | 规范是两句；「两棵树都不映射」应标为项目加强约束 | 已改（§4.6 两层分开） |
| 裁决四 · 动态节点 | 本方写「有 `size` 即动态」 | **纠正**：`reg` 优先、`size` 忽略；不支持的是无 `reg` 需选址的节点 | 已改（§4.3） |
| 裁决五 · 留痕 | 有界结构化诊断，含原因与四元组 | 方案可签；需定稿字段表与错误码表；配额按处置独立 | 方案已签，两张表待下一轮定稿 |
| 裁决六 · 页表树 | 本方新增，倾向甲 | **否甲**，推收窄版乙；C′ 为退步方案 | 本方接受（§6.3），并补 §6.4 一条边界 |

**签字分层（第二轮框架，本方采纳）**：政策原则多数可签；实现合同仍有几处要回写（禁止清单的三条页表合同、阶段对象补中间类型、设备树保留来源的再拆分、样本配额）；硬件与整机读数的证据不能由文档推理代替。

#### 11.2 第三轮的结论与当前分歧

| 条目 | 第三轮结论 | 本方处理 |
|---|---|---|
| 2.10 ISA 掩码 | **撤回上一轮的质疑**，确认本仓取值与规范一致 | 分歧关闭（§7.1） |
| 2.11 的属性位数值 | 提出正式 PDF 与主流实现头文件的冲突表，并修正本方「未能取得来源」的表述 | 采纳为「已核实的发布文本冲突」，但注明本方未能逐字核对；本方另核实 2.11 修订历史只记「新增热插拔属性」 |
| 版本门控 | 「2.11 的位按基线外处理」不安全，必须整体拒绝 | **接受并重写 §7.1**——同一位在两个配置档里含义相反，本方上一版的写法确实会让误读静默通过 |
| x86 PAT 索引位 | 本方「PCD/PWT 两位」的描述错误，应为 PAT/PCD/PWT 三位 | **接受**，§1.1 与术语表已改；这是本方自己的技术错误 |
| 裁决三 | 上一版四条类型约束与路径 B 矛盾，需「预备分类结果」 | **接受**，§3.3 已重写，并补本仓用运行时重置表达同一约束的读数 |
| 裁决四 | `/memory` 出界闸不得宣称符合「忽略」；三个保留来源要分开；规范小节号给出 | 出界闸结论**已撤回**；来源已拆三个；出处按「评审人核实、本方未逐字核对」分级 |
| 裁决六 | 支持收窄版乙，但补三条页表合同；L 层归属修正为「规范 L2+L4，项目加 L3」 | 全部采纳，见 §6.3、§0.3、§4.6 |
| 裁决六成员范围 | 本方「只放 `no-map`」的判断被支持（不整批放 UEFI 保留类型），但要补设备树保留映射块 | 采纳 |
| 结构性意见 | 六个范围集合不要用加枚举值的方式收纳 | 采纳并升格为合同条款（§6.7） |
| 流程 | 优先阻断项改完即冻结结构，不再做无边界润色 | 采纳（§8） |

**当时仍开放的一点（第五轮已关闭，此处照录以保留历史判断）**：出界闸三案（A/B/C）本方给出的路线是「先修输入来源、再定权限，第一版停在只做诊断」，评审人倾向直接删掉这道闸。这是偏好与风险的权衡，不是事实分歧，留给下一轮一并定。（第四轮重申此点仍未决；**第五轮由本方让步关闭，选 A**，见 §4.4.1 与 §12.3 第 4 条。）

#### 11.3 第四轮的结论与两次互相撤回

| 条目 | 第四轮结论 | 本方处理 |
|---|---|---|
| 2.10 有两份正式出版物（2022 原始版与 2024 Errata A），属性常量彼此不同 | 评审人核对两份 PDF 后提出；本方核实了「两份文档并列存在」这一层 | **本方撤回上一版「2.10 配置档取值已核实」**：当时取的是标注 2.10 的 HTML，无法判定属于哪一份出版物。评审人同时撤回上一轮对本方结论的确认 |
| `EFI_SYSTEM_TABLE.Hdr.Revision` 不能当选择器（2.10 Errata A 与 2.11 同为 `2.100`） | 评审人核实；本方核实了依赖库一侧的相符观察（最高常量是 `EFI_2_100`，且被字符串化成 `"2.10"`） | **接受并换机制**：改成固件组合白名单（身份 + 配置档标识 + 样本 + 材料版本），修订字段与厂商字段降级为佐证 |
| 本仓常量组合与任何单一出版物都不完全吻合 | 按矩阵读数（EDK2 的 `WP`/`MORE_RELIABLE` 同原始版、`ISA_MASK` 同勘误版） | 接受，并据此把 §2.5 的逐位表改为按配置档标识键控，不再标注笼统的「2.10 档」 |
| `WP` 语义 | 规范把它归在物理保护属性下、同时说明现代用法常作缓存属性；它是能力声明 | 采纳，§2.5 那行照双重归类改写 |
| `SP` 排除政策 | 规范给的是提示，不是绝对禁止映射；本项目的更严政策要标注为项目加强 | 采纳，§2.5 那行改写 |
| 页级归一化必须只有一份集合 | 向外取整后若可用表仍按原始字节切除，同一页会一边禁映射、一边可分配 | **接受为先决项**，§6.3 已写强制合同并给出反例形状 |
| 合并区间与携带来源互相冲突 | 拆成「交接用的不相交区间集合」与「引导期来源诊断」两个对象；若交接清单要带来源则必须是位图 | 接受，§6.3 已写；与 §6.7 的建模纪律一致 |
| §0.3 输出表没列第四种输出，且它不属于 UEFI 分类器 | 应画出「三输入 → 唯一归一化与集合规划阶段 → 跨边界清单」的结构 | 接受，§0.3 已补 |
| 预备阶段对象不会清零静态内存 | 本方上一版「不做也不会读到脏数据」的说法过头；要写独占、有效长度、每轮从零计数 | **接受并纠正措辞**，§3.3 补六条不变量，重置降级为防御性措施 |
| 两条路径的覆盖验收 | 应是两条独立用例（本方上一版的倾向被直接签署） | 记为已定 |
| 「固件常驻区」不是第三个保留来源 | 它是前缀阈值，建议改名以免被当作区间集合 | 接受，§4.3 改为「两个区间来源 + 一个前缀阈值」并命名 `ReservedPrefixEnd` |
| 保留映射块可以包含设备树本体 | 本仓两条腿都在引导期持有指向原始设备树存储的指针，一律禁访问会反咬自己 | 接受，§4.3 补「复制后使用或失败」的生命周期合同，并指出本仓已有的同形先例 |
| AArch64 共享性 / RISC-V 原子能力 | 属性索引与共享性是两个维度；平台识别不等于能力证据 | 接受，进 §1.3 条款与 §1.5 证据表（列为 Gate，不作结构阻断） |
| 40 字节长度包含补齐位 | 本方上一版写成「之外另有 4 字节」 | 接受，§0.2 已改 |
| 冻结判断 | 仍不能冻结完整实现合同；但不需要再来一轮六组泛讨论，先改三项先决项 | 采纳（§8、§13） |

**四轮累计的互相撤回记录**：第二轮评审人撤回「本仓掩码比规范宽」（本方核实的四处读数一致）；第三轮本方撤回「阶段类型只需约束一条腿」；第四轮**双方各自撤回一次关于 2.10 的结论**——本方撤回「已核实」，评审人撤回「对本方结论的确认」。这四件事的共同原因是同一个：拿一份不带出版物标识的材料当「规范」。因此 §7.1 现在要求任何一致性断言都必须带**出版物标识与日期**。

#### 11.4 第五轮的结论与本文档的自我收缩

第五轮收到两份意见（一份只挑缺口、一份通读全文），判据一致：架构方向不再重开，缺的是**数据合同本身的闭合**。本方逐条核实后全部采纳，没有一处需要不同意（§12.3）。与前四轮不同，本轮的产出主要是**本方删掉自己写过的东西**：

| 条目 | 第五轮结论 | 本方处理 |
|---|---|---|
| 属性档身份 | §2.5 那张表是 EDK2 布局 + 原始版 + 勘误版的**混合读数**，不能称「2.10 配置档」，要给独立且不可混淆的标识 | **接受**，命名为 `MEMATTR_PROFILE_UEFI_RAW_0_9`（以本仓钉死的依赖库布局为基础，刻意不宣称规范归属）。两位评审人独立给出同一判断 |
| 逐位表漏项 | 「可热插拔」位在本档布局里有定义（`0x100000`），逐位表却没交代它 | **接受**，§2.5 补一行：按**档外扩展位**出现即报错，并注明若改绑到含该位的档则要转为「已知但不支持」。本方核实依赖库确实没有这一位（§10 现量命令） |
| 白名单的键 | 厂商串 + 厂商版本不是不可混淆的构建身份；样本不能当选择器 | **接受**，§7.1 补架构 / 机器类型 / CPU 模型 / 固件构建标识 / 配置档标识等维度，并把匹配算法写成四路（唯一 → 用档；零 / 多 / 样本违约 → 拒绝启动）。样本降级为**验证证据** |
| 「版本」被当成一个概念 | 表头修订、描述符版本、配置档标识回答三个不同的问题；`DescriptorVersion=1` 推不出属性位布局相同 | **接受，并按读数分轻重**：本方核实本仓解析层**已经**校验描述符版本（不受支持即整图作废）与步长（不小于 40、须为 8 的倍数，实际步长即跨距），所以这条不是代码缺口；缺的是**记录**——摘要行与致命行都不带这三个字段，因此解析失败时无法区分「结构版本不支持」与「属性配置档未命中」（§10 锚点）。三字段已分别写进 §7.1 白名单表与 §5 诊断字段表 |
| 一个集合 vs 多个集合 | 第四轮的「归一化只能有一份集合」若照字面实现，会把**允许映射**的普通静态保留区一起禁掉 | **接受并改写**：一份坐标、多个语义集合，配 `AvailableRanges ∩ PmmExcludedRanges = ∅`、`BootMapDeny ⊆ PmmExcluded`、`ReservedMapped ∩ BootMapDeny = ∅`、`AvailableRanges ∩ ReservedMapped = ∅`、`StartupRequiredObjectRanges(阶段) ∩ BootMapDeny = ∅` 五条关系（§0.3、§6.3、§4.4.2）。这是本轮最有价值的一条——本方上一版的表述会让实现者写出一个静默过严的清单 |
| 取整规则的适用范围 | 不止带禁止语义的范围；**所有** PMM 排除范围都要按页取整，否则同页剩余字节仍会被分配 | **接受**，§6.3 扩写，映射禁止集合只作更严子集 |
| 区间数学 | 原则已定但不足以唯一实现，缺六条边界（半开区间、取整公式、相加溢出、地址空间末端、排序与合并、暂存区超限） | **接受**，§4.4.2 逐条写成可编码条款，并指向本仓同形先例 `os/kernel/src/memmap.rs:cut_memmap` |
| 第三个区间输入 | §0.3 数据流有三个输入，§4.3 只定义了两个 + 一个阈值；「启动信息里的保留映射表」在流程图里有、解析合同里没有 | **本方核实后删除**：本仓交接结构里的 `reserved_regions` 是**分类器的输出**（保留表本身，由引导壳 `build_memmaps()` 产出、内核解码），不是任何人交给规划阶段的输入（§10 现量命令与生产者—消费者链）。数据流改为「两个区间来源 + 一个前缀阈值」，并在 §7.2 登记「若将来出现本项目自己交出的保留区间**输入**，必须同批给出生产者、字段布局、生命周期、准入语义与消费者」 |
| §6.6 与 §6.3 冲突 | 刚把交接集合与来源诊断拆成两个对象，却又要求交接条目携带来源与原因 | **接受**，§6.6 改为纯区间 `{起点, 页数}`，来源只留在诊断侧；诊断可截断、交接禁止清单不可截断 |
| §2.1 与行 6 冲突 | 「类型不在 0–15 就分类错误」与「将来允许登记 OS loader 自定义类型」不能同时成立 | **接受**，第 1 步判据改成「既非标准类型、也非本配置档明确登记的项目扩展类型」；登记形状是编译期枚举变体 + 穷尽匹配，不是运行时注册表 |
| 普通主存语义与写回位混层 | §1.1 说统一的是语义，§2.1 第 5 步又把写回位放回统一准入条件 | **接受**，第 5 步拆成**来源准入**与**架构语义准入**两级（§2.1）。这是概念层错误，不是措辞 |
| 运行时服务类型 | 这两种类型只靠 `RUNTIME` 属性位保护；属性位缺失就可能经普通属性准入落入普通内存路径 | **接受**，§2.1 补显式政策：按类型永不进可用表，类型与属性位不一致即分类错误。本方核实当前实现第 5 步末尾是 `_ => Ok((Disposition::Side, len))`，即这条政策今天**隐式成立但未被命名**（§10） |
| `PERSISTENT` 的完整范围承诺 | 承诺与自己的诊断模型没有共同载体（样本有界、可截断、不跨界） | **本方删掉该承诺**，选「第一版不承诺将来可直接恢复」；§12.5 记为过度表述 |
| `reusable` / 多段 `reg` / `hotpluggable` | 三条第一版政策缺失：单独 `reusable` 怎么办、`reg` 多地址长度对要不要全遍历、`/memory` 的热插拔提示怎么办 | **接受**，§4.3 补三条。本方另核出一条**本仓现存缺陷**：设备树收集器确实遍历了全部地址/长度对，但超过 8 条时静默丢弃、不失败也不记诊断（§10） |
| 出界闸 A / B / C | 两位评审人都反对本方的 C 路线：既然这条腿不该让 `/memory` 参与决策，留着它就要长期维护一套跨来源一致性判断 | **本方让步，选 A**（UEFI 腿完全忽略 `/memory`）。§12.3 第 4 条那处分歧因此由本方关闭 |
| 三来源冲突判据 | 「一方要授权、一方禁止」不够机械化 | **接受**，§4.4.3 改成优先级表（禁止访问优先、保留优先于普通使用、启动必需对象相交则复制或拒绝、第一版无用途授权通道一律按无授权处理） |
| RISC-V 原子能力 | 从「平台有 A 扩展」推进到「按物理区间分级」，且 LR/SC 与原子读改写是两条独立能力；这一项是内核正确性前提，不是性能 | **接受并升格**：从验收细节改为**平台准入合同**（§1.3） |
| x86 PAT 与 MTRR | 不存在一个叫「跟随 MTRR」的 PAT 类型；实际是页表选 PAT 表项，再与该地址的 MTRR 类型按架构组合表算出有效类型 | **接受**，§1.1 改写；MTRR 分析器列为有独立输入输出与合成用例的纯计算件，须覆盖是否启用、默认类型、固定范围、可变范围、重叠优先级、地址宽度与掩码、与 PAT 组合后的有效类型（§1.3、§7.2） |
| 补齐位恒为零 | 规范没有这样规定；那 4 个字节是 ABI 对齐间隙，不是语义字段 | **接受**，§0.2 改写为「解析器不得依赖其内容，也不应把非零当格式错误」 |
| 第一次退出尝试之后 | 规范警告引导服务可能已部分关闭；此后不得依赖固件控制台、协议或文件系统，且重试不应再分配 | **接受**，§3.4 新增「受限重试阶段」一问（两阶段还是三阶段，粒度请评审人定）；本方核出现有重试循环内**确实没有**控制台写入，但该约束今天纯属巧合成立【**第九轮标注**：这条已被本仓一条文本级审计用例部分否证，见 §11.8 与 §12.5 第九轮②】 |
| 诊断以 UEFI 为中心 | 三类来源里两类没有 `Type` 与 `Attribute`；字段表要按来源拆层，计数要分「原始来源重复计数」与「归一化后唯一页数」两种口径 | **接受**，§5.2 拆四层字段组，§3.3 问题 D 的输出清单从「三类」改为覆盖规划阶段的四个集合 |
| 「真机」这个称呼 | 本项目没有物理开发板，把 QEMU 启动叫真机会让读者误以为已有实体硬件验证 | **接受**，§0.1 改成三档：宿主单元测试 / QEMU 整机启动 / 物理硬件验证。本文已清除该词的全部自有用法 |
| 架构分层与 trait | 这份文档没有定义 trait 边界，但**不建议**为体现语义抽象而先立一个大而全的架构 trait；接口应在政策与消费者明确后按真实行为差异确定 | **接受并采纳为流程约束**：本轮不引入 `MemoryArch` 一类的抽象。四位一体的职责切分（来源适配 / 通用政策 / 架构语义合同 / 架构页表实现）记入 §7.2 作为将来接口设计的方向 |
| 冻结判断 | 政策结构与核心概念可冻结；分类政策与区间规划合同补齐后冻结；完整实现合同仍不能冻结 | **采纳**，并据此把本文的定位从「继续润色」改为「定向修订 + 转代码与取证」（§8、§13） |

**第六轮新增的三条本仓现存缺陷**（评审人看不到代码，由本方按读数补入，均登记在 §10）：

1. 设备树收集器 `os/libs/minix-boot/src/memmap_source.rs` 在 `DTB_MEMMAP_MAX = 8` 用满后**静默丢弃**剩余区间——既不失败也不记诊断，多内存条的机器会少识别 RAM。
2. 运行时服务类型的「不进可用表」今天靠裁决最后一段类型表末尾的兜底分支隐式实现，一旦有人在兜底之前插入新的准入分支，该政策会静默失效。
3. 重试循环对「固件服务已不可依赖」这条约束没有任何类型或断言保证（与 §3.4 剩余一问同一处理）。

**五轮累计的互相撤回与让步**：除上表各条外，本方在第五轮主动撤回两处自己主张过的表述（`PERSISTENT` 的完整范围承诺、出界闸的 C 路线），并删除一句与自己决定冲突的旧表述（「被排除的区域不许访问」）。三位评审人本轮没有撤回任何结论。

#### 11.5 第六轮的结论、一处反驳与第一次改写冻结条目

第六轮只处理具体问题，不重开六组讨论。本方逐条核实：**七条采纳、一条部分采纳并反驳**，另有点名四处旧文未清已全部改掉。

| 条目 | 第六轮结论 | 本方处理 |
|---|---|---|
| 第 4 步与运行时类型交叉合同矛盾（`LOADER_DATA + RUNTIME` 一处判诊断成员、一处判分类错误） | 本方核对后**确认矛盾成立**，且修法与评审人一致：一致性校验单独成步、排在属性裁决之前 | **采纳，并因此改写冻-6**：裁决顺序由五步变六步。本文第一次动冻结条目，动的是步数与顺序，各步政策内容不变；改动理由与影响面写在 §2.1 开头，不藏在正文里 |
| 「类型与属性双向等价」 | 评审人自己先收紧：规范里属性位讲运行期映射要求、类型讲用途与生命周期，两者回答的问题不同，**不能从两个定义直接推出双向等价**；并要求把严格拒绝标为项目政策 | **采纳他的限定、反驳他上一段的判据形状**：第五版本方写的「普通类型带 `RUNTIME` 也判分类错误」按字面会误杀设备窗口类型带该位的形态。第 4 步改为**只查一个方向**，另加两条方向性用例防止改回双向。这是本轮本方唯一不同意的意见，且反对的是自己上一轮采纳过的写法 |
| 8 KiB 那条几何要求挂在属性位上 | 评审人给出规范转述；本方从官方文档站检索到同一条，但**未能逐字核对，且取到的是 2.9A 页面、与本文基线 2.10 是不同出版物** | 第六轮按「检索取到、待逐字核实」分级登记，并转成一条本仓待补判据。**第七轮整条撤销**：该要求对本项目三个目标架构是 4 KiB，8 KiB 属 Itanium 章节（§2.1 的冲突表与本表下一轮记录）。本行照录以显示本方当时的判断如何被自己后续核对推翻 |
| 启动必需对象集合没有输入来源 | 本轮最大新缺口：三行输入推不出内核入口、栈、两棵树的页、交接结构、分类器输出数组、原始设备树缓冲区、清单自己的存储 | **采纳，新增 §6.8 `StartupObjectInventory`**：五个字段、生产者（引导壳，分类之前组装）、按阶段的消费时机、编译期容量与超限拒绝。本方按读数补一条**支持该缺口的现场证据**：直接映射覆盖验收的注释记录过一次真实故障——内核镜像与栈既不在常规内存表、也不是启动模块、也不是自举页表树，只靠前三个来源时它们的别名从未被装过，走别名访问即缺页，注释留着具体故障地址与「每轮复现」的读数，并把「一条描述符同时跨越候选与无关页时尾部没有叶子」记为**已知残余缺口** |
| 五条集合关系不完备 | `ReservedMapped` 与诊断成员同样不许分配，等式没写；`PmmExcluded` 应是「不许分配」的完整集合 | **采纳，确认为真漏**。扩到七条，并把排除集合按定义写成完整集合、所有来源经同一规划过程汇入；同时保留「汇入同一排除集合不代表映射政策相同」这句（那是第五轮已经纠正过的区别）。两条可推出的冗余本方有意留下以便逐条断言，请评审人若要最小无关集时指明删哪几条 |
| 区间数学与既有实现未必一致 | 「向外取整 + 末端饱和」与「`start + len` 溢出即拒绝」不天然一致；建议先 `checked_add` 再取整；并要求核实 crate 依赖、别用复制函数解决 | **采纳，且本方核实结果比提问更具体**：那条 C 复刻函数**确实**用饱和加法（注释自述夹到顶端），而分类器**确实**已是两步 checked、三种溢出各有独立错误码与用例。合同改为两步走并指名以分类器为样板、饱和只用于末端钳制；那条 C 复刻函数不改。**crate 读数**：引导壳已经依赖内核库，「不能反向依赖」的前提在本仓不成立，但沿那条边取用仍是错方向；两消费方的交集是 `minix-boot`/`minix-types`，新数学落那里、不复制。**独占上界**：现有 `1 << 48`/`1 << 52` 都远小于 `u64::MAX`，`end == 上界` 可表示；64 位全地址空间平台将来必须改末元素语义或加宽中间量 |
| RISC-V 还缺排序与一致性维度 | 规范把内存排序也列为按物理区域的属性，一致主存要满足指定内存模型；**支持原子操作不等于多处理器内存一致** | **采纳**，清单加两列（是否一致主存、采用哪种排序模型），准入判据收紧为三条同时成立，并把「模拟器能启动」明确排除在补位手段之外。本方补一条现状读数：这一列今天是空的，属准入缺口而非待验证事实 |
| 白名单：运行时身份没闭合 | 部分维度只有宿主才知道（模拟器版本、机器与处理器参数、固件文件哈希），来宾读不到；配置档标识是选档结果，作输入即循环定义；缺字段应判不匹配、禁止未定义通配、外部证据不得冒充运行时身份 | **采纳，其中循环定义是本方写出的逻辑漏洞**（第五轮那张表确实把配置档标识排在维度列又把它当选择结果）。改为两层表：运行时可读的才参与匹配，标识由唯一命中返回；流水线证据单独落盘。**本方补一条现实约束**：本仓厂商与厂商版本零读取点，实际可读维度比表里更窄，故建议第一版只登记一条已核实组合，并就此请评审人裁定起步方式 |
| 四处旧文未清 | §4.4.1 仍留「先 C 后 B」与请裁定问题；§4.7 两问已由 §4.4.3 与「第一版无授权通道」取代；§8 第一张阻断表仍写三输入含已删的 phantom；§13 取证项仍要求改造出界闸的输入来源 | **四处全改**，并按评审人给的处置执行：删除或标注过时、指向现行条款。§13 那行整行重写为 A 案的拆除判据。**本方另按读数修正评审人给的判据一处**：那条闸门在本仓只有 1 个生产调用点（aarch64 的 UEFI 腿），两条裸机腿从来不叫它，所以落地判据是生产调用点由 1 变 0，不是「UEFI 腿消失、裸机腿保留」 |
| 两处待裁定粒度 | 重试阶段定为**三档**（可用 / 受限重试 / 已退出），并说明比规范更严的「首次尝试后不再分配、不用控制台」是项目为可验证性选的约束；运行时类型与属性不一致**可以严格拒绝，但必须标为项目政策**，除非拿到规范条款与实际固件证据 | **两条均按裁定关闭**，§3.4 与 §12.4 相应改状态。本方接受「两档也能做但不建议」的理由——把「首次尝试已失败、固件可能部分关闭」藏进仍叫「可用」的对象里，正是本文反复处理的那种「一个标签盖两件事」 |
| 冻结判断 | 政策结构与四项方向（属性档身份、主存语义分层、一份坐标多集合、出界闸 A）可签；运行时类型政策要修顺序与定性、启动对象来源未闭合、排除集合关系不全、区间溢出要与实现对照、RISC-V 缺排序合同、白名单运行时键要落地、旧文要清。**做完四项定向修订即支持冻结政策与实现合同的结构**，硬件读数与代码接线转下一阶段，不再为原则问题重写设计 | **第六轮采纳为本轮的收口条件**（§8、§13）。**第七轮撤销该收口条件**：本方当时认为剩余四项「都是取值问题而非结构问题」，评审人下一轮即指出其中「白名单运行时键」是结构问题（本方的算法会拒绝本项目自己的三条腿）、另暴露出三项结构性缺口（登记表下游未接全、活对象与可分配页没有集合约束、物理地址上界两份实现），因此「做完四项即冻结」不成立。本行照录，是为了把**本方错分类的代价**留在文档里：分类错了会让读者在错误的时间停止审阅 |

---

#### 11.6 第七轮的结论：一条规范事实纠错与三个结构缺口的暴露

第七轮的第一条不是设计意见，而是**本方写错的规范事实**。它的影响面比一个待实现细节大，因为它已经进入第六轮的裁决（本方当时向评审人提了「严格报错还是记录后继续」这道选择题）。本方核对后撤销判据本身。

| 条目 | 第七轮结论 | 本方处理 |
|---|---|---|
| 8 KiB 对齐要求不属于本项目三个目标架构（UEFI 2.10 与其 Errata A 都是 4 KiB；2.9A 里 x64/AArch64/RISC-V 各架构章节同为 4 KiB，8 KiB 出自 Itanium 章节） | 评审人核对官方文本后提出 | **撤销该判据**，不在两种严格度之间选择——被撤销的不是严格度而是一个不属于本项目的要求。本方自己的检索读数与另一份公开翻译在同一位置给出 8 KiB 与 4 KiB，冲突如实登记成表；本方承认这违反了本文 §7.1 的自定纪律（当时没核对那句话属于哪个架构分支），记入 §12.5。现有 4 KiB 检查与要求一致，无需改动 |
| 登记表只接了冲突检查一个消费者，没接映射规划与覆盖验收，因此**没有解决它自己要解决的那次缺页** | 评审人称本轮最有分量的一条 | **采纳**（§6.8）：按字段派生三种结果——分配排除、阶段访问要求、页表映射要求；第三种写成 `StartupMappingRequirements(树,阶段) ⊆ MappedRanges(树,阶段)`，并按两棵树与阶段两个轴分别验收。本方承认第六轮「已闭合」的标注过早，§8 的状态改为「结构已定、下游接线待代码核验」 |
| 没有任何一条集合约束能阻止「仍然存活的启动对象被分配器重新分配」 | 评审人给的错误形状：加载器数据进可用表、其中一页放着栈或页表页、内核起来后被重分配 | **采纳**，新增 R4（活对象汇入 PMM 排除集合）与 R7（与可用集不相交）。规范背景（加载器退出引导服务后仍占用加载代码/数据区间）标为**评审人核实、本方未逐字核对**。「阶段」同时加到排除集合与可用集合上；本仓没有释放点的定义，因此第一版一律保持排除、不自动归还 |
| 「七条里有两条冗余」数错了（七条里只有一条可推出） | 建议要么改文案、要么补上 `Available ∩ ReservedMapped = ∅` | **采纳**，补入该条使等式扩到十条、三条冗余，并把「编号在左、推出依据在右」定为写法：**数量由表自己决定，不由散文里的数字决定**。这条纠错本身值得留在文档里——它是本方第二次在同一处数错（第六轮也数错过一次） |
| 物理地址上界的语义、来源与越界处置未对三架构统一闭合；x86 的 52 位是格式上限不是每颗处理器的能力；RISC-V 没有取值 | 列为三个结构性缺口之一 | **采纳并修正其一条前提**（新增 §4.4.4）。本方核实：本仓不是没有这个概念，而是**有格式上限与能力上限两份实现，且内存图准入用的正是粗的那一份**——内核侧三架构各有一份「能力与格式取小」的访问器（x86 读 CPUID 扩展叶子、AArch64 读 `PARANGE` 且低于 48 位判不支持、RISC-V 取平台位数与编码位数之小），引导壳那个编译期常量的注释自己写着「将来由分页层同名访问器取代」。**RISC-V 是有值的**（Sv39 下 48），缺的是按平台登记的证据。另按其 4.1 给出越界处置的三档来源表，并规定不得存在未区分来源的默认裁剪 |
| 「第一版只登记一条已核实组合」不能弥补身份不充分；单条记录会让算法拒绝本项目自己的环境 | 给出方案 A（运行时身份闭合）/ B（限制支持环境并诚实声明），倾向保留 fail-closed、不开放泛化匹配 | **采纳，本方选 B 并撤销第六轮那条建议作为解决方案**（§7.1、§13）。两种情况都核对致命：必需字段读不到即零命中→拒绝自己的三条腿；声明不约束→唯一命中是虚假的唯一性；样本补不上，因为本方自己已规定样本不是选择器。并要求 §7.1 的标题在他确认 B 后按「选择器未启用」改写——本方不替他先改，避免同一处两种口径 |
| 受限阶段要不要读配置表 | 建议第一版直接禁止；并提醒「直接读系统表内存」不等于「调用引导服务」，规范没把只读访问一律禁止 | **采纳**（§3.4 关闭本方第六轮留的追问）。本方补两点：这条是**为可验证性选的项目加强**而非合规要求；并核出一条连带——aarch64 那条腿的设备树快照来自配置表查询、其原始缓冲区还被直接映射来源引用，故「首次尝试前完成复制」与 §4.3 的复制规则必须落在同一个动作上 |
| 别问「十一项是否完备」，改用可追溯性判据（跨内存图改变/页表切换/分配器启用边界的对象要能追溯范围、最后使用阶段、每棵树上的访问方式） | 并点名引导壳自身映像与执行栈未列 | **采纳**（§6.8）。本方按这条判据自查出**两类**漏项并补入：引导壳映像与栈（那段不用堆、把诊断写进栈上定长缓冲的中止处理路径正跨边界）、原始内存图缓冲与规划工作区（核对结论：与分类器输出存储**是两块不同存储**，所以要单列） |
| 四处旧文未清（篇头「五轮」、8 KiB 散在六处、§8 把登记表与排除集合标为已闭合） | — | **全部改**：篇头改七轮并把 8 KiB 从各处按「已撤销」统一处理；§8 的「先决已闭合」改为「结构已定、接线待核验」，并把身份键从取值类升为先决 |
| 冻结判断：**暂不建议冻结**；架构方向无需重开、不必重讨论六组政策；下一轮围绕三项结构缺口补合同并做一次完整交叉审计 | — | **采纳**。本方同时撤销第六轮写的「四项先决做完即可冻结」与「遗留全是取值问题」——后者是本方把结构问题错分类。本方对「什么算闭合」的标准据此收紧：**写完字段与生产者不算闭合，所有下游消费者都要点名** |

#### 11.7 第八轮的结论：两份合同推到可证明，以及本方第四次同类错误

第八轮没有新原则，全部是把第七轮刚立的合同推到「代码与测试能证明」的程度。评审人判**仍不冻结**，本方同意；本轮**没有新增不同意项**，八项全部核对成立并采纳，其中一处本方修正了他的前提、一处修正了自己的推理。

| 条目 | 第八轮结论 | 本方处理 |
|---|---|---|
| `PmmExcludedRanges` 声称是完整集合但没有完整生成来源：类型强制排除的两类去向根本不产生区间 | 列为先决项；「R1–10 全通过也证明不了覆盖」；不能用可截断的诊断样本解决，也不能靠「没进可用表」反推 | **采纳**（§0.3）。补统计范围（取他建议的第二种：进入本项目内存政策的范围，不是全部地址空间）、补构造式（含 `TypeExcludedRanges`，且按他的说法它**不必**成为另一份跨边界协议）、把验收拆成关系测试与**来源完整性测试**两组。§5.1 那条现状从政策结论改登记为待修正的实现事实；本方按读数补三处空分支的现量命令 |
| 映射子集合同过弱：只证明「某处有映射」，不证明映射正确 | 他给的例子（映射到别的虚拟地址、权限可写不可执行，物理覆盖检查通过而合同未兑现）；要求区分四维 | **采纳并承认这是本文自己的老毛病换了位置**：从第三轮起一直说「不要用一位压两件事」，这里用「有映射」压了四件事。合同按物理覆盖 / 虚拟路径 / 权限 / 属性与共享性四维重写；并说明后两维不是新要求，而是**第一次把裁决一的承诺接到具体对象** |
| 映射要求单一四选一字段的形状 | 要求按 `(树, 阶段)` 索引；并明确「这是把对象真正需要的访问语义记全，不是引入大而全的架构抽象」 | **采纳**。本方核 §6.8 自己那张表就写着两棵树要求不同、却给了一个全局字段——本文第六次把同时为真的多件事写成一个对象 |
| 登记表该不该做第五份映射候选来源 | 他直接裁定：**不要**。登记表首先是**映射义务的权威来源**，不是把所有登记对象都映射出去的区间表；不满足时补带完整映射语义的对象候选或拒绝启动 | **采纳并撤销本方第七轮的倾向**。本方按他的理由重审发现自己的倾向确实错：若登记表直接当候选来源，引导壳自身映像会因为被登记而自动成为最终直接映射候选，而它只需要活到交接跳转前 |
| 三条生命周期 | 复制后的设备树缓冲区要独立登记；登记表自己的存储要登记（除非有包含关系证明）；中止路径可达阶段要用控制流证明 | **全部采纳**（§6.8）。第二条本方推广为一条自检规则：任何要求「必须能访问」的清单，其自身存储都要进清单（本文对禁止清单已用过一次，登记表是第二个）。**第三条本方核到控制流读数并修正自己第七轮的推理**：进入内核的函数返回类型是不返回型、页表切换发生在该函数内部、切换后同段仍有可中止调用——故该路径跨的是**引导根启用**而非切到最终根，不要求最终根映射。本方第七轮「注释写明驻留所以跨边界」是**从注释推行为**，同类错误第四次 |
| 栈的物理范围 | 不能由加载器数据类型与内存图快照反推（一条描述符可能覆盖多个对象；虚拟栈顶推不出物理底部）；应从分配点或已有映射取，两者都不行才考虑扩展交接结构 | **采纳并撤销本方第七轮写法**。本方核交接结构确实只有虚拟栈顶、无物理栈底字段，他的推论成立。结论相同（不改布局）但**理由完全不同**：第七轮是「省事」，本轮是「布局里根本没有可推的字段」 |
| 方案 B 的具体形状 | 给出七行分工表；核心是「每个被支持的构建产物预先绑定一个属性档，流水线证明它是在对应受控固件环境中启动的」；**并划边界**：B 只保证流水线之内，不得写成「运行时 fail-closed 已保证」 | **采纳并本轮关闭**（他说这项不必再问）。§7.1 标题、身份表、§8 依赖图第 0 步、§13 两条取证项同步改；「登记项上限为 1」随运行时选择器作废。本方另按他的提醒把「档有标识 / 档怎么绑产物 / 环境如何证明用了对应固件」拆成三件事写 |
| 来源收集缓冲超限 | 影响准入、排除与禁止的缓冲**容量不足一律拒绝启动**；诊断样本可截断而来源缓冲不可，因为后者决定政策结果；并把父节点单元数与 `ranges` 为空标注为项目支持子集（规范措辞是建议级） | **采纳**，收紧本方此前「失败或至少打诊断」的措辞；本仓静默丢弃登记为现存缺陷。子集标注已补，并加一句防错：不标注会让将来有人把它当兼容性缺陷去「修规范」 |
| `CR0.CD / CR0.NW` 仍未进入合同 | 指出这是前几轮就该有而漏掉的一条，且「实现成本相对低、最容易被遗漏」 | **采纳**，作为 x86 条款⑦。本方按读数补一条更具体的事实：全仓 5 处写 `CR0` 全是读改写自己关心的位（PE/PG/TS/MP/NE/EM/WP），**无一处读或清 CD/NW**，即缓存禁用状态被原样透传；并核到同形先例（分页处注释明写固件不保证 WP 且显式置位），说明本仓已承认「某些固件位不可依赖」，只是没覆盖到缓存控制位 |
| 十条关系的量词不统一 | 未量词化的 R2 与带阶段的 R5 推不出 R6；要求显式 `∀p` 并声明哪些集合阶段无关；阶段边界最后一次使用的归属必须统一定义 | **采纳**。本方核到自己上一轮刚说「冗余关系要一一对应用例」，而一条形式上推不出的冗余关系，它的用例断言的其实是另一件事——这句话比补量词更让本方难受。归属约定本方给出「归前一阶段」的取舍并声明可由他改 |
| 旧文复活 | §12.3 第 5 条里那句 8 KiB **要删掉而不是加旁注**；§11 标题轮次、§12.4 重复行、§13 两处早期「做完即冻结」口径 | **四处全改**。他对删除而非旁注的要求本方认为正确：留着会让读者以为本方的单向立场依赖一条已撤销的规范事实，而它不依赖 |
| §0.2 把属性整体定义成「能力位图」 | 属性位是异质的：能力 / 内存性质 / 操作系统必须遵守的映射要求 / 政策提示，四类不同；整体定义成能力会让读者第二次把它们压成一个抽象 | **采纳**（本方核对：与本文自己对运行期位的用法直接矛盾）。改写后 §2.1 第 4 步与 §2.5 逐位表才第一次有了准确的概念基础 |

#### 11.8 第九轮的结论：抽象之间的边界，以及本方第一次「评审人要求的合同代码里已经有了」

第九轮的十五条意见本方**逐条核对后全部接受，本轮没有新增不同意项**；但本轮有一处与前面八轮的性质不同：本方在这一轮**撤销了自己上一轮刚写下的一条实现判断**（详见本节最后一行与 §12.5 第九轮），也就是说本轮既有一次「评审人对、本方错」，也有一次「评审人的要求本仓早已兑现、错在本文没写」。本方本轮核出的两处新代码事实（两份设备树、路径 A 封装丢弃退出状态）都不在评审人给出的形状里，是他给的方向、本方按调用链走出来的。

| 条目 | 第九轮结论 | 本方处理 |
|---|---|---|
| 统计范围还没被证明是一个完整分区：构造式只保证不相交，不保证盖住候选 | 他给的形状是「一个有效区间既没进可用表又漏进所有排除集合，十条等式仍可全过」；并要求按启动方式分别构造、先与统计范围求交、去掉兜底桶、域外对象报错而非并入排除集 | **采纳**（§0.3）。补 R5b 不相交并集（R5 仍单列，便于测试报告区分「重叠」与「漏收」）；两条腿各给构造式；`Other` 兜底桶从合同里删除——他这条理由本方认为最值钱：留着它，将来任何漏类别都能塞进去，来源完整性审计就白做 |
| 一个集合同时表达「仍需访问」与「仍需保留」 | 两者会在对象最后一次使用之后分叉，他举的正是本仓现场（原始设备树缓冲区复制完即不再需要映射，但第一版不还页） | **采纳并拆两个派生集合**（§0.3、§6.8）：`StartupObjectPmmReservedRanges(阶段)` 归 R4/R7，`StartupMappingRequirements(树,阶段)` 归 R10/R13；并写明「最后一次使用归前一阶段」只撤销后者 |
| L2 层没有完整的禁止集合，最终直接映射靠四份候选「恰好没包含」 | 建议 `FinalMapDenyRanges` 与 `BootMapDenyRanges ⊆ FinalMapDenyRanges`；另指 R1 只证明保留区「不许分配」、没证明它真的进了最终根 | **采纳**（R11、R12）。本方补一条本仓自己的现场：本文 §10 那条来源说明里就写着「一条保留描述符跨越候选与无关页时尾部没有叶子」的未竟项——**「恰好没包含」失效的形状本仓已经记录过一次**，所以这条不是假想风险 |
| 设备窗口的两个 UEFI 类型不该合并成一条去向 | `EfiMemoryMappedIO` 是固件为运行期服务请求映射的 MMIO；`EfiMemoryMappedIOPortSpace` 的规范定义挂在 Itanium 平台架构上；三个概念（PMM 政策域 / 固件运行期 MMIO 描述 / 架构与设备树给出的实际设备映射窗口）不因都与设备有关而同一 | **采纳**（§2.1 第 3 步）。本方另记一条自检：这是**一周内第二次被 Itanium 专属条款咬到**（前一次是 8 KiB），因此规范断言的标注要求从「出版物标识 + 日期」扩到第三维「通用条款还是某架构专属条款」，无法判定时按「不适用本项目」处理 |
| 权限用「最小 + 允许上界」留了开放解释口 | 裁定改为默认精确 R/W/X 组合，需要自由度时显式列举完整组合；另要求页级权限冲突检查、虚拟路径给出可比较的具体关系、别名审计连权限一起看 | **采纳**（§6.8）。本方特别接受那条「引导根一律宽权限不等于最终根的默认要求」；页级检查本方按他的例子写成具名用例（同页 `RX` 与 `RW` 分别通过对象级检查仍不成立） |
| 阶段边界不得按「切换辅助函数是否返回」判定 | 保留四档、不再讨论档数；边界按体系结构状态真正切换的时点定义，切换前后都访问的对象要在两侧分别登记义务 | **采纳并撤销本方第八轮刚定的归属规则**。本方上一轮才用控制流证明中止路径跨过了引导根启用，这一轮才意识到**同一件事否证了自己写的「归前一阶段」**——若按函数返回判归属，就会把已在新根下的访问记到旧阶段。本方第八轮总结的纪律是「新抽象要点名每个下游」，本轮漏的是「新证据要回读它否证了哪条旧规则」 |
| `DevicePath` 可以不进 PMM 排除集合，但域外去向没闭合 | 「有地址输入、却返回长度零且没有其它完整去向」这种静默丢失必须禁止；返回值形状是实现细节，禁止要发生在合同层次 | **采纳**（§0.3 末尾与待裁表第 1、2 行）。本方按读数确认这条在本仓是**已存在的事实**：两条去向都返回零长度且三处消费点全为空分支，与上一轮那条「排除集合缺来源」是同一个缺口的两面 |
| 登记表怎样跨交接边界 | 他倾向**只传内核真正消费的、稳定且无指针的派生合同**，不传整表；但不能出现引导壳一份义务表、内核又从四份来源猜一遍的双重权威 | **采纳**（§6.8 新增一节）：固定布局的派生记录进交接结构，登记表本体留在引导壳；并写清派生数据的完成时点、覆盖风险与容量失败形状。他这一条把本文第六轮「不改固定布局」的结论接上了「那内核从哪拿到义务」这个此前没人回答的问题 |
| 方案 B 的运行时选档残留 | 「不是 B 的问题，是文档没完成 A→B 迁移」；四步单向流程；引导壳保留一条独立拒绝，但对象必须表述为「描述符违反构建期绑定档」 | **采纳**（§7.1 整段撤下匹配算法）。他对「不能把后者包装成前者」的强调本方单独抄进正文：流水线之外引导壳无法证明当前固件身份，只能证明当前描述符符合选定的档 |
| `ExitBootServices` 要按状态码分支 | 成功 / 已经过期的版本戳导致的参数错误可重试 / 其它直接终止；不需要第四阶段 | **文档部分采纳，实现判断本方写错并本轮撤销**：这三路分类在本仓配对循环**已经实现**且已写进该文件的硬合同（重试上限为 2），本方原本把它登记成实现缺口，读代码后撤回。本方另按同一条调用链核出**真实缺陷在另一条腿**：路径 A 那个封装把退出调用的返回值整体丢弃，失败与成功对调用方不可区分，它被 `prepare_boot` 与 22 个测试内核入口调用——这才是现存缺陷第五条 |
| aarch64 的 UEFI 腿上有两份设备树，本文当一份写 | 他不建议靠文字润色解决，要求用调用链确认 | **照做并核出比他怀疑的更糟的结论**：磁盘那份只解析 `/memory`（A 案删的正是它的内存用途），配置表那份引导壳**只登记地址、从不解析**——所以设备树的保留语义在这条腿上**今天根本没有来源**，§4.2–§4.6 那套政策对它是悬空的（新增 §4.4.1b） |
| §6.5 的 C′ 与 §6.2 / §6.3 的乙案同时存在两种现行口径 | 要么把 C′ 标成明确的临时降级模式并列出不兑现的合同，要么删掉 §6.5 | **采纳并选后者**：C′ 不是当前政策、也不是临时兼容模式，本节只说明它被什么取代。「哪些 `no-map` 相交时拒绝、哪些跳过映射」不允许由实现者自行决定 |
| 架构与平台侧四项 | RISC-V 清单代码化并唯一命中；原子登记到操作类别 / 宽度 / 对齐 / 排序强度 / 证据；`effective_pa_limit` 记号并纠正「Sv39 只有 48」；x86 普通页与大页分别验收并做两层；`CR0` 检查失败的处置写明 | **全部采纳**。本方核出「Sv39 只有 48 位」确是本方写错：分页层两个常量分别是平台 48 与格式 56，取小后得 48 是平台约束的结果 |
| 状态表比正文慢一轮（§0.5 / §7.2 / §8 / §12.4 / §13 / 文首） | 逐处点名，要求同步 | **全部同步**。本方承认其中三处是同一轮里改了正文忘了表——这正是本文第八轮总结的「把写了当成闭合」在文档自身状态上的复发 |
| 不要再扩大抽象数量、不要引入总括的 `MemoryPolicy` 或大而全的架构 trait | 「当前需要的是把已有集合、对象义务、候选来源和页表验收之间的关系接严」 | **采纳为收尾纪律**，并把它作为本轮的自检：第九轮只拆已有集合之间的边界，**没有新增任何顶层抽象** |

#### 11.9 第十轮的结论：政策方向收敛之后，数据模型自己成了阻断项

他这份意见标题写「第九轮」（审阅的却是本方第九轮回填后的正文），本方记为第十轮并在文首登记编号差异。本轮十五条意见与七个裁定的问题，本方**逐条核对后全部接受，本轮仍然零反驳**；但本轮有三处是本方上一版刚写下的东西被再拆一次，且新核出的代码事实比评审人给的形状更硬。

| 条目 | 第十轮结论 | 本方处理 |
|---|---|---|
| 统计范围在建立时就减掉了待统计的排除项，分区式会通过却不覆盖——**假闭合** | 他给的形状：`TypeExcludedRanges ∩ PmmPolicyUniverse` 可能是空集，于是「不重叠」与「盖住」都成立而被类型强制排除的范围从未被统计；建议先定义来源域再定义政策域 | **采纳并承认这是本轮最严重的一处**。改为 `SourceDomain` → `PmmPolicyUniverse` 两层，只减设备地址类；本方另把「先减前缀阈值」也改成域内成员 `ReservedPrefixRanges`（同一个错误的另一半）。**唯一分歧**：`DeviceAddressRanges` 必须留在域外，否则要进排除侧满足分区式，会否证第九轮已裁的「设备路径在政策域之外」；本方按理由保留并给它完整的不可截断存储 |
| 声明要单列 `DeviceTreeReservedRanges`，构造式里没有它 | 三层拆分：两个原始来源分别保留证据、归一化成政策集合、按消费分工进排除与禁止 | **采纳**。本方核对这是「宣布成员却忘了写进等式」，属第七轮那条纪律的又一次成立；两个原始来源不合并且要带来源标签 |
| R5b 缺阶段量词；`AvailableRanges` 的两种含义混用 | 补 `∀p`；定死为「政策上有资格」，分配器是否启用由阶段表单独表达 | **采纳**。本方核出混用的后果：S1、S2 分配器未启用会被迫把整个统计域解释成排除成员——状态问题被读成政策判断 |
| L2 禁止集合只有子集关系，等于没定义；且没有传给构造最终根的一方 | 五个问题逐项回答；建议加 `MappedRanges(最终根) ∩ FinalMapDenyRanges = ∅`，并指出 R11 不是它的替代品 | **采纳并撤回本方第九轮那句「不必新增交接协议」**。内核手里没有诊断成员与类型排除，无法重建 ⇒ `FinalMapDenyRanges` 改为独立跨界集合。给出五项具名成员构造式 + 点名哪些排除范围仍允许映射（`ReservedMappedRanges`、启动对象页**不在** L2 里）；新增 R15 验收关系、R14 与前提 R16 |
| R13 仍是一维子集关系，把正文的四维承诺缩回去 | 改为合同谓词 `MappingSatisfies` 并拆五个可独立失败的子判据；另指「不要求映射」的记录不该待在映射义务里 | **采纳，本方承认这是他本轮最尖锐的一条**：正文扩四维、形式化缩一维，而实现者照的是形式化。R13a–e 各配具名用例；上位概念改 `StartupAccessRequirements(树,阶段)`，派生出「必须有叶子」子集；「不要求映射」交给 R15 与负向检查，不用魔法值 |
| R10 留着一条实际不存在的例外通道，且最终根只查了 L3 子集 | 第一版一律相交即拒；未来新增安全访问路径要作为政策与 ABI 的明确变更；禁止集合按树选择 | **采纳（他的 A 项）**。本方另承认后半句是本方自己造的漏洞：既然拆了 L2 / L3，最终根必须查 L2，否则拆分等于收回一半。`MappingDeniedSet(树)` 落成 |
| §6.8 的对象目录没跟上正文承诺；串口设备窗口无处承载；S1 的树归属不明 | 三类来源目录；设备窗口二选一；S1 要么作外部前提要么作第三棵树 | **全部采纳**。目录按固定镜像 / 启动期分配 / 派生协议逐块列出；本方取「设备窗口不属于登记表、由独立设备映射义务承载」，理由是两类存储的容量与失败形状不同；S1 裁定取「外部平台前提」，代价是 S1 的访问需求不得声称已过 R13，另进平台假设清单 |
| 阶段边界要推进到「架构切换完成」；四档不增加 | 新根写入且地址转换同步与指令同步完成后才算切换；过渡窗口内双侧覆盖或证明不访问 | **采纳**。本方自加两条：两次切换各一条跨架构用例（构造同步序列内的访问并断言失败）；双侧覆盖只针对登记过义务的对象且禁止集合两侧都成立，否则退化成「两棵树映射一切」 |
| 类型 12 的定性与冻结规则冲突 | 它是「已知但本项目不支持的标准类型」，不是未知类型；二选一，他倾向拒绝启动 | **采纳并改判拒绝启动**。本方加一条取证门：前提「实际固件不输出类型 12」本仓无样本证据，样本回来后复核；若确实输出则改域外完整保留且必须带出处 |
| 类型 11 要有载体、容量与生命周期 | 不能只说「交给运行期 MMIO 描述」 | **采纳**。与 12 **故意不对称**：类型 11 在三架构常规出现，拒绝启动会杀掉自己的腿 ⇒ 域外完整保留，`DeviceAddressRanges` 给出来源、存储、消费者与超限即拒绝——这同时补上第九轮 §0.3 第五条欠的域外出口 |
| 完整诊断成员与可截断样本之间缺通道 | 写成合同：每条诊断成员范围必须完整贡献给排除结果与适用的最终禁止结果，不得靠样本恢复；承载完整集合的缓冲超限一律拒绝 | **采纳**。本方并据此区分「不跨界」与「不需要完整存储」——此前本文把两者混在「只是内部诊断」一句话里；§3.3 问题 D 的输出清单按真实存储重写，并禁止「只有数学定义、没有独立存储」的集合冒充缓冲区 |
| 设备树规范的 `should` / `shall` 与保留映射块类型条款的分级 | 项目支持子集要标注；把建议性条款升级为严格要求要标项目加强 | **采纳**。**本方在本条上抓出自己一处更严重的问题**：第九轮本方在档案与台账里写着「已标注」，而正文 §4.3 从来没有那半句——本轮补进正文并登记这条虚假完成记录（见 12.5 第十轮），此后「已改 X」必须带一条能在正文检索到 X 的关键词 |
| x86 `CR0.CD/NW` 的定性 | 是本项目 x86 第一版选的更严格运行状态前提，不是跨架构必要条件 | **采纳并加标签「项目准入加强」**，理由限定为「第一版只支持缓存控制正常启用状态下的 PAT/MTRR 合同，不接管继承来的关闭状态」；否则读者会推出本文从未说过的「x86 一关缓存就违反普通主存语义」 |
| RISC-V 原子清单要把操作落到真实指令路径 | `compare_exchange` 不是与 AMO 并列的基础能力：A 扩展的 LR/SC 序列、`AMOCAS` 属 Zacas、8/16 位涉 Zabha 且不蕴含 LR/SC 同宽 | **采纳并换掉这张表的主键**：实现路径 × 宽度 × 对齐 × 区域 PMA 等级 × 排序要求，取证挂在编译产物的指令清单。本方读数：内核与库层原子用法 122 处，`Zacas` / `Zabha` 在本仓文本里零命名 |
| 映射义务 ABI 要存 OS 语义而非架构编码值；字段形状五件待定 | 反对 `PAT 索引 = AttrIdx = PBMT = 0` 这种同字段不同语义；本方 §8 写「已改」略早 | **采纳并如实降级 §8**（本文第二次在状态列写太早）。ABI 改 `MemoryClass` / `Shareability`，三架构各自负责证明兑现；五个待定项逐条列出，「不要求映射」由派生出空集合表达。本方另核出这条正是本文第一轮定的「抽象语义而不是抽象机制」原则在 ABI 侧的镜像，而第九轮没按它检查过 |
| 七个开放问题的裁定 + 十一处旧口径 | 逐条给出裁定并点名 §7.1 是本轮最明显的状态表问题 | **全部落笔**。§7.1 那句「只有第一层参与匹配」与它下面表格自己的标题冲突，本方第九轮清了算法段却漏了引导语；§12.4 两行关闭、两问新提出 |

#### 11.10 第十一轮的结论：形式化关系自己成了新的语义压缩点

他这份标题写「第十轮回填版审阅意见」，本方记为第十一轮并在正文文首登记编号差异。本轮 29 条意见与七处他直接裁掉的问题，
本方**逐条核对后全部接受，连续第三轮零反驳**；但他这一轮的判断变了性质：不再指出「缺哪个抽象」，而是指出
**本方第十轮新补的形式化关系自己又把不同语义压回了一个集合关系**——这恰好是本文从第一轮起反对的那个毛病的最新形态。

| 条目 | 第十一轮结论 | 本方处理 |
|---|---|---|
| `ExplicitPolicyDeniedRanges` 漏在排除集合公式外，R16 缺阶段量词 | 二选一，他倾向加入公式；且**不得成为换名的 `OtherPolicyExcludedRanges`**，没有具体政策项就规定它为空 | **采纳**（他的第 1、2 项）。加进构造式 + E-1 / E-2 封闭登记表 + **第一版为空** + 每个元素必须可追溯到一条具名登记的完整性断言；R16 补 `∀p`。本方承认这是自己第十轮「宣布成员却漏写等式」的同型第二次 |
| 集合差会掩盖来源矛盾 | 减法之前先做来源自洽检查，否则 `DeviceAddressRanges` 变成能消除矛盾的减法桶 | **采纳**，新增关系 **R17**（RAM 类来源 ∩ 设备类来源 = ∅，非空拒绝启动）+ 四类来源对各自的处理表 |
| `ReservedPrefixEnd` 与 `ReservedPrefixRanges` 互相矛盾 | 标量输入与派生区间视图分开；并问它是否独立存储 | **采纳**：`ReservedPrefixEnd` 标量、`PrefixRangeView` 派生视图，**明确无独立存储**（不进容量清单），旧名除名 |
| 只有 R15 测最终根，引导根实际映射没有负向验收 | 拆两条，并明确 `MappedRanges` 不能混设备映射与专用路径 | **采纳**：R15a 测 `DirectMapRanges(引导根,p) ∩ L3`、R15b 测 `FinalDirectMapRanges ∩ L2` |
| **L2 禁的是进入最终直接映射，不是禁一切映射**（他列为本轮最危险处） | R10 应按树**与访问路径**选判据；若第一版就是不给专用路径，要明写并限缩 §2.2 行 3 | **采纳**：R10 改为 `MappingDeniedForPath(树, 路径)`；第一版**不提供可绕过 L2 的专用路径例外**，相交即拒绝启动；§2.2 行 3 那半句限缩为**未来政策**；将来加强为绝对禁止须按项目加强政策变更登记，不借用 L2 这个词 |
| 术语表两处仍是旧公式与旧名字 | 词条不能保留与正文相反的公式，否则读者面对两套定义 | **采纳并登记为自身错误**：`PmmPolicyUniverse` 词条第十轮只改了一半仍是第九轮公式、`归一化集合族` 还写着已拆掉的旧名——本方改正文时漏改词条 |
| 设备树保留声明在 UEFI 腿上要先证明被固件图覆盖 | 覆盖对照不得靠求交记账；范围覆盖是规范强制、类型是建议级（可项目加强） | **采纳**：拆两档写清强度归属；未覆盖即输入矛盾拒绝启动，**不得降格成「求交为空视为域外」** |
| 非 UEFI 腿的原始 DTB 不在对象目录里 | 每条启动方式都要有自己的权威输入与生命周期；DTB 落在保留映射块时读取要先于禁止生效 | **采纳**：`AuthoritativeDtbSource(启动方式)` 进目录；补「读取先于禁止生效或复制到可访问位置」这条跨腿约束 |
| 复制前置缺「源可读性」 | 五问：头部可读、checked 末端、整段可读、复制目标容量、各偏移限制在 totalsize 内 | **采纳**。他这句本方原样收下：**区间算术正确不等于源内存真的存在且可读**；五问答全部写成可测前置 |
| 「没有 DTB」「DTB 重复」「DTB 损坏」被合并 | 拆四态；损坏不得降格成「没有输入」 | **采纳**，四态表；并删掉本方上一版留在 §4.4.1b 的「是否解析配置表那份」备选（他已裁定，本方不能同时写合同已定与请裁合同是否存在） |
| 类型 11 与设备窗口又被并成一个域外集合 | 拆 `FirmwareRuntimeMmioRanges` 与 `DeviceWindowRanges`，消费者不同不得共用集合 | **采纳**。本方承认第十轮刚在 §2.1 分开 11 与 12，转头在域外集合里又缝上，是他说的「刚拆开的两种身份重新合并」 |
| 「样本一旦出现就自动改判类型 12」不成立 | 样本只证明固件输出，不证明本项目有消费者与合同 | **采纳并撤回本方那句自动改判**：真输出时先重新裁决该平台是否仍受支持，或另行设计完整处置 |
| 设备映射义务表只有一句名字 | 补齐字段、生产者消费者、是否跨界、容量与布局版本、参与页表验收、与两个来源集合的关系 | **采纳**（六个问题逐条落地）+ 新增关系 **R18** 让设备义务也过禁止冲突检查——本方第十轮那句「串口不属于登记表」若不同步补这条，最该拒绝启动的冲突反而无人检查 |
| §3.3 与 §5.3 两份容量清单不同步 | 合成一张唯一权威清单 | **采纳**：§5.3 那张表成为唯一清单，§3.3 问题 D 只引用；本轮把 `DeviceMappingRequirements`、`FirmwareRuntimeMmioRanges`、`DeviceWindowRanges`、两份禁止集合、义务 ABI 全部列入 |
| `TypeExcludedRanges` 与 `PmmExcludedRanges` 的载体未定 | 各自二选一并写清；本方纪律要能应用到自己新加的名字上 | **采纳**：前者**流式贡献、不独立存储**（完整性靠逐条输入断言），后者**不跨界**（内核消费的是它算出的可用表）。「不跨界 ≠ 不需要完整存储」与「出现在公式里 ≠ 必须有独立缓冲」两句一并写进 §5.3 |
| R13d 用架构字段语言、R13e「整体兼容」不可测 | R13d 改测有效 OS 内存语义，三架构各自证；R13e 给可判定的别名归并规则 | **采纳**。本方承认第十轮刚把 ABI 改成语义字段、转头验收谓词又假设各架构都有「叶子属性索引」；`MemoryClass = NormalWriteBack` 与本仓 §1.1「不要求平台有物理缓存」顶牛，故拆成主存类别 / 一致性与共享性 / 排序 / 原子能力四维，具体缓存策略只在架构证明层 |
| 引导服务能力状态与内存生命周期阶段是两维 | 分名，否则出现无法表达的组合；S4 名称与进入条件错位；表头「PMM 可用」易混政策资格 | **采纳**：`BootServicesState` 与 `MemoryLifecyclePhase` 两套；S4 改「最终根生效阶段」，服务器就绪降为独立事件；表头改「物理页分配器是否已启用」 |
| 「122 处原子用法」口径 | 那是**匹配行数**，不是调用数也不是指令数 | **采纳并登记为自身错误**：本方第十轮刚立「计数类断言不能只看计数」，本轮就在同一文档第二次犯；改为「122 行匹配」 |
| 计数与旧口径（第 19、21、22、23、24 项） | 输出总表仍是拆分前模型；「十三条 / 十六条」口径不一；§6.7「六个集合」已不完整；§2.2 行 1 仍写等选档；术语表旧名 | **全部采纳**：顶部输出表按当前模型重写；计数定死为「18 条主编号（含 R4b、R5b）+ R13a–R13e 五个验收面」；§6.7 改为六组分类；行 1 与术语表同步 |
| 下一轮范围 | 不做全篇泛读，只对公式、集合边界、跨界载体、实际页表结果四类逐条审计，并给了七项先决 | **接受**，本方把七项先决 全部转成正文条款与材料册取证项，本轮**没有新增顶层抽象** |

#### 11.11 第十二轮的结论：连接处闭合之后，首次给出冻结边界

他这份标题写「第十一轮重写稿」，本方记为第十二轮。29 条意见（含七项先决）**逐条核对后全部接受，连续第四轮零反驳**；
本轮他第一次给出冻结路线：**先修完 先决 再冻结政策结构，硬件证据与实现缺陷作为独立的实现门槛**。

| 条目 | 第十二轮结论 | 本方处理 |
|---|---|---|
| 统计域公式漏掉 `FirmwareRuntimeMmioRanges`（第一项先决） | 拆了两份集合却只减一份，类型 11 描述符两边都不落，**R5b 不成立**；建议两个都减 | **采纳**。公式两个都减；写死该集合的职责是「来源完整性记录 + 交叉核验件」而不是义务 |
| R17 太笼统 | 需要按来源身份的封闭兼容矩阵，并给 RamLike / DeviceLike 的精确定义与「具名可验证对应关系」的具体规则 | **采纳**：五条类别对 + 三个集合定义 + 五条对应规则（匹配形状 / 部分交叉不允许 / 只按页区间 / 重复取并 / 不匹配即拒绝） |
| 设备义务没进 R13、R18 有自由变量（第二项先决） | 新增 R13-Device；R18 补路径量化并写清 `DeniedFor`；`R15a` 应改名以免与 DirectMap 混淆 | **全部采纳**；并采纳他 11.1 的倾向：普通启动对象的 `DedicatedPath` 第一版为**未支持值**，设备窗口走独立路径语义 |
| 设备义务表自身存储没进对象目录（第三项先决） | 容量清单不等于生命周期合同；还要检查暂存区；并指登记表自身存储的递归问题 | **采纳**：目录补 ABI 存储行、暂存区包含关系检查；递归用「符号地址取自链接期静态范围」解决 |
| 非 UEFI 腿 DTB 内核消费未闭合（第四项先决） | 需要 `PostShimConsumer` 三值；可读性要拆「来源合法」与「当前可达」 | **采纳**，两条都落成正文条款；`OriginalSource` 分支要求「有安全副本或明确拒绝该布局」 |
| 无 DTB 的 UEFI 准入没条件（第五项先决） | 必须有平台证据证明「无额外 no-map / 保留映射块语义或已由其它来源承载」，否则拒绝；证据不能是「QEMU 能启」 | **采纳**，写进四态表第一行 |
| 关系计数与测试数不一致（第六项先决） | 不要再拿「十八条」当测试数量承诺，给出四种口径 | **采纳**：编号区间 / 21 行具名关系式 / R13 五个验收面 / 实际测试另计，§8 同步 |
| `NormalWriteBack` 与 `CoherentShareable` 仍把机制写成语义（第七节） | 应拆成地址空间类别、一致性、共享范围、排序、原子能力五个字段，可缓存性不作跨架构必填 | **采纳**；排序字段只表达「内核要求的最低语义」，架构模型名属证明材料 |
| RISC-V 缺 SupportedAccess（第八节） | PMA 还描述访问类型：取指、普通读写、硬件页表读写 | **采纳**，四项进准入；Coherence 与 MemoryOrdering 独立登记 |
| TypeExcluded 流式贡献只有一个下游（9.1） | 必须同时贡献给 L2 累积器，两个下游各自计数与预检 | **采纳**，并写进「两遍扫描」合同 |
| §3.3 容量清单没真正引用 §5.3（9.2） | 应改写成「按 §5.3 逐项统计」 | **采纳** |
| 旧口径八处（第十节） | 含 §0.5 两项已裁、§1.4「剩余两问」、§2.2 行 1 前后矛盾、§8 计数与 DTB 身份、§12.4、§3.3 | **全部清理**；其中 §2.2 行 1 被视为「最需要立刻改」的一处 |
| 11.1–11.4 四条边界 | DedicatedPath 身份、R13e 标项目政策、前缀派生视图归一化、E-1 登记表必须进正文 | **全部采纳**；E-1 的七个必填字段写进 §0.3 |
| 冻结意见 | 暂不冻结，但**先修 先决 再冻结政策结构**；硬件证据作独立实现门槛 | **采纳并据此新增冻结边界声明**，请他确认 |

---

## 三、本方对二十轮评审的回应（原正文第十二节的 12.1、12.2、12.3、12.5）

> 原第十二节的 12.4「本轮分歧的关闭状态」**留在正文**——那是当前待裁问题，不是历史。

#### 12.1 采纳（只改正文，无需再讨论）

收窄版乙替代甲；x86 设备页 UC 同批；MTRR 多核一致性；AArch64 两棵树遍历 + 各核读回；RISC-V 清单收窄与「不隐含物理缓存」；物理内存属性措辞；`reg` 优先于 `size`；保留映射块为第三输入（第四轮再拆成区间集合与前缀阈值两类）；UEFI 腿的 `/memory` 只忽略来源身份不忽略保留语义；路径 A 的事务表述；构建期与运行期两道证明；阶段类型覆盖两条腿；样本配额按处置独立；错误码表与政策原因表分开、只共用命名体系；`NumberOfPages` 非零写进正文。

**第三轮追加采纳**：x86 PAT 索引是三位（本方上一版写错）；`/memory` 出界闸不得宣称符合「忽略」；版本门控必须整体拒绝 2.11（本方上一版「按基线外位处理」不安全）；阶段类型补「预备分类结果」；设备树保留来源拆三个名字；`no-map` 的规范合同是 L2+L4、L3 属项目加强；禁止清单的三条页表合同（最小页粒度、冲突检查、第一遍硬预检）；逐位处置表与可测的 `ranges`/单元数判据；范围集合的建模纪律。

**第四轮追加采纳**：选档机制换成固件组合白名单（表头修订字段区分不了 2.10 原始版、2.10 Errata A 与 2.11）；逐位表改为按配置档标识键控；`WP` 的双重归类与 `SP` 的项目加强性质分开写；**页级归一化只能有一份集合**（向外取整后的同一区间集合要同时用于可用表切除、清单生成、对象冲突检查与覆盖验收）；交接区间集合与来源诊断拆成两个对象（合并不得丢来源，要带来源就得用位图）；§0.3 正式列出第四种输出并画出唯一生成阶段；预备阶段对象补独占访问、有效长度与每轮从零计数（类型不会替本项目清零静态内存）；「固件常驻区」改名 `ReservedPrefixEnd` 并归为阈值而非区间集合；保留映射块与设备树本体相交时按「复制后使用或失败」处理；AArch64 的共享性字段与 RISC-V 的原子能力证据进验收；40 字节长度包含补齐位。

**第五轮追加采纳**（两份意见，判据一致）：属性档改为有身份的实现兼容档并补齐档外扩展位的定性；白名单补身份维度与「唯一/零/多命中/样本违约」四路算法，并明确**样本是验证证据不是选择器**；把「一份集合」改为「一份坐标 + 多个语义集合」并写出五条集合等式，取整规则扩到全部 PMM 排除范围；「版本」拆成表头修订、描述符版本、配置档标识三个字段并各自入诊断；补运行时服务类型的显式政策与类型×属性一致性判据；删掉 `PERSISTENT` 的完整范围承诺与 §6.1 的「不许访问」表述；`reusable`、多段 `reg`、`/memory` 的 `hotpluggable` 三条第一版政策；区间数学写成可编码条款；第一次退出尝试后引入受限重试阶段；诊断字段按来源拆层并区分两种计数口径；把「真机」这个称呼换成三档证据名称；§4.4.1 的出界闸分歧按 A 关闭（本方撤回自己的 C 路线）。

**第六轮追加采纳**：裁决顺序由五步改六步（新增「类型与生命周期属性的一致性」一步，排在属性裁决之前）；补上 `StartupObjectInventory` 作为规划阶段的第四个输入并单列一节定义其字段、生产者与生命周期；集合等式由五条扩到七条并把 `PmmExcludedRanges` 定义为「不许分配」的完整集合；区间溢出改为两步走（先 `checked_add` 验原始长度、再取整与钳制），饱和只允许出现在末端钳制；RISC-V 清单补排序与一致性两个维度并把准入收紧为三条同时成立；白名单身份拆成「运行时可读（参与匹配）」与「流水线证据（不参与匹配）」两层、配置档标识改为匹配**输出**、补「缺字段即不匹配 / 禁止未定义通配 / 外部证据不得冒充运行时身份」三条规则；重试阶段按裁定定为三档；运行时类型与属性不一致、`UNACCEPTED`/`PAL_CODE` 整机拒绝、`HOT_PLUGGABLE`/`SP` 的排除三处统一标注为项目加强政策；§4.4.1、§4.7、§8、§13 四处旧决策文字清理完毕。

**第七轮追加采纳**：撤销本方第六轮写入的 8 KiB 对齐判据（评审人核对 UEFI 2.10 与其 Errata A 为 4 KiB，8 KiB 属 Itanium 架构分支，不适用本项目三个目标架构）——**撤销判据本身，不在两种严格度之间选择**；把启动对象登记表接上三个真实下游并新增页表映射合同 `StartupMappingRequirements(树,阶段) ⊆ MappedRanges(树,阶段)`；新增 R4「仍然存活的启动对象汇入 PMM 排除集合」与 R7，并把「阶段」同时加到排除集合与可用集合上，第一版一律保持排除、不自动归还（本仓没有释放点的定义）；集合等式由七条扩到十条并按「编号在左、推出依据在右」重写；补入「可用 ∩ 可映射保留区」使冗余计数成立；物理地址上界的语义改按「当前目标在当前分页配置下实际可表示并可访问的独占上界」定义，越界处置按来源分三档且禁止未区分来源的默认裁剪；白名单改为必须在方案 A（来宾身份闭合）与方案 B（限制支持环境并诚实声明）之间显式择一，本方选 B 并撤回第六轮「只登记一条」作为解决方案；受限阶段禁止再读配置表；登记表完备性改用可追溯性判据（跨三类边界的对象要能追溯范围、最后使用阶段、每棵树上的访问方式）；§8 里本方标注为「已闭合」的两条改为「结构已定、接线待核验」，并把身份键从取值类升为先决；同时撤销第六轮「四项先决做完即可冻结」与「遗留全是取值问题」两句判断。

**第八轮追加采纳**（本轮没有新增不同意项，八条核对全部成立）：补 `PmmPolicyUniverse` 统计范围与 `PmmExcludedRanges` 的构造式，并把 §5.1「排除与设备路径不产生区间」从政策结论改登记为待修正的实现事实；把映射合同从一维物理覆盖扩成四维（覆盖、虚拟路径、权限、属性与共享性），映射要求字段改为按 `(树, 阶段)` 索引；接受他对「登记表不做第五份候选来源、而是映射义务权威来源」的裁定并撤销本方第七轮的倾向；补三类生命周期（复制后的设备树缓冲区、登记表自身存储、中止路径的可达阶段）；栈的物理范围改为从分配点或已有映射取，禁止由描述符类型反推；来源收集缓冲超限一律拒绝启动（区别于可截断的诊断样本），并把设备树的两条硬性判据标注为项目支持子集；x86 增条款⑦要求逐核核对 `CR0.CD / CR0.NW`；R1–R10 补显式阶段量词并规定阶段边界最后一次使用的归属；方案 B 落成「构建期绑定 + 外部环境清单 + 样本符合性检查」并明确它不保证流水线之外；§0.2 的属性整体定义改写为异质标志的四类；删除 §12.3 第 5 条里复活的 8 KiB 句子（整句删除，不加旁注）；§11 标题轮次、§12.4 重复行、§13 两处早期冻结口径同步修正。

**第九轮追加采纳**（本轮十五条核对后全部接受，无新增不同意项）：统计范围按 UEFI / 非 UEFI 分别给构造式并补分区式 R5b，去掉 `Other` 兜底桶、每一项先与统计范围求交、登记对象越出统计范围时报错而不是并入排除集合掩盖；把「仍需访问」与「仍需保留」拆成两个派生集合并分别交给不同关系约束；补 L2 层的 `FinalMapDenyRanges` 与 R11、R12，取消「靠候选来源恰好没包含来满足 L2」这种没有覆盖合同的满足方式；`EfiMemoryMappedIO` 与 `EfiMemoryMappedIOPortSpace` 拆开，三个概念不因都与设备有关而合并；权限默认精确、页级权限冲突检查、虚拟路径给出可比较的具体关系、别名审计连权限一起看；阶段边界改按体系结构状态真正切换的时点定义并把四档落成转换表（档数保留）；映射义务以固定布局的派生记录跨交接边界、登记表本体不过界、禁止双重权威；`DevicePath` 不进 PMM 排除集合的前提与域外去向写成合同（禁止「有地址输入却返回零长度且无其它完整去向」）；方案 B 的运行时匹配算法整段撤下、改为四步单向流程，引导壳那条独立拒绝的对象表述为「描述符违反构建期绑定档」；退出调用按状态码三路分派写进 §3.4（本方核对后确认**实现早已如此，缺的是文档**）；C′ 从「下一批换乙案」的备选降为**不是当前政策**，正文只保留乙；`CR0` 检查失败 fail-closed 且不得顺手清位；x86 每种叶子尺寸分别做两层验收；RISC-V 平台清单落代码常量表、原子能力登记到操作类别 / 宽度 / 对齐 / 排序强度 / 证据；上界统一记号并纠正「Sv39 只有 48 位」；来源完整性与集合关系测试分两组；§0.5 / §7.2 / §8 / §12.4 / §13 / 文首六处状态标签同步。

**第十轮追加采纳**（十五条意见与七个裁定全部核对成立，本轮无新增不同意项）：统计范围改为`SourceDomain` → `PmmPolicyUniverse` 两层构造、只减设备地址类、前缀阈值改为域内具名成员，以消除他给的那种**假闭合**（分区式通过而被类型强制排除的范围从未进入统计域）；R5b 补阶段量词；`AvailableRanges` 定死为「政策上有资格」，分配器是否启用只由阶段表表达；把第九轮宣布却漏写进构造式的 `DeviceTreeReservedRanges` 补齐并保留两路原始来源与来源标签；给 `FinalMapDenyRanges` 完整具名成员构造式、点名哪些排除范围仍允许最终直接映射、新增 R15 验收关系与 R14 及其前提 R16，**并撤回「不必新增交接协议」**改为独立跨界集合；R13 改为 `MappingSatisfies` 并拆五个可独立失败的子判据，上位概念改 `StartupAccessRequirements`；R10 删掉第一版不存在的例外通道并按树选禁止集合；对象目录按三类补齐、设备窗口归独立映射义务、S1 裁定为外部平台前提并另立平台假设清单；阶段边界推进到「架构同步序列完成」、过渡窗口双侧覆盖或证明不访问；类型 12 改判拒绝启动，并附一条样本取证门、类型 11 走域外完整保留并给出载体与容量；完整诊断成员的贡献与可截断样本分离写成合同、「不跨界」不再等同「不需要完整存储」、§3.3 问题 D 按真实存储重写；设备树规范 `should` / `shall` 与保留映射块类型条款的项目加强标注；x86 `CR0.CD/NW` 标为项目准入加强；RISC-V 原子登记主键换成真实指令实现路径；映射义务 ABI 存 OS 内存语义而非架构编码值、§8 那格由「已改」降级为「分工已定、字段合同待定」；他裁掉的七个开放问题全部关闭，§7.1、§5.5、§4.7、§2.2 行 1、§12.4 等十一处旧口径同步清理。

**第十一轮追加采纳**（29 条意见与他七处裁定全部核对成立，连续第三轮无新增不同意项）：`ExplicitPolicyDeniedRanges` 补进排除集合公式并给出封闭登记表（第一版为空）、R16 补阶段量词、新增 R17 来源自洽先于域外减法与 R18 设备义务过禁止冲突检查；`ReservedPrefixEnd` 与 `PrefixRangeView` 分开命名并明确无独立存储；R10 由按树升级为按树**与访问路径**选判据，第一版不提供绕过 L2 的专用路径例外、§2.2 行 3 相应限缩为未来政策；实际负向验收拆成 R15a / R15b 并按映射种类分开被测集合；域外地址拆成 `FirmwareRuntimeMmioRanges` 与 `DeviceWindowRanges`、撤回本方「样本一旦输出即改判」那句自动规则；设备映射义务补齐六个问题并进入唯一存储清单；`TypeExcludedRanges` 定为流式贡献、`PmmExcludedRanges` 定为不跨界；设备树两路声明的覆盖对照拆成规范强制与项目加强两档、非 UEFI 腿补 `AuthoritativeDtbSource` 与「读取先于禁止生效」、复制前置补源可读性五问、退化规则拆成四态并删掉已裁事项的备选口径；R13d 改测有效 OS 内存语义、`MemoryClass` 拆成四维、R13e 改成按（物理页, 阶段或转换窗口）归并的可判定关系；引导服务能力与内存生命周期分成两个状态机、S4 改名、表头改写；顶部输出表、计数口径、§6.7 分组、§2.2 行 1、术语表与「122 行匹配」口径全部同步。

**第十二轮追加采纳**（29 条与七项先决 全部核对成立，连续第四轮无不同意项）：统计域两个域外集合都减；R17 封闭兼容矩阵与三个来源集合定义；R13-D 让设备义务共用五维判定；R18 补 `∀树∀p∀路径` 并写死 `DeniedFor` 的两条路径；`R15a` 改名 `BootIdentityMappedRanges`；`MappingPath` 四值 + `DedicatedPath` 第一版未支持；设备义务 ABI 存储进对象目录、暂存区包含关系与「符号取自链接期」的递归规避；`PostShimConsumer` 三值；DTB 可读性拆来源合法与当前可达；无 DTB 的准入条件；计数四种口径；`AddressSpaceClass` / `CoherenceContract` / `SharingScope` / `OrderingRequirement` / `AtomicOperationRequirements` 五字段；RISC-V `SupportedAccess` 四项；`TypeExcludedRanges` 两下游 + 两遍扫描；§3.3 引用 §5.3；E-1 七个必填字段进正文；冻结边界声明。

#### 12.2 本方补强（评审人看不到代码，本方按读数补进去的）

1. **§4.4**：aarch64 的 UEFI 腿**已经**同时使用固件内存图与设备树 `/memory`（后者仅作出界闸），且注释已记录「用设备树全景替换固件结果会把自占用交给分配器，待裁决」。第二轮的叠加规则因此有确定的插入位置，行 5 的保留区例外在这条腿上是可测试的，不是抽象告诫。
2. **§6.4**：启动映射禁止清单管不到**交接之前**那段由固件建立的页表（本仓注释记录了「引导期物理 RAM 由固件恒等映射」）。因此满足 L3 不等于满足 L4；这条局限必须写进合同，否则引入清单会被误读为满足了规范两句。
3. **§7.1 副产品**：分类器测「未知属性位」用的哨兵取值与主流实现框架里真实的「可热插拔」位取值相同，注释的自述已经不成立。这条只在读常量时才会发现。
4. **§2.2 行 1 / 行 3 / 行 4 / 行 5 与 §4**：所有「拒绝启动」判据统一成一条横切规则——与启动必需对象清单相交且无安全映射路径——复用同一份清单，避免五处各写一套判据。
5. **§0.3**：把「不建普通映射」拆成 L1–L4 四层词汇表。这不只是措辞问题：裁决六的三条承诺在两棵树上成立情况完全不同，没有这张表时评审与本方都在用一句话指三件事。

**第五轮追加补强**（本方读代码后补进评审意见的三条具体形状）：① 评审人要求「区间数学与本页取整同源」，本方核出本仓已有同形实现 `os/kernel/src/memmap.rs:cut_memmap`（向外取整 + 末端饱和 + 宿主用例），因此 §4.4.2 的条款不是从零设计。**第六轮修正 ①**：本方当时说「将既有函数升为唯一入口」是**过头的**——那条函数的末端用饱和加法，与本轮定的「先 `checked_add` 验长度、溢出即拒」不兼容；本方另核出分类器里已有正确的两步 checked 形状，样板因此换成后者（详见 §4.4.2）；② 评审人质疑运行时服务类型只靠属性位保护，本方核实第 5 步末尾的兜底分支确实让它们落到「诊断成员」，政策结果正确但**没有名字**，故 §2.1 那行的价值是显式化而非改变行为；③ 评审人建议重试阶段收紧对固件服务的依赖，本方核实当前重试循环内没有控制台写入，因此该约束今天成立纯属巧合——这条从「设计建议」改为「把巧合变成断言」。【**第九轮标注**：「纯属巧合」不成立——本仓有一条文本级 EBS 边界审计覆盖两条入口；「从巧合变成断言」的方向仍然对，但缺口应改写为「文本级而非类型级、且不覆盖重试轮内部」。】

**第六轮追加补强**（本方按读数补进去的，评审人无法取得）：① §6.8 的必要性不是理论推演——直接映射覆盖验收的注释记录过一次真实故障，内核镜像与栈不在那三个来源里、别名从未被装过、走别名访问即缺页，注释留着具体故障地址与「每轮复现」的读数，并把跨候选的尾部缺口记为**已知未闭合**；② §4.4.2 的两步走本仓**已经有一处正确样板**（分类器连续两步 checked、三种溢出各有独立错误码与用例），所以这条合同不需要新设计，只需指定谁作样板；③ §4.4.2 的 crate 归属问题有确定答案——引导壳**已经**依赖内核库，评审人担心的反向依赖在本仓不成立，但两消费方的交集是 `minix-boot`/`minix-types`，新数学应落那里；④ §4.4.2 的独占上界今天不触发（两个取值远小于 `u64::MAX`，且判据是「末端大于上界才越界」，等于上界合法）；⑤ §7.1 的运行时身份比表里列的窄（厂商与厂商版本零读取点），因此建议第一版只登记一条已核实组合；⑥ §13 那条闸门拆除的判据修正为「生产调用点 1 → 0」，因为两条裸机腿从来不叫那个函数；⑦ 顺手查出一处现存脆弱性：那条「常规内存不得与自举分配相交」的断言用裸加法算末端，今天被上游校验挡住所以不可达，但绕过分类器的新调用会拿到一个静默失效的守卫。

**第七轮追加补强**（本方按读数补进去的）：① **本仓的物理地址上界不是「缺一个值」，而是同一概念有两份实现且用错了那一份**——内核侧三个架构各有一份「能力与格式取小」的访问器（x86-64 读 CPUID 扩展叶子的物理地址位数并与 52 取小、AArch64 读 `PARANGE` 且低于 48 位时直接判为不支持、RISC-V 取平台位数与分页模式编码位数之小，Sv39 下为 48），而内存图准入用的是引导壳的编译期常量，那段注释自己写着「将来由分页层同名访问器取代」——所以本方**修正了评审人的一条前提**：RISC-V 有取值，缺的是把那个平台位数按平台登记的证据，不是数值本身；真正的缺口是「通过准入的区间可能在页表构造处不可表示」，而本仓现在只靠一条更晚的断言拦它；② 评审人的「登记表只接了一个消费者」在本仓有对应的代码落点——最终直接映射正是由四份来源取并集构造，那段来源说明里就写着那次缺页与一条「需要对已装候选做区间减法才能闭合」的未竟项，所以第三种派生结果的接入位置是确定的，不必新发明；③ 本方按他给的可追溯性判据自查，除他点名的引导壳映像与栈之外又找出一类：原始内存图缓冲区与规划工作区**与分类器输出存储是两块不同的存储**（现量命令见 §10），且引导壳那段中止处理路径刻意不用堆、把诊断写进栈上定长缓冲——它的可运行前提本身就是「这些页没被分配器拿走」；④ §6.7 那条「同一区间的多重身份要允许并存」在第七轮有了具体用途：一个页既可以既是活对象页又落在诊断成员里，R4 与 R2 因此必须能同时成立，不能用互斥枚举表达。

**第八轮追加补强**（本方按读数补进去的，多数反而把评审人的问题说得更精确）：① 评审人说「排除集合缺一个来源」，本方核到具体形状比这更硬——`adjudicate` 对固件保留与不可用类型**返回长度零**，因此这三处空分支（预检计数、写入分发、提交落盘）不是「忘了统计」而是**根本没有长度可统计**，改时要从返回值动起；② 他要的「按树与阶段索引的映射要求」在本仓有现成的落点：`dm_coverage.rs` 那四份来源与 `bootstrap_tree_candidates` 已经在做「候选来自哪里」，缺的正是义务与候选的对照，所以这不是新层而是补一次检查；③ 他用控制流问「中止路径真的跨切根吗」，本方核到的答案两边都不是原样对：切换发生在**被引导壳调用的那个不返回函数内部**，切换后同段还有一处会中止的取值，所以跨的是引导根启用、不跨最终根——本方第七轮的说法（跨切根）与「不跨」的直觉都不准；④ `CR0` 那条本方数出是 5 处写点、全部只改自己的位，另核到「固件不保证 WP」那条注释是同仓库已有的同形先例，这使补这条的成本论证从「应该做」变成「同处顺手做」；⑤ 设备树快照那一处本方还核到一个相关事实（不是缺陷）：那个函数返回的区间表指向的是本仓自有的两块静态数组、不是局部缓冲，所以局部缓冲析构不影响其有效性——但那两块静态数组自己就是跨阶段自有对象，同样要登记。本方按此再补一项而不是把它写成悬垂问题。

**第九轮追加补强**（本方按调用链与常量读数补进去的，五条里有两条反把评审人的问题改写了方向）：① 评审人要的「退出状态码分类」**本仓配对循环早已实现**——三路分派、重试上限是一个常量（读数 2）、其它状态码立即失败，并且该文件头注释把这三条明列为自身执行的硬合同；本文档落后代码五轮。本方由此把这条从「补实现」改成「引用现有用例并把文档写全」——因为连「锁死分类的用例」本仓也有两枚（一枚锁重试预算耗尽、一枚锁「非参数错误立即失败且不重读图」），并**撤销本方本轮写下的那条不成立的缺陷登记**（见 12.5 第九轮①）。② 顺同一条调用链，本方核出**真实**缺陷在另一条腿：路径 A 的退出封装把返回值整体丢弃，失败与成功对调用方不可区分，它被 `prepare_boot` 与 22 个测试内核入口调用——这才是现存缺陷第五条。③ 评审人怀疑两份设备树可能是真的，但本方核出的形状比他描述的更硬：磁盘那份**只解析** `/memory`（A 案删的就是它的内存用途），配置表那份引导壳**只登记地址、从不解析**；所以问题不是「哪一份可信」，而是「这条腿上设备树保留语义今天没有任何来源」，§4.2–§4.6 那套政策对它是悬空的（新增 §4.4.1b）。④ 本方回读本仓测试目录，发现自己第五轮写进 §10 的那条「代码事实」已经过期：本仓**有**一条文本级 EBS 边界审计（两条入口、剥注释、注入负控），所以那句「纯属巧合、没有任何断言」不成立，缺口应改写为「文本级而非类型级、且不覆盖重试轮内部」——一条本方自己标为「已核实」的断言在正文里活了四轮。⑤ 本方按本轮新写的「登记对象越出统计范围要显式处置」这条判据自查中止路径的依赖集，立刻撞上本仓一处真实矛盾：它要写的裸串口寄存器窗口根本不在统计范围里——**新合同在本轮抓到了一处旧假设的失效，也抓到了本文自己写过头的那一句**：本方第一版把处置写成「一律报错」，那样会把一个合法的设备地址判成启动失败；判据因此按来源分成两类（内存类越界即矛盾、拒绝启动；设备地址类只承担映射义务，由设备窗口政策校验授权来源），本方没有把设备窗口塞进统计范围来绕过它。⑥ 本方取这几条零命中读数时踩到一个取证陷阱：用 `grep -rn 'no-map' os --include=*.rs` 想证明「`no-map` 从未被实现」，读数却是 2——两条命中都是服务器层注释里 `errno-mapping` 的子串。**子串巧合会把零读成有**，而本文大量使用「本仓零读取点」这类断言；本方据此在 §4.4.1b 写下一条取证规则：这类断言要么用带边界的检索式，要么逐条看一眼命中行，不能只看计数。

**第十轮追加补强**（本方按调用链与常量读数补进去的，其中一条比评审人给的形状更硬）：① 他要确认「内核是否也改用复制后的设备树」，本方核出的答案是**这次复制在本仓从未存在**：引导壳没有任何与设备树相关的复制动作、登记的只是一个物理地址、内核 `parse_by_kind` 就按那个原始地址解引用解析——所以本文第七轮起那句「原始缓冲区最后使用点＝复制完成」描述的是一个不存在的动作，他担心的「禁止映射原始 DTB 而内核仍持有原始地址」是现在时的冲突，不是假设。② 他给类型 11 与 12 留出两种处置，本方按实际出现频率把两者**拆开走**：12 拒绝启动、11 域外完整保留，并给 `DeviceAddressRanges` 补齐来源、存储、消费者与超限行为——这同时补上了第九轮 §0.3 第五条欠的域外出口。③ 本方核出仓库注释里一条与本条相关但**尚未复现**的实测读数：AAVMF 的配置表「8 条目、ACPI2 在、两个设备树 GUID 都不在」；若成立，他的裁定「采用配置表 DTB」在本项目 aarch64 UEFI 测试环境上等于「没有设备树保留输入」。本方把这条连同取证项一起登记，不做选择性的沉默。④ 他要求把原子登记落到实现路径，本方顺手量出规模：内核与库层 `compare_exchange` 与 64 位原子类型用法 122 处，而 `Zacas` / `Zabha` 两个扩展名在本仓文本里零命中——准入表连输入都还没有。⑤ 本方另核到一处可作为设备映射义务落点的同形物 `identity_device_windows`（5 处引用），避免下一批从零发明。

**第十一轮追加补强**——本方按他给的方向自己核对，多数结论是「本方上一轮的写法在本仓连载体都没有」：① 他要确认的设备树两路原始来源集合、`AuthoritativeDtbSource`、本轮新加的公式成员与设备义务名字，**在仓库文本里全部零命中**（`grep -rn -e DtbMemoryReservationBlockRanges -e DtbReservedMemoryRegRanges -e AuthoritativeDtbSource os --include=*.rs` 与 `grep -rn -e ExplicitPolicyDeniedRanges -e PrefixRangeView -e DeviceMappingRequirements -e FirmwareRuntimeMmioRanges -e DeviceWindowRanges os --include=*.rs` 两条命令读数均为 0）——所以这些不是「改改名字就有」的东西，本轮全部按「本轮定合同、下一批实现」登记，并按本方第十轮那条规则逐条带现量命令。② 最近的同形物是 `identity_device_windows`（5 处引用），本方把它标为**待改造的来源件**而不是已完成的载体，避免读者以为设备义务已经落地。③ 他把「非 UEFI 腿的原始 DTB」列成遗漏项，本方核对时顺手确认这条腿的输入确实来自 `dtb_ptr()`（`os/boot-shim/src/opensbi_helpers.rs` 直接用固件交付的 DTB 物理地址），所以这条不是假想风险：非 UEFI 腿同样存在「禁止映射与仍要读取」的顺序问题。

**第十二轮追加补强**：① 他要求「具名、可验证的对应关系」不能只有一句解释，本方按本仓现状给出五条可执行规则，并明确**不比对设备名与驱动字符串**（那需要平台身份，第一版不做）——这条限制是照本仓能力写的，不是理论简化；② 他指出的递归问题（登记表自身存储要登记、而登记需要自身符号地址），本方核本仓既有形态后确定用**链接期静态符号范围**，与 `identity_device_windows` 那类来源件同构；③ 他问 `FirmwareRuntimeMmioRanges` 到底承担什么职责，本方回答「来源完整性记录 + 与设备窗口的交叉核验件」，并据此不给它任何映射义务；④ 冻结边界那段是新写的：把「政策结构」与「实现门槛」明确分开，并附本轮七项先决 的落点供他逐条确认。

#### 12.3 不同意（附证据，请评审人裁定）

1. **「ISA 掩码比规范宽」——关闭的方式本身又被修正**。第三轮评审人撤回了该断言（当时比对的四处读数确实一致）；第四轮进一步发现那四处里被当成「规范」的一份并没有出版物标识，而 2.10 实际有原始版与勘误版两份正式文档。所以这条分歧的最终结论不是谁对谁错，而是**任何「与规范一致」的断言都必须带出版物标识与日期**（§7.1）。本方同时撤回自己上一版「已核实」的说法。
2. **属性位数值的证据分级**。三份正式出版物的逐位数值来自**评审人核实**（本方对规范站点的抓取被拒绝，未能逐字复核）；EDK2 一行、本仓常量与依赖库的修订常量由**本方核实**。本文不推测哪一份写错，也不替规范做勘误——处理方式是把选档改成固件组合白名单，让「猜固件读的是哪份 PDF」这个动作从实现里消失。
3. **「先只约束辅助/测试腿」**：本方上一版的这一倾向被第二轮否决，本方**同意被否决**，但把「模拟固件测试装置也要产出阶段对象」列为接受的前提条件，否则类型约束在配对循环那条腿上只是纸面存在。
4. **出界闸三案的取舍**：评审人倾向 A（删掉），本方曾倾向「先把输入换成固件在同一次启动里交出的设备树，再停在 C（只诊断）」。第五轮两位评审人共同给出反驳，理由是本方无法反驳的：既然这条腿本就不该让 `/memory` 参与决策，那么留着它——哪怕只作诊断——也要长期维护一套跨来源一致性判断，而这不属于内存图政策的职责；当前输入还是从磁盘镜像读来的，与本机拓扑没有证明关系。**本方让步，选 A**，本文不再保留该分歧（§4.4.1）。拓扑一致性验证若将来确实需要，作为独立议题另立，不重新混入本政策。

**第五轮没有新增不同意的条目**：本轮两份意见的判据一致，本方逐条核实后全部采纳，其中三处由本方主动删除自己写过的表述（`PERSISTENT` 的完整范围承诺、§6.1 的「被排除的区域不许访问」、§0.3 数据流里那个不存在的第三个输入），一处由本方让步关闭（上表第 4 条）。

5. **「类型与运行时属性双向等价」——第六轮，本方反对的正是自己上一轮采纳过的写法**。评审人在第六轮一方面要求把严格拒绝标为项目政策、另一方面在建议表里仍写着「其它类型带 `RUNTIME` 不能作为普通内存继续进入准入流程」，而第五版本方写的是「普通类型却带 `RUNTIME` → 分类错误」。**本方不同意把任何带 `RUNTIME` 位的非运行时类型判成错误**，理由是本方核到的规范形状：属性位表达的是「该区域需要操作系统在建立运行期虚拟地址映射时给出映射」这一**映射要求**；而「退出引导服务后必须保留」那条要求挂在**类型**上。两句对象不同，推不出双向必要。支持这一立场的还有设备窗口类型本身：规范对它用途的描述就包含「固件请求操作系统把这段内存映射 IO 区映射进运行期地址，以便运行时服务访问」，另有本方核到的第三方实现注释同样写明该类型「可被运行时服务使用」——**因此「非运行时服务类型带该位」不但可能，而且是被预期会出现的形态**，把它判成分类错误会直接误杀合法固件。本方第七轮曾在这条里附带引用过一条 8 KiB 的几何要求，**第八轮按评审人要求整句删除而不是在旁边加注**：那条要求属 Itanium 分支，留着它会让读者以为本方的立场依赖一条已被撤销的规范事实——它不依赖。**本方采纳的部分**是一致性校验必须存在且单独成步，但只查「运行时服务类型缺 `RUNTIME` 位」这一个方向（§2.1 第 4 步），并新增两条方向性用例锁死方向。请评审人确认这个单向判断；若他坚持双向，需要先给出规范里要求其它类型**不得**带该位的条款，或给出实际固件从不这样输出的样本证据。

**第十轮没有新增不同意项**，但本方在他给的结构上保留了两处自己的判断并写明理由：`DeviceAddressRanges` 留在统计域之外（若留在域内就必须进排除侧才能满足分区式，而那会否证第九轮已裁的「设备路径属政策域之外」），以及类型 11 与 12 不对称处置（11 在三架构常规出现，拒绝启动会让本项目自己的腿起不来）。两处都不是反驳他的判据，而是把他的判据接到本项目的实际输入形态上。

**第十一轮没有新增不同意项**，本方保留的两处自己的判断都是「按他的判据接到本项目实际输入形态」而不是反驳：其一是 `PmmExcludedRanges` 他给的两个方案里本方取第二案（不跨界，内核消费它算出的可用表），理由是他自己那句「不能只写不可分配就算交接完成」——本方认为把它留在引导壳内、只把结果跨界，比在交接结构里新增一份完整集合字段更省契约；其二是设备地址集合本方按他拆成两份，但**第一版让 `FirmwareRuntimeMmioRanges` 没有任何消费者**（他不反对这一点，本方说明是为了不让「记录」被误读成「义务」）。

**第十二轮没有新增不同意项**，但本方对他两处建议保留了执行口径：其一，`PostShimConsumer` 的三值里，UEFI 腿第一版取 `CopyTarget`、两条非 UEFI 腿第一版取 `None`（引导壳读完即止，内核平台描述改由交接快照提供）——这与「每条腿都要有自己的生命周期记录」一致，但具体取值属实现选择，写进合同的是**必须三选一、不许留空**；其二，无 DTB 的准入证据形态，本方限定为「固件文档、平台规范或该受控环境的明确约定」，不把「QEMU 能启动」算证据。

**第九轮没有新增不同意项**，十五条全部核对成立。本方本轮唯一与评审人给出方向不同的地方，不是反驳他的判据，而是他的判据落到本仓后指向了另一个位置：他要求补的是实现，本方核出实现已在、缺的是文档，而真正的实现缺口在他没点名的另一条退出腿上。**这条差异本方按「评审人对、本方补现场」处理**，因为他的政策要求逐字成立，只是本仓的兑现位置与他设想的不同。

**同一轮里本方另外承认的两处自己写出的漏洞**（不属于不同意，属于自查）：§7.1 把配置档标识同时列进匹配维度又当成选择结果，是**循环定义**；§4.4.2 把既有饱和函数说成「升为唯一入口」，与本文件同节要求的「溢出即拒绝」自相矛盾——两处都已改，记录在 §12.5。

### 本方历轮实质错误登记（原 12.5）

#### 12.5 本方此前各处实质错误（留此便于核对）

把「保留区与 `/memory` 重叠」当成冲突裁决项；把 `no-map` 被映射的后果写成必然硬件异常；RISC-V 腿被要求采集它并不产出的 UEFI 样本；物理内存属性被笼统写成「归 M 态软件管理」；「有 `size` 就是动态节点」；「整节政策不适用于 UEFI 腿」划得过宽；上一版对阶段类型覆盖范围的判断。

**第三轮又抓出本方三处**，全部已改：x86 选 PAT 表项被写成 `PCD`/`PWT` 两位（实为 `PAT`/`PCD`/`PWT` 三位，且普通页与大页的 PAT 位位置不同）；「只当闸门不违反忽略要求」是本方替规范做的宽松解释（撤回，改 §4.4.1 三案）；「每个已知属性位都有明确处置」长期只是判据、正文没有逐位兑现（§2.5 补）。另有一条属于**本方写作纪律的失守**：上一版把「2.11 才出现的位按基线外处理」写得很像容错设计，而它实际会让同一位在两个配置档里含义相反的情况静默通过——教训是**凡是「按版本解释位」的地方，必须先拒绝再解释**。

**第五轮又抓出本方五处**，全部已改：① 把「一份归一化坐标」写成「一份区间集合」，会把允许映射的普通保留区连带禁掉；② 数据流图里画了一个本仓并不存在的第三个区间输入；③ §6.6 要求交接条目携带来源，与 §6.3 刚拆开的两个对象自相矛盾；④ 逐位表漏掉「可热插拔」位的定性，并笼统称本档为「2.10 配置档」；⑤ §6.1 保留了一句「被排除的区域不许访问」，与自己「不整批纳入禁止清单」的决定冲突。此外「补齐位恒为零」是**规范没有说的**（那 4 个字节是 ABI 间隙，不是语义字段），「跟随 MTRR」也不是一个 PAT 类型。

**第六轮又抓出本方四处**，全部已改：① **本文档自己的内部矛盾**——旧第 4 步（现第 5 步）让「候选类型带 `RUNTIME` 位」成为诊断成员，第五轮补的交叉合同又让「普通类型带 `RUNTIME` 位」成为分类错误，同一个 `LOADER_DATA + RUNTIME` 两处判法相反。本方核对确认矛盾成立，修正方式是把它单独成步并只查一个方向，代价是本文第一次改写冻结条目（冻-6 由五步变六步）；② **循环定义**——§7.1 把配置档标识同时列为匹配维度与选择结果，按字面实现就是「要选档先知道档」，改为两层身份表、标识由命中返回；③ **与本文件同节自相矛盾的「同源」表述**——§4.4.2 说新数学与既有那条末端饱和的函数同源并「升为唯一入口」，而同节要求溢出即拒绝；本方另核出分类器已是正确的两步 checked 形状，样板因此换成后者，那条 C 复刻函数保持原位不改；④ **数据流缺失一个输入**——第五轮删掉 phantom 之后没有补上「启动必需对象」的真实来源，导致 §6.3 第二条要求的冲突检查没有输入可查，本方把 §0.3 画成「三行输入推不出四行输出」却没察觉，由评审人指出后新增 §6.8。

**第七轮又抓出本方三处**，全部已改：① **一条本方当成规范事实写进来的错误**——「带运行期属性位的描述符必须 8 KiB 对齐、长度为 8 KiB 整数倍」。它不是措辞问题：本方不但写进了正文，还据此向评审人提了一道「严格报错还是记录后继续」的政策选择题，也就是说错误已经进入裁决链。评审人核对官方文本后指出该要求是 4 KiB，8 KiB 那一支属于 Itanium 架构章节；本方自己的检索读数与另一份公开翻译在同一位置给出两种数值，而本方**当时没有核对那句话属于哪个架构分支**——这正是本文 §7.1 从第四轮起强制要求的「任何一致性断言要写明出版物标识、发布日期与取证位置」，本方第三次在同一类错误上跌倒，也是第一次栽在「章节归属」这一维（前两次的成因是「哪一份出版物」，这次是「同一份出版物的哪一个架构分支」）；② **把接错了下游的新抽象标成已闭合**——第六轮新增登记表时只接了冲突检查一个消费者，却在 §8 标为「已闭合」，而它要解决的那次故障起因是对象没进映射构造来源；一个只写完字段与生产者的抽象被本文当成合同交付，比没有这个抽象更危险，因为它会让读者以为历史问题已经闭合；③ **又一次数错自己列的关系条数**（「七条里有两条冗余」实际只有一条可推出），且**把三个结构性缺口错分进「取值问题」**、据此写下「四项先决做完即可冻结」——分类错误比计数错误的代价大得多，因为它决定了读者什么时候可以停止审阅。①②③ 合起来的成因是一个：**本方把「写了」当成了「闭合」**。本文此后加一条自检：任何新抽象进入正文时，必须点名它的每一个下游消费者以及「接不上时会怎样失败」，否则状态一律写「结构已定、接线待核验」，不得写「已闭合」。

**第八轮又抓出本方四处**，全部已改：① **把「写了」当成「闭合」的第二次现场**——第七轮新增的登记表本方在 §8 标为已闭合，而它只接了一个消费者；评审人上一轮刚说过的那句「接错下游的新抽象和没有这个抽象几乎同样危险」，本方第七轮采纳了它，第八轮却仍需要他来兑现它——说明「把一句采纳进来的话写进正文」不等于「按它自检」；② **从注释推行为**——本方第七轮断言引导壳中止路径「驻留所以跨边界」，依据是代码注释的自我描述，不是控制流；这是本文批评过三次的做法，本方第四次踩在同一类上（前三次是「把一处出处写成规范如此」，这次是「把一句注释写成程序行为」）；③ **又一条把异质的东西压成一个字段**——映射要求写成四选一的全局枚举，而本文自己那张表就写着两棵树要求不同；④ **一条形式上不成立的推导**——宣布「R6 由 R2 与 R5 推出」却没给量词，而这条推导正是上一轮「冗余关系要一一对应用例」那个说法的支撑。四处里有三处（①③④）的共同点是**本方在宣布一件东西已经做完时没有检验它的下游或它的推导**——第七轮总结的「写完字段与生产者不算闭合」这条纪律，本方自己下一轮就违反了，所以本文把它从「建议」升级为 §12.5 的自检条款并写清违反现场。

**本批编辑中本方自己造成并已修掉的工程事故**（与政策无关，但按本文「改动需要被看见」的纪律记在这里）：一处是本方插入的表格行漏了换行，与上一行粘连成同一行；一处是两段批量插入把代码片段标记按字面写进了正文，导致七十余处记号出现在成品文本里。两处都由本方在一次自检（逐表核对每行的分隔符数量与表头是否一致）中发现并修复；修完后仍有两行的分隔符数与表头不符，本方核查那是**既有文本里的命令行含转义竖线**，本批之前就在，因此按「不擅改与本轮无关内容」的约定只登记不改。**第八轮与第九轮又各留一处同类事故**：第八轮批量插入时把一个四字残片（「第六轮那」）粘在了第八轮错误登记段的段首，本方第九轮做表列自检时才发现——它不影响任何政策，但正是本文一直警惕的那种**渲染时不报错的静默错位**，已在第九轮删除并在此标注；第九轮则是同一类代码片段标记事故**第三次**发生（98 处按字面出现在成品文本里），本方本轮把「批量插入后必须全文搜一次标记残片」写进编辑流程，而不是再靠一次事后自检。**本轮另外处置一类本文从未归类过的排版问题**：若干检索式用竖线写「或者」，放进表格时会打断列——本方把本轮改动到的那些换成不带竖线的 `grep -e` 写法（语义等价，且本方逐条跑过），本轮没碰到的既有行仍按上面的约定只登记不改（档案第五节里还留着一行那种形状，读者看到它跨列时不必以为档案被改坏了）。**这条自检此后每次批量插入都要跑**：本文表格极多，列数错位在渲染时不报错，只会让读者看到少一列的行而意识不到少了什么——这与本文一直担心的「静默的错位」是同一类问题，只不过这次发生在文档自己身上而不是代码里。

第六轮那四处里有三处（①②③）是同一个成因的三种表现：**本方在采纳评审意见时只把它当成局部补丁，没有回读它与前文的关系**。第五轮那种「删掉一句兑现不了的承诺」的修正是对的，但采纳别人加进来的一句话同样需要检查它与既有句子是否相容——① 与 ③ 都是新加的话与旧话打架，② 是新加的话自身不自洽。教训写进正文的方式不是口号，而是本文此后每次采纳都要求「点名它改与谁相邻的那条」：第六轮与第七轮的正文变化每一条都写了它替换或限定了哪一段旧文。

**第四轮又抓出本方四处**，全部已改：① 「本仓取值与规范一致、已核实」缺出版物标识——2.10 有 2022 年原始版与 2024 年 Errata A 两份正式文档，本方当时取的是一份无法判定归属的 HTML 页面；② 「系统表头的修订字段用来选择或拒绝配置档」赋予该字段它不具备的区分能力，撤回后选档改成固件组合白名单；③ 「不做重置也不会读到脏数据」过头——类型系统不会清零静态内存，改为独占访问 + 有效长度 + 每轮从零计数；④ 把「固件常驻区」当成第三个保留来源（它是前缀阈值），以及把 40 字节写成「之外另有 4 字节补齐」。

**第九轮又抓出本方五处**，全部已改：① **本方第一次「没读代码就登记实现缺陷」**——本方在本轮一度把「配对循环不区分可重试与不可重试」写成本仓现存缺陷第五条，而读代码的结论恰恰相反：三路分派、重试上限与「其它状态立即失败」都在，且已写进那个文件的硬合同。成因不是粗心而是**方法**：本方把评审人的**政策要求**当成了对**现状的描述**——评审人说「合同里要写明只有版本戳失效可重试」，本方读成「本仓现在不这样」并顺着写了下去。这是本文从第一轮起反对的那类错误的镜像：**把「应该如此」与「已经如此」写在同一个句子里**。本方第九轮起给本文加一条硬规则：**凡本文出现「本仓某处是现存缺陷」，必须与该缺陷的现量命令同段出现，没有命令就不许登记**——此前本文只要求规范断言带出处，没要求缺陷断言带出处，而缺陷断言同样会被实现者当作改代码的依据，代价更高。② **本方自己标为「已核实」的代码事实过期了四轮**——「重试约束今天纯属巧合成立、没有任何断言」这条从第五轮起同时写在 §10 与 §13，而本仓实际有一条文本级审计用例覆盖两条入口（还带注入负控）。本方从未回读过那个测试文件。教训不是「要定期复查」这种口号，而是一条可执行的：**代码事实也要带复核日期**，与本文对快照提交的要求同构；本方本轮起在 §10 每条被改写的读数后面就地标注「第 N 轮修正」。③ **本方第八轮刚总结的纪律，第九轮又被证明不够**——第八轮的结论是「新抽象进入正文时必须点名每一个下游」，本轮补上它缺的另一半：**新证据进入正文时必须回读它否证了哪条旧规则**。本方第八轮用控制流证明中止路径跨过了引导根启用，却保留了自己写的「边界上最后一次使用归前一阶段」——同一条事实在同一份文档里同时否证着另一条事实，本方两轮都没看出。④ **又一条数错自己状态的地方**：§0.5、§7.2、§8、§12.4、§13 与文首六处轮次标签落后正文，其中三处是本方同一轮里改了正文忘了表——这是第八轮「把写了当成闭合」在**文档自身状态**上的复发，本方本轮把它列为独立的错误类而不是并入旧文清理。⑤ **工程事故两处**（代码片段标记残片 98 处、第八轮遗留的四字残片粘在段首），见本节末尾那段。五处里 ①② 是同一个根：**本方连续两轮把「我没读过」写成了「本仓没有」**，而前几轮的错误形态都是「我读过但读错了出处」。本文此后因此把「代码事实」拆成两级：带现量命令的算核实，只有叙述的算**未核**，不得写进正文。①②之外本方还要单独登记 ① 的**第二层**：那条退出状态码合同并不只在代码里，本仓的设计笔记与阶段台账早已写明它并已有两枚宿主用例锁住两个方向；本方是在三份材料都存在于同一个仓库的情况下把它当成待裁问题问出去的。成因很具体——简报被本文定位为「不依赖代码即可评审」，于是本方每轮只回读简报本身，从不回读项目其它文档。**这条比「没读代码」更值得警惕，因为它披着「这份文档是自包含的」这件合理外衣**。本文此后每轮回填前先跑一次跨文档检索：本轮被问的判据里出现的关键名词，在设计笔记与阶段台账里是否已有段落。

**第十轮又抓出本方六处**，全部已改：① **最严重的一处：本方在档案与阶段台账里登记了一条正文里并不存在的修改**。第九轮本方在本档案第八轮条目末尾与阶段台账都写着「已把设备树两条硬性判据标注为项目支持子集而非规范强制」，而正文 §4.3 那一行从来没有那半句——本方第十轮按评审人的第十节核对时才发现。这条与前几轮的「把写了当成闭合」不同档：那些是把未完成的说成完成，这条是**在一处记录里宣称已在另一处完成，而两处都不能自证**；评审人只读正文会以为标注存在，只读档案也会被误导。本文此后加一条硬自检：**凡在档案或台账登记「正文本轮已改 X」，必须同时给出一条能在正文里检索到 X 的关键词，收口前用该关键词回读正文一次**（已写进本档案 §六 第 5 条）。② **正文改了、等式没改**：第八轮把映射合同扩成四维、第九轮仍把 R13 写成物理覆盖的子集关系，而实现者与测试照的正是形式化那一行——本文第五次「把写了当成闭合」，也是第一次发生在**同一份文档的两个层次之间**。③ **断言了一个不存在的能力**：「`FinalMapDenyRanges` 可由内核从已交接输入确定性重建、不必新增交接协议」，而本方从未检查内核手里有哪些输入；它拿不到诊断成员与类型强制排除，所以那句话是空的。④ **把「已知但本项目不支持」写成「未知」**：类型 12 的定性与 §0.2 的标准类型表和冻-7 冲突，本方写那句时没有回读自己那张类型表。⑤ **清了算法段、漏了引导语**：§7.1 撤下运行时匹配算法之后，「只有第一层参与匹配」那句还留着，而它下面表格的第一层标题自己写着「不作匹配键」——本方第九轮逐段清理，却没做过一次「同一节内两句话是否互相矛盾」的通读。⑥ 工程事故一处：本轮把带竖线的管道命令写进表格单元，打断了一行四列（第三次同类），已换成 `grep -rc` 形式，并把「表格单元内不得出现裸竖线」并入每次批量插入后的自检。

第十轮这六处里 ①②③④⑤ 有一个共同根：**本方每轮都在写「下一批要做什么」，却很少回读「上一轮那些句子今天还成立吗」**。① 是这个根最贵的一次表现——它不是漏改，而是一处**主动写下的不实记录**。本文此后凡涉及「本方已改」的表述，都要求带一个可回读的锚（正文小节号或一段可检索的原文），这比要求「更仔细」可执行得多。

**第十一轮又抓出本方四处**，全部已改：① **术语表落后正文一整轮**——他点名的两处（`PmmPolicyUniverse` 词条仍是第九轮那套「先减掉待统计排除项」的公式，与正文第十轮的两层构造**相反**；`归一化集合族` 词条还挂着第十轮已经拆掉的 `StartupRequiredObjectRanges`）都是本方第十轮**改正文时没改词条**。这一类比一般笔误贵得多：词条是第三方读者的第一入口，他照词条实现，而词条在本文里是错的。本方第十轮刚立「登记已改 X 要带可检索关键词」，本轮暴露这条规则还缺半边——**此后回改任何名字时，术语表与顶部总表必须同一批改，并各留一个可检索锚**。② **又把匹配行数当成使用点数**：「122 处原子用法」实际是 `grep -rc` 的行匹配数，既不是调用次数也不是原子指令条数——本方第十轮刚写过「零命中类断言不得只看计数」，第十一轮就在同一份文档第二次犯，说明那条规则当时只覆盖了「零命中」没覆盖「非零计数」。本方把它改成「122 行匹配」并写明它不能顶替什么。③ **同一节内两句话互相打脸的第三次发生**：§0.3 写「前缀阈值不进任何区间集合」，构造式里却放 `ReservedPrefixRanges` 作成员，术语表再加一个词条——本方第十轮清理 §7.1 那处同类问题时说要做「同节通读」，本轮没有做。④ **本方第十轮那次「拆开」只拆了一半**：§2.1 把类型 11 与 12 分开，却在域外集合里把类型 11 与架构设备窗口并成一个 `DeviceAddressRanges`、还让它们共用消费者；他称此为「刚拆开的两种身份重新合并」，本方核对完全成立——**拆分的判据不是命名，而是消费者与后果是否也不同**，本方第十轮只做到了命名。
四处里 ①③④ 是同一个成因：**本方的「已改」停在被点名的那一段，没有向外扩到引用它的词条、总表与相邻集合**。本文此后要求每次改名或拆分时列一张「谁引用了这个名字」的清单（正文、术语表、顶部输出表、材料册），清单没跑完不得写「已改」。

**第十二轮又抓出本方两处**，全部已改：① **又一次「宣布已加、其实没加」**——第十一轮本方在正文写「`ExplicitPolicyDeniedRanges` 第一版为空，且只能经下面 E-1 那种具名登记增长」，而**正文里从来没有 E-1 的登记表**，只有一句提到它的句子；他（11.4）指出这一点时，本方第一反应是「它应该在材料册里」，但本文的铁律是「只读正文即可裁决」——被引用的规范依据不在正文里，等于没有依据。本轮已把 E-1 / E-2 表与七个必填字段写进 §0.3。这是第十轮那条「已改 X 必须带可检索关键词」的规则**第二次被绕开**：上一次是词条落后正文，这一次是正文引用了不存在的表。此后该规则扩成两条：**引用即自证**（正文说「见下面 X」，X 必须能在同一节内检索到）与**改名先列引用清单**。② **公式与名词分解不同步的第二次发生**：第十一轮把 `DeviceAddressRanges` 拆成两份，公式里只减了一份——与第十轮「拆了类型 11 与 12 却在域外集合里又并回去」同类：**拆分做了、依赖它的公式没跟着改**。两处都说明本方的「拆」停在命名层；本轮起每次拆分同时改三处：定义、公式、消费者清单。

**第二十六轮之后的落码阶段又抓出本方一处**：本方在缺陷表第五条登记「路径 A 的退出封装丢弃返回值」，并据此准备了「给调用点补失败分支」的实现批次；核对依赖源码后发现 `uefi 0.33` 的 `exit_boot_services` 在失败时直接 reset 机器、根本不返回，22 个测试内核调用者都不面对返回值，该「缺陷」不存在，实现批次撤销。这是本方第二次「未读源码先登记」（第一次是第九轮①），形态不同：第九轮是把评审人的**政策要求**误读成现状描述，这次是把**自己对封装签名的记忆**当成现状——本方从未打开过 `uefi-0.33.0/src/boot.rs`。硬规则再加一条：**凡登记「本仓现存缺陷」，必须附一条读过实现路径的读数；只凭调用点与签名形状推断的，一律写「待核」**。

第三至十二轮的四十处错误（第三轮三处、第四轮四处、第五轮五处、第六轮四处、第七轮三处、第八轮四处、第九轮五处、第十轮六处、第十一轮四处、第十二轮两处；落码阶段那一处另记于上）有三个共同成因，第九至第十一轮又各暴露出它们的新形态。**其一是出处问题**：本方把「我读到的一处出处」写成了「规范如此」。评审人在第三轮也犯了同一条，并在第四轮主动撤回对自己结论的确认；第七轮本方第三次跌倒在这条上，且这次错在一个新的维度——**章节归属**：出版物是对的（官方 2.9A），但那一支讲的是 Itanium，本方没有核对它适用哪些架构。因此 §7.1 的强制要求已从两维扩到三维：**出版物标识、发布日期、该句所在的架构分支或通用章节**，缺任一维不得写成「已核实」。**其二是抽象层级问题**：本方多次把「同一时刻都为真的一组事实」写成「同一个对象」——补齐位与语义字段、坐标归一化与集合合并、不分配与不许映射、描述符版本与属性位布局、类型与生命周期属性，都是这一类的混写。这类错误的共同后果不是文档难看，而是**实现者会照字面写下一个静默过严或静默过宽的对象**。因此第五轮之后，本文新增的每条合同都必须能回答两问：它约束的是哪一类对象，以及违反它时是启动失败还是静默少一块内存——后者一律不接受。**其三是第七轮才暴露的「把写了当成闭合」**：新抽象只写完字段与生产者就被本文标成合同交付（登记表的第二个例子最严重——它接错了下游，却让读者以为那次真实缺页已经解决）。这类错误的后果与前两类不同：它不会让实现写错，而是让**审阅停止得太早**。因此本文自第七轮起状态词汇收紧：一个抽象只有点名了每一个下游消费者、并写明接不上时会怎样失败，才允许标为已闭合；否则一律写「结构已定、接线待核验」。

---

## 四、已被重排替代的阻断表（原正文第八节）

**优先阻断项与补强（第四轮重排）**：上一版列的五项优先阻断里，第 1 项被本轮替换（它预设修订值能区分出版物，而这一点不成立），另加三项。本方逐条给出「文档是否已改」与「还欠什么」：

| 优先级 | 内容 | 文档状态 | 还欠 |
|---|---|---|---|
| **先决** | **配置档选择机制重做**：表头修订字段区分不了 2.10 原始版 / 2.10 Errata A / 2.11，选档改成**已核实的固件组合白名单**；登记四份出版物的读数与本仓常量的混合归属 | §7.1 已重写；§2.5 改为按配置档标识键控 | 白名单的数据结构、至少一套已采样本的组合、固件身份与修订字段的读取 |
| **先决** | **页级归一化只允许一份集合**：向外取整后的同一区间集合必须同时用于可用表切除、禁止清单生成、启动对象冲突检查与覆盖验收；否则同一物理页一边不许映射、一边可被分配 | §6.3 已写入，并给出可构造的反例形状 | 末端相加的溢出与架构上界检查；规则同样适用于保留映射块条目 |
| **先决** | **禁止清单的唯一生成阶段**：三类输入汇入归一化与集合规划阶段，产出跨边界清单；合并区间时不得丢来源 | §0.3 已正式列出第四种输出并画出该阶段；§6.3 已拆成「纯区间集合」与「来源诊断」两个对象。**本行的输入名单已过时，照录以保留评审人当时的判断**：其中「启动信息保留输入」经本方核实并不存在（第五轮删除），第六轮又补了一个真实输入「启动对象登记表」。现行名单以 §0.3 为准：UEFI 分类结果、设备树保留语义、前缀阈值、启动对象登记表 | 区间传输格式与来源位图的具体定义 |
| **次级** | 预备阶段对象对静态输出缓冲的独占、有效长度与失败轮次的状态管理（类型不会替本项目清零内存） | §3.3 已补六条不变量；两条路径各一条独立验收用例的判断已获评审人签署 | 实现 |
| **次级** | 「固件常驻区」是前缀阈值而不是区间集合（改名 `ReservedPrefixEnd`），不得与任意保留范围取并集 | §4.3 已改分类与命名 | 命名落地 |
| **次级** | 保留映射块与设备树本体、启动模块原始存储相交时的访问阶段与生命周期（复制后使用，或失败） | §4.3 已补规则，并指出本仓已有「复制后再用」的同形先例 | 交接前各访问点的阶段清单 |
| **次级** | 措辞两处：版本 1 的 40 字节**包含**补齐位；`WP` 的规范双重归类与 `SP` 的项目加强性质要分开写 | §0.2、§2.5 已改 | — |
| 整机与硬件验收 | AArch64 的共享性字段与属性索引是两个维度，要分别验收；RISC-V 平台清单必须含原子操作能力的证据 | §1.3、§1.5 已补 | 整机读数 |

**第五轮重排的阻断表**（架构方向不再讨论，缺口全部是数据合同；两份意见的判据一致）：

| 优先级 | 必须闭合 | 本方状态 |
|---|---|---|
| 先决 | 属性档要有**身份**（不叫「2.10 档」）、完整位表（含「可热插拔」这类档外扩展位的定性）、匹配算法四路分明 | §2.5 已命名并补行；§7.1 已写算法与身份维度 |
| 先决 | 唯一归一化阶段产出**多个语义集合**及其集合关系，不再统称一个禁止集合 | §0.3、§6.3 已改；数学条款 §4.4.2 |
| 先决 | 第三个区间输入要么给出真实定义、要么删掉 | 本方核实本仓**没有**该列表 → 已从数据流删除（§0.3） |
| 先决 | §6.6 与 §6.3 的条目字段矛盾、§2.1 与行 6 的类型注册矛盾 | 两处已改 |
| 次级 | 运行时服务类型的显式政策与「类型 × 运行时属性位不一致即错误」 | 已补 §2.1 |
| 次级 | `PERSISTENT` 完整范围与有界样本的冲突 → 二选一 | 选「不承诺可恢复」，承诺已删 |
| 次级 | `reusable` 单独出现、多段 `reg`、`/memory` 的 `hotpluggable` | 已补三条第一版政策 |
| 次级 | 第一次退出尝试后的受限重试阶段 | 已写成 §3.4 剩余一问（粒度待定） |
| 次级 | 诊断字段按来源拆层 + 两种计数口径 | 已补 §5.2 |
| 整机与硬件验收 | x86 PAT/MTRR 有效类型、AArch64 MAIR 与共享性、RISC-V 分区间原子能力 | §1.3、§1.5 已改写；RISC-V 一项升为准入条件 |



**第十轮移入的第六轮重排阻断表**（原正文 §8，被第十轮那张表替换；评审人当时明确「只处理具体问题，不重开六组讨论」）：

**第六轮重排的阻断表**（评审人本轮明确「只处理具体问题，不重开六组讨论」，本方按其四项定向修订组织）：

| 优先级 | 必须闭合 | 本方状态 |
|---|---|---|
| 先决 | 运行时类型的政策**顺序**与**规范定性**：修掉旧第 4 步（现第 5 步）与交叉合同的矛盾，区分规范要求与项目加强 | 已改（§2.1 六步 + §2.1.1），冻-6 随之由五步改六步；**双向等价那半句本方反驳**，改为单向校验（§12.3 第 5 条） |
| 先决 | `StartupObjectInventory` 的生产者、生命周期，以及 `PmmExcludedRanges` 的完整关系 | **第六轮本方标为「已新增、已闭合」是过早的（第七轮评审人指出，本方核对确认）**：登记表当时只有冲突检查一个消费者，既没有将存活对象汇入 PMM 排除集合，也没有把「映射要求」这个字段接到两棵树的构造与覆盖验收上。第七轮补齐为三个派生结果与 R4/R7 两条关系，并把它的状态改为**结构已定、下游接线待代码核验**。集合等式由五条扩到七条再扩到十三条（§0.3） |
| 先决 | 区间运算的溢出与上界表示、代码归属；RISC-V 的一致主存与排序合同 | 两步走与代码归属已定（§4.4.2）；**但「上界」的语义第七轮才暴露为未闭合**——本仓同一个概念有格式上限与能力上限两份实现，而内存图准入用的是前者（新增 §4.4.4）。RISC-V 补两列并收紧准入（§1.3） |
| 先决 | 清理旧决策、把配置档选择机制改为构建期绑定（第八轮关闭） | 四处旧文已清（§4.4.1、§4.7、§8、§13）；身份拆两层、标识改为输出、三条匹配规则已写（§7.1）。**但「补齐运行时匹配键」这条第六轮被本方归入取值问题，第七轮按评审人意见升为先决**：本方的算法在当前读取能力下会拒绝本项目自己的三条腿（§7.1 的三情况表） |
| 已撤销 | 原「8 KiB 几何要求的处置分级（严格 / 记录后继续）」一条 | **第七轮撤销整条**：评审人核对 UEFI 2.10 与 Errata A 后指出要求是 4 KiB，8 KiB 属 Itanium 章节。本方撤回判据本身，因此不存在「严格还是宽容」这道选择题；现有 4 KiB 检查已经与三个目标架构一致（§2.1、§7.2 第 12 项） |
| 次级 | 登记表的阶段档数与栈的物理范围取法：**已关闭（第九轮裁定）**——四档保留但边界按体系结构状态切换定义；栈从分配点或已有页表映射取（**「最小完备集是否十一项」这一问已被评审人给的判据取代：改为按「是否跨越内存图改变 / 页表切换 / 分配器启用三类边界」逐个对象追溯**） | 本方按新判据复查，自查出两类漏项（引导壳自身映像与栈、原始内存图缓冲与规划工作区）已补入 §6.8；阶段档数与栈的取法**第九轮已由评审人裁定并落笔**（四档保留、边界按体系结构状态切换；栈从分配点或已有页表映射取） |
| 次级 | 集合关系是否收敛为最小无关集；新数学落 `minix-boot` 还是 `minix-types` | 本方倾向保留三条冗余以便逐条断言、落 `minix-boot`；**待评审人裁定** |
| 整机与硬件验收 | 仍等 x86 PAT/MTRR 有效类型、AArch64 MAIR 与共享性、RISC-V 分区原子能力**与一致性/排序**证据 | §1.3、§1.5 已改写 |

**评审人第七轮的冻结判断被本方采纳，并据此改写上一段结论**：本轮之后**仍不建议冻结**。第六轮本文写过「四条先决做完即冻结结构」，那个判断现在看不成立——评审人指出、本方核对确认的三个结构性缺口是：

1. **启动对象登记表的三个下游没有全部接上**（尤其存活对象与可分配页不得重叠这一条），它直接关系到对象会不会被分配器覆盖，以及本文自己记录的那次直接映射缺页是否真的被解决。**第七轮已在文档层补齐关系与合同，但闭合要靠代码接线与验收用例**，不能靠本文再说一遍就算完。
2. **属性配置档没有一个当前可执行、足以区分目标固件组合的运行时身份键**，单条登记记录补不上这个缺口（§7.1 三情况表）。第一版要在方案 A（身份闭合）与方案 B（限制支持环境并诚实声明）之间**显式择一**，本方选 B 并请评审人确认。
3. **物理地址上界的语义、来源与越界处理没有对三个架构统一闭合**（§4.4.4）：x86-64 的 52 位是格式上限而不是每颗处理器的实际能力，而本仓内存图准入用的正是这个格式常量；RISC-V 侧那个平台位数是写死常量而非按平台登记的证据。

本方接受「不必再发散到属性政策、设备树 A 案或架构抽象原则」这条限制——第七轮的正文也确实只动了上述三项加两处清理与一条规范事实纠错。下一轮的范围按评审人的话组织：**围绕这三项补合同，再做一次完整的交叉审计**，不重开六组讨论。

**同时撤回第六轮那句「遗留全是取值问题」**：它当时就已经不成立（身份键问题是结构问题，本方把它错分类了），第七轮把它改写成上面三条。本文对「什么算已闭合」的判断标准也据此收紧——**一个抽象只写完字段与生产者不算闭合，它的所有下游消费者都要点名**。

第三轮那五项里仍然有效的部分：出界闸的项目策略（§4.4.1，本方与评审人的 A/C 取舍**已在第五轮由本方让步关闭，选 A**）、阶段对象补中间类型（§3.3）、清单的三条页表合同（§6.3，第五轮又补了归一化与来源分离两条，第六轮补了它的输入来源 §6.8）、逐位表兑现（§2.5，改为按配置档键控）。「三个保留来源」一项在第四轮被修正为「两个区间来源 + 一个前缀阈值」，第六轮再补一个非内存来源的输入（启动对象登记表）。

**结构性提醒（第三轮，本方接受并升格为合同条款，见 §6.7）**：本轮已经出现六个语义不同的物理范围集合。下一版不要用「往处置枚举里再加一个值」的方式容纳它们——每个集合要单独定义输入、输出与消费者，集合之间用区间运算表达。否则在虚拟内存、物理页分配器、直接映射三个下游会各自再长出一套彼此不一致的判断，那正是本文要消灭的病。**第六轮的验证**：这一条纪律救了一次——新增 `StartupObjectInventory` 时本方的第一反应是「再给分类器加一种处置」，按这条纪律检查后改为「规划阶段的第四个输入、不是第五种去向」，登记表因此没有污染处置枚举。

**流程判断**：第三轮建议不再做无边界润色，第四轮重申——完成上表的先决项文档修改后即**冻结政策与实现合同的结构**，随后转入代码与真实样本核验。本方同意，并据此把 §13 的材料包从「评审需要」改为「下一批认领的取证清单」。本轮唯一的例外是配置档选择机制：它不是一个可以留到实现阶段再定的细节，而是 §2.5 整张逐位表的输入前提，因此必须在冻结前定成白名单形态。

---

---

## 五、已被后续轮次替代的判断与取证表（原正文第十三节）
**第十轮移入的已被替代判断（原正文 §7.1 的运行时选档残留）**——第八轮确认方案 B、第九轮撤下匹配算法、第十轮清完正文之后，下面五段从正文移到这里留档；它们是「配置档白名单作为运行时选择器」这条旧路线的完整形状，**不再有任何一条是现行政策**：

**替换机制：以已核实的固件组合为键**（采纳第四轮建议，本方补实现含义）：

| 信息 | 能用来做什么 | 不能用来做什么 |
|---|---|---|
| `EFI_SYSTEM_TABLE.Hdr.Revision` | 记录表接口修订，作为白名单条目的佐证与诊断输出 | **不能单独决定属性配置档** |
| `FirmwareVendor` / `FirmwareRevision` | 识别具体固件产品与版本 | 厂商自定义字段，**不等于规范规定的 UEFI 发行版本**，不是普遍有效的版本选择器 |
| **项目登记的固件组合配置档**（条目内容：固件身份 + 配置档标识 + 逐位取值 + 原始属性样本 + 材料版本与日期 + 文件校验值） | 决定用哪张逐位表解释属性字 | — |
| 未登记的固件组合 | 第一版**拒绝启动**，除非另有明确写下的兼容合同 | 不得仅凭修订值落进某个档就进入分类 |

**白名单条目的字段与匹配算法（第五轮要求写成能实现的合同，第六轮再按「谁在什么时候看得见」拆层）**。上一版列的键（厂商串 + 厂商版本 + 修订值 + 档标识 + 样本）不够：厂商串只标识厂商、厂商版本是厂商自定义值，**两者合起来都不是不可混淆的固件构建身份**，同一组值可能覆盖不同板卡、不同构建选项与不同平台配置。

第六轮评审人补了三个更尖锐的问题，本方核对后**全部成立**，其中第二条是本方自己写下的逻辑漏洞：

1. **一部分身份维度在运行中的系统里根本读不到。** 模拟器的完整版本、它的机器类型与中央处理器参数、固件镜像文件的哈希，这些是**宿主侧**的知识；来宾里的引导程序看不到命令行，也看不到自己是从哪个文件加载的。把它们写进「匹配键」等于要求运行时用一个观察不到的字段做判据。
2. **`AttributeProfileId` 被同时当成输入与输出**——上一版那张表里它排在维度列，而它其实是匹配**结果**。按字面实现会写成「要选档，先知道档」的循环定义。本方承认这是我的写法错误，不是评审人误读。
3. **外部证据不能冒充运行时身份**，否则登记一堆宿主侧字段会让白名单看起来比实际更有区分力。

据此把字段拆成两层（**第十轮改正本句**：第九轮这里还写着「只有第一层参与匹配」，而它下面那张表的第一层标题自己写着「第一版不作匹配键」——**本方上一版清残留时清了算法段，没清这句引导语**，是评审人第十节替本方抓出来的）。方案 B 之下**两层都不参与运行时选档**：第一层只作运行时诊断，第二层只作流水线证据：

| 层 | 字段 | 用途 | 运行时可读？ |
|---|---|---|---|
| **运行时诊断**（第一版不作匹配键） | `FirmwareVendor` / `FirmwareRevision` | 记录用；第八轮之后它们不再是匹配键——方案 B 下配置档由构建期绑定决定，不靠运行时读厂商字段来选 | 是，系统表里 |
| | 架构 + 启动方式 | 区分同一固件家族的不同引导腿 | 是，编译期即确定 |
| | 设备树平台识别串（裸机腿）/ 板卡标识 | 平台身份 | 是，设备树里 |
| | 表头修订值、描述符版本 | 佐证与诊断，**不作为选档依据** | 是 |
| | 本次读图实际出现的类型与原始属性位组合（样本） | **验证证据**，不是选择器 | 是 |
| **流水线证据（不参与匹配）** | 模拟器程序版本、机器类型与关键配置、处理器型号参数 | 证明这套测试用的是哪套环境 | **否**（宿主侧知识） |
| | 固件镜像文件的校验值、固件构建号或源码提交 | 让条目可被撤销或升级 | **否**，除非项目自己把哈希随引导介质交给来宾（第一版不做） |
| | 证据来源与日期 | 审计 | 否 |

**这条矛盾要说透**（它是本轮三个结构性缺口之一，不是取值问题）：

| 情况 | 按本文算法的结果 | 为什么不可接受 |
|---|---|---|
| 把厂商串与厂商版本列为**必需**字段 | 当前实现读不到 → 缺字段即不匹配 → **零命中 → 拒绝启动**。本项目三条腿今天全部会被自己的白名单算法拒之门外 | 一个把自己开发环境判为不支持的准入合同不是合同，是把未完成的字段清单伪装成了政策 |
| 把这两项显式声明为**不约束** | 唯一那条登记项实际只按「架构 + 启动方式」匹配，**会命中同一架构同一启动方式下的任意固件** | 而这些固件的属性位布局未必相同（§7.1 整节讲的就是这件事）。此时白名单给出的「唯一命中」是虚假的唯一性 |
| 指望样本补上这个缺口 | 走不通 | 本文已正确规定**样本不是选择器**；某位没出现在样本里，不能证明该固件的其它构建不会给出它。用样本当键等于把自己刚立的规矩推翻 |

**因此第一版必须二选一并写明是哪一个**（评审人给的方案 A / B，本方采纳其形式并声明本方选 B）：

- **方案 A · 运行时身份闭合**：找到一个**来宾实际可读、且足以区分受支持固件组合**的身份维度。厂商串与版本可以作佐证，但本文既然认定它们「必要而不充分」，就必须再证明它们与其它字段组合后确实唯一——这是要拿证据的，不是声明的。
- **方案 B · 明确限制支持环境（本方选这条）**：第一版只运行于某个受控的模拟器环境时，**把这条限制写成外部支持前提**——由构建与测试流水线证明「本次启动跑在登记过的那套环境上」，并明确声明**这不是引导程序自己完成了运行时白名单识别**。宿主才知道的固件镜像哈希、模拟器版本与参数**不得**伪装成来宾已验证的身份。

**请评审人确认一点**：方案 B 成立时，白名单在运行时的实际角色会退化成「样本违约检查」（读到的位组合是否违反已登记档的约束），而**不再承担选档**。本方认为这是诚实的形状，但它与 §7.1 现在的标题（「配置档选择」）有出入——如果评审人同意 B，本节标题与该段的定位需要按「选择器未启用、启用前提是什么」改写，本方等他的裁定再动，避免本文连续两轮在同一处留下两种口径。

本方补的实现含义：白名单是**代码里的数据**，条目要能被宿主测试枚举，匹配是纯函数（输入身份、输出唯一档或三种失败）。**还要明确一点**：某个位没出现在样本里，**不能**证明该固件以后不会给出它——样本是「实际输出过什么」的证据，不是「能力上界」的证据，所以逐位表的兜底分支（未知位失败）不能因为样本干净而被放松。第一版真正能登记的组合只有本仓实际跑过并采到样本的那几套；这与裁决一的 RISC-V 三档清单是同一条纪律。

**本节结论（第四轮裁决：当前不能签）**：签的**不是**「2.10 可以、2.11 不行」，而是「**先有配置档选择机制，才有逐位表可用**」。在选档机制改成固件组合白名单、且至少登记一套已采到样本的组合之前，§2.5 整张逐位表都缺一个正确的输入前提。本文材料由四项再扩为**五项**：出版物标识与日期、常量来源与出处、允许掩码及其来源、固件身份（厂商串与厂商版本）、实际固件的原始属性样本（连同描述符类型）。

当前缺口（代码事实）：全仓既不读表头修订字段，也不读厂商身份，诊断输出里两者都没有；现量命令：`grep -rn 'revision' os/boot-shim/src os/libs/minix-boot/src --include=*.rs`（读数为零命中）。补这两项读取本身不需要任何政策变更，可以先行落地，作为白名单条目的数据来源。


**第五轮追加三条本仓现存缺陷作为同批处理项**（评审人看不到代码，由本方按读数补入）：设备树收集器在 8 格上限用满后**静默丢弃**剩余区间，多内存条的机器会少识别 RAM 且无线索；`RUNTIME_SERVICES_CODE/DATA` 的「不进可用表」今天只靠第 5 步末尾的兜底分支隐式成立，新增一条准入分支就可能静默破坏它；重试循环对「不得再依赖固件服务」没有任何类型**或断言**保证，今天成立纯属巧合。【**第九轮标注**：本条只有「没有类型保证」成立；文本级审计用例已存在，见 §12.5 第九轮②。】三者都要与上面两件事同批落地，而不是留成注释里的提醒。

区间规划阶段的输出按第五轮从「一份禁止清单」改为 §0.3 表里那四行（`PmmExcludedRanges`、`BootMapDenyRanges`、按阶段的 `StartupRequiredObjectRanges`、来源诊断），它们与分类器自身的两类输出（可分配、可映射但不可再分配）合起来构成 §6.7 登记的六个集合；关系写在 §0.3 与 §4.4.2，第六轮由五条扩到**七条**集合等式（补上「可映射保留区」与「诊断成员」也属于不许分配的完整集合这两条）。材料包一的白名单条目按第五轮的写法是「档标识 + 身份维度 + 样本」三样齐全才选得出档。

**第六轮把这句改准**：配置档标识是匹配的**输出**而不是输入，所以条目实际缺的是「运行时可读的身份维度 + 样本」两样；而本仓现在连厂商串与厂商版本都没有读取点（§10 现量命令），因此**第一版实际只能登记一条已核实组合**（§7.1 末与 §7.2 第 14 条）。本轮另有一项材料包之外的收口条件由评审人给出并被本方采纳：**四项先决修订做完即可冻结政策与合同的结构**，剩下的都是取值问题（8 KiB 判据的处置分级、登记表的最小完备集与阶段档数、集合等式是否收敛为最小无关集、白名单起步方式），不必再为尚未取得的硬件读数重写设计原则。本方按此把四类先决与四项待裁定取值分开登记：前者进 §8 的第六轮阻断表，后者进 §7.2 的等待项与 §12.4 的遗留表。

**第七轮把上面这段再改准，并撤销其中两个判断**。（一）「第一版只登记一条已核实组合」作为**解决方案**撤销——评审人给出两种情况，本方核对确认两者都致命：那些字段若列为必需，本仓读不到就是零命中，**本文的算法会拒绝本项目自己的三条腿**；若显式声明不约束，那条登记项实际只按架构与启动方式匹配，会命中同架构同启动方式下的任意固件，而这些固件的属性位布局未必相同，此时「唯一命中」是虚假的唯一性。指望样本补上也不行——样本不是选择器，某位没出现在样本里证明不了该固件其它构建不会给出它。身份键因此改为**必须显式二选一**（来宾可读身份闭合，或把限制写成外部支持前提并诚实声明引导程序没有自己完成识别），本方选后者，并在身份键闭合前保留 fail-closed、不开放泛化匹配。（二）「四项先决做完即可冻结」与「剩下的都是取值问题」两句撤销：本方第六轮把三个**结构性**缺口错分进了取值类。第七轮的收口条件改为**本轮之后仍不建议冻结**，剩余三项结构性缺口（登记表三个下游未接全、缺可执行的运行时身份键、物理地址上界语义未统一）与一次完整交叉审计才是下一步；本轮撤回的 8 KiB 判据本身就是「把一条不属于本项目的要求当成判据写了进来」的教训，也说明本方上一版「结构已定」的判断偏早。

**第三轮的流程结论当时被本文采纳**：优先阻断项的文档修改完成后**冻结政策与实现合同的结构**——**这个「改完先决项即可冻结」的口径已被后续四轮逐次推翻**（第四轮：先决项换掉一批；第五轮：结构可冻结但实现合同不行；第七轮：撤销「四项先决做完即冻结」；第八轮：评审人再次判「暂不冻结」，本方同意）。下表保留，是为了留下每一轮当时以为「做完这些就够了」的清单。；下表由「评审人需要本方提供的材料」改为「下一批认领的取证清单」，不再对本文做第四轮润色。

| 取证项 | 归属 | 判据 |
|---|---|---|
| 固件身份（厂商串与厂商版本）与表头修订字段读进入口诊断（**第八轮改造：仅作诊断与佐证，不再是建立白名单条目的前置**——配置档改由构建期绑定） | 引导壳 | 现量命令 `grep -rn 'FirmwareRevision\\|fw_vendor' os --include=*.rs \\| grep -v dockercargo` 从 0 变非 0；至少一套「固件身份 + 配置档标识 + 原始样本」的白名单条目可被宿主测试枚举。**注意这些字段只能作为白名单的佐证，不能单独选档**（§7.1） |
| 真实固件的原始属性样本（四时点 × 两条 UEFI 腿） | QEMU 整机 | 样本里出现过的位组合逐条能落到 §2.5 的某一行 |
| 消费依赖库已有的设备树保留映射块遍历接口 | 设备树来源 | 三条设备树腿的条目数与区间读数；与启动必需对象的相交检查结果 |
| 各腿实际收到的设备树（或其文本形式） | QEMU 整机 | 是否存在 `/reserved-memory`、有无 `no-map`、父节点 `ranges` 是否为空、单元数是否一致（**第八轮按评审人意见分级**：设备树规范对子节点 `#address-cells`/ `#size-cells` 与根节点取相同值的措辞是建议级的 `should` 而非强制，本仓把它当硬性判据属于**项目自己收窄的支持子集**，不是规范的一条 MUST；要求 `ranges` 为空同理——那是本项目不支持地址转换这条实现边界的产物。两处都要在正文标注为项目子集，不得写成规范要求，以免将来有人把它当成兼容性缺陷去「修规范」） |
| 出界闸按 A 案拆除（**第六轮改掉本表旧写法**：旧行要求「改用固件配置表交出的设备树来改造闸门」，与第五轮已定的 A 案冲突，照做会把已删的准入判据又建回来） | 引导壳 | UEFI 腿不再把设备树的 `/memory` 当 RAM 来源或准入判据；设备树快照本身仍要读（平台描述与保留子树都在树里）。现量命令 `grep -rn dtb_covers_conventional os --include=*.rs` ——**本方核实的现状**：该函数只有 1 个生产调用点，就在 aarch64 的 UEFI 腿里（`os/boot-shim/src/uefi_helpers.rs`），另有 1 处同文件测试与 1 处导出；两条裸机腿从来不叫它（它们走 `DtbMemoryMap::collect`）。因此 A 案落地的判据是**生产调用点从 1 变 0**、该函数本身是否保留另行决定，不是「UEFI 腿消失、裸机腿保留」 |
| 两棵树的页表叶子遍历（属性索引 + 禁止范围空洞检查） | 内核架构层 | 按最小页粒度的空洞判据为 0；两棵树各一次 |
| 三架构串口与判定脚本的稳定前缀 | 测试设施 | 成功摘要行与致命行都能被判定脚本解析 |

**第五轮追加的取证项**（前三条是**本仓现存缺陷**的修复，不需要新读数；后三条是新的数据合同落地）：

**第六轮追加的取证项**（先决四条对应本轮四项定向修订；标注「本方已核」的行给出可复核命令）：

| 取证项 | 归属 | 判据 |
|---|---|---|
| 设备树收集器在 8 格上限用满后的静默丢弃改为失败或显式诊断 | 设备树来源 | 现量命令：`grep -n 'DTB_MEMMAP_MAX' os/libs/minix-boot/src/memmap_source.rs` 后逐处确认写入点带 `else`；宿主用例构造 9 段区间，断言要么报错要么留下诊断，不得静默少识别一整块 RAM |
| `RUNTIME_SERVICES_CODE/DATA` 的显式类型政策替代兜底分支 | 引导壳分类器 | 新的第 4 步（类型与生命周期属性的一致性）里有一条按类型的显式判据；宿主用例覆盖「这两种类型 + 无运行时属性位」「与 `WB` 同时出现」两种形状，断言都不进可用表；删除该显式分支后用例必须失败（否则它测的仍是兜底） |
| 重试阶段对固件服务依赖的约束 | 引导壳 | 第一次退出尝试之后进入受限阶段，类型上拿不到引导服务的控制台、协议与分配接口；用例证明反复取键失败期间**没有**新的分配，且失败轮次的输出缓冲有效长度按 §3.3 的不变量归零 |
| 三个版本字段的**记录**与失败归因（校验本仓已有，见 §10 锚点） | 引导壳解析层与诊断层 | 摘要行与致命行都带上表头修订值、描述符结构版本与选出的配置档标识；用例断言「结构版本不受支持」与「配置档未命中」两种失败在诊断行里可读出差，且都以实际步长为跨距而不是写死 40 |
| 六个集合与十条集合等式的可执行断言 | 区间规划阶段 | 宿主用例**逐条**断言 §0.3 那十条：`ReservedMappedRanges ⊆ PmmExcludedRanges`、`DiagnosticMemberRanges ⊆ PmmExcludedRanges`、`BootMapDenyRanges ⊆ PmmExcludedRanges`、`AvailableRanges ∩ PmmExcludedRanges = ∅`、`AvailableRanges ∩ DiagnosticMemberRanges = ∅`、`ReservedMappedRanges ∩ BootMapDenyRanges = ∅`、`StartupRequiredObjectRanges(阶段) ∩ BootMapDenyRanges = ∅`（末条除非该阶段有经审的其它安全访问路径）；两条冗余条各留一条用例，目的是让「某一类范围漏出排除集合」有独立报警；区间数学九条边界（§4.4.2）各有对应用例，含末端恰好等于架构上界、原始长度相加溢出被拒、饱和只发生在末端钳制、暂存区超限 |
| x86 有效内存类型分析器的独立输入输出 | 内核架构层 | 纯计算组件，输入为 MTRR 使能位、默认类型、固定与可变范围、重叠优先级、物理地址宽度与 PAT 布局；输出为每个物理区间的**有效**类型；合成用例覆盖「PAT 选写回而 MTRR 判不可缓存 → 有效类型仍为不可缓存」。原始寄存器一致**不算**通过（§7.2 第 10 项） |



### 第七轮追加的取证项（原文错位排在第八轮之后，此处按轮次归位）

**第七轮追加的取证项**（对应三个结构性缺口；这一轮的重心从「文档写清楚」移到「代码接得上、用例测得到」，评审人明确要求下一轮做完整交叉审计而不是继续补概念）：

| 取证项 | 归属 | 判据 |
|---|---|---|
| 登记表三个派生结果各自接到真实消费者 | 区间规划阶段 + 内核映射规划 | ① 分配排除：R4 与 R7 有宿主用例，构造「一段加载器数据既进可用表又放着栈页」的形状并断言被排除；② 冲突检查：现有用例；③ **映射要求**：断言 `StartupMappingRequirements(树,阶段) ⊆ MappedRanges(树,阶段)` 在两棵树上分别成立。**这一条是本轮最重要的取证项**，因为它检验登记表是否真的解决了本文自己记录的那次缺页，而不是只多了一个抽象 |
| 可用表生成与物理页分配器的初始化路径读的是同一份排除集合 | 内核 | 现量命令与读数：本仓最终直接映射由四份来源取并集构造（可用表区间、自举页表树、登记的启动模块、保留表区间），而这段来源说明里正写着那次缺页与一条「需要对已装候选做区间减法才能闭合」的未竟项。验收要证明登记表的接入**消掉了那条未竟项**，而不是在它旁边再加一份来源 |
| 引导壳自身映像与栈、原始内存图缓冲、规划工作区进入登记表 | 引导壳 | 现量命令：`grep -rn "map_buf" os/boot-shim/src/ebs_pair.rs` 与分类存储的定义处，确认「原始图缓冲」与「分类器输出」是两块不同存储、各自有物理范围与最后使用阶段；中止处理路径那段「不用堆、写进栈上定长缓冲」的代码也要在登记表里 |
| 物理地址上界的两份实现合一 | 引导壳 + 内核架构层 | 内存图准入改读分页层那个「能力与格式取小」的值（**代码事实**：内核侧三个架构各有一份这样的访问器，x86-64 读 CPUID 扩展叶子、AArch64 读 `PARANGE` 且低于 48 位直接判不支持、RISC-V 取平台位数与分页模式编码位数之小），引导壳那个编译期常量按它自己的注释预告的方式被替换。用例：构造一段处于能力上界与格式上界之间的描述符，断言它被拒绝而不是被接受后在页表构造处失败 |
| 越界处置按来源分档，不存在未区分来源的默认裁剪 | 区间规划阶段 | 三个来源各一条用例（UEFI 描述符越界拒绝、设备树保留范围越界拒绝或按其政策、本项目启动对象越界拒绝）；归一化入口签名里能看到处置由调用方给出 |
| RISC-V 的平台位数从常量改为按平台登记的证据 | 平台清单 | 与 §1.3 的平台准入合同同批：登记每个受支持平台的实际物理地址位数与其出处（平台文档或实测），并把分页层那个平台常量改为从清单取值 |
| 白名单方案 B 的外部前提落地形态 | 测试设施 + 引导壳 | 若采 B：测试流水线要能证明「本次启动跑在登记过的那套环境上」，且引导壳的诊断行**不得**声称自己验证过宿主才知道的镜像哈希或模拟器参数；若采 A：给出一组来宾可读且能区分目标固件组合的身份字段，并证明它们的组合唯一。两条路都要能回答 §7.1 那三情况表里的第一种（不会拒绝本项目自己的三条腿） |

---

## 六、本档案的维护约定

1. 正文每轮回填时，把该轮的变化清单、结论表、回应与错误登记追加到本档案对应小节；正文只留当前状态与待裁问题。
2. 被撤销的判断在本档案保留原文并就地标注撤销理由，不删除、不改写。
3. 本档案不参与「能否签字」的判定：评审人签的是正文的合同条款；本档案回答的是「这条是怎么变成现在这样的」。
4. 拆分本身也记一笔：本档案创建于正文完成第八轮回填之后，正文与本档案的边界如上；拆分没有改变任何一条政策判断。
5. **一条第十轮新增的硬规则**：凡在本档案或阶段台账登记「正文本轮已改 X」，必须同时给出一条能在正文里检索到 X 的关键词（小节号或一段原文片段），收口前用该关键词回读正文一次。这条规则的起因是本方第九轮写下过一条正文里并不存在的修改，详见 12.5 第十轮第一条。

**第十一轮移入正文 §4.4.1 的三案细节**（A 案已在第五轮由本方让步选定，这里保留另外两案与让步理由，避免正文同时留两种口径）：

### 4.4.1 出界闸的项目策略：本方撤回上一版的结论（第三轮，本方接受）

上一版宣称「只当闸门不等于违反忽略要求」。第三轮不接受：规范原文是**必须忽略 `/memory` 节点**，而不是「不得用它代替内存图」。本方复述当前行为后同意这个判断——本仓确实让 `/memory` 参与了**启动准入决策**（落在设备树 RAM 全景之外就中止启动），因此不能写成「完全符合忽略要求」。这条撤回与「要不要保留这道闸」是两个问题，必须分开选：

| 案 | 语义 | 代价 |
|---|---|---|
| A | UEFI 腿**完全忽略** `/memory`，只用固件内存图 | 丢掉一类已有告警（固件快照与平台拓扑不一致）；实现最简单，规范关系最干净 |
| B | 保留出界闸，但**明确登记为本项目自己的额外平台一致性检查**，不宣称它是规范的「忽略」行为 | 规范关系诚实了，但准入权仍交给一份来源可疑的输入 |
| C | 只做诊断：报告不一致，不改变准入与内存集合 | 保观测、去掉误中止；但真出现「把非 RAM 当可用内存」时不再被拦住 |

**第五轮结论：选 A，本方撤回自己的 C 路线**（两位评审人共同反对，理由成立，本方接受）。上一版本方主张「先把输入换成固件在同一次启动里交出的设备树、权限停在只做诊断」，并把它当作比 A 更保观测价值的方案。反驳是：既然这条腿本就不该让 `/memory` 参与决策，那么无论它是否改变准入，留着它都要**长期维护一套跨来源一致性判断**，而这不是内存图政策的职责；况且当前输入还是从磁盘镜像读来的、与本机拓扑无证明关系。保观测的收益不值这份持续负担。

本方接受，并补三条落地含义：

1. **删的是这条腿上的 `/memory` 使用**，不是删设备树解析本身——同一份镜像里的设备树快照在 UEFI 腿上仍要读，因为平台描述与 `/reserved-memory` 保留语义都在树里。「忽略 `/memory`」与「读保留子树」不冲突（§4.2）。
2. 因此**出界闸的对照逻辑不能顺手把整段设备树读取一起删掉**——那是 §2.2 行 5 保留区例外的插入位置。
3. 若将来确实需要「固件内存图 vs 平台拓扑」的一致性检查，它作为**独立的平台一致性验证**立项，不重新混进分类政策。

本方保留的一条提醒：删除后，「固件把非 RAM 报成常规内存」这一类故障在本链路上不再有引导期拦截。它并未被本方案解决，只是被移到别的职责层（覆盖验收与后续内存测试），这个残余风险要如实写在这里，不能因为选了 A 就当它消失。

**（第六轮清理）** 上一版这里还留着「先做 C、输入换可信之后再升到 B」的落地顺序与一条请评审人裁定的问题。既然第五轮已定 A，那段方案与那条问题都已失效，本方删除，只保留下面这一条仍然生效的合同：

另外，`/reserved-memory` 与固件内存图的对照**不按描述符逐条相等**验收：一个静态保留区间可能横跨多条固件描述符，固件也可能把一片保留区切成若干相邻描述符。要验的是**覆盖集合与类型**（该保留范围在固件图里是否被全部覆盖，且覆盖它的描述符类型符合 §4.3 表里的要求），不是分段形式。**这一条与 A 案不冲突**：A 案删的是把 `/memory` 当 RAM 来源与准入判据，保留子树的解析与对照照旧（§4.2），而它正是 §2.2 行 5 那条保留区例外的插入位置。


@@@PART@@@

**第十一轮移入正文 §6.5 的退步方案 C′ 完整形状**（第九轮已判定它**不是当前政策**；留在这里只为说明它被什么取代，以及为什么不允许实现者自决「哪些 `no-map` 相交时拒绝、哪些跳过映射」）：

### 6.5 曾考虑的退步方案 C′：**不是当前政策，本节只说明它被什么取代**（第九轮定）

**本节此前的问题**（评审人第 14 项，本方核对成立）：§6.2 与 §6.3 已经采纳收窄版乙，而本节还写着「本方接受先 C′、下一批换乙的路线」——**同一份文档里两处现行口径**。评审人说得对：不能让实现者自己决定哪些 `no-map` 与恒等段相交时拒绝启动、哪些跳过映射，L3 的加强政策必须只有一个生效版本。本方把本节改为单一口径：

> **第一版正式政策就是收窄版乙**：禁止清单是一份跨交接边界的集合对象，引导根与最终根两棵树都按它执行（§6.3、§6.6）。

C′ 的内容保留在这里，只作为**它被取代的理由**与一段仍然成立的判据：

- **C′ 不是一段可以自行选用的降级模式，本文不提供它。** 它与乙的差别不是「清单大小」而是「引导根有没有判据」：C′ 允许引导根继续无差别覆盖，只在相交且无法证明安全时拒绝启动。那正是裁决六要修的那件事——评审人在第三轮就指出「分类器排除了某段」不等于「引导根没映射它」。若允许实现按 C′ 交付，L3 承诺在最关键的那棵树上不成立。
- **C′ 里有一条判据被乙吸收，不浪费**：「相交且无法证明安全时拒绝启动」就是 §6.3 第二条的冲突检查，它本来就是乙的一部分，而不是乙的替代方案。此前本节把它写成 C′ 的特征，是本方造成了这处混淆。
- **升级不改数据流这一条仍然成立**：C′ 与乙共用同一份清单对象，差别只在引导根构造时是否查询它。本方按这个形状把「实现内部可以分几步接线」与「哪一份政策生效」分开——**分步接线允许，切换政策不允许**。任何一步未接完时，正确的表现是启动失败并指出缺哪一段判据，不是自动退回 C′ 的语义。

**因此本节对实现者的约束只有一句**：`no-map` 与保留映射块的 L3 禁令按乙执行；本文没有「先松后紧」的备选档，也不引入编译期开关来模拟它。


@@@PART@@@

**第十一轮移入正文 §8 的三层状态框架**（第二轮提出、此后未再更新；现行状态看正文第十一轮那张阻断表）：

**三层状态（采纳第二轮的框架）**：

| 层次 | 当前判断 |
|---|---|
| 政策原则 | 大部分已收敛，可签 |
| 实现合同 | 先决项按第五轮重排，见下面那张阻断表；三处已在本文闭合（属性档身份、多个语义集合、第三个区间输入已删除） |
| 硬件与 QEMU 整机读数 | 不能用文档推理代替，必须实际取得 |

**第十二轮移入正文 §0.5 的两张裁定表**（第九轮「七个开放问题」与第十一轮「七处裁定」；它们已全部落进正文合同，留此仅供追溯）：

**本轮新提出、请评审人回答的两问**——两问都由本方按他的方向核出新事实后提出，不是旧问题重开：

1. **设备树保留语义的输入身份**：UEFI 腿上只允许使用固件配置表交出的那份设备树，磁盘镜像里附带的那份降为开发辅助信息——本方接受这个前提后，剩下的取舍在 §4.4.1b 末尾：要么**解析配置表那份**以取得 `no-map` 这类内存图表达不了的映射禁令语义，要么 UEFI 腿的保留语义**完全**由固件内存图的类型与属性承载、`/reserved-memory` 解析只在非 UEFI 腿启用。本方倾向前者，理由是后者会让 L3/L4 的加强政策在这条腿上无从取得。**请定一条**，本方不再同时保留两种口径。
2. **整套设备树保留政策的性质**：本方本轮核出全仓**从未**解析过设备树的保留子树（不是某条腿漏接）。那么 §4.2–§4.6 应当按「政策已定、等接线」冻结，还是按「政策已定、等实现」冻结？本方本轮改成后者，但这会让冻结判断往后挪一整批实现，请评审人确认这个定性——它决定读者什么时候可以停止审这一块。

**第十轮他把自己那一节列的七个开放问题直接裁掉了**（他原文的说法是「避免继续把已经有明确方向的问题悬在那里」），本方逐条核对后全部按裁定落笔，并登记落点：

| 开放问题 | 他的裁定 | 落笔与影响 |
|---|---|---|
| UEFI 腿的设备树保留语义来源 | 采用配置表那份，磁盘那份不得获得政策决定权；**复制后引导壳与内核都消费同一份复制目标** | §4.4.1b 的五步数据流。本方核对：复制从未发生、内核仍用原始地址，故这一条是**实现缺口**而不是文档缺口 |
| §4.2–§4.6 的状态 | 语义已形成，输入身份与生命周期链未闭合；代码属**待实现**不是待接线 | §4.7 与 §8 相关行按此改写 |
| 将来把某个 UEFI 类型升级为 L3 禁止映射 | 按类型、平台与证据**逐项登记**，不设通用升级开关 | §6.6 的变更清单加一行 |
| RISC-V 平台清单归属 | 代码常量表为准、文档作证据附件；**已裁定，不应再列为开放问题** | §12.4 那一行本轮关闭 |
| 新区间数学落哪个 crate | 倾向 `minix-boot`（两消费方的依赖交集，职责属启动内存政策；不必为区间结构塞进 `minix-types`） | §12.4 那一行本轮关闭；本方核对与本仓依赖读数一致 |
| 十六条关系要不要收敛 | 保留冗余（他写的是 R6、R7、R8；本方本轮新增的 R14 同档处理） | §0.3 那段计数已同步 |
| 诊断样本配额 | 固定、按处置独立配额；**排除与诊断成员优先**，设备路径只要汇总加较小样本；致命记录始终独立保留 | §5.3 那条配额判据与 §5.5 第二问（本轮按裁定关闭） |

**他另外给的两条建议本方按裁定处理**：A 项（R10 第一版不留例外通道、按树选禁止集合）见 §0.3 的 R10 改写；B 项（新增 R14 并在 R16 里写清它成立的前提）见 §0.3 的关系表与本节新增那段。

**第十一轮他给出的一批裁定与本方的落笔**（这一轮他不再只挑缺口，而是把七处直接裁了，并把两处「本方留给他的选择」自己选了）：

| 他裁的事 | 裁定 | 本方落笔 |
|---|---|---|
| `ExplicitPolicyDeniedRanges` 的归属 | 进排除侧，且**不得成为换名的兜底桶**；没有具体政策项就规定它为空 | §0.3：加进构造式 + E-1 / E-2 封闭登记表 + 第一版为空 + 来源完整性断言 |
| R16 的量词 | 补 `∀p` | §0.3 关系表 |
| 两棵树的实际负向验收 | R15 拆成引导根与最终根两条，且被测集合不得混入专用路径与设备映射 | R15a / R15b |
| 设备地址的两类身份 | 拆成 `FirmwareRuntimeMmioRanges`（类型 11，第一版无消费者）与 `DeviceWindowRanges`（真实窗口，生成设备义务）| §2.1 第 3 步、§6.8 |
| 类型 12 的样本改判 | **样本不自动改变归属**：它只证明固件输出，不证明本项目有合同 | §2.1 那格的取证门按此改写 |
| 设备树输入缺失 / 重复 / 损坏 | 三种状态必须分开，损坏不得降格成「没有输入」 | §4.4.1b 四态表 |
| 「是否解析配置表 DTB」这道二选一 | 他已裁定采用配置表那份 ⇒ 备选作废 | §4.4.1b 删掉备选口径，只留实现与证据待完成 |
| `compare_exchange` 等原子能力 | 登记主键必须是**真实指令实现路径**，且「122」要说清是行匹配 | §1.4 与 §8 两处口径改写 |

**他本轮问「L2 是否要加成绝对禁止」这一题本方直接答**：不要——本方按他的判据把 L2 限回「不得进入最终直接映射」的原义，**同时第一版不提供绕过它的专用路径例外**（两句要一起写，缺一句话就变成他批评过的那种「看起来可用的口子」）。若将来真要加强成绝对禁止，按 §7.2 那条流程作为一次**项目加强政策**变更登记，不借用 L2 这个词。
