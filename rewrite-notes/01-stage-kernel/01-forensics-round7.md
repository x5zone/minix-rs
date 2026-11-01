# 01 · 引导链 v6 最后一轮设计裁决取证（round-7：arena 与 KernelInfo 闭环 / 信封唯一性 / EBS 映射依赖）

> **生成日期**：2026-10-28。
> **用途**：六轮外部评审提出的三个实质问题（单一 arena 与 bootstrap 侧 `KernelInfo` 的闭环、arena 是否包含最终信封、是否值得避免 EBS 后读取 `MemoryMapOwned`）的定向取证。只调查与比较，不改代码。事实标 **已证实 / 反证 / 尚无证据**，裁决建议独立标注。核心结论先行：**shim 侧跨镜像路径对内存图「内容」的消费经取证为零（bump 注册在 validate 之前完成、fallback 分支被短路），但 shim 侧 `KernelInfo` 仍需类型正确的切片字段——采评审方案 A（arena 含语义工作分区，双写，4 页）；arena 兼任信封宿主、移除独立 bump blob 页；方案乙（EBS 配对循环内分类）在锁定 uefi 0.33.0 上无公开 API 可达，且其消除的只是边际新增面、「EBS 后固件页表有效」前提两案共有——维持方案甲，升乙登记为条件触发项。**

---

## A. 单一 arena 与 bootstrap 侧 `KernelInfo` 的闭环（已证实消费面；裁决采方案 A）

### A.1 两条准备函数的真实消费面（逐字段，已证实）

`os/boot-shim/src/main.rs:fn main` 的执行顺序是裁决的地基（节选）：

```rust
let result = UefiBootShim::prepare_boot(1024);          // L61：EBS 发生在内
// --- Below this point, UEFI boot services are gone ---
boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);  // L66：注册 bump 区
minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);         // L67
#[cfg(target_arch = "aarch64")]
minix_kernel::bootstrap_to_kernel_image(&result.kernel_info, result.root_page);  // L86
#[cfg(not(target_arch = "aarch64"))]
minix_kernel::arch_boot(&result.kernel_info, result.root_page);    // L88
```

`os/kernel/src/lib.rs:fn boot_validate_and_prepare` 对 `KernelInfo` 的逐字段消费：

- 标量断言：`kern_virt_base`/`kern_phys_base`/`kern_size`（对齐与 huge 页选择）、`kern_stack_top`（16 字节对齐）——全部**标量**，与内存图无关；
- 切片长度：`KernelInfo::validate`（`os/libs/minix-boot/src/kernel_info.rs:fn validate`）只读 `memmap.is_empty()`/`param_buf.len()` 等**胖指针长度元数据**，不解引用数据指针；
- **唯一的内存图内容消费点** = L499-515 的 fallback bump 注册（`kernel_info.memmap().iter().find_map(...)` 遍历 conventional 找 DM-admissible 区）——但该分支仅在 `boot_alloc::boot_alloc_region().is_none()` 时执行，而 **main.rs L66 已在跳转之前显式注册**（跨镜像与 x86 内联两条产线同序）→ 产线上 fallback 被短路。fallback 的真实消费者是宿主测试内核与未显式注册的引导形态（`os/kernel/src/dm_coverage.rs` 测试族多处走此路径）。

`os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable` 的消费面：Step 1 恒等映射按**常量** `IDENTITY_MAP_END` 全量铺（注释自陈「必须映射全部低地址空间，不只 CONVENTIONAL——UEFI loader 代码可能住 LOADER_DATA」），Step 2 内核高半只读 `kern_virt_base`/`kern_phys_base`/`kern_size` 标量，Step 2b MMIO 用架构常量——**零内存图内容消费**。

**结论（已证实）**：跨镜像路径 shim 侧对 `memmap`/`reserved_regions` **内容**的消费为零；`KernelInfo` 被消费的是标量字段与切片长度。x86 内联路径不经 wire（进程内直传），其 `establish_boot_dm` 消费的是 `store_kernel_info` 之后的落地副本。镜像侧（aarch64）`arch_boot_resume_high_half` 中 `resume_boot_pt_alloc` 先于 `boot_validate_and_prepare` 注册 bump（`os/kernel-image/src/main.rs:fn rust_image_main`）→ fallback 同样短路，`store_kernel_info` 从 arena 受控复制。

### A.2 那 shim 侧为什么仍需要语义数组（方案 A 的精确理由）

评审的担心「最终图只在 `WireMemoryRegion` 数组里，现有准备函数要 `&KernelInfo`」比取证结果**重**——准备函数不要内存图内容。但三个真实需求仍要求 arena 内保留语义工作分区（`[MemoryRegion]` 数组）：

1. **类型正确性**：shim 侧 `KernelInfo.memmap: &'static [MemoryRegion]` 是被 `validate`（读 len，要求非空）与 `bootstrap_to_kernel_image` 按值浅拷的类型成员——把 wire 数组强转成切片被明令禁止（布局门禁管的是「类型演进成本」，不是「允许 reinterpret」）；
2. **fallback 路径的真实消费者**：测试内核与未注册形态走 fallback 读**内容**——语义数组是非 wire 的供给面；
3. **写 handoff 后不得再读失效源**：语义分区让 shim 侧一切潜在内容消费（含将来新增的断言）都有稳定供给，不碰 shim 的 UEFI 池堆临时数组（快照后分配、A-β 类）。

### A.3 裁决（标注：裁决建议）：方案 A——一个物理 arena、双份逻辑表示

- 分类循环（方案甲，基于 EBS 终图）**同一循环双写**：`MemoryRegion` 进语义分区（shim 侧供给）、`WireMemoryRegion` 进 wire 分区（镜像侧供给）——纯计算、无第二笔分配；语义分区与 wire 分区**分开存储、分开写入、无共享布局**（拒绝方案 C 的布局耦合；方案 B「改准备消费者吃迭代器」被否——fallback 消费者含测试基建，接口改动面大而收益为零，因产线上根本不读内容）。
- 语义分区的生命周期 = shim 侧至跳转；wire 分区 = 至镜像侧深拷；镜像侧忽略语义分区（受控复制只读 wire 分区——协议上语义分区不在 `WireSlice` 登记范围）。
- **精确容量（编译期常量，非估算）**：信封 `BootHandoffV1`（定长，协议冻结时钉死，按字段预算 ≤ 256 B）+ wire 分区 128×16 + 256×16 + 12×32 + 4×16 + 1024 = 7,616 B + 语义分区 128×16 + 256×16 = 6,144 B ≈ **14 KiB → 4 页（16 KiB）整分配**；分区容量与偏移全部 `const` 算术 + `size_of` 断言，任一分区超限显式失败。物理分配次数 = 1；逻辑表示 = 两套（评审要求的显式区分，照录）。

---

## B. arena 是否包含最终交接信封（裁决：arena 即信封宿主，移除 bump blob 页）

### B.1 现状证据（已证实）

现状独立 blob 页的来历（`os/kernel/src/lib.rs:fn bootstrap_to_kernel_image`，切根后）：

```rust
let (blob_phys, _) = boot_alloc::boot_pt_alloc()   // ← bump 区另分配一页作 handoff blob
    .expect("…");
let (bump_base, bump_end) = boot_alloc::boot_alloc_region();
let bump_next = boot_alloc::boot_alloc_next();      // 读最终游标（此刻才确定）
let handoff = minix_boot::BootHandoff { kernel_info: *kernel_info, root_page: root_page.0,
                                        bump_base, bump_next, bump_end };
unsafe { core::ptr::write(blob_phys.0 as *mut minix_boot::BootHandoff, handoff) };
unsafe { core::arch::asm!("isb", "br {entry}", in("x0") blob_phys.0, …) };
```

接收入口形状：`os/kernel-image/src/main.rs:fn rust_image_main(handoff: *const BootHandoff)`——x0 指针参数，**对「指针指向 arena 基址（信封在头部）」完全同形**。建表的中间页表页另经 `pt_alloc`（= `boot_pt_alloc`）从 bump 区出——blob 页只是 bump 的消费者之一，移除它不触碰 bump 机制。

### B.2 裁决（标注：裁决建议）：唯一对象模型——arena 起始处即最终信封

1. **EBS 前**：arena（4 页）一次 `MaxAddress` 分配；布局自基址起 = `BootHandoffV1` 信封 → wire 分区 → 语义分区（偏移编译期常量）。
2. **EBS 后、切根前**：分类双写两分区 + 序列化其余载荷 + 填信封静态字段（kernel 标量、各 `WireSlice` 的 arena 内地址、协议头）。
3. **切根后**：在信封内补运行期字段（`root_page`、`bump_base`/`bump_next`/`bump_end`——游标此刻才确定，与现状读游标的时序逐字同位）；**删除 `boot_pt_alloc()` 的 blob 页分配**；跳转 `x0 = arena 基址`。
4. **接收端**：从 arena 基址验证协议头 → 受控复制五步（只读信封 + wire 分区）→ 恢复游标。

**验收改写**：v5/v6 的「`bump_next` 必须越过 blob 页本身，且 blob 分配不会被恢复时重复分配」废止，改为两条：**交接载荷（arena）不占 bump 区**（arena 经 `MaxAddress` 独立分配，构造保证）；**信封记录的 `bump_next` = 跳转前游标真值**（发送端在切根后、跳转前读取——与现状时序同位，恢复端无重复分配面）。arena 信封与载荷同一分配，「单一 handoff arena」名实一致。

---

## C. 是否避免在 EBS 成功后读取 `MemoryMapOwned`（已证实 API 面；裁决维持方案甲、升乙条件触发）

### C.1 锁定版本的公开 API 面（已证实，决定性）

方案乙（「GetMemoryMap（预分配缓冲）→ 分类写 arena → EBS(map_key)，失败重试」）需要的两个入口在 uefi 0.33.0 的**公开面**上不存在：

```console
$ grep -n "^pub fn\|^pub unsafe fn" uefi-0.33.0/src/boot.rs   # 顶层公开面（节选）
131:pub fn allocate_pages(…)
172:pub fn allocate_pool(…)
258:pub fn memory_map(mt: MemoryType) -> Result<MemoryMapOwned>      # ← 内部分配缓冲
1223:pub unsafe fn exit_boot_services(memory_type: MemoryType) -> MemoryMapOwned  # ← 高层封装
$ grep -n "^pub fn\|^pub unsafe fn" uefi-0.33.0/src/system.rs
18/31/42/52/76/102/128: firmware_vendor / firmware_revision / uefi_revision /
                        with_config_table / with_stdin / with_stdout / with_stderr   # ← 无 boot_services 访问器
$ grep -n "fn get_memory_map_and_exit_boot_services" uefi-0.33.0/src/boot.rs
1158:unsafe fn get_memory_map_and_exit_boot_services(…)   # ← 私有（pub(crate) 同级的内部 fn）
$ grep -n "boot_services_raw_panicking" uefi-0.33.0/src/boot.rs
90:pub(crate) fn boot_services_raw_panicking(…)   # ← raw 表获取器不公开
```

raw 表本体（`uefi-raw 0.9.0` 的 `BootServices.get_memory_map`/`.exit_boot_services`，`unsafe extern "efiapi" fn` 字段）存在，但 uefi 0.33.0 既不 re-export `uefi_raw`，也不公开任何 `BootServices` 表获取器。**方案乙在锁定版本上无公开 API 路径**；最小自有实现 = 绕开 `uefi::entry` 宏自写 UEFI 入口（原生入口本有 image_handle/system_table 两参）自存表指针 + 与 crate 内部静态（`SYSTEM_TABLE`、allocator）双轨并存——百行级 unsafe 与双真值源，正是 round-4 估计「绕 crate 下沉 raw 面」的实际下限再加码。

### C.2 方案乙的技术前提逐问（已证实 / 推导分列）

- **分类是否破坏 MapKey？**（C.2）不破坏——分类是纯计算写预分配 arena，MapKey 失效源 = 任何改变内存图的 Boot Services 调用；循环内若严格零 BS 调用、零分配，则 GetMemoryMap 与 EBS 之间图不变（**推导成立**，前提是循环体可审计）。
- **重试合规性？**（C.3）合规——「EBS 失败（MapKey 过期）→ 重取 GetMemoryMap → 再试」正是 UEFI 规范钦定的 loader 使用模式（EDK2/Linux EFI stub 同款）；且 MapKey 匹配语义 ⟹ 成功那次 EBS 时点的图与分类所用图一致——乙的分类图与甲的终图**等价**，乙无快照过期问题（**已证实**：MapKey 机制语义）。
- **乙是否实质消除固件页表依赖？**（C.4）**部分**——乙消除「EBS 后读取 helper 新分配的 map 缓冲」这一**新增**读取面；但「EBS 后、自建根启用前，固件页表保持有效」两案**共有且不可避免**：四步时序的步骤 3-4（切根的建表写 VA==PA、切根后补信封、绝对跳转）本身就是代码执行，全都站在这条前提上（这是任何 UEFI loader 的行业基线前提，OVMF/EDK2/Linux 同站）。乙把边际新增面归零，但规范沉默区的**类别**无法消除——同一未证明假设的主体被保留，只是暴露面变小（**裁决性判断**）。

### C.3 裁决（标注：裁决建议）：维持方案甲；「升乙」登记为条件触发项

- 正确性：甲的暴露窗口 = EBS 返回 → 分类完成（无 BS 调用、无中断源、毫秒级），且有真机实证（两条 UEFI 腿现状即在此窗口执行建表前的一切代码）；乙在此窗口之外还要求自建 raw 双轨。
- 实现复杂度：甲 = 捕获返回值（改动一处）；乙 = 自写入口 + 表指针管理 + 外部缓冲 GetMemoryMap + 重试循环 + 与 crate 静态双轨（见 C.1）。
- 重试可审计性：甲的重试在 crate helper 内（上游维护）；乙的重试自维护，循环体「零分配零 BS 调用」的审计义务落本项目。
- 维护成本：甲随上游升级免费；乙钉死 raw 面版本。
- **条件触发项成文**：若 (a) 上游 uefi crate 公开回调式 exit 或 raw 表获取器，或 (b) 项目引入第二固件实现且其 EBS 后页表行为未验证，或 (c) 真机出现该窗口的页表失效病例——届时按 C.2 已验证的等价性把分类循环迁入配对重试（分类代码零改动，只换调用骨架）。

---

## 汇总：对 v6 裁决的修改与维持

| 项 | 处置 |
|---|---|
| §1.5（七）arena 布局 | **修改**：加语义工作分区（双写）、容量改精确 4 页（编译期常量）、分区偏移断言；信封宿主 = arena 起始处 |
| 独立 bump blob 页 | **移除**（B 裁决）：跳转 `x0 = arena 基址`；「游标越过 blob 页」验收废止，改「arena 不占 bump 区 + 信封游标 = 跳转前真值」 |
| §1.5（六）方案甲 | **维持**；附 C 批评估结论（乙无公开 API、前提两案共有、升乙条件触发三项） |
| `KernelInfoView`/`BootReadPhase` 退役、x86 栈/`rbx`/cli/cld、`load_elf_into` 前提归边界、red-zone、T3/T4 | **维持**（本轮不再动） |
| 冻结阻塞项 | 无新增；T1 剩余 = arena 分区常量与信封字段的实施钉死（随实施首批完成） |
