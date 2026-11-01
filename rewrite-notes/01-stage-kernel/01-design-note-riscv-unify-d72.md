# 01 · 两份待裁设计：三架构统一的执行形态（riscv64 是否收编进「shim + 独立内核镜像」）与 bump 池耗尽结构债（D-72）

> **用途与读者**：这份文档是写给**看不到本仓库的第三方评审**的自包含裁决简报。两个议题都需要一次性的方向裁决，裁定后由仓库侧实施，不再展开成多轮设计讨论。
> **自包含声明**：文中引用的代码都以「路径 + 符号 + 摘要」给出，关键片段逐字内嵌；不依赖任何仓库内其它文档即可判断。凡涉及操作系统与 Rust 的通用概念（引导固件、页表、`no_std`、trait）默认读者已具备。
> **标注约定**：每条事实标「已证实」（有代码位置）或「尚无证据」；裁决项与建议分开写，建议不是裁决。

> **设计状态（冻结）**：方向裁决冻结，实施受前置门约束。
>
> 本设计的核心架构目标、三架构统一标准及 D-72 资源归属裁决已经冻结。冻结后不再重新讨论既定方向；仓库侧按裁决实施。
> 议题一在入口契约闭合门通过前，不进入相关接线实施。实现过程中发现的新事实可以触发针对性裁决，但不得无依据地退回各架构分别维护独立的高层启动流程。
> D-72 按候选 A 实施，以映射正确性、VM 页表资源回收、失败路径清理及 bump 池不再承担运行期映射开销作为验收依据。
>
> **文档版本**：本版完成了冻结前的四项一致性修订（§5.5）——文件装载职责表述统一、riscv 第一阶段计划与裁决回填对齐、入口 ABI/固件服务/文件来源三维度区分、两个内核入口的汇合点与验收条件。§1.3 与附录的原始问题清单保留用于复核裁决依据，裁决见 §五。

---

## 0. 核心设计标准（评审回填，作为本议题的裁决标准）

**核心目标（提出方原话）**：三架构统一——操作系统只定义自己需要的语义，架构通过 trait 提供这些语义的实现、屏蔽硬件机制；三种架构共同实现同一套操作系统设计，而不是三套实现逐渐长得相似。

外部评审把这条目标提炼为四条强制约束（用于否决方案，不是代码风格偏好）：

1. **启动契约统一**：无论输入来自 UEFI、OpenSBI、U-Boot 还是受支持的直核启动方式，都必须把各自输入转换成同一份合法的内核启动信息；公共流程不得依赖某个固件特有的寄存器约定。
2. **公共启动流程统一**：入口参数解析与必要的架构初始化完成之后，各路径应尽早汇合；内核不得按 `target_arch` 在高层选择不同的启动设计。
3. **架构差异在实现边界内表达**：页表、地址转换、特权级切换、屏障与控制权转移的差异可以存在于架构实现中；公共层调用它真正需要的语义。不要为了形式统一造空泛 trait——只有当公共代码确实需要一种跨架构能力、且各实现方式实质不同时，trait 才有价值。
4. **任何暂时不统一的实现都必须有明确债务记录**：说明为什么暂时存在、最终收敛到哪里、用什么条件验收。x86-64 的内联执行即属此类（收口任务 T2）。

**评审给出的三层模型**（逻辑分层，不要求机械拆 crate）：

| 层 | 职责 | 允许出现的差异 |
|---|---|---|
| 一、固件入口适配 | UEFI / OpenSBI / U-Boot 入口各自解析自己的调用约定，取得并校验启动信息 | 该固件的寄存器/参数约定 |
| 二、统一启动契约与语义流程 | 共同的启动信息、内存布局约束、页表准备要求、交接载荷验证规则 | **不得出现某架构专属的启动分支** |
| 三、架构能力实现与控制权交接 | 各架构按语义契约完成硬件操作，进入统一内核启动主线 | 指令序列、屏障、地址转换同步 |

**一处必须防矫枉过正**：不要求"全仓禁止 `#[cfg(target_arch)]`"。架构实现知道自己是什么硬件是职责；需要禁止的是"公共 OS 语义因架构不同被拆成不同设计"。判据：将来加第四种架构时，是在为它实现既定语义，还是必须重新设计一条新的 OS 执行流程。

**对 T2 的重新定位（评审要求）**：T2 不再只是"x86-64 的优化任务"，而是**统一启动形态尚未完成的技术债**；在 T2 完成之前，三架构统一是目标状态、不得宣称已实现。直核自引导能力保留，但术语应为**辅助启动路径**（服务嵌入式部署、固件载荷与测试场景，继续维护与验收），不称"降级/废弃"。

## 议题一：三架构统一的执行形态

### 1.1 目标与当前状态

**目标（提出方立场）**：x86-64、aarch64、riscv64 三个架构应共用同一条主线——**独立的 boot-shim 可执行体 + 独立的内核镜像，经交接载荷（handoff）连起来**。理由是分层职责：boot-shim 的职责是**为内核屏蔽固件形态**（UEFI、U-Boot/OpenSBI，或将来的其它固件），内核镜像只接收一份与固件无关的启动信息。**统一的是内核所依赖的启动契约与启动语义，不强制统一启动文件的物理装载者**：UEFI 入口可以经文件系统协议取得文件，U-Boot 可以预先装载内核与模块并提供地址清单——两者都必须向共同启动流程提供经过验证的启动信息。“谁读文件、谁建根、谁交接”可以按平台选择；需要抽象时，抽象出有明确语义的能力（例如“取得一段已校验的文件字节”），不为了统一而强迫每种平台实现同一种文件读取机制。差异只落在 boot-shim 内部的固件实现。 另一个支撑该目标的观察：非 `FW_PAYLOAD` 的 riscv 硬件生态正在向 UEFI 收敛（与 ARM 服务器同路），这意味着 riscv 的固件形态会继续增加而不是减少——把"固件差异"关进 boot-shim 内部、让内核镜像不变，正是应对这种漂移的结构。若将来 riscv 出现可用的 Rust UEFI 目标，shim 只是多一个固件实现，主线形状不变。

**已证实的三架构现状（代码位置 + 摘要）**：

1. **契约层已经是统一形状**。`os/libs/minix-boot/src/boot_shim.rs` 定义 `pub trait BootShim { fn prepare_boot(bump_pages: usize) -> BootPrepareResult; }`，两个实现：`os/boot-shim/src/uefi_helpers.rs:struct UefiBootShim` 与 `os/boot-shim/src/opensbi_helpers.rs:struct OpenSbiBootShim`。两个实现都产出同一个 `KernelInfo`（14 字段的启动信息结构）与根页表物理地址。**这条契约本身已经与固件无关**。

```rust
// 契约（os/libs/minix-boot/src/boot_shim.rs，节选）
pub struct BootPrepareResult {
    pub kernel_info: KernelInfo,   // 14 字段的启动信息（内存图、内核位置、模块表……）
    pub root_page: PhysBytes,      // 自举根页表物理地址
    pub bump_base: u64,
    pub bump_end: u64,
}
pub trait BootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult;
}
```

```rust
// 分道（os/boot-shim/src/main.rs，节选；固件实现由构建特性二选一，交接由架构分道）
#[entry]
fn main() -> Status {
    let result = UefiBootShim::prepare_boot(1024);   // ← 此处固件实现可换：UefiBootShim / OpenSbiBootShim
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);
    #[cfg(target_arch = "aarch64")]
    minix_kernel::bootstrap_to_kernel_image(&result.kernel_info, result.root_page); // 跨镜像交接
    #[cfg(not(target_arch = "aarch64"))]
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);                  // 内联（x86 过渡形态）
    unreachable!()
}
```
2. **UEFI 腿是"shim 独立产物"**：`os/boot-shim/Cargo.toml` 的 `[[bin]] required-features = ["fw-uefi-image"]`，`fw-uefi-image` 隐含 `uefi`；`os/boot-shim/src/main.rs` 的 `fn main` 直接调用 `UefiBootShim::prepare_boot(1024)`，之后按架构分道：aarch64 走 `minix_kernel::bootstrap_to_kernel_image`（建自举表、开分页、绝对跳进独立 `kernel.elf` 高半入口，交接载荷 `BootHandoff` 放在第一参数寄存器 `x0`），x86-64 目前是**内联执行**（内核代码静态链接进 shim 的 PE 镜像，进程内调 `arch_boot`，这是已裁定的过渡形态，收口任务登记为 T2）。
3. **riscv64 腿没有接线**：`OpenSbiBootShim::prepare_boot` 在仓内**零调用方**（`os/boot-shim/Cargo.toml` 只给 UEFI 建 bin；`opensbi` feature 存在但不隐含任何 bin）。riscv64 的当前产线链是**内核镜像自引导面**：U-Boot 用 `fatload` 把 `kernel.elf` 与模块读进内存、`bootelf` 直接跳进内核镜像入口，镜像内的 `os/kernel-image/src/bootface.rs:fn bootface` 自己解析设备树、自己分配页、自己调 `arch_boot`。代码注释把这条链称为"生产装机面"，原因是它在 `2026-09` 落地时 `OpenSbiBootShim` 这条腿没有 wiring，并不是"架构上不需要独立 shim"。
4. **riscv64 的自引导面有几处只对 QEMU 成立**：
   - 页池基址硬编码：`opensbi_helpers.rs` 的 `MODULE_REGION_BASE = 0x8000_0000 + 0x0200_0000`（声明为"QEMU virt DRAM 基址 + 32 MiB"），`MODULE_REGION_SIZE = 32 MiB`，注释原文 "OpenSBI has no firmware to call; everything is hardcoded"。真板 DRAM 基址不一定是 `0x8000_0000`，这 32 MiB 也未必空着。
   - 入口参数通道只对 OpenSBI 直载成立：`install_dtb_ptr(a1)` 的文档写明"OpenSBI 把设备树物理地址放在 `a1` 交给 S 模式载荷"；内核镜像 riscv64 入口 `rust_image_main(boot_hart: u64, dtb_phys: u64, boot_file_table_pa: u64)`（参数按 `a0`/`a1`/`a2` 约定）同样依赖这个约定。
   - U-Boot 的 `bootelf` **不保证**上述寄存器约定：它把控制权跳到 ELF 入口时不按 OpenSBI 的 `a0`/`a1` 语义置参。这是议题一最大的技术未知点。
5. **aarch64 分离腿的全套件已在**：`os/kernel/src/lib.rs:fn bootstrap_to_kernel_image`（shim 侧：建表 + 开分页 + 分配载荷页 + 写 `BootHandoff` + `isb; br` 绝对跳转）与 `:fn arch_boot_resume_high_half`（接收侧：只做 additive 阶段——校验、恢复页池游标、深拷启动信息、建立 Direct Map、进主流程，绝不重建根）。两者当前都是 `#[cfg(target_arch = "aarch64")]`；`bootstrap_to_kernel_image` 的泛型参数是 `AArch64Paging`，函数体里真正与架构绑定的只有 `isb; br` 两条指令。
6. **交接载荷是架构无关的**：`os/libs/minix-boot/src/handoff.rs:struct BootHandoff` 是 `#[repr(C)]` 的纯数据（启动信息 + 根页物理地址 + 页池区间与游标），不含任何架构概念；跨架构复用它不需要改类型，只需要按目标架构的入口约定传递指针。

### 1.2 提案：riscv64 收编进主线（分三步，每步可独立验收）

**第一步：把 shim 腿接上线（只做"能跑"，不碰真板）**。

- `os/boot-shim/Cargo.toml` 增加 riscv64 bin 的 feature 词表（例如 `fw-riscv-opensbi = ["opensbi"]`），`main.rs` 按 feature 选固件实现（`OpenSbiBootShim` 与 `UefiBootShim` 二选一），bin 目标按 `riscv64gc-unknown-none-elf` 构建。
- **入口参数契约（已裁，见 §5.2/§5.3）**：使用 `bootelf` 的文档化参数机制——`bootelf [-p|-s] [-d <fdt_addr>] [<image_addr> [<arg>]...]`，`image_addr` 之后的参数以 `argc, argv` 交给应用，`-d` 按地址设置设备树（需 U-Boot 构建启用对应配置，实测核实）。解析与校验限制在**入口适配层**，不依赖 OpenSBI 的 `a0/a1` 语义；设备树地址、启动清单地址及有效性校验写入入口契约。`go` 带参不作为默认方案。
- **装载责任（已裁）**：可由 U-Boot 预载 `kernel.elf`、12 个模块与 `BootFileTable`（沿用既有 `MNXBOOT1` 契约），shim 经 `FileLoader` 能力消费预载清单；也可由 shim 自读——两者都向共同启动流程供给经校验的字节，Gate 条件见 §5.3。
- 验收：QEMU 里 U-Boot 脚本 `fatload` 上 `boot-shim.elf`、`bootelf` 带参跳进去，shim 打印自己的路标、从预定来源取得 12 个模块字节、建表、跳进 `kernel.elf` 高半并跑到内核自己的第一个路标。这条链上统一的是"内核拿到经校验的启动信息"这一语义，不是"由谁做物理读取"。

**第二步：把交接与接收泛化到 riscv64**。

- `bootstrap_to_kernel_image` 的 cfg 门从 aarch64 放宽到 aarch64+riscv64（x86 随 T2），泛型换成对应架构的 `Paging` 实现。**待实施验证的预期（不是已证事实）**：架构相关部分只剩跳转指令与寄存器名。地址转换同步（例如 RISC-V 的 `fence`/`sfence.vma` 与指令缓存 `fence.i` 的职责划分）、执行环境与特权状态、控制权转移的屏障要求，都必须由对应架构实现在实施时逐项确认并写明，不得因为"函数可以泛化"就默认这些要求已经满足。
- `arch_boot_resume_high_half` 同批泛化（函数体是纯 additive 的 Page 操作，架构差异在 `Paging` 实现里）。
- `kernel-image` 的 riscv64 入口要**分道**：直核场景（QEMU `-kernel`、真板 `FW_PAYLOAD`）仍走 `bootface`，shim 交接场景走新入口。aarch64 的分道办法是"`x0 == 0` → 自引导面，否则视为载荷指针"，这个办法在 riscv64 **不成立**：直核时 `a0` 是 hartid，可能是 0 也可能不是。所以需要一个新的、不可混淆的分道判据（见 1.3 问题二）。
- 验收：两种形态（`bootelf` + shim、`-kernel` 直核）在 QEMU 上各自跑通，且互不回归。

**第三步：真板化（可后置，但必须先定方向）**。

- 页池基址与大小从设备树 `/memory` 推导（或由构建期配置注入），删除 `MODULE_REGION_BASE` 这类 QEMU 常量；编译期断言 `BUMP_END <= boot_dm_admissible_end()` 换成运行期检查 + 明确失败。
- 真板窗口内验证"U-Boot 装载 shim → shim 装载内核"的完整链。

### 1.3 需要评审裁决的三个问题（原始待裁清单；裁决已记录于 §五，保留用于复核依据）

**问题一（最关键）：shim 在 U-Boot 下从哪里拿设备树地址？** 候选：

1. **U-Boot 环境变量 + 约定内存槽**：boot 脚本把设备树地址写到一个约定地址（U-Boot 的 `setenv` 只能存字符串，实际做法是脚本读取一个已知的 U-Boot 符号或固定槽），shim 从该槽读。缺点是引入一个"脚本与 shim 的隐式契约"。
2. **U-Boot 显式传参**（**裁决修正版**）：使用 `bootelf` 自身的文档化参数机制（`bootelf [-p|-s] [-d <fdt_addr>] [<image_addr> [<arg>]...]`，参数以 `argc, argv` 交给应用），由入口适配层解析校验；不以 `go` 替代 `bootelf`（裁决见 §5.2）。
3. **shim 自己扫内存找 FDT 魔数**：设备树头有固定魔数 `0xD00DFEED`，可在 DRAM 前若干 MiB 扫描。缺点是扫描范围需要上界、可能误命中，且是"猜"不是"契约"。
4. **构建期注入**：boot 脚本把设备树复制到 / 打包成随镜像的 blob（QEMU 测试友好，真板不通用）。

**裁决（§5.2）**：采用修正版候选 2——`bootelf` 文档化参数机制 + 入口适配层解析校验（设备树魔数 `0xD00DFEED` 校验，先例：`BootFileTable` 的 `MNXBOOT1`）；候选 1/3/4 分别降为特定板级配置、拒绝作正常路径、仅测试/固定平台。`-d` 依赖 U-Boot 构建配置，须在实际版本实测核实。

**问题二：riscv64 内核镜像的两种入口如何分道？** 候选：

1. **两个独立的 ELF 入口符号**（例如 `_start_shim` 与 `_start_direct`），链接脚本按构建特性二选一。最干净，代价是构建矩阵多一维。
2. **同一入口 + 魔数载荷**：shim 传的指针指向的交接载荷以固定魔数开头，直核时 `a0` 是 hartid（小整数，解引用前先判是否落在已知可读区间）。需要"读之前先验证地址合法性"，比 aarch64 的 null 判据复杂。
3. **同一入口 + 专用寄存器约定**（例如约定 `a3` 携带魔数，直核时由 QEMU/固件保持 0）。依赖"调用方按约定置寄存器"，U-Boot `bootelf` 不保证寄存器初值，风险较高。

建议：**1**，因为 riscv64 的两种形态来自两个不同的装载者，构建期区分比运行期猜更可靠。

**问题三：统一后的定位与术语**。当前的文档把 riscv64 的自引导面称为"生产装机面"，把三种形态称为"平行的装机面、不合并"。若采纳统一方向，需要明确：自引导面（`bootface`/`bootface_a64`）降级为**特定嵌入式形态**（riscv 的 `FW_PAYLOAD`、QEMU `-kernel` 测试通道），而**三架构的主线 = shim + 独立镜像**。请评审确认这个定位，以及"降级"是否准确（它仍要保留、仍要测试，只是不再是主产线）。

### 1.4 风险与成本

- **改动量（估计）**：第一步 ≈ 一个新 bin 目标 + `main.rs` 分道 + 入口参数契约（几十行）；第二步 ≈ 两处 cfg 泛化 + riscv 入口分道（几十行到一百行）；第三步 ≈ 池基来源改造 + 真板验证（窗口依赖）。
- **最大风险**：U-Boot `bootelf` 的寄存器/参数行为在不同版本上不一致——这也是问题一必须先裁的原因。若无法达成可靠契约，议题一的结论可能是"QEMU 先统一、真板等固件通道"，而不是无条件统一。
- **不做的代价**：riscv64 继续由内核镜像自己承担固件对话（`bootface` 里硬编码 QEMU 布局、依赖 OpenSBI 寄存器约定），三架构的分层目标只在两架构成立；每加一种 riscv 固件形态，内核镜像里就多一条引导分叉。

---

## 议题二：D-72 —— bump 池耗尽的结构债

### 2.1 事实

**bump 池是什么**：`os/boot-shim/src/main.rs` 调用 `UefiBootShim::prepare_boot(1024)`，`1024` 是**引导期页表页预算**（1024 × 4 KiB = 4 MiB）。这个池通过 `os/kernel/src/boot_alloc.rs` 的 `BootAlloc` 管理：单调前进的游标、失败回滚一格、**没有任何归还路径**（`alloc` 只做 `fetch_add` 与越界回滚）。池里的页有两个用途：建自举页表的中间页，以及——**下面这个关键点**——运行期为每个新地址空间根注入内核信息页时需要的中间页表页。

**内核信息页的注入链（已证实，唯一注入点）**：内核在启动后段构建一页"内核信息"（`os/kernel/src/kerninfo.rs` 的 `KERNINFO_PAGE`，内容是内核版本、用户栈顶等只读数据），把它映射到每个用户地址空间的固定虚拟地址 `0x2_0000_0000`。映射动作发生在 `os/kernel/src/syscall.rs:fn vmctl_set_addr_space`（VM 服务在绑定新地址空间根时发起的唯一提交点）：查目标根里该虚拟地址是否已映射，未映射就在目标根里装一条用户只读页表项，**中间页表页从 bump 池取**。代码注释自陈这是临时方案："mapping-responsibility 转移尚未发生，不新增外部契约"。

```rust
// 注入点（os/kernel/src/syscall.rs:fn vmctl_set_addr_space，节选；注释为原文摘录）
// ── 把 kerninfo 页的用户只读映射复制进每个新根。C 把 `.usermapped` 段经
// VMCTL_KERN_PHYSMAP 协议映射进每个进程地址空间；本重写由内核在本唯一提交点
// 代做（kerninfo.rs 模块 doc 登记的 mapping-responsibility 转移尚未发生，
// 不新增外部契约）。中间表页经 boot_pt_alloc（其区域已从 VM free list 扣减）。
if let Some(ki_pa) = crate::kerninfo::kerninfo_page_phys() {
    let mut target_paging =
        minix_arch::CurrentPaging::from_active_root(minix_types::PhysBytes(ptroot_phys));
    let needs_map = match target_paging.query(VirBytes(KERNINFO_USER_VA)) {
        Some((cur, _flags)) => cur.0 != ki_pa,
        None => true,
    };
    if needs_map {
        target_paging
            .map(VirBytes(KERNINFO_USER_VA), PhysBytes(ki_pa), PageFlags::read_only())
            .expect("vmctl_set_addr_space: failed to map the kerninfo page into the new root");
    }
}
```

**数字（代码注释原文）**：`prepare_boot(1024)` 上方的注释记录——引导自身约需 100 页；每次新根绑定（exec 重建根，每子进程一次）的注入链要 1–3 页；**池无回收路径，真机在约 40 次绑定后 `AllocationFailed` 死亡**（当时的池更小）；1024 页 = boot 需求 + 每绑定开销 × 数百次绑定 × 安全系数。也就是说当前上限约为**两三百次 exec 重建根**，之后确定性失败。失败形态已经是响亮的：注入点 `.expect(...)` 直接 panic（`vmctl_set_addr_space: failed to map the kerninfo page into the new root`），不是静默。

**额外成本**：VM 启动交接的记录把整个 bump 池区间**整段**从 VM 的空闲内存里扣除（用掉的与没用的都算内核保留），所以这 4 MiB 是每次启动固定损失的 VM 可分配内存。

### 2.2 两个候选

**候选 A：把映射责任移交 VM（推荐）**。

- 做法：内核不再在 `vmctl_set_addr_space` 里注入页表项；改为把内核信息页的**物理地址**交给 VM，由 VM 在构建每个新根时自己在固定虚拟地址 `0x2_0000_0000` 装这条用户只读映射。VM 的页表页来自 VM 自己的物理内存分配器（进程退出时会归还），因此**绑定次数不再有上限**；bump 池恢复为"只服务引导期建表"。
- 内核侧改动：删除注入块（约 30 行）；把 `kerninfo_page_phys()`（当前唯一消费者就是这个注入点）的结果发布给 VM。发布通道候选：① 在 VM 交接载荷（`os/libs/minix-types` 的 `VmBootHandoff`，带 magic/version）里加一个字段，版本号加一——同批构建契约不受影响，且不碰用户态 ABI；② 新增一个 VMCTL 查询调用——多一条进程间协议面。
- VM 侧改动（跨线：VM 服务属另一条工作线）：建根路径里多一条固定地址映射；`fork` 继承与 `exec` 重建两条路径要分别确认（若 `fork` 走页表复制会天然继承，则只需 exec 重建路径）。
- 风险：用户态通过 `MINIX_KERNINFO` 系统调用拿到该页的用户虚拟地址并直接读；固定地址与只读语义必须保持不变。VM 侧若漏映射，用户态第一次读会缺页——属于"响亮失败"，但要在验收里覆盖。

**候选 B：给 bump 池加归还路径**。

- 做法：`BootAlloc` 增加归还接口，VM 销毁地址空间根时把注入链用掉的页表页还回池子。
- 问题：这些页是在**内核上下文**里分配的（注入点在内核侧），而 VM 销毁根时跑在 **VM 自己的上下文**里，其页表分配器注册的是 VM 自己的实现。要让页回到 boot 池，需要跨上下文的页归属追踪（哪一页来自哪个分配器），改动比候选 A 大，收益只是"同一个池可重复用"，并没有把每绑定的开销移出引导期资源。

### 2.3 需要评审裁决的问题

1. **方向**：候选 A 还是 B？（建议 A：责任归位——谁建根谁映射，与"每个地址空间自己的页表自己建"的既有分工一致。）
2. **物理地址传递通道**：交接载荷加字段 + 版本号（建议）还是新增查询调用？若加字段，`VmBootHandoff` 的版本策略是一律加一，还是允许向后兼容的小版本？
3. **`fork` 与 `exec` 两条路径**：`fork` 若复制页表是否天然带上这条映射？需要评审给出判断依据（C 原型的语义是每个地址空间一条用户映射，`fork` 复制后仍指向同一物理页——这与本方案一致）。
4. **验收**：真机上连续 exec 重建根 N 次（N 远大于 40），不出现 `AllocationFailed`；用户态 `query_kerninfo` 在首次 exec 后仍能读到正确内容；bump 池用量不再随绑定次数增长（若保留 4 MiB 池，其扣除代价是否随 A 方案下降？——A 方案下池仍需服务引导期建表，整段扣除的记账不变，但"增长"消失）。

### 2.4 与议题一的关系

两者独立：D-72 是运行期内存所有权问题，与引导形态无关；议题一是引导期分层形态。可以分开裁决、分开实施。若两份都采纳，实施顺序建议议题二的候选 A 先做（它更小、风险更低、能先解除确定性上限），议题一按 1.2 的三步推进。

---

## 附：需要评审回答的清单（原始问题，裁决已记录于 §五，保留用于复核依据）

**议题一**：① 是否同意"三架构主线 = shim + 独立镜像，自引导面降为特定嵌入式形态"？② U-Boot 下 shim 的设备树来源选哪条（四个候选）？③ riscv64 两种入口的分道用哪种判据（三个候选）？④ 是否接受"QEMU 先统一、真板等固件通道"的分期？

**议题二**：① 候选 A 还是 B？② 物理地址传递通道与版本策略？③ `fork`/`exec` 路径的判断？④ 验收条件是否够（N 次 exec + 用户态读取 + 池增长消失）？

---

## 五、裁决回填与仓库侧确认（八轮评审）

### 5.1 裁决结果（外部评审）

| 待裁项 | 裁决 | 限制或前提 |
|---|---|---|
| 三架构目标形态 | **通过** | 独立 shim + 独立内核镜像作为目标主线；统一的是内核所接收的启动契约与控制权交接，不一定是每个环境都由同一组件读文件 |
| riscv64 收编进主线 | **有条件通过** | 先闭合入口 ABI、文件/模块来源、地址与内存布局契约（入口契约闭合门） |
| U-Boot 设备树来源 | **修正候选二后通过** | 用 `bootelf` 的文档化应用参数协议；不依赖 `a1` 自动携带 FDT |
| riscv64 两种入口分道 | **通过候选 1** | 两个明确区分的构建产物（`kernel-direct.elf` / `kernel-shim.elf`）与入口契约，不做运行时猜测 |
| 直核自引导面定位 | **通过** | 称"辅助/兼容启动路径"，继续维护与测试；不称降级 |
| QEMU 先行、真板后置 | **通过** | QEMU 通过不等于真板契约成立，验收分开 |
| D-72 资源归属方向 | **通过候选 A** | 内核拥有信息页，VM 拥有地址空间中的映射与对应页表资源 |
| 物理地址发布方式 | **有条件通过 `VmBootHandoff` 字段** | 必须先证实该地址在交接构造时已确定（本版已证实，见 5.2） |
| 交接载荷版本策略 | **增加必需字段后升版本** | VM 必须拒绝不支持的布局版本，不得静默漏映射；可考虑加结构尺寸字段；不要把必需字段做成可缺省可选值 |
| `fork` 与 `exec` 映射 | **分别验收** | 是否自然继承取决于真实页表复制路径（本版已核实：两者都在 VM 新建根，不继承，见 5.2） |
| bump 池预算 | **候选 A 完成后复核** | 不再把运行期地址空间绑定开销计入启动池预算；按各架构实测峰值重新裁定 |

### 5.2 三项仓库侧确认（本版核实结论）

**（一）riscv64 的实际 U-Boot 启动现场**

- 现有 QEMU 测试链（`os/qemu-tests/test-riscv64-uboot.sh`）：宿主 `/usr/lib/u-boot/qemu-riscv64_smode/uboot.elf`（Ubuntu 24.04 的 `u-boot-qemu`，注释记 2025.10），smode 形态需垫在 OpenSBI（`-bios default`）之下；裸机形态直接作 `-bios`。启动脚本只有两行：`fatload virtio 0 ${loadaddr} boot.elf` 与 `bootelf ${loadaddr}`——**不传参数、不传设备树、只装一个 ELF**（被装的是 `hello-boot-riscv64` 测试内核，不需要 DTB 与模块表）。
- 现有 riscv 生产链（`bootface`）的文件来源：U-Boot 脚本用 `fatload` 预载 `kernel.elf`、12 个模块与 `BootFileTable`（魔数 `MNXBOOT1`），镜像内 `bootface` 消费这张表（表项给出物理地址与长度）。**也就是说"由 shim 读文件"这条承诺在 riscv 上原本就不是字面成立**：固件把文件预载进内存、把地址表交给下一级。与评审结论一致——必须实现的是"内核不读固件专有格式、不依赖固件入口寄存器约定"；"谁读文件"由平台选择，接缝就是 `FileLoader` 这个抽象（UEFI 实现走文件系统协议；opensbi 实现走预载表）。
- 由此 入口契约闭合门 的第一项有明确答案：**用 `bootelf` 的文档化参数协议**（官方文档：`bootelf [-p|-s] [-d <fdt_addr>] [<image_addr> [<arg>]...]`；`image_addr` 之后的参数以通用 `argc, argv` 形式交给应用；`-d` 按地址设置设备树）。入口适配层解析 argc/argv 取得设备树与启动清单地址并校验，再把它们转换成统一启动输入；不依赖 OpenSBI 的 `a0/a1` 语义（那两种约定不能混用）。

**（二）`VmBootHandoff` 的构造时序（已证实，候选 A 的前置成立）**

- `kerninfo::init_kerninfo` 在 **kmain Phase B.5** 执行（`os/kernel/src/lib.rs` 的 `kmain` 里，`boot_stage!("kernel: kmain B.5 kerninfo page ok")` 之前），其中把内核信息页映射进活动根并**缓存物理地址**（`kerninfo.rs` 的 `KERNINFO_PAGE_PHYS`，由 `map_and_publish` 写入）；`kerninfo_page_phys()` 在此之后返回稳定地址。
- `vm_handoff::build_vm_handoff` 在 **Phase C** 的 `init_proc_and_boot` 内执行（同一文件，注释 "Classification point (A2)" 段）。Phase B.5 早于 Phase C。
- **结论**：在构造 `VmBootHandoff` 时内核信息页物理地址已经确定，直接加字段不需要倒置任何初始化顺序；无需为此新增 VMCTL 查询面。

**（三）bump 分配器与 VM 页表生命周期（候选 A 的归属前提成立）**

- `boot_alloc::boot_pt_alloc` 的注册点：boot-shim `main.rs`（交接前）、`bootface_a64.rs`（直核腿）、内核 `boot_validate_and_prepare` 的兜底分支。内核上下文里所有 `Paging::map` 的中间页表页（包括 D-72 的注入点）都从它取——这就是"运行期地址空间绑定消耗启动期资源池"的机制。
- VM 侧有独立的分配器与**归还钩子**：`os/servers/vm/src/alloc_page.rs` 的 `vm_pt_alloc`/`vm_pt_free`，经 `minix_arch::pt_alloc::register`/`register_free` 注册。也就是说"VM 的页表页可随地址空间销毁回收"是既有机制，不是要新建的能力——候选 A 只是把映射动作挪到拥有这套机制的一侧。
- **`fork` 与 `exec` 都新建根，不继承**：`os/servers/vm/src/fork.rs` 的 `do_fork` 先 `pt_new` 建子进程新根、再 `fork_regions` 复制父进程**已登记的虚拟区间**、最后 `sys_vmctl_set_addrspace` 把根交给内核；`os/servers/vm/src/vm_server.rs` 的 `exec_bootproc` 同样 `pt_new` + `pt_bind` 重建根。内核信息页的映射不是 VM 的登记区间，**不会被这两条路径天然带上**——所以候选 A 的实现点应放在 VM 的"根创建"统一函数里（`pt_new` 之后、`set_addrspace` 之前），一处覆盖 fork 与 exec；两条路径仍要分别验收。

### 5.3 入口契约闭合门（实施前置；通过前不开始相关接线）

1. **U-Boot 版本、配置与启动命令**：目标版本（QEMU 测试链用 Ubuntu 24.04 的 `u-boot-qemu` 2025.10；真板另行核实）×`bootelf` 的参数传递与 `-d` 支持是否启用（需相应配置）。
2. **shim 如何接收设备树**：`bootelf -d <fdt_addr> <shim_addr> <args...>`；入口适配层把 argc/argv 解释为（设备树地址、启动清单地址……）并校验（FDT 魔数 `0xD00DFEED`、地址落在已知内存范围内）。
3. **`kernel.elf` 与 12 个模块由谁装载**：U-Boot 预载（沿用 `BootFileTable` 契约）还是 shim 自读；装载位置、长度、内存占用范围全部进契约。
4. **shim 内存布局**：页池、设备树、内核镜像、模块与交接载荷互不重叠（现 `MODULE_REGION_BASE` 等 QEMU 常量必须改为从设备树 `/memory` 推导或构建期注入）。
5. **两个内核产物的入口契约**：`kernel-direct.elf`（直核：`a0`=hartid、`a1`=设备树，现有 OpenSBI 语义）与 `kernel-shim.elf`（只接收交接载荷）；两者各自的特权状态、页表状态、寄存器状态要求写清。

### 5.4 D-72 实施要点（按裁决修订）

- **所有权三分**：`KERNINFO_PAGE` 归内核（长期保留、内容稳定、不得被 VM 当普通帧回收）；每个地址空间的 `KERNINFO_USER_VA` 映射归 VM（建根时建立、随页表生命周期销毁）；建立该映射的中间页表页归 VM 的页表分配器（建根/销毁流程负责分配与回收）。
- **不为 `KERNINFO_PAGE` 的所有权松口**：映射责任移交不等于帧所有权移交；fork/exec 都不得释放该帧，也不得把映射改成可写。
- **版本策略**：`VmBootHandoff` 加字段即升载荷布局版本；VM 显式验证版本，不认识就拒绝（不得静默漏映射）；可考虑补结构尺寸字段；不把必需字段做成可缺省。
- **异常路径**：核验 VM 在部分建表失败、根创建失败、进程退出时的页表页释放，防止新分配的页在异常路径泄漏。
- **验收四项 + 一项资源断言**：① 连续 exec 重建根远超原耗尽次数后 `query_kerninfo` 读取正确（覆盖 fork 与 exec 组合）；② 反复创建/销毁地址空间后 VM 页表分配器资源回到稳定水平；③ 固定虚拟地址、用户可读、只读语义、所指向物理页正确（fork 后仍指向同一页、仍只读）；④ 建根或映射中途失败时页表页可释放；⑤ **直接断言：地址空间创建与销毁期间，内核侧不再经这条路径调用启动期 bump 分配器**（比"跑一千次没崩"更强）。
- **4 MiB 复核**：候选 A 只消除运行期增长，不自动归还启动时已整段保留的 4 MiB。随后按各架构实测启动期峰值重新裁定 `bump_pages`，确认固定保留成本与 VM 空闲内存记账是否仍合理。

### 5.5 冻结前的四项一致性修订（本版已落实）

**（一）文件装载职责的表述统一**：冻结的原则是"统一内核所依赖的启动契约与启动语义，不强制统一启动文件的物理装载者"（§1.1 已改述，§1.2 第一步验收同批对齐）。UEFI 入口经文件系统协议取文件、U-Boot 预载并交地址清单，两者都向共同启动流程提供经过验证的启动信息；需要抽象时抽象"取得一段已校验文件字节"这类有明确语义的能力（现有 `FileLoader` 即此形状），不强迫各平台实现同一种文件读取机制。

**（二）riscv 第一阶段计划与裁决对齐**：§1.2 第一步改按 `bootelf` 文档化参数机制（参数以 `argc/argv` 交应用、`-d` 设置设备树），入口适配层解析校验，不依赖 `a0/a1`；`go` 带参不再作为默认方案；装载责任写明"U-Boot 预载 + `BootFileTable` 或 shim 自读"两选，由 §5.3 第 3 项定案；`-d` 依赖 U-Boot 构建配置，实际版本必须实测核实（不能只凭通用文档认定测试环境已启用）。

**（三）三个维度的区分（入口 ABI / 固件服务 / 文件与启动信息来源）**：现有 `UefiBootShim`/`OpenSbiBootShim` 的命名容易把三个本来不同的问题混为一谈：

| 维度 | 真正需要回答的问题 |
|---|---|
| 入口 ABI | 谁启动了当前程序？参数以什么约定传入？ |
| 固件服务 | 当前环境提供哪些底层服务（文件系统、内存分配、控制台……）？ |
| 文件与启动信息来源 | 内核、模块、设备树与启动清单如何取得？ |

一个由 U-Boot 启动的 shim 运行在 OpenSBI 提供运行时服务的环境之上，但它的**入口参数并不因此变成 OpenSBI 的内核入口参数**；它的文件来源也可能是 U-Boot 的预载清单，而非 OpenSBI 的文件服务。**这三个维度不要求三个 trait、也不要求拆三个 crate**——关键是不要把它们错误地定义成互斥的一组"固件实现"。`BootShim`/`FileLoader` 与入口适配模块的边界由仓库侧按现有职责裁剪，实施时不得把三个维度固化成同一个选择开关。

**（四）两个内核入口的汇合点（写入验收标准）**：`kernel-direct.elf` 与 `kernel-shim.elf` 可以有不同的入口参数解析与必要的架构初始化，但**完成适配后必须进入同一套公共内核启动语义流程**（同一段：校验启动信息 → 建立/接收自举根 → 落地区深拷 → 建立 Direct Map → 进入主流程）。不得因为入口不同就各自维护一套长期独立的高层启动流程。另：§1.2 第二步里"泛化后架构差异只剩跳转指令和寄存器"的表述已改为**待实施验证的预期**——地址转换同步、执行环境、控制权转移的屏障要求必须由各架构实现在实施时逐项确认并写明。

### 5.6 冻结范围与状态

| 范围 | 判定 |
|---|---|
| 三架构统一的核心原则与目标形态（§0 + §1.1 修订后表述） | **可以冻结** |
| D-72 采用候选 A 的设计裁决（§2 + §5.4） | **可以冻结** |
| 本文件作为"无内部矛盾、可直接指导全部实施的完整规格" | **本版四项修订后达到**（§5.5）；残余开放项只剩 §5.3 的入口契约事实取证（实施前置，不是设计问题） |

**冻结后不再重新讨论的方向**：三架构目标主线、统一启动契约与语义流程、直核为辅助启动路径、D-72 资源归属。实现期间出现的新事实可以触发**针对性**裁决（局部重开），但不得无依据地退回"各架构分别维护独立的高层启动流程"。
