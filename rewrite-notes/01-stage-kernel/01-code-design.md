# 01 · 引导链设计裁决简报（自包含，供多轮外部评审）· v2

> **本文的用途**：`01-stage-kernel` 引导链里每个「需要设计与裁决才能动码」的 TODO，逐条写成自包含简报。目标读者是**看不到本仓库的第三方评审**（网页端 GPT）：每个论断所需的代码、C 源对照片段、既有裁决原文都已内嵌，不需要任何仓库内引用就能完整理解问题与候选方案。
> **与仓库的关系**：每节标题括号里的编号是 `01-todo.md` 的条目号；裁决结论回填 `01-todo.md` 对应条目后由仓库侧实施。已有专门设计文件的主题**不进**本文（大页能力收口 → `boot-dm-hugepage-design.md`，独立线程迭代中）。
> **版本记录**：v2（2026-10-21）按外部首轮评审裁决重写——T1 改题（wire 协议冻结为主问题，信封合一降为次级）、T2 补完整入口状态契约与源数据映射两证明、T3 的 UEFI 取参来源修正为当前镜像 `LoadOptions`、T4/T5 定案、新增跨主题 wire 规则 / 实施次序 / 三条硬性验收条件。各条目内的裁决项标 **【已裁】**，仍开放的标 **【待裁】**。
> **v3（2026-10-21 二轮评审，定向增补不改结构）**：四个针对性裁决——① T1 解码边界改三层模型（wire → `KernelInfoView<'a>` → 内核自有），废弃会伪造 `'static` 的 `decode() -> KernelInfo`；② 源切片可读性给出具体机制（UEFI 池地址上界三层：AllocateMaxAddress 构造 / pre-EBS 断言 / 解码重验）；③ wire 布局递归闭合（`Wire*` 元素类型，`BootModule.name` 定长字节槽）+ `BootSlice.ptr` 改 `addr: u64` 物理地址语义；④ T4 补 DTB 生命周期前提。T2 双契约拆分、入口清单细化；T3 的 ARCH/BOARD 语义归属经仓库取证收窄；状态收紧表见 §10。
> **v4（2026-10-21 三轮评审，定向增补不改结构）**：原则级修正——v3 把恒等映射机制写进了共享解码规则，违反「抽象语义、不描述机制」第一原则。五项：① `BootSlice<T>` 改非泛型 `WireSlice`（unused type param 编译错）+ `byte_len` 字节口径 + 溢出检查区间公式；② 「wire 全部地址都是物理地址」收窄为逐字段声明（`WirePhysAddr`/`WireVirtAddr`）；③ 可读性判据改语义接口（`BootReadPhase.readable_intervals`），机制归腿——依赖方向佐证：`minix-boot` 只依赖 `minix-types`，本就不该命名 `minix-arch` 的常量；④ wire 序列化与 UEFI 快照/EBS/回收闭环（T1 冻结条件 +1）；⑤ `reserved_regions` 消费面取证：两消费点两语义，T2 按消费语义拆分输入。
> **背景速览（三结构体一句话版）**：`KernelInfo` 是引导期语义载荷本体（内存图、内核位置、启动模块……，对应 C 的 `kinfo_t`）；`BootPrepareResult` 是 `BootShim::prepare_boot` 的**进程内返回值信封**（KernelInfo + 根页 + bump 区间，不跨二进制）；`BootHandoff` 是 aarch64 跨镜像跳转的 `#[repr(C)]` **线格式载荷**（KernelInfo + 根页 + bump 区间 + 游标，按裸指针解引用）。前两个的设计有历史可查；`BootHandoff` 是 aarch64 方案 A 跑通途中的救火产物，留下「两信封 90% 重叠」的债——本文 T1 收这条。其余四节（T2–T5）是 x86 跳转协议、启动参数通道、直核腿第四链、内核落地区，各有独立问题。

---

## 0. 跨主题总规则与实施次序（首轮评审新增）

### 0.1 wire 数据的生命周期规则 **【已裁】**

所有跨镜像数据遵循同一条规则：**wire 数据先验证，确认映射可访问后，才解码为内核内部语义对象**。wire 里的字节来自另一个镜像的写入方，即使生产者可信，接收方的类型系统也不应该让一段未经校验的字节轻易变成安全引用。这条规则是 T1（wire 类型与解码边界）、T2（首次读取前的映射契约）、T5（落地区 = 解码终点）的共同基础。

### 0.2 五项依赖与建议实施次序 **【已裁】**

五项不是五个独立 TODO，实施次序按依赖排列（「先做」不等于「必须严格串行」——T4 完全独立可先行；T1 的 wire 契约与 T5 的存储接口可先联合设计再分批实施）：

```
T4  aarch64 直核内存图去重        —— 独立项，最先实施
T1  冻结跨镜像 wire 协议与布局     —— 先定原始字段/指针长度表示/哨兵/验证边界，
                                     再决定进程内结果类型去留
T5  五块切片的内核落地区           —— 为所有后续引导形态建立载荷生命周期不变量
T3  真实参数生产侧接入             —— 依赖 T5 落地区 + 统一参数格式 + 必备键 + 截断语义
T2  x86 切换独立镜像               —— 最后收口交接/首次读取映射/入口状态/游标恢复/根切换
```

不把 T1/T2/T3/T5 捆成一个大实现批次：四者共享概念但验证边界不同，拆批利于定位故障，也符合本项目在引导与内存问题上的取证方式。

**v3 补充（二轮评审最终建议）**：不推翻次序，收紧门槛——T4 独立先行实施；T1 的解码边界（§1.4 三层模型）与 T2 的映射证明（§1.5（四）+ §2.8）先落成可审查的短设计补充（本文即承载），再按 T5 → T3 → T2 推进；不让两个未闭合的安全条件（伪造 `'static`、未证明的首读可读性）成为后续实现的错误假设。

---

## T1（D-67 / D-69 / D-71 派生）：跨镜像 wire 协议冻结与布局钉死

> **v2 改题说明**：v1 把「信封合一（删 `BootPrepareResult`）」当首要目标。首轮评审改判：固定布局与减少结构体数量之间没有必然联系——「信封是否合一」降为次级问题；主问题是**如何建立单一权威的跨镜像 wire 协议、如何把 wire 字节安全地恢复成内核内部语义对象**。已裁裁决原文：「统一跨镜像协议的唯一权威定义，固定全部跨镜像布局；进程内结果类型是否保留，按语义边界决定，不以减少结构体数量作为独立目标。」

### 1.1 现状：三个结构体的完整定义与分工

**`KernelInfo`**（`os/libs/minix-boot/src/kernel_info.rs`，`#[repr(C)]`，跨引导形态共享的语义载荷）：

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],          // 可用物理内存清单
    pub reserved_regions: &'static [MemoryRegion],// 运行期仍要经低地址访问的占用区
    pub kern_virt_base: VirBytes,                 // 内核链接高半基址
    pub kern_phys_base: PhysBytes,                // 内核物理装载基址
    pub kern_size: u64,                           // 内核镜像字节数
    pub free_upper_idx: Option<usize>,            // 恒等+内核映射后首个空闲页目录下标
    pub user_sp: VirBytes,                        // 用户栈顶
    pub kern_stack_top: VirBytes,                 // 内核初始栈顶
    pub syscall_entry: VirBytes,                  // 系统调用入口
    pub boot_modules: &'static [BootModule],      // 12 个启动模块（PM/VM/VFS/RS…）
    pub bootstrap_start: PhysBytes,               // C 时代 unpaged 段（Rust 恒 0）
    pub bootstrap_len: u64,                       // 同上（Rust 恒 0，validate 强制）
    pub platform_sources: &'static [PlatformDescSource], // DTB/RSDP 固件描述符来源
    pub param_buf: &'static [u8],                 // C 的 param_buf[1024]（当前恒空）
}

#[repr(C)]
pub struct MemoryRegion { pub base: PhysBytes, pub len: usize }   // ← len 是 usize
```

**`BootPrepareResult`**（`os/libs/minix-boot/src/boot_shim.rs`，无 `#[repr(C)]`）：

```rust
pub struct BootPrepareResult {
    pub kernel_info: KernelInfo,
    pub root_page: PhysBytes,
    pub bump_base: u64,   // 引导期页表页分配池 [base, end)
    pub bump_end: u64,
}

/// 模块注释自称（原文）：
/// "It is the **only** data crossing the boot-shim → kernel boundary."
```

**`BootHandoff`**（`os/libs/minix-boot/src/handoff.rs`，`#[repr(C)]`）：

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoff {
    pub kernel_info: KernelInfo,
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,   // ← 比 BootPrepareResult 多的字段：blob 写完后的池游标
    pub bump_end: u64,
}
const _: () = assert!(core::mem::size_of::<BootHandoff>() <= 4096);
```

分工实况：

- `BootPrepareResult` 只在**同一个地址空间**里被消费：UEFI 内联腿（x86_64，内核静态链进 boot-shim 的 PE）里 `main()` 拿到它，取字段调 `arch_boot(&result.kernel_info, result.root_page)`。bump 游标不进它，因为同空间下游标活在内核可读的全局里。
- `BootHandoff` 只在 aarch64 跨镜像腿被消费：boot-shim 建好页表开分页后，把它写进一个 bump 分配的物理页，绝对跳进独立 `kernel.elf` 的高半 `_start`，`x0` 递 blob 物理指针。接收方链接的是**自己的另一份** `minix_kernel` 副本，`.bss` 不共享，所以 bump 游标必须进载荷。
- 两个信封表达的不是同一件事：**进程内准备结果**（构建完成后当前镜像可直接消费的数据）与**跨镜像交接协议**（接收方必须把一段物理内存当固定格式载荷来解释）。字段大量重叠不代表必须是同一种 Rust 类型——内部语义对象与外部传输格式不同，正是值得由类型表达的边界。这是首轮评审对 v1 方案甲的核心修正。

### 1.2 布局钉死的漏洞（现状自陈，v1 诊断经评审确认成立）

`KernelInfo` 的 `#[repr(C)]` 只钉字段顺序与 POD 字段偏移，模块注释原文承认：

```text
// NOTE: the `&'static [T]` fields' internal (ptr, len) order is still
// toolchain-defined; the cross-image contract therefore relies on both
// images being produced by one `xtask image` run (identical rustc +
// profile) — see `handoff` module doc.
```

即跨镜像正确性靠**工序纪律**（两个镜像出自同一次构建），不靠结构保证。Rust Reference 对 `repr(C)` 的保证范围是：字段顺序、对齐与填充遵循 C 布局规则；但**不能据此推导内部的 Rust 类型（尤其引用胖指针）也有完整稳定的布局保证**——胖指针成员顺序目前的具体表示不构成可依赖的通用 ABI 承诺。未钉死的类型自由度：

1. 五个切片字段（`memmap`/`reserved_regions`/`boot_modules`/`platform_sources`/`param_buf`）的内部 `(ptr, len)` 顺序；
2. `free_upper_idx: Option<usize>` 的判别式/niche 布局——`Option<usize>` 是零成本优化目标，布局完全 toolchain-defined，比切片更隐蔽；
3. `MemoryRegion.len: usize`（跨镜像 blob 里宽度随 target 变）vs `kern_size/bootstrap_len: u64`——同一份 `#[repr(C)]` 载荷里同一逻辑概念两种宽度。

### 1.3 已裁项 **【已裁】**

| # | 裁决 | 内容 |
|---|---|---|
| 1 | 切片 wire 类型 | 采用专用 `BootSlice<T>`（否决更通用的 `CPtrLen<T>` 命名——理由不是风格：这个类型表达的是「启动交接载荷中的切片描述符」，不是任意场景可安全使用的 C 指针包装器） |
| 2 | `free_upper_idx` wire 表示 | 显式 `u64` + `u64::MAX` 哨兵（否决为它引入 C 风格 enum 布局——避免从外部字节构造非法枚举判别值的问题；索引语义若将来复杂化再升级协议版本） |
| 3 | `MemoryRegion.len` | `usize` → `u64`（与 `kern_size`/`bootstrap_len` 同宽；64 位目标上是一轮语义不变的类型统一，将来加 32 位目标不再出现协议字段宽度随目标改变） |
| 4 | 协议头 | wire 格式加轻量协议头（`magic`/`version`/`size`），接收端**先验证协议头再解释后续字段**。收益不是支持混版本镜像（同批构建要求不变），而是产物混用或布局意外变化时尽早报错，不进内核才炸 |
| 5 | 布局验证层级 | 布局断言（`size_of`/`align_of`/关键字段 `offset_of`）必须在**各受支持架构的交叉编译检查**中运行，而不是只在宿主机；作为构建门禁而非文档里的布局表 |
| 6 | ptr 宽度 | `BootSlice.ptr` 宽度随目标架构变**不算缺陷**：当前是同一目标架构下的两个镜像交换，要钉死的是同目标架构内的字段布局，不是宣称一份 wire blob 跨 32/64 位传递（v3 收口：ptr 改定宽 `addr: u64` 后此自由度整体消失，见 §1.5（二）；本行原则仍是协议版本演进的边界） |

`WireSlice` 的 wire 形状与验证要求（v4 定形，替代 v3 的 `BootSlice<T>` 草图）：

```rust
#[repr(C)]
pub struct WireSlice {
    addr: u64,      // physical address of the payload — same semantics as root_page/bump_*
    byte_len: u64,  // payload size in BYTES (not element count)
}
```

v3 草图两个问题在此收口：① 泛型参数 `T` 不出现在任何字段中，照抄会触发 unused type parameter 编译错误——改为**非泛型 `WireSlice`**，元素类型在验证与转换点显式给定；② `len` 的单位未定义。裁决：**wire 长度承载字节数**——消除「元素数 × size_of」乘法溢出面，元素计数由 `byte_len / size_of::<Wire元素>()`（检查式整除）在验证后恢复；字节口径同时让 `WireBootModule`（16B 定长名槽）这类宽元素无需特例。

区间验证公式（共享解码边界，全部溢出检查）：

```text
end = addr.checked_add(byte_len)?              # ① 区间末端不溢出
assert!(addr % align(Wire元素) == 0)           # ② 元素对齐（非空载荷时）
assert!([addr, end) ⊆ phase.readable_intervals) # ③ 落在上下文声明的可读区间内（§1.5（四））
assert!(byte_len <= 该载荷的协议上限)           # ④ 每字段协议上限（§1.5（六））
```

**wire 读出的 `addr`/`byte_len` 不得未经验证就转成 Rust slice**（§0.1 规则的落地）。v3 的「`addr < BOOT_IDENTITY_MAP_END`」判据从共享规则中移除——那是恒等映射机制的表述；且 `BOOT_IDENTITY_MAP_END` 住 `minix-arch`（`os/arch/src/lib.rs:pub const BOOT_IDENTITY_MAP_END`），而 `minix-boot` 的依赖清单只有 `minix-types`：共享解码器本来就不该能命名这个常量。可读性判据改由语义接口承载（§1.5（四））。

`free_upper_idx` 哨兵的配套前提（裁决附带条件）：有效的页目录索引永远不可能等于 `u64::MAX`；解码时先识别哨兵、再做合法索引范围检查；转 `usize` 用检查式转换；文档写明 `None` ↔ 哨兵的编码对应关系。

`MemoryRegion.len` → `u64` 的配套边界检查（裁决附带条件）：u64 长度到宿主索引/slice 长度的转换、`base + len` 溢出、内存图裁剪与合并、VM 内存分类与物理帧计数/位图大小计算——当前均为 64 位目标，大概率语义不变，但边界点逐一过。

### 1.4 解码与落地的三层模型（v3 二轮 P0 裁决，原「进程内信封去留」的解答）**【已裁框架；生命周期细节冻结为实施门槛】**

v2 的形状一草图（`decode() -> KernelInfo`）有生命周期伪造问题：`KernelInfo` 的切片字段是 `&'static [T]`，而源数据来自 boot-shim 的 UEFI 池堆，首次进程根切换后原映射不保证存在——wire 解码器把地址直接恢复成 `&'static [T]`，等于让类型声称「永久有效」；调用方即使承诺马上复制，也改变不了类型表达的生命周期。这与 T5 的落地区规则（源切片不得作为内核永久数据来源）直接矛盾。裁决：解码边界改三层，**不经由 `'static`**：

| 层 | 类型 | 生命周期语义 |
|---|---|---|
| wire 层 | `BootHandoffV1`（+ §1.5（一）的 `Wire*` 元素类型） | 固定布局原始字节：`BootSlice.addr: u64` 物理地址、定宽数值、协议头。不暴露任何安全引用 |
| 视图层 | `KernelInfoView<'a>` | 已验证的启动视图。仅在源数据确实可访问的阶段存在（bootstrap 根活跃、首次切根之前），生命周期诚实绑定该阶段，不伪装 `'static` |
| 内核层 | `KernelInfo`（内核自有） | `store_kernel_info` 深拷五块切片后构造；切片指针全部指向内核落地区（T5），供之后全部阶段使用 |

示意 API（边界形状，非最终签名）：

```rust
impl BootHandoffV1 {
    /// Decode the wire blob into a validated boot view. SAFETY contract:
    /// blob 位于 `phase` 声明的可读区间内且满足对齐；`BootReadPhase<'ctx>`
    /// 是「启动读取阶段」令牌——由引导腿在「映射已立、尚未首次切根」的
    /// 窗口内构造，携带 readable_intervals（§1.5（四））。视图生命周期
    /// 绑定令牌借用而非 blob 引用：源切片可位于与 blob 无关的独立分配页。
    unsafe fn decode_view<'ctx>(
        blob: &BootHandoffV1,
        phase: &BootReadPhase<'ctx>,
    ) -> Result<KernelInfoView<'ctx>, DecodeError>;
}

/// Landed copy: five slices repointed into kernel-owned storage.
fn store_kernel_info(info: &KernelInfoView<'_>) -> Result<KernelInfo, StoreError>;
```

**实施门槛（生命周期细节，冻结前收口）**：源切片不一定与 blob 同一分配区——视图生命周期**绑定 `BootReadPhase` 令牌**（代表实际数据有效期），不绑定 blob 引用（v4 三轮修正：v3「绑定输入引用」的表述不精确，§9.4 同步）；安全 API 不提供任何转 `'static` 出口；深拷完成后后续内核逻辑只读落地数据；必要时用调用顺序约束防止「源视图尚在使用时切换映射」。不为此建庞大的类型状态体系——API 表达的安全前提与真实启动时序对齐即可。剩余细节：`BootReadPhase` 的构造点与宿主测试构造途径、`KernelInfoView` 整体 vs 逐字段视图。

**T2 resume 流程审计结论（二轮质询，仓库侧已核）**：内存相关消费面已读落地副本——`arch_boot`/`arch_boot_resume_high_half` 在 `store_kernel_info` 之后经 `crate::kernel_info()` 取全局副本，`establish_boot_dm(kinfo, root_page)`、kmain、VM 交接分类都消费该副本。但两个未落地字段是例外：`platform_sources` 的全局 getter 返回的仍是指向 UEFI 池堆的原指针（平台发现在首次切根前消费——时序巧合），`param_buf` 消费点在 syscall 期。这正是 T5 要消除的混合状态；三层模型落地后 `store_kernel_info` 成为「解码 → 落地」唯一收口。

### 1.5 v3 增补裁决（二轮评审：递归闭合 / 地址语义 / 可读性机制）

**（一）wire 布局递归闭合**：固定切片描述符不等于固定被描述数据的布局。规则成文：

> wire 布局必须递归闭合。固定 `BootSlice<T>` 的同时，必须固定其全部元素类型与嵌套字段的 wire 表示；不得让未固定布局的 Rust 类型仅凭被 `BootSlice<T>` 包装，就成为跨镜像协议的一部分。

仓库侧取证（三个元素类型各有缺口）：`MemoryRegion`（`PhysBytes` + `usize`）字段简单但 `PhysBytes` 是 newtype，其 repr 需显式钉或 wire 侧改裸 `u64`；`BootModule.name: &'static str` **含引用**——字符串载荷需定长字节表示（wire 侧用 `[u8; 16]` NUL 填充定长名槽，恰与内核 `BOOT_MODULE_NAME_STORE` 的 16B strlcpy 池、bootface `NAME_POOL` 三方同形）；`PlatformDescSource = (PlatformDescKind, PhysBytes)`（`os/libs/minix-boot/src/platform.rs:pub struct PlatformDescSource`）中 `PlatformDescKind` 是 Rust 默认布局枚举——判别值宽度与表示 toolchain-defined。实现方式二选一，**裁决方式一**：

1. 独立 wire 元素类型（`WireMemoryRegion`/`WireBootModule`/`WirePlatformDescSource`）：字段全定宽、枚举判别用 `#[repr(u32)]` 或裸 `u32`、名字用定长字节槽；显式转换到内核内部类型。ABI 约束与领域模型分离。
2. 证明现有元素类型已满足完整 wire 布局并用断言钉死——但 `BootModule.name` 的间接引用与 `PlatformDescKind` 的枚举布局使方式二同样要改类型定义，改造量与方式一相当而边界混合，不如方式一干净。`WireBootModule.name` 的 16 字节槽位是**协议上限**（模块名的 wire 表示规格），不是「恰好等于内核 `.bss` 名字池宽度」的实现巧合——超长名的验证与截断规则归协议契约（与 C `store_kernel_info` 的 15+NUL strlcpy 语义对齐）；内核落地区宽度将来若变，wire 协议不随之变。

**（二）地址空间语义逐字段声明（v4 收窄，纠正「wire 全部地址字段都是物理地址」的过宽表述）**：`KernelInfo` 本就混有两类地址——`kern_phys_base`/`root_page`/`bump_base`/`bump_next` 是物理地址，`kern_virt_base`/`user_sp`/`kern_stack_top`/`syscall_entry` 是虚拟地址；同为一个 64 位数不代表同一种地址。裁决：**物理语义字段** = `WireSlice.addr`、根页、bump 区间；**虚拟语义字段** = 内核虚拟基址、用户栈顶、内核栈顶、系统调用入口，各自保留明确的虚拟地址语义；**任何地址字段不得依赖未声明的地址空间解释**。实现用显式 wire 类型表达边界——`WirePhysAddr(u64)` / `WireVirtAddr(u64)` 两个定宽 newtype（关键是类型边界忠实表达语义，两个新类型本身不是目的）。消费侧对应关系：物理字段经 §1.5（四）的可读性接口验证后转裸指针；虚拟字段按接收方内核映射解释（高半窗口），不做恒等转换。这是「抽象语义、不描述机制」的直接体现。

**（三）布局门禁两层 + 协议头验证顺序**：布局门禁从「单条 assert」升为两层——① **协议固定值**：每个受支持目标上，wire 类型（含全部 `Wire*` 元素类型）的 size/align/关键字段 offset 都有确定基准值，交叉编译检查逐目标比对（`assert!(size_of::<BootHandoffV1>() <= 4096)` 只证明装得进页，不证明字段没挪位）；② **版本演进规则**：字段布局或语义变化必须显式升级协议版本，「恰好同批构建」不豁免变更记录。协议头 `magic`/`version`/`size` 有价值但不能代替布局检查。接收端读取顺序成文：先验证 blob 地址在已知可访问区间、满足对齐、足以容纳协议头 → 读头字段 → 版本与长度检查通过 → 才把地址按 `&BootHandoffV1` 解释——**头验证之前不得形成任何引用**。

**（四）源切片可读性：机制归腿、判据归语义接口（v4 三轮改判）**：UEFI 规范不承诺池分配落在低地址——「bootstrap 根覆盖 [0,4GiB)」本身不构成「五块切片载荷都可读」的证明；aarch64 的既有实测（池页存活到首次切根）是现象记录，不是保证。但 v3 把「`addr < BOOT_IDENTITY_MAP_END`」写进共享解码规则，是**把 x86/UEFI 腿的恒等映射机制抬进了共享协议**——违反「抽象语义、不描述机制」第一原则（判据三问：把接口名里的硬件名词删掉还能说清承诺吗——「数据可读」能，「恒等窗口」不能）。v4 改判双层：

1. **共享层只承诺语义接口**：`BootReadPhase<'ctx>` 令牌携带 `readable_intervals`（本次启动读取阶段保证可读的物理区间表）；共享解码器的全部判据 = §1.3 区间公式对 `readable_intervals` 求子集。`minix-boot` 不出现任何架构常量——依赖方向佐证：`minix-boot` 只依赖 `minix-types`，命名 `minix-arch` 的 `BOOT_IDENTITY_MAP_END` 需要新增依赖，原则判断与依赖方向在此同向。
2. **机制归各腿**：可读区间表由引导腿构造 `BootReadPhase` 时计算——UEFI 腿的机制 = 五块载荷改走 `AllocatePages(AllocateMaxAddress, …, 上界=恒等窗口上界)`（分配器构造保证）+ `prepare_boot` 在 EBS 前 fail-fast 断言逐载荷验区间（越界即停机，信息含载荷区间与窗口上界两个坐标）；aarch64 跨镜像腿 = bootstrap 根恒等窗口同款论证；直核腿载荷本就在镜像管理的 bump 区。v3 的三层机制（构造/防线/解码）保留——但「构造与防线」属腿内机制、「解码」用共享语义接口：层的划分不变，判据的归属改了。实施细节（`uefi` crate 对 AllocateMaxAddress 的暴露形状、现有 `Vec` 分配路径的改法）待实施确认。

**（五）进程内信封的收尾口径（随三层模型更新）**：`BootPrepareResult` 保留为进程内结果类型（它是进程内对象，是否加 `#[repr(C)]` 不再是跨镜像问题）；「删除它以消除冗余」不再是 T1 的目标——跨镜像唯一权威定义是 `BootHandoffV1`，进程内消费面是否复用其字段由语义边界决定。D-69 的 `build_kernel_info` 双版本统一仍随本条同批（统一签名带 `reserved_regions` 形参）。

**（六）wire 序列化与 UEFI 内存图快照/EBS/回收的闭环（v4 三轮新增，T1 冻结条件 +1）**：`Wire*` 独立类型的一个直接实施后果——`WireBootModule.name` 从引用改定长字节槽后，**wire 元素布局与 `KernelInfo` 内部数组不再等价**，`BootHandoffV1::from_kernel_info` 构建的是一份全新序列化载荷（协议头 + `WireMemoryRegion` 数组 + `WireBootModule` 数组（内嵌定长名）+ `WirePlatformDescSource` 数组 + `reserved_regions`/`param_buf` 的 wire 数据），不能指向原数组。三个必须共同设计的时序与所有权问题：

1. **分配次序**：`AllocatePages` 会改写 UEFI 内存图（分配成功后内存图反映新分配的类型）——**所有影响最终内存图的 UEFI 分配必须在最终快照之前完成；快照后仍发生分配则必须重新取得并处理内存图，且保证 EBS 用有效 MapKey**。仓库现状佐证：`uefi_helpers.rs:fn prepare_boot` 的快照已是最后一步（"memmap ok (final pre-EBS snapshot)" 路标在平台来源定位之后）——wire 序列化载荷的分配必须插在快照之前。这不是接收端地址验证能补救的问题。
2. **所有权与回收**：wire 载荷页属 UEFI LOADER_DATA 而非 Conventional——进内核后不会因「深拷完成」自动回到 PMM。两个候选：沿用既有 bump 区所有权规则（opensbi/bootface 腿现状：bump 区整段从 VM 空闲清单扣除）；或显式登记逐页回收。倾向前者（与现有记账同构，无新机制；成本 = 少量常驻内存，有意选择并成文）。
3. **「原始数据存储位置」与「wire 序列化存储位置」是两个问题**：原 `KernelInfo` 数组是否需要在低地址，取决于它们在哪个阶段被读（UEFI 腿进程内路径：shim 自读自转，无需跨镜像可读）；wire 载荷的物理存储必须满足接收方可读（§1.5（四）区间表由腿声明）。不混为一谈。

**T1 冻结条件增补**：wire 序列化区的分配、最终内存图快照、EBS 时序与最终回收策略形成闭环（本节 1–3），缺一不冻。

### 1.6 影响面（供评估工作量）

- 定义侧：`os/libs/minix-boot/src/{kernel_info,boot_shim,handoff}.rs`。
- 构造侧：boot-shim 两腿（`uefi_helpers.rs`/`opensbi_helpers.rs`）、kernel-image 两面（`bootface.rs`/`bootface_a64.rs`）、内核与 qemu 测试内核的手工构造点（仓库内 `param_buf: &[]` 形状的构造点约 30+ 处，多数在测试）。
- 消费侧：三层模型下只在解码/落地边界（`decode_view` + `store_kernel_info`）加转换；内核普通消费点继续读 `KernelInfo` 的 `&[T]` 字段，不接触裸 `addr/len`。
- 文档：01 篇 §3.4（两信封叙述 + 「同一次构建产出」让步句）与 §3.9（`free_upper_idx` 语义）同步。

---

## T2（I-1 / NK1 / OQ-N6）：x86 从内联执行切到独立镜像跳转

> **v2 增补说明**：v1 说 x86 版「基本是 aarch64 两段去 cfg 门 + asm 换 SysV」，评审判定这句**低估了入口状态契约**——`rdi` 传参只是协议的一部分，跨镜像绝对跳转不满足普通函数入口的 ABI 前提（无 `call` 压返回地址、无调用方保证栈对齐）。v2 补完整入口契约（§2.3）、源数据映射两证明（§2.4）、`reserved_regions` 退场复核（§2.5）。

### 2.1 背景与既有裁决（原文内嵌）【已裁部分】

本树的引导形态共五种（登记于 `os/libs/minix-boot/src/boot_form.rs`）：x86_64 UEFI 内联（内核静态链进 boot-shim PE、进程内调 `arch_boot`）、aarch64 UEFI 跨镜像、riscv64 OpenSBI 自举、aarch64 QEMU 直核、riscv64 早期 hart 邮箱。

直核自引导（`bootface`/`bootface_a64`）在真板上的落位不对称，矩阵与理由见 01 篇 §3.7 的 `[ARCH: FW_PAYLOAD 直核形态（Minix3 无对应）]` 一段：x86 无对应形态（平台规范强制 BIOS/UEFI）、aarch64 以覆盖测试为主、riscv64 测试与真板嵌入式并存；它是辅助启动路径，主线仍是独立 shim + 独立内核镜像。

aarch64 走跨镜像的原因是结构性缺陷取证：内核内联在 PE 低地址执行时，第一次进程根切换（换 TTBR0 + `tlbi alle1is`）会摧毁内核自己的取指映射（真机取证：`elr`/`vbar` 钉死在 `0x43e0_xxxx`）。修复是方案 A：boot-shim 建表开分页后**绝对跳**进独立 `kernel.elf` 的高半入口（高半映射住永不切换的 TTBR1），此后整个内核在高地址执行、对根切换免疫。

跨镜像交换的语义裁决已完成——coordination 台账裁决原文（`rewrite-notes/coordination/TODO-LEDGER-DONE.md` §2）：

> | `OQ-N6` | 自举载体的双镜像交换采「每进程独立根」原语；「加栈顶参数」的快案被否（段面互撞与零隔离是结构性缺陷） | 已裁，实施待领 |

即：aarch64 的 `BootHandoff`（x0 递 blob 指针）+ 接收侧在活跃根上只做增量（不重建根）+ 每进程独立根，就是 OQ-N6 的参考实现。**尚未发生的是 x86 腿的实施**（NK1 = C-29「boot loader per-process 地址空间/栈」的收口）：x86_64 的产线今天仍是内联。

x86 镜像侧的现状（`os/kernel-image/src/main.rs` 头注原文）：

> ## 交付边界（诚实登记）
> `_start` 目前立栈后驻留（halt）——boot-shim → 内核镜像的跳转交接协议（KernelInfo 怎么递、入口态怎么约定）是 NK1（C-29，OQ-N6 设计轮）的裁决范围，这里不代决。当前 UEFI 产线仍是 boot-shim 内联内核、进程内调 `arch_boot`。

x86 的存量资产：`x86_64.ld` 链接脚本（高 VMA + 低物理 LMA）与 `check-layout.sh` L0–L9 布局校验已在位；aarch64 的 shim 侧跳转函数与镜像侧 resume 函数均已实现（cfg 门在 aarch64）。

### 2.2 aarch64 参考实现（逐段摘录，x86 版的模板）

**shim 侧**（`os/kernel/src/lib.rs:fn bootstrap_to_kernel_image`，aarch64 cfg）：

```rust
// 建表 + 开分页（与内联路径共享同一段建表代码，两路产出逐字节一致的根）
let kern_huge = boot_validate_and_prepare::<AArch64Paging>(kernel_info);
build_bootstrap_root_and_enable::<AArch64Paging>(kernel_info, root_page, kern_huge);
// 分配 blob 页（游标在其后读——T1 的 bump_next 时序差的来处）
let (blob_phys, _) = boot_alloc::boot_pt_alloc()
    .expect("bootstrap_to_kernel_image: handoff blob page alloc failed");
let (bump_base, bump_end) = boot_alloc::boot_alloc_region().unwrap();
let bump_next = boot_alloc::boot_alloc_next();
let handoff = minix_boot::BootHandoff { kernel_info: *kernel_info, root_page: root_page.0,
                                        bump_base, bump_next, bump_end };
// 写 blob（恒等映射下 VA==PA 可写；切片载荷指向的 UEFI 池页在首次切根前有效）
unsafe { core::ptr::write(blob_phys.0 as *mut minix_boot::BootHandoff, handoff) };
// 绝对跳：入口 = kern_virt_base（高半）；x0 = blob 指针；isb 后 br
unsafe { core::arch::asm!("isb", "br {entry}",
    in("x0") blob_phys.0, entry = in(reg) entry, options(noreturn)) }
```

**镜像侧分道**（`os/kernel-image/src/main.rs:fn rust_image_main`，aarch64）：

```rust
fn rust_image_main(handoff: *const minix_boot::BootHandoff) -> ! {
    // 直核（QEMU -kernel，x0=0）走自引导面；UEFI 链 x0=blob 指针走高半 resume
    if handoff.is_null() { bootface_a64::bootface_a64(); }
    // … 恢复 bump 游标（shim 侧在写 blob 后推进过）… 
    minix_kernel::boot_alloc::resume_boot_pt_alloc(h.bump_base, h.bump_next, h.bump_end);
    minix_kernel::arch_boot_resume_high_half(&h.kernel_info, PhysBytes(h.root_page))
}
```

**镜像侧 resume**（`os/kernel/src/lib.rs:fn arch_boot_resume_high_half`，aarch64 cfg）的关键纪律——**绝不重建根**：

```rust
// 在「活跃根」上只做增量：validate + 游标恢复 + KernelInfo 落 .bss + DM 覆盖 + kmain
set_current_root_phys(root_page);          // 根地址只记录（shim 已激活它）
store_kernel_info(kernel_info);            // 切片深拷进本镜像 .bss（见 T5）
crate::dm_coverage::establish_boot_dm(kinfo, root_page);  // 在活跃根上加 DM 映射
// 绝不调 build_bootstrap_root_and_enable——它会 new_from_page(root_page) 清零
// 整棵在用根（含装着本函数的 L0 槽），下一条取指即故障
```

### 2.3 x86 入口状态契约（v2 新增，评审要求补齐）**【待裁 → 契约内容冻结】**

交接协议必须写成一份**完整的机器状态契约**，而不是「`rdi` 递个指针」。已定方向与待冻结细节：

**已定方向**：

- 交接寄存器用 `rdi` 递 blob 物理指针——与内核现有 `jump_to_kmain` 用 `rdi` 递第一参数的约定一致。
- 绝对入口 = 内核镜像高半 `_start`。
- **`_start` 是机器入口 trampoline（裸汇编），不是被 `call` 进来的普通 Rust 函数**——跨镜像 `jmp` 不压返回地址、无调用方栈对齐保证，Rust 入口的前置条件必须由 trampoline 手工建立。

**trampoline 必须建立（并逐项写进契约、失败可查）的状态清单**：

1. **内核栈**：从 blob 读 `kern_stack_top`（或先落到镜像内一小块 bootstrap 栈再切换——两种取法待裁），`rsp` 置为该值并保证 16 字节对齐（SysV 要求 `call` 前栈对齐；trampoline 后续 `call` Rust 入口时对齐必须成立）。
2. **方向标志**：`cld`（SysV 假定 DF=0；UEFI 调用约定同为 Microsoft x64/SysV 混合，跳转前 shim 侧也显式 `cld` 双保险）。
3. **中断状态**：约定 `IF=0`（关中断进入；shim 侧 EBS 后本就无中断投递路径，镜像侧在 IDT 建立前不开中断）。
4. **根页表**：CR3 = bootstrap 根（shim 侧 `enable` 时已装入，trampoline 只断言不重装）。
5. **执行模式**：长模式 + 分页已开（shim 侧保证）；trampoline 首条指令在高半 VA 取指（该映射住 bootstrap 根的高半窗口，切进程根不换它）。
6. **blob 指针**：`rdi` = blob 物理地址（恒等窗口内 VA==PA 可读）。
7. trampoline 完成上述后，按 SysV 前提 `call` Rust 入口（Rust 入口从 `rdi` 收 blob 指针，参数寄存器恰不冲突）。

**其余待裁**：

- blob 页分配与恒等映射：aarch64 依赖恒等窗口 [0,4GiB) 下 VA==PA 可写；x86 的 bootstrap 根同构（`BOOT_IDENTITY_MAP_END = 0x1_0000_0000`，唯一权威 `os/arch/src/lib.rs:pub const BOOT_IDENTITY_MAP_END`），bump 池受 DM 可采上界 `min(BOOT_IDENTITY_MAP_END, VM DM 窗口)` 约束——opensbi 腿已有编译期断言先例（`assert!(BUMP_END <= minix_arch::boot_dm_admissible_end())`），x86 腿同款断言是否足够待实施时确认。
- trampoline 的栈取法（直接 blob→kern_stack_top，还是镜像内临时栈二段式）。
- `bootstrap_to_kernel_image`/`arch_boot_resume_high_half` 的 cfg 门泛化：aarch64+x86 两家同型可去 cfg 共享；riscv64 直核腿不需要 shim 侧跳转，不编译即无共享问题。

### 2.4 首次读取前的源数据映射：两证明分离（v2 新增）**【待裁 → 证明义务成文】**

评审纠正了 v1 的一处简化：**blob 可访问 ≠ blob 里所有切片指针都可访问**。aarch64 的实测（`handoff.rs` 模块注释 "the boot-services pool survives EBS for the boot window"）证明了池页在 EBS 后存活到首次切根，但不能推广成「UEFI 池页天然一直有效」。每块切片载荷要分别证明两件事：

1. **物理内容与所有权**：载荷后备物理页还存在、尚未被任何内存管理器复用。UEFI 侧的依据是快照口径（LOADER_DATA/池堆页不在 conventional 空闲清单，固件不复用）+ shim 不再分配（写 blob 后到跳转前无任何分配——须明文写成 `build_bootstrap_root_and_enable` 调用后的契约约束）。
2. **虚拟可访问性**：当前激活的页表确实映射了这些页、接收方读取合法。恒等窗口 [0,4GiB) 覆盖低物理地址的池页；但这是 bootstrap 根的属性——**首次进程根切换后此映射即消失**，这正是 T5 落地区（在映射消失前完成深拷）存在的理由。

T5 的深拷解决生命周期问题，但**不能弥补第一次读取前就缺失的映射**——两件事各自成立，缺一不可。

### 2.5 `reserved_regions` 是否随 x86 切换退场（v2 新增，评审质询的答复）**【待裁 → 按字段落点分别处理】**

评审质询：v1 把 `reserved_regions` 同时描述为「运行期仍要经低地址访问的固件占用区」与「x86 内联执行模型特有的运行期恒等窗口」——两者不等价，不能因切跨镜像就断定整字段退场。

**仓库侧复核答复**（依据 01 篇 §3.4 字段叙述与 `kernel_info.rs` 字段注释）：当前该字段确实装载**两种语义**——

- **x86 内联窗口**（fix27 引入）：内核代码被链进 boot-shim PE、在低物理地址执行，其代码/GDT/IDT/TSS 窗口必须被每个进程页表恒等重放。**这部分随 T2 退场**（内核切到独立高半镜像后，低地址不再有活代码）。
- **UEFI 链通用的固件占用区**（字段注释原文："everything the memmap snapshot reports as non-conventional RAM"——ACPI 表、LOADER_DATA 分配等内核运行期经 Direct Map 访问的页）。**这部分不随 T2 退场**——它表达真实的固件内存语义，与执行模型无关。

因此 `reserved_regions` 字段本身保留，退场的只是它的 x86 内联窗口内容；UEFI 腿在 T2 后继续填非空表（内容收窄为固件占用区）。这同时回答 D-69 的 builder 统一方向：统一签名必须保留 `reserved_regions` 形参（UEFI 腿 T2 后仍传非空），不能按「该字段将死」设计接口。01 篇 §3.4 的字段叙述与 L1488 读者 TODO 据此改写——「x86_64 待退场」的是内联窗口半，不是全字段。

### 2.6 迁移批次与回退 **【已裁】**

**QEMU 先行、短期双轨**（有期限的迁移策略，不是永久双协议）。验证序列：

1. 保持现有内联路径可构建可运行；
2. QEMU 测试启用独立镜像跳转，覆盖 handoff 读取、游标恢复、内核落地区、后续根切换；
3. 增加交接数据与映射边界的失败测试（见 §9.3）；
4. 通过后翻转产线路径，删除旧路径与文档临时说明（01 篇 §3.7 三形态表 x86 行、§4.8 实现现状、L1422/L2624 口径、§3.1/§3.3 终态叙述补指针——L1424 张力登记的候选 ① 同批）。

### 2.7 与 01 篇文档的联动

- §3.4 L1422「跨镜像跳转协议还没裁决（NK1/OQ-N6）」→ 改「OQ-N6 已裁（每进程独立根原语，aarch64 交接即其实现）；x86 腿实施收口 = NK1（C-29）」。
- §4.8 L2624「跳转协议尚未定案」→ 同口径。
- §3.4 `reserved_regions` 双语义拆分叙述（§2.5 答复落点）。

### 2.8 v3 增补（二轮评审：契约拆分 / 入口清单细化 / 可读性机制接线）

**契约拆分（纠正 v2 的混写）**：v2 把「blob 写入后不得再分配」写成 `build_bootstrap_root_and_enable` 调用后的约束，与参考实现时序不符（blob 页分配发生在该函数返回**之后**）。拆成两条独立契约：

1. **建表返回契约**：`build_bootstrap_root_and_enable` 返回后，bootstrap 根已满足该阶段所需映射（恒等 + 高半 + MMIO），根页基址已装入 CR3。
2. **blob 写入后契约**：blob 写入完成起、到绝对跳转为止，**不得有任何可能分配、释放或复用源载荷页面的操作**——这是 handoff 数据生命周期的关键约束，落在 shim 侧跳转流程（`bootstrap_to_kernel_image` 的 blob 构造段与 `br` 之间的代码行集合），不是建表函数的属性。

**入口状态清单细化（§2.3 补四项）**：

- `cli` 显式关中断写在 trampoline 内（不依赖「EBS 后固件已关中断」的推断）；
- trampoline 切到的内核栈必须已映射于当前 bootstrap 根（`kern_stack_top` 落在高半窗口——该映射住 bootstrap 根，切进程根不换它）；
- `call` 前 `rsp` 调整到 16 字节对齐、由 `call` 自身建立返回地址与栈状态——trampoline 以普通调用者身份满足 SysV 前提，Rust 入口不承担机器入口职责；
- 编译器栈使用前提成文为**内核构建约束**：no-red-zone（Rust 侧禁用 red zone 的构建旗标按工具链版本核实后写入构建配方），段状态前提（CS/SS 沿用 shim 的 GDT 段、内核在自身 prot_init 阶段重建）写进契约表。

**可读性机制接线**：§2.4 两证明的「虚拟可访问性」半，由 T1 §1.5（四）的三层机制承载——AllocateMaxAddress 构造使载荷必然落入恒等窗口、pre-EBS 断言防线、解码层逐切片重验；aarch64 的既有实测降级为**现象记录**，不再充当论证依据。

**blob 页与游标的附加验收**（并入 §9.3）：x86 handoff 的 `bump_next` 必须已越过 blob 页本身，且 blob 分配不会被接收方恢复分配时重复发出。

**共享与架构后端的分界（v4 三轮增补，对 §2.7 cfg 门候选的限制）**：`rdi`/`rsp`/`CR3`/`IF` 是 x86 机器状态，aarch64 入口涉及其它寄存器与异常状态——它们可以各出现在自己的架构实现里，**不得为共享一个函数揉进 `minix-boot` 的通用交接语义**。分界：共享部分负责验证 handoff、描述载荷布局、确认数据有效、恢复内核语义状态（`arch_boot_resume_high_half` 的 validate/落地区/DM/kmain 序列——真正同构的语义阶段）；架构后端负责建立满足目标平台入口契约的机器状态并完成最终跳转（`bootstrap_to_kernel_image` 的建表使能 + trampoline + `br`/`jmp`——机器入口）。v2 的「cfg 门泛化」候选据此加限制：**只有在行为确实同构的语义阶段才统一实现；不得仅因函数流程相似就把硬件差异移入通用代码**。x86 的 `cli`/栈建立/长模式前提不为三架构统一抽象成同名硬件操作 trait——共享边界表达所需入口条件，各后端自行实现如何满足。

**`reserved_regions` 消费面追踪答复（v4，评审质询的仓库侧取证）**：两类区域在消费点的处理**确实不同**，混装一个字段是真实的设计张力——

| 消费点 | 语义 | T2 后去向 |
|---|---|---|
| `os/kernel/src/dm_coverage.rs` source 4（`reserved_regions_candidates`，`:129` 注释与 `:292` 函数） | **内核 Direct Map 可达性候选**——固件占用区（ACPI/LOADER_DATA）须经 DM 窗口可读；明确「只进 KERNEL DM、不进 VM DM」（PMM 资源语义，测试 `test_establish_boot_dm_reserved_is_kernel_window_only` 钉住） | **保留**——固件占用语义与执行模型无关 |
| `os/kernel/src/vm_handoff.rs` 的 kern_ident 构造（`:498` 遍历 `reserved_regions()`，与 `:501` 的 `minix_arch::kernel_identity_mmio_regions()` 并列两输入） | **每进程恒等重放窗口**——把区间逐段恒等装进每个进程页表（fix27 的 x86 内联执行模型） | **随 T2 退场**——内联窗口半消失后该输入为空，kern_ident 只剩 MMIO 架构常量源 |

裁决：字段本体保留，但**按消费语义拆分输入**——T2 实施时 `vm_handoff` 的 kern_ident 输入面删掉 `reserved_regions` 源（`dm_coverage` 源保留）；不为「生产者都叫 reserved」维持单一混装语义。是否进一步拆成两个独立字段（DM 可达候选 vs 恒等重放），待 T2 实施时按拆分后的消费面形状定——判断依据是消费者要求的行为，不是字段名。

---

## T3（D-66）：`param_buf` 生产侧接入

### 3.1 语义与 C 参照【已裁部分维持】

`KernelInfo.param_buf` 对应 C 的 `char kinfo.param_buf[MULTIBOOT_PARAM_BUF_SIZE]`（1024 字节，`minix3/minix/sys/arch/i386/include/multiboot.h` 的 `MULTIBOOT_PARAM_BUF_SIZE`）。C 生产侧在 `minix3/minix/kernel/arch/i386/pre_init.c`（`mb_set_param` 写入 `key=value\0` 对，**ARCH/BOARD 两个变量必定写入**）；消费侧 `minix3/minix/kernel/system/do_getinfo.c` 的 `GET_MONPARAMS` 分支按 `length = sizeof(kinfo.param_buf)` **定长**整块拷给调用者（不足零填充）。

Rust 消费侧已对齐：`os/kernel/src/misc.rs:fn monparams_snapshot` 把切片零填充进 `[u8; 1024]` 再整块返回；空切片进出等价于 C 的 `param_buf[0] == '\0'`。护栏在 `KernelInfo::validate`：`param_buf.len() <= 1024`（fail-fast）。当前生产侧全部传空（构造点注释标记 P9-1）。

### 3.2 UEFI 腿来源修正（v2 改判）**【已裁】**

v1 候选一是「枚举 `Boot####` 变量、取 `BootCurrent` 对应项的 `OptionalData`」。评审改判：**首选读当前 shim 镜像自己的 `EFI_LOADED_IMAGE_PROTOCOL`**——

- `LoadOptions` 指向传给当前镜像的参数数据，`LoadOptionsSize` 给出实际字节数（UEFI 规范明确给出的两个字段）；
- `EFI_LOAD_OPTION.OptionalData` 是传给加载镜像的二进制缓冲区，**规范不保证它是标准命令行文本格式**——文本语义必须是 Minix-RS 自己的契约（ASCII/UTF-8、允许的参数语法），不能声称 UEFI 规范保证了编码。

裁决理由：直接消费固件实际传入本镜像的数据，不需要重新解析启动变量里的设备路径与描述文本；`Boot####` 枚举降为排查手段或特定固件兼容的备用路径，不进正常生产链。

### 3.3 参数规范化规则（v2 新增）**【已裁框架，细节待裁】**

参数生产不是「复制一段外部命令行」，统一规则四步：

1. **生成该引导形态必须提供的内部参数**——C 参照下 `ARCH`、`BOARD` 两个必备键必须先保留（`mb_set_param` 必定写入；若直接把外部输入转成 `param_buf` 而不重新生成这两个键，消费端表面兼容但语义不一致）；
2. 解析该形态的可选输入（UEFI = LoadOptions；SBI = `/chosen/bootargs`）；
3. 所有输入规范化为 C 侧预期的 `key=value\0` 序列（两条腿共用同一规则，不允许 UEFI/SBI 产出两种格式）；
4. 写入最终载荷前执行容量与边界检查。

**v3 增补裁决（二轮评审）**：

- **ARCH/BOARD 的语义归属（仓库侧取证）**：写入侧——C 两架构生产路径（`minix3/minix/kernel/arch/i386/pre_init.c:149-150` 硬编码 `BOARD_ID_INTEL`、`minix3/minix/kernel/arch/earm/pre_init.c:274-275` 由 `machine.board_id` 推导）都必写 `ARCHVARNAME`/`BOARDVARNAME`，属**全架构生产者共同行为**；消费侧——`minix3/minix/lib/libsys/env_get_param`（用户态服务器经 `GET_MONPARAMS` 读 boot 变量，`env_get_prm.c`），即 **ARCH/BOARD 是用户态 boot 变量契约，内核本身不消费**。裁定：通道形状是内核契约（已定）；ARCH/BOARD 键生成是「为用户态消费方保持 C 兼容」的义务——Rust 用户态 env 层尚未重写，**写入时点与 BOARD 值来源按腿实施时取证定案**（ARCH 取目标架构常量即可；UEFI 腿的 BOARD 取值源需给出证据），不允许静默缺键——宁可写占位值也不留缺键，与 C 消费方 `find_key` 的查不到返回错误语义对齐。
- **重复键规则（定案）**：内部权威键（生产侧自生成的 ARCH/BOARD 等）**禁止被外部输入覆盖**——外部输入含同名键时拒绝该键并给诊断（不整体 fail-fast，保留其余参数）；其余普通键的覆盖优先级显式列表，不靠拼接顺序的偶然。
- **LoadOptions 编码契约（定案）**：Minix-RS 输入约定 = ASCII 文本、`\0` 分隔 `key=value`、整条记录粒度截断（§3.5）；不符合约定的输入（非 ASCII 字节、无 `=` 的裸记录）按逐记录拒绝 + 诊断处理，不猜测编码。
- **输入来源兼容边界（v4 三轮定案）**：`LoadOptions` 的实际内容取决于镜像的启动方式——经 **UEFI Shell** 启动时，Shell 传入自己的命令行形式（可执行路径 + 空白分隔参数，UEFI Shell 规范定义其字符串形式），与本项目的 NUL 分隔记录**不是同一格式**。明文约束：Minix-RS 的 ASCII/NUL 参数格式是**本项目启动参数协议，不是对所有 UEFI `LoadOptions` 的通用解释**；本腿明确支持的启动来源 = UEFI Boot Manager 按约定格式传入的参数数据；不符合项目格式的来源拒绝或经明确的来源适配层规范化，不猜测编码与分隔符——「不自动兼容 UEFI Shell 命令行」是显式决定，不是缺陷。

### 3.4 SBI 腿 **【已裁方向，验收条件成文】**

采用 DTB `/chosen/bootargs`（标准固件递参通道；QEMU `-append` 即测试通道）。**验收条件针对仓库实际启动链**：必须用本仓实际使用的 QEMU 命令、固件与启动方式，实测确认 `-append` 的数据确实出现在被解析的 DTB 节点里——不能按其他架构或启动协议的习惯推断这条链必然成立。

### 3.5 截断语义（v2 改判）**【已裁】**

v1 的「截到 1023 字节再补 NUL」只适合单字符串，**不适用于 NUL 分隔的多条参数记录**。改判：以规范化后的整个 `param_buf` 容量为准——不超过 1024 字节的前提下保留**完整参数记录**，超长输入按规则丢弃放不下的**尾部整条记录**（不留半截 `key=value`）并提供诊断；数据一旦进入 `KernelInfo`，长度超限仍由 `validate` fail-fast 拒绝。C 侧的有界参数行为与 Rust 内部不变量同时保住。

### 3.6 硬前置与实施次序（v2 升格为硬性顺序）**【已裁】**

「只动生产侧、内核零改动」的原登记结论作废：UEFI 腿的参数字节落在 UEFI 池堆——与 `memmap`/`reserved_regions`/`boot_modules` 三块切片同病（池堆页在首次进程根切换后不被内核页表重放；取证摘录见 T5），而 `GET_MONPARAMS` 消费点在 syscall 期（远晚于首次切根）。**实施顺序定为：先让内核建立稳定的五块切片落地不变量（T5），再打开任何真实参数来源（本条）**——不允许以「目前 param_buf 为空不会出问题」为由先接参数再补落地区。另：`KernelInfo` 补 `param_buf` getter（getter 家族 14 字段缺 1）。

---

## T4（D-68 / I-22 复核）：直核腿第四链收口进 `DtbMemoryMap`

> **v2 状态：已裁，独立小批实施（五项中最先）。**

### 4.1 背景：I-22 丁案已裁已实施（原文内嵌）

内存图三链收口的裁决（`todo.md` §7.4 I-22 丁案，2026-10-18）：抽 `pub trait MemoryMapSource { fn collect(&self) -> (&'static [MemoryRegion], &'static [MemoryRegion]); }` 进 `minix-boot`，语义 = conventional（PMM 可直分）+ reserved（运行期经 Direct Map 访问但不得重分配）；三个 impl——UEFI（`UefiMemoryMap`，住 boot-shim）、DTB（`DtbMemoryMap`，住 minix-boot，带固件常驻区削界纯函数 `clip_reserved` + 三条编译期护栏 assert）、Multiboot（占位空实现）；引导形态层不合并。

**实施现状（2026-10-20 复核）**：代码已全部落地——trait 与 DTB 实现带 8 个宿主测试在 `os/libs/minix-boot/src/memmap_source.rs`；UEFI/opensbi/bootface 三链接线完成；opensbi 侧硬编码常量与单区断言测试已删；01 篇文档硬编码表述清零。**唯一漏网**是 aarch64 直核面 `bootface_a64.rs`。

### 4.2 漏网的第四链（现状摘录）

```rust
// os/kernel-image/src/bootface_a64.rs（aarch64 QEMU 直核自引导面）
static mut MEMMAP_STORAGE: [MemoryRegion; MEMMAP_MAX] = [/* …零值格… */];   // L141

fn fill_memmap(fdt: &fdt::Fdt<'_>) -> usize {   // L276：all_nodes 全树遍历 /memory
    // …与 DtbMemoryMap::collect 同构的解析循环…
}
const fn clip_firmware(base: u64, len: u64) -> Option<(u64, u64)> {  // L315：固件常驻区削界
    // …与 memmap_source::clip_reserved 同一段算术的第二份实现…
}
```

模块头注自陈（原文）：「两门全绿后的去重收敛留给独立清理增量」。评审确认：这是「共享语义已有统一出口、调用方绕过出口」的典型情况——削界算术双份（库内护栏护不住这份）、语义出口绕过（`/reserved-memory` 等后续语义调整只改库内则此腿静默漂移）、又一处无计数保护的 `static mut`（I-25 同类形态）。

### 4.3 裁决与实施要点 **【已裁】**

- 删除 `bootface_a64.rs` 的 `fill_memmap`/`clip_firmware`/`MEMMAP_STORAGE`，改调 `DtbMemoryMap::new(&fdt, fw_end).collect()`；让库内既有测试覆盖统一后的逻辑。独立小批实施，不与 T1/T2 捆批。
- **`fw_end = 0` 可以接受，但必须带着明确的引导形态前提成文**：此引导形态不存在需要经 `DtbMemoryMap` 固件前缀裁剪保留的区间（aarch64 QEMU 直核不在 DRAM 放固件常驻区：DTB 在 RAM 基址 `0x4000_0000`、镜像在 `0x4020_0000`，均为内核自用）。对照：bootface.rs（riscv）传 `KERN_PHYS_BASE = 0x8020_0000`——OpenSBI 常驻 `[0x8000_0000, 0x8020_0000)` 必须削出。
- **两个机制分开，不互相替岗**：`DtbMemoryMap` 的固件前缀裁剪（aarch64 直核腿为空）≠ 内核自身镜像占用的排除（后者始终由内核侧 `cut_memmap` 负责）。`fw_end = 0` 只表示「该路径没有固件保留前缀」，这个约定要写进文档；不为这一项单独引入策略抽象。

**DTB 自身的生命周期（v3 补充前提，二轮评审）**：`fw_end = 0` 只回答「固件前缀裁剪在该腿为空」，**不回答 DTB 字节本身是否被妥善保留**。仓库侧取证：aarch64 直核腿的 `KernelInfo.reserved_regions = &[]`（opensbi 版 `build_kernel_info` 硬编码），内核 Phase A.2 的 `cut_memmap` 只切内核镜像与模块（D-64①），**DTB 物理区间不在任何保留清单里**（全仓 grep 无 dtb×cut/reserve 命中）——它落在 conventional 空闲账内，最早的复用者是 VM PMM（VM 启动后）。当前 DTB 最后一次访问是平台发现（kmain Phase A–D），早于 VM PMM——次序今天成立，但这是与 `platform_sources` 同类的**相位次序巧合**，不是结构保证。补充前提成文（验收必查）：

> `fw_end = 0` 仅表示该引导形态没有需要由固件前缀裁剪机制处理的保留区。DTB 数据本身是否需要暂时保留，另由其消费时序和内存所有权规则决定，不由 `fw_end = 0` 自动保证。

验收补充两项：① 核对 aarch64 直核腿 DTB 最后访问点（平台发现）与最早复用点（VM PMM）的次序证据；② 结构化修复候选——Phase A.2 对 DTB 区间补一笔 `cut_memmap`（与 D-64① 内核镜像自切同型），或等价地把 DTB 区间纳入 VM handoff 的 `deducted` 记录，使「复用不得早于最后访问」从次序巧合变成结构排除；实施时对照 C earm 侧 DTB 预留行为定案。此澄清落地后 T4 维持独立小批实施、不等 T1/T2。

**v4 三轮增补（通用内存所有权语义，不进 DTB 特例）**：结构化修复走**通用机制**——引导链把「需要暂时保留的物理区间」经现有交接机制（handoff `deducted` 排除记录或内存图裁剪）告知后续阶段；通用 VM/PMM 只处理「预留区间」语义，不添加「特殊处理 DTB 地址」的机制分支（它不知道也不需要知道那是 DTB、ACPI 描述符还是别的启动数据）。生命周期收尾必须显式：预留区间何时允许回收写进交接记录；若裁决为**永久扣除**（少量内存损耗换最简安全性），那是**有意选择**并成文，不是不知不觉的遗留。验收同步收紧为两条都查：DTB 最后访问早于最早复用**且**所有权规则实际阻止提前复用（不只是时序上恰好没发生）。

---

## T5（D-70）：内核落地区补全——`platform_sources` 与 `param_buf`

> **v2 状态：已裁采案 1。**

### 5.1 现状与取证（原文内嵌）

`os/kernel/src/lib.rs:fn store_kernel_info` 把三块切片深拷进内核 `.bss` 落地区，复制动机的取证原文（注释节选）：

```text
// fix27c: the `reserved_regions` payload lives in the boot-shim's
// UEFI-pool heap — a `&'static` slice whose backing pages are only
// mapped by the firmware's 1:1 table. After the higher-half jump
// the kernel's own tables do not replay that mapping and the same
// VA reads zeros (real machine: res=127 zeroed=127 …).
// NK4-C (2026-09-25): the same disease takes the `memmap` and
// `boot_modules` payloads — … (real machine vh1: all conventional
// memmap entries read back length 0 at the `vm_handoff::classify`
// point … → VM free list empty → VM panic).
```

三块落地区：`RESERVED_REGION_STORE`（256 条）、`MEMMAP_REGION_STORE`（MAXMEMMAP 条）、`BOOT_MODULE_STORE` + `BOOT_MODULE_NAME_STORE`（12 名 × 16B 定长池，strlcpy 语义）。配套测试 `test_store_kernel_info_lands_firmware_slices` 钉「指针必须 repoint 到 .bss」+ 幂等重存。

**KernelInfo 有五块切片，剩两块未落地**：

- `platform_sources`：UEFI 腿 `find_platform_sources()` 以 `Box::leak` 落在 UEFI 池堆。当前安全的唯一原因是**消费点全部在首次切根之前**（平台发现在 kmain Phase A–D 完成，首次进程根切换在 Phase F）——无结构保证的时序巧合，与三块切片当年的发病时序同型。
- `param_buf`：当前恒空（D-66 未落地），但消费点 `misc.rs:fn monparams_snapshot` 在 syscall 期——D-66 接入后是实害（T3 §3.6）。

### 5.2 裁决与结构不变量 **【已裁】**

**采案 1：五块切片全部经 `store_kernel_info` 落入内核自有存储**。评审把不变量升格为一句可长期遵守的规则：

> `KernelInfo` 中所有需要继续使用的切片载荷，都由 `store_kernel_info` 统一落到内核拥有的存储区；此后内核不依赖启动端临时切片的生命周期。

这条比「这几个字段该在什么时候被消费完」更容易理解、维护、验证——「三块有结构保证、两块靠时序」的混合状态一次消除。案 2（consume-before-first-root-switch 契约 + 断言）否决：param_buf 的消费点在切根后，该案对它根本不可用。

实施要点（评审附带条件）：

```rust
// globals.rs 增：
pub(crate) const PLATFORM_SOURCE_STORE_LEN: usize = 4;   // 候选值，见下
pub(crate) static PLATFORM_SOURCE_STORE: SyncUnsafeCell<[PlatformDescSource; 4]> = …;
pub(crate) static PARAM_BUF_STORE: SyncUnsafeCell<[u8; MULTIBOOT_PARAM_BUF_SIZE]> = …;
// store_kernel_info 增两段：长度断言 + ptr::copy + repoint——与 memmap 段逐字同构
```

- **容量 4 是候选值而非定案**：必须先核实「最多两个来源（DTB + RSDP）」是类型与生产路径的**真实上限**还是当前常见情况；容量作为有意设计的上限写入，配套超容量测试。
- **param_buf 落地的零填充语义**：重复调用不得暴露上一次输入的遗留内容——即使最终切片较短，也保持「清零再复制」的明确语义。
- 实施顺序：**先于 T3**（T3 的硬前置，见 §0.2）。

**v3 补充两核实（二轮评审，实施前完成）**：

1. **容量 4 必须是已核实上限而非经验值**：生产路径取证——UEFI 腿 `find_platform_sources` 每架构至多推 1 条 DTB + 1 条 RSDP（每类首个命中即 `break`，`os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources`），opensbi/bootface 各腿恒 1 条 DTB——今天真实上限 2，容量 4 = 2 × 有意余量。核实义务：确认未来无「同类多实例」场景（多 RSDP/嵌套描述符）后，把「上限 2、容量 4 的构成」写进容量常量文档。
2. **深拷覆盖全部间接数据**：`PlatformDescSource` 已核为纯数据 `(PlatformDescKind, PhysBytes)`（`os/libs/minix-boot/src/platform.rs:pub struct PlatformDescSource`），无嵌套指针，直接复制成立；`BootModule.name` 的间接引用由既有名字池承接（strlcpy 语义，已在三块落地区中）；`param_buf` 为平铺字节。`PlatformDescKind` 的枚举布局疑虑归 T1 递归闭合（§1.5（一））处理，不属本条。

**v4 三轮增补（与 T1 wire 序列化联动复核）**：切片载荷现在有三个存储阶段——原始 `KernelInfo` 载荷（boot-shim 侧分配）、wire 序列化载荷（T1 §1.5（六），UEFI 腿在快照前分配）、内核 `.bss` 落地区（本条）。三阶段各自的「谁分配、何时可读、何时允许回收」随 T1 §1.5（六）闭环一次写清；特别地，**「已深拷」不等于「相关页面都可立即释放」**——原始载荷与 wire 载荷的回收时点由各自所有权规则决定（倾向：全部归入 bump/LOADER_DATA 区整段扣除的既有记账，无逐页回收机制）。

### 5.3 测试要求（随验收条件 §9.3）

正常来源全部复制、落地后指针已 repoint、幂等重存、超容量明确失败（不静默截断不越界）——四类断言补进既有测试族；落地区容量断言与 01 篇 §5.3 TODO-03 的溢出臂测试同批。

---

## 9. 三条硬性验收条件（v2 新增，评审 §8 全文采纳）

### 9.1 首次根切换之前的读取契约 **【待裁 → 随 T2/T5 成文】**

x86 与 aarch64 的跨镜像路径中，明确记录以下各项，把生命周期依赖从说明性注释变成可审查的执行顺序：

- handoff blob 在哪里、由谁映射；
- 每块切片数据在哪个物理区间；
- 接收端第一次访问这些载荷时使用哪个根；
- `store_kernel_info` 完成之前，哪些页面不能交给 VM/PMM；
- 在哪个具体调用之后，可以不再访问源切片。

### 9.2 wire 格式的跨目标构建验证 **【已裁方向，随 T1 落地】**

wire 布局验证必须是**构建门禁**，不是文档里的布局表：每个受支持目标的 `BootHandoff`（或其 v1 后继 wire 类型）总大小与对齐、关键字段偏移、嵌套载荷类型的大小与对齐、wire 版本在生产与消费双方的一致性——全部在交叉编译检查中运行。依据：Rust Reference 对类型布局保证的范围有明确界定，显式布局定义 + 构建检查比记录当前编译结果可靠。

### 9.3 失败条件测试 **【已裁，随各条实施补齐】**

用失败行为固定架构契约，各条配套：

| 测试 | 所属 |
|---|---|
| 非法 `free_upper_idx`（哨兵外的越界值）与越界切片长度在解码阶段被拒绝 | T1 |
| 切片源地址不在当前映射内时，不允许直接形成可访问引用 | T1/T2 |
| `store_kernel_info` 对超容量输入明确失败 | T5（兼 TODO-03） |
| `param_buf` 超长不得生成不完整参数记录 | T3 |
| T4 的固件裁剪测试在实际调用路径中仍然生效 | T4 |
| x86 handoff 游标必须越过 blob 页本身，且 blob 分配不会被恢复时重复分配 | T2 |

### 9.4 v3 增补验收（二轮评审）

- **三层解码模型是 T1/T2 的实施门槛**：宿主测试必须能构造 `KernelInfoView<'ctx>` 与 `BootReadPhase<'ctx>`；「视图不得伪造 `'static`」以类型签名保证（视图生命周期绑定启动读取阶段令牌的借用——代表真实数据有效期，不绑定 blob 引用、不提供任何 `into_static()` 出口；v4 修正「绑定输入引用」的不精确表述）。
- **逐切片首读可读性**：T1 §1.5（四）双层结构各就其位——腿内机制层（AllocateMaxAddress 构造 + pre-EBS 断言）宿主不可测（UEFI 专属，真机窗口验收）；共享语义接口层（`readable_intervals` 求子集）宿主可测（构造越界区间输入 → 解码拒绝）。
- **wire 递归闭合进布局门禁**：`Wire*` 元素类型与 `BootHandoffV1` 同等对待（size/align/offset 基准值逐目标检查）。
- **wire 序列化闭环**（v4）：T1 §1.5（六）的分配次序（快照前分配）做成 prepare_boot 流程注释契约；LOADER_DATA 区整段扣除的记账选择成文。
- **协议头验证顺序**：接收端「先验头、后解引用」做成单元测试——坏 magic / 坏 version / size 不足三案拒绝；可执行断言是**头部校验通过之前，不创建完整 wire 类型引用、不访问切片载荷、不执行后续解码**（读头字段本身当然要读头部字节——测试钉的是越权面，不是「零读取」）。
- **DTB 生命周期**：T4 §4.3 的验收（次序证据 + 所有权规则实际阻止提前复用）落盘。

### 10. v4 状态收紧表（三轮评审裁定回填）

| 条目 | 状态 |
|---|---|
| T1：wire 作为唯一权威定义 | 已裁（维持） |
| T1：`Wire*` 递归固定布局 | 已裁（维持） |
| T1：`WireSlice` 精确表示与区间验证 | **v4 已补并冻结**（非泛型 + `byte_len` 字节口径 + 溢出检查区间公式，§1.3） |
| T1：地址空间语义 | **v4 已纠并冻结**（逐字段声明物理/虚拟，`WirePhysAddr`/`WireVirtAddr`，§1.5（二）） |
| T1：映射有效性 | **v4 已改**（可读区间语义接口替代恒等映射判据、机制归腿，§1.5（四））——腿内机制随 T2 实施验收 |
| T1：wire 数据存储 | **v4 已补**（分配/快照/EBS/回收闭环，§1.5（六））——T1 冻结条件 +1 |
| T1：`KernelInfoView` 生命周期 | **v4 已改并冻结**（绑定启动读取阶段令牌 `BootReadPhase`，§1.4） |
| T2：x86 独立入口 | 已裁（维持），机器状态由 x86 后端实现 |
| T2：共享引导代码 | **v4 已限**（仅真正同构的语义阶段共享；机器入口不合并，§2.8） |
| T2：`reserved_regions` 消费面 | **v4 已核**（两消费点两语义；T2 实施时按消费语义拆分输入，§2.8） |
| T3：参数格式 | 已裁（维持），UEFI Shell 兼容边界成文（§3.3 v4） |
| T4：DTB 生命周期 | **v4 已改**（通用所有权语义 + 显式收尾；禁止 DTB 特例进 PMM，§4.3） |
| T5：五块切片落地 | 已裁（维持），三存储阶段联动复核成文（§5.2 v4） |

实施次序维持 §0.2。三轮评审最终判定：v4 可作为 T4 独立实施与 T5 设计的基础；优先修正三项（切片区间按元素大小、共享解码不硬编码恒等映射机制、wire 分配与快照回收共同设计）均已落文——T1 wire 协议在 §1.5（六）闭环成文后达到冻结条件。

---

## 附：本文未收录的裁决主题（指针）

- **大页能力/选页策略/启动 DM 安装的四层抽象**（D-59）：`boot-dm-hugepage-design.md`（仓库内，独立线程迭代中的专门设计文件；含目标设计、五阶段迁移计划与测试矩阵）。
- **OQ-N6 裁决上下文**：`rewrite-notes/coordination/TODO-LEDGER-DONE.md` §2（「每进程独立根」原语与被否的「加栈顶参数」快案）。
- **x86 内核独立镜像的布局产出**：`os/kernel-image/x86_64.ld` 与 `check-layout.sh` L0–L9（已就绪，T2 只裁交接协议）。
- **UEFI 规范事实**（首轮评审核对官方文档确认）：`EFI_LOADED_IMAGE_PROTOCOL` 的 `LoadOptions`/`LoadOptionsSize` 字段；`EFI_LOAD_OPTION.OptionalData` 为二进制缓冲区、不保证文本编码（T3 §3.2 的依据）。
