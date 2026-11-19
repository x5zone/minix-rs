# P-DESIGN-D2 · 01-boot-shim-bootstrap 设计/实现差异审查（R34 确认轮）

基线核对（开工前执行）：

```
$ wc -l rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
3565 rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
$ sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
d909c236a373636678e66b13c0ad5ebde769ead738f09d81a846803647fcb98c  rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
```

行数与 sha256 前缀 `d909c236a3736366` 与任务给定的基线一致。

审查范围：第 3 章（§3.1–§3.10）与第 4 章（§4.1–§4.9）；实现核对以 `tools/doc-code-map.sh ... --check` 给出的 54 个文件为起点，另外核对了文档设计陈述直接点名的 `os/libs/minix-boot/src/{boot_storage,boot_prepare,bump,kernel_info,handoff_v1,handoff_v1_read,handoff_v1_write,platform}.rs`、`os/boot-shim/src/{lib,loader,ebs_map,ebs_pair,uefi_helpers,opensbi_helpers,uboot_main}.rs`、`os/kernel/src/{lib,boot_alloc,boot_policy,globals,vm_handoff}.rs`、`os/kernel-image/src/{main,bootface,bootface_a64,bootface_common}.rs`、`os/arch/src/{lib,arch/paging_ext,arch/pt_alloc,x86_64/paging}.rs`。

冻结决策对照：D-032/D-033/D-035/D-037/D-040/D-041/D-044 逐条在实现里找到落点；D-034（`records_in_wire`）在文档与实现里都没有该名字，实现用的是“空切片 + 静态约定”，与“咨询标记、非类型级强制”的裁决一致，不作为差异记录。

---

## DIFF-TABLE

| 维度 | 文档陈述 | 现有实现 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 | `KernelInfo` 14 字段；`BootStorage` 是七类载荷的所有者；“两张内存表由 `commit_map_tables` 一次提交”，其余五族“各自只允许提交一次”（§3.4.5）；`ValidationLevel` 三态替代两 bool（§4.4 隐含）；描述符落点五档 `Allocate/ReservedMapped/Side/Excluded/DevicePath`（§4.2.2）；`bootstrap_start/len` 与 `free_upper_idx` 整体删除，因为“布尔事实不占两个字段”（§3.5/§3.9） | `BootStorage` 除 `map_dirty` 外还有 `modules_committed`/`platform_committed`/`param_committed`/`sources_committed`/`startup_committed` 五个 bool（`boot_storage.rs:84-89`）；`set_startup_len` 是 `#[cfg(test)]`（`boot_storage.rs:293`）；`ValidationLevel` 是 `pub(crate)` 三变体枚举（`kernel_info.rs:192`）；`Disposition` 是五变体枚举（`ebs_map.rs:276`）；`MapError::Policy` 变体只由 `From<PolicyError>` 构造，全仓无调用点（`ebs_map.rs:353/369-372`） | 文档把 `set_startup_len` 列为生产五 setter 之一，实际生产只有 `push_startup`；文档 §4.2.2 把 `MapError::Policy` 写成越界上抛的通道之一，实际只有 `RejectedDescriptor`；“已提交长度但槽未写”在类型上可表示 | 需用户裁决（两条 P3 文档面偏差 + 一条 P3 状态压缩机会）。删除字段的方向本身正确，实现已落地；不重开 §3.5/§3.9 |
| 身份 | 模块身份 = 位置 + 规范名（`MODULE_NAMES` 单表，strict 门按位置核名，§4.4）；`KernelInfo.authority: InputObjectId` 由构造点写入、`validate_requiring_sources()` 不再收参数（F-R17-04，§3.4.1）；`authority = StartupObjects` 没有生产者；arena 的身份由 startup 记录承载而不是“是否还活着”（§4.7） | `MODULE_NAMES` 住 `minix-boot`，`loader.rs:71` 只重导出；strict 分支断言派生标量（`kernel_info.rs:453-470`）并按 `MODULE_NAMES[i]` 比对名字（`:474-481`）；wire 解码 `ram_authority` 只接受 0/1，其它值 `BadRamAuthority`（`handoff_v1_read.rs:162-166`）；startup 记录只做覆盖断言（`kernel_info.rs:520-532`） | 无实质差异。`authority` 的“不可配对”承诺兑现为“同一值携带声明”，构造点仍可传错的表述与实现一致 | 接受 |
| 所有权 | `BootStorage` 所有者、`BootReport<'a>` 视图、`FileBytes` 区分 Borrowed/Owned（§3.4.5）；接收侧深拷进 `.bss` 七落地区（§4.5）；U-Boot 腿“全部写进 storage”，`BootPrepareResult` 只带报告与三个类型化地址（§3.4.5）；直核两腿承认有“两个所有者”（`BootStorage` + `DTB_FREE/DTB_RESERVED/DTB_SOURCES`，§3.4.5） | 上述均成立；另有未进文档的第三条所有权通道：U-Boot 腿的文件索引与 DTB 指针走 `static mut BOOT_FILE_TABLE_PTR` / `static mut DTB_PTR`（`opensbi_helpers.rs:481/534`），安装函数的二次安装检查是 `debug_assert!`（`opensbi_helpers.rs:497-506`）；`KERNEL_INFO` 是 `Option<KernelInfo>` 且 `store_kernel_info` 无条件写入（`globals.rs:277`、`lib.rs:1559`），`&'static` 返回值与 `BklProtected` 的安全论证依赖“写完只读” | 文档的“storage 是唯一所有者”只对语义表成立；文件索引/DTB 指针是两条无生命周期表达、release 下静默覆盖的全局；`KERNEL_INFO` 的写一次契约未强制 | 需用户裁决（P2 两条）。U-Boot 腿与直核腿的存储划分本身接受 |
| 错误 | 全链路 fail-closed：`read_required` panic、模块缺件 panic、`BootWindows::push` 反序/空区间 panic、`pt_alloc::register` 两档都断言、配对循环按状态码分派（§3.2/§3.8/§4.2.1）；描述符错误走 `PolicyError`→`MapError`（§4.2.2 写“经 `MapError::Policy`/`RejectedDescriptor` 上抛”） | `pt_alloc::register` 用 `PT_CLAIMED.swap` + `assert!`，release 同样生效（`pt_alloc.rs:100-101`）；`FileLoader::read_required` 默认 panic（`loader.rs:138-145`）；`BootWindows::push` 先断言 `start < end`（`lib.rs:1082`）；`PairError` 是枚举，`EBS_ATTEMPT_LIMIT = 2`（`ebs_pair.rs:54`）；`MapError::Policy` 未被构造 | 唯一的实现面偏差是 `install_boot_file_table`/`install_dtb_ptr` 的二次安装检查只在 debug 生效，与项目对 `pt_alloc` 明示的“两档都 fail closed（F-R23-05）”策略相反；`MapError::Policy` 是死通道 | 需用户裁决（P2 一条 + P3 一条） |
| 并发 | boot 单线程、kernel SMP + BKL；`PT_ALLOC` 写一次后只读，`PT_CLAIMED` 串行化写、`PT_REGISTERED` 发布（§3.8）；`BootStorageCell` 一次性借用（§3.4.5）；`BootAlloc` 的 load/check/store 在 boot 单线程前提下等价（§4.7 未展开并发） | `pt_alloc.rs:61-69` 与文档引文逐字对应；`BootStorageCell::take` 用 `AtomicBool::swap(AcqRel)` + `assert!`（`boot-shim/src/lib.rs:164-172`）；`BootAlloc` 模块文档明写“AtomicU64 不使 bump 序列可线性化，按 boot-only 单线程使用”（`boot_alloc.rs:14-22`） | 无差异。`trait-audit.md` 把 `BklProtected` 判为 `NO-IMPL`（0 实现）是计数假阴性：实现由 `bkl_protected_impls!` 宏生成（`globals.rs:126-171`），`Sealed` 同理。已核实，不作死抽象 | 接受（附审计读数更正） |
| 抽象边界 | `minix-kernel` 不依赖 boot-shim/uefi，共享类型住 `minix-boot`（§3.3）；取件差异只收在 `FileLoader`，没有内存图来源 trait（理由：EBS 的 MapKey 配对携带不了）（§3.4.1）；大页能力放 `HugePages: Paging` 扩展 trait（§3.6）；裸 `(u64,u64)` 只允许在固件/分配器边界，内核内部一律 `PhysRange`，`BootWindows::check` 返回 `CheckedWindows` token（§4.4） | `os/kernel/Cargo.toml` 的 `[dependencies]` 无 `boot-shim`、无 `uefi`；`os/kernel-image/Cargo.toml` 只在 riscv64/aarch64 target 表加 `boot-shim`（opensbi feature）与 `fdt`；`HugePages` 无默认实现、标识位（x86 PS）留在 `map_huge` 内（`x86_64/paging.rs:1189/1214`）；`BootWindows.ranges: [PhysRange; 32]`，`push` 是唯一转换点，`CheckedWindows` 只由 `check` 构造，建根只收 token（`lib.rs:1068-1216`）；x86_64 的 `kernel_identity_mmio_regions()`（`arch/src/lib.rs:258`）只有测试读者，Step 1 的 `identity_device_windows()` 在 x86 返回空（`kernel/lib.rs:1015`） | 边界本身成立。遗留物一处：x86 的 LAPIC/IO-APIC 固定页表与 `device_window_regions()`（Step 2c 的实际机制）重复，且无生产读者 | 接受边界；P3 一条遗留物 |
| 可测试性 | `FileLoader` 是真正的接缝（`MockLoader`）；`BootAlloc` 的实例 API 让测试不污染全局；`BootPageSupport::from_parts` 受 `cfg(any(test, feature="test-support"))` 限制；`ebs_pair` 用 `RawOps` 注入固件调用；内存表单表的 `set_memmap_len`/`set_reserved_len` 只给测试（§3.2/§3.8/§4.4） | 上述逐条成立（`loader.rs:509`、`boot_alloc.rs:35-156`、`paging_ext.rs:157-160`、`ebs_pair.rs:73`、`boot_storage.rs:162/182`）；`enter_kernel_image_v1`/`prepare_kernel_image_v1` 是 `cfg(not(mock))` + `!`，宿主测试只能覆盖到 `BootWindows` 与 `BootAlloc` 这一层 | 无差异；文档 §5 已自认「宿主覆盖不到发送侧入口」 | 接受 |

## FINDING RD34-DESIGN-D2-01（P3）

- 位置：文档 §4.2.2 末段（越界描述符上抛路径）；实现 `os/boot-shim/src/ebs_map.rs:343-373`（`MapError` 定义与 `From<PolicyError>`）。
- 严重度：P3
- 分类：[DEL]
- 问题：文档写“越界即整张图作废（`PolicyError::OutOfPhysRange`，经 `MapError::Policy`/`RejectedDescriptor` 上抛）”，把 `MapError::Policy` 写成两条并列的活通道之一。实现里 `MapError::Policy(PolicyError)` 只由 `impl From<PolicyError> for MapError` 构造，而该 `From` 全仓无调用点：`classify` 用 `adjudicate(desc, pa_limit).map_err(|failed| rejection(index, desc, failed))?`（`ebs_map.rs:705`），计数与写入两遍都用 `rejection(...)`（`ebs_map.rs:681`），越界经 `MapError::RejectedDescriptor(DescriptorRejection { failed: OutOfPhysRange, .. })` 上抛。
- 影响：错误面出现两个语义重叠的变体，读者会以为存在一条“只有失败原因、没有描述符上下文”的路径；实际诊断永远带 index/raw_type/attr，`Policy` 分支是死码。C 侧没有 `Result` 注释表，这条变体是 Rust 侧重构的中间产物，不是 C 语义的载体。
- 证据或推理：`grep -rn "MapError::Policy" os/ --include=*.rs` 只命中 `ebs_map.rs:371`（`From` 体内）；`From<PolicyError>` 没有任何 `?` 或 `map_err(MapError::from)` 调用点（`checked_shape`/`checked_len`/`adjudicate` 都返回 `Result<_, PolicyError>`，调用方一律显式转 `rejection`）。
- 最小修复：删 `MapError::Policy` 与 `impl From<PolicyError>`；文档 §4.2.2 那句改成“经 `MapError::RejectedDescriptor`（携带失败描述符与 `PolicyError`）上抛”。两个改动互不依赖，任一单独做都能消掉“文档指一条不存在的路”的问题。
- 置信度：高（构造点与调用点都已穷举 grep）。
- 是否需源码核实：否（已核实）。
- 若我错了：若存在本仓外的 `MapError::Policy` 构造者（例如某个下游 crate 依赖 `From<PolicyError>` 做 `?` 转换），删除会破坏 API。但 `ebs_map` 是 boot-shim 的内部模块，`MapError` 未在 `lib.rs` 再导出，下游无法依赖。

## FINDING RD34-DESIGN-D2-02（P2）

- 位置：文档 §3.2（取件抽象）、§3.4.5（所有权）与 §4.3（`BootFileTable`）；实现 `os/boot-shim/src/opensbi_helpers.rs:481-506`（`BOOT_FILE_TABLE_PTR`）、`:534-545`（`DTB_PTR`）、`os/boot-shim/src/uboot_main.rs:424-431`。
- 严重度：P2
- 分类：[MODERN]
- 问题：U-Boot 腿在 `BootStorage` 之外还有两条“无所有者表达”的全局：文件索引表地址与 DTB 物理地址都住在 `static mut` 里，由 `unsafe fn install_boot_file_table` / `install_dtb_ptr` 写入，二次安装检查是 `debug_assert!`。文档把所有权的两条通道讲成 `BootStorage`（所有者）+ `FileBytes`（借用/拥有），没有提到这两个安装型全局；而 release 构建里第二次 `install_boot_file_table` 会静默改写指针——第一次安装之后已经取过 `boot_file_table()` 的读者，与第二次安装之后取表的读者，会看到两张不同的表，且没有任何运行期信号。
- 影响：与项目已经冻结在 `pt_alloc` 上的策略直接冲突——那里的注释写“A second registration is a boot bug: silently keeping the first allocator hides a wiring error (F-R23-05), so fail closed in release too”（`pt_alloc.rs:97-101`），`BootStorageCell::take` 同样是两档 `assert!`（`boot-shim/src/lib.rs:164-172`）。同一份启动契约里，三种一次性全局用了两档 fail-closed，一种用 debug-only，读者无法从“一次性写入”这把伞下判断哪一种真的会停机。可观察行为的影响面有限（当前只有一个调用者），但它是所有权陈述的反例：`FileBytes::Borrowed` 的“底层存储不得被覆盖”合同依赖的正是这张表的地址稳定。
- 证据或推理：`install_boot_file_table` 的 `debug_assert!(BOOT_FILE_TABLE_PTR == 0, "install_boot_file_table called twice")` 在 `--release` 下被编译掉；`prepare_boot` 从 `boot_file_table()` 取表，`uboot_main` 先 `build_shim_table()` 再 install。DTB 同理：`install_dtb_ptr` 写入 `static mut DTB_PTR`，`prepare_boot` 用 `dtb_ptr().expect(...)` 读。
- 最小修复：把这两件东西从全局改成参数——`prepare_boot(storage, bump_pages, table: &BootFileTable, dtb_pa: u64)`。`uboot_main` 是唯一调用者（`grep -rn "prepare_boot(" os/`），改一个签名即可；`install_*` 与两个 `static mut` 一起删除，`boot_file_table()` 的 magic 校验移进 `BootFileTable` 的构造/校验入口。行为不变，且把“二次安装”变成“类型上不存在”。
- 置信度：中高（唯一调用者已核实；若还有别处调 `prepare_boot` 未被我 grep 到，改签名会多一处编译错误，仍安全失败）。
- 是否需源码核实：否（已核实）；但“是否愿意改公共库函数签名”是设计裁决，不是事实。
- 若我错了：若将来需要“表在更早的汇编跳板里装好、Rust 侧晚知道”的形态，参数化会要求跳板把表地址转交进来；但当前 `uboot_main` 的 `build_shim_table` 就是 Rust 侧代码，没有这个约束。若坚持保留全局，最小代价是把 `debug_assert!` 改成 `assert!` 并给 `DTB_PTR` 同样的两档检查——那就不是 [MODERN] 而是“补一致性”，代价更低但不删结构。

## FINDING RD34-DESIGN-D2-03（P2）

- 位置：文档 §3.4.5 末段（“直核腿在信任边界上有两个所有者……若将来把这三类也搬进 storage，需要同时删掉收集器的静态量与第二套守卫”）；实现 `os/libs/minix-boot/src/memmap_source.rs:160-224`（`DTB_FREE`/`DTB_RESERVED` + 原子计数）、`:619-632`（`DTB_SOURCES`）、`:1360-1395`（一次性守卫 + 发布）、`os/kernel-image/src/bootface.rs:147/214/427`（`collect_with_sources` 与本腿的 `build_kernel_info` 实参）、`os/kernel-image/src/bootface_a64.rs:148`、对照 `os/boot-shim/src/opensbi_helpers.rs:296-335`（`collect_owned()` 路径）。
- 严重度：P2
- 分类：[MODERN]
- 问题：文档把“直核两腿的第二套静态表”登记为刻意取舍，理由是“让 DTB 收集器保持单一写入点”。但同一份收集器已经有 `collect_owned()`：U-Boot 腿正是用它把 `free()/reserved()/sources()` 拷进 `BootStorage` 的槽位再提交，然后完全不碰 `DTB_*` 静态量。也就是说“单一写入点”与“全量 owner”并不互斥——`collect_owned` 已经是既成事实的第三条路，直核腿只是还没换过去。
- 影响：两套所有权意味着两套一次性守卫、两套容量口径（`DTB_MEMMAP_MAX`/`DTB_SOURCE_MAX` 与 `WIRE_*_CAP`）、以及 `BootStorage` 里约 9 KiB 永远不写的 memmap/reserved/source 槽（文档已量化）。更实际的风险是漂移：`KernelInfo.memmap` 在直核腿指向 `DTB_FREE`，在 v1 接收腿指向 `MEMMAP_REGION_STORE`，两条路径的“切片从哪来”不同，而消费者只应按区间读——现在多了一条只有直核腿走的分支。
- 证据或推理：`opensbi_helpers.rs` 的 `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE).collect_owned()` 返回拥有型表，随后 `memmap_slots[..].copy_from_slice(owned.free())`、`commit_map_tables(...)`、`source_slots[..].copy_from_slice(owned.sources())`、`set_memmap_sources_len(...)`；`bootface.rs:214` 走的是另一条 API：`DtbMemoryMap::new(&fdt, KERN_PHYS_BASE).collect_with_sources()` 发布到 `DTB_FREE` 一族并返回借用切片，`bootface.rs:356-441` 只从 `storage` 取 modules/platform/startup，memmap/reserved/sources 三类仍由静态表提供（作为 `build_kernel_info` 的实参）。
- 最小修复：两个 `bootface` 改成与 U-Boot 腿相同的三步（`collect_owned` → 拷入 storage 槽 → 提交），`build_kernel_info` 的 `memmap`/`reserved_regions`/`memmap_sources` 三个实参改读 `storage.*()`；删掉 `DTB_FREE`/`DTB_RESERVED`/`DTB_SOURCES`/三个 `_N` 原子与第二套守卫。`boot_validate_and_prepare` 与 strict 门不用改——它们本来就只认切片。
- 置信度：中（`collect_owned` 路径成立已核实；“删静态量”还须确认没有第三方读者；`grep -rn "DTB_FREE\|DTB_RESERVED\|DTB_SOURCES" os/` 目前只命中 `memmap_source.rs` 自身，无外部读者）。
- 是否需源码核实：部分需要——需确认直核腿的 `platform_sources`/`startup` 已经写 storage（已核实：`bootface.rs:325/377-399`），剩余只是 memmap/reserved/source 三类。
- 若我错了：若直核腿在 `arch_boot` 之前还有一次“读 `DTB_FREE` 而不经 `KernelInfo`”的消费（例如 DM 候选或诊断打印），删静态量会漏改一处；以编译错误为准，属于安全失败。若“单一写入点”的真实动机是省 RAM（静态表比 storage 槽小），那与文档写的理由不符，需要作者澄清——因为我引的是文档自己的理由。

## FINDING RD34-DESIGN-D2-04（P2）

- 位置：文档 §3.4.1（`KernelInfo` 字段表“新增”行与 `kern_stack_top`/`syscall_entry` 展开）、§3.4.3（wire 结构）、§4.4（strict 门的“派生标量相等”行）；实现 `os/libs/minix-boot/src/boot_storage.rs:356-381`、`os/boot-shim/src/lib.rs:101-132`、`os/libs/minix-boot/src/kernel_info.rs:447-470`、`os/libs/minix-boot/src/handoff_v1_write.rs:138-150`、`os/libs/minix-boot/src/handoff_v1_read.rs:88`。
- 严重度：P2
- 分类：[MODERN]
- 问题：`BootReport` / `KernelInfo` 里有三个标量是两端都能精确算出的常量关系：`user_sp = minix_types::USER_STACK_TOP`（每架构编译期常量）、`syscall_entry = kern_virt_base`、`kern_stack_top = kern_virt_base + kern_size`。三者在发送侧由 `report()`/`build_kernel_info` 现算，写进 wire（`user_sp`/`kern_stack_top`/`syscall_entry` 三个 `WireVirtAddr`，24 B），接收侧再读出来，strict 门再把它们与同一组公式比对（`kernel_info.rs:453-470`）。传输、解码、断言三段都在证明“同一台机器上的同一个常量等于它自己”。
- 影响：三处成本。(1) wire 多 24 B 与三个字段的解码/编码路径；(2) `build_kernel_info` 的 11 个参数里有 3 个只能传推导值（传别的值会被 strict 门拒），属于“合法类型、错误语义”的参数面——正是 §3.5 删 `bootstrap_len` 时用过的判据；(3) strict 门的三条 `assert_eq!` 占了“挡伪造 wire”这一职责的 3/7，但它们挡的伪造只有“把常量写成别的常量”，而接收侧完全可以自己算。
- 证据或推理：`report()` 的 `user_sp: VirBytes(minix_types::USER_STACK_TOP)`、`kern_stack_top: VirBytes(kern_virt_base.0 + kern_size)`、`syscall_entry: VirBytes(kern_virt_base.0)`；`build_kernel_info` 同样三行；`write_envelope_report` 里还有 `debug_assert_eq!` 复核同一关系。C 侧 `user_sp`/`virt_kern_start` 是 `kinfo` 字段，但 C 的 `kinfo` 是全局变量、同镜像内消费，没有“跨镜像 ABI”这层；Rust 把值送过边界再验证，是翻译残留式的“字段照搬”。
- 最小修复：wire 只留 `kern_virt_base`/`kern_phys_base`/`kern_size` 三个布局标量；接收侧解码后按同一公式补 `user_sp`/`syscall_entry`/`kern_stack_top`（`KernelInfo` 字段可保留，避免波及全仓读者；删的是 wire 字段、builder 参数与 strict 断言）。信封版本 9→10，收发两侧同批改；`envelope_offsets` 的编译期断言同步。
- 置信度：高（生产五腿的赋值点全部核实；测试内核不走 wire，不受影响）。
- 是否需源码核实：否（已核实）；但“是否值得为 24 B 动 wire 版本”是用户裁决。
- 若我错了：若将来某条腿需要 `kern_stack_top` 与 `base+size` 解耦（例如镜像尾有 2 MiB 收口填充、而引导栈要落在填充之前的 `.ld` 符号上），公式就不再成立。文档 §4.4 的第三条 bullet 已经写明“`kernel_boot_stack_top` 是另一个对象、定在收口之前”，但当前代码把它当 sp 用的是镜像尾——所以解耦需求目前不存在；一旦出现，正确做法是**那时**再新增一个字段，而不是现在为一个不存在的需求保留三个可伪造字段。

## FINDING RD34-DESIGN-D2-05（P2）

- 位置：文档 §4.5（`store_kernel_info` 的三个调用点与“同址跳过”）；实现 `os/kernel/src/globals.rs:275-277`、`os/kernel/src/lib.rs:1453-1563`、`:1738`、`:3673-3677`。
- 严重度：P2
- 分类：[MODERN]
- 问题：`KERNEL_INFO` 的注释写“Only written once during boot（单线程，BKL 之前）”，`KernelInfo` 被列进 `bkl_protected_impls!` 的理由也是“write-once-read-only after boot”。但 `store_kernel_info` 是无条件的 `*KERNEL_INFO.get() = Some(k)`，而直核路径上它被调用两次：`arch_boot_finish` 存一次，`kmain` 再存一次（`lib.rs:1738`，实参就是上一次返回的 `&'static` 全局引用）。第二次调用写回的字节与第一次相同，`land_slice` 因同址全部跳过，看起来无害；但它是对一个已经以 `&'static KernelInfo` 交出去的单元做写操作，`unsafe impl BklProtected` 与 `kernel_info()` 的“After boot, read-only”注释都建立在这个写不再发生的假设上。
- 影响：这不是性能问题，而是安全论证的前提没有被代码强制。任何未来新增的 `store_kernel_info` 调用点（例如某个“重新落地”的补救路径）都会在 `&'static` 引用存活期间改写全局；如果新值与旧值不同，所有持有旧引用的读者会看到“切片头换了、但引用没变”的半更新，而 release 下不会有任何断言。当前五条腿的调用序是安全的，但安全来自“大家都没写第三个调用点”，不是类型或断言。
- 证据或推理：`store_kernel_info` 的尾部 `*KERNEL_INFO.get() = Some(k);` 无 `is_none()` 守卫；`kmain` 的实参来自 `jump_to_kmain(kinfo, ...)` 传入的全局地址（`arch_boot_finish`/`resume_additive` 都把上一次的返回值当 `kinfo`），所以第二次是同址写。测试也会多次调用（`lib.rs:5836/5878/5928/...`），说明“写一次”从来不是被强制的性质。
- 最小修复：在 `store_kernel_info` 开头判断“已存值存在且七个切片指针与标量全等”时直接 `return` 已存引用（早于任何写入与拷贝）；不同则保留现状（测试需要）或加 `#[cfg(not(test))] assert!` 定位第三个调用点。前者约 12 行，不改任何调用点，`kmain` 那次从“写同值”变成纯读；后者把“再来一次不同值的 store”变成启动期停在现场。
- 置信度：高（调用点与数据流已核实）。
- 是否需源码核实：否（已核实）。
- 若我错了：若 `kmain` 的 `kernel_info` 实参在某些腿不是全局副本（例如测试内核直接传栈上的 `KernelInfo`），那么第二次 store 是真正的拷贝而不是同址写，early-return 条件不会触发、行为不变；这反而说明“写一次”假设在测试载体里本来就不成立，`#[cfg(not(test))]` 守卫是必须的。

## FINDING RD34-DESIGN-D2-06（P3）

- 位置：文档 §3.4.5（“其余五族的 `push_*`/`set_*_len` 各自有 one-shot 提交断言”）；实现 `os/libs/minix-boot/src/boot_storage.rs:64-90`。
- 严重度：P3
- 分类：[MODERN]
- 问题：`BootStorage` 用六组“长度 + 已提交 bool”表示六类载荷的写入状态（`map_dirty` 加五个 `*_committed`），其中 `modules_len`/`platform_len` 等在 `push_*` 阶段就递增、`*_committed` 直到 `set_*_len` 才置位。于是“长度已经是 12、但从未提交”和“提交了 3、但槽里是上一轮的 12 条”都是可表示状态；`report()` 只拦 `map_dirty` 一族，另外五族的半提交只能靠调用纪律。文档已经承认这一点（“依赖调用纪律，不靠 report() 挡”），但承认不等于消除：一个字段承担两件事（游标 + 状态）就是这六组 bool 存在的原因。
- 影响：`report()` 的语义因此是“当前快照”而不是“完整交接载荷”：UEFI 腿在分类前用它写非内存图载荷（`uefi_helpers` 预 EBS 一次），分类后再用一次。读者必须知道“第一次 report 的 memmap 是空的、这是合法的”，这个信息只写在 §4.2 动作 5 的散文里。任何新增的第五类载荷都要再复制一套 bool 与 setter。
- 证据或推理：`BootStorage` 字段 `modules_len`/`modules_committed`、`platform_len`/`platform_committed`、`param_len`/`param_committed`、`memmap_sources_len`/`sources_committed`、`startup_len`/`startup_committed`；`push_startup`/`push_platform_source` 在已提交后 panic，`set_*` 在重复提交时 panic。六组布尔没有一组参与 `report()` 的拦截，只参与“第二次调用 panic”。
- 最小修复：把每组 `(len, committed)` 换成一个两态枚举或 `Option<usize>` 的等价物——`enum Live { Open(usize), Committed(usize) }`（`push_*` 只在 `Open` 时递增，`set_*` 把 `Open` 变 `Committed`，`report()` 读 `Committed` 或按“当前快照”显式读长度）。这删掉五个 bool、让每组只有一处状态来源；“长度未提交”与“已提交”不再能同时为真。若不想动 `report()` 的“快照”语义，至少把五个 bool 合成一个 `committed: u8` 位图，代价是一个新概念，不如枚举。
- 置信度：中高（状态机结构已核实；收益是局部可读性，不是 bug 修复）。
- 是否需源码核实：否（已核实）。
- 若我错了：若作者刻意要“长度可在提交前被外部读”（当前没有任何读者这么做），枚举会把这种半提交暴露面收窄；但 `report()` 已经是唯一出口，收窄正是目标。

## FINDING RD34-DESIGN-D2-07（P3）

- 位置：实现 `os/kernel/src/boot_alloc.rs:184-202`（三个访问器）；文档 §3.7（`let bump_next = boot_alloc::boot_alloc_next();`）与 §4.7。
- 严重度：P3
- 分类：[MODERN]
- 问题：`boot_alloc_region()`、`boot_alloc_used_bytes()`、`boot_alloc_next()` 是三个独立访问器，各自把三个原子重新读一遍、重建 `BumpRegion`、重跑 `is_consistent()` 断言。`boot_alloc_next()` 在“未初始化”时返回**哨兵 0**（`region().unwrap_or((0, 0))` 后加 `used_bytes()`），而它的唯一消费者（`prepare_kernel_image_v1`）在上一行已经 `.expect("... bump region not registered")` 过，所以哨兵分支永远不可达。
- 影响：三份重复的读取-校验逻辑意味着三次可能不一致的读取（每个原子独立 `Relaxed` load，boot 单线程所以实际一致，但读者得自己论证）；哨兵 0 违反了本文件其余部分的“未初始化 → `Option::None`”约定（`region()` 就是 `Option`）。`boot_alloc_next` 的名字也容易被读成“下一个游标”，而实现是 `base + used_bytes()`，与 `next` 原子没有直接关系。
- 证据或推理：`region()` 已经读了 `base`/`next`/`end` 三元组并断言一致性，却只返回 `(base, end)`；`boot_alloc_next` 再读 `region()` 与 `used_bytes()`（后者又读三个原子）。唯一调用点 `os/kernel/src/lib.rs:461/494`。
- 最小修复：把 `region()` 改成返回 `Option<(u64, u64, u64)>`（base/next/end），删除 `boot_alloc_used_bytes` 与 `boot_alloc_next`；调用点用三元组拆出需要的两个值，`used_bytes` 由 `next - base` 现场算。三份校验收敛成一份，哨兵 0 消失。
- 置信度：高（调用点已穷举）。
- 是否需源码核实：否（已核实）。
- 若我错了：若还有本仓外的读者依赖 `boot_alloc_used_bytes()` 的语义（`vm_handoff` 用它填 `kernel_allocated_dynamic`），改成三元组后那处要跟着改一行——仍是编译期可见的改动，不会静默。

## FINDING RD34-DESIGN-D2-08（P3）

- 位置：文档 §4.6（“x86_64 的 `kernel_identity_mmio_regions()` 本身返回 IO-APIC 与 local APIC 两条固定页，但当前没有生产读者”）；实现 `os/arch/src/lib.rs:250-264`、`os/kernel/src/lib.rs:1003-1017`、`os/kernel/src/vm_handoff.rs:1186-1197`。
- 严重度：P3
- 分类：[DEL]
- 问题：文档自己承认这张表在 x86_64 上没有生产读者，但没有把它列为待删物。实际读法比文档更糟：x86_64 的 `identity_device_windows()` 明确返回 `&[]`（Step 1 不用它），Step 2c 用的是另一张表 `device_window_regions()`（`arch/device_window.rs`）；唯一读 `kernel_identity_mmio_regions()` 的地方是 `vm_handoff.rs` 的一个自检测试（断言表自身页对齐、有序），也就是说这张表靠“测试读自己”活着。同时 `arch/src/lib.rs` 的文档注释仍写着“A process page table missing them faults the first tick after a CR3 switch”——这条理由的前提（进程根重放恒等窗口）已随 §3.4 的恒等重放退场，注释是陈旧前提。
- 影响：读者面对三张“设备窗口”表（`identity_device_windows`、`device_window_regions`、`kernel_identity_mmio_regions`）无法从名字判断哪张是活的；x86 的固定页常量与 `device_window_regions` 里的同类常量可能漂移而无人发现（没有生产读者就没有对账点）。这是“用注释维持的死机制”，正是本项目反复清理的那一类。
- 证据或推理：`os/arch/src/lib.rs:258` 的 x86 实现返回两条固定页；`os/kernel/src/lib.rs:1015-1017` 的 `not(any(aarch64, riscv64))` 分支返回 `&[]`；`device_window_regions()` 在 `os/kernel/src/lib.rs:1369/2437/2481` 与三个 arch 的 smp 文件里被实际使用。
- 最小修复：删除 x86_64 的 `kernel_identity_mmio_regions()` 实现（保留 riscv64 的实现，它仍是 Step 1 的活输入），把 mock/其它目标的注释改成“riscv64 专用”；同步删掉 `vm_handoff` 里那条只为它存在的自检测试，或把测试改成只对 riscv64 目标编译。`stale premise` 注释随手改掉。
- 置信度：中高（已确认 x86 分支无生产读者；若某个 QEMU 测试内核在 x86 上直接调它，grep 会命中，目前没有）。
- 是否需源码核实：否（已核实）。
- 若我错了：若“进程页表重放恒等窗口”其实没退场、而我只看了 §3.4 的陈述没看全实现，那么这张表仍是活输入，删除会导致 x86 用户进程第一次 tick 缺 LAPIC 映射。反证判据：`git grep -n "恒等重放\|kernel_identity_mmio_regions" os/servers/vm` 应为空；我核到的是 vm 侧没有读者，但“没有读者”也可能意味着“该重放功能本身还没实现”，那时这张表是未来实现的预留——那就应把它降级为 TODO 注释而不是可执行常量表。

## FINDING RD34-DESIGN-D2-09（P3）

- 位置：文档 §3.2（“另有编译期 `const _` 把 `NAME_POOL[i]` 与 `boot_shim::loader::MODULE_NAMES[i]` 逐字节比较”）；实现 `os/kernel-image/src/bootface_common.rs:27-68`。
- 严重度：P3
- 分类：[MODERN]
- 问题：模块名有两份数据：`minix_boot::MODULE_NAMES: [&str; 12]`（经 `boot_shim::loader` 重导出）与 `NAME_POOL: [[u8; 16]; 12]`（逐字节字面量），后者存在的理由是物理视图下 `&str` 的胖指针不可解引用（文档与源码注释都写了）。这个理由对“运行时不要解引用 `&str`”成立，对“必须手写第二份字节”不成立：两份可以在编译期由一份派生。
- 影响：表顺序/拼写改一处必须在两处同时改，漏一处由 `const _` 编译期断言兜住——也就是说正确性有保障，代价是维护面翻倍、以及读者要读 12 行转义串确认它们真的对应。C 侧只有一张 `struct boot_image image[]`，没有第二份字节表。
- 证据或推理：`const _: () = { ... assert!(got[j] == want[j], "NAME_POOL order drifts from MODULE_NAMES") ... }` 已经在 const 上下文里逐字节比较两份表；同样可以在 const fn 里从 `MODULE_NAMES[i].as_bytes()` 拷进 `[[u8; 16]; 12]`（工具链是 1.94，const 可变局部可用；这段 const 块本身就在用 `while` 循环）。
- 最小修复：`NAME_POOL` 改成 `const fn build_name_pool() -> [[u8; 16]; 12]` 的求值结果，函数体把 `MODULE_NAMES[i].as_bytes()` 拷进槽并补 0；`NAME_SLOT` 与长度断言保留。删掉第二份字面量与对账断言。物理视图约束不变（结果仍是纯字节数组、无重定位）。
- 置信度：中（const fn 能力已确认；`as_bytes()` 在 const 上下文可用是稳定能力）。
- 是否需源码核实：否（已核实两份表存在与断言形态）。
- 若我错了：若 `MODULE_NAMES` 的 `&str` 在某些目标上被链接器放进需要重定位的 section、导致 const 求值在交叉编译时失败，派生方案会编译不过——那是编译期可见的失败，可退回现状。另一种反对意见是“手写表是刻意的双人复核”，但 `const _` 断言已经把复核自动化了，手写部分只剩抄写风险。

## FINDING RD34-DESIGN-D2-10（P3）

- 位置：实现 `os/kernel/src/lib.rs:241-278`（`arch_boot` x86_64 的取证打印块）。
- 严重度：P3
- 分类：[DEL]
- 问题：x86_64 的 `arch_boot` 里有一块自标 “Boot-visibility evidence capture (temporary)” 的取证代码：打印 `kernel_info` 实参地址、`reserved_regions` 长度与首条，附一段描述历史 bug（RDI 被 BOOTX64.EFI codegen 复用、观察到 `kinfo=0x1ddda2ee`）的 20 行注释。x86_64 没有生产直核腿，但 `arch_boot` 被 `KERNEL_ENTRY_ANCHOR` 取地址（`os/kernel-image/src/main.rs:113-115`，链接脚本 `KEEP`），所以这段代码与字符串会进 x86_64 生产镜像。
- 影响：每次启动多两次串口写入（`arch_boot arg=` 与 `reslen=` 两行，带十六进制打印），以及一条 `kernel: arch_boot entered` 路标；这些不在文档 §6.1 的路标清单里，读者按文档排查时会把它们当噪声。历史 bug 的因果已经写在 `store_kernel_info` 的调用注释里（`arch_boot_finish` 的注释也复述了），取证块与它是重复叙述。
- 证据或推理：块首注释自带 `(temporary)`；`arch_boot` 的文档注释写“called by boot-shim after ExitBootServices”，而 x86_64 的产线入口是 `arch_boot_resume_v1`，`arch_boot` 只被测试载体与 anchor 引用。C 侧没有对应物（C 的 `pre_init` 没有“打印入参指针”这一步）。
- 最小修复：删掉取证块（保留 `kernel: arch_boot entered` 一条路标，若门脚本 grep 它——需先核对脚本），把历史因果留在 `arch_boot_finish` 的注释里一条即可。若担心回归不可见，改成 `#[cfg(feature = "boot-forensics")]` 门控，默认不编译。
- 置信度：高（代码可见、无生产读者）。
- 是否需源码核实：需核对门脚本是否 grep `arch_boot arg=`/`reslen=`（`grep -rn "arch_boot arg\|reslen=" os/qemu-tests tools`）。若某脚本依赖它，删除会改测试面，那时改门控而非删除。
- 若我错了：若这段取证正是最近一次真机归因的证据来源、作者刻意留着等下游修复确认，那么它应在文档 §6.3（典型失败模式）里被登记，而不是以 `(temporary)` 留在生产入口。无论哪种，当前形态（无登记、无门控、默认编译）都不是终态。

## FINDING RD34-DESIGN-D2-11（P3）

- 位置：文档 §3.4.5 第二段（“`set_modules_len`/`set_platform_len`/`set_param_len`/`set_memmap_sources_len`/`set_startup_len` 各自只允许提交一次”）；实现 `os/libs/minix-boot/src/boot_storage.rs:292-302`。
- 严重度：P3
- 分类：[DEL]
- 问题：五件套里的 `set_startup_len` 是 `#[cfg(test)] pub(crate)`，生产代码没有这个入口（生产用 `push_startup`）。文档把它与四个生产 setter 并列，读者会以为直核腿/U-Boot 腿通过“提交一次长度”来定稿 startup 表；实际两条腿都是逐条 `push_startup`，最后以 `startup_objects().len()` 为长度。
- 影响：这是“文档点名了一个不存在的生产 API”的小号实例，与 FINDING-01 同类但位置不同。它不影响行为，影响的是按文档读代码的人对“startup 表怎么定稿”的判断。
- 证据或推理：`set_startup_len` 上的 `#[cfg(test)]`；`boot_storage.rs` 里 `push_startup` 的 `assert!(!self.startup_committed, ...)` 才是生产一次性语义；`set_memmap_len`/`set_reserved_len` 同样是 `#[cfg(test)]`，但文档没有把它们列进这句（它列的是 `commit_map_tables`），所以这处不是同类问题的常态，而是孤例。
- 最小修复：把这句改成“`set_modules_len`/`set_platform_len`/`set_param_len`/`set_memmap_sources_len` 各自只允许提交一次，startup 表由 `push_startup` 逐条追加并在提交后拒绝继续追加”；顺带注明两个单表 `set_*_len` 仅供测试。
- 置信度：高。
- 是否需源码核实：否（已核实）。
- 若我错了：若作者的意图是“把 `set_startup_len` 也开放给某条尚未落地的腿”，那文档是在描述终态而非现状，应像 §3.1.1 那样标明“未接线”；但当前没有任何腿调用它，`#[cfg(test)]` 也不允许。

## 翻译残留分类（任务第 2 项的六类盘点）

| 类别 | 本范围内的实例 | 判断 |
|---|---|---|
| 单实现依赖注入 trait | 无。`BootShim` 已在 `5ed051a53` 后删除（`grep -rn "BootShim" os/` 只剩两处注释：`opensbi_helpers.rs:242`、`boot-shim/src/lib.rs:5`）；`FileLoader` 有 2 个生产实现（`UefiFileLoader`、`UbootFileLoader`）+ `MockLoader` 测试实现，是接缝不是残留 | 接受 |
| 无状态 Manager | 无。`Serializer`/`MapStorage`/`BootStorage`/`BootWindows` 都持有状态；`PlacementPool`（`bootface_common.rs:159-167`）是无方法的参数包，但它的存在理由（三个相邻 `u64` 互换仍能编译）有实测动机，不算 Manager | 接受 |
| 布尔或字符串状态机 | `BootStorage` 的 `map_dirty` + 五个 `*_committed`（FINDING-06）；`PlatformDescSource.len = 0` 是长度哨兵不是状态机；`PageSupport.supported: bool` 是硬件事实位，两种取值都合法，不属此类；`BootStorage` 里 `ZERO_REGION`/`ZERO_MODULE`/`ZERO_SOURCE`/`ZERO_RECORD` 是未用槽占位，只经已提交前缀暴露 | 一条 P3 |
| 哨兵返回值 | `boot_alloc_next()` 未初始化返回 0（FINDING-07）；`pt_alloc::free_pt_page` 在 `register_free` 之前走 `uninit_free` no-op（`pt_alloc.rs:74/171-176`）——“未注册”与“已注册但无需归还”不可区分，但唯一调用链是 VM 销毁路径，且文档 §3.8 已写“未注册归还钩子时……中间页表页泄漏”，语义与注释一致，不单列 finding；`arena_pa == 0` 分道是固件入口 ABI 的判别位，与 C 的 multiboot magic 同构 | 一条 P3 + 两条接受 |
| clone 到能编译 | `KernelInfo: Copy`（`#[repr(C)]` POD，跨边界按值交接必需）；`KernelMapPolicy`/`BootPagePolicy`/`BootPageSupport` 手写 `Copy`（小值 + `PhantomData`）；`BootReport: Copy`（纯引用视图）。未发现“克隆大对象只为过借用检查器”的实例 | 接受 |
| 对象图共享 | `bootface` 在写完后用 `as_ptr()` 重建五个 `&'static` 切片（`bootface.rs:408-425`），与 `store_kernel_info` 的七落地区是同一手法的两处实现；两处都有 one-shot 或深拷论证。共享面里真正的问题是 `KERNEL_INFO` 的重复写入（FINDING-05）与 U-Boot 腿的两个 install 型全局（FINDING-02） | 两条 P2 |

## 死抽象（任务第 3 项：trait-audit 读数逐条自核）

- `BklProtected`（audit：`NO-IMPL`，0 实现 31 引用）与 `Sealed`（audit：`SINGLE-IMPL`）：**审计假阴性**。两者都是 sealed marker trait，实现由 `bkl_protected_impls!` 宏生成（`os/kernel/src/globals.rs:126-171`），宏调用里有 20+ 个类型（`KernelInfo`、`ProcessTable`、`IrqManager<…>`、`u8` 等）。trait-audit 的 `[SCOPE]` 已声明“宏、cfg、跨行写法可能漏计”，此处正是漏计。**不是死抽象。**
- `MapError::Policy`：死枚举变体（FINDING-01）。
- `kernel_identity_mmio_regions()` 的 x86_64 臂：死表（FINDING-08）。
- `BootPrepareResult`（audit 未列，因为定义在 `minix-boot` 且只被 `boot-shim` 转导出）：唯一消费者是 `uboot_main`，但它是 `prepare_boot` 的返回契约（报告 + root + bump + arena 四个类型化值），删掉就要让 `prepare_boot` 改成四个 out 参数。**保留**。
- `BootStorageFull`：两个 `push_*` 的错误类型，生产调用者立即 `expect`。它是“固定容量列表满了”的类型化表达，不是抽象层。**保留**。
- `FileLoader`（audit：`MULTI-IMPL`，3 实现 15 引用）：测试接缝 + 两固件实现，**保留**，且是本篇唯一被文档证明“承担测试职责”的 trait。
- `HugePages`（`MULTI-IMPL`，4 实现 28 引用）、`HigherHalf`（4/31）、`BootArch`（4/55）：多实现，跨架构分派，**保留**。
- `PageAllocator`（`type ... = fn(...)`，不是 trait）：2 个实现（UEFI `allocate_pages`、U-Boot bump），作为函数指针传递，**保留**。

## 非法状态（任务第 4 项：可表示但语义不可能）

| 状态 | 可表示处 | 语义上为何不可能 | 当前防线 | 处置 |
|---|---|---|---|---|
| 已提交长度、槽未写 | `BootStorage` 六组 `(len, *_committed)` | 报告会暴露未定义条目 | 调用纪律 + 下游 strict 门 | FINDING-06（P3） |
| 全局 `KernelInfo` 二次写入 | `KERNEL_INFO: Option<KernelInfo>` + 无条件赋值 | `&'static` 已发出后不得改写 | 无（同值写掩盖） | FINDING-05（P2） |
| 文件表二次安装 | `static mut BOOT_FILE_TABLE_PTR` | 两个“唯一”表同时存在 | `debug_assert!`，release 静默 | FINDING-02（P2） |
| `MemoryRegion` 零长/未对齐 | 结构体本身 | 消费者会静默跳过或算错跨度 | `validate_at` 三档都拒；生产侧分类器/收集器先拒 | 接受（改类型需 newtype 并波及 wire 解码，收益不抵） |
| `PlatformDescSource.len == 0` 表示“未知” | 类型 | 零长 blob 无意义 | 窗口构造回退到 2 MiB 片 | 接受（旧生产者兼容，文档 §3.4.3 明写） |
| `arena_pa == 0` 选直核面 | 入口参数 | v1 链不会传 0 | 固件 ABI 约定；riscv64 接收臂显式 halt | 接受（与 C 的 magic 判别同构，改协议会动固件 ABI） |
| `CheckedWindows` 不携带 `phys_addr_limit` | token | token 只证明“对某个上限检查过” | 唯一构造点用同一 `support` | 未单列 finding；若要做，把 limit 存进 token 是 P3 级改动 |
| `BootWindows` 未用槽是 `EMPTY_PHYS_RANGE` | 数组填充 | 空窗口不该被映射 | `as_slice` 只暴露 `len` 前缀 | 接受（哨兵不可观测） |

## ARCH-PROPOSAL

无。本轮没有发现“需要改变 boot 是什么”的机制替换、新责任、新字段、新检查或新策略；FINDING-04 建议删字段（收缩传输面、不减语义），FINDING-02/03 建议删所有权通道，FINDING-08/09/10 建议删残留物，都不扩张 boot 的定义。若用户认为“wire 只留三个布局标量、其余派生”属于交接协议语义变更而非纯现代化，可把 FINDING-04 升级为 `[ARCH: 交接载荷去掉三个派生标量]` 并走三处一致标注；我按任务的 `[ARCH]` 定义（机制替换、新责任/字段/检查/策略）判断它不构成扩张，故留在 FINDING。

## VERDICT

**值得改（结构性，按优先级）**

1. FINDING-02（P2，[MODERN]）：U-Boot 腿的两个 install 型 `static mut` 全局 + debug-only 二次安装检查。它与项目已冻结的“两档 fail-closed”策略（`pt_alloc`/`BootStorageCell`）不一致，且是“所有权由类型表达”这一设计承诺的反例。唯一调用者，参数化成本极低。
2. FINDING-05（P2，[MODERN]）：`KERNEL_INFO` 的写一次契约未强制，`kmain` 的同值重写是对已发 `&'static` 的写入。同值早返回约 12 行。
3. FINDING-03（P2，[MODERN]）：直核两腿的第二套 DTB 静态表。文档已登记为取舍，但 `collect_owned` 让它从“将来再说”变成“现在可删”；删掉的是两套守卫、两套容量口径与 `KernelInfo.memmap` 的第二条来源路径。
4. FINDING-04（P2，[MODERN]）：wire 里的三个派生标量。收益是删 24 B + 3 个 builder 参数 + 3 条自证断言，代价是信封 9→10；这条最需要用户裁决，因为它是“跨镜像 ABI 面”的收缩，收益不在启动行为而在后续维护。

**值得改（清理/文档面，可与上批一起或单独做）**

5. FINDING-01（P3）：`MapError::Policy` 死变体 + 文档把它写成活路径。
6. FINDING-08（P3）：x86 的 `kernel_identity_mmio_regions` 死表 + 陈旧前提注释。
7. FINDING-10（P3）：`arch_boot` 的 `(temporary)` 取证块。
8. FINDING-07（P3）/ FINDING-09（P3）：三个 `boot_alloc` 访问器合一、`NAME_POOL` const 派生——都是“少一份重复、外部行为不变”。
9. FINDING-06（P3）/ FINDING-11（P3）：`BootStorage` 状态压缩、`set_startup_len` 的文档口径。二者都不影响行为；确认轮默认 SUGGESTION。

**合理取舍（拒绝改动）**

- §3.5/§3.9 的字段删除路线（`bootstrap_start/len`、`free_upper_idx`）：文档的论证（布尔事实占两个字段、没有真实读者）成立，实现已落地；不重开。
- `BootWindows`/`PhysRange`/`CheckedWindows`：`PhysRange` 拒反向、`push` 拒空、`check` 出 token、建根只收 token，四层各挡一类错误；token 不携带 limit 是小瑕，不抵改造。D-035/D-037 的裁决与实现一致。
- `FileLoader` 单 trait 双实现 + Mock：满足“承担测试接缝”的判据，不是单实现依赖注入。
- `Disposition` 五态枚举、`ValidationLevel` 三态枚举：已经是“bool 换 enum”的正确形态（`ValidationLevel` 的文档还写明了它替代两 bool 的四种组合中只有三种合法）。
- `KernelInfo: Copy` 与 `bootface` 的 `&'static` 重建：`#[repr(C)]` POD + one-shot 存储，这是跨镜像交接的必要手法；`Copy` 没有复制大对象（七个切片是 16 B 胖指针、七个标量各 ≤8 B，整结构约 224 B），`store_kernel_info` 的 `let mut k = *kernel_info` 也是同量级。
- aarch64 `arena_pa == 0` 分道、`PlatformDescSource.len == 0` 回退：都是固件/旧生产者边界上的约定，改它们要动 ABIs 或 wire 兼容。
- 直核腿的 medany/物理视图约束（`NAME_POOL` 不要重定位、`_start` 只清到 `__bss_end`）：这是机器状态事实，不是设计选择；FINDING-09 只删“手写第二份字节”，不碰“不要 `&str` 胖指针”的约束。
- 本轮是确认轮，设计 P3 默认 SUGGESTION：FINDING-01/06/07/08/09/10/11 均未实施，只登记。仓库文件未被修改；本文件是唯一写入物。

**结论**：文档第 3、4 章的设计陈述与现有实现在七个维度上总体一致；不一致集中在“文档点名了非生产 API 面”（FINDING-01/11）和“所有权/一次性契约在三个全局上没有统一表达”（FINDING-02/05/03）。没有发现非法状态导致的现存 bug，也没有发现需要改变 boot 定义的架构演进项。

