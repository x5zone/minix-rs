# 01 · 引导链 v4 定向取证报告（只调查，不改代码）· 2026-10-21

> **用途**：外部三轮评审提出的剩余争议（wire 序列化时序 / 物理地址读取语义 / x86 入口 / DTB 生命周期 / ARCH-BOARD 语义），逐项对真实代码取证。每条发现标 **已证实 / 反证 / 尚无证据**，附仓库相对路径 + 符号锚点。本报告是简报 `01-code-design.md` v4 的证据补充，不改其裁决结构。
> **一句话总览**：评审的三项 P0 **全部被证据坐实**，其中两处比评审预计的更严重（切片载荷页已进内核空闲清单、C 内核回读 board 键）；一处被现状代码**提前解决**（x86 二段式栈已是三架构既成实现）；一处改变了裁决框架（Rust 内核无 board 回读，义务射程待定）。

---

## 任务 A：wire 序列化、UEFI 内存图与所有权

### A.1 原始 `KernelInfo` 各切片的分配点（已证实）

| 载荷 | 分配 API | 上界约束 | 位置 |
|---|---|---|---|
| `memmap` / `reserved_regions` 两表 backing | `Vec`（UEFI 池分配器）→ `Box::leak` | **无**（AllocatePool 任意落点） | `os/boot-shim/src/uefi_helpers.rs:fn build_memmaps`（leak 在 L403-404） |
| `boot_modules` 数组 backing | `Vec` → `Box::leak`（loader.rs:281 起） | **无** | `os/boot-shim/src/loader.rs:fn load_boot_modules_with_loader` |
| 模块 blob 内容 | `allocate_pages(AllocateType::AnyPages, LOADER_DATA)` | **无**（AnyPages 任意落点） | `os/boot-shim/src/uefi_helpers.rs:fn alloc_module_pages`（L564-568） |
| `platform_sources` | `Vec` → `Box::leak` | **无** | `uefi_helpers.rs:fn find_platform_sources`（L138） |
| `param_buf` | `&[]` 常量 | — | 两腿构造点 |
| 内核段体 | **不经 UEFI 分配**——直接写 ELF LMA（`seg.paddr`，链接脚本定址） | 链接脚本定址（< 4GiB） | `os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory`（L218-228 直写循环） |
| root page / bump 区 | `allocate_pages(AllocateType::MaxAddress(bootstrap_alloc_limit(size)), LOADER_DATA)` | **有**：上界 = `min(BOOT_IDENTITY_MAP_END, VM DM 窗口)`（`boot_dm_admissible_end()`） | `uefi_helpers.rs:fn alloc_root_page`/`fn alloc_bump_region`（L449-485） |

**结论**：全链只有 root page 与 bump 区有「落在恒等窗口内」的构造保证；池分配（三块 Vec）与模块 blob（AnyPages）均无。

### A.2 快照与 EBS 的实际顺序（已证实，含一处评审未预期的发现）

`prepare_boot` 执行序（`uefi_helpers.rs:fn prepare_boot`）：root/bump 分配（L154-155）→ 内核装载（L172-178）→ 12 模块（L180-182）→ 平台来源（L188）→ **最终快照**（L193 `UefiMemoryMap.collect()` → `build_memmaps`）→ 双区间断言（L201-202）→ aarch64 出界闸（L211-238）→ `build_kernel_info`（L250-279）→ `exit_boot_services`（L282）。快照后到 EBS 前**无任何分配**（build_kernel_info 与断言均为纯计算）——已证实。

**发现 A-α（反证「快照 = EBS 时点图」）**：`os/boot-shim/src/uefi_helpers.rs:fn exit_boot_services`（L536-544）走 uefi crate 的 `boot::exit_boot_services(MemoryType::LOADER_DATA)` helper——它内部**另取一张新内存图**再调 EBS，返回图被丢弃（注释原文 "The returned memory map is discarded because the kernel already received the CONVENTIONAL regions via `build_memmap()`"）。即：**内核快照与 EBS 时点固件看到的是两次独立 GetMemoryMap**。现状两者一致的原因是中间零分配；wire 序列化的任何「快照后分配」都会使两图分叉——评审问题 A 的机制基础在此。

**发现 A-β（比评审预计更严重）**：`build_memmaps` 内部，`boot::memory_map()` 先分配快照缓冲再 GetMemoryMap（缓冲被分类为 LOADER_DATA，注释 L319-330 自陈），**但两表 Vec 的 backing 在 GetMemoryMap 之后分配**——注释原文："The two output slices are built from the same pool AFTER this snapshot, so the pages backing them are still reported as conventional/free in `regions`"。即：**memmap/reserved 两表自己的载荷页就在内核 conventional 空闲清单里**，安全性靠 `store_kernel_info` 尽早深拷（NK4-C 修的是消费面）。vh1 取证（memmap 条目在 VM 分类点读零、DM 覆盖时还完好）正是这批页被 VM bootstrap 分配器拿走覆写的表现。wire 数组若照抄「Vec + leak」模式将继承同一性质——**wire 载荷必须在快照前预分配**。

### A.3 Wire\* 数组的分配/序列化/首读时点（尚无证据——纯设计草图）

`grep -rn "BootHandoffV1\|WireSlice\|WireMemoryRange|WireBootModule|WirePlatformDescSource" os --include="*.rs"` 零命中。当前唯一跨镜像载荷仍是 `BootHandoff`（v2 现状）。候选时序只能给设计意图，不能给证据。

### A.4 建根后原始切片可读性（已证实「无构造保证」）

bootstrap 根内容 = 恒等 [0,4GiB) + 高半内核 + MMIO（`build_bootstrap_root_and_enable`，`os/kernel/src/lib.rs`）。逐块可读性依据：

| 载荷 | 新根下可读？ | 依据 |
|---|---|---|
| root page / bump 区 | **构造保证** | MaxAddress 上界（A.1 表） |
| 内核段体 | 已证实 | LMA 由 `x86_64.ld`/`aarch64.ld` 定在 < 4GiB（`KERNEL_PHYS_BASE = 0x200000` 等），且 `check-layout.sh` 钉 |
| 三块 Vec backing（memmap/reserved/module 数组/platform_sources） | **无构造保证** | AllocatePool 无上界约束；QEMU virt 现状可读是因为 RAM < 4GiB 的**平台拓扑**，不是机制 |
| 模块 blob 内容 | 无构造保证（AnyPages）；但 VM 启动前无人读内容 | `alloc_module_pages` AnyPages；`store_kernel_info` 只拷 12 条 (name,start,len) 三元组 |

**推论**：若 wire 序列化（复制 Wire\* 数组）安排在 `build_bootstrap_root_and_enable` **之后**，序列化时要读的正是这些无构造保证的池页——评审问题 B 成立。**建议裁决方向（证据支持）**：wire 序列化安排在开根**之前**（shim 侧固件映射全域可读时），开根后只填根页/游标/协议头等定宽字段。

### A.5 wire 缓冲区与独立 AllocatePages 页是否可能进 PMM 空闲清单（已证实排除机制）

排除机制**不是**「属于 bump 区」，而是**快照时的内存类型分类**：

- `vm_handoff::classify` 的 deduction 清单 = 内核镜像 + 根页 + bump 区整段 + bootstrap 已用帧 + 12 模块 blob（`os/kernel/src/vm_handoff.rs:fn classify`，L196-230）——只覆盖 MaxAddress 两笔与模块页。
- VM free list = **conventional ∩ DM 可表示 − LiveBootstrap**（vm_handoff.rs 模块头注释与 `fn classify_regions`）。
- `build_memmaps` 只把 `CONVENTIONAL` 放进 free 侧，非 conventional（含 LOADER_DATA）进 reserved 侧 → reserved 只进 DM 覆盖候选（source 4），**永不进 PMM**。

**结论（回答评审问题 C）**：wire 页只要以 LOADER_DATA 类型在**最终快照之前**分配，即被分类排除出 PMM——**不需要在 bump 区内**；快照后分配的页（无论 LOADER_DATA 与否）会落入 conventional 清单（发现 A-β）。bump 区是「MaxAddress 上界 + 整段 deducted」的特例，不是排除机制的必要条件。

### A.6 时序表（现状实测 + wire 方案插入点）

| # | 步骤 | 现状 | wire 方案要求 |
|---|---|---|---|
| 1 | root/bump MaxAddress 分配 | uefi_helpers L154-155 | 不变 |
| 2 | 内核段直写 + 模块 AnyPages 分配 + 模块数组 Vec + platform_sources Vec | L172-188 | **wire 载荷预分配插在此段末**（固定容量，LOADER_DATA） |
| 3 | 最终快照 GetMemoryMap | L193 | 不变（wire 载荷此时已被分类为 LOADER_DATA） |
| 4 | 快照后计算：断言、分类转换、**wire 序列化写入预分配缓冲** | 现状只有断言与 build_kernel_info | 只写不分配 |
| 5 | EBS（内部第二次 GetMemoryMap） | L282 | 两图一致 ⇔ 步骤 4 后零分配 |
| 6 | 建根开根 + 写 blob + 跳转 | bootstrap_to_kernel_image（aarch64） | wire 数据已在低址 LOADER_DATA 区，恒等窗口可读 |
| 7 | 首次读取 + 落地（store_kernel_info） | 镜像侧 resume 第一步 | 不变 |
| 8 | 可回收时点 | LOADER_DATA 区不自动回 PMM（分类排除是永久的） | 同左；回收 = 有意的新裁决（现状无回收机制） |

---

## 任务 B：BootReadPhase 与物理地址读取语义

### B.1 类型存在性（已证实：零代码）

`BootReadPhase` / `KernelInfoView` / `WireSlice` / `WirePhysAddr` / `BootHandoffV1` 全部零命中——**仅有设计草图**，v4 §10 的「已冻结」均指设计文字而非实现。

### B.2 物理地址 → 可读字节的现行转换（已证实：恒等假设在转换点）

现行唯一转换：`core::ptr::write(blob_phys.0 as *mut BootHandoff, …)`（`os/kernel/src/lib.rs:fn bootstrap_to_kernel_image`，L318 写）与镜像侧 `unsafe { &*handoff }`（`os/kernel-image/src/main.rs:fn rust_image_main` aarch64，L291 签名）——**`addr as *const T` 直转**，依赖 bootstrap 根下 VA==PA。评审 P0-2 判定成立：把常量换成区间表不改变「转换点隐含恒等映射」的事实；共享层的读取语义必须是一个**转换能力**而非一张表。

**最小 API 形状建议（任务 B-5）**：

```rust
impl BootReadPhase<'ctx> {
    /// Semantic promise: within this boot-read phase, the validated
    /// physical interval is readable; the returned view is tied to 'ctx
    /// and the leg guarantees no unmapping/reuse while 'ctx lives.
    /// Mechanism (identity window / TTBR0 walk / shim-local mapping)
    /// lives entirely in the leg that constructed the phase.
    pub fn translate(&self, iv: PhysInterval) -> Option<&'ctx [u8]>;
}
```

共享解码器只消费 `translate` 的结果——三架构机制（x86 恒等窗口 / aarch64 同 / 直核腿 satp=0 物理视图）全部留在腿内。

### B.3 各启动形态对跨镜像 handoff 的实际使用（已证实）

| 形态 | 用 `BootHandoff`？ | 证据 |
|---|---|---|
| x86_64 UEFI 内联 | 否（进程内 `arch_boot(&KernelInfo, …)`） | `os/boot-shim/src/main.rs:fn main` L88 |
| aarch64 UEFI 跨镜像 | **是（唯一消费者）** | `main.rs:fn main` L86 → `bootstrap_to_kernel_image`；`os/kernel-image/src/main.rs:fn rust_image_main`（L291，x0 递指针） |
| riscv64 OpenSBI 自举 | 否（bootface 镜像内自举） | `main.rs:fn rust_image_main` L333（a0/a1/a2 三寄存器） |
| aarch64 QEMU 直核 | 否（x0=0 分道走 bootface_a64） | `main.rs` L291 分道 |

### B.4 令牌生命周期与映射有效期的类型保证（已证实：不存在）

现状无任何令牌；映射切换点 = Phase F `switch_to_user` 后每进程 `switch_address_space`。平台发现在 Phase A.5（`os/kernel/src/lib.rs` Phase A.5 段，PlatformContext 初始化）消费 `platform_sources` 原指针——与切根之间只隔**阶段次序**，无类型约束。评审「方向 B（有界 unsafe 阶段）」的前提成立：Rust 类型系统今天不能阻止「令牌被借用时切根」，因为它根本不存在；引入后也只能靠调用顺序收敛（方向 B），除非做映射有效期与借用绑定的完整 typestate（超需求）。

---

## 任务 C：x86 独立镜像入口

### C.1 `_start` 是否需要从 handoff 读栈顶（反证评审担忧——已是二段式）

**已证实：不需要，也不读。** x86 `_start`（`os/kernel-image/src/main.rs` global_asm，L111-138）：清 `.bss`（RIP 相对）→ `lea rsp, [rip + kernel_boot_stack_top]` → `xor ebp, ebp` → `call rust_image_main`。栈是**镜像内部链接脚本预留的 64 KiB .bss 栈**（`os/kernel-image/x86_64.ld`：`kernel_boot_stack_bottom = .; … kernel_boot_stack_top = .;`）。aarch64（L182-184 `mov sp`）与 riscv64 同构——**三架构 trampoline 全部用镜像内 bootstrap 栈，从不信任 blob 字段**。评审建议的「二段式栈策略」是现状，不是待办；blob 里的 `kern_stack_top` 只在 Rust 面验证通过后由 Rust 切换（aarch64 resume 链已如此：`jump_to_kmain(kinfo, kinfo.kern_stack_top)` 在验证与落地之后）。

### C.2 rdi 现状被占用（已证实——NK1 的新具体约束）

x86 `_start` 用 `rdi` 作 .bss 清零游标、`rcx` 作界限；`call {rust_image_main}` 时 `rdi = __bss_end`。且 `fn rust_image_main()`（L263）**无参**——x86 跨镜像入口尚未接收 blob。NK1 实施时：blob 指针**不能沿用 `rdi` 直传**（会被清零循环覆写），需改用固定寄存器（如 `rbx`/`r12` caller 不破坏）或内存槽（跳转前写入镜像 .data 固定槽）——这是 v4 §2.3 入口契约清单没有的具体约束，需补进 T2。

### C.3 协议头验证步骤（已证实现状 + 待实施项）

现状 x86 链无 blob 读取，无验证可谈；aarch64 链 `&*handoff` 裸解引用**先于任何验证**（`main.rs` L291-293 直转引用）——v4 §1.5（三）「先验头后解引用」在现状代码中不成立，是待实施契约。C-1 的四步读取顺序（基本可读性 → 头 → 全字段 → 视图）与 P0-1 的签名矛盾（见「契约修正」节）在此一并裁决。

### C.4 入口状态归属（已证实）

现状：**全部机器状态由各架构 trampoline 建立**（三份独立 global_asm，无共享 trampoline）——`cli`（x86 仅在停机循环 L133，**进入 Rust 前未显式 cli**，依赖 EBS 后固件中断状态——契约未成文）、栈、清零、FP 门（SD-23 compile_error 在 Rust 面）。共享面只有 `rust_image_main` 的语义序列。**尚无证据**：no-red-zone 构建旗标（x86_64 target 的构建配方未查到显式禁用；SD-23 只门 aarch64/riscv64 的 FP）——需实施前核实 `os/.cargo/config.toml` 与 xtask 注入旗标。

**推荐控制流（综合 C.1-C.4，与评审二段式一致且为现状最小改动）**：`_start`（内部栈 + 清零 + 从固定槽取 blob 指针）→ `rust_image_main(blob)` → `decode_view` 四步验证 → 验证通过后由 Rust 读 `kern_stack_top` 并在 `jump_to_kmain` 切正式栈（aarch64 先例）。trampoline 永不触碰 blob 字段。

---

## 任务 D：T4 的 DTB 生命周期（aarch64 直核）

### D.1 DTB 基址与最后访问（已证实）

- 基址：`DTB_PA = DRAM_BASE = 0x4000_0000`（`os/kernel-image/src/bootface_a64.rs:const DTB_PA`）；长度从 blob 头 totalsize 读。
- 产出：bootface_a64 解析 memmap + `platform_sources = [DTB @ DTB_PA]`（L247）。
- 内核侧最后访问：**Phase A.5 平台发现**（`os/kernel/src/lib.rs` Phase A.5——PlatformContext 从 KernelInfo 初始化；`platform_desc()` 懒加载一次后不再触 DTB 原始字节）。此后 DTB 原始区间无读者。

### D.2 区间流向（已证实：无任何排除）

bootface_a64 的 memmap（`fw_end = 0` 不削）把 DTB 所在 DRAM 段报成 conventional → 内核 Phase A.2 只 cut 内核镜像与模块（D-64①）→ `vm_handoff::classify` 的 deduction 不含 DTB → **DTB 区间进 VM free list**。最早复用者 = VM PMM（Phase F 后）。次序（A.5 平台发现 < VM PMM）成立但无结构保证——与 v3 结论一致，本轮补充了完整流向链。

### D.3 C earm 对照（已证实：C 无对应物）

`grep -rn "fdt\|device_tree\|dtb" minix3/minix/kernel/arch/earm/pre_init.c` 零命中。C 侧内核**不持有固件 blob 指针**——板级信息经编译期 board 表（`machine.board_id`）在 boot 期一次取定。Rust 经 `platform_sources` 持 PA、内核重解析，是 `[ARCH]` 演进引入的**新生命周期义务**，C 无先例可对照——临时预留/永久排除的裁决不能引 C 为据。

### D.4 临时预留 vs 永久排除（已证实：现无恢复机制）

现有 memmap API 只有 `add_memmap`/`cut_memmap`（`os/kernel/src/memmap.rs`），cut 是破坏性移除，**无「预留后恢复可分配」通道**；handoff `deducted` 也是单向记录。临时预留需新增 API（成本：一处 cut 变体 + 回收时机语义）。永久排除成本 = DTB 大小（QEMU virt DTB 数 KB 量级，占 128MiB+ RAM 的比例可忽略）。**证据支持的裁决框架**：若永久排除，成本以 KB 计、机制零新增；若临时预留，需新 API 且收益只省几 KB——证据倾向**永久扣除 + 显式成文**（「有意选择」），与评审「成本明显高于损耗则永久可接受」的条件相符。

---

## 任务 E：ARCH / BOARD 的真实语义

### E.1 全部生产者与消费者（已证实）

- 键名：`"arch"` / `"board"`（`minix3/minix/include/minix/const.h:138-139`，`ARCHVARNAME`/`BOARDVARNAME`）。
- 生产者：C 两架构 boot 期写入（`minix3/minix/kernel/arch/i386/pre_init.c:149-150` 硬编码 `BOARD_ID_INTEL`；`minix3/minix/kernel/arch/earm/pre_init.c:274-275` 由 `machine.board_id` 查表生成）。Rust：无（param_buf 恒空）。
- **消费者（决定性发现）**：**C 内核自己回读 board 键**——`minix3/minix/kernel/main.c:133`：`machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME));`。board 值经 `get_board_id_by_name` 映射回板级 ID，是**内核板级识别的输入**，不是展示字符串。`arch` 键：全树无读者（仅写）。

### E.2 占位值判定（已证实：破坏语义）

评审判断成立且比预期更硬：任意占位 board 名经 `get_board_id_by_name` 查表必然失败（无此板）——占位值不是「语法通过语义无害」，而是**直接破坏 C 内核板级识别**。v4 §3.3「宁可写占位值也不留缺键」**必须废除**。

### E.3 Rust 侧射程（已证实：无回读消费者）

`grep -rn "get_board_id_by_name|board_id" os --include="*.rs"`：仅 `os/kernel/src/misc.rs:311` 与 `os/servers/sched/src/kernel_api/transport.rs:292` 的 **GET_KERNELINFO 通道字段**（C `board_id` 的 wire 对位，按值透传），无板级表、无回读逻辑。即 Rust 内核的板级识别走 `platform_sources`（DTB/RSDP），**不继承 C 的 board 名回读语义**——这是 `[ARCH]` 演进（或待登记缺口）。

**裁决框架据此改变**：`board` 键的义务射程 = 「Rust 是否继承 main.c:133 的回读语义」。不继承 → 键退化为展示信息，真实值来源仍应给（UEFI 腿对位 C i386 硬编码或取自固件表）；继承 → 必须有真实值 + Rust 侧查表。**「显式未知值」只有在先定义 Rust 侧未知语义后才有意义**——两步裁决，不可跳步。

---

## 汇总裁决输入

### 必须修改的设计契约（证据驱动）

1. **P0-1 签名矛盾**：`decode_view(blob: &BootHandoffV1, …)` 预先形成完整引用，与 v4 §1.5（三）「头验证前不得形成引用」矛盾（§1.4 vs §1.5（三）自相矛盾）。改：入口收原始地址/裸字节，四步分段验证（基本可读性 → 头 → 全字段+载荷描述符 → 构造视图）；B.2 的 `translate` 与四步合并设计。
2. **wire 枚举与 newtype 布局**：wire 判别值以裸 `u32` 接收、验证后转 Rust 枚举（`#[repr(u32)]` 不使任意 u32 成为有效判别值）；`WirePhysAddr`/`WireVirtAddr` 需 `#[repr(transparent)]` 写进协议。
3. **wire 载荷预分配**：发现 A-β 证实「快照后分配的页进 conventional 空闲清单」（`build_memmaps` 自身即此形态，注释自陈）。wire 载荷（含为 memmap 内容预留的固定容量缓冲）必须在最终快照**前**以 LOADER_DATA 分配；快照后只写不分配。排除机制 = 类型分类（A.5），不要求进 bump 区——v4 §1.5（六）的「倾向沿用 bump 记账」改为「分类排除 + 任意 LOADER_DATA 区」。
4. **序列化时点**：wire 序列化安排在 `build_bootstrap_root_and_enable` **之前**（A.4 证实池页可读性无构造保证）；开根后只填定宽字段（根页/游标/协议头）。v4 §2.2 示例顺序（建根后才组 blob）需同步改。
5. **T2 入口契约补两条**：blob 指针不能走 `rdi`（.bss 清零游标占用，C.2）；trampoline 不读 blob 字段（C.1 已是现状，成文为契约而非候选）。
6. **生命周期状态降级**：v4 §10「`KernelInfoView` 生命周期已冻结」改为「方向已定（有界 unsafe 阶段，方向 B）；类型系统不能阻止借用到期的映射切换（B.4），正确性由实现边界的调用顺序契约承载、并在解码/落地边界以 debug 断言辅助」。
7. **ARCH/BOARD**：废除「宁可占位」；board 键改两步裁决（先裁 Rust 是否继承 main.c:133 回读语义，再裁值来源）；arch 键在树无读者，义务降为展示兼容。

### 可以保留不变的已裁项

- wire 唯一权威定义（`BootHandoffV1`）、`Wire*` 独立元素类型、`free_upper_idx` 哨兵、`MemoryRegion.len`→u64、协议头三字段、布局门禁两层与跨目标交叉编译检查。
- 三层解码模型（wire → view → 落地）与 `store_kernel_info` 唯一收口。
- T2 方向（独立镜像 + rdi 系交接 + QEMU 先行双轨 + OQ-N6 继承）——仅入口寄存器选择需避开 rdi。
- T3 的 LoadOptions 来源、四步规范化、权威键禁覆盖、整条记录截断、SBI bootargs 实测验收。
- T4 的去重迁移与 `fw_end=0` 形态前提（本报告 D.4 只影响其 DTB 预留策略子项）。
- T5 五块切片落地、容量 4=已核上限 2×余量。
- 实施次序 T4 → T1 → T5 → T3 → T2。

### 任何代码修改之前必须完成的阻塞条件

1. T1：四步验证入口的最终 API 形状 + wire 序列化时序（快照前预分配/开根前序列化）落成文字裁决（本报告 A/B/C 证据已齐，可裁）。
2. T2：blob 交接寄存器选型（避开 rdi）+ x86 no-red-zone 构建旗标核实（尚无证据）。
3. T3：board 回读语义的继承裁决（E.3）。
4. T4：DTB 永久扣除 vs 临时预留的二选一（D.4 证据倾向永久，待裁）。
