# 01 · 引导链 v5 最后设计闭环取证（round-6：切根时序 / 解码边界存废 / arena 布局 / 入口与装载归层）

> **生成日期**：2026-10-28。
> **用途**：五轮外部评审提出的四项实质设计问题（切根前后读取时序、`KernelInfoView` 存废、独立 WireSlice vs 单一 arena、x86 入口栈与共享装载归层）+ 方案甲闭环核实的定向取证。只调查与比较，不改代码。事实标 **已证实 / 反证 / 尚无证据**，裁决建议独立标注。核心结论先行：**现状 shim 侧切根后零源切片解引用（浅拷信封而已），序列化放「EBS 后、切根前」与现有代码结构完全兼容；`KernelInfoView` 在复制路径下无语义位置，建议退役；单一 arena 一至三页装下全部 wire 载荷且与方案甲分类输出合一；x86 现状栈即镜像内 bootstrap 栈（裁决维持），共享装载器的恒等假设按「前提归边界」成文。**

---

## A. 切根前后的真实读取时序（已证实）

### A.1 切根生效点与两侧读取清单

切根生效点 = `build_bootstrap_root_and_enable` 内 Step 3 的 `paging.enable()`（`os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable`，Step 3 注释自陈 "Enable paging"）——TTBR0/TTBR1（x86 为 CR3）自此指向 `root_page`。此前一切内存访问都在固件遗留映射下（恒等、覆盖全部已安装 RAM）。

aarch64 跨镜像路径（shim 侧 `os/kernel/src/lib.rs:fn bootstrap_to_kernel_image`）在 `enable()` **之后**的全部读取（逐项，来自函数体）：

```rust
// enable 已返回，活动根 = bootstrap 根（恒等 [0,4GiB) + 高半 + MMIO）
let (blob_phys, _) = boot_alloc::boot_pt_alloc()        // bump 分配 blob 页（区内有 DM 上界）
    .expect("…");
let (bump_base, bump_end) = boot_alloc::boot_alloc_region()  // 读内核全局（.data）
let bump_next = boot_alloc::boot_alloc_next();
let handoff = minix_boot::BootHandoff {
    kernel_info: *kernel_info,   // ← 浅拷：只拷结构体与胖指针，不解引用数据指针
    root_page: root_page.0, bump_base, bump_next, bump_end,
};
unsafe { core::ptr::write(blob_phys.0 as *mut minix_boot::BootHandoff, handoff) }; // 写 blob 页（恒等）
let entry = kernel_info.kern_virt_base().0;             // 读标量（信封在 shim 栈，低地址）
unsafe { core::arch::asm!("isb", "br {entry}", …) };
```

**切根后被读对象的完备清单**：bump 游标（内核全局）、`kernel_info` 信封本体（住在 shim 的 PE/栈内存，低地址、恒等窗口内）、blob 页（bump 区、DM 上界内）、一个标量字段。**零 UEFI 池堆源切片内容解引用**——SAFETY 注释原文自陈胖指针 "remain dereferenceable by the kernel image before the first root switch"（只被拷贝、由镜像侧消费）。

### A.2 源切片内容的真实读取点（镜像侧）

`os/kernel/src/lib.rs:fn arch_boot_resume_high_half`（镜像侧入口）：

- `boot_validate_and_prepare(kernel_info)` → `KernelInfo::validate`（`os/libs/minix-boot/src/kernel_info.rs:fn validate`）——只读 `memmap.is_empty()`/`param_buf.len()` 等**切片长度元数据**（胖指针的 len 半边），不解引用数据指针。
- `store_kernel_info(kernel_info)`（`os/kernel/src/lib.rs:fn store_kernel_info`）——**真正的源内容读取**：深拷 `memmap`/`reserved_regions`/`boot_modules`（含逐名 strlcpy 进名字池）三块，读的是池堆源数组，此时活动根 = bootstrap 根（恒等 [0,4GiB)），池页低于 4 GiB 才可读——「池页存活到首次切根」假设的实义即此窗口。
- `platform_sources`/`param_buf` 两块未落地（T5 缺口）：前者全局 getter 返回原指针，平台发现（Phase A–D）在首次 `switch_to_user`（Phase F）前消费——时序巧合；后者现恒空。

x86 内联路径对照（`os/kernel/src/lib.rs:fn arch_boot`，x86_64 cfg）：`store_kernel_info` 在**函数第一段**（`enable()` 之前）执行——内联腿是「先深拷（固件映射下，无 4GiB 约束）后切根」，比跨镜像腿更早、约束更松。

### A.3 wire 载荷的分配时间 vs 填充时间（现状反证 + 设计裁决）

现状无 wire 载荷（**尚无证据**项为零——`BootHandoffV1`/`Wire*` 全部未实现，round-3 B 批已证零命中）。v5 设计只钉了「EBS 前分配」，**序列化时点未定**——这正是评审指出的缺口：提前分配 ≠ 内容已备，序列化动作本身要读源数据，它在哪个映射下读决定约束。

**裁决建议（四步时序，与现状代码结构零冲突）**：

1. **EBS 前**：分配 arena（单一连续 LOADER_DATA，`MaxAddress(bootstrap_alloc_limit(size))`）及一切 post-EBS 缓冲；只分配不填充。
2. **EBS 后、切根前**：捕获 helper 返回的 `MemoryMapOwned` → 分类循环**直接以 `WireMemoryRegion` 形态写入 arena**（分类即序列化，无中间数组）；同步序列化 `WireBootModule`（读池堆名字串）、`WirePlatformDescSource`、param 字节。此窗口活动映射 = 固件遗留恒等映射，**任意池页可读、无 4GiB 约束**。
3. **切根**（`build_bootstrap_root_and_enable` 的 `enable()`）：此后不再读任何只在固件映射下可读的数据——现状 shim 侧已经满足（A.1 清单），序列化提前到步骤 2 后该性质保持。
4. **切根后**：只补运行期信封字段（bump 游标三件套）+ 写信封 + 绝对跳转——与现状 `bootstrap_to_kernel_image` 尾段逐字同形。

该时序同时化解两件事：序列化源读取摆脱 4GiB 约束（固件映射覆盖全 RAM）；「切根后仍读固件映射数据」这一类别从流程上消失，不依赖类型系统。接收端（镜像侧）首读/复制/恢复链维持现状形状（A.2）。

### A.4 分类输出数组的上界豁免（评审的顺手优化成立）

arena 是唯一在切根后（镜像侧）被读的载荷缓冲 → 唯一需要 MaxAddress 上界。不存在「切根后才不读、却背上界约束」的临时数组——分类输出即 arena 的 memmap/reserved 区（A.3 步骤 2），无双份数组。评审要求「不应把不必要的低地址约束扩大到临时数据」在 arena 形态下自动满足。

---

## B. `KernelInfoView` / `BootReadPhase` 存废（已证实零实现；裁决建议退役视图层）

### B.1 现状（已证实）

两者**零代码命中**（round-3 B 批已证，本轮未变）。镜像侧真实消费链（A.2）：blob 信封 → validate（长度元数据）→ `store_kernel_info`（读源内容、拷进 `.bss` 落地区、从落地区指针构造 `&'static`）→ 全局 `kernel_info()` 供全内核消费。**这条现状链就是复制路径本身**——没有任何中间视图对象的位置被代码要求过。

### B.2 关键论证：复制路径下「合法 `'static`」不依赖视图层

v3 引入视图层的动机是防 `decode() -> KernelInfo` 伪造 `'static`（源指针指向池堆）。复制路径下该动机消失：`store_kernel_info` 从**内核 `.bss` 落地区**构造切片引用——指向内核自有存储的 `&'static` 是真实合法的，不伪造任何东西。视图层剩余的 purported 价值只有「持源引用的受控窗口」，而这个窗口在复制路径里根本不需要持有引用（读一片、拷一片、即弃）。

生命周期参数的真实约束力检验（评审四问逐答）：

1. `BootReadPhase<'a>` 若是 `PhantomData` 草图——生命周期参数与真实借用无关，纯标注（已证实：草案即如此）。
2. 无任何类型机制阻止其它入口在「视图存活」时切根——`switch_address_space` 是全局可达的普通函数（`os/kernel/src/proc.rs` 一族），不经过令牌借用边界（已证实：无此约束代码）。
3. 因此「视图绑定令牌借用」只是把纪律写进签名，不产生类型保证——与被否决的 `translate` 草图同病。

### B.3 `readable_intervals` 的来源（概念纠正成立）

评审纠正成立：区间表是**验证函数的输入**（声明由腿构造：固件映射窗口 / bootstrap 根窗口 + 分配上界 + 生命周期声明），不是证明。三个独立条件不得互相冒充：① 地址算术与协议上限（纯计算）；② 实际执行环境可读（腿的机制保证：分配上界或映射窗口）；③ 读取期间不复用/失效（阶段契约：读取发生于首次切根前 + LOADER_DATA 不进 free list）。验证逻辑只做「wire 地址 ⊆ 声明区间」的子集检查——声明本身的真实性由腿负责并在文档成文，不由解码器自证（禁止把 wire 自报区间登记进表再用该表证明可读）。

### B.4 裁决建议（标注：裁决建议）

**`KernelInfoView` 与 `BootReadPhase` 退役；三层模型收敛为两层 + 一条受控复制边界**。边界是一个函数序列（不是带生命周期魔法的类型）：

1. 受限原始读取验证协议头（坏 magic/version/size 拒绝；通过前不构造完整 wire 类型引用）；
2. 复制固定大小信封为内核拥有的值（信封内切片字段仍是已验证的地址/长度描述符，不是外部引用）；
3. 逐载荷再验证（溢出、长度、对齐、协议上限、目标落地区容量、⊆ arena 边界）；
4. 在受保证读取阶段（镜像侧 resume 序列内）直接把 wire 元素转换并复制进内核 `.bss` 固定容量存储——`store_kernel_info` 的 wire 版即此；
5. 复制完成后构造最终 `KernelInfo`（全部指针指向落地区），普通内核代码只读自有数据。

模块 ELF 字节流不经这条边界（Phase C 装载路径，见 E.4）；handoff 元数据小而定容，五步全覆盖，**不需要任何跨调用存活的外部引用**。`unsafe` 边界只剩两处且契约明确：① 边界的受限原始读取（头验证与复制源读取——由 resume 序列的阶段契约背书）；② `load_elf_into` 的模块直读（E.4）。

---

## C. 独立 WireSlice（A）vs 单一 handoff arena（B）（比较 + 裁决建议 B）

### C.1 容量算术（已证实，基于已裁 wire 类型形状）

按 v5 已裁的 wire 元素形状计算上界：`WireMemoryRegion` 16 B × 128（conventional，容量对齐 `MAXMEMMAP`）+ 16 B × 256（reserved，对齐既有 `RESERVED_REGION_STORE` 容量）+ `WireBootModule` 32 B × 12 + `WirePlatformDescSource` 16 B × 4 + param_buf 1024 B + 协议头/信封 ≈ **8 KiB，即 2 页；取余量 3 页**。整个 handoff 载荷一至三页装完。

### C.2 逐维比较

| 维度 | A：多份独立载荷 | B：单一 arena（裁决建议） |
|---|---|---|
| EBS 前分配次数 | 5–6 次（信封 + 五块载荷各一） | **1 次**（arena 整段，MaxAddress） |
| 内存图扰动 | 每次分配新增一个 LOADER_DATA 描述符，快照 B 膨胀 | **一段**；终图记账一笔 reserved |
| 可达性验证 | 逐载荷独立区间表 + 逐项 ⊆ 检查 | **验 arena 边界一次** + 每载荷 ⊆ arena（同一坐标系） |
| 与方案甲分类输出的关系 | 分类输出数组与 wire 数组两份数据，需二次搬运或双份保存 | **分类循环直接产 `WireMemoryRegion` 写入 arena（分类即序列化），无双份**——正面消除评审问的「重复保存」 |
| 容量与失败语义 | 各数组独立上限 | arena 容量 = 各区上界之和（编译期常量算术）；任一区超限显式失败（方案甲原则复用），失败点唯一 |
| 版本演进 | wire 类型定义不变 | 同左；arena 内布局仍由 wire 类型与信封次序定义，arena 只是分配策略 |
| 衔接成本 | x86 内联路径不跨镜像，两方案都不触碰 | 同左；x86 独立镜像与 aarch64 腿共享 arena 构造器（各腿一次 MaxAddress 调用） |

### C.3 裁决建议（标注：裁决建议）

**采 B。协议字段零改动**：`WireSlice.addr` 仍是物理地址（值为 arena 内地址，语义与 v5 已裁一致），不引入相对偏移——偏移只省 8 B/项却引入 base+off 加法验证面，收益不成比例（评审同样不主张现在改偏移）。新增一条验证规则：**全部 `WireSlice`/载荷区间 ⊆ [arena_base, arena_end)**（§1.3 区间公式的第 ③ 步在 arena 形态下具体化为对 arena 求子集，逐载荷仍查溢出/对齐/上限）。arena 页 EBS 前经 `MaxAddress(bootstrap_alloc_limit(arena_size))` 一次分配；终图记其 LOADER_DATA（reserved）；镜像侧深拷完成后整段死页常驻——有意选择并成文（与方案甲对 `MemoryMapOwned` backing 的常驻裁决同型）。

v5 已裁条目零推翻，仅收拢：§1.5（六）「wire 载荷在快照前分配」具体化为「arena 一次分配」；方案甲验收条件 1 的「固定容量输出数组」具体化为「arena 的对应分区」。

---

## D. 方案甲最终内存图与容量闭环（已证实 / 设计约束分列）

1. **`MemoryMapOwned` 在 EBS 后被读取的位置（现状：零）**——已证实：`os/boot-shim/src/uefi_helpers.rs:fn exit_boot_services` 包装函数丢弃返回值（注释自陈 "The returned memory map is discarded"）。方案甲新增的分类读取（A.3 步骤 2）发生在 EBS 后、`enable()` 前，其可访问性前提 = **固件遗留页表（CR3/TTBR）在 EBS 后保持不变**。证据分层如实写：现状 UEFI 两腿 EBS 后的全部执行（建表对 root 页的 VA==PA 写入、栈访问、`prepare_boot` 返回路径）都依赖该前提且真机跑通——**已证实（实现事实）**；UEFI 规范对 EBS 后固件页表的存续无明文承诺（OVMF/edk2 实现保持 CR3 不变）——**规范沉默区**，文档按「实现事实 + 规范未保证」双标签登记，不笼统声称「规范保证覆盖」。
2. **EBS 后零动态分配**——已证实（现状）：EBS 后路径 = `raw_serial`（COM1 端口输出，非内存分配）、返回 `BootPrepareResult`、调 `arch_boot`/`bootstrap_to_kernel_image`（validate 只读、建表走 bump 区外的 root 页、blob 页走 bump 分配——均非 UEFI 池分配）。方案甲分类循环的零分配是**设计约束**（读 `MemoryMapOwned` + 写 arena，纯计算），验收测试钉「分类路径无 `Vec`/`allocate` 调用」（§9.3 已列）。
3. **`MAXMEMMAP` 的身份**——已证实：它是**内核侧 `FREE_MEMMAP` 存储容量**（`os/kernel/src/memmap.rs:pub const MAXMEMMAP`，值 128；C 原值 40，Rust 因 UEFI 图描述符 routinely 超过 40 而扩容），不是 UEFI 描述符数上限（规范无上限，OVMF 典型 30–60 条描述符，conventional 分类结果 ≤ 描述符总数）。承载性口径 = **容量上限 + 超限显式失败**（分类循环写满即 fail-fast，拒绝截断内存图——方案甲验收条件 3 原文），不是「证明实际永不超限」。arena 的 memmap/reserved 分区容量取同一常量族。
4. **各缓冲的末次读取阶段**——已证实：arena（含其分类分区）末读在镜像侧 `store_kernel_info`（bootstrap 根下）→ 是唯一需 MaxAddress 上界的缓冲（A.4）；无其他切根后读取的临时数组。
5. **`AllocateMaxAddress` 的整段口径**——已证实（round-5 已录，维持）：`bootstrap_alloc_limit(size) = boot_dm_admissible_end().checked_sub(size)`（`uefi_helpers.rs:fn bootstrap_alloc_limit`，注释自陈 "passing `bound - size` keeps the whole region under the bound"）——整段末端的构造性保证，不是起始地址断言。arena 与模块 blob 的分配改造都经此函数。

---

## E. x86 入口与共享装载的剩余设计点（已证实 + 裁决建议）

### E.1 栈：现状即「镜像自身 bootstrap 栈」，裁决维持二段式（已证实）

`kernel_boot_stack_top` 是链接脚本在 `.bss` 里预留的栈区符号（`os/kernel-image/src/main.rs` 头注自陈「栈区由链接脚本在 .bss 里预留」；`x86_64.ld` 定义 bottom/top；`check-layout.sh` 校验布局），现状 `_start` 即 `lea rsp, [rip + kernel_boot_stack_top]` 起步——**镜像内已知、已映射、对齐的 bootstrap 栈**，从不读 blob。**裁决建议（成文现状为契约）**：x86 `_start` 起步用镜像内 bootstrap 栈；**禁止在协议头验证之前从 blob 读 `kern_stack_top` 装入 `rsp`**（未经验证的数据不得控制第一段 Rust 执行的栈地址）；`kern_stack_top` 仅在信封验证后由高半跳转（`jump_to_kmain` 一族）使用——那是已验证字段，且现状本就无「预验证换栈」步骤，维持（不为换栈而换栈）。

### E.2 blob 指针寄存器：推荐 `rbx`（裁决建议）

`rdi` 被清零游标占用（`lea rdi, [rip+__bss_start]` 起步、`call` 时 `rdi=__bss_end`）。清零循环只触碰 `rdi`（游标）与 `rax`/`rcx`（值/计数），callee-saved 寄存器全程不动。推荐 **`rbx`**：SysV 明确 callee-saved、无惯用特化职责（`r12` 在部分 runtime 惯作 TLS 基址，虽本内核 no-PIE 无 TLS，仍选更中性的 `rbx`）；trampoline 在 `call` 前以 `mov rdi, rbx` 转交。备选内存槽（`.data` 固定槽）不劣但多一次内存序考虑，无必要。

### E.3 入口状态（维持已裁待实施）

`cli`/`cld` 显式化、16 字节栈对齐、`IF=0` 契约维持 v5 §2.8 已裁口径（含「`cli` 不覆盖同步异常与 NMI」的分开登记）。red-zone 已闭合（round-5 B 批），不重查。

### E.4 `load_elf_into` 的归层：前提归边界，不加 trait（已证实 + 裁决建议）

已证实的事实链：

```rust
// os/arch/src/arch/boot.rs:fn load_elf_into —— 直转 unsafe（注释原文节选）
// SAFETY: `module.start` points to the VM ELF image in physical
// memory; during boot the identity map covers this address range.
let image = unsafe {
    core::slice::from_raw_parts(module.start.0 as *const u8, module.len)
};
```

- 直转点如上——共享层内的隐式恒等假设由 SAFETY 注释自陈。
- 它是**三架构共享 free function**（模块头注自陈三个 per-arch 实现曾逐字节相同、合并为一）；公共入口 `load_vm_elf`（内核 Phase C 装 VM，`os/kernel/src/lib.rs:fn init_proc_and_boot` 两处调用）与 `load_process_elf`（per-image 根装载，test-sysboot RS 直装等）。全部调用点都在 **bootstrap 根活跃期**（首次 `switch_to_user` 之前）。
- 因此「模块物理地址在当前激活根下恒等可读」是**三架构共同的隐式机制假设**，住在共享层。

两候选的比较（评审 E.5）：**a. 前提归边界**——保留读取机制，把前提升格为显式契约（调用方保证模块区间在当前激活根下可读：UEFI 腿由 v5 已裁的 MaxAddress 分配构造保证，直核腿由 bump 区内落点保证），共享层文档写明这是机制前提、不是语义承诺。**b. 改收字节视图**——装载器收 `&[u8]`，物理→虚拟的转换上移调用方（每调用点一行 unsafe）。裁决建议 **a**：三架构读取行为完全同构（同一恒等窗口机制），改签名只是把同一行 unsafe 从装载器搬到 N 个调用点，**不消除假设本身**，却增加调用点数量与 BootModule 物理区间（`img_lo/img_hi` 与 `ImageInsideSegments` 检查仍需物理地址）的双重表示。评审的判据「只在确实消除共享层机制假设时推荐抽象」在此的答案：消除不了，故不抽象。评审的提醒成文：**分配器加上界 ≠ 共享层机制假设升格为架构无关语义契约**——上界保证的是「落点可读」这一事实，装载层文档仍须以机制前提的措辞陈述恒等依赖，两者分层登记。

---

## 汇总：会改变 v5 设计的决策（裁决建议清单）

| # | 决策 | 性质 |
|---|---|---|
| 1 | wire 序列化时点 = EBS 后、切根前（固件映射窗口）；切根后只补运行期信封——四步时序列为 T1/T2 硬性实施条件 | 裁决建议（基于 A 批已证实现状结构兼容） |
| 2 | `KernelInfoView`/`BootReadPhase` 退役；解码边界 = 受控复制函数序列（五步）；`unsafe` 边界收窄为两处（受限原始读取 + 模块直读） | 裁决建议（基于 B 批已证零实现与复制路径论证） |
| 3 | 采单一 handoff arena（B）：EBS 前一次 MaxAddress 分配 2–3 页；`WireSlice.addr` 维持物理地址；新增「全部载荷 ⊆ arena」验证；分类即序列化、无双份数组 | 裁决建议（基于 C 批比较） |
| 4 | x86 栈维持镜像内 bootstrap 栈二段式，禁止预验证读 `kern_stack_top`；blob 寄存器推荐 `rbx` | 裁决建议（基于 E.1/E.2 已证实现状） |
| 5 | `load_elf_into` 前提归边界成文（选项 a），不改签名、不加 trait；「上界 ≠ 语义契约升格」分层登记 | 裁决建议（基于 E.4 已证实调用面） |
| 6 | 方案甲前提成文双标签：「固件页表 EBS 后不变」= 实现事实（真机跑通）+ 规范沉默区；`MAXMEMMAP` = 内核存储容量口径 + 超限显式失败 | 事实澄清（D 批） |

不动项（评审确认维持）：T4 DTB 永久扣除、red-zone 撤销、T3 真实值或显式省略（§3.3 前后文统一属文档清理，非新裁决）、模块 blob MaxAddress 上界与区间末端验证。
