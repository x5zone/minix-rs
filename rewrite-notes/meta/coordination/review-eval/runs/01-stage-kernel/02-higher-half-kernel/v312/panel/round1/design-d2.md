DOC 02-higher-half-kernel.md

VARIANCE: D-024 D1 停跑，D2 采用文档主张↔源码事实回退

## C-CONTRACT（独立重建，先于对照）

C 侧与本篇射程有关的合同只有一条主线：**一个低半可寻址的启动信息对象，从低半建到高半消费，地址在切换前后不变**。

- 对象：`kinfo_t kinfo` 是 `pre_init.c` 里的全局（`minix3/minix/kernel/arch/i386/pre_init.c:27`），不是发送者/接收者两种形态。`pre_init()` 在物理视图里填充它，返回 `&kinfo`；`head.S` 把 `%eax` 压栈后 `call kmain`（`head.S:63-90`）；`kmain` 再 `memcpy(&kinfo, local_cbi, sizeof(kinfo))`（`main.c:115-129`，实为同一个全局的自拷贝，语义是"确认全局副本可用"）。
- 地址：`kinfo` 属于 `pre_init.o`，被 `Makefile.inc` 的 unpaged 名单收进 `.unpaged_bss`；`kernel.lds:15-18` 把 `.unpaged_*` 放在 `_kern_phys_base` 起的低地址（VMA=LMA），高半段才加 `_kern_offset`。所以同一个数字地址在开分页前后都有效——低半靠 VMA=LMA，高半靠恒等映射与既有条目。C 不需要在跳转时翻译这个指针。
- 布局量：`_kern_phys_base`、`_kern_vir_base`、`_kern_size` 是链接符号（`kernel.lds:3-28`），栈顶 `k_initial_stktop` 也是链接符号（`mpx.S:631-634`，`k_initial_stack` 4 KiB，`K_STACK_SIZE = I386_PAGE_SIZE`）。全部由链接脚本产生，内核没有派生出第二份。
- 映射：`pg_identity` 全铺 1024 个 4 MiB 大页=4 GiB；`pg_mapkernel` 用 4 MiB 大页覆盖 `[kern_vir_start, +kern_kernlen)`，两个基址有 4 MiB 对齐断言，跨距没有对齐断言，最后一块向上取整（`pg_utils.c:162-202`）；`vm_enable_paging` 置 PGE/PSE 的顺序与 CR0.PG 相关（`pg_utils.c:204-245`）。
- 错误：C 在启动期一律 `assert`/`panic`；没有"外部输入解析"这一层，所以没有 `Result` 错误集。
- 并发：`pre_init`→`kmain` 单线程；BKL 在 `kmain` 之后才建立。启动信息对象的写一次读多性质由执行次序保证，无类型标记。
- 抽象边界：`pg_utils.c` 直接写 `u32_t pagedir[1024]` 与页目录项位；没有 trait，也没有"页尺寸能力"查询，4 MiB 大页是编译期事实。
- 可测试性：没有任何宿主测试；唯一证据是能启动。ELF 解析在 multiboot 装载器里，内核镜像自己不解析。

Rust 侧的设计合同（Ch3/Ch4 的主张）是：把上述对象拆成发送侧借用视图 `BootReport<'_>` 与接收侧 `KernelInfo<'static>`，接收侧落 `.bss`；栈顶改为派生访问器；布局量由 ELF 现算或常量副本；错误分为"外部字节 `Result`"与"可信链 `assert!`"；硬件差异收进 trait。下面逐维对照。

## DIFF-TABLE

| 维度 | 文档 Ch3/Ch4 主张 | 源码事实 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 | §3.3 栈顶是派生值 `KernelInfo::kern_stack_top()`，wire 不再带 `kern_stack_top`/`user_sp`/`syscall_entry`；§3.7 跳转只收 `kinfo`；§4.5 发送侧构造 `BootReport<'_>`，接收侧落 `.bss` 后构造唯一 `KernelInfo` | `os/libs/minix-boot/src/boot_storage.rs:262-293` 的 `report()` 产出借用视图；`os/kernel/src/lib.rs:1578-1715` 的 `store_boot_report`/`land_and_store` 落 `.bss` 并构造 `KernelInfo`；`os/libs/minix-boot/src/kernel_info.rs:253-266` 的访问器；`handoff_v1.rs:300-302` 的 wire 只有三个布局标量 | 主体一致。§4.5 两处路径图（doc:1646、1655）仍写"组 KernelInfo"；§3.1/§3.3 三处仍写载体合成 `KernelInfo`；§3.3 的"公式只留访问器一份"不成立（见 DESIGN-06） | 设计接受；文本必须同步（DESIGN-02/05/06） |
| 身份 | §3.2 `ValidatedKernelImage` 私有构造是"两道检查已跑过"的证据；§3.1 `KernelLoadResult.entry_point` 是入口的单一来源；§3.7 `-> !` 让返回不可表示 | `os/boot-shim/src/loader.rs:178-240`：`KernelImage` 字段私有，`validate()` 是 `ValidatedKernelImage` 唯一构造，`Deref` 只暴露布局；`store_boot_report` 的一次性槽与 `kmain` 的 `ptr::eq` 断言（`lib.rs:1830-1847`） | 身份机制一致。载体身份有两种：`test-higher-half` 用 `BootReport`（`os/qemu-tests/.../test-higher-half/src/main.rs:53`），`test-rt-birth` 用 `KernelInfo` 后直接调 `arch_boot_impl`（`:181-184`）。文档把两种写成一种 | 接受；载体措辞需覆盖两种（DESIGN-05） |
| 所有权 | §3.2 `KernelImage<'a>` 把镜像字节与段表绑在一起；§4.5 两条路径都在跳转前把 `KernelInfo` 落进 `.bss`，理由是发送侧载荷在高半切换后不可消费 | `KernelImage` 持 `elf_data` 与 `segments`（`loader.rs:178-186`）；`.bss` landing pad 在 `globals.rs:279-385`；UEFI 腿的源切片来自固件池（`uefi_helpers.rs:1086-1134`）；直核腿的 `BootStorage` 是 `run_self_boot` 的栈局部（`bootface_common.rs:352-355`） | 机制一致。§4.5 把失效原因统一写成"shim 堆高半后未映射/读零"；直核腿的栈帧在镜像 `.bss` 内，Step 1/2 都映射，失效原因是换栈后栈区被 `kmain` 复用覆盖，不是页错误 | 机制接受；理由需按腿分述（DESIGN-04） |
| 错误 | §3.2 `ElfError` 12 个变体分四组；§3.5 `KernelMapError` 的阶梯与消息链 `arch_boot: page-size policy: kernel mapping policy: ...`；可信链用 `assert!` | `os/libs/minix-elf/src/lib.rs` 的 `ElfError` 恰 12 个变体；`boot_policy.rs:172-210` 的 `BootPagePolicyError` 三层 Display；`lib.rs:872-960` 的 `assert!`/`panic!` | 一致。`KernelMapError` 文档只列失败分支（`FallbackUnsupported`/`NoKernelPageSize`），未声称穷举；实际还有 `InvalidSupport`/`InvalidKernelLayout`，不影响结论 | 接受 |
| 并发 | §3.7 跳转只从引导 CPU 调用一次；§4.5 跳转前单线程 | `KERNEL_INFO` 是 `SyncUnsafeCell<Option<...>>`（`globals.rs:279`），`store_kernel_info` 用一次性断言（`lib.rs:1590-1600`）；`BklProtected` 为写一次读多的静态提供 `unsafe impl`（`globals.rs:105-189`）；BKL 在 `kmain` 之后才建立 | 一致；文档未提 `BklProtected`，但 §3/§4 不需要它 | 接受 |
| 抽象边界 | §3.4 `HugePages: Paging` 超集、页标识位留实现、`Paging` 24 个成员；§3.6 Step 1/2/2b/2c；§3.7 `HigherHalf` 契约；§3.8 两条接收路径 | `os/arch/src/arch/paging_ext.rs:75` 与三份 `map_huge` 实现（x86 的 `huge_flag` 局部、1 GiB 分支、2 MiB 叶子守卫）；`lib.rs:1324-1510` 的 Step 1/2/2b/2c；`higher_half.rs` 的 trait 与三份实现 | 设计一致。两处文本有问题：§3.7 的 Safety ② 与 §4.5 的 `ptr::eq` 主张互斥（DESIGN-01）；Step 1 源码注释与实现自相矛盾（文档 1459 已指出，属源码侧） | 接受；DESIGN-01 需裁决 |
| 可测试性 | §3.2 解析器被五个 crate 依赖，宿主测试与三个消费者共用；§4.2 物理摆放只有真机，宿主跑 buffer 孪生 | `grep -rl 'minix-elf =' os/*/Cargo.toml os/*/*/Cargo.toml` 输出五行；`minix-elf` 27 个测试、`loader.rs` 32 个测试（实跑 `grep -c`）；`load_segments_into_buffer` 与物理版共享 `segment_spans` | 一致；载体在"合成 `KernelInfo`"上的措辞漂移同上 | 接受；措辞见 DESIGN-05 |

## SUBTRACTION-TABLE（V3.6，C-32）

| 加法提案 | 减法替代（闭包 / fn 参数 / 现有类型 / 不做） | 减法拦不住的理由 | 保障强度对比 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| 给 `KernelInfo` 加私有字段 + `ValidatedKernelInfo` 令牌，禁止未校验发布 | 保留现有 `assert!` 门（`validate_before_tables`/`validate_requiring_sources`）与一次性槽 | 无。启动路径是可信链，`assert!` 是规定的 fail-fast 形态；类型化只会把 panic 变长 | 等同（两者都在消费点拒绝坏输入；类型化不增加检查） | 2（令牌 trait + 构造器） | 否 |
| 给栈顶加 `AlignedStackTop` newtype 或 16 字节对齐令牌 | 现有 `boot_validate_and_prepare` 的 `%16` 断言 + 三份实现的本地断言 + `kern_stack_top()` 访问器 | 无。对齐是数值谓词，newtype 仍要靠构造期断言 | 等同 | 1 | 否 |
| 把 `BootReport` 拆成 `CompleteReport`/`WireBackedReport` 两个类型，表达"source_records 是否可以借道 wire" | 保留一个 `BootReport` + `source_records.is_empty()` + `BeforeTables`/`Strict` 两档门 | 无。UEFI 腿的空列表是已文档化的合法形态，严格门在接收侧仍会拒绝空记录 | 等同 | 2（两类型 + 转换） | 否 |
| 给 `BootStorage` 加 `finish()` 状态门，要求五个 family 都已提交 | 保留五个一次性 `fill_*` 提交 + 入口校验（`memmap`/`boot_modules` 非空、strict 要求 records） | 无。UEFI 腿合法地不提交 `sources`（classifier 直写 wire），`finish()` 无法区分"合法空"与"忘记填" | 更弱（会把合法腿挡掉） | 1 | 否 |
| 把 `arch_boot_finish` 与 `resume_additive` 合并成一个带策略枚举的尾函数 | 保留两个泛型函数 + 共享的 `boot_validate_and_prepare`/`store_*`/`establish_boot_dm` | 无。两条路径的差别正是 J3/J4 的分界（建根 vs 加性），合并会把"不得重建根"这条约束藏进运行时分支 | 更弱（编译期分支消失） | 1 枚举 + 1 分支 | 否 |
| 给 `kinfo` 指针加 `PhysPtr`/`VirtPtr` 两个 newtype，用类型阻止直核腿传错视图 | 在现有跳转分支里做一次指针归一化（或修正文档描述） | 无——**这条减法确实拦不住**：同一个指针的"视图"由运行期 PC 决定，不是由类型决定；`PhysPtr` 与 `VirtPtr` 都能装同一个数值，编译器无法知道当前 PC 在哪一侧 | 等同（类型不能表达运行期视图） | 2 | 否 |

本轮无新增抽象提案被采纳。DESIGN-01 的最小修复是"归一化一个指针"或"改一句描述"，不是新增类型；因此 SUBTRACTION-TABLE 全为否，且没有任何加法 FINDING。

## TRANSLATION-RESIDUE（任务 2）

按六类逐项核对（只看 Ch3/Ch4 射程内的启动结构）：

1. **单实现依赖注入 trait**：0。`HigherHalf` 4 个实现（三架构 + 测试/占位）、`HugePages` 5 个、`BootArch` 4 个、`BootDmInstaller` 4 个、`DirectMapArch` 4 个、`FileLoader` 3 个、`Paging` 5 个（trait-audit 读数；`dyn` 计数只有 `FileLoader` 为 1，不构成对象图的运行时多态层）。没有"只有一个实现却为注入而存在"的 trait。
2. **无状态 Manager**：`KernelMapPolicy`/`BootPagePolicy`/`DmPagePolicy` 不是 Manager，是携带"已校验布局 + 已选粒度"的值对象；它们防止执行路径重新拼一份 layout（`boot_policy.rs:68-97`、`lib.rs:1000-1010` 的注释与用法）。保留。
3. **布尔或字符串状态机**：对外状态已用 `ValidationLevel` 枚举（`kernel_info.rs:155-180`，注释明确说它替换了两 bool 的四组合）；`BootStorage` 的五个 `*_committed` 是内部一次性守卫，不是对外状态机，没有把非法组合暴露给消费者。保留。
4. **哨兵返回值**：`arena_pa == 0` 是交接 ABI 的"没有 arena"约定（`main.rs:364`、`:400`、`:446`），不是 Rust 自造的 Option 替代；`elf_file_end` 返回 `Option<usize>`，`None` 表示"不是可解析 ELF/没有 PT_LOAD"，调用方回退到整块拷贝（`loader.rs:307-320`、`:625`）。保留。
5. **clone 到能编译**：没有发现。`KernelInfo`、`BootReport`、`MapCounts`、`KernelLoadResult` 的 `Copy`/`Clone` 都是 POD 值语义；没有为了借检查而复制大块载荷的写法。
6. **对象图共享**：`KernelInfo` 的 `&'static` 切片指向 `.bss` landing pad，是 C"一个全局对象"的现代替身；落地后所有消费者共享同一份只读数据，符合单线程启动 + 之后只读的合同。保留。

只因为 C 这样写而存在、但今天仍有读者的结构：`kern_phys_base + kern_size` 这对布局标量（Step 2 映射与 `kern_stack_top` 都要）、`param_buf` 的 1024 字节上界（`GET_MONPARAMS` 的 C 契约）、`MemoryRegion {base, len}` 的 wire 形状（D-110 已裁决不统一为 `PhysRange`）、`boot_modules` 的位置身份（对应 C `image[]` 顺序）。这些不是"翻译残留"，是外部行为。

## DEAD-ABSTRACTION（任务 3）

trait-audit 在 related-code 清单内只给出 5 条：`HigherHalf`（4 impl / 31 ref）、`HugePages`（5 / 31）、`BootArch`（4 / 58）、`BootDmInstaller`（4 / 12）、`DirectMapArch`（4 / 93）；全部 MULTI-IMPL。把范围放宽到 `os/arch/src`、`os/kernel/src`、`os/boot-shim`、`os/libs/minix-boot` 后，`Paging`（5 / 242）、`CpuContextArch`（3 / 61）、`FileLoader`（3 / 16）、`StacktraceArch`（3 / 16）也都是多实现。**本轮没有可删的 trait。**

必须自己核实的两个审计读数：

- `BklProtected` 被审计记成 `NO-IMPL`（`os/kernel/src/globals.rs:105`）。这是**假阳性**：实现由 `bkl_protected_impls!` 宏批量生成，显式类型 19 个（`KernelInfo`、`KerninfoPage`、`HandoffMemRegion`、`MemoryRegion`、`BootModule`、`PlatformDescSource`、`SourceRecord`、`ProcessTable`、`PrivTable`、`IrqManager`、`SmpState`、`CpuInfoTable`、`IpcFilterPool`、`KRandomness`、`ProcNr`、`ClockState`、`SprofInfo`、`KmessRing`、`u8`），x86 另两个，共 21 个，另有 `Option<T>` 与数组的泛型 impl。它不是死抽象，而是 `SyncUnsafeCell` 的封闭安全门。
- `Sealed` 被记成 `SINGLE-IMPL`，同样是宏生成，实际每个 `BklProtected` 类型都有一个空 impl。结论同上。

`KernelInfoView`（`kernel_info.rs:188-222`）不在审计表里，手工计数是 2 个 impl（`KernelInfo`、`BootReport<'_>`），被 `validate_view`、`check_boot_postconditions_view`、`land_and_store` 三个泛型体消费。它承担"发送/接收两形态共用一份校验体"的语义职责，不是单实现注入，保留。

## ILLEGAL-STATE（任务 4）

1. `KernelInfo` 全字段 `pub` + `Copy` + `pub fn store_kernel_info`，可以构造一个未校验的实例并发布到一次性槽。**类型上可表示、语义上不应发生**。判据：这是可信链，`assert!` 是规定的 fail-fast 形态；类型化会新增令牌概念而拦截力不增。接受，不提案。
2. `BootReport` 的 `source_records` 在 UEFI 腿为空、在直核腿非空，同一个类型承载两种强度的视图。**可表示、语义上合法**（空列表是已文档化的 wire 借道形态）；严格门在接收侧拒绝空记录。接受。
3. `kern_size` 只被检查 `> 0` 与 16 字节对齐，一个"对齐但偏小"的值会把 `kern_stack_top` 放进内核全局数据区。**可表示、接收侧不可判**：真实跨距只有发送侧知道；文档 §3.3 已写明两个前提。接受（信任发送侧）。
4. `BootStorage` 的部分提交（例如只提交 modules，不提交 map）可表示，`report()` 会产出空 family。**可表示**；入口的 `memmap`/`boot_modules` 非空与 strict records 检查会拒绝生产腿，UEFI 的 sources 空是合法形态。接受。
5. x86 `jump_to_kmain` 的两个断言：`stack_top & 0xF == 0` 与 `(stack_top - 24) & 0xF == 8`。第二个在第一个成立时恒真（`-24 mod 16 = 8`），**不可能独立失败**。它不是非法状态，是一条死断言（DESIGN-08）。
6. 直核腿的 `kinfo` 指针：物理别名与高半别名都映射同一个对象，数值不同。`KernelInfo` 的身份（`ptr::eq`）无法区分"同一对象的两个别名"。这是 DESIGN-01 的根。

## MINIMIZATION（任务 5，先减法）

按"能删一层就删一层"的顺序：

- **M1（采纳，P2）**：`boot_validate_and_prepare` 改为收 `&KernelInfo`（或 `KernelInfoView`），栈顶对齐断言调用现成访问器，删掉 `lib.rs:895-899` 的本地 `kv.checked_add(ks)` 复算。删掉三个位置形参、一处公式副本；不新增概念。
- **M2（采纳，P3）**：v1 接收的 `KernelInfo` 字面量（`lib.rs:733-760`）与 `land_and_store` 里的字面量（`lib.rs:1697-1710`）二选一：把解码后的 `.bss` 切片包成 `BootReport<'_>`，调 `store_boot_report(report, authority)`。`land_slice` 的指针相等守卫会让这次落地退化为零拷贝；v1 现有的 `validate_requiring_sources` 保留在 report 上。删掉约 25 行构造与一处必须同步维护的字段清单。
- **M3（采纳，P3）**：删 x86 的 `(stack_top - 24) & 0xF == 8` 断言（或改成注释）。它不能独立失败；真正的 asm 序列检查在载体的 `sp_aligned` 与 §3.7 的推演里。
- **M4（拒绝）**：合并 `arch_boot_finish` 与 `resume_additive`。两者的区别是 J3/J4 分界，合并会把"v1 不得重建根"从结构约束降成运行时判断。代价大于收益。
- **M5（拒绝）**：合并 `store_kernel_info` 与 `store_boot_report`。两个入口分别绑定"authority 来自值"与"authority 来自参数"，合并会把一对可能不一致的东西重新变成两个参数。
- **M6（拒绝）**：删掉 `BootReport`/`KernelInfo` 的二分，回到 C 的单对象。跨镜像腿的所有权与生命周期无法用单对象表达；C 能这么做是因为 unpaged 段保证同一数字地址两边有效，而 §4.6 已明确 Rust 不再有 unpaged 段。
- **M7（拒绝）**：删掉 `ValidationLevel`，回到两个 bool。现有枚举正是为了消除四组合里那个非法组合，回退没有收益。

## FINDING DESIGN-01（P0）

- 分类（D-047）：`[MODERN]`（它恢复的是 `HigherHalf` 已有的"kinfo 在高半地址"契约，不新增责任；若裁决认为"指针视图归一化"属于新责任，则升级为 `[ARCH]`）
- 位置：文档 §3.7 Safety ②（`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1065`）与 §4.5 交接段（`:1691`）；源码 `os/kernel/src/lib.rs:215-247`（`arch_boot_finish`）、`:1578-1715`（`store_boot_report`/`land_and_store`）、`:1830-1847`（生产 `kmain` 的 `ptr::eq`）；三份 `os/kernel/src/arch/{x86_64,aarch64,riscv64}/higher_half.rs` 的 `jump_to_kmain`
- 严重度：P0（P0-design-wrong 与 P0-code-bug 交界）
- 决策：需用户裁决（文档侧先删/改 Safety ②；代码侧登记为 P0-code-bug，解冻后修）
- 问题：文档说直核面"在分页开启前传物理视图引用"，同一份文档又说生产 `kmain` 用 `ptr::eq` 断言形参与 `KERNEL_INFO` 同址。直核腿的形参是物理视图下算出的地址，`kmain` 在高半视图重算同一个 `.bss` 符号，两者数值不同；两条主张不能同时成立。
- 影响：若代码按现状执行，riscv64 自引导与 aarch64 direct-core 在 `kmain` 入口必 panic；若代码实际传的是高半地址，则 §3.7 的括注是错误事实。两种结局都落在生产启动链的第一条语句上。
- 证据或推理：三处源码对同一条前置条件的说法已经互相打架。trait 的 Safety 写"`kinfo` pointer is valid and accessible at high address"（`os/kernel/src/boot/higher_half.rs:38-39`）；x86 实现的 SAFETY 注释却写"direct-core passes the physical-view reference"；生产 `kmain` 用 `ptr::eq` 要求参数等于高半 `.bss` 全局。`arch_boot_finish` 先 `store_boot_report`（返回 `&'static KernelInfo`，指向 `globals::KERNEL_INFO`），再 `arch_boot_impl` 开分页，最后把同一个 `kinfo` 交给 `jump_to_kmain`；三份实现分别用 `in("rdi")`/`in("x0")`/`in("a0")` 原样传递。直核腿的 `arch_boot_finish` 在物理视图执行（§3.1 表第 5 行；入口 asm 置 SP 到 `.bss` 的物理别名），PC 相对的静态取址给出物理地址；`kmain` 在加 `delta` 后的高半执行，PC 相对取址给出高半地址。工件反汇编可复核：`os/target/aarch64-unknown-none/release/kernel` 的 `arch_boot_finish` 在 `0xffff800000001cb8` 处 `adr x0, KERNEL_INFO` 后 `bl jump_to_kmain`，`kmain` 在 `0xffff800000005c1c` 处 `adr x8, KERNEL_INFO; cmp x0, x8; b.ne panic`；`os/target/riscv64gc-unknown-none-elf/release/kernel` 同形（`arch_boot_finish` 的 `auipc a0`、`kmain` 的 `auipc a1` + `bne a0,a1`）。C 侧没有这个问题：`kinfo` 在 `.unpaged_bss`（`kernel.lds:15-18`），VMA=LMA，同一个数字地址在两侧都有效；Rust 删掉 unpaged 段（§4.6）以后，同一个对象有了两个数字别名，跳转必须选一个。
- 最小修复：推荐"以代码为准的归一化"，即恢复 trait Safety 已经写下的"kinfo 在高半地址"：在直核分支把 `kinfo` 指针与 `kmain` 目标一起加 `delta`（riscv64 已有 `auipc/bltu` 条件，只需在 `3:` 分支对 `a0` 做同样的加法——注意 `in("a0")` 要改为 `inout("a0")`，或先把指针搬到临时寄存器再加回；aarch64 需要补一条 PC 与 `kern_virt_base` 的比较，再决定是否对 `x0` 加 `delta`），使 `kmain` 收到的始终是 `.bss` 副本的高半别名。若本轮只能改文档：把 §3.7 ② 的括注改为"两条路径都传 `.bss` 副本的高半别名；直核腿的引用值在物理视图下取得，`kmain` 的同址断言按高半别名归一化"，并把该断言登记为待修 P0-code-bug。不要改成"两条腿都传物理地址再让 kmain 比较物理别名"——那会让 v1 腿的现成高半引用失效。
- 减法替代（V3.6，C-32）：不做 `PhysPtr`/`VirtPtr` newtype（见 SUBTRACTION-TABLE 最后一行：视图由运行期 PC 决定，类型装不下）；最小修复是既有跳转分支里的一次指针加法，或一句话。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（无类型级提案）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（无加法提案；若采用"指针归一化"，读者只需记住"跳转要把指针和目标放进同一视图"这一句，不新增类型）。
- 置信度：中高（源码 + 两份工件反汇编；未在真机跑 direct 产物）。
- 是否需源码核实：是。需要构建 direct 产物并在 QEMU/真机看 `kmain` 是否触发 `kmain: KernelInfo argument and KERNEL_INFO global must be the same .bss copy`。
- 若我错了：若直核腿实际交给 `kmain` 的已是高半别名（例如加载器把入口抬到高半，或跳转实现内部重取了引用），则 §3.7 ② 的括注是唯一错误，删掉"直核面在分页开启前传物理视图引用"即可；§4.5 的 `ptr::eq` 描述无需改动。

## FINDING DESIGN-02（P0）

- 分类（D-047）：`[MODERN]`
- 位置：文档 §4.5 两条路径图（`:1646`、`:1655`）
- 严重度：P0（P0-fact）
- 决策：接受（改文档）
- 问题：路径图仍写发送侧"组 `KernelInfo`（v1 交接载荷）"与"组 `KernelInfo` → `minix_kernel::arch_boot`"；当前五条腿的发送侧统一构造 `BootReport<'_>`，`KernelInfo` 只由接收侧在 `.bss` 落地后构造。
- 影响：读者按路径图去找发送侧的 `KernelInfo` 构造点会找不到；同一章下面（`:1687`）已经正确写成"发送侧只构造 `BootReport<'_>` 借用视图"，形成章内自相矛盾。
- 证据或推理：v1 发送侧 `uefi_helpers.rs:1086-1134` 是 `BootStorage::new` → `fill_*` → `storage.report(...)` → `Serializer::write_envelope_report`/`write_boot_modules`/...；自引导发送侧 `bootface_common.rs:549-556` 是 `storage.report(...)` → `report.validate_requiring_sources(DTB)` → `arch_boot(&report, ...)`。两处都没有 `KernelInfo` 字面量。
- 最小修复：`:1646` 改为"读 12 个模块 → 组 `BootReport<'_>` → 序列化 v1 arena（信封与 memmap/reserved/modules/platform/param/sources 六个分区）"；`:1655` 改为"→ 组 `BootReport<'_>` → `minix_kernel::arch_boot`"。接收侧的 `KernelInfo` 构造保留在 `:1649`/`:1656` 的箭头上。
- 减法替代（V3.6，C-32）：无，因这是删除旧形状的文本，不是新增结构；删掉两处旧名词即可。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用。
- 置信度：高（两处源码路径已读）。
- 是否需源码核实：否（已核实）。
- 若我错了：若文档的"组 KernelInfo"指的是接收侧（例如箭头被误读为跨两行），则只需在路径图里把"发送→接收"的边界画清；但按现有箭头位置，它落在发送侧。

## FINDING DESIGN-03（P0）

- 分类（D-047）：`[MODERN]`
- 位置：文档 §4.1 `KERNEL_ENTRY_ANCHOR` 条目（`:1207`）
- 严重度：P0（P0-fact）
- 决策：接受（改文档）
- 问题：文档把该静态的类型写成 `fn(&KernelInfo, PhysBytes) -> !`；源码是 `fn(&BootReport<'_>, RamAuthority, PhysBytes) -> !`。
- 影响：这是一条"代码引用"；类型错两项（参数类型与参数个数），读者会按旧签名理解内核启动面的 ABI，也会误判 `KERNEL_ENTRY_ANCHOR` 与 `arch_boot` 的绑定关系。
- 证据或推理：`os/kernel-image/src/main.rs:113-115`：
  `static KERNEL_ENTRY_ANCHOR: fn(&BootReport<'_>, RamAuthority, PhysBytes) -> ! = minix_kernel::arch_boot;`
  `os/kernel/src/lib.rs:254-258`、`:275-279`、`:806-810` 三个 `arch_boot` 分支同签名。
- 最小修复：把引文改为源码原文（保留 `fn(&BootReport<'_>, RamAuthority, PhysBytes) -> !`），并在同句说明它绑定的是 `arch_boot` 的"发送侧借用视图 + RAM authority + root page"三参形态。
- 减法替代（V3.6，C-32）：无，因这是替换错误引用，不是新增结构。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用。
- 置信度：高。
- 是否需源码核实：否（已核实）。
- 若我错了：若该条目原本引用的是历史签名并已标注"旧形"，则不是事实错误；但文中没有这种标注。

## FINDING DESIGN-04（P1）

- 分类（D-047）：`[MODERN]`
- 位置：文档 §4.5 副本落地理由（`:1691`）
- 严重度：P1（P0-fact 的轻量形态：机制理由对一条腿不成立）
- 决策：接受（改文档的理由分述）
- 问题：文档把两条路径的落地理由统一写成"`reserved_regions`/`memmap`/`boot_modules` 原本指向 boot-shim 堆上的字节，高半切换后那块内存在活动根里没有映射，再读是页错误；历史真机上还出现过读回全零"。直核腿没有 boot-shim，且它的 `BootStorage` 在 `run_self_boot` 的栈帧（镜像 `.bss` 内），Step 1/2 都映射，读到的是有效字节，只是换栈后会被 `kmain` 的栈复用覆盖。
- 影响：读者在直核腿看到"数据损坏"时，会按"页错误/读零"去查映射，而真因是栈区被复用；这条理由也是 DESIGN-01 的邻居（同一段对"副本"的描述不一致）。
- 证据或推理：`bootface_common.rs:352-355` 的 `let mut storage = minix_boot::BootStorage::new();` 是局部；`main.rs:270`/`:317` 的 `la sp, kernel_boot_stack_top` 把 SP 置到镜像 `.bss` 的 64 KiB 引导栈；`jump_to_kmain` 把 SP 换成 `kern_stack_top`（收口后的镜像尾），`kmain` 的栈向下增长，先覆盖收口填充再覆盖引导栈，`BootStorage` 正在其中。UEFI 腿的源切片才在固件池（`uefi_helpers.rs:1086-1134`），其失效是"切换后不在活动根"或"读回全零"。
- 最小修复：把理由拆成两句：UEFI 腿是"固件池在高半切换后不可依赖（可能未映射或读零）"；直核腿是"发送侧存储位于引导栈，跳转后 SP 换到镜像尾，栈复用会覆盖它"。结论（必须落地）不变。
- 减法替代（V3.6，C-32）：无，因这是修正理由的适用域，不新增结构。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用。
- 置信度：高（存储位置与栈切换次序均已核实）。
- 是否需源码核实：否（已核实）。
- 若我错了：若直核腿的 `BootStorage` 实际被编译器提升为 `.data` 静态（`new()` 非全零时会这样，但当前代码是函数内 `let mut`，不会被提升），则"栈复用"不成立，应改为"静态存储但内容会被后续启动阶段复用"；结论方向仍是"按腿分述"。

## FINDING DESIGN-05（P2）

- 分类（D-047）：`[MODERN]`
- 位置：文档 §3.1（`:691`、`:716`）与 §3.3（`:884`）
- 严重度：P2
- 决策：接受（改文档措辞）
- 问题：三处把测试载体统一写成"以合成的 `KernelInfo` 走进同一段引导代码"/"合成 `KernelInfo` 的路径"。实际载体有两种：`test-higher-half` 合成 `BootReport` 并调 `arch_boot`；`test-rt-birth` 等合成 `KernelInfo` 并直接调 `arch_boot_impl`。
- 影响：读者会以为所有载体都走 `KernelInfo` 构造；`test-higher-half` 是 §5.3 的主要判据载体，这个错名会直接影响对 `kern_stack_top()` 来源的理解。
- 证据或推理：`test-higher-half/src/main.rs:19` 导入 `BootReport, RamAuthority`，`:53-62` 构造 `BootReport`，`:74` 调 `arch_boot(&report, ...)`；`test-rt-birth/src/main.rs:181-184` 先 `store_kernel_info(&kernel_info)` 再 `arch_boot_impl::<...>(&kernel_info, root_page)`。
- 最小修复：把"合成的 `KernelInfo`"改成"合成的启动记录（`BootReport<'_>` 或测试侧 `KernelInfo`，按载体形态）"；§3.1 的"`KernelInfo` 的基址必须由载体自己填对"改成"启动记录的基址必须由载体自己填对"。
- 减法替代（V3.6，C-32）：无，因这是把旧名词换成覆盖两形态的名词，不新增结构。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用。
- 置信度：高（两类载体均已读）。
- 是否需源码核实：否（已核实）。
- 若我错了：若作者只想描述 `test-rt-birth` 一类载体，则可保留 `KernelInfo`，但必须点名载体；当前句是"载体（QEMU 测试内核）"全称。

## FINDING DESIGN-06（P2）

- 分类（D-047）：`[MODERN]`
- 位置：文档 §3.3（`:856`、`:1075` 的"公式只留下访问器这一份"）；源码 `os/kernel/src/lib.rs:895-899`
- 严重度：P2
- 决策：接受（改文档；源码收口在冻结期后做，先记台账）
- 问题：文档说 `kern_stack_top` 的公式只留在访问器一处；源码 `boot_validate_and_prepare` 仍在本地 `VirBytes(kv.checked_add(ks).expect(...))` 复算同一公式，再断言 16 字节对齐。公式在文本上有两份。
- 影响：这不是行为错误，但破坏了"单一来源"的论证；将来改公式（例如加入对齐舍入）时，访问器与启动断言可能不同步，而文档会让读者以为不会。
- 证据或推理：`lib.rs:895-899` 的局部 `kern_stack_top` 只用于 `:920-924` 的 `%16` 断言；`kernel_info.rs:252-266` 的访问器是另一份。`boot_validate_and_prepare` 的三个调用点（`arch_boot_impl`、`resume_additive`）都持有 `&KernelInfo`，完全可以把签名改成 `fn boot_validate_and_prepare<A>(info: &KernelInfo) -> BootPageSupport<A>`，顺带删掉三个位置形参。源码上方 `lib.rs:860-871` 的注释还说"要用 struct literal 让名字和值同行"，但当前签名仍是三个位置参数，注释本身也半旧。
- 最小修复：`boot_validate_and_prepare` 收 `&KernelInfo`（或 `KernelInfoView`），调用 `info.kern_stack_top()` 做断言；文档把"公式只留下访问器这一份"改为"公式只留在访问器；启动期断言直接调用它"。
- 减法替代（V3.6，C-32）：采纳减法本身：删除一处分部公式与三个位置形参，不新增类型。减法拦不住的理由：不适用（这是减法提案）。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（减法让读者少记一处公式）。
- 置信度：高（源码事实）；影响中等。
- 是否需源码核实：否（已核实）。
- 若我错了：若作者把"公式"限定为"消费者取值的公式"而不含启动期校验，则文本可以不改；但源码注释里的"单一来源"论证仍建议收口。

## FINDING DESIGN-07（P3）

- 分类（D-047）：`[MODERN]`
- 位置：`os/kernel/src/lib.rs:733-760`（`arch_boot_resume_v1` 的 `KernelInfo` 字面量）与 `:1697-1710`（`land_and_store` 的字面量）
- 严重度：P3
- 决策：接受（可选减法；代码冻结期后实施）
- 问题：接收侧有两个 `KernelInfo` 构造点。v1 路径先解码到 `.bss` store，再手工构造 `KernelInfo`，随后 `resume_additive` 调 `store_kernel_info`；直核路径由 `store_boot_report` → `land_and_store` 构造一次。
- 影响：字段清单要在两处同步；新增字段时编译器会两边报错（结构体字面量要求全字段），所以不是静默漂移，只是维护面翻倍。落地语义（同址跳过拷贝）在两处各写一遍。
- 证据或推理：v1 的 `KernelInfo` 用 `core::slice::from_raw_parts` 指回刚填的 store；`land_and_store` 的 `land_slice` 在同址时跳过拷贝。把 v1 解码后的切片包成 `BootReport<'_>` 再调 `store_boot_report(report, authority)`，会走到同一构造点，`land_slice` 的指针相等守卫让五个落地都退化为零拷贝；v1 现有的 `kinfo.validate_requiring_sources()` 改为在 report 上调用。
- 最小修复：删掉 `arch_boot_resume_v1` 里的 `KernelInfo` 字面量，构造 `BootReport` 后交给 `store_boot_report`；`resume_additive` 改为接收已存储的 `&'static KernelInfo`，不再二次 `store_kernel_info`。
- 减法替代（V3.6，C-32）：采纳减法：删掉一个构造点与一次无拷贝的 store 调用；不新增类型。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（减法）。
- 置信度：中（可行性已按 `land_slice` 的同址分支推演；需要一次宿主编译验证）。
- 是否需源码核实：是（改后跑 discard/landing 测试）。
- 若我错了：若 v1 的 `KernelInfo` 构造故意不走 `BootReport`（例如为了避免在解码后再次借用 `.bss` store），则保留两处也有理由；当前源码没有写出这种理由。

## FINDING DESIGN-08（P3）

- 分类（D-047）：`[DEL]`
- 位置：`os/kernel/src/arch/x86_64/higher_half.rs:35-47` 的两条 `assert_eq!`
- 严重度：P3
- 决策：接受（可选减法；代码冻结期后实施）
- 问题：`stack_top & 0xF == 0` 与 `(stack_top - 24) & 0xF == 8` 在 `u64` 上等价（`-24 mod 16 = 8`，所以第二个断言在第一个成立时恒真）。它是死断言，不能独立捕获任何故障。
- 影响：读者会以为生产路径有两条独立的 psABI 检查；实际只有"输入对齐"一条，asm 序列本身（`and rsp,-16`/`push 0`/`sub rsp,8`）没有被生产断言覆盖。真正的覆盖在载体 `kmain_verify` 的 `sp_aligned`（`lib.rs:2259-2300` 一带）。
- 证据或推理：数学等价；源码两条断言都作用在 asm 之前的同一个 `stack_top` 上，asm 的效果没有被回读。
- 最小修复：删掉 `(stack_top - 24) & 0xF == 8` 断言，保留 `stack_top & 0xF == 0`，并把 psABI 的算式留在注释里（现有注释已经写了 `entry RSP = stack_top-24 (8 mod 16)`）。如果作者希望保留一条"生产路径的 psABI 见证"，应改为在 `kmain` 入口回读 RSP 并断言（那才是新覆盖，但会增加启动期侵入）。
- 减法替代（V3.6，C-32）：采纳减法：删一条恒真断言，不新增结构；不减保障（它本来不提供保障）。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（减法）。
- 置信度：高（算术等价）。
- 是否需源码核实：否（已核实）。
- 若我错了：若作者的原意是"断言 asm 序列预期效果"，那么它需要回读 RSP 才有意义；当前形态确实不能失败。

## ARCH-PROPOSAL

本轮无 `[ARCH]` 提案。

DESIGN-01 目前按 `[MODERN]` 记账：它修的是同一对象两个数字别名的归一化，不新增字段、检查或策略。若主 agent 或用户裁决认为"给 `HigherHalf` 契约新增'指针视图归一化'责任"属于改变 boot 的机制责任，则应把它改记为 `[ARCH]`，并按三处一致（文档 + 设计 + 代码）走 `[ARCH: boot 交接指针视图]` 流程；本轮不预先按 `[ARCH]` 提案写实施计划。

## VERDICT

值得改的差异：

- **DESIGN-01（P0）** 是本轮最重要的发现。它不是文本洁癖：文档的 Safety ② 与 `ptr::eq` 描述互斥，直核腿的 `kinfo` 指针视图必须被明确归一化。建议以代码为准修文档并登记 P0-code-bug；若要一次修到位，在 `jump_to_kmain` 的 direct 分支把指针与目标一起加 `delta`（aarch64 需补 PC 比较）。这一条需要用户/作者裁决"改文档还是改代码"：代码处于冻结区，文档可改，但文档怎么改取决于代码的真值。
- **DESIGN-02/03（P0-fact）** 是 DESIGN-02(b) 落地后的文本残留：两处路径图仍写发送侧 `KernelInfo`，一处代码引用仍是旧签名。它们与本章自己的正确段落矛盾，必须在 CONVERGED 前修掉。
- **DESIGN-04（P1）** 的落地理由只对 UEFI 腿成立；直核腿是栈复用覆盖，不是页错误。修理由的适用域，不修机制。
- **DESIGN-05/06（P2）** 是载体命名与"单一公式"论证的精度问题，随正文一起修。
- **DESIGN-07/08（P3）** 是可选的减法：合并两个 `KernelInfo` 构造点、删一条恒真断言。它们不阻断，但能让"唯一构造点/唯一断言"的说法变成字面事实。

合理取舍（不建议改）：

- 发送侧 `BootReport<'_>` 与接收侧 `KernelInfo<'static>` 的二分：跨镜像所有权与 `.bss` 落地都需要它；C 的单对象依赖 unpaged 段，Rust 已明确淘汰该段（§4.6）。
- 启动期用 `assert!`/`panic!` 而不是 `Result`：可信链的 fail-fast 形态正确；`Result` 只留给 ELF 字节这类外部输入。
- `arch_boot_finish` 与 `resume_additive` 两个尾函数：J3/J4 的分界是结构，不是重复。
- `store_kernel_info` 与 `store_boot_report` 两个入口：authority 的来源不同，合并会把一对可能不一致的值重新变成两个参数。
- 2 MiB 收口与 1 GiB/2 MiB 阶梯、`HugePages` 超集、`HigherHalf` 的 `-> !`：都与 C 的外部行为一致，且把差异放在了正确的层。

需要用户裁决：

- DESIGN-01 的"文档 vs 代码"归属（以及是否升级为 `[ARCH]`）。其余 7 条均可由作者按 `[MODERN]`/`[DEL]` 代裁。

本轮没有发现需要删除整个 trait、整个层次或合并两个状态的死抽象；`trait-audit` 的 `BklProtected NO-IMPL` 与 `Sealed SINGLE-IMPL` 是宏生成实现的漏计，不能据此动刀。
