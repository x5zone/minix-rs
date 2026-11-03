# 01-todo — 01 篇关联 todo 总台账

> **重写**: 归拢版（九轮评审终态后重制，取代 2026-10-20 版本）。

> **来源与方法**：本文件归拢 `01-boot-shim-bootstrap.md`（下称「01 篇」）射程内的**全部** todo 项——代码项与文档项都含，凡该篇 §7.3 关联代码清单与正文 TODO 标记涉及的条目一律在册。三个来源：
> ① **本会话九轮外部评审终态**：`01-code-design.md` v2→v9外部评审迭代至冻结；七份调查报告 `01-forensics-round3.md` 至 `01-forensics-round9.md`——**T1–T5 设计已冻结（v9）**，原「待裁决」条目全部拿到裁决，剩余工作收敛为实施、直修与文档同步；
> ② **stage 权威台账** `todo.md` §7（D/I/T 系条目的现状读数、修法、DoD、现算命令以其为权威，本文件只给摘要 + 指针 + 状态）；
> ③ **01 篇正文与 §5.3 交接表**（正文 TODO 标记 L1378/L1408/L1424/L1488/L1532 与 TODO-01~07，唯一登记处在篇内，本文件管状态与批次）。
> **口径**：`minix3/` C 源为真值；`os/` Rust 实现的偏差分三类——**注释失真**（改注释即闭环）、**抽象收口**（设计已裁决、待动码）、**测试缺口**（补测试即闭环）。行号为登记日快照，动手前以符号锚点重新定位。

---

## 0. 使用说明（给代码迭代 agent）

1. **一次一条**（todo-fix 纪律）：每条自带现状与验收；动手前先跑证据命令复核（源码可能已前移）。
2. **裁决已全部完成**：§2 各条的设计载体（`01-code-design.md` v9）已冻结，**直接按裁决实施**，不再需要外部评审轮次；只有实现证伪某项既定前提时才重开对应局部裁决（v9 冻结条款）。
3. **每条做完的通用验收**：`cargo test -p <对应 crate>` 通过；涉共享契约层（`os/libs/minix-boot`）的条目加三架构 `cargo build` 与 qemu 引导测试（`os/qemu-tests/run_all.sh`）回归；受影响的 01 篇章节同批更新（Doc Code Sync）。
4. **硬约束**：`#![no_std]`（测试除外）；错误映射 Minix3 errno；注释全英文自包含（`os/` 下）；三档术语 `[ARCH: ...]` 三处一致。

---

## 1. 总览（全部条目 × 当前状态 × 去向）

| 条目 | 一句话 | 类型 | 当前状态 | 去向 |
|---|---|---|---|---|
| **T1（D-67 吸收 + D-69 同批）** | 跨镜像 wire 协议 + handoff arena + 方案乙配对循环 | ✅ 已实施（01TODO-T1CUT，2026-10-30） | v9 冻结；类型/序列化/分类/配对/解码/失败族/builder/getter 与 2c/3b 联合切根全部落地；aarch64 v1 arena 端到端到 `/etc/rc`，旧 blob 删除，run_all 39/0/0 | §2.2 |
| **T2（I-1/NK1/OQ-N6）** | x86 切独立镜像（入口契约全套已裁） | ✅ 已实施（01TODO-T2X86，2026-10-30） | 入口契约落地：`_start` 建栈/清 `.bss`/`cli`/`cld`/`rbx` 交接；`arch_boot_resume_v1` x86 分支；`reserved_regions` 恒等重放半退场；模块 blob 上界；x86 冒烟门 PASS，run_all 39/0/0 | §2.5 |
| **T3（D-66）** | `param_buf` 生产侧接入 | ✅ 已实施（01TODO-T3PARAM，2026-10-30） | 统一 normalizer + UEFI LoadOptions + DTB `/chosen/bootargs`；aarch64 直核读数 `bytes=0x15 first=ARCH=aarch64`，UEFI 腿 `bytes=13`（无 LoadOptions）；内核零改动 | §2.4 |
| **T4（D-68 + DTB 扣除）** | bootface_a64 第四链迁移 + DTB 永久扣除 | 设计已冻结 → ✅ 已实施（01TODO-B3） | 独立小批完成；`cut_memmap` 真实 totalsize 记账测试与 aarch64 直核冒烟在案 | §2.1 |
| **T5（D-70）** | 落地区补 `platform_sources`/`param_buf` | ✅ 已实施（01TODO-T5） | 五块落地区在位；容量 4/1024 与 fail-fast 测试在案 | §2.3 |
| **D-59** | 大页能力/选页策略四层抽象 | ✅ 五阶段全部实施（D59PAGE/D59SIZE/D59POLICY/D59DEFEND/D59RENAME；合并 4dc6a797f/8d99bb774/f9bab95c2/5479b8083） | 启动选页先能力后对齐、`PageSize`/统一能力 API、事实与纯规划拆分、预检式安装、`map_huge` 防御、机械重命名全部落地；末轮三架构 `run_all` 39/0/0。覆盖损失门按 §2.6 未关闭验收门独立立项，不随迁移闭合 | §2.6 |
| **D-72（本轮新登记）** | bump 池耗尽结构债（kerninfo 注入链每新根抽页、池无回收、真机 ~40 次绑定死亡） | ✅ 已实施（01TODO-D72VM，2026-10-30） | 映射责任移交 VM（候选 A）：`VmBootHandoff` v7 加 `kerninfo_pa`，VM 建根统一映射，内核侧注入块删除；源码序钉 2/0 | §2.7 |
| **D-73（本轮新登记）** | riscv64 独立 boot-shim 腿：三架构统一主线 = shim + 独立内核镜像，自引导面降为特定嵌入式形态（FW_PAYLOAD / `-kernel` 测试） | ✅ U-Boot → boot-shim → kernel-uboot 全链 PASS（6/6 路标到 entering scheduler）；直核腿不回归；门读数见 §2.8 实施记录 | 交付=`riscv64-uboot-shim`（物理链接 VMA=LMA=0x84500000，自带 bootstrap 栈与 MNXBOOT1 表自建）+ `kernel-uboot` 接收镜像（`fw-riscv64-uboot` 特性，`arch_boot_resume_v1` 加性恢复）；入口 ABI=`bootelf -p <img> <dtb_addr>` 的 `argv[0]`=DTB 物理地址（表由 shim 在 .bss 自建，避开 staging 共变问题）；三处实施期修正：jump_to_kmain 执行位置探针（接收腿不加链接偏移）、shim memmap `firmware_reserve_end`=0x80200000（bootface 同源，堵 OpenSBI 足迹漏进 VM 空闲表）、门脚本 build 失败不再被 `set +x` 掩盖；门=`test-riscv64-uboot-shim.sh` | §2.8 |
| **N-1~N-5（本轮新登记）** | `BootAlloc::resume` 无区间校验 / vm_handoff 头注释与实现不一致 / 零消费者便利包装 / bootface 双份名字池与常量 / 池页零化机制归因未定案 | 直修或取证 | N-1/N-2/N-3 ✅（01TODO-B1）；N-4 ✅（01TODO-B4）；N-5 ✅（01TODO-N5 探针：映射重放成立、复用前提未出现） | §3 |
| D-60 | `IDENTITY_MAP_END` 双定义 | 闭合 | ✅ 双定义归一为全仓单定义（回勾确认）；01 篇 §4.6 措辞随修（见 §5） | §3 |
| I-22 | `MemoryMapSource` trait 收口三链 | 代码已落地 | ✅ 代码落地；文档连带 ①–⑦ 已逐项核（⑤ bootface 头注随 01TODO-B2 修正）；遗留 = D-68（T4 批） | §3 |
| I-28（本轮新登记） | `MemoryMapSource` trait 无多态消费者（无泛型约束、无 `dyn`；两个 impl 无活消费者） | 设计裁决 | ✅ **设计冻结（v12 终审）**：删除 trait 与空占位，保留 `DtbMemoryMap` 具体实现与内存图语义说明，正式 supersede I-22（supersede 记录属实施动作，随原型批执行） | §3 |
| I-29（本轮新登记） | UEFI 内存图分类双份实现；测试图不反映快照后（条件性）分配；分类判据用地址代替类型（`RESERVED`/`UNUSABLE`/`UNACCEPTED`/未知区间被塞进「可访问」保留表）；`Attribute` 未读且三架构缓存属性机制各不相同 | 设计裁决 + 测试改造 | ✅ **架构设计与目标合同冻结（v12 终审）**：唯一解析/分类、零分配 + 预分配输出、`ram_top` 删除（不留 characterization test）、方案 A、PA/DM 两合同、`DescriptorRef` 含 Attribute、side list 语义、五步裁决顺序（I-29-17，含四组合落定）。**暂缓冻结（各有验收门）**：属性政策矩阵逐行批准（凭原型 ①+三架构证据）、三架构 WB 合同（I-29-16）、Pass 1/2 具体 API（归原型 ①）、路径 A 运行验收（原型 ②）。设计讨论结束，转原型与实现（§7 执行序列） | §3 |
| I-30（本轮新登记） | 01 篇正文补「内存图生产的四层责任」概念讲解：UEFI 固件事务 → 描述符解析 → OS 内存语义分类 → 内核映射与分配，四层各有边界、不得混谈 | 文档教学批 | 允许开放（刚登记；概念讲解可先行，政策细节句随 I-29 落地同批校准） | §5 |
| D-71 | `BootForm::source_path` 悬空路径 | 注释/登记回修 | ✅ 已修（01TODO-B1） | §3 |
| I-26 | opensbi_helpers 孤儿注释 + "hardcoded" 顶注 | 注释回修 | ✅ 已修（01TODO-B1） | §3 |
| I-27 + L1424 | 01 篇 L1422/L2624「还没裁决」口径 + 张力指针 | 文档回修 | ✅ 已修（01TODO-B1；OQ-N6 口径与张力指针同批） | §3 |
| TODO-01~07 | 01 篇 §5.3 交接表七条测试缺口 | 测试缺口 | TODO-01/02/03/06/07 ✅；TODO-04 ✅（文本级，T1 接线后升级）；TODO-05 允许开放（真机窗口） | §4 |
| I-17/I-18/I-19 | 01 篇 §2.6 拆节 / 章头体检 / 讲解条续扫 | 文档结构 | ✅ 已改（01TODO-DOC） | §5 |
| I-23 / I-24 + L1378/L1408 | §3.7 fw_payload 段 / §3.1 否决表拆行 | 文档内容 | ✅ 已改（01TODO-DOC；设计侧 [ARCH] 镜像在主树工作区，未代提交） | §5 |
| L1488 / L1532 | 01 篇读者待办（`reserved_regions` 退场学习项 / 张力块性质澄清） | 读者侧 | 允许开放（用户侧学习/澄清项，AI 不代决） | §5 |
| I-25 | `os/` 全仓注释语言与锚点整改 | 代码注释 | ✅ 全仓完成：1,549 个 Rust 文件注释 CJK=0、广义锚点=0；654 个纯注释文件非注释 token 与基线零差异，另 1 个 `SimPaging` 测试替身按 D-59 补齐（合并 e754ea77a/9c9c24bbe/f933b53ab）。审计 `tmp/i25-final-audit.md`，构建 `run_all` 39/0/0、workspace 8,284 passed/0 failed | §5 |
| I-21 | 篇头「路径约定」重复上收 | 文档结构 | 允许开放（延后决策；W-10/W-11 冻结后再议） | §5 |
| I-20 | boot-shim 再导出注释 | 已闭合 | ✅（commit b0da9074e） | — |

---

## 2. 实施批次（设计已冻结，按既定次序 T4 → T1 → T5 → T3 → T2）

> 本节每条的**完整裁决内容**在 `01-code-design.md`（v9 状态收紧表 §10 + 各节）；本文件只给实施要点、验收锚点与现状复核命令。九轮评审的关键终态：wire 载荷走**单一 handoff arena**（信封宿主 + 语义工作分区双写 + 仅 `byte_len` 描述符 + `arena_len` 字段，独立 bump blob 页移除）；内存图权威走**方案乙**（`uefi::table::system_table_raw()` raw 配对循环，分类在 Boot Services 活着的规范明文窗口，MapKey 显式配对）；解码边界为**受控复制五步序列**（`KernelInfoView`/`BootReadPhase` 退役）；x86 入口契约全套已裁（镜像内 bootstrap 栈二段式、`rbx` 传 blob 指针、显式 `cli`/`cld`、red-zone 由目标规格闭合无需旗标、`load_elf_into` 机制前提归边界）。

### 2.1 批次一 —— T4（独立小批，最先）

**D-68：`bootface_a64` 第四条 memmap 链迁移**（权威：todo.md §7.1；裁决：01-code-design §4.3）

- 删 `os/kernel-image/src/bootface_a64.rs` 的 `fill_memmap`/`clip_firmware`/`MEMMAP_STORAGE`，改调 `DtbMemoryMap::new(&fdt, 0).collect()`；`fw_end = 0` 带形态前提成文（该腿无固件常驻区于 DRAM），与内核侧 `cut_memmap` 两机制不替岗。
- **v5 定案增补**：DTB 永久扣除——Phase A.2 补一笔 `cut_memmap(dtb_pa, totalsize)`（与 D-64① 内核镜像自切同型，损耗 ≤ 2 页）；**腿侧无需预对齐**（round-5 C 批核实：`cut_memmap` 自动页化扩展 `[floor_P(start), ceil_P(start+len))`，带 saturating/回绕保护，非页对齐测试已在库 `test_cut_memmap_alignment_*`）。
- 验收：aarch64 target 构建通过；直核冒烟；`grep -n "MEMMAP_STORAGE\|clip_firmware" os/kernel-image/src/bootface_a64.rs` 归零；DTB 扣除端到端检查（真实 `dtb_pa + totalsize` 传入后记账与预期页区间一致）。**状态**：✅ 01TODO-B3——`MEMMAP_STORAGE`/`clip_firmware`/`fill_memmap` 三归零；Phase A.2 Step 4 用 `dtb_blob_len`（8 字节头）取真实 totalsize 后 `cut_memmap`；宿主测试用真实 QEMU blob（7699B @ 0x4000_0000）断言切除后自由段 `[0x4000_2000, +512MiB-0x2000)`；aarch64 kernel-image（SD-23 RUSTFLAGS 门）0 error；`test-smp-aps-aarch64.sh` 直核全链 PASS（ap-arrived=1）。

### 2.2 批次二 —— T1（wire 协议 + arena + 方案乙；九轮评审的主体的落地）

**T1 实施拆步（裁决载体：01-code-design §1.3/§1.4/§1.5（四）（六）（七）/§9.2–9.4）**：

1. **协议类型层**：`BootHandoffV1` + `Wire*` 元素类型（`WireMemoryRegion`/`WireBootModule` 16B 定长名槽/`WirePlatformDescSource`）；信封内切片描述符**仅 `byte_len: u64`**（`WireSlice{addr,byte_len}` 已在 v8 退役——固定分区布局下地址由 arena 基址 + 协议偏移推导，三类指向错误不可表达）；协议头 `magic`/`version`/`size`（= 信封结构大小）+ **`arena_len`**（= 协议常量 `ARENA_CAP`，接收方验证相等）；`free_upper_idx` 用 `u64::MAX` 哨兵；`MemoryRegion.len` → u64。
2. **arena 布局常量**（编译期 + 进布局门禁基准）：自基址起 = 信封（≤ 256 B 预算）→ wire 分区（128×16 + 256×16 + 12×32 + 4×16 + 1024 = 7,616 B）→ 语义工作分区（`MemoryRegion` 双写供给 shim 侧，6,144 B）≈ **4 页（16 KiB）**；`Wire*` 元素 size/align/offset + 分区偏移逐目标交叉编译断言。
3. **方案乙配对循环**（`prepare_boot` 末节替换现「快照 A + helper EBS」）：`uefi::table::system_table_raw()` → raw `boot_services` → 外部缓冲 `GetMemoryMap`（自管 buffer：`memory_map_size()` 实查 + `desc_size×8` 余量，EBS 前 LOADER_DATA 单独分配）→ 分类**双写**（`WireMemoryRegion` 进 wire 分区、`MemoryRegion` 进语义分区；非内存图载荷——模块名/平台来源/param——在此前已序列化）→ 填信封静态字段 → raw `ExitBootServices(image_handle, map_key)`。
4. **四条硬性实施契约**（缺一不实施，round-9 + 九轮终审）：① **EBS 后三零**（零分配/零释放/零 BS 调用；RAII 清理在循环前完成——`prepare_boot` 链 Drop 面盘点为零；失败与终止路径同受约束）；② **失败原因分派**（仅 `EFI_INVALID_PARAMETER` 重试；其他状态立即失败；总尝试 2 次超限走不依赖 BS 的冷复位；缓冲三段处置——EBS 前可扩容 / 失败后只复用 / 复用不足 fail-closed；不沿用旧分类结果）；③ **描述符遍历**（`descriptor_size` 步长；引用建立前验证——version=1、desc_size ≥ size_of、**desc_size 为对齐整数倍**、首地址对齐、边界与整除；对齐不满足不强行建引用；循环体零 fmt/日志）；④ **区间 checked 算术**（`checked_mul`/`checked_add` + 页数非零 + PA 范围验证后才生成 `MemoryRegion`）。
5. **接收端受控复制**（镜像侧 `store_kernel_info` 的 wire 版五步）：受限原始读取验头（通过前不构造 wire 引用）→ 复制信封 → 逐载荷验证（容量 + 编码守恒两条；溢出/对齐/⊆ 已由布局构造保证）→ 受保读取阶段复制进 `.bss` 落地区 → 从落地区构造 `KernelInfo`。**切根后**：arena 信封原地补运行期字段（bump 三件套）、跳转 `x0 = arena 基址`（独立 blob 页删除）；验收改「arena 不占 bump 区 + 信封游标 = 跳转前真值」。
6. **九面失败测试族**（§9.3）：无效 version / desc_size 不足 / 对齐不合法 / 不整除 / 扩展步长 / 区间溢出与零页数 / 复用仍不足 fail-closed / 第二次 EBS 失败 / 非 `INVALID_PARAMETER` 状态不重试。

**D-67 存留件**（原信封合一 a–e 的 v9 后形态）：模块注释 "only data crossing the boundary" 失真句随实施删除；`BootPrepareResult` 保留为进程内类型（合一已裁不做）；builder 统一（D-69）随本批——`build_kernel_info` 收敛单一构造面、统一签名带 `reserved_regions` 形参；getter 家族补 `param_buf`。

**D-69**：两处 `build_kernel_info`（opensbi 8 参 vs uefi 9 参）随本批收敛；证据 `grep -rn "fn build_kernel_info" os --include="*.rs"`。

**T1 落地后 01 篇需同步的章节清单**（本轮 review 补列，避免实施者漏改）：§3.1 交接表「内存布局」行（快照两份列表 → 配对循环 + 终图）；§3.4 L1422 段的「切片深拷」叙述（受控复制五步）；§4.1/A. 主流程骨架（`prepare_boot` 末节换配对循环、blob 页移除）；§4.2 八步表（步骤 6「两次列表在同一快照分流」与「列表页快照后分配」的不变量整段改写——方案乙下不再有「快照后分配」类别）；§4.5 落地区表（三块 → 五块随 T5、加「wire 版受控复制」）；§4.7 bump 游标段（blob 页移除后的 `bump_next` 语义）；§5.1 M4/M12/M13 的断言与覆盖状态；§6.1 里程碑清单（新增配对循环路标、`memmap ok (final pre-EBS snapshot)` 字样替换）。

**子步进度**：1a ✅（01TODO-T1A）`handoff_v1.rs` 类型/arena 常量/布局门禁；1b ✅（01TODO-T1B）`handoff_v1_write.rs` 生产者序列化器；2a ✅（01TODO-T1C）`ebs_map.rs` 描述符走查分类；2b ✅（01TODO-T1D）`ebs_pair.rs` 配对循环可测核心；3a ✅（01TODO-T1E）`handoff_v1_read.rs` 接收端解码器；5a ✅（01TODO-T1F）D-69 单一 builder + D-67 旧口径；5b ✅（01TODO-T1G）`param_buf()` getter；**2c/3b ✅（01TODO-T1CUT，2026-10-30）**——aarch64 `prepare_and_enter_kernel_image`（arena + 配对循环 + 建根 + 原地补字段 + `x0=arena`）与 `enter_kernel_image_v1`/`arch_boot_resume_v1`（五块解码进 T5 落地区 + additive resume）落地，旧一页 `BootHandoff` blob 与 `bootstrap_to_kernel_image` 删除。**4 九面失败测试 ✅**（映射：无版本→`rejects_unsupported_version`；步长不足→`rejects_descriptor_smaller_than_prefix`；不对齐→`rejects_unaligned_descriptor_size`；不整除→`rejects_map_size_not_multiple`+`rejects_non_divisible_descriptor`；扩展步长→`padded_descriptor_stride_is_honored`；区间溢出/零页→`rejects_page_count_overflow`/`rejects_range_overflow`/`rejects_zero_page_count_on_produced_region`；复用不足 fail-closed→`too_small_on_retry_is_fail_closed`；二次 EBS 失败→`two_invalid_parameters_exhaust_the_retry_budget`；非 INVALID 不重试→`other_ebs_status_fails_immediately`）。**2026-10-30 补**：TODO-04 已升级为两条入口的文本级审计（`prepare_boot` 与 `prepare_and_enter_kernel_image`，5/0）；T1 的配对循环状态码修正是 `EFI_ERROR_BIT | code` 高位编码，原 2/5 只命中低字节。

**2c/3b 联合切根实施方案（下一笔开工前先读；T5 已备齐五块落地区）**：

- **载体**：aarch64 专属 `uefi_helpers::prepare_and_enter_kernel_image(bump_pages)`（只在 `target_arch = "aarch64"` 编译），不复用 `BootPrepareResult`——避免给 38 处构造点加字段；x86 内联路径保持到 T2 再统一。
- **发送顺序**：装载 kernel/模块 → 平台源 → root/bump 分配 → arena（`AllocateMaxAddress(bootstrap_alloc_limit(4 页)), LOADER_DATA, 4`）→ map buffer（`AllocatePool`，`memory_map_size()` 实查 + 余量）→ 非内存图载荷序列化（信封/模块/平台源/param）→ 配对循环（raw `get_memory_map` → `classify_map` → raw `exit_boot_services`；uefi-raw 0.9 的表字段签名无 `This`，状态码用 EFI_STATUS 高位编码）→ 建根开分页 → 原地补 runtime 字段 → `x0 = arena` 跳转。
- **接收端**：kernel 侧 `arch_boot_resume_v1(arena_pa)`（aarch64）：identity 读 arena → `decode_envelope` → 五块分区解码进 T5 落地区（memmap/reserved/modules+名池/platform/param）→ 构造 `KernelInfo` → `validate` → `set_current_root_phys` → `establish_boot_dm` → `jump_to_kmain`；`kernel-image/src/main.rs` 的 `x0` 非空支路改为传 arena（类型从 `*const BootHandoff` 改 `usize`）。
- **删除项**：`bootstrap_to_kernel_image` 的 blob 页分配与 `BootHandoff` 写入在切根落地后清理；01 篇 §3.4/§3.7/§4.7/§4.8 的 blob 叙述同批改写。
- **验证**：`test-shim-bootmarks-aarch64.sh` 先验 shim 两路标；`test-cmd-smoke-aarch64.sh` 全链；`run_all` 39 段不回归（x86 路径未动）。
- **风险**：raw EBS 与固件 ABI（aarch64 上 `efiapi == C`）；arena 必须落在 bootstrap 恒等窗口内；map buffer 的分配必须在 EBS 前完成，循环内零分配。

**2026-10-30 实施完成（01TODO-T1CUT）**：上述载体、发送顺序、接收端与删除项全部落地；实现偏离一处——`uefi` 0.33 的 `memory_map_size` 是 crate 私有，map buffer 改为 raw `GetMemoryMap` 空缓冲探测加余量、循环内按 `BUFFER_TOO_SMALL` 扩容；`uefi-raw 0.9` 作为 boot-shim 直接依赖引入（与 `uefi` 同一版本，`Cargo.lock` 未变）。门读数：minix-boot 57/0、boot-shim test-all 53/0 加边界审计 5/0、kernel mock 845/0/8、三架构 kernel/kernel-image check 0 error、`test-shim-bootmarks-aarch64` PASS、`run_all` 39/0/0；`test-cmd-smoke-aarch64` 到达 `/etc/rc`，`du`/`ls` 目录读腿在 `f03aa8ffc` 基线同样失败（既有 A 板问题，非本批回归）。

### 2.3 批次三 —— T5（先于 T3）

**D-70**（裁决：01-code-design §5，采案 1）：`globals.rs` 增 `PLATFORM_SOURCE_STORE`（4 条，**容量 4 = 已核实真实上限 2 × 有意余量**）与 `PARAM_BUF_STORE`（1024，清零再复制语义）；`store_kernel_info` 五块切片全落地；验收 = `test_store_kernel_info_lands_firmware_slices` 扩断言 + 超容量 fail-fast 测试（兼 TODO-03 第五块）。**状态**：✅ 01TODO-T5。两 pad 已在 `globals.rs`（platform 4 / param 1024，前者补 `BklProtected` 审计条目）；`store_kernel_info` 五块全部重指向；落地区测试扩平台源/param（含幂等再存指针稳定），超容量负控 5 枚；01 篇 §4.5 三块→五块、M8/TODO-03 行、§5.2 计数 4→6 同批；kernel mock 845/0/8、三架构生产 check 0 error。

### 2.4 批次四 —— T3（D-66）

- UEFI 腿读当前镜像 `EFI_LOADED_IMAGE_PROTOCOL` 的 `LoadOptions`（非 `Boot####` 枚举）；规范化四步统一规则；**ARCH 有可靠来源（编译目标）则生成、BOARD 有真实值才生成、不能确认显式省略**（v5 废除占位；结论范围 = 未发现仓库内消费者）；截断按整条记录丢弃；SBI/直核腿来源 = `/chosen/bootargs`，验收须实测本仓 QEMU 链 `-append` → DTB 节点通路（或诚实结论 = 该腿长期空切片）。
- 验收：`GET_MONPARAMS` 与 C 同形（1024 定长零填充）；01 篇 §3.4 param_buf 表述同步。

**2026-10-30 实施完成（01TODO-T3PARAM）**：`minix-boot` 新增 `boot_params.rs`（`normalize_param_buf` 唯一规则：NUL/空白分词、可打印 ASCII、内部键拒绝、重复键首条胜出、整条记录截断）；`build_kernel_info` 增第 10 个 `param_buf` 形参；UEFI 腿读 `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions`（非 UCS-2/二进制不猜，逐条拒绝），OpenSBI 与两个直核面读 DTB `/chosen/bootargs`（`all_nodes` 走查，避开 `find_node` 的 SD-38 缺陷）。`KernelInfo` 与内核零改动。读数：minix-boot 65/0、boot-shim test-all 53/0、kernel mock 845/0/8、三架构 kernel/kernel-image check 0 error；aarch64 直核注入 `/chosen/bootargs "ARCH=probe BOARD=qemu FOO=bar"` 后串口 `param_buf bytes=0x15 first=ARCH=aarch64`（ARCH 13 字节 + `FOO=bar` 8 字节；BOARD 无来源按规则省略，外部同名键被拒）；UEFI 腿 `param_buf bytes=13 rejected=0 dropped=0`（AAVMF 启动管理器未传 LoadOptions，生成 ARCH 一条）。riscv 直核面复用同一 `bootface.rs` 代码，其现有测试夹具的模块表为空、走不到 `build_kernel_info`，该腿的端到端读数留待 D-73 shim 腿。

### 2.5 批次五 —— T2（I-1/NK1/OQ-N6：x86 切独立镜像）

- OQ-N6 已裁（每进程独立根原语）；本批 = NK1（C-29）实施收口。全部入口契约已冻结：trampoline 状态清单（镜像内 bootstrap 栈起步、**禁止验证前读 `kern_stack_top`**、显式 `cli`/`cld`、16B 对齐、`rbx` 持 blob 指针跨 `.bss` 清零）；red-zone **无需任何旗标**（`x86_64-unknown-none`/`x86_64-unknown-uefi` 规格 `disable-redzone: true`，round-5 B 批实测）；`load_elf_into` 前提归边界成文（不改签名不加 trait）；`reserved_regions` 按消费语义拆分输入（`dm_coverage` 保留、`vm_handoff` kern_ident 输入删除）；模块 blob 分配改 `MaxAddress(bootstrap_alloc_limit)` + 区间末端验证；QEMU 先行短期双轨后翻产线删旧路径。
- 联动：01 篇 §3.7 三形态表 x86 行、§4.8、L1422/L2624/L1424 张力块（同批，见 §3）、`os/kernel-image/src/main.rs` 头注「交付边界」段更新。

**2026-10-30 实施完成（01TODO-T2X86）**：`prepare_and_enter_kernel_image` 与 `enter_kernel_image_v1`/`arch_boot_resume_v1` 的 cfg 扩到 `any(aarch64, x86_64)`；x86 的最终跳转走 `__minix_x86_enter_image` 全局汇编（`cli`/`cld`、`rbx=arena`、`jmp entry`），镜像 `_start` 建栈、清 `.bss`（只碰 rdi/rax/rcx）、`and rsp,-16`、`mov rdi,rbx`。**实现中发现并修复一处 ABI 真 bug**：`extern "C"` 在 `x86_64-unknown-uefi` 上是 Win64 约定（rcx/rdx），与 stub 的 SysV 取值不符，曾把 `KernelInfo` 指针当入口地址跳进栈；改为 `extern "sysv64"` 后 x86 门稳定 PASS（连跑三次）。`reserved_regions` 的 kern_ident 输入已从 `vm_handoff::build_identity_windows` 删除（只剩架构 MMIO 恒等窗口），`dm_coverage` 源保留；`alloc_module_pages` 改 `MaxAddress(bootstrap_alloc_limit(size))` 并验证区间末端；`load_elf_into` 的恒等可读前提成文。新增 `test-shim-bootmarks-x86.sh`（五路标：三 shim 行 + 接收端行 + `entering scheduler`），`test-cmd-smoke.sh` 的 M14 交接行改为 `[raw] entering kernel image v1`。门读数：kernel mock 845/0/8、minix-boot 65/0、boot-shim 53/0 + 边界审计 5/0、三架构 kernel/kernel-image check 0 error、x86 冒烟门 PASS（三次）、aarch64 冒烟门 PASS、`test-cmd-smoke.sh` 到达 `/etc/rc`（仅 `du` 既有 A 板红）、`run_all` 39/0/0。

### 2.6 独立线 —— D-59（`boot-dm-hugepage-design.md`，不进 01-code-design）

- 设计文档已定稿（2,249 行；五轮评审收尾；lint 0 error）；**待实施**，迁移五阶段（先启动选页 → `PageSize`/能力 API → 事实/规划拆分与策略 → `map_huge`/SMP/预检 → 机械重命名）。
- 开放问题（§10）现状：大多已决（`BootDmInstaller` 封口、物理上界路线 A + 统一取小、MMU 模式变更时重算）；剩小项 = `test-support` feature 打包方式、`DmPagePolicy` 构造边界、`KernelLayout::from_info` 取舍、`supports_huge_page` 命名——随实施定。
- **未关闭验收门（独立立项，不随迁移闭合）**：候选区间整段丢弃的覆盖损失（reserved 与候选部分重叠时非重叠尾部失去 kernel-DM 叶子）——按优先级区间差集归一化另立专项 + 真机证明；本项闭合前集合级不变量不承诺「所有有资格映射的字节都被保留」。
- 与 01 篇的交点：TODO-05（2 MiB 回退档真机启动）随本条实施 + NK1 一并验。

### 2.7 D-72（新登记）——bump 池耗尽的结构债：1024 页是 runway，不是终态
> **裁决（八轮评审，通过候选 A）**：映射责任移交 VM；所有权三分——`KERNINFO_PAGE` 归内核（长期保留、不得被 VM 当普通帧回收）、每个地址空间的 `KERNINFO_USER_VA` 映射归 VM（建根建立、随页表销毁）、中间页表页归 VM 的页表分配器；`VmBootHandoff` 加字段并**升载荷版本**（VM 拒绝不支持版本，不得静默漏映射）；`fork`/`exec` 分别验收；4 MiB 启动池预算在候选 A 后按实测峰值复核。**仓库侧前置已核实**：`init_kerninfo` 在 Phase B.5、`build_vm_handoff` 在 Phase C——加字段无需倒置初始化顺序；`fork`（`vm/src/fork.rs:do_fork` 的 `pt_new` + `fork_regions`）与 `exec`（`vm_server.rs:exec_bootproc`）都新建根、不继承映射，实现点应放在 VM「根创建」统一函数。详见 `01-design-note-riscv-unify-d72.md` §5.2/§5.4。

**现状（代码注释自陈，属 01 篇射程：`prepare_boot(N)` 的预算正是 §4.1 讲的那个池）**：

- 预算构成：boot 期约 100 页（恒等/内核映射、DM 覆盖、引导身份窗口逐页拆解、交接链），其余余量给**每次新根绑定（exec 重建根）的 kerninfo 注入链**留 1–3 页/次。证据：`os/boot-shim/src/main.rs:fn main` 的 `prepare_boot(1024)` 上方注释（NK4-C 1.55 B30 段）。
- **池无回收路径**：`BootAlloc` 是单调 bump（`alloc` 只前进、失败回滚一格），页表页从不归还；真机（bn9m）在 **~40 次绑定后 `AllocationFailed` 死亡**——注释原文 "the pool has no free path … died AllocationFailed after ~40 binds"。
- **代价已量化且落在 VM 可分配总量上**：`vm_handoff` 按 `boot_alloc_region()` **整段**扣除（用掉的与剩下的都算内核保留），1024 页 = **4 MiB 每次启动从 VM 可选内存里永久扣掉**。
- 注释自陈不是修好了："Before the structural fix lands (**reclaimable page-table pages / moving the kerninfo mapping responsibility to VM per kerninfo.rs doc** — WORKLOG 1.55 follow-up) this pool only postpones the deterministic panic." `kerninfo.rs` 模块 doc 侧对应口径：本次重写不移植 `.usermapped` 段，`init_kerninfo` 自建一页并在**每个新根**里复刻该映射（即注入链的来源）。
- **未登记核实**：`todo.md` 与旧 `01-todo.md` 全量检索 `reclaimable|kerninfo-injection|bn9m|1.55` 只命中 I-25 的锚点样例——本债此前**没有条目**；01 篇 §4.1 只描述了预算构成，未写耗尽故障与结构修复缺口。

**待裁决（两候选，实施前需一次小裁决）**：

1. **可回收页表页**：给引导期页表池加回收路径（C 的 `pt_free` 语义在 boot 侧的对应物），或改用 VM 的页表池承担新根建表；
2. **kerninfo 映射责任移交 VM**：内核只发布 kerninfo 页的物理位置，由 VM 在建每个进程根时把该页映射进去（与 C 的 `.usermapped` 段语义更近；`kerninfo.rs` doc 已把这个方向写成"待结构修复"）。

**登记理由**：这是 01 篇射程内唯一的"已跑通过的临时方案被当作终态"的结构债——不影响当前启动，但在 exec 密集负载下是确定性上限（~40 次绑定）。**验收方向**：设计裁决落文（01-code-design 增补一节或独立小设计）→ 实施 → 真机验证"exec 重建根 N 次（N ≫ 40）不耗尽"或"bump 池不再承担每绑定开销"。

**证据命令**：

```
sed -n '38,62p' os/boot-shim/src/main.rs          # 预算构成与 bn9m 读数
grep -n "init_kerninfo\|new root\|replicating" os/kernel/src/kerninfo.rs | head
grep -n "boot_alloc_region" os/kernel/src/vm_handoff.rs | head -3
```

**2026-10-30 实施完成（01TODO-D72VM，候选 A）**：`VmBootHandoff` 升到 v7 并加 `kerninfo_pa`（非零/页对齐校验；v6 及以下拒收）；`minix_types` 增 `KERNINFO_USER_VA` 单一权威，内核 `kerninfo.rs` 改为再导出；`kernel/vm_handoff.rs` 在 Phase C 填 `kerninfo_page_phys()`；`kernel/syscall.rs` 的 `vmctl_set_addr_space` 每根注入块删除（源码序钉 `os/kernel/tests/set_addr_space_no_boot_alloc_pin.rs` 2/0 钉住"不再出现 `boot_pt_alloc`/`kerninfo_page_phys`/`KERNINFO_USER_VA`"）；VM 侧 `BootParams` 加 `kerninfo_pa`、`init_page_table` 在 `map_kernel` 之后把该页只读映射到 `KERNINFO_USER_VA`（失败走既有 `free_page_table` 回滚）。`fork` 与 `exec` 都经 `init_page_table` 新建根，一处覆盖两条路径。门读数：minix-types 331/0、minix-vm 543/0、kernel mock 845/0/8、三架构 check 0 error、`run_all` 39/0/0（含 rt-birth 的 `kerninfo=ready` 查询）。4 MiB 预算按实测复核：映射不再从启动池取页，池只承担启动期本身，1024 页维持不变（净值见 `kernel_allocated_dynamic` 交接字段）。

### 2.8 D-73（新登记）——riscv64 独立 boot-shim 腿：三架构统一为主线

> **裁决（八轮评审，有条件通过）**：方向通过；先闭合 **入口契约闭合门（输入契约）**再接线——① U-Boot 版本与 `bootelf` 支持面（官方文档：`bootelf [-p|-s] [-d <fdt_addr>] [<image_addr> [<arg>]...]`，`image_addr` 之后的参数以 `argc/argv` 交给应用，`-d` 按地址设置设备树）；② shim 入口适配层解析 argc/argv（设备树 + 启动清单）并校验，不依赖 OpenSBI 的 `a0/a1` 语义；③ kernel.elf 与 12 模块由谁装载（沿用 U-Boot 预载 + `BootFileTable` 契约，或 shim 自读）与地址/长度/内存范围进契约；④ 页池/设备树/镜像/模块/交接载荷互不重叠（`MODULE_REGION_BASE` 等 QEMU 常量改为 DTB `/memory` 推导或构建期注入）；⑤ 两内核产物入口契约（`kernel-direct.elf` 直核：`a0`=hartid、`a1`=DTB；`kernel-shim.elf`：只收交接载荷），且两入口适配完成后必须进入同一套公共内核启动语义流程（汇合点验收）；⑥ `-d`/参数传递依赖 U-Boot 构建配置（`CONFIG_CMD_ELF_FDT_SETUP` 一类），实际版本须实测核实；⑦ 入口 ABI / 固件服务 / 文件与启动信息来源三个维度分开表达，不固化成同一个"固件实现"选择开关。**分道 = 两个构建产物**（不做运行时分道猜测）。直核自引导面称「辅助启动路径」，保留并继续验收；QEMU 通过 ≠ 真板契约成立。rustc 现无 riscv64 UEFI 目标（`*-unknown-uefi` 列表无 riscv64），UEFI-on-riscv 只作未来扩展。详见 `01-design-note-riscv-unify-d72.md` §五。

**方向（提出方立场，已获有条件通过）**：三架构共用同一条主线——**独立 boot-shim 可执行体 + 独立内核镜像 + 交接载荷**。boot-shim 的职责是为内核屏蔽固件形态（UEFI、U-Boot/OpenSBI 或将来其它）；riscv64 也因此收编进主线，自引导面（`bootface`/`bootface_a64`）**降级为特定嵌入式形态**（riscv 的 `FW_PAYLOAD`、QEMU `-kernel` 测试通道），保留但不作主产线。依据：`BootShim` trait 本就与固件无关（`UefiBootShim` / `OpenSbiBootShim` 两个实现产同一 `KernelInfo`）；riscv 的 shim 腿主体（`OpenSbiBootShim`/`UbootFileLoader`/`BootFileTable`/`bump_alloc_pages`）已在仓内，只是**零接线**（bin 只按 `fw-uefi-image` 构建，`OpenSbiBootShim::prepare_boot` 无调用方）——当前"riscv 产线走自引导面"是历史缺线，不是架构结论。

**已知缺口（实施前要解决）**：

1. **bin 与 feature 词表**：`os/boot-shim/Cargo.toml` 需增 riscv64 bin 词表（如 `fw-riscv-opensbi`），`main.rs` 按 feature 选固件实现；目标 triple `riscv64gc-unknown-none-elf`。
2. **入口参数通道**：U-Boot `bootelf` 不按 OpenSBI 的 `a1` 传设备树（现有 `install_dtb_ptr(a1)` 与 `rust_image_main(a0/a1/a2)` 约定只对 OpenSBI 直载成立）——需裁决取树通道（候选：`go` 带参 / 约定内存槽 / 扫 FDT 魔数 / 构建期注入）。
3. **交接泛化**：`bootstrap_to_kernel_image` 与 `arch_boot_resume_high_half` 当前 `#[cfg(target_arch = "aarch64")]`，需放宽到 riscv64（架构差异仅跳转指令 `fence.i`+`jr` 与寄存器；`BootHandoff` 类型本身架构无关）。
4. **内核镜像两形分道**：直核（`bootface`）与 shim 交接共存；aarch64 的「`x0==0` 走自引导」判据在 riscv 不成立（直核 `a0` 是 hartid）——需新判据（候选：两个独立 ELF 入口符号 / 魔数载荷 / 专用寄存器约定）。
5. **池基去 QEMU 化**：`opensbi_helpers.rs` 的 `MODULE_REGION_BASE = 0x8000_0000 + 32 MiB` 与 `BUMP_END` 编译期断言是 QEMU 布局硬编码，真板不可用——改为从设备树 `/memory` 推导或构建期注入 + 运行期检查。

**裁决载体**：`rewrite-notes/01-stage-kernel/01-design-note-riscv-unify-d72.md`（完全自包含，含议题二 D-72；外部评审已完成并冻结，裁决见其 §五）。裁决回填本条目与 `01-code-design.md`（T2 的「riscv64 直核腿不需要 shim 侧跳转」表述随裁决更新）。

**验收方向**：QEMU 上 U-Boot `fatload` 载 `boot-shim.elf` → `bootelf` → shim 读 12 模块 → 建表 → 跳 `kernel.elf` 高半 → 内核首个路标；`-kernel` 直核形态不回归；真板窗口另计（`FW_PAYLOAD` 形态保持可用）。

**入口契约实测结论（D-73，GPT 裁决核定后的更正版）**：早前的「bootelf 无 argv」结论**已被探针证伪**——`help` 的 usage 文本只写 `[address]` 不足以证明参数通道不存在。本机 U-Boot 2025.10（`/usr/lib/u-boot/qemu-riscv64_smode/uboot.elf`，OpenSBI 之下 S-mode）用最小 RISC-V 探针（`_start(argc, argv, a2)`，SBI console 打印）实测：**`bootelf -p <img> [args…]` 会把 image 地址之后的参数以 `(extra_argc, extra_argv)` 交给应用入口**——`bootelf -p $loadaddr` → `argc=0`；加 `87654321` → `argc=1, argv[0]="87654321"`；加 `aaaa bbbb` → `argc=2, argv[0]="aaaa", argv[1]="bbbb"`；a2 是 U-Boot 堆内指针（随调用变化），**不是 FDT**。两个此前没看见的门：① `autostart` 环境变量默认未定义时 bootelf 只装载不执行（必须 `setenv autostart yes`）；② 不带 `-p` 时本 build 的 extra-arg 形状未通过（`bootelf $loadaddr deadbeef` 报 `## No elf image at address 0x80200000`），契约应钉在 `-p` 上。`-d` 未在 help 列出且本设计不需要它（DTB 地址进 BootFileTable，由 shim 自己读）。

早前 hello-boot 的 `bootelf` `Load access fault`（EPC `0x8f73d74e`）的真因也修正为：**staging == LMA 自重叠**——默认 `loadaddr=0x80200000` 既是 fatload 落点又是 hello-boot 的 PT_LOAD 目标，bootelf 边读边覆盖装载头。探针把 LMA 放到 0x84000000、staging 留在 0x80200000 后两次全链干净执行。`bdinfo` 布局证据（256M 配置）：DRAM `0x80000000-0x8fffffff`；lmb reserved = OpenSBI `0x80000000-0x8005ffff` + `0x8deaa000-0x8deacfff` + U-Boot 区 `0x8dead8d0-0x8fffffff`（relocaddr `0x8f6cf000`、malloc `0x8eeaf000-0x8f6cf000`、fdt_blob `0x8eead8e0`）；可用带 = `0x80060000-0x8dea9fff`，默认 staging `loadaddr=0x80200000`、boot.scr 执行拷贝 `0x8c100000`。

**去向（按裁决更新为首选路线）**：保留独立 BootShim 方向；下一步是物理地址链接的 `riscv64-uboot-shim.elf` 探针→产物（VMA=LMA=物理入口、自带 bootstrap 栈），入口 ABI=U-Boot `bootelf -p <img> <BootFileTable 地址>` 的 `(extra_argc, extra_argv)`，`argv[0]` 传 MNXBOOT1 表地址；DTB/kernel/12 模块由 U-Boot 预载、表承载；**不需要新增固定 handoff 槽**（固定槽仅作板级备用 ABI）。LMA 安全带从 bdinfo/lmb 推导并由 D-73 专属布局配置生成，不硬编码 `0x90000000`。`BootHandoffV1` 冻结协议不动；U-Boot 的入口问题由 shim 适配层消化。完整探针证据：`/tmp/d73/{probe.elf,argv-base2.txt,argv-plus.txt,argv-matrix.txt}` 与本段命令。

**2026-10-31 实施完成（D-73 全链 PASS）**：新增 `os/boot-shim/src/uboot_main.rs` + `riscv64-uboot.ld`（物理链接 VMA=LMA=0x84500000，`.text.boot` 入口自清 SIE、自清 .bss 含 `.lbss`、自立 64KB bootstrap 栈）+ `build.rs`（特性门挂链接脚本）；`RiscvUbootBootShim`（原 `OpenSbiBootShim` 更名）经 `prepare_boot` 用 MNXBOOT1 表读文件——表由 shim 在自身 .bss 用 `uboot_layout` 常量自建（kernel 0x86000000/2MB、模块 0x87000000+1MB×12、DTB 0x8c000000），boot.scr 只 fatload 文件，绕开预建表的 staging 共变问题；arena 走 v1 序列化后 `enter_kernel_image_v1` 跳接收镜像。`kernel-image` 增 `fw-riscv64-uboot` 特性：`rust_image_main(arena_pa)` → `arch_boot_resume_v1`（riscv64 臂与 x86_64 同构）。实施期三处修正：① `Riscv64HigherHalf::jump_to_kmain` 加 `auipc` 执行位置探针——接收腿已被跳到高半链接地址，medany 的 PC 相对 `la` 已解析真值，链接↔装载偏移不得再加（故障读数：EPC=0xffffff7f7fe0477a = kmain 链接址 0xffffffc00000477a + delta 0xffffffbf7ffe0000，算术精确闭合）；② shim 的 `DtbMemoryMap::new` 第二参从 0 改为 `KERN_PHYS_BASE=0x80200000`（bootface 同源语义）——此前 `[0x80000000,0x80200000)` OpenSBI 足迹漏进 VM 空闲表，VM 把它选作 PMM 元数据供体，向 DM(0x80000000) 写入页 fault（stval=0x0000001080000000 = DM 基 64GiB + PA 0x80000000），而内核 DM 计划正确地拒绝映射固件区间；③ 门脚本 Stage 1 的 `set +x` 曾把构建失败掩盖成旧二进制续跑（`|| fail` 检查到的是 `set +x` 的退出码），改 `set -e -x`。门读数：`test-riscv64-uboot-shim.sh` PASS（6/6 路标：shim enter/表建/模块装载/进 v1/接收 resume/entering scheduler；空闲表修正后 n=2 无固件区间）；直核腿 `test-riscv64-boot-full.sh` PASS（rc marker）；docker kernel mock 850/0/8；boot-shim test-all 58/0；三架构 kernel check 0 error。用户腿取证诊断扩展（`ura/usp/ua0/ua2` 进 riscv64 用户腿 trap 面板）随本批保留。
---

## 3. 直修批（无设计依赖，可立即可做）

### I-22 收尾回勾（代码已落地，三件尾巴）

1. `todo.md` §7.1 的 I-22 条目改记「代码已落地（丁案 A–F 除 aarch64 直核腿外），遗留 D-68 与文档连带」；
2. **文档连带 ①–⑦ 逐一核对**（todo.md I-22 条目所列：01 篇 L1265/L1945/L2288 三表、opensbi_helpers 顶注（→I-26）、bootface 头注、§3.7/§4.8 合流句、04 篇平台发现分工段）——未见专查记录，回勾时逐项过；
3. D-68 之外的连带（名字池/常量合并）不在本条射程。

**状态**：✅ 01TODO-B2——①–⑦ 逐项核过：① §3.1 L1265 表、② L1961「内存与参数准备」行、③ L2288 `memmap` 行、④ opensbi 顶注（I-26 已修）、⑤ bootface 头注（本批修正对已删 `build_memmap` 的指涉）、⑥ §3.7/§4.8 合流句、⑦ 04 篇 §3.1 平台发现分工段，全部已按新口径在位；`grep -cE '硬编码的 QEMU virt DRAM 布局|单条 DRAM'` → 0。第 1 件（`todo.md` §7.1 改记）已在本批台账行同步；遗留仅 D-68（T4 批）。

### I-28（本轮新登记）`MemoryMapSource` trait 零多态载荷：删除与否待设计专项裁决

**现状**：`os/libs/minix-boot/src/memmap_source.rs` 的 [`trait MemoryMapSource`](os/libs/minix-boot/src/memmap_source.rs:trait MemoryMapSource) 全仓**无任何多态使用**——既无泛型约束也无 `dyn`；四个调用点写 `use MemoryMapSource as _` 或 `<DtbMemoryMap as MemoryMapSource>::collect`，只为把方法引入作用域，改成固有方法后这四处可原样删。三个 impl 的活死状态：`DtbMemoryMap` 活跃（riscv 直核面 [`fn bootface`](os/kernel-image/src/bootface.rs:fn bootface)、aarch64 直核面 [`fn bootface_a64`](os/kernel-image/src/bootface_a64.rs:fn bootface_a64)、OpenSBI/U-Boot shim [`impl RiscvUbootBootShim`](os/boot-shim/src/opensbi_helpers.rs:impl RiscvUbootBootShim) 三处产出内存图；aarch64 UEFI 生产入口 [`fn prepare_and_enter_kernel_image`](os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image) 另有一处调用，用途是 DTB 越界校验、不产出内存图）；`UefiMemoryMap` 的唯一消费者是 [`impl BootShim for UefiBootShim`](os/boot-shim/src/uefi_helpers.rs:impl BootShim for UefiBootShim) 的 `prepare_boot`，而该函数**全仓零调用方**；`MultibootMemoryMap` 返回空表，零引用。注意 `build_memmaps`/`build_memmap` 不能删——14 个 QEMU 测试内核经 `build_memmap()` 调用它，死的是包在外面的那层 `UefiMemoryMap` 壳。

**为什么算问题而不只是"没用到"**（v2 按评审修正表述）：这条 trait 确实提供**静态接口约束**（`impl` 的方法与签名受编译器检查）与**静态分发**（两处全限定调用），但**没有任何消费者依赖它所提供的抽象边界**——调用方都知道自己是哪个具体类型，抽象既没降低调用方的类型依赖，也没统一它宣称要统一的两侧。UEFI 生产链接不进来不是惰性，是签名表达力不足——分类必须与 `ExitBootServices` 共用同一个 MapKey（[`fn pair_and_classify`](os/boot-shim/src/ebs_pair.rs:fn pair_and_classify) 把两者显式配对）、产物必须直接写进 arena 分区（[`fn classify_map`](os/boot-shim/src/ebs_map.rs:fn classify_map) 的 `serializer.push_memmap`）、失败面是 `BUFFER_TOO_SMALL` 与 `EFI_INVALID_PARAMETER` 重试。`fn collect(&self) -> (两张表)` 这三样都装不下。于是这条 trait 的多态维度实际永远只有 DTB 一支 + 一个死实现 + 一个空占位；留着会让人以为接新来源只需 impl 一下。

**待裁决（用户已开设计专项；第三方评审 v1 已给方向：删除）**：方向 = 删除 trait，`collect` 降为 `DtbMemoryMap` 的固有方法，删 `MultibootMemoryMap`，随 A1 去留一并处理 `UefiMemoryMap`，把「可用表 / 保留表」语义搬进 `KernelInfo` 字段文档（或共享结果类型），并改写模块头注不再宣称「唯一出口」。**必须走正式 supersede，不得伪装成顺手清理**：记录需答六问（当初为何引入 / 假设的消费者与实现 / 当前实际能力 / 哪些假设未兑现 / 本次为何撤销 / 语义文档去向）。重新引入的判据：**第二种真实来源出现且存在真实消费者依赖共同语义时**，再按真实差异设计。保留方案 B（只改注释）与 C（真正接线 UEFI）已由评审否决，理由分别见文档 §1.4 与 §1.2。

**验收（裁决后）**：`grep -rn "MemoryMapSource" os --include="*.rs"` 结果与裁决方向一致（删除则只余零命中）；三架构 `cargo build` 与 `os/qemu-tests/run_all.sh` 回归；代码侧不留"统一出口"这类与事实不符的注释表述。

### I-29（本轮新登记）UEFI 内存图分类双份实现：`classify_map` 与 `build_memmaps`

**现状**：UEFI 侧有两份功能相同的分流实现——生产路径的 [`fn classify_map`](os/boot-shim/src/ebs_map.rs:fn classify_map)（经 [`fn pair_and_classify`](os/boot-shim/src/ebs_pair.rs:fn pair_and_classify) 调用）与 [`fn build_memmaps`](os/boot-shim/src/uefi_helpers.rs:fn build_memmaps)（只被辅助入口 A1 与 14 个 QEMU 测试内核经单数形式 `build_memmap()` 调用）。三条分流规则逐条相同（`CONVENTIONAL` 进可用表；`MMIO`/端口空间丢弃；其余且起点低于 RAM 顶进保留表），**校验强度与一致性保证不同**：`classify_map` 有版本/尺寸/对齐/边界/零页数/溢出/物理上限七类自建校验与“同一次读图取键退出”的结构配对；`build_memmaps` 无自建校验（解析交给第三方 `uefi` 库）、不判 `PA_LIMIT`（该常量就在同文件相邻处却只被生产路径使用）、零页数会产出长度 0 的区间，且分类快照与退出是先后两次读图，一致性靠“其间不分配”的顺序纪律维持。

**测试面的后果（v3 已按第二轮评审重写）**：见下方 v3 补记。要点是：问题不止「两份实现重复」，更在于**测试路径拿到的那张内存图不反映快照之后的分配**。

**v3 核心发现（三类快照后分配）**：① `build_memmaps` 的 `Vec` 与两次 `Box::leak` 发生在它取到快照之后；② 12 个夹具内核在 `build_memmap()` 之后才 `alloc_root_page`/`alloc_bump_region`；③ `uefi` 0.33.0 的 `exit_boot_services` 内部先 `allocate_pool` 建缓冲、再重读图取键。三类都不会反映在分类结果里。最危险的是 ①：内核拿到的 `memmap` 切片本身住在被误报为可用的内存里，一旦 PMM 开始分配就可能覆盖内存图自身。生产路径没有此问题——读图与退出之间**零分配**（分类只往 arena 写），且 `assert_bootstrap_outside_memmap` 三次守护。v2 的修复「先分配后快照再断言」只覆盖 ②，不够。

**断言分工（修正 v2 §2.5/§2.6 的内部矛盾）**：12 个夹具内核**不验证完整分类语义，但必须验证本测试自举占用不进入可用表**；完整分类规则由专门测试负责；宿主单测覆盖非法输入；真实 UEFI 路径新增输出内容断言。

**v3 候选（两条完整修复路径）**：路径 A（推荐先研究）——保留 `exit_boot_services` 返回的 `MemoryMapOwned`（当前被 `let _mmap =` 丢弃），退出后用它分类并写入预分配存储，天然满足「分类图 = 退出图」；路径 B——测试走生产式配对事务（学得到重试路径，但接线重）。分工建议：夹具内核用 A，验证配对重试另立专门测试。

**② 段现状更正**：v2 写「当前已唯一」不准确——生产路径只有一份项目自有的原始字节解析器，**测试路径尚未复用它**（用第三方库解析结果）。两个目标：一=只统一 ③（承认测试不验证原始解析器），二=连解析一起统一（优先考察）。

**输出接缝细化**：输入必须是**可重复遍历的视图**（分类扫两遍算 RAM 顶），不能是一次性迭代器、不能分配中间数组；输出在回调与私有 sink 之间比较（不优先返回两个大数组）；容量 128/256 属交接协议契约，测试不得复制一份。

**v4 核心发现（`ram_top` 边界 + 类型生命周期 + 反向偏差）**：

- **I-29-7 `ram_top` 边界（最高优先）**：第二遍分类判据是 `desc.physical_start < ram_top`（严格小于）。自举分配用 `AllocateMaxAddress(bootstrap_alloc_limit(..))`，只规定上界不规定取位取向，因此根页表页/bump 池/arena 很可能落在**剩余常规区间的最高端**，其起点**等于** `ram_top` → 该描述符**两张表都不进**。`assert_bootstrap_outside_memmap` 只查可用表，抓不到。**核实结论：当前未见功能性故障**——`dm_coverage` 的 source 2（`bootstrap_tree_candidates`）用交接携带的根页表地址与 `boot_alloc` 记录的 bump 区间单独登记，不依赖保留表；arena 与读图缓冲不被内核访问；被丢弃的条目不在可用表内也不会被分配出去。但「非常规低于 RAM 顶都进保留表」这条成文不变量为假，且 12 条宿主单测无「起点恰等于 `ram_top`」用例。三选项待裁决：`<=` / 显式承认不完备 / 改判定依据（与任何 RAM 区间相交或按 RAM 类型）。
- **I-29-8 类型生命周期政策**：当前把 `BOOT_SERVICES_CODE/DATA`、`ACPI_RECLAIM` 一并永久放保留表 = 永不复用，属保守政策需成文；`PERSISTENT`/`UNACCEPTABLE` 是「碰巧进保留表」，无显式论证。
- **I-29-9 反向偏差**：`boot::memory_map` 返回的 `MemoryMapOwned` 在 `build_memmaps` 内是局部变量，函数返回时 `Drop` 在引导服务仍存活的前提下 `free_pool` **释放**了那块缓冲，而快照里它仍是 `LOADER_DATA` → 保留表**多报**一块实际已可用的内存。生产路径的读图缓冲用裸 `allocate_pool` 且从不释放，不受此影响。

**路径 A 前提已核实（v4）**：`uefi` 0.33.0 的 `trait MemoryMap` 公开 `meta()` / `buffer()` / `entries()`，`MemoryMapOwned` 实现该 trait → 退出后可把原始字节交给生产解析器（**目标二可行**）；注意 `meta().map_size` 是有效长度、`buffer().len()` 含冗余条目，不可混用。`MemoryMapBackingMemory::Drop` 先判 `are_boot_services_active()`，退出后不释放。补失败契约：退出后分类出错走不依赖引导服务与动态分配的 fail-stop（裸串口 + 停机）。

**`0.33` 版本下的 `Drop` 与实现位置**：`MemoryMapBackingMemory` 在 `src/mem/memory_map/impl_.rs`，`MemoryMapOwned` 同文件；`boot::memory_map` 在 `src/boot.rs` 返回 `MemoryMapOwned`。

**v5 核心发现（类型/属性/生命周期政策 + 规范与实现的区分 + 对象–页级区分）**：

- **I-29-8 扩充：UEFI 类型必须分类，不能靠地址**。分类器只显式认识类型 7/11/12，其余全部走「低于 RAM 顶即进保留表」。按 `uefi-raw` 0.9.0 的枚举逐项对照，**三处语义冲突**：`RESERVED`(0) 与 `UNUSABLE`(8) 规范说「不可用」却进「可访问」的保留表；`UNACCEPTED`(15) 必须被接受后才能使用，把它当普通可访问内存是错的（机密计算平台上读取会直接故障）；≥16 的 OEM/OS-loader 区语义未知却被默认收进保留表。另 `PERSISTENT`(14)、`PAL_CODE`(13) 属「碰巧进保留表」，无显式论证。`BOOT_SERVICES_*`(3/4) 与 `ACPI_RECLAIM`(9) 永久不复用是**政策**，必须成文。采纳四轮评审的五类分组（立即可用/保留但可访问/延迟可回收/需特殊处理/不可用不应映射），原则：**不要用同一个 reserved 标签同时表达「不可分配」「需要 DM」「暂时不能回收」「根本不能访问」四种含义**。建议判据顺序改为「先类型策略、再地址过滤」。
- **I-29-8 新增：`Attribute` 字段完全未读**。`RawMemoryDescriptor` 只有 ty/physical_start/number_of_pages 三个字段（偏移 0/8/24），`Attribute`（偏移 32）从未访问；而直接映射两个窗口用固定页标志（`PRESENT|WRITABLE|GLOBAL` / `PRESENT|WRITABLE|USER`），**统一写回缓存，不区分设备内存**。对普通 RAM 无碍，对运行时服务数据/持久内存/未接受内存是空白。
- **I-29-7 收窄：`ram_top` 边界拆成三命题**。命题一（规范可能性：`AllocateMaxAddress` 只约束区间最高地址，不规定取向）无平台依赖；命题二（OVMF 实际落点）需真机样本；命题三（若出现起点等于 `ram_top` 则会被丢弃）已由读码确认。**不批准把 `<` 改成 `<=`**（四轮评审否决理由：只把边界上的所有非常规类型放进保留表，没证明它们是适合 DM 的 RAM）。边界用例应先补，固定当前行为。
- **`bootstrap_alloc_limit` 算术已核对**：三种规范读法（限 base / 限区间最高地址含 / 限末端不含）下传 `bound - size` 都**不越界**；差别只是读法 B/C 下少用约 `size` 字节。**代码安全，注释只证明了读法 A**——注释需改，非缺陷。
- **I-29-9 收窄：对象分配 ≠ 页级内存图变化**。`AllocatePool` 只在池扩容时才从常规内存取页、才产生新描述符；`FreePool` 也只归还池对象，整页回收取决于池的空闲块合并。因此 §2.5.1 的 ①③ 是**条件后果**（只有 ② root/bump 是必然后果），§2.5.8 从「已确认」降级为「待六步探针验证」。全局分配器 `UefiPoolAllocator` 每次 `alloc` 都走 `allocate_pool`、`dealloc` 为空实现。
- **路径 A 析构审查已闭合**：`MemoryMapBackingMemory::drop` 在退出分支仍调 `log::debug!`，但**两层保障**——① 本项目全仓未调用 `uefi::helpers::init()`，无 logger 注册；② `exit_boot_services` 首句 `helpers::exit()` → `logger::disable()` 把输出指针置空，`Logger::log()` 遇空指针直接返回。`boot-shim` 只启用 `uefi` 的 `alloc` 特性，无 `logger`/`log-debugcon`。

**v6 定稿修订（第五轮评审 + 本文自主判断）**：

- **I-28 可冻结**：删除 trait 与空占位、保留 `DtbMemoryMap` 与语义文档、正式 supersede I-22。实施顺序注意：`UefiMemoryMap` / `build_memmaps` 的删除范围跟随 I-29 结果，不作为 I-28 独立清理。
- **I-29-8 仍为主阻塞项（尚未收口）**：类型 → 访问策略的显式映射。v6 修正 v5 的一处事实错误并补三架构证据：① 类型数值区间分四段（`0..15` 标准 / `0x10..0x6FFFFFFF` 保留数值区**非 OEM** / `0x70000000..0x7FFFFFFF` OEM / `0x80000000..0xFFFFFFFF` OS loader——v5 的「≥16 = OEM/OS-loader」是事实错误）；② 三架构缓存属性机制各不相同：aarch64 由 `MAIR_EL1=0xFF` 的 Attr0 **显式**定 Normal WBWA；x86-64 的 `flags_to_pte` 不设 PCD/PWT 且全仓不编程 IA32_PAT，落回默认 PAT0=WB（依赖「固件未改写 PAT0」前提）；**riscv64 的 Sv39 PTE 没有缓存位**，缓存性由平台 PMA 决定——v5「统一 WB」在 RISC-V 上是假设不是结论，已改；③ `EFI_MEMORY_RUNTIME` 是独立于 MemoryType 的属性（用运行时服务就必须处理其映射要求）。采纳四状态处置（`Available`/`ReservedMapped`/`Excluded`/`Special`）+ 第一版政策表草稿（§2.5.7.1），`RESERVED`/`UNUSABLE`/`UNACCEPTED`/未知类型不得默认进普通保留表。
- **I-29-7 定性升级**：`ram_top` 是「起点过滤启发式」——只查起点，不证明描述符整体在 RAM 包络内（形态二：越界尾部整段收下不截断），也拦不住包络内的非 RAM 类型（形态三：包络内的 `RESERVED` 照样进保留表）。**暂不裁决，不批准 `<=`**；边界用例以 characterization test 固定当前行为，contract test 跟随最终政策表，勿让旧行为变成新契约。
- **`bootstrap_alloc_limit` 算术更正**：规范只有一种读法（分配区间最高地址 ≤ 输入值），v5 的「三种读法」不成立。按规范传 `bound - 1` 即足够；当前传 `bound - size` 更严格（差额 `size - 1` 字节），**可能造成不必要的分配失败**（v5 未写出的后果）。两个选项待裁决（改 `bound - 1` / 保留并写成显式保守策略）；无论选哪个，现有注释「保证 base 不超过 limit」的错误规范读法必须删。
- **路径 A 两处前提降级**：① `MemoryMapOwned` 缓冲——`allocate_pool` 无地址上界，`LOADER_DATA` 只证明资源归属，不证明落在自建页表覆盖内；退出后至分类期间的访问需核实（测试内核顺序上大概率成立，但那是顺序事实非结构保证）；分类完成后无需纳入内核 DM。② 静态输出数组——「出现在快照里」≠「内核消费期间可访问且不被 PMM 分配」；且镜像区间可能恰落 §2.5.6.1 形态一边界（不在可用表也不在保留表），「不在可用表」断言会通过但 DM 访问会缺映射。验收从五项增到**六项**（新增「输出生命周期」）；夹具自举检查扩到输出数组地址范围，不只查 root/bump。
- **析构审查维持闭合**（五轮评审认可两层保障）；实现约束措辞收紧：「EBS 成功返回后，分类期间允许持有并读取 `MemoryMapOwned`；其析构不得尝试调用 Boot Services；分类器、输出容器与 fail-stop 路径不得触发动态分配或依赖 Boot Services 的日志输出」。
- **v6 本文自主判断**（§2.5.7.2，超出评审意见的三条）：① `Excluded`/`Special` 处置必须可观测（计数 + 类型分布走返回值），否则重演「描述符静默消失」；② 类型→处置写成编译期穷尽 `match`（枚举 + 三个后备变体），新增标准类型变编译错误而非运行时漂移；③ `ram_top` 降级为「结果裁剪」而非「准入门槛」——先按类型得集合，再用 RAM 包络裁剪结果，三种形态各有归宿且不需新抽象。另补 §2.5.9 两处注释修正：分配器注释「固件整体回收」错误（退出后是 loader-owned 交给 OS，不自动变常规内存——该误解易被下游误用为「shim 占的内存会自动还回来」）；`build_memmaps` 注释把「内核先深拷」说成万无一失（只保证内容不被覆盖，不保证可用表干净）。
- **测试规范同步**：§2.6.4 的「保留表符合 RAM 顶规则」已删除，改为按政策表的六条验收（只有 Conventional 进可用表 / 只有明确允许的类型进保留表 / 排他类型不得进 / MMIO 走设备路径 / 区间契约 / 类型×属性组合符合政策）。

**研究顺序（v6 按五轮评审重排，§7）**：① 修文档规范事实（v6 已完成文档侧，代码注释待改）→ ② 定策略表（草稿 §2.5.7.1，待裁决）→ ③ 核三架构映射落实（部分已核，PAT/MTRR 初始化与 RISC-V PMA 假设待核）→ ④ QEMU+OVMF/AAVMF 运行样本（不称「真机」；四时点含 `Attribute`；探针用预分配输出避免自污染）+ 路径 A 六项验收实验 → ⑤ 最小接缝原型。**样本不能回答政策问题**（未知类型是否有资格进 DM、Runtime 描述符如何处理、三架构如何落实属性）——所以 ②③ 在 ④ 之前。bootmarks 路标合并不在阻塞项内。

**v7 定稿修订（第六轮评审；一条是本文撤回自己的提议）**：

- **v7 撤回 v6 的「`ram_top` 结果裁剪」提议（§2.5.7.3），定稿为从分类器中删除**。评审的反驳成立且关键在于：v6 没说清 RAM 包络用什么数据构造。两种取法都错——① 包络 = Conventional 并集：`AllocatePages` 本来就把页移出 Conventional，刚分配的根页表页 `[B,C)` 与包络交集**恰为空**，最需要保护的自举内存反而被裁掉；② 包络 = `[0, ram_top)`：不证明区间内每个地址是 RAM，且 `ram_top` 只反映**剩余可用**内存的末端，不代表系统 RAM 最高地址（高地址 RAM 可已被固件占用）。更深一层：UEFI 内存图本身就是更直接的分类来源（无硬件支持的地址无描述；不可访问区报 `RESERVED`），类型字段承接 `ram_top` 的保护职责（芯片组空洞 → `Excluded`），地址维度只剩 `PA_LIMIT`（架构能力检查）。
- **I-29-8 政策表升为 v7 讨论基线**：四状态补充**两个正交维度**（分配策略 × 映射策略）——`ReservedMapped` 不再同时表达「不可分配」与「需要普通 DM」；`RUNTIME_SERVICES_*` 定为 `Special`（当前不支持运行时服务调用，其内存不可分配但不因此自动进普通 DM）；`UNACCEPTED`/`PAL_CODE` 收紧为「**存在即拒绝启动**」（与 §1.5 批评空表占位同一原则：边界处大声失败好过深处静默出错）；未知类型 = 分类错误。
- **可观测性细化**：`Excluded` 允许统计排除（计数 + 字节 + `first_n_diagnostics` 固定容量数组：raw_type/physical_start/number_of_pages/attribute）；`Special` **不能只返回计数**（地址范围会丢，将来无法从统计数恢复）；未知类型直接触发分类错误。
- **穷尽 `match` 的前提修正**（v6 的写法不成立）：`_ =>` 通配会吞掉新增标准类型。四条件缺一不可：`KnownMemoryType` 枚举（0–15 各一变体）+ 从 `u32` 安全转换（不得直接转枚举）+ 分类函数**无通配**穷尽 `match` + 未知值单独携带原始值。另按评审用词：`0x10..0x6FFFFFFF` 称「未分配／保留数值范围」，不称「未来标准类型集合」。
- **`bootstrap_alloc_limit` 裁决选 A**：改 `bound - 1`（`bound` 已是安全上界，无需再损失约一个请求大小的空间）。四前提核对（bound>0 / size 页对齐非零 / 返回页对齐故规范约束落在最高字节 / bound 是半开末端而 UEFI `Memory` 含末字节——两者差一故传 `bound - 1`）。随附两条单测。
- **`AllocatePages` 措辞再修**：必然**改变内存图内容**，但不保证描述符数量增加（整段覆盖时固件可直接改写该条类型）；§2.5.1/§2.5.2 已同步。
- **三架构证据再收紧**：aarch64 基本闭合，留一项待查（所有 DM 叶子的 `AttrIndx` 是否都为 0）；x86-64 的「落回默认 PAT0=WB」不能写成运行时事实——boot-shim 运行在固件之后，固件可能改写过 PAT，且有效类型需与 MTRR 共同决定；riscv64 的「PTE 无缓存位」过于绝对——Svpbmt 定义了 PBMT 位（62–61），准确说法是「本仓当前编码不使用 PBMT」，PMA 平台相关并非默认 WB。OS 应自行建立依赖的内存类型环境（读/重设 IA32_PAT、检查 MTRR），不把固件状态当永久合同。
- **`Attribute` 消费决策（新增 I-29-10）**：方案 A（仅准入校验，输出保持 `{base,len}`，不改交接协议）为第一版推荐，显式限制成文；**来源分层**（新增 I-29-11）：UEFI 类型政策与 DTB 来源政策各自成文汇入 OS 统一内存语义（§2.5.7.6 分层图）——`MemoryType+Attribute` 是 UEFI 适配器输入，不是三架构共同输入；RISC-V 走设备树不经此分类。DTB 侧政策尚未成文，需补。
- **v7 剩余工作收敛为四件**（§7）：① 删 `ram_top`（随政策同批）；② 定稿政策表；③ 定稿三架构普通内存映射合同（与②并行，可能反向修改政策表——架构表达不了就降级 `Excluded`）；④ 明确 `Attribute` 交接方式（方案 A/B）。不再开新议题；I-28 可冻结；I-29 设计原则冻结、实施设计待四件完成。

**v8 定稿修订（第七轮评审；七处，一处规范事实 + 一处会影响分类器行为）**：

- **§2.5.7.3 规范推论修正**：无物理硬件支持的地址空洞**通常没有描述符**，不是「一定报告为 `RESERVED`」；「有硬件但 OS 不应访问」才必须报 `RESERVED`。删除 `ram_top` 的结论不变；i440fx 的 `RESERVED` 空洞是单点固件样本，不能推广成「所有空洞都报 RESERVED」。
- **政策表定稿为三列**（分配策略 × 普通 DM × 失败处置），消除两处不一致：① `ACPI_NVS` 原写「ReservedMapped + 按消费者决定映射」与定义自相矛盾——改为「分配=保留；普通 DM 由 ACPI NVS 消费者存在性决定（当前无消费者→不建）」；② `Special` 各行补失败政策——`RUNTIME_SERVICES_*` **不得因当前不调用运行时服务就拒绝启动**（普通 UEFI 机器正常出现，保证不进 PMM 即可）；`PERSISTENT` 保留完整范围信息（不能只留计数）；`UNACCEPTED`/`PAL_CODE` 维持存在即拒绝启动；未知类型 = 分类错误。四状态保留为**文档术语**，代码承载分配/映射/支持状态三个独立判断。
- **`Attribute` 准入具体化（I-29-10）**：进 `Available`/`ReservedMapped` 必须证明 `EFI_MEMORY_WB` 已设置（不得仅凭类型是 CONVENTIONAL/LOADER_DATA 就按 WB 映射）；`RUNTIME` 独立于类型判断（`CONVENTIONAL + RUNTIME` 组合不得进 `Available`，宿主测试必测）；`NV`/`SP` 独立处理；`RO/RP/XP` 是能力位不是当前权限；ISA 位不得静默忽略；`EFI_MEMORY_ATTRIBUTES_TABLE` 登记为未来边界。
- **RISC-V 补 PBMTE 启用链（I-29-8 三架构合同）**：使用 PBMT 需同时验证扩展实现 + `menvcfg.PBMTE=1`（S-mode 可用性由 M-mode 位控制）+ OpenSBI 是否已建立；且 PBMT 取值只有 PMA/NC/IO，**没有「强制 WB」的取值**——普通 RAM 缓存性仍依赖平台 PMA。
- **取消 `ram_top` 的 characterization test**（v7 曾提议保留）：旧行为已被 §2.5.7.3 判定为错误而非「待裁决」，保留测试席位反而增加维护成本。改为 contract test 直接验证新政策（5 条用例，「旧 `ram_top`」仅作测试数据构造参数）。
- **两遍扫描改出新理由**：旧理由（第一遍算 `ram_top`）随删除失效；新理由是事务性（先验证全部待输出描述符再写输出）。是否保留两遍归输出接缝原型决定。
- **`bootstrap_alloc_limit` 保留 `size` 参数**（v7 示例丢掉了原 `checked_sub(size)` 的请求校验，v8 修正）：`size > 0` / 页对齐 / `size <= bound` 三断言 + `bound - 1`；`PA_LIMIT` 与 `bound` 统一半开末端语义。
- **DTB 来源政策明确为 I-29-11 验收**：分层图中 DTB 侧已核事实只有 `/memory` 的 `reg` 解析 + 固件前缀裁剪；`reserved-memory` 子树、`no-map`、重叠检查**无证据**，且 DTB `reserved-memory` 不能直接等同 UEFI `ReservedMapped`。登记为后续独立问题，不阻塞 UEFI 侧实现。

**v9 定稿修订（第八轮评审；四个实现缺口 + 两个事实修正 + 三处残留清理）**：

- **I-29-13 新增：数据合同贯通**。v8 要求属性准入却没把 `attribute` 放进 `DescriptorRef`——「唯一解析器」与「属性准入」两条要求没接上。v9 定稿数据合同：`DescriptorRef { raw_type, physical_start, number_of_pages, attribute }`，由唯一生产解析器完成字段边界验证与 `physical_start` 4 KiB 对齐校验（UEFI 对描述符字段有明确对齐要求，现有代码只查了 `descriptor_size` 对齐）。**I-29-2/3 冻结前必须补上**。
- **I-29-14 新增：特殊范围输出落点**。三行「保留但不映射」（ACPI_NVS / RUNTIME_SERVICES / PERSISTENT）不属于现有两格；v9 裁决加 side list 或第三种 sink 写入承载完整范围，`PERSISTENT` 必须有范围清单（不采用「有意遗忘」选项）。**与接缝原型互为前置**——结果形状定了才能定政策表。
- **I-29-12 新增：PA 表达能力 vs DM 覆盖分两个合同**。`PA_LIMIT` 只查架构页表可表达范围（半开语义）；DM 覆盖由映射规划/验收层独立保证。`ReservedMapped` 与 `Available` 的每个区间都需 DM 覆盖验收（可分配性与可访问性不能脱节）；分类器不做覆盖检查，但交接与映射合同必须补上。
- **I-29-10 补穷尽属性表（§2.5.7.7）**：补 `WP`（记录，不静默忽略）、`MORE_RELIABLE`/`CPU_CRYPTO`（登记「当前不解释」）；`ISA_VALID` **出现即分类错误**（fail-closed，三架构都不解释 ISA 掩码）；`WB` 缺失逐类处置——`CONVENTIONAL` 不进 `Available`，`LOADER_DATA` 不能一刀切（引导装载仍需读取，走恒等窗口是否可行需逐腿核实）；`CONVENTIONAL + RUNTIME` 完整结果 = 不进 PMM、不建普通 DM、保留完整范围（`RUNTIME` 不禁止普通 DM，但项目当前不支持运行时服务）。
- **`AllocatePool` 扩容来源按规范收紧**：UEFI 明确扩容从 `EfiConventionalMemory` 取页——§2.5.3 三情形收成两行，「扩容取自非常规区域」不是规范允许的分支。风险更确定：扩容一旦发生，backing pages 必然落在旧快照可用表里。待测问题收窄为三个：是否扩容、扩多少页、是否落入变化窗口。
- **三处版本残留清理**：§6.11 / §4 事实 23 / 附录 A 的 RISC-V 旧说法统一为「本仓 `flags_to_pte` 不编码 PBMT（Svpbmt 定义了 62–61 位，S-mode 受 `menvcfg.PBMTE` 控制）」；x86 统一为条件句「若 PAT0 未被固件改写且 MTRR 允许，才表现为 WB」；§6.9 旧公式解释改为「当前代码快照，目标实现 `bound - 1`，真正的不变量是 `base + size <= bound`」——不再为 `bound - size` 背书。
- **§5.2 文档矛盾修正**：I-29-8 从「v8 政策表定稿 + 待逐行批准」的冲突表述统一为「政策基线已形成，尚待最终批准」。

**v9 剩余工作（第八轮评审明确：不再泛化抽象设计）**：① 修正分类输入与输出合同（I-29-13/14/12）；② 完成属性政策矩阵（穷尽到「五位必居其一」）；③ 清理事实摘录与注释（§6.11/事实23/附录A/§6.9，v8-v9 已完成文档侧）；④ 两个小原型——生产解析器接静态 sink（确认零分配与容量错误）、路径 A 跑通（退出后缓冲、输出数组生命周期、映射覆盖）。

**v10 定稿修订（第九轮评审；一处规范事实错误 + 四项合同补齐）**：

- **`EFI_MEMORY_WP` 规范归类修正**：它是**缓存性能力位**（write-protected cacheability policy，UEFI 2.5+ 用法），不是与 `RO`/`RP`/`XP` 同类的物理保护状态——v9 把它放在「物理内存保护属性」下又与 RO/RP/XP 区分，是分类错误。确立术语纪律：不得把「保护能力」与「缓存策略能力」混称权限属性（否则讨论 PAT/MTRR 与 MAIR 时两种语义会搅在一起）。
- **规范版本基线显式定为 UEFI 2.10**：穷尽表只承诺覆盖 2.10；2.11+ 新位（如 `EFI_MEMORY_HOT_PLUGGABLE`）按未知属性政策 fail-closed，不为「穷尽」引进整个 2.11 体系；升级是独立政策决定。补 `ISA_MASK` 非零而 `ISA_VALID` 为零的组合同样判分类错误。
- **`WB` 缺失逐类处置重写（无后门）**：v8 的「其余类型不新增后果」违反总原则——`LOADER_CODE`/`BOOT_SERVICES_*`/`ACPI_RECLAIM` 缺 WB 仍会进普通保留映射。v10 补全逐行：全部「不建普通 DM + 范围进 side list」，明确**不存在从属性准入失败绕回普通映射的后门**。属性裁决顺序成文：先未知/不支持条件，再独立属性（`RUNTIME`/`NV`/`SP`）覆盖，最后 WB 准入。
- **side list 生命周期修正**：v8 的「为未来保留」不成立——不进 KernelInfo 也不传内核的清单没有为未来内核功能保留范围。v10 定稿：**shim 内部诊断输出，不构成跨引导边界的保留协议**；未来内核消费这些区域必须扩展 KernelInfo 或另建交接途径。成员由分配 × 映射策略决定，不写死三个类型名。
- **I-29-15 新增：两遍扫描的 preflight 合同**。Pass 1 必须预检**所有**第二遍可能失败的条件（可用/保留/side list 容量、wire+semantic 双写容量、整数转换）——v8 的「两遍天然事务性」未闭合；Pass 2 不可失败的提交合同。给 sink 示意合同（`preflight`/`push_available`/`push_reserved_mapped`/`push_unmapped_reserved`/`finish`）。
- 版本标记清理：§2.6.2/§2.5.7.7/§2.5.7.8/§5.2/§7 的「v8 修正」字样更新为 v9/v10。

**v10 剩余工作（五件）**：§5.3 表更新——① 输入输出合同（I-29-13/14/12）；② 属性政策矩阵（基线 2.10 + WP 归类 + ISA 组合 + 裁决顺序 + 无后门）；③ 事实摘录清理（v8/v9 已完成文档侧）；④ 两个小原型；⑤ preflight 合同（随原型 ① 落地）。**政策表逐行批准时按「任何 Type × Attribute 组合必须唯一确定输出」验收**。

**v11 定稿修订（第十轮评审；五类修订 + 宣告冻结分层——设计方向收口，转原型）**：

- **补齐独立属性的「最终普通映射结果」（§2.5.7.7 新表）**：v10 只写清了 `CONVENTIONAL + RUNTIME` 的完整结果，`NV`/`SP` 只定分配策略未定映射策略，`RUNTIME` 未逐类展开——与「side list 成员由分配策略 × 映射策略决定」自相矛盾。v11 定稿统一规则：`RUNTIME`/`NV`/`SP` 任一命中 → 不进 PMM + 不建普通 DM + 范围进 side list，对**所有** MemoryType 统一适用；评审点名的三个组合（`CONVENTIONAL+NV`、`CONVENTIONAL+SP`、`LOADER_DATA+RUNTIME`）逐行落定。
- **属性组合测试系统化（§7）**：v10 的「随机采样若干组合」降为补充手段；政策定义过的维度交叉（WB 有无 / `RUNTIME` / `NV` / `SP` / `ISA_VALID`×`ISA_MASK` 合法非法组合 / 未支持属性位 × 全部受影响 MemoryType）必须穷测——否则「任何组合唯一确定输出」只是设计宣言。
- **preflight 合同失败语义修正（§2.6.2）**：v10 示意合同的 `finish(self) -> Result<Classification, OutputError>` 与「Pass 2 不可失败提交」在类型表达上自相矛盾。v11 裁决为**类型层面表达**（`finish` 预检全过后返回普通结果，或 Pass 1 产出经验证的提交凭证再进入写入），具体形态不在文档预决，归原型 ① 比较。
- **`WP` 规范背景补全（§2.5.7.7）**：UEFI 2.10 仍保留 `EFI_MEMORY_WP` 历史上的 "Physical memory protection attribute" 标签，同时注明它通常用于缓存策略并区分 `RO`/`WP` 的现代用途——保留这一规范背景，避免读者误以为规范从未如此归类；v10 的「缓存性能力位」归类结论不变。
- **清理 `ram_top` 残留句（§2.5.6.1）**：「最多在类型策略定案后作为辅助过滤」与 §2.5.7.3 的撤回裁决矛盾，属版本残留，已删。
- **I-29-16 新增（§5.2）：三架构普通 WB 映射的最低保证**。UEFI 2.10 的 `Attribute` 是区域支持的属性能力而非当前设置，`EFI_MEMORY_WB` 只证明固件声明支持，不证明项目最终建立的映射是 WB（x86 PAT/MTRR 未读取校验、aarch64 DM 叶子 `AttrIndx` 未确认、riscv64 不编码 PBMT 且 PMA 平台相关）。三架构各自写成可验收合同（x86：PAT/MTRR 前提的读取或自行初始化；aarch64：全部叶子 `AttrIndx` 核查；riscv64：PMA 声明），与原型并行——该合同成立前政策表不得宣告完全冻结。

**冻结分层（第十轮评审裁决，本文采纳）**——**已冻结**：I-28（含正式 supersede I-22）；I-29 的唯一生产解析器/唯一分类规则、零分配分类、预分配输出、`ram_top` 从分类器删除、UEFI 与 DTB 各自适配汇入共同 OS 内存语义、`PA_LIMIT` 与 DM 覆盖两个合同分离、`DescriptorRef` 含原始 `Attribute` 并统一几何校验、属性准入方案 A（不扩展公共交接协议）、side list 语义（shim 内部诊断输出，非内核交接协议）。**暂不冻结**：UEFI 2.10 属性政策矩阵（等逐行批准 + 原型 ① 验证）、三架构普通 WB 映射合同（I-29-16）、Pass 1/Pass 2 具体 API（目标合同冻结，API 随原型）；side list 的具体存储/容量/sink API 随原型确定；DTB `reserved-memory`/`no-map` 仍为 I-29-11 后续独立问题，不阻塞 UEFI 分类器。**v11 封存为阶段性基线，不再有第十一轮纯文档讨论。**

**v11 剩余工作（三件事 + 两个原型）**：① 补齐 `NV`/`SP`/`RUNTIME` 的映射去向（§2.5.7.7 表已落文，随政策表逐行批准收口）；② 三架构普通内存映射最低保证 → 可验收合同（I-29-16，与原型并行）；③ preflight API 失败语义的类型层面表达 + 原型验证。原型 ① = 生产解析器接静态 sink（零分配、容量错误、side list 第三种写入形状、preflight 两种形态比较）；原型 ② = 路径 A 端到端（退出后缓冲、输出数组生命周期、映射覆盖，§2.7 六项验收）。代码侧待改随实施：`bootstrap_alloc_limit` 改 `bound - 1` + 删错误注释；`UefiPoolAllocator`/`build_memmaps` 注释修正（§2.5.9）；`classify_map` 落地类型政策与属性准入；I-22 正式 supersede 记录。

**v12 终审修订（第十一轮终审；三处修订 + 正式冻结宣告——设计讨论阶段结束）**：

- **裁决顺序细化（I-29-17 新增，目标合同冻结）**：v11 的「任意 Type + RUNTIME/NV/SP」与 §2.5.7.1 的类型强制处置之间优先级悬空（`RESERVED+NV`/`UNUSABLE+SP`/`MMIO+RUNTIME`/`PAL_CODE+NV` 四组合无答案）。v12 落定五步顺序：① 未知/ISA fail-closed → ② `UNACCEPTED`/`PAL_CODE` 拒绝启动 → ③ 类型强制处置（`RESERVED`/`UNUSABLE` 排除、`MMIO`/`MMIO_PORT_SPACE` 设备路径）——**类型胜过属性**（类型回答「是什么」，属性回答「怎么用」；属性无法把「不可触碰」改判成「因政策不映射」，误进 side list 会把排除原因记成属性原因）→ ④ 普通内存候选 × 独立属性（候选外延 = `CONVENTIONAL`/`LOADER_*`/`BOOT_SERVICES_*`/`ACPI_*`/`RUNTIME_SERVICES_*`/`PERSISTENT`）→ ⑤ WB 准入。四组合逐行落定：`Excluded`/`Excluded`/设备路径/拒绝启动。
- **ISA 测试维度与政策表对齐（§7）**：v11 维度表把 `ISA_VALID=1` 标成「合法」与 §2.5.7.7「出现即分类错误」矛盾；v12 改三分支（`ISA_VALID` 出现无论掩码 / `ISA_MASK`≠0 且 `ISA_VALID`=0 / 两者均未出现）并分别写明预期结果（前两分支分类错误，第三分支正常裁决）。
- **§7 工作清单与 §5.3 对齐**：删除残留旧「四件事」清单（含已冻结的「删 `ram_top`」）；§7 重写为冻结后收敛路径 = 五步执行序列（原型 ① → 原型 ② ∥ 三架构 WB 合同 → 政策矩阵逐行批准 → 冻结实现规格进生产实现）+ 穷测矩阵（补「裁决顺序组合」维度行，四具名用例必测）。
- **正式冻结宣告（终审裁决落文）**：**I-28 设计冻结**（删除 trait 与空占位、保留 `DtbMemoryMap` 与语义说明、正式 supersede I-22——记录属实施动作）；**I-29 架构设计与目标合同冻结**（唯一解析/分类、零分配 + 预分配输出、`ram_top` 删除不留 characterization test、方案 A、PA/DM 两合同、`DescriptorRef` 含 attribute、side list 语义、五步裁决顺序）。**暂缓冻结（各有明确验收门，不是架构没想清楚）**：UEFI 2.10 属性政策矩阵逐行批准（凭原型 ① + 三架构证据）、三架构普通 WB 合同（I-29-16）、Pass 1/Pass 2 具体类型形态（目标合同已冻结，归原型 ①）、路径 A 端到端运行验收（API 可行性已核实，归原型 ②）。**不再有纯文档讨论轮，剩余问题由真实代码和实验裁决。**

**证据与方案展开**：`rewrite-notes/01-stage-kernel/01-design-note-memmap-consolidation.md`（**v12 冻结版**：篇头冻结宣告、§2.5.7.7 五步裁决顺序 + 四组合表、§5.2 I-29-17、§5.3 执行序列、§7 穷测矩阵三分支 ISA + 裁决顺序行）。**状态**：I-28 设计冻结；I-29 架构设计与目标合同冻结；属性政策矩阵 / 三架构 WB 合同 / 具体 API / 路径 A 验收暂缓（各有验收门）；**下一步 = 原型 ① 与 ②**。

### D-71：`BootForm::source_path` 悬空 + 测试无存在性检查

路径改 `os/libs/minix-boot/src/handoff.rs`；`test_each_boot_form_has_distinct_source` 补五条落点的存在性断言；顺带核其余四条落点。证据：`ls os/boot-shim/src/handoff.rs`（不存在）vs `ls os/libs/minix-boot/src/handoff.rs`。验收：`cargo test -p minix-boot` 全绿。**状态**：✅ 01TODO-B1（路径已改，五落点 `repo_root.join(source_path()).exists()` 断言入测试；首跑以错拼 `../..` 的红验证断言有鉴别力）。

### I-26：opensbi_helpers 孤儿注释 + "hardcoded" 顶注

删两条孤儿 doc 注释（L61–L64）；顶注改 "Memory map → `DtbMemoryMap::collect(dtb_ptr())` (DTB `/memory`; missing DTB halts)"；grep 全文件清剩余 "hardcoded"。验收：`cargo check -p boot-shim`（opensbi feature）通过。**状态**：✅ 01TODO-B1（孤儿注释已删，两句 hardcoded 归零，opensbi 形态 27/0）。

### I-27 + L1424（同批，三处都动 §3/§4 现状 vs 终态叙述）

L1422/L2624「还没裁决/尚未定案」→「OQ-N6 已裁（每进程独立根原语，aarch64 `BootHandoff` 交接即其实现）；x86 腿实施收口 = NK1（C-29）待认领」；L1424 张力块按候选 ① 补「当前产线形态与终态的差距见 §3.4」指针；`os/kernel-image/src/main.rs` 头注顺一句「OQ-N6 已裁」。证据：`grep -n "还没裁决\|尚未定案" rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`。**状态**：✅ 01TODO-B1（两句归零；§3.1/§3.3 现状标注与 main.rs 头注同批，doc-lint error 0）。

### N-1 `BootAlloc::resume` 与 `region()` 缺区间自洽校验（跨镜像 wire 字段未验证即信任）

**现状（本轮新登记）**：`os/kernel/src/boot_alloc.rs:fn resume` 三值裸 store，无 `base <= next <= end` 断言；`fn region` 只判 `end == 0`，不判 `base < end`。`alloc` 的越界判据只有 `pa >= end` 回滚——**不检查 `pa < base`**。跨镜像路径上 `bump_base/bump_next/bump_end` 来自 wire 载荷（对端同批构建，现可信；但"可信"是工序纪律不是结构保证）。后果推演：若 `next < base` 成立，`alloc` 会从声明区间**之外**发页（在 `end` 之前一路成功）——页表页落进未登记区域，扣除表与 DM 覆盖都不含它；若 `base > end`，`region()` 返回倒置区间，DM 候选与扣除两侧都按错区间算。

**修法（两处补一道，不重复）**：① `resume`/`region` 入口断言 `base < end && base <= next && next <= end`（引导期单线程，fail-fast 与 `KernelInfo::validate` 同风格）；② **T1 的 wire 验证必须覆盖同一检查**（§2.2 第 5 步受控复制的"逐载荷验证"清单里加"bump 三件套区间自洽"）——wire 字段不该只靠发送端纪律。两条不必都做，但②是 T1 的必做项、①是现状的可选加固（若做，宿主测试补三类倒置/越界输入）。

**证据命令**：`sed -n '60,90p' os/kernel/src/boot_alloc.rs`；`grep -n "fn resume\|fn region" os/kernel/src/boot_alloc.rs`。**状态**：✅ 01TODO-B1（① 已做：两入口断言 + 五枚宿主测试——1 枚合法 resume + 3 类倒置/越界 + 1 枚 region 侧倒置 init；② wire 侧检查仍为 T1 必做项）。

### N-2 `vm_handoff.rs` 模块头注释与扣除实现不一致（注释回修）

**现状**：模块 doc 的扣除表行写 "reserved module blobs | `KernelInfo.boot_modules()` **minus the VM module** (reclaimed after ELF copy, C: protect.c:450-451)"；而实现 `os/kernel/src/vm_handoff.rs:fn classify`（"Kernel-reserved: every boot module blob **INCLUDING VM's own**"——该处成修注释记录了完整理由：VM 启动前其段帧从本 free list 取出，回收即启动前腐坏）与交接模块清单（"every module except VM's"，C `kinfo.module_list[]` 口径）是**两份不同清单**。01 篇 §3.8 已如实指出"表头注释的旧口径"，但代码注释本身未改——读者先读模块 doc 会得到错的前提。

**修法**：模块 doc 该行改为两行分述——「扣除记录（`classify`）：全部模块含 VM 自身（成修理由：VM 启动前其段帧从本 free list 取，回收即启动前腐坏）」与「交接模块清单（`VmBootHandoff.modules`）：除 VM 外各模块（C `kinfo.module_list[]` 语义）」。验收：`grep -n "minus the VM module" os/kernel/src/vm_handoff.rs` 归零。**状态**：✅ 01TODO-B1（模块头与 `classify` doc 两处均改为两清单分述；01 篇 §3.8 的引文与说明句同批）。

### N-3 零消费者便利包装（`uefi_helpers::load_kernel_elf` / `load_boot_modules`）

**现状**：`os/boot-shim/src/uefi_helpers.rs` L575/L580 两个 `pub fn` 文档称"供集成测试等直接调用 UEFI 路径"，全仓检索**零调用方**（`grep -rn "load_kernel_elf\|load_boot_modules\b" os --include="*.rs"` 只命中定义与一处 doc 引用）。死公共 API 会让读者以为存在另一条产线入口。

**修法**：删除两函数（连同其 doc 注释）；`minix-elf` doc 里对 `uefi_helpers::load_kernel_elf` 的指涉改为指 `loader::load_kernel_with_loader`。验收：上述 grep 归零（`load_boot_modules_with_loader` 保留）。**边界**：`build_memmap()`（单数）**保留**——两个 qemu 测试内核（`test-higher-half`/`test-protection`）在用。**状态**：✅ 01TODO-B1（两函数已删，全仓 grep 归零；`alloc_module_pages`/`build_memmap`/`load_boot_modules_with_loader` 保留）。

### N-4 `bootface.rs` ↔ `bootface_a64.rs` 名字池与常量的重复收敛（独立清理增量）

**现状**：`bootface_a64.rs` 头注自陈"两门全绿后的去重收敛留给独立清理增量"，D-68 的边界也写着"名字池/常量合并超出本条"——但台账里**没有独立条目**接住这件事（只在两处"边界"句里被提及）。重复面：模块名池、`MODULE_NAMES` 同源常量、`DTB_PA`/`KERN_SIZE` 类布局常量、串口打印辅助。风险：两份副本各自漂移（与 I-22 的削界算术双份同型，只是对象换成名字池/常量）。

**修法（D-68 落地后）**：先 `grep -n "NAME_POOL\|MODULE_NAMES\|DTB_PA\|KERN_SIZE" os/kernel-image/src/bootface*.rs` 列全重复面；能下沉到 `boot-shim` 库（`opensbi_helpers` 已有的名字表）就下沉，不能下沉的（架构差异的真常量）留两份并在双方注释互指。**验收**：重复面清单落盘 + 每处按"下沉/互指"处置；不追求两文件结构合并（头注与公理 3 已裁不合并引导形态层）。**状态**：✅ 01TODO-B4。清单与处置——**下沉**到新模块 `os/kernel-image/src/bootface_common.rs`（`cfg(any(aarch64, riscv64))`）：`NAME_SLOT`/`NAME_POOL`、`NAME_POOL↔MODULE_NAMES` 编译期对账、`be_cell`、`fdt_table_pa`、`in_memmap`、`in_memmap_range`、`table_magic_ok`（两文件逐字同形）；**留两份互指**：`say`/`say_hex`（`early_console` 实现按架构不同）、`place_modules`（本地 bump 池与 `MODULE_TABLE` 胶水，差异仅池基）、布局常量组（`DRAM_BASE`/`KERN_PHYS_BASE`/`DTB_PA`/`KERN_SIZE`/`BUMP_*`/`PT_BUMP_PAGES`）。偏离说明：工单原写「能下沉到 boot-shim 库就下沉」，实际落在 kernel-image 包内的共享模块——这批 helper 是 kernel-image 侧零堆/物址视图专用，放进 boot-shim 公共面会为一个消费者扩大库边界；两架构直核冒烟（`test-smp-aps-aarch64`、`test-kernel-image-riscv64-bootface`）均 PASS。

### N-5 池页零化机制归因未定案（开放调查项，非代码缺陷）

**现状**：01 篇 §3.4 明说两组真机读数（`res=127 zeroed=127`；"DM 建立时完好、分类点读零"）指向同一设计结论（入口尽早深拷），但**机制归因未定案**——两条候选（页表不重放 1:1 映射 / 被当空闲复用清零）各带可检验推论，文档标注"可作为后续取证的切入口"。T1 的受控复制设计**不依赖**该归因（拷贝先于一切分配），所以不是 T1 的阻塞项；但证据链缺最后一环，且它决定"池页可读窗口"的教科书式解释。

**修法（定向调查）**：按文档给的两条推论各做一次定向读数——（a）若"页表不重放"：在切根前后各读同一池页地址，读数应在切根时刻突变；（b）若"复用清零"：在 `store_kernel_info` 之前人为推迟消费（测试内核）并在首个 `pg_alloc_page` 之后回读，读数应随复用时点零化。落点：`tmp/` 取证或 qemu 测试内核探针。**验收**：归因结论落盘（一句话 + 读数），01 篇 §3.4/§4.5 相应句从"未定案"改为定案或保留双候选并写明判别结果。**状态**：✅ 01TODO-N5。探针在 `test-kernel-map`：快照之后分配一枚低于 4GiB 的 LOADER_DATA 页、填位置标记，EBS 前/后/开分页后三次整页校验和；读数 `pa=0x5e9b000`、`pre=post_ebs=post_enable=0x4a00_0000_0001_ff00`、`verdict=replayed`、`in_snapshot_free=0`。判别：①"页表不重放 1:1"对该形态不成立（映射重放、字节未变）；②"被当空闲复用"的结构前提未出现；历史零化读数的剩余解释是深拷之前的入口引用链（NK4-A），与深拷落地后症状消失一致。01 篇 §3.4 两处已按读数改写，探针保留为可复跑证据。

---

## 4. 测试缺口批（01 篇 §5.3 TODO-01~07，归拢时点复核）

| ID | 机制 | 断言 | 复核现状 | 去向 |
|---|---|---|---|---|
| TODO-01 | M4 深拷先于一切动用空闲清单者 | 三入口在首个页分配前完成 `store_kernel_info` | ✅ 已覆盖：四入口源码序钉（`store_before_alloc_pin.rs`）；运行期断言经 QEMU 测试内核直接调 `arch_boot_impl` 分阶段验证证明不成立，已撤（判据写在钉注释） | 已交接完成 |
| TODO-02 | M7 分配器写一次 | 二次 `register` debug 失败、release 拒改写 | ✅ 已覆盖：`pt_alloc_write_once.rs` 2 条（`register`/`register_free`，debug panic、release 拒改写且指针不变） | 已交接完成 |
| TODO-03 | M8 落地区容量 fail-fast | 超容量输入 fail-fast（不截断） | ✅ 已覆盖：三枚超容量 `should_panic`（reserved 257 / memmap 129 / modules 13，各持 boot-globals 锁） | 已交接完成 |
| TODO-04 | M12 EBS 阶段边界 | EBS 后路径不出现 `uefi::` 调用 | ✅ 已覆盖：`os/boot-shim/tests/ebs_boundary_audit.rs` 文本级审计（切函数体、去注释、扫 EBS 后语句；注入负控 + 注释豁免自检）；T1CUT 接线后已按最终形状升级为两条入口（`prepare_boot` 与 `prepare_and_enter_kernel_image`）各一套，5/0 | 已交接完成 |
| TODO-05 | §4.6 大页回退档 | 2MiB 回退档下真机启动 | 仍 open：当前环境 QEMU-only（无真机，窗口不可得），真机断言保持开放；软件路径的 QEMU 等价覆盖存在候选——能力压制腿（如 x86 `-cpu qemu64,-pse` 关 2MiB PSE，逐架构类推）可逼 `plan_dm_policy` 走回退档，尚未实施、未排期 | 真机窗口项（等硬件）；候选软路径腿另行立项 |
| TODO-06 | M14 x86_64 路标链 | `kernel loaded`/`12 boot modules loaded`/`handing control` 三路标 | ✅ 已覆盖：`test-cmd-smoke.sh` Stage 3b 逐条 `grep -qF`；本批实跑三行全在。脚本后续 du/stat/find 腿在主树 HEAD 亦失败，属既有问题不在本条 | 已交接完成 |
| TODO-07 | §4.2 UEFI 侧宿主可测面 | 两列表互斥完备、root/bump 不与 free 相交 | ✅ 纯函数已覆盖：`assert_bootstrap_outside_memmap` 3 条（相离/贴边通过、包含/跨界 panic）；`build_memmaps` 调用点单根，可测面随 T1 分类循环纯函数化后重定义 | 纯函数三案完成；其余随 T1 |

---

## 5. 文档批（01 篇正文，改文档不动码；文档线程执行）

| 条目 | 内容 | 同批 |
|---|---|---|
| I-17 | §2.6 拆节（`alloc_lowest` 缺陷分析、静态页表池两段独立成 §2.6.3；纯移动不删内容；§2.6 现 347 行占章 31%） | I-18 |
| I-18 | §1、§3–§6 章头导语与 ≥300 行巨节体检 | I-17 |
| I-19 | §3–§6 讲解条续扫「只报事实不报道理」（§2 已扫 40 条修 4；§3–§6 约 90 条待过） | — |
| I-23 | §3.7 补 `fw_payload` 真板用途段 + 三架构落位矩阵（~10 行，`[ARCH]` 三处一致；现 `fw_payload` 术语零命中） | I-22 文档连带、I-24（§3.7 一次改完） |
| I-24 + L1378/L1408 | §3.1 否决表拆两行（UEFI 形态被否 / opensbi 形态采纳）；正文 L1378「OpenSbiBootShim 无调用方」叙述与 L1408 依赖方向 TODO 同批收敛 | I-22、I-23 |
| L1488 | 「x86_64 的待退场，此处待核实，并透彻学习」——`reserved_regions` 内联窗口退场路径。**用户侧学习项**；可回读 01-code-design §2.5/§2.8（消费面拆分）辅助 | NK1/T2 |
| L1532 | 「这个是交互式文档类TODO我读不懂」——L1424 块的性质是**文档-代码张力登记**（正文以终态口径写、代码实况是过渡形态，要求正文标注）。**用户侧澄清项**，处置同 L1424（§3 I-27 批） | L1424 |
| I-25 | `os/` 全仓注释语言与锚点整改（低优；01 篇射程内样例：uefi_helpers D-64②/F8 段、bootface 头注等，随各条顺手按 a/b 口径处理） | 全仓批 |
| **§5.2.1 测试计数漂移**（本轮新登记） | `opensbi_helpers.rs` 文档记 13、`grep -c '#\[test\]'` 实际 **12**（连带"15+13+1=29"的算式应为 15+12+1=28）；`x86_64/paging.rs` 文档记 14、实际 **15**。按复算命令重跑后改数（其余核过：loader 15、uefi_helpers 1、kernel_info 9、platform 8、boot_form 6、minix-elf 23、memmap 16、boot_alloc 5、vm_handoff 7、dm_coverage 10 均一致） | 文档线程；与 §5.1 M6 行的"paging.rs 14 条"同处改 |
| **§4.6 `IDENTITY_MAP_END` 措辞**（本轮新登记） | 正文写"`IDENTITY_MAP_END`（lib.rs 里固定的低地址上界）"——D-60 闭合后 kernel 侧已是 `use minix_arch::BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END` 别名，"lib.rs 里固定"失真；改为"唯一权威在 `minix-arch`（`BOOT_IDENTITY_MAP_END`），kernel 侧是别名" | D-60 回勾行（§3） |
| **I-25 追加样例：`globals.rs` 陈旧注释**（本轮新登记） | `FREE_MEMMAP` 上 `#[allow(dead_code)] // SMP memmap infra (checklist D-17); not yet wired to all call sites`——"checklist D-17"与 `todo.md` 的 D-17（IPC 过滤）不同指、锚点不可达；且 `FREE_MEMMAP` 已被 `kmain` A.2 使用，"not yet wired" 多半过期（`allow` 现在可能只为 mock 构建保留）。落地时核实 `allow` 是否仍必要，注释按实况改写 | I-25 |
| I-21 | 篇头「路径约定」13 篇重复上收 | ✅ 已上收（I21HOIST 批，用户授权关闭）：全局化入 README「全局阅读约定」，13 篇篇头改一句指针 |
| **I-30（本轮新登记）** | **01 篇正文补「内存图生产的四层责任」概念讲解**。第十轮评审收尾语点名了这件事："你这份文档已经把一个很容易被混淆的问题拆得相当清楚了——UEFI 固件事务、描述符解析、OS 内存语义分类、内核映射与分配，是不同层次的责任。现在需要做的是让这些合同经受代码和实验检验，而不是继续给它们添加抽象层。" 设计笔记 v11 已把四层拆开（§0.3 MapKey 机制 + §1.2 三条结构原因 = 固件事务层；§2.6.2 数据合同 = 解析层；§2.5.7.1 政策表 + §2.5.7.6 分层图 + §2.5.7.7 属性表 = 分类层；I-29-12 两合同 + §0.4 消费方 = 映射与分配层），但 01 篇正式文档还没有这段教学：§3.4「内存图交付成两张表」段（L1474-1487）只讲两张表的形状与五条腿来源，§4.2 只给三条现行分流规则——四层各自的责任边界（分类判据为何不能靠地址 / 固件事务为何绑死分类时机 / DM 覆盖与可分配性为何是另一个合同）未成段讲解。**落点**：§3.4 该段扩写或紧随其后新增小节。**时序约束**：四层概念讲解可先行（分层模型已获评审确认、属冻结方向），但涉及政策细节的句子不得先于 I-29 政策表批准承诺未冻结内容——先行段落只讲责任边界与判据，具体政策行文指设计笔记 v11 对应节；I-29 实施落地后 §4.2 分流规则叙述按四层模型同批改写（属性准入 / side list / WB 缺失处置都会改现行三条规则）。**验收**：读者不开设计笔记即可从 01 篇理解四层责任划分及其理由；事实断言带锚点（设计笔记节号 + 代码符号）；doc-style-lint / doc-ref-check --check / anchor-resolve 全绿 | I-29 实施批（§4.2 同批部分） |

> **2026-10-29 回勾（01TODO-B1）**：§5.2.1 计数漂移与 §4.6 `IDENTITY_MAP_END` 措辞两项已在正文闭合。前者除登记的三处（opensbi 13→12、`15+13+1=29`→`15+12+1=28`、paging 14→15）外，复算还纠正了 minix-boot 总数（23→32，原算式漏 `memmap_source.rs` 的 9 条）并把 boot-shim 的 28 口径写准（须带 `--features test-all`）；后者两处（§4.6 Step 1 与 §1 的同一措辞）均改为「唯一权威在 `minix-arch` 的 `BOOT_IDENTITY_MAP_END`，kernel 侧是别名」。

> **2026-10-29 回勾（01TODO-DOC）**：I-17（§2.6 内部拆五节，最大节 176 行；顶层不改号以保 prompt/ 与 02 篇的历史锚点）、I-18（§4/§6 补章头导语；≥300 行巨节体检：仅 §2.6 入选且已拆，§2.4 287 行未达阈值记录不改）、I-19（§3–§6 讲解条续扫 ~90 条，修 4 条纯事实条目）、I-23（§3.7 FW_PAYLOAD 落位矩阵 + `[ARCH]` 三处一致）、I-24 + L1378/L1408（否决表拆两行、TODO 块改状态叙述、I-22 终态句）、I-25 射程样例（FREE_MEMMAP 陈旧锚点、D-64②/F8 不可达标签）六项闭合。设计侧 `[ARCH]` 镜像因该文件在主树是用户未提交脏版，落在工作区未代提交。

---

## 6. 射程外指针（防重查）

- **D-60** 已闭合（kernel 侧 `use minix_arch::BOOT_IDENTITY_MAP_END` 归一，全仓单定义）；**I-20** 已闭合（b0da9074e）。
- **I-1**（kernel 独立 ELF 构建接入 xtask）原 DEFERRED 的解除前置「入口交接 ABI 设计 + boot-shim 终局跳转」已由 T2 v9 全套裁决补齐——不再独立登记，随批次五实施（bin 目标 + build.rs 一并）。
- **I-5**（ACPI RSDP）DEFERRED：物理机 x86_64 支持时实现，当前三家族 QEMU virt 均不依赖。
- **`VmBootHandoff`**（`os/libs/minix-types/src/types/boot.rs`）是内核→VM 的运行期上层交接、带 magic/version，属另一层契约，**不在 01 篇射程**。
- **T-10**（riscv64 U-Boot 真实启动链集成）在 todo.md §7.3，非 01 篇正文登记（§7.4 有引用），随 CI 窗口处理。

---

## 7. 验收总则（每条通用）

1. **证据先行**：动手前跑该条证据命令确认现状；与工单不一致时以源码为准并写明偏离。
2. **裁决已冻结**：§2 各条直接按 `01-code-design.md` v9 实施；实现证伪既定前提才重开局部裁决，不开新设计轮次。
3. **文档同步**：涉 01 篇正文的同批更新（Doc Code Sync 七项）；§5.3 交接表回填覆盖状态。
4. **门禁**：`cargo test -p <crate>` 与 `cargo clippy` 通过；共享契约层（minix-boot）条目加三架构构建与 qemu 引导冒烟；文档五门在受影响文档复跑全绿。
