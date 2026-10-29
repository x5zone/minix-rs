# 01 · 引导链 v4 追加取证报告（第二轮：EBS helper / 模块 blob / red-zone / env 消费面）· 2026-10-21

> **用途**：四轮评审追加任务的定向取证（只调查不改码）。承接 `01-forensics-round3.md`；每条标 **已证实 / 反证 / 尚无证据**，附路径 + 符号锚点。核心结论先行：**评审的 EBS helper 怀疑在锁定源码中完全坐实**（分配发生在 EBS 调用之前），这推翻了 round-3 报告「两图一致」的前提；模块 blob 存在与 wire 元数据同型的恒等直读缺口；`arch`/`board` 键在 C 用户态零消费者。

---

## A. 重新核实 UEFI 最终内存图（裁决性发现）

### A.1 锁定版本与 helper 真实实现（已证实，推翻 round-3 前提）

`os/Cargo.lock` 锁定 **uefi 0.33.0**。其 `exit_boot_services(memory_type)`（`~/.cargo/registry/src/…/uefi-0.33.0/src/boot.rs:fn exit_boot_services`，L1223-1248）的真实序列：

```rust
pub unsafe fn exit_boot_services(memory_type: MemoryType) -> MemoryMapOwned {
    crate::helpers::exit();
    let mut buf = MemoryMapBackingMemory::new(memory_type).expect("Failed to allocate memory");  // ← L1226：EBS 前的 allocate_pool
    for _ in 0..2 {                                       // 两次重试循环
        match unsafe { get_memory_map_and_exit_boot_services(buf.as_mut_slice()) } {
            Ok(memory_map) => return MemoryMapOwned::from_initialized_mem(buf, memory_map),
            …
        }
    }
    runtime::reset(ResetType::COLD, status, None);
}
```

`MemoryMapBackingMemory::new`（`uefi-0.33.0/src/mem/memory_map/impl_.rs:fn new`，L276-289）：`boot::memory_map_size()` 探测当前图大小 → `boot::allocate_pool(memory_type, map_size + desc_size×8)`（L279，**+8 条目余量**，对位 Linux `EXTRA_ENTRIES=8`）→ 返回裸池指针。`get_memory_map_and_exit_boot_services`（boot.rs L1158-1172）在该缓冲区里 GetMemoryMap → 立即用 MapKey 调 `(bt.exit_boot_services)`，失败则**同一缓冲区重试**（最多 2 次）。

**逐项回答评审问题**：

1. **helper 在内部为最终图分配缓冲区？** 是。分配发生在**第一次 `bt.exit_boot_services` 调用之前**（GetMemoryMap 前必须有缓冲区）。类型 = 调用方传入的 `memory_type`——本仓传 `LOADER_DATA`（`uefi_helpers.rs:fn exit_boot_services` L542）。
2. **缓冲区会被返回或保留吗？** 会——它被包进 `MemoryMapOwned` 随返回值交给调用方；**不释放**（模块文档原文："the allocation will not be freed on drop"）。即这张图的 backing 是 EBS 时点新分配的 LOADER_DATA 页，**它不在内核收到的 conventional 清单里**（保留），但**也不在任何 deduction 记录里**（未记账——见 A.3）。
3. **`UefiMemoryMap.collect()` 的快照缓冲何时释放？** `build_memmaps` 的 `boot::memory_map(MemoryType::LOADER_DATA)`（uefi crate `memory_map()` 同样内部分配缓冲）返回的图缓冲与两张输出 Vec 一样是池分配、永不释放（boot 阶段单次）。**释放不改变内存图**（UEFI `AllocatePool` 改图、`FreePool` 同样改图——但这里从不 FreePool）。
4. **两图差异来源**：内核 memmap（快照 A，L193）与 EBS 时点图（快照 B，helper 内部）之间，调用方零分配——**但 helper 自己分配了快照 B 的缓冲**。逐项差异：快照 B 中，快照 A 的两表 Vec backing、内核段体（不占新页）、模块 blob 均已存在（无差异）；**新增差异仅一份** = helper 自己的 map 缓冲（LOADER_DATA，若干页）。另注意 EBS 重试臂：第一次 MapKey 过期失败 → 第二次循环**不再分配**（缓冲复用），但 UEFI 2.9+ 允许 EBS 失败后再分配——helper 未利用，重试臂内零新增分配。
5. **哪些快照后分配可有意回收**：helper 的 map 缓冲是唯一一块「快照后分配、启动后不再使用」的内存（boot-shim 拿到 `MemoryMapOwned` 后只用其判定成功即丢弃）。它在快照 A 中仍是 conventional（分配发生在快照 A 之后）→ **进内核 conventional 空闲清单** → VM PMM 可复用 → 但它 EBS 后无任何读者 → 被复用无害。这是「无意依赖时序巧合反而安全」的孤例，**必须成文**而非依赖推理。

**裁决输入（两方案成本）**：

- **方案甲（用 helper 返回的最终图为权威）**：`exit_boot_services` 改为捕获返回的 `MemoryMapOwned`，在**预分配固定容量输出缓冲**里完成 conventional/reserved 分类，EBS 后零动态分配。成本：分类代码从 `build_memmaps` 迁移到 EBS 之后；`uefi_helpers` 现状结构（先分类后 EBS）需反转；好处 = 图与 MapKey 天然配对、无「快照过期」类别。**与现有 API 兼容性最高**——helper 已经把最终图交出来了，现状是被丢弃。
- **方案乙（预分配图缓冲 + 自己配对 GetMemoryMap/EBS）**：用 `boot::memory_map_size()` 预分配 LOADER_DATA 缓冲，循环 `get_memory_map` → `exit_boot_services_raw(map_key)`（原始表调用），重试时复用缓冲。成本：绕开 helper 重写重试逻辑（helper 的重试是私有的 `get_memory_map_and_exit_boot_services`，不可直接复用）；好处 = 图时点完全自控、可保证「分类用图 = EBS 用图」。
- **倾向甲**：不重造重试、不新增裸 FFI 面；且方案甲下「快照 A」（`UefiMemoryMap.collect()`）可以整体降级取消——全部分类以 EBS 终图为准，同时消灭发现 A-β（两表 Vec 载荷在快照 A 后分配的问题随快照 A 取消而消失）。

### A.2 时序修正（覆盖 round-3 A.6 表）

round-3 报告「快照后到 EBS 前无任何分配」**不成立**——helper 在 EBS 前分配了 map 缓冲。修正后的裁决性表述：

> 快照 A 与 EBS 之间**存在一次确定性的分配**（helper map 缓冲，LOADER_DATA）。它当前无害的唯一原因是「EBS 后无人读它」；任何把「快照后零分配」当作不变量的设计（含 wire 序列化）都必须改走方案甲/乙的显式配对，不能建立在「无调用方分配」的观察上。

---

## B. memmap 输出缓冲区与 wire 缓冲区的分配方案（逐类台账）

| 缓冲 | 分配者 | 物理范围 | 首读映射 | 末读时点 | 进 VM free list？ | 回收时点 |
|---|---|---|---|---|---|---|
| 快照 A 图缓冲（`boot::memory_map` 内部池） | uefi crate helper | AllocatePool 任意 | 恒等（shim 期） | `build_memmaps` 分类完（EBS 前） | 是（快照 A 报它 conventional）——**但 EBS 后无读者，无害** | PMM 复用即可（现状隐式）；方案甲下此缓冲随快照 A 取消而消失 |
| `memmap`/`reserved` 两表 Vec backing | `Box::leak`（uefi_helpers L403-404） | AllocatePool 任意 | 恒等（shim 期）→ 内核经落地区 | `store_kernel_info` 深拷（内核第一动作） | **是**（A-β：快照 A 报 conventional）——深拷后靠 VM bootstrap 分类扣除间接保护，vh1 取证即此页被覆写的事故 | 深拷完成后可回收；现状无回收机制（整段 LOADER_DATA 区不回 PMM 的记账只覆盖 bump，**不覆盖这些池页**） |
| 模块元数据数组 Vec backing | `Box::leak`（loader.rs） | AllocatePool 任意 | 恒等 | `store_kernel_info` 深拷 | 是（同上） | 同上 |
| 模块 blob 内容 ×12 | `allocate_pages(AnyPages, LOADER_DATA)`（L564-568） | **任意，无上界** | **恒等直读**（见 D 批） | VM/内核装载该模块时（Phase C 起） | 否（LOADER_DATA → reserved → 不进 free list；模块区间还全额 deducted） | VM 完成装载后可回收（C protect.c:450-451 reclaim 语义）；现状 `vm_handoff` 对 VM 模块同样扣除（续-187） |
| `platform_sources` Vec | `Box::leak`（L138） | AllocatePool 任意 | 恒等 | Phase A.5 平台发现 | 是（快照 A 报 conventional）——消费点在切根前，时序巧合 | 平台发现完成后可回收 |
| wire 载荷（设计） | 裁决：快照 A 前预分配 LOADER_DATA（固定容量） | 预分配时定 | 恒等（bootstrap 根恒等窗口） | 镜像侧 `store_kernel_info` | **否**（快照 A 前分配 → 快照 A 已报 LOADER_DATA → reserved 侧） | 落地完成后可回收；随整段扣除记账 |
| EBS helper map 缓冲（快照 B） | uefi crate `MemoryMapBackingMemory::new` | AllocatePool 任意 | 恒等（EBS 前） | helper 内部 GetMemoryMap（EBS 前） | **是**（快照 A 报 conventional）——EBS 后零读者，无害孤例 | PMM 复用即可；**须成文** |
| handoff blob 页 | `boot_pt_alloc`（bump 区内） | MaxAddress 上界内 | 恒等 | 镜像侧解码 | 否（bump 整段 deducted） | 无回收（bump 记账） |

**回答评审 B-4（分类能否零动态分配）**：能，且方案甲就是形态——EBS 终图已在预分配缓冲（helper 分配的），输出两表改写入调用方预分配的固定容量缓冲（容量上界 `MAXMEMMAP` 已存在，`os/kernel/src/memmap.rs`），分类循环纯计算。唯一阻碍是 uefi crate 的 `memory_map()`/`exit_boot_services()` 高层 API 不暴露外部缓冲——需下沉到 `boot_services_raw` 面或贡献/复刻 helper（方案乙）；若采方案甲，分类输入是 helper 已产出的图，输出缓冲预分配即可，不需要绕 crate。

---

## C. BootReadPhase 最小可行 API（回答 P0-2 残余）

现状证据（round-3 B 批已证）：`BootReadPhase`/`KernelInfoView` 零代码命中（纯草图）；现行 PA→字节是 `addr as *const T` 恒等直转（`bootstrap_to_kernel_image` L318 写 blob、`load_elf_into` L438 直读模块 blob）。

**评审四问逐答**：

1. **谁保证引用生命周期覆盖映射有效期？** 现状：无人——恒等窗口的失效点（首次 `switch_address_space`，Phase F）与消费点之间只有阶段次序。
2. **令牌创建后哪些操作切根/复用页面？** `switch_to_user`（Phase F）后的每进程 `switch_address_space`；VM PMM enable 后的帧分配（复用 conventional 页）。二者都在 `store_kernel_info` 与平台发现**之后**——现状安全全靠该次序。
3. **现有调用顺序能阻止视图用于失效阶段吗？** 不能（无类型约束，B.4 已证）。
4. **如何避免源引用逃逸？** 见下。

**最小 API 裁决建议（方向 B 收紧版）**：

```rust
/// 由引导腿在「映射已立、首次切根前」构造；Drop-only 语义：
/// 借用随 &self 作用域结束而终止，无 'static 出口。
pub struct BootReadPhase<'a>(PhantomData<&'a ()>);

impl<'a> BootReadPhase<'a> {
    /// 语义承诺：'a 期间 iv 可读；返回视图生命周期 ≤ 'a，
    /// 且不越过本次调用（返回引用经调用方立即消费，不存储）。
    /// 机制（恒等窗口/TTBR0/satp=0）归构造方腿。
    pub fn translate(&self, iv: PhysInterval<'a>) -> Option<&'a [u8]>;
}
```

三条收紧：① `translate` 返回引用的生命周期取**调用期**（`&self` 借用），不取 `'ctx` 贮存——解码循环「验证一片、读一片、落地一片」逐片推进，不持跨调用视图；② `PhysInterval<'a>` 由腿在构造时对全部载荷区间逐一登记（含模块 blob——见 D 批缺口），**未登记区间无法构造**，杜绝未验证 wire 地址直转；③ 「视图存活期间不得切根/复用」由**实现边界的调用顺序契约 + debug 断言**（存活性桩：`BootReadPhase` 存在时 `switch_address_space` debug-panic）承载——类型系统不承担映射有效期（round-3 已证不可能且评审方向 B 已认可），但存活桩把「借用未到期就切根」从静默 UB 变成显式崩溃。**不建 typestate。**

---

## D. 模块 blob 的地址可访问性（已证实：潜伏缺口，与 wire 同型）

追踪链（评审 D 批四问）：

1. **分配区间**：`alloc_module_pages`（`uefi_helpers.rs:fn alloc_module_pages` L564-568）——`AllocateType::AnyPages, LOADER_DATA`，**无上界**（对照 root/bump 的 MaxAddress 上界）。
2. **首读者与映射**：`os/arch/src/arch/boot.rs:fn load_elf_into`（L437-441）——`core::slice::from_raw_parts(module.start.0 as *const u8, module.len)` **恒等直读**，注释原文："during boot the identity map covers this address range"（L430）。读时点 = 内核 Phase C `arch_boot_proc`（VM 模块）与 VM 装载其余 boot 模块——**bootstrap 根活跃期**。
3. **区间覆盖核查**：恒等窗口 = [0, `BOOT_IDENTITY_MAP_END` = 4GiB)。blob 落点若 > 4GiB（大内存机器 AllocatePages 高地址偏好、或 conventional 上区），bootstrap 根**不含其映射** → VM 装载时直读即缺页。QEMU virt 128MiB RAM < 4GiB 恰好无害——**拓扑巧合，非机制**，与 round-3 A.4 对池页的判定同型。
4. **候选方案**：分配侧加 MaxAddress 上界（对位 root/bump 先例：`bootstrap_alloc_limit` 已存在且语义已裁——「DM 可采上界」）；或腿提供读取能力。**证据强倾向前者**：模块 blob 的直读发生在固定阶段（bootstrap 根恒等窗口活跃期），上界约束使「恒等直读」从拓扑巧合变成构造保证，改动一处分配调用 + 无新抽象。三架构语义统一：直核腿（openssi/bootface）的模块落点本就在 bump 区（已有上界），UEFI 腿补齐后三腿同律。

**连带**：`WireBootModule` 只落地 (name,start,len) 三元组——blob 字节本身不经 wire 传输、留在原页由接收方直读（`store_kernel_info` 不拷内容）。wire 元数据可读 ≠ blob 可读，评审判断正确；且 blob 的消费时点（Phase C 起）晚于 `store_kernel_info`，落地区保护不了它——**blob 的可读性只能靠分配侧上界或读取侧映射，二选一，无第三路**。

---

## E. x86 入口与构建约束

1. **blob 入口寄存器协议**（已证实现状 + 待裁）：`rdi` 被 .bss 清零游标占用（`_start` L122 `lea rdi, [rip+__bss_start]` → 清零循环），`call rust_image_main` 时 `rdi=__bss_end`。候选：`rbx`/`r12`（callee-saved，清零循环不碰）或固定内存槽。倾向 callee-saved 寄存器（无内存序顾虑，SysV 下 `rbx` 保留语义明确）。
2. **栈**（反证成立，维持 round-3）：`lea rsp, [rip+kernel_boot_stack_top]`（L130）——镜像内 64 KiB .bss 栈，二段式即现状。
3. **入口状态核实**：`cli` 仅在停机循环（L133-135），**进 Rust 前未显式 cli**（依赖 EBS 后固件中断状态）——v4 §2.8 的「显式 cli」仍是待实施契约，现状无。`cld` 现状无。栈对齐：`lea rsp, [top]` 对齐取决于链接脚本布局（`kernel_boot_stack_top` 收口处 16B 对齐需实施时以 `check-layout.sh` 验证）。CR3/长模式由 shim 侧建立（aarch64 先例同构）。
4. **no-red-zone（尚无证据 → 本轮反证补齐）**：`os/.cargo/config.toml` 的 `[target.x86_64-unknown-uefi]` rustflags = `["-C","target-feature=-sse,-sse2,-mmx"]`（C12/SD-41 修复），`[target.x86_64-unknown-none]` = `["-C","relocation-model=static","-C","code-model=large"]`——**两处均无 `-Z red-zone=no`**（且 red-zone 旗标是 nightly `-Z`，稳定版工具链下 `efi` 目标的默认 red-zone 行为 = **启用**）。x86_64-unknown-uefi 目标默认 abm 含 red zone；内核在 `kernel_boot_stack_top` 栈上运行时，中断/异常（trampoline 阶段无 IDT，风险窗口 = trampoline 到 IDT 建立之间——此窗口内无中断投递则 red zone 无实害；但 IDT 建立**之前**若发生异常（清零循环不应异常），red zone 被硬件压栈覆盖属 UB）。**裁决输入**：x86 内核目标需补 `-Z red-zone=no`（对位 Linux 内核 `-mno-red-zone`），nightly 旗标或改用 `#[no_red_zone]` 编译属性不可行 → 实际路径 = xtask 对 x86 内核镜像注入 `-Z red-zone=no`（xtask 已对 aarch64 注入 `-C target-feature=-neon,-fp-armv8` 先例，`os/kernel-image/src/main.rs` SD-23 注释）。**登记为 T2 实施前置**。
5. **共享/后端分界**：维持 v4 §2.8——blob 指针传递方式（寄存器选型）与 trampoline 是 x86 后端事务；共享层只表达「入口收到已验证 blob」语义。

---

## F. ARCH / BOARD 的真实消费契约（已证实：C 用户态零消费者）

全树检索结果：

- 生产者：仅 C 内核 boot 期（`i386/pre_init.c:149-150` 硬编码 `BOARD_ID_INTEL`；`earm/pre_init.c:274-275` 板表生成）。Rust 无。
- **C 内核自读**：`main.c:133` `machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME))`——board 键是**内核板级识别输入**（round-3 E 批已证）。
- **C 用户态**：`env_get_param("board")` / `env_get_param("arch")` / `env_get("board")` / `env_get("arch")` **字面量键全树零命中**。`env_get_param` 的通用机制存在（`libsys/env_get_prm.c`——先查 argv，再 GET_MONPARAMS 拉全量 param_buf 找键），drivers/rs232/tty/mmcblk 等调用方查的是各自业务键（`label`、`pci_bus` 等），无人查 arch/board。**通用 API 的存在 ≠ 具体键的消费**。
- Rust 侧：无 `get_board_id_by_name` 等价物；`board_id` 仅作 GET_KERNELINFO 通道透传字段（`misc.rs:311`、`sched` transport.rs:292），值来源 = 内核 `platform_desc()`（DTB/RSDP），与 param_buf 的 board 键**无耦合**。

**裁决输入（评审 F-4 的三种路径落到各腿）**：

- **内核内部平台识别**：Rust 已由 `platform_sources`（DTB/RSDP）承担——C 的「board 名 → board_id 回读」语义**不被继承**（Rust 无板级名表）。param_buf 的 board 键与内核识别**解耦**。
- **用户态环境变量协议**：C 消费面 = 通用 `env_get_param` 机制 + 零 arch/board 字面量调用者。Rust 用户态 env 层（未重写）将来若复刻该机制，arch/board 只是「可能被未来脚本/调试工具查询」的展示数据。
- **结论**：**board 键降级为展示信息是证据支持的**（两消费面——C 内核回读与 C 用户态字面量查询——Rust 均不继承）；`arch` 键本就全树无读者（round-3 已证），同样降级。值来源仍要求**真实**（UEFI 腿 ARCH=目标架构常量；BOARD 值来源实施时取证——对位 C 的板表或固件表），**禁止任意占位**的理由从「破坏 board_id 识别」修正为「展示数据也必须真实，假数据误导调试」（较弱但成立）。
- v4 §3.3 的「不允许静默缺键——宁可写占位值」**废除**，改为：「键生成保留（C 协议形状兼容），值为真实值或显式省略该键；键缺失的语义 = `find_key` 返回 NULL（C 消费方既有错误臂），无 Rust 侧义务填充」。若未来 Rust 用户态出现真实 board 查询需求，届时立项。

---

## G. DTB 永久扣除的页边界（已证实：粒度机制在通用层已有）

1. **DTB 基址/长度来源**：aarch64 直核 `DTB_PA = DRAM_BASE`（`bootface_a64.rs:const DTB_PA`），长度 = blob 头 `totalsize`（大端 u32）——`bootface_a64.rs` 解析处 + opensbi 腿同款（round-3 A.1）。
2. **cut 粒度**（已证实）：`os/kernel/src/memmap.rs:fn cut_memmap` 与 VM 分类的最终产物是**页对齐区间**——`vm_handoff::classify_regions` 与 VM PMM 的帧化都以 `PAGE_SIZE` 粒度（`vm_handoff.rs` deduction push 前有页对齐断言 L144："deduction start must be page-aligned"）。即：扣除 DTB 时应给**页对齐后的区间** `[page_floor(DTB_PA), page_ceil(DTB_PA + totalsize))`。
3. **末端溢出与边界**：`DTB_PA + totalsize` 需 `checked_add`；页 ceil 对 u64 溢出同理。bootface 侧既有先例：`dtb_pa + total as u64 <= 0x1_0000_0000` 界限闸（bootface.rs:312 / bootface_a64.rs:352）已在读 blob 时防了上溢——cut 侧复用同一算术。
4. **损耗估算**：QEMU virt DTB totalsize 数 KB（典型 2–8 KiB）→ 页 ceil 后最多 1–2 页（8–16 KiB，128MiB RAM 的 ~0.01%）。**永久扣除成立**（机制零新增：Phase A.2 一笔 `cut_memmap(page_floor, ceil - floor)`，对位 D-64① 内核镜像自切）。
5. **通用性**：按评审要求——实现为「引导腿把页对齐预留区间经 `cut_memmap`/deducted 交给通用机制」，PMM 不出现 DTB 字样。**注意 A-β 教训**：cut 的参数必须**页对齐后**传入，否则 cut 不掉与 DTB 相交的边缘页（cut_memmap 语义 = 精确区间移除，非页化——实施时先验证 cut 对非页对齐参数的行为，必要时在腿侧先对齐再调）。

---

## 汇总

### 新发现 P0/P1

| # | 级别 | 发现 |
|---|---|---|
| P0-A | P0 | **EBS helper 在 EBS 前分配 map 缓冲**（uefi 0.33.0 `boot.rs:fn exit_boot_services` L1226 → `MemoryMapBackingMemory::new` L279 `allocate_pool(LOADER_DATA)`）——「快照后零分配」前提作废；该缓冲在内核 conventional 清单内（EBS 后零读者，现状无害孤例，须成文）。裁决方案甲（EBS 终图为权威 + 预分配输出缓冲 + 快照 A 取消）优于乙（自管配对），同时消灭 A-β。 |
| P1-D | P1 | **模块 blob 恒等直读无上界保证**（`alloc_module_pages` AnyPages + `load_elf_into` L438 直读）——大内存机器 blob 落 > 4GiB 则 VM 装载缺页。修法：分配改 `MaxAddress(bootstrap_alloc_limit(...))`（先例 root/bump 已有），三腿同律。 |
| P1-E | P1 | **x86 内核目标未禁 red zone**（`.cargo/config.toml` 两 target rustflags 均无；uefi 目标默认启用）——trampoline 到 IDT 建立之间的异常窗口内 red zone 被压栈覆盖属 UB。修法：xtask 对 x86 内核镜像注入 `-Z red-zone=no`（SD-23 aarch64 FP 门先例）。登记 T2 前置。 |
| P2-F | P2 | arch/board 键 C 用户态**零字面量消费者**（`env_get_param("board"/"arch")` 全树零命中）——两键降级为展示信息证据成立；v4「宁可占位」废除，改为「真实值或显式省略」。 |
| P2-G | P2 | DTB 永久扣除成立（损耗页化后 ≤ 2 页）；实现 = 引导腿页对齐后 cut_memmap，通用 PMM 无 DTB 字样。 |

### 可维持不变的既有裁决

wire 唯一权威（`BootHandoffV1`）、`Wire*` 递归闭合、`free_upper_idx` 哨兵、`MemoryRegion.len`→u64、协议头三字段、布局门禁两层、三层解码模型、`store_kernel_info` 唯一收口、T2 方向与 OQ-N6 继承、T3 LoadOptions 来源 + 四步规范化 + 权威键禁覆盖 + 整条记录截断、T4 去重迁移与 `fw_end=0`、T5 五块落地 + 容量 4、实施次序。

### 状态表条目修改建议

- T1「wire 数据存储」：从「v4 已补（闭环）」改为「**分配次序契约需重写**——快照 A 取消与否随方案甲/乙裁决定；A-β 与 EBS helper 缓冲两事实并入」。
- T1「映射有效性」：维持「语义接口已裁」，追加「`translate` 返回引用取调用期 + 未登记区间不可构造 + 存活 debug 桩」三条 API 收紧（§C）。
- T2 入口契约：追加两条前置（blob 寄存器避开 rdi、`-Z red-zone=no` 构建注入）。
- T3：`ARCH/BOARD` 行改为「C 用户态零消费者已证；两键降级展示信息，真实值或显式省略，占位禁令维持」。
- T4：追加「页对齐区间 cut」与「DTB totalsize 上溢检查（复用既有界限闸算术）」两验收点。

### 代码实施前仍未满足的阻塞条件

1. **T1**：方案甲/乙裁决（本报告 A.1 已给成本对比，可裁）→ 随后 wire 预分配时序按裁决重写。
2. **T1**：四步验证入口 + `translate` 调用期引用的最终签名（§C 已给形状，可裁）。
3. **T2**：blob 寄存器选型（rbx/r12 vs 内存槽）+ xtask red-zone 注入配方验证（nightly 旗标在仓库工具链的可用性）。
4. **T4**：`cut_memmap` 对非页对齐参数的行为验证（G.5）——若破坏性移除精确区间，腿侧先对齐再调。
5. **T3**：无阻塞（降级裁决证据已齐）；仅 BOARD 真实值来源（对位 C 板表）在 UEFI 腿实施时取证。
