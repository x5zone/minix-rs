# 01 · 引导链 v4 第二轮外部评审后的最小补证（只调查不改码）

> **生成日期**：2026-10-28。
> **用途**：外部评审对 round-4 报告提出三项定向补证——重建分配时间线台账（消除报告内部矛盾）、核实 x86 实际编译目标与 red-zone、核实 `cut_memmap` 边界语义。本报告逐项给出仓库侧答案，并声明对 round-4 结论的修正。每条标 **已证实 / 反证 / 尚无证据**，附路径 + 符号锚点。核心结论先行：**round-4 的三处表述错误已定位并修正**——快照口径混用（A 批）、red-zone 推导不成立且无需任何旗标（B 批）、`cut_memmap` 本就页化扩展而「精确切除」是错误猜测（C 批）。

---

## A. 按「每一份实际 backing allocation」重建的分配时间线（已证实，修正 round-4 的口径混用）

### A.0 矛盾根源：两个「内存图」被当成了一个

round-4 同时写下「EBS helper 缓冲不在内核收到的 conventional 清单里」（A.1 第 2 点）与「它在快照 A 中仍是 conventional → 进内核 conventional 空闲清单」（A.1 第 5 点、B 表）。两句冲突的原因不是证据分歧，而是**口径混用**：

- **快照 A**（`build_memmaps` 内 `boot::memory_map(MemoryType::LOADER_DATA)` 取得的活图）——内核收到的 conventional 清单**以此为来源**；
- **快照 B**（EBS 时点活图，即 helper 返回的 `MemoryMapOwned` 所描述的图）。

helper 缓冲分配在快照 A 之后：快照 A 把它记 conventional（进内核 free list），快照 B 把它记 LOADER_DATA（保留）。A.1 第 2 点错拿了快照 B 的口径说「内核清单」，第 5 点用快照 A 口径才对。下表把两口径拆开，矛盾随之消失。

### A.1 逐 backing 台账（按真实执行顺序）

`prepare_boot` 的时序骨架（真实代码节选，序号对应下表；省略处为诊断打印与断言）：

```rust
// os/boot-shim/src/uefi_helpers.rs:fn prepare_boot（时序骨架）
let root_page = alloc_root_page();                          // ① 快照 A 前
let (bump_base, bump_end) = alloc_bump_region(bump_pages);  // ② 快照 A 前
let kern = loader::load_kernel_with_loader(&file_loader);   // ③ 内核段直写，不经 UEFI 分配
let boot_modules =
    loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages); // ④⑤ 快照 A 前
let platform_sources = find_platform_sources();             // ⑥ 快照 A 前
let (memmap, reserved_regions) = UefiMemoryMap.collect();   // 快照 A（图缓冲 ⑦ 在 GetMemoryMap 前一刻）
//   └─ build_memmaps 内：Box::leak(regions) / Box::leak(reserved)  // ⑧ 两表 backing，快照 A 后
let kernel_info = build_kernel_info(…);                     // 纯组装，无分配
exit_boot_services();                                       // ⑨ helper 图缓冲（快照 B 载体），快照 A 后
```

| # | 对象 | 分配点（符号锚点） | 相对快照 A | UEFI API / 类型 | 快照 A 记录 | 快照 B 记录 | 末次读取 |
|---|---|---|---|---|---|---|---|
| 1 | root 页 | `uefi_helpers.rs:fn alloc_root_page` | 前 | AllocatePages(MaxAddress, LOADER_DATA) | LOADER_DATA（reserved 侧） | LOADER_DATA | shim 建表期 |
| 2 | bump 区 | `uefi_helpers.rs:fn alloc_bump_region` | 前 | AllocatePages(MaxAddress, LOADER_DATA) | LOADER_DATA | LOADER_DATA | 内核页表分配期（游标推进） |
| 3 | 内核段体 | `os/boot-shim/src/loader.rs:fn load_kernel_with_loader` | 前 | **不经 UEFI**（直写 conventional RAM，F8 更正注释自陈） | conventional → 进 free list，内核 A2 侧 `deducted` 扣除 | conventional（同左） | 装载与跳转期 |
| 4 | 模块 blob ×12 | `uefi_helpers.rs:fn alloc_module_pages`（AnyPages） | 前 | AllocatePages(AnyPages, LOADER_DATA) | LOADER_DATA（reserved） | LOADER_DATA | VM/内核装载该模块（Phase C 起，bootstrap 根活跃期） |
| 5 | 模块元数据数组 + 名字串 | `loader.rs:fn load_boot_modules_with_loader`（`Box::leak` L312/L320） | 前 | AllocatePool→LOADER_DATA，leak | LOADER_DATA（reserved） | LOADER_DATA | `store_kernel_info` 深拷（内核第一动作） |
| 6 | platform_sources 数组 | `uefi_helpers.rs:fn find_platform_sources`（`Box::leak` L138） | 前 | AllocatePool→LOADER_DATA，leak | LOADER_DATA（reserved） | LOADER_DATA | 平台发现（kmain Phase A–D，首次切根前） |
| 7 | 快照 A 自身图缓冲 | uefi crate `boot::memory_map` 内部 | 取图前一刻 | AllocatePool→LOADER_DATA | LOADER_DATA（缓冲先分配后 GetMemoryMap，活图记它 LOADER_DATA——`build_memmaps` L319-330 注释自陈） | LOADER_DATA | `build_memmaps` 分类完（EBS 前） |
| 8 | 两表 Vec backing ×2 | `uefi_helpers.rs:fn build_memmaps`（`Box::leak` L403-404） | **后** | AllocatePool→LOADER_DATA，leak | **conventional（A-β 本体）** | LOADER_DATA | `store_kernel_info` 深拷 |
| 9 | EBS helper map 缓冲（快照 B 的载体） | uefi 0.33.0 `boot.rs:fn exit_boot_services` → `MemoryMapBackingMemory::new` | **后**（首次 EBS 调用前） | AllocatePool→LOADER_DATA | **conventional** | LOADER_DATA（它描述的就是自己所在的图） | helper 内部 GetMemoryMap；boot-shim 判定成功后即丢弃 |
| 10 | wire 载荷（设计，未实施） | 待实施 | 计划：快照 A 前（v4 §1.5（六）已裁） | AllocatePages/Pool → LOADER_DATA | LOADER_DATA（reserved） | LOADER_DATA | 镜像侧 `store_kernel_info` |

**快照 A 之后、快照 B 之前的新分配全集 = #8（两笔）+ #9（一笔），再无其他**。这是消除 round-4 内部矛盾后的最小完整差异集。

### A.2 对 round-4 三处错误的定点修正（已证实）

1. **A.1 第 2 点错**（EBS helper 缓冲「不在内核收到的 conventional 清单里」）：内核清单来自快照 A，helper 缓冲分配在快照 A 后 → **在**内核 free list 内。正确表述：「它在快照 B 中是 LOADER_DATA（保留），但在快照 A 中被记 conventional、随分类进入内核 free list——内核清单与 EBS 时点真实图不一致，这正是 A-β 的同型放大，也再次论证快照 A 不能充当最终内存图」。
2. **A.1 第 4 点错**（「两表 Vec backing 在快照 B 中均已存在，无差异」）：两表 backing 在快照 A **之后**分配（A-β 定义即此，round-3 已证）——快照 A 记 conventional、快照 B 记 LOADER_DATA，**恰是两图差异的主体之一**。模块 blob、platform_sources、模块元数据、root/bump、快照 A 自身缓冲才是「快照前已存在、两图一致」的集合（表 #1/2/4/5/6/7）。
3. **B 表两行错**：platform_sources 行与模块元数据行的「进 VM free list = 是」不成立——两者的 backing 均在快照 A 前分配，活图记 LOADER_DATA，进 reserved 侧、不进 free list。B 表把「A-β（两表 Vec）」的错误归类复制到了这两行。两表 Vec 行与 EBS helper 行的「是」维持正确。

### A.3 「哪些必须 reserved、哪些可有意回收」的判断（已证实，基于上表）

- **由最终图记 reserved（结构性正确）**：#1/2/4/5/6/7 在两图中都是 LOADER_DATA，任何一张图作权威输入都正确排除它们。方案甲下 #8/#9 也自然归入（快照 B 记两者 LOADER_DATA）——不存在「快照后分配漏记」类别，A-β 的根因被消灭而不是被枚举规避。
- **可在末次读取后有意回收（须成文，不是漏记）**：#8 两表 backing（`store_kernel_info` 深拷后死页）、#9 helper 缓冲（boot-shim 丢弃后零读者）。方案甲下这两类的保守记账 = reserved 常驻（少量内存损耗），是明确、可审计的策略——若未来要回收，须显式登记逐页回收，不得依赖「快照 A 恰好把它算 free」这种与最终图打架的账目。
- **内存图准确性与「是否实际被覆写」是两个命题**：现状快照 A 不准确（#8/#9 记错侧）但恰好无害（#8 深拷先于一切分配、#9 零读者）。无害性依赖两段时序巧合，不构成「快照后分配可以漏记」的一般许可——跨镜像统一 handoff 协议的核心正确性证明不应携带时序例外。

### A.4 方案甲实施前提的逐条检查（已证实可行性 + 两点设计澄清）

外部评审给方案甲（EBS 返回的 `MemoryMapOwned` 为唯一权威输入）附的四项验收条件，对照仓库现状逐条：

1. **EBS 前预分配全部 post-EBS 输出缓冲**：可行。分类输出走固定容量数组（容量上界 `MAXMEMMAP` 已存在，`os/kernel/src/memmap.rs:pub const MAXMEMMAP`）；wire 载荷按 v4 §1.5（六）在快照前分配。**须在 bootstrap 根下读的缓冲**（wire 载荷、输出数组若跨镜像传递）用 `AllocateMaxAddress(bootstrap_alloc_limit(...))` 分配（先例：`alloc_root_page`/`alloc_bump_region` 同款算术），不依赖池地址碰巧低。
2. **用 helper 返回的最终图分类**：现状 `uefi_helpers.rs:fn exit_boot_services` 丢弃返回值（L539-543 注释自陈 "The returned memory map is discarded"）——改为捕获并交给分类循环即可，无需绕 crate。
3. **EBS 后零动态分配**：分类循环纯计算、只写预分配数组；容量不足显式失败（拒绝截断内存图）。
4. **`MemoryMapOwned` backing 生命周期成文**：helper 的图缓冲属 LOADER_DATA 且 drop 不释放（crate 文档 "the allocation will not be freed on drop"）。方案甲下按最终图记 reserved 是保守正确的策略——同时声明「它常驻」而不是「它已被回收」。

**设计澄清一（读取时点与映射窗口）**：`MemoryMapOwned` 的 backing 由 helper 用 `AllocatePool` 分配，**任意地址、无 MaxAddress 上界**。这不违反「bootstrap 根下读的缓冲须有上界」——因为分类循环读它的时点在 **EBS 后、自建页表启用前**，此时仍在固件遗留的 identity map 下（固件映射覆盖全部已安装 RAM，与 bootstrap 根的 [0,4GiB) 窗口是两套映射）。该时点约束须与方案甲一起成文；输出数组与 wire 载荷（会在 bootstrap 根下被读）才需要 MaxAddress 上界。

**设计澄清二（与 v4 §1.5（六）第 1 点的关系）**：v4 的「所有影响最终内存图的 UEFI 分配必须在最终快照之前完成」在方案甲下从**流程纪律**升级为**结构保证**——分类直接基于 EBS 终图，「快照后分配」这一类别整体消失，无需再枚举或预测。

---

## B. x86 实际编译目标与 red-zone（反证成立：round-4 P1-E 撤销）

### B.1 实际 target triple（已证实）

```rust
// os/xtask/src/image.rs（target 定义节选）
Self::X86_64 => "x86_64-unknown-none",   // 内核独立镜像（T2 的 kernel.elf）target
Self::Aarch64 => "aarch64-unknown-none",
// …
shim_target: "x86_64-unknown-uefi",      // x86 boot-shim（PE 载体，内核当前内联于其中）
```

- **x86 内核独立镜像（T2 的 kernel.elf）**：`x86_64-unknown-none`——`os/xtask/src/image.rs:53`（内核 ELF 缺省路径即 `target/x86_64-unknown-none/<profile>/kernel`）。
- **x86 boot-shim（PE 载体，内核当前内联于其中）**：`x86_64-unknown-uefi`——`os/xtask/src/image.rs:77`（`shim_target: "x86_64-unknown-uefi"`）。
- 工具链：stable rustc 1.94.1（默认工具链；另装有 nightly）。

### B.2 目标规格的 red-zone 默认值（已证实，决定性）

用 nightly rustc 打印两个目标的基础规格（stable 不开 `-Z unstable-options`，但 target spec 是随编译器分发的固定数据，与哪个工具链打印无关）：

```console
$ rustc -Z unstable-options --print target-spec-json --target x86_64-unknown-none | jq '."disable-redzone"'
true
$ rustc -Z unstable-options --print target-spec-json --target x86_64-unknown-uefi | jq '."disable-redzone"'
true
```

两个目标的基础规格**显式设置 `disable-redzone: true`**（对位 Linux x86_64 内核 `-mno-red-zone` 的同款默认）。`os/.cargo/config.toml` 的 rustflags 只叠加 relocation-model/target-feature，不触碰 red-zone；也没有任何自定义 target JSON（全仓无 `--target-json`/自定义 spec 命中）覆盖默认项。

### B.3 结论（撤销 P1-E）

- round-4 P1-E（「x86 内核目标未禁 red zone……修法 = xtask 注入 `-Z red-zone=no`」）**撤销**。其推理链「配置里没有显式禁用旗标 → 默认启用」不成立：对 freestanding/UEFI 目标，**规格默认即禁用**，「没有旗标」正是默认生效的形态。附带的旗标形式也写错了——red-zone 的 codegen 选项是 `-C` 级（`-C no-redzone`），不存在 `-Z red-zone=no` 这个形式；且本例根本无需注入。
- 不需要任何新构建门；若要防回归，可选做法是把「内核镜像产物不得假设 red zone」写成文档事实而非检查项（没有可靠的反汇编特征能廉价断言「未使用 red zone」，规格级证据已足够）。
- **独立的两项维持待实施**（与 red-zone 无关，不受撤销影响）：`_start` 进入 Rust 前的显式 `cli`/`cld`（round-4 E.3 已证现状皆无；`cli` 只屏蔽可屏蔽中断，不覆盖同步异常与 NMI，故它只是入口契约的一项、不能替代「早期入口代码不触发异常」）；blob 物理指针跨 `.bss` 清零循环的寄存器/槽位选型（`rdi` 被清零游标占用，候选 callee-saved `rbx`/`r12` 或固定内存槽）。

---

## C. `cut_memmap` 的边界语义（已证实：自动页化扩展，「精确切除」是错误猜测）

### C.1 实现语义（`os/kernel/src/memmap.rs:fn cut_memmap`，L175 起）

```rust
let cut_start = start & !(page_size - 1);              // start 向下取整到页边界（扩大切除）
let cut_end_raw = start.saturating_add(len);           // start+len 饱和加——不溢出
let mut cut_end = (cut_end_raw + page_size - 1) & !(page_size - 1);  // end 向上取整到页边界
if cut_end == 0 && cut_end_raw > 0 {
    cut_end = u64::MAX;                                 // 取整回绕 → 切到可寻址内存末尾
}
```

逐条回答三项问题：

1. **非页对齐输入是精确切除还是页扩展？** 页扩展。语义恰为外部评审给出的公式 `[floor_P(start), ceil_P(start+len))`——`start` 向下、`end` 向上扩展到页边界，与 DTB 永久扣除所需的区间形状逐字对应。round-4 G.5「`cut_memmap` 语义 = 精确区间移除，非页化」是**错误猜测**（该猜测自己也标了「实施时先验证」，本条即验证结果）。
2. **取整溢出如何处理？** 三层：`saturating_add` 防区间加法溢出；取整回绕检测（`cut_end_raw` 超过 `u64::MAX - 4095` 时加法回绕，回绕值必 `< 4095`，与 `!(page_size-1)` 相与必为 0——检测条件恰好完备）；回绕后保守切到 `u64::MAX`。
3. **腿侧先对齐还是底层保证？** **底层已保证，腿侧无需预对齐**。DTB 扣除调用可直接传原始 `[dtb_pa, totalsize)` 区间；传页对齐后的区间结果相同（幂等）。`vm_handoff.rs` 的 `DeductionCollector::push`（L138-153）另有自己的对齐断言（start 必须页对齐、len 向上取整）——那是**记账侧**与 cut 侧的对账约定，不是对 cut 的前提要求；两处语义一致（注释原文 "rounded up to the page the way `cut_memmap` rounds its cuts"）。

### C.2 非页对齐最小测试已在库（反证 round-4 的「待验证」登记）

`os/kernel/src/memmap.rs` 既有测试：`test_cut_memmap_alignment_round_down_start`（L389，非页对齐 start 向下扩展）、`test_cut_memmap_alignment_round_up_end`（L405，非页对齐 end 向上扩展）、`test_cut_memmap_preserves_total_free_memory`（L428，切除守恒）。**无需新增测试即可关闭 G.5**；DTB 实施时的增量验收只剩「真实 `dtb_pa + totalsize` 传入后 reserved 记账与预期页区间一致」这一条端到端检查。

---

## D. round-4 结论修正清单与 v4 裁决影响

| round-4 原结论 | 本轮处置 | 依据 |
|---|---|---|
| P0-A：EBS helper 缓冲「不在内核 conventional 清单里」（A.1 第 2 点） | **修正**：在快照 A 口径下它进 free list；「保留」只对快照 B 成立 | A 批台账 #9 |
| P0-A：两表 Vec backing「两图间无差异」（A.1 第 4 点） | **修正**：它是两图差异主体之一（A-β 本体） | A 批台账 #8 |
| B 表 platform_sources / 模块元数据「进 free list = 是」 | **修正**：均在快照 A 前分配，进 reserved 侧 | A 批台账 #5/#6 |
| P0-A 裁决方向（方案甲优于方案乙） | **维持并被外部评审正式采纳**；附四项验收条件（A.4） | 外部裁决 + A.4 可行性检查 |
| P0-C：`translate` 返回 `&'a [u8]` + debug 存活桩的 API 草图 | **撤销**（外部评审否决：`&'a` 非调用期借用；debug 断言不能承载安全 API 前提）——三路径重裁见 v5 §1.4 | Rust 语义 + 外部裁决 |
| P1-D：模块 blob AnyPages 无上界 | **维持**，修法（MaxAddress 上界）进入待实施设计 | round-4 D 批未被质疑 |
| P1-E：x86 未禁 red zone，注入 `-Z red-zone=no` | **整体撤销** | B 批 |
| T2 入口前置「xtask red-zone 注入配方验证」 | **删除**（同上）；`cli`/`cld` 与 blob 寄存器选型两项维持 | B.3 |
| P2-F：arch/board 降级展示信息 | **维持**；措辞收窄为「未发现仓库内消费者」 | 外部裁决 |
| P2-G：DTB 页对齐永久扣除 + 「cut 语义待验证」 | **方向维持**；待验证项关闭（cut 自动页化，测试已在库） | C 批 |

### 会改变 v4 裁决的结论（交付要求第 4 条）

1. **T1 wire 数据存储**：v4 §1.5（六）的「快照前分配」流程纪律升级为方案甲结构保证（分类基于 EBS 终图，快照 A 取消）；`MemoryMapOwned` 读取时点约束（固件映射窗口内）与输出缓冲 MaxAddress 上界成文。
2. **T1 映射有效性 API**：`BootReadPhase::translate` 草图作废，改为三路径（作用域回调 / 复制 / 狭窄 unsafe）择一而裁，`PhysInterval` 构造权封闭。
3. **T2**：red-zone 前置项删除；模块 blob 分配上界进入设计。
4. **T3**：「宁可写占位值」废除（真实值或显式省略），结论范围收窄为「未发现仓库内消费者」。
5. **T4**：`cut_memmap` 语义核实完毕，DTB 扣除可直接传原始区间，页化由底层保证；永久扣除损耗 ≤ 2 页维持。
