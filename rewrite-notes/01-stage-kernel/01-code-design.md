# 01 · 引导链设计裁决简报（自包含，供多轮外部评审）

> **本文的用途**：`01-stage-kernel` 引导链里每个「需要设计与裁决才能动码」的 TODO，逐条写成自包含简报。目标读者是**看不到本仓库的第三方评审**（网页端 GPT）：每个论断所需的代码、C 源对照片段、既有裁决原文都已内嵌，不需要任何仓库内引用就能完整理解问题与候选方案。
> **与仓库的关系**：每节标题括号里的编号是 `01-todo.md` 的条目号；裁决结论回填 `01-todo.md` 对应条目后由仓库侧实施。已有专门设计文件的主题**不进**本文（大页能力收口 → `boot-dm-hugepage-design.md`，与 GPT 迭代中）。
> **背景速览（三结构体一句话版）**：`KernelInfo` 是引导期语义载荷本体（内存图、内核位置、启动模块……，对应 C 的 `kinfo_t`）；`BootPrepareResult` 是 `BootShim::prepare_boot` 的**进程内返回值信封**（KernelInfo + 根页 + bump 区间，不跨二进制）；`BootHandoff` 是 aarch64 跨镜像跳转的 `#[repr(C)]` **线格式载荷**（KernelInfo + 根页 + bump 区间 + 游标，按裸指针解引用）。前两个的设计有历史可查；`BootHandoff` 是 aarch64 方案 A 跑通途中的救火产物，留下「两信封 90% 重叠」的债——本文 T1 收这条。其余四节（T2–T5）是 x86 跳转协议、启动参数通道、直核腿第四链、内核落地区，各有独立问题。

---

## T1（D-67 / D-69 / D-71 派生）：boot 信封合一与跨镜像布局钉死

### 1.1 现状：三个结构体的完整定义与分工

**`KernelInfo`**（`os/libs/minix-boot/src/kernel_info.rs`，`#[repr(C)]`，跨引导形态共享的语义载荷）：

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],          // 可用物理内存清单
    pub reserved_regions: &'static [MemoryRegion],// 运行期仍要经低地址访问的固件占用区
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
- 两信封的 `KernelInfo` 之外字段 4/5 重叠；`BootPrepareResult` 的存在理由只剩「`BootHandoff.bump_next` 在跨镜像路径上分配 blob 页之后才能读」这一时序差。

### 1.2 布局钉死的两个漏洞（现状自陈）

`KernelInfo` 的 `#[repr(C)]` 只钉字段顺序与 POD 字段偏移，模块注释原文承认：

```text
// NOTE: the `&'static [T]` fields' internal (ptr, len) order is still
// toolchain-defined; the cross-image contract therefore relies on both
// images being produced by one `xtask image` run (identical rustc +
// profile) — see `handoff` module doc.
```

即跨镜像正确性靠**工序纪律**（两个镜像出自同一次构建），不靠结构保证。未钉死的类型自由度两处：

1. 五个切片字段（`memmap`/`reserved_regions`/`boot_modules`/`platform_sources`/`param_buf`）的内部 `(ptr, len)` 顺序——Rust 对 `&[T]` 胖指针的成员顺序没有 ABI 承诺；
2. `free_upper_idx: Option<usize>` 的判别式/niche 布局——`Option<usize>` 是零成本优化目标，布局完全 toolchain-defined，比切片更隐蔽。

另有一处宽度不一致：`MemoryRegion.len: usize`（跨镜像 blob 里宽度随 target 变）vs `kern_size/bootstrap_len: u64`（注释明说用 u64 防 32 位截断）。同一份 `#[repr(C)]` 载荷里同一逻辑概念两种宽度，没有理由。

### 1.3 候选方案

**方案甲（推荐）：单一 `#[repr(C)]` 信封 + 全钉死布局**

1. `BootShim::prepare_boot` 返回类型改 `BootHandoff`；删除 `BootPrepareResult`。进程内路径多读一个 `bump_next` 字段无害（返回时 = 当时光标快照）；跨镜像路径在分配 blob 页之后用一个 mutator 补写游标：

```rust
impl BootHandoff {
    /// Cross-image path: refresh the bump cursor AFTER the handoff blob
    /// page itself has been bump-allocated, so the receiver resumes
    /// allocation past the blob.
    pub fn with_current_cursor(mut self) -> Self {
        self.bump_next = <read current cursor from the boot allocator>;
        self
    }
}
```

2. 五个切片字段换显式 `#[repr(C)]` newtype（构造/`as_slice()` 转换收敛在 minix-boot 边界内）：

```rust
#[repr(C)]
pub struct CPtrLen<T> { ptr: *const T, len: u64 }   // 顺序由本定义钉死，宽度显式 u64
```

3. `free_upper_idx: Option<usize>` 换显式表示（`u64` + 哨兵 `u64::MAX`，或 `#[repr(C)]` 两字段小 enum）；同步改 §3.9 的语义叙述。
4. `MemoryRegion.len: usize` → `u64`（与 `kern_size` 同宽）。
5. 编译期断言补齐：`size_of`/`align_of` 之外加关键字段 offset 断言（现有先例只有 `BootHandoff` 的 4096 上限一条）。
6. 注释改写为 "layout fully pinned; same-build remains a deployment detail"。

**方案乙：保留两信封，只钉布局**——`BootPrepareResult` 加 `#[repr(C)]`、同样钉切片与 Option。省约 30 处构造点改名，但「同一份 boot 状态有两个互为冗余的类型」的设计债仍在，且两个类型的字段同步要靠纪律（今天 `bump_next` 的缺席已经证明纪律会漏）。

**方案丙：信封合一但切片不换 newtype，只把「同批构建」升级为 xtask 强制校验**（构建时校验 rustc 版本/指纹一致）。保留语言层自由度，靠工具链锁死——本质上把结构保证换成另一种工序纪律。

### 1.4 影响面（供评估工作量）

- 定义侧：`os/libs/minix-boot/src/{kernel_info,boot_shim,handoff}.rs`。
- 构造侧：boot-shim 两腿（`uefi_helpers.rs`/`opensbi_helpers.rs`）、kernel-image 两面（`bootface.rs`/`bootface_a64.rs`）、内核与 qemu 测试内核的手工构造点（仓库内 `param_buf: &[]` 形状的构造点约 30+ 处，多数在测试）。
- 消费侧：内核读切片处各加一次 `as_slice()` 转换。
- 文档：01 篇 §3.4（两信封叙述 + 「同一次构建产出」让步句）与 §3.9（`free_upper_idx` 语义）同步。

### 1.5 联动：`build_kernel_info` 双版本统一（D-69）

与信封同批必须处理的缝合面：两个同名函数、不同签名并存——

- `opensbi_helpers.rs:fn build_kernel_info`（8 参，体内硬编码 `reserved_regions: &[]`），被 riscv64 与 aarch64 两个直核面复用；
- `uefi_helpers.rs:fn build_kernel_info`（9 参，多 `reserved_regions` 形参），UEFI 腿专用。

`reserved_regions` 的语义是「x86 内联执行模型特有」的运行期恒等窗口（内核代码链进 boot-shim PE、在低物理地址执行；x86 切跨镜像后该字段在 UEFI 腿也要退场——见 T2）。候选：

- **甲**：统一为单一签名（带 `reserved_regions` 形参），各腿显式传 `&[]` 或非空表——直核面传空成为「显式声明此腿无恒等窗口」而非「函数替你决定」。
- **乙**：拆 per-leg builder（`trait KernelInfoBuilder`），每腿实现自己的字段填充——更显式但为三腿三实现，trait 只有形态差异没有行为差异（项目判据：trait 需要 ≥2 行为不同的实现才成立，三腿构造逻辑同构，倾向否）。

### 1.6 待裁决问题清单

1. 方案甲/乙/丙择一（倾向甲：一次清掉类型冗余与两处布局漏洞；成本是机械改动面大，可与 NK1/x86 切换同批摊薄）。
2. `free_upper_idx` 的显式表示选 `u64` 哨兵还是 `#[repr(C)]` enum？（哨兵更薄；enum 更 Rust 但 repr(C) enum 布局同样要钉 tag 形状。）
3. 切片 newtype 的命名与 API 形状：`CPtrLen<T>` 双字段直露 vs 更语义化的 `BootSlice<T>`？`len` 用 `u64`（显式宽）还是 `usize`（宿主便利、跨镜像再转）？——倾向 `u64`，与 `kern_size` 对齐。
4. `MemoryRegion.len` 宽度统一是否顺带改（`usize` → `u64`）：改则触碰全部消费点（kernel memmap 账、VM 分类），不改则 `#[repr(C)]` 布局随 target 变——跨镜像 blob 只在同 target 对之间传递，宽度一致成立；但「布局随 target 变」与钉死精神相悖。倾向改。
5. D-69 的 builder 统一选甲/乙（倾向甲）。
6. 落地批次：与 T2（x86 切换）同批，还是先行独立落地（纯重构、行为零变化、风险低）？——倾向先行：T2 落地时信封已经是单一定义，x86 腿直接产出正确形状。

---

## T2（I-1 / NK1 / OQ-N6）：x86 从内联执行切到独立镜像跳转

### 2.1 背景与既有裁决（原文内嵌）

本树的引导形态共五种（登记于 `os/libs/minix-boot/src/boot_form.rs`）：x86_64 UEFI 内联（内核静态链进 boot-shim PE、进程内调 `arch_boot`）、aarch64 UEFI 跨镜像、riscv64 OpenSBI 自举、aarch64 QEMU 直核、riscv64 早期 hart 邮箱。

aarch64 走跨镜像的原因是结构性缺陷取证：内核内联在 PE 低地址执行时，第一次进程根切换（换 TTBR0 + `tlbi alle1is`）会摧毁内核自己的取指映射（真机取证：`elr`/`vbar` 钉死在 `0x43e0_xxxx`——内核在切根瞬间取不到自己的下一条指令）。修复是方案 A：boot-shim 建表开分页后**绝对跳**进独立 `kernel.elf` 的高半入口（高半映射住永不切换的 TTBR1），此后整个内核在高地址执行、对根切换免疫。

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

### 2.3 x86 版待裁决问题

x86 版的形状基本是「aarch64 两段去 cfg 门 + asm 换 SysV」，但有五个 x86 特有问题需要裁决：

1. **交接寄存器**：SysV ABI 第一参数寄存器是 `rdi`。x86 的 jump asm 用 `in("rdi") blob_phys`（对位 aarch64 `in("x0")`）？还是用 `rax`/自定义约定？内核入口第一条指令按什么读？（注意内核自己已有先例：`jump_to_kmain` 用 `rdi` 递 `&KernelInfo` + `rdx` 递栈顶——内核侧 `kmain` 取全局副本的设计使参数寄存器仅是形式参数。）
2. **blob 页的分配与恒等映射**：aarch64 依赖「恒等窗口 [0,4GiB) 下 VA==PA 可写」。x86 的 bootstrap 根同样是 [0,4GiB) 恒等 + 高半内核（`IDENTITY_MAP_END = 0x1_0000_0000`，唯一权威 `os/arch/src/lib.rs:pub const BOOT_IDENTITY_MAP_END`），bump 池受 DM 可采上界约束（`min(BOOT_IDENTITY_MAP_END, VM DM 窗口)`）——直接复用同一段即可，还是 x86 需要额外校验（如 blob 页必须 < 4GiB 的编译期断言已在 opensbi 腿先例：`assert!(BUMP_END <= minix_arch::boot_dm_admissible_end())`）？
3. **UEFI 特有的一步：`ExitBootServices` 的时点**。aarch64 UEFI 腿里 EBS 发生在 shim 的 `prepare_boot` 末尾（建表开跳之前）。x86 切跨镜像后时序不变（EBS 仍在 shim 侧），但需确认：EBS 后到 `br` 前的窗口里，UEFI 池页（KernelInfo 切片载荷）必须仍可读——aarch64 已实证「UEFI 池页在 EBS 后存活到首次切根」（`handoff.rs` 模块注释：the boot-services pool survives EBS for the boot window），x86 同理成立？还是需要把切片深拷提前到 shim 侧（blob 里放拷贝而非指针）？——**这是 x86 切换最大的未知数**，候选答案见问题 4。
4. **切片载荷的深拷时点**：T5 指出 UEFI 池堆的页在首次进程根切换后不再被重放。aarch64 跨镜像腿的解法是接收侧 `store_kernel_info` 尽早落 `.bss`（resume 第一步）。x86 版照抄即可（`arch_boot_resume_high_half` 的 x86 版同构）；但 blob 里那份 `*kernel_info` 仍是浅指针——写 blob 与读 blob 之间如果 shim 侧还有任何分配/清零动作（当前没有：写完就 `br`），指针就悬了。裁决点：是否在方案里明文约束「blob 写入后到 `br` 前不得有任何分配」，还是把五块切片在 shim 侧就深拷进 blob 页（blob 上限 4096 字节装不下 memmap 全量——不可行）→ 只能选前者，写成 build_bootstrap_root_and_enable 的调用后契约注释。
5. **切换批次与回退**：x86 切换是一次性翻转（`main.rs` 的 cfg 分支删掉 `arch_boot` 内联路径）还是双轨灰度（QEMU 先行、真机后跟）？内联路径删除后，`reserved_regions` 的 UEFI 腿非空表（x86 内联执行模型的运行期恒等窗口）随之退场——01 篇 §3.4 的 `reserved_regions` 字段叙述与 L1488 的读者 TODO 都指向这个退场，文档同步随同批。

### 2.4 与 01 篇文档的联动

- §3.4 L1422「跨镜像跳转协议还没裁决（NK1/OQ-N6）」→ 改「OQ-N6 已裁（每进程独立根原语，aarch64 交接即其实现）；x86 腿实施收口 = NK1（C-29）」。
- §3.1/§3.3 的终态叙述补「当前产线形态与终态的差距见 §3.4」指针（L1424 张力登记的候选 ①）。
- §4.8 L2624「跳转协议尚未定案」→ 同 L1422 口径。
- x86 切换落地时：§3.7 三形态表 x86 行、§4.8 两处实现现状、§3.4 `reserved_regions` 退场括注、§5.2 证据表同步。

### 2.5 待裁决问题清单

1. 五个 x86 特有问题的取舍（§2.3，尤其问题 3/4 的 EBS 窗口与深拷时点）。
2. 切换批次：一次性翻转 vs 双轨灰度（倾向 QEMU 先行——qemu-tests x86 套件就是灰度验证面）。
3. `bootstrap_to_kernel_image`/`arch_boot_resume_high_half` 的 cfg 门泛化方式：去 cfg 直接三架构共享（x86/riscv64 若同型可复用），还是保留 per-arch？riscv64 直核腿不需要 shim 侧跳转（bootface 在镜像内建表），故这两个函数的跨架构共享实际只有 aarch64+x86 两家——cfg 门删除后 riscv64 编不编这两段（不编译就无所谓共享）。

---

## T3（D-66）：`param_buf` 生产侧接入

### 3.1 语义与 C 参照

`KernelInfo.param_buf` 对应 C 的 `char kinfo.param_buf[MULTIBOOT_PARAM_BUF_SIZE]`（1024 字节，`minix3/minix/sys/arch/i386/include/multiboot.h` 的 `MULTIBOOT_PARAM_BUF_SIZE`）。C 生产侧在 `minix3/minix/kernel/arch/i386/pre_init.c`（`mb_set_param` 写入 `key=value\0` 对，ARCH/BOARD 两个变量必定写入）；消费侧 `minix3/minix/kernel/system/do_getinfo.c` 的 `GET_MONPARAMS` 分支按 `length = sizeof(kinfo.param_buf)` **定长**整块拷给调用者（不足零填充）。

Rust 消费侧已对齐：`os/kernel/src/misc.rs:fn monparams_snapshot` 把切片零填充进 `[u8; 1024]` 再整块返回；空切片进出等价于 C 的 `param_buf[0] == '\0'`。护栏在 `KernelInfo::validate`：`param_buf.len() <= 1024`（fail-fast）。

当前生产侧全部传空：`uefi_helpers.rs` 与 `opensbi_helpers.rs` 的构造点都是 `param_buf: &[]`，注释标记 P9-1。

### 3.2 两条生产腿的接入方案

**UEFI 腿**：UEFI 规范里启动项参数走 **Load Option**（`EFI_LOAD_OPTION`：`EFI_BOOT_SERVICES::LoadImage` 的设备路径指向一个 `\EFI\BOOT\*.efi` 描述结构，其 `OptionalData` 字段携带任意字节）。接入面：

- 候选一：boot-shim 启动时枚举 `LoadOption`（`efivar`/`GetNextVariableName` 读 `Boot0001`…），取当前 `BootCurrent` 对应项，解析 `OptionalData` 为参数正文，按 C `mb_set_param` 的 `key=value\0` 约定拆分。
- 候选二：不读 Load Option，改读 ESP 上约定路径的配置文件（例如 `\EFI\minix\boot.cfg` 的 `args=` 行）——绕开 UEFI 变量服务，实现更薄，但偏离「固件保存的启动项参数」的 C 语义（C 的 multiboot cmdline 来自引导器配置，语义上更接近 Load Option）。
- 裁决点：Load Option 的 `OptionalData` 语义需要按 UEFI 规范现场核实（是 ISO-8859-1 命令行还是任意二进制——Windows 的 `/efi` 用法是文本，但规范不承诺）；1024 护栏截断策略（C 是定长数组天然截断，Rust 切片超长应 fail-fast 还是截断——`validate` 现在是 fail-fast，接入时生产侧主动截断到 1023+NUL 更贴近 C）。

**SBI/直核腿**：riscv64 直核入口实参是 `a0`=hartid、`a1`=DTB、`a2`=BootFileTable（`os/kernel-image/src/main.rs:fn rust_image_main` riscv64 签名），**没有命令行通道**。QEMU `-kernel` 直核形态没有固件参数入口。三个候选：

- 候选一（诚实空）：该腿长期 `&[]`，注释写明「无参数通道」是终态而非欠账——嵌入式 `-kernel` 装机本来没有参数来源；
- 候选二：BootFileTable 扩一个参数表项（U-Boot 腿 `fatload` 一个 params 文件、表里多一行）——成本极低，且 U-Boot 腿本来就有文件系统；直核腿仍空；
- 候选三：DTB `/chosen/bootargs` 解析（标准 Linux 语义，QEMU `-append` 会写进 DTB）——语义最正（固件递参的标准通道），成本是 DTB 字符串 → `key=value\0` 的拆分逻辑。

倾向：UEFI 腿候选一（语义对位 C）、SBI 腿候选三（`/chosen/bootargs`，QEMU `-append` 即测试通道）。两腿都过 1024 护栏。

### 3.3 硬前置：param_buf 落地区（与 T5 联动）

**「只动生产侧、内核零改动」的原登记结论需要修正**：UEFI 腿的参数字节将落在 UEFI 池堆——与 `memmap`/`reserved_regions`/`boot_modules` 三块切片同病（池堆页在首次进程根切换后不被内核页表重放；真机取证 `res=127 zeroed=127` 与 vh1「memmap 15 条在 DM 覆盖时完好、VM 分类点全读零」，摘录见 T5）。而 `GET_MONPARAMS` 的消费点在 syscall 期——远晚于首次切根。因此 D-66 落地**必须**同时在 `store_kernel_info` 增加 param_buf 落地区（T5 案 1 的第二块）。

### 3.4 待裁决问题清单

1. UEFI 腿：Load Option `OptionalData` vs ESP 配置文件（倾向 Load Option，语义对位 C）。
2. SBI 腿：诚实空 vs BootFileTable 扩项 vs DTB `/chosen/bootargs`（倾向 bootargs）。
3. 超长截断：生产侧截断到 1023+NUL（C 对位）vs `validate` fail-fast（现状）——倾向生产侧截断 + validate 保留 fail-fast 双层。
4. `KernelInfo` 补 `param_buf` getter（getter 家族 14 字段缺 1）。

---

## T4（D-68 / I-22 复核）：直核腿第四链收口进 `DtbMemoryMap`

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

模块头注自陈（原文）：「两门全绿后的去重收敛留给独立清理增量」。

### 4.3 问题与修法

三重问题：① 削界算术双份——`clip_reserved` 的编译期护栏（纯固件段→None / 镜像上段→原样 / 跨界段→削尾）只护库内那份，bootface_a64 这份的回归无测试可拦；② 语义出口绕过——下一次削界语义调整（如丁案预留的 `/reserved-memory` 解析）只改库内，aarch64 直核腿静默漂移；③ `static mut MEMMAP_STORAGE` 是 `os/` 下又一处无计数保护的 `static mut`（I-25 同类形态），且 `MEMMAP_MAX=8` 与库内 `DTB_MEMMAP_MAX` 双定义。

修法：`fill_memmap`/`clip_firmware`/`MEMMAP_STORAGE` 整体删除，改调 `DtbMemoryMap::new(&fdt, fw_end).collect()`。**`fw_end` 取值需裁决**：bootface.rs（riscv）传 `KERN_PHYS_BASE = 0x8020_0000`（OpenSBI 常驻 `[0x8000_0000, 0x8020_0000)` 必须削出，OpenSBI 区被 VM 位图元数据 memset 过）；aarch64 直核腿 QEMU 不在 DRAM 里放固件常驻区（DTB 在 RAM 基址 `0x4000_0000`、镜像在 `0x4020_0000`，均为内核自用并自行扣除）——**倾向传 0（不削）**，理由：该腿没有「固件常驻且内核不自知」的区间，削界语义在 aarch64 直核腿无对象；内核侧 `cut_memmap` 已扣镜像自身。

### 4.4 待裁决问题清单

1. `fw_end` 传 0 的理由是否成立（对照 aarch64 直核的固件占位实况）。
2. 迁移批次：独立小批直修（与 T1/T2 无耦合，`bootface_a64` 已依赖 `minix-boot`，改动面一个文件）。

---

## T5（D-70）：内核落地区补全——`platform_sources` 与 `param_buf`

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

- `platform_sources`：UEFI 腿 `find_platform_sources()` 以 `Box::leak` 落在 UEFI 池堆。当前安全的唯一原因是**消费点全部在首次切根之前**——平台发现在 kmain Phase A–D 完成，首次进程根切换在 Phase F `switch_to_user` 之后。这是无结构保证的时序巧合；三块切片当年的发病时序（「Step 4 建表时还完好、VM 分类点读零」）与此同型。
- `param_buf`：当前恒空（D-66 未落地），但消费点 `misc.rs:fn monparams_snapshot` 在 syscall 期——D-66 接入 UEFI 腿后是实害（见 T3 §3.3）。

### 5.2 候选方案

**案 1（推荐）：补两块落地区，与既有三块同构**

```rust
// globals.rs 增：
pub(crate) const PLATFORM_SOURCE_STORE_LEN: usize = 4;   // DTB + RSDP 上限 2，留余量
pub(crate) static PLATFORM_SOURCE_STORE: SyncUnsafeCell<[PlatformDescSource; 4]> = …;
pub(crate) static PARAM_BUF_STORE: SyncUnsafeCell<[u8; MULTIBOOT_PARAM_BUF_SIZE]> = …;
// store_kernel_info 增两段：长度断言 + ptr::copy + repoint——与 memmap 段逐字同构
```

成本约 264 + 1024 字节 `.bss`；换来五块切片全部由结构保证、`store_kernel_info` 成为「KernelInfo 切片载荷唯一落地点」的完整不变量（文档 §4.5 的三块落地区叙述随之改五块）。

**案 2：显式化 consume-before-first-root-switch 契约**——`platform_sources` 字段文档与消费点写明「必须在首次切根前消费完毕」+ debug 断言（如平台发现完成时打桩记录时点）。省字节，但把正确性系在注释与断言上，且 param_buf 一旦接入（消费点在切根后）此案**对 param_buf 不可用**——param_buf 必须落地，platform_sources 可选案 2。

### 5.3 待裁决问题清单

1. 案 1 vs 案 2（倾向案 1 全补：一次消除「三块有结构保证、两块靠时序」的混合状态；param_buf 半无论如何要落地）。
2. `PLATFORM_SOURCE_STORE_LEN` 上限（4 是否合理——SBBR 真机 DTB+RSDP 两源，4 留一倍余量）。
3. 落地区容量断言的测试（与 01 篇 §5.3 TODO-03 的溢出臂测试同批补）。

---

## 附：本文未收录的裁决主题（指针）

- **大页能力/选页策略/启动 DM 安装的四层抽象**（D-59）：`boot-dm-hugepage-design.md`（仓库内，与 GPT 迭代中的专门设计文件；2046 行，含目标设计、五阶段迁移计划与测试矩阵）。
- **OQ-N6 裁决上下文**：`rewrite-notes/coordination/TODO-LEDGER-DONE.md` §2（「每进程独立根」原语与被否的「加栈顶参数」快案）。
- **x86 内核独立镜像的布局产出**：`os/kernel-image/x86_64.ld` 与 `check-layout.sh` L0–L9（已就绪，T2 只裁交接协议）。
