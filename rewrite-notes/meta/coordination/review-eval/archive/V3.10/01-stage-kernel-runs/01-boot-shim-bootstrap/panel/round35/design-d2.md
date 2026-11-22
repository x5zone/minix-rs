# 01 · Boot-shim 引导准备与控制权交接 — 设计第二遍（D2）独立差异审查

> 基线：本项目禁用 D1（`DECISIONS.md` D-024「D1 停跑」），本遍只做「文档第 3、4 章主张 ↔ 源码事实」的独立差异审查。
> 只使用了目标文档 §3/§4、`related-code.txt` 列出的源码、`minix3/` 的 C 源与只读命令（`rg`/`sed`/`grep`）。未读取作者解释性材料、历史评审与其他面板角色的输出。
> 分类标记：`[MODERN]`（C 有此语义，换现代表达）、`[DEL]`（C 没有此物，删翻译残留或多余层）、`[ARCH]`（改变 boot 是什么，只写入 `## ARCH-PROPOSAL`）。

## DIFF-TABLE

| 维度 | 文档主张 | 现有实现 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 | `ValidationLevel` 三档取代两个布尔，`BeforeTables`/`Standard`/`Strict` 各自有调用者（`kernel_info.rs:187-214`）；`BootStorage` 用 `map_dirty` 挡住"槽已写、长度未提交"的视图流入消费者（`boot_storage.rs:84,137,144,154,174,363`）；`startup` 的生产提交口是 `push_startup`，提交后拒绝写入（文档 `:1769`）。 | 三档枚举确实存在且三档都可达；`map_dirty` 只覆盖内存图的两张表；`startup_committed` 只在零调用者的 `#[cfg(test)] set_startup_len` 里置真（`boot_storage.rs:293-302`），`startup_slots_mut()` 全仓零调用者（`:288-290`），因此 `push_startup` 的 one-shot 断言在任何构建里都不可达（`:306`）。`pt_alloc` 用两个 `AtomicBool` 表达写者状态（`pt_alloc.rs:61-69`），`dtb_copy` 的配置表读取用 `Phase` 枚举（`dtb_copy.rs:70-90`）。 | 半提交防护只覆盖 1/6 载荷族；startup 的"提交"在生产是空操作；内存表 dirty 是真实机制，其余五族只有调用纪律。 | 部分成立。受理 D2-01、D2-02。 |
| 身份 | 模块名是位置身份，strict 门按 `MODULE_NAMES` 逐位核名；`authority` 把"内存图与它的声明"绑成同一个值，wire 只接受 0/1（`kernel_info.rs:175-184,474-491`；`handoff_v1_read.rs:162-166`）；`arena_pa == 0` 是 aarch64 直核分道值（文档 §4.8）。 | 前三条成立。但 `KernelInfo.authority` 的类型是 `InputObjectId`，它有三个变体，`startup_record()` 必须用 `StartupObjects` 作记录标签（`memmap_source.rs:766-780`），所以"权威 = 启动对象"在类型上可表达、在协议上被解码拒绝、在任何生产者里都不存在。aarch64 的 boot 形态身份靠寄存器 x0 的零值，文档自己写明这是非规范观测。 | 类型没有把"不可能"编码掉；boot 形态身份是哨兵而非显式判别。 | 受理 D2-10；哨兵问题转入 `ARCH-PROPOSAL`。 |
| 所有权 | `BootStorage` 是所有者、`BootReport<'a>` 是借用视图，v9 之后 arena 只有 wire 一份表示（`boot_storage.rs:1-26`；`handoff_v1.rs:399-418`）；`FileBytes` 显式区分借用与拥有（`loader.rs:107-135`）；三条 v1 腿深拷进接收方 `.bss`（`kernel/src/lib.rs:649-742`）。 | 三条腿的所有权模型不同：UEFI 腿的 `BootStorage` 只承载 memmap/reserved/modules/platform/param 五类，source 与 startup 由分类器和 EBS 后代码直写 wire（`uefi_helpers.rs:1144-1187`）；U-Boot 腿七类全走 storage（`opensbi_helpers.rs:310-444`）；两个直核腿的 storage 只承载 modules/platform/param/startup 四类，memmap/reserved/source 仍由 `memmap_source.rs` 的 `static mut` 表（`DTB_FREE`/`DTB_RESERVED`/`DTB_SOURCES`）持有。`BootReport` 的 `memmap_sources`/`startup_objects` 在 UEFI 腿恒为空，且只有 U-Boot 主函数读它们（`uboot_main.rs:484,490`）。 | "一份存储、一套守卫"只完成一半（文档 §3.4.5 自述）；`BootReport` 的两个记录切片是腿特定的半视图，不是通用报告字段。 | 文档诚实，但类型没有表达腿差异。受理 D2-08；D2-09 需用户裁决。 |
| 错误 | fail-closed：缺件 `read_required` panic、模块缺件 `read + unwrap_or_else(panic!)`、容量错误 `HandoffV1WriteError`/`BootStorageFull`、二次注册 `assert!`（`loader.rs:146-157`；`pt_alloc.rs:91-111`）。`read` 本身只返回 `Option`，不决定致命性（文档 `:1502`）。 | 一致。另有两处比文档弱或误导：`free_pt_page` 在未注册归还钩子时静默走 `uninit_free` 空操作（`pt_alloc.rs:71-74,165-176`），而 `alloc_pt_page` 未注册时断言；`alloc_module_pages` 把"分配出的末地址越过 DM 可达上界"与"固件分配失败"都折成 `None`（`uefi_helpers.rs:1280-1289`），上层统一报 `out of memory`。 | 错误模型的对称性与诊断精度有缺口，但生产可达路径都由更早的门挡住。 | 受理 D2-11；`alloc_module_pages` 一处并入同条。 |
| 并发 | `pt_alloc` 写一次：`PT_CLAIMED.swap(AcqRel)` 抢写权、`PT_REGISTERED.store(Release)` 发布、读者 `Acquire` 配对（`pt_alloc.rs:24-35,100-140`）；`BootAlloc` 的原子只满足接口契约、不可线性化，boot 单线程（`boot_alloc.rs:14-22`）；`BootStorageCell` 的 `unsafe impl Sync` 以"单线程、一次性取用"为据（`boot-shim/src/lib.rs:134-173`）。 | 与文档一致。另有 `static mut` 族（`memmap_source.rs:158-166,619`；`opensbi_helpers.rs:481,534`；`uefi_helpers.rs:41-42`）全部靠单 hart 契约；`is_free_registered()` 的读不参与任何断言。 | 没有与文档相反的实现。并发安全全部依赖"boot 单线程"这一外部前提，代码没有把前提变成类型；这是本模块的合理选择，但前提失效时多处会静默。 | 接受（boot 契约的一部分，不单列缺陷；与 Kernel = SMP + BKL 的约束不冲突，因为临界区只在进入调度器之前）。 |
| 抽象边界 | 第一原则"抽象语义，不描述机制"；`HugePages` 的页表标识位不进 trait（`paging_ext.rs:85-98`；`x86_64/paging.rs:1160-1180`）；`FileLoader` 是"只处理字节来源"的最小接缝，两个实现各自只提供 `read`（文档 `:1502`）。 | 大页标识位确实留在架构实现内，`PageFlags::DEVICE` 是语义位、各架构自行编码。`UbootFileLoader::read` 不只是字节来源：它对读到的切片跑 `minix_elf::segment_iter`，把长度修剪到最后一个 PT_LOAD 的文件尾，非 ELF 输入回退整槽（`opensbi_helpers.rs:198-224`，修剪在 `:211-216`）；UEFI 的 `read` 返回固件给出的确切长度（`uefi_helpers.rs:1247-1259`）。`loader.rs:465-468` 直接把 `data.len()` 写成 `BootModule.len`。 | "read 只处理字节来源"不成立；同一个模块文件在两条腿得到不同的 `BootModule.len`（U-Boot 去尾，UEFI 留尾），而该长度进入恒等窗口、VM 扣除记录与模块 blob 边界。 | 受理 D2-05。 |
| 可测试性 | `FileLoader` 是真实接缝（两个生产实现加 `MockLoader`，`loader.rs:509`）；`pt_alloc` 单例由独立进程的集成测试覆盖（`pt_alloc_write_once.rs`、`pt_alloc_concurrent.rs`）；strict/standard 两档让测试内核绕过生产门（`kernel_info.rs:248-266`）。 | 一致。另有三处测试质量问题：`test_getters_return_field_values` 是 `assert_eq!(x, x)` 恒真断言，且文件里没有任何 getter；`write_envelope(&KernelInfo)` 的注释说"direct-core legs still use it"，但直核腿不构造 `Serializer`，只有测试调用；`source_audit()` 只有一个测试调用者。 | 一条不可失败的测试被文档计入覆盖；两个 API 的实际消费者只有测试。 | 受理 D2-06、D2-12。 |

### 判定摘要（对应任务第 2–5 项）

- **翻译残留**：`UbootFileLoader::read` 里的 ELF 修剪（与 `build_shim_table` 的 `staged_elf_used_len` 重复，两处都按最后一个 PT_LOAD 取尾）；`KernelInfo.syscall_entry`（C 的 `kinfo_t` 没有这个字段，见 `minix3/minix/include/minix/param.h:14-54`，Rust 侧新加但值是镜像基址）；空 getter 段落与恒真测试；`startup_committed` 这套"提交"语汇。布尔状态机方面，`BootStorage` 的六个布尔与 `pt_alloc` 的两个 `AtomicBool` 都由单线程契约兜住，没有把状态组合扩成非法可表示（`dtb_copy` 的 `Phase` 枚举是正面例子）。哨兵方面，`arena_pa == 0` 与 `ZERO_SOURCE` 是两处语义靠约定维持的零值。克隆方面，`collect_param_buf_into` 把 `LoadOptions` 复制进 `Vec` 只为在协议句柄关闭后继续使用（`uefi_helpers.rs:307-313`），是生命周期折中而非语义需要，量级不足以单列。无状态 Manager 形态在 boot 路径上不存在；`BootStorage`/`BootReport` 是有状态的所有者与视图。
- **对象图共享**：`KernelInfo` 的七个切片全部指向内核 `.bss` 的七个落地区，`KERNEL_INFO` 再按值存一份 `Copy`；`kernel_info()` 与 `store_kernel_info` 的返回值指向同一存储（文档 §4.5），这是单所有者模型，不是共享可变对象图，不构成发现。
- **死抽象**：`startup_slots_mut()`（零调用者）、`set_startup_len`（零调用者）、`startup_committed`（任何构建都不会为真）；`source_audit()`（只有测试）；`write_envelope(&KernelInfo)`（只有测试，注释却称直核腿在用）；`Default for BootStorage`/`BootStorageCell`/`BootAlloc`（无调用者，推测是 `clippy::new_without_default` 的代价，属于工具驱动而非设计驱动，不单列缺陷）。结构体层面没有整体可删的中间层：`BootPrepareResult` 只有一个消费者，但四个字段语义各异（报告、根页、bump 区间、arena），合成元组只会更差；`BootStorage`/`BootReport`/`FileBytes`/`BootStorageCell` 分别承担所有者、视图、借用与拥有、一次性取用的职责，不是无状态转发器。
- **非法状态**：`BootStorage` 的 `set_*_len` 可以把长度设到"实际写入数量"之外，`report()` 会暴露零填充占位记录（最危险的是 `ZERO_SOURCE` = 物理地址 0 的 DTB 源）；`KernelInfo.authority = StartupObjects` 在类型上可表示、协议上被拒；`BootReport` 的"空切片"同时表示"没有记录"和"记录在 wire"；`arena_pa == 0` 同时表示"直核启动"和"参数未设置"。
- **最小化机会**：删 `startup_committed` 与外层提交语汇（D2-01）；用统一 dirty 位替换六族提交纪律（D2-02）；`startup_objects` 分区只服务一条 `any()` 断言（D2-09）；`BootReport` 去掉两个腿特定切片、改成独立的记录视图（D2-08）；`boot_validate_and_prepare` 与 `jump_kernel_image_v1` 用命名结构替代裸 `u64`/同型实参（D2-04、D2-07）；`KernelInfo`/`MemoryRegion`/`BootModule` 上的 `#[repr(C)]` 没有任何布局消费者。

## FINDING DESIGN-D2-01（P2）

- 位置：`os/libs/minix-boot/src/boot_storage.rs:84-90`、`:287-313`；文档 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1769`
- 严重度：P2
- 分类：`[DEL]`
- 问题：`startup_committed` 在任何构建里都不会变成真。唯一置真入口 `set_startup_len` 标了 `#[cfg(test)]`，而且全仓零调用者（连测试也没有，`boot_storage.rs:293-302`）；`startup_slots_mut()`（`:288-290`）同样零调用者；于是 `push_startup` 的 `assert!(!self.startup_committed)`（`:306`）是一条不可达断言。文档把 `push_startup` 说成"startup 分区的生产提交口（它在 `startup_committed` 之后同样拒绝写入）"，描述的是一个任何构建都无法进入的状态。
- 影响：读者会把 startup 分区当成与内存图同级、有封口纪律的分区；实际生产只有"只增不封口"的追加，而且两个"提交"入口都是死代码。若将来有人用 `startup_slots_mut()` 填槽而忘了提交，`report()` 会静默返回 0 条记录（没有 dirty 屏障，见 D2-02），错误被推迟到接收侧 strict 门，现场离根因很远。
- 证据或推理：`boot_storage.rs:89`（字段）、`:116`（初值）、`:288-290`（零调用者访问器）、`:293-302`（零调用者的测试专用置真入口）、`:305-313`（不可达断言）；`opensbi_helpers.rs:465-473`、`bootface.rs:376-403`、`bootface_a64.rs:348-373` 都只用 `push_startup`；全仓 grep `startup_slots_mut` 与 `set_startup_len` 都只有定义行。
- 最小修复：删除 `startup_committed`、`set_startup_len`、`startup_slots_mut`，把 startup 分区定为"只追加、无提交"的唯一协议，文档对应句改为"startup 只有 append 口"。若坚持保留封口语义，则两个直核面与 U-Boot 腿必须在 push 完成后各调用一次提交入口，并删除零调用者的槽访问器。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若 `startup_slots_mut` 是给仓外消费者的公开接缝，应至少标注"无生产调用者"并加 `#[cfg(test)]`；但"生产提交口"这一措辞仍然不实。
- 结论：接受（采用删除方案；代价是 4 处小删除与两段文档改写，收益是去掉一个会误导的提交概念）。

## FINDING DESIGN-D2-02（P2）

- 位置：`os/libs/minix-boot/src/boot_storage.rs:143-190`、`:199-285`、`:356-381`；文档 `:1768-1769`
- 严重度：P2
- 分类：`[MODERN]`
- 问题：`report()` 只检查 `map_dirty`。其余五族（modules/platform/param/sources/startup）的"槽数组访问器 + 长度提交"之间没有屏障，而 `set_*_len` 只校验长度不超过容量、不校验实际写入了多少。具体反例：`platform_slots_mut()` 只写 1 条，随后调用 `set_platform_len(4)`，`report()` 会暴露 3 条 `ZERO_SOURCE` 占位记录，即 `PlatformDescSource::new(DTB, PhysBytes(0))`（`:54`）。文档同一段（`:1769`）承认"没有 map_dirty 级的 report 拦截"，但前一段（`:1768`）的断言"报告永远看不到未写槽"因此不成立。
- 影响：一个写错的 setter 参数会把"物理地址 0 的 DTB 源"送进内核；`push_platform_windows`（`kernel/src/lib.rs:1123-1141`）会为它建立 `[0, 2 MiB)` 恒等窗口，内核随后尝试解析它。modules 族同理会送出名称为空、起点为零的模块。当前五个生产调用点都按"写多少提交多少"执行，所以是潜在故障，不是现行故障；但它正是 `map_dirty` 当初要消灭的那类"合法类型、错误语义"的视图。
- 证据或推理：`boot_storage.rs:54`（占位源不是全零）、`:143-190`（dirty 只覆盖两张表）、`:229-237`（`set_platform_len` 只有上界检查）、`:362-365`（`report` 只查 `map_dirty`）；文档 `:1768-1769` 的自我矛盾。
- 最小修复：给每族一个 `dirty` 布尔——访问器置位，`commit_map_tables`/`set_*_len` 清除（push 路径不需要置位），`report()` 统一断言全为假。约 20 行，公开签名不变。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若文档的本意是"长度只能由与写入同源的计数产生"（分类器返回计数、装载器返回条数、规范化返回字节数），那应把 `set_*_len` 的参数改成一个不可从外部构造的计数令牌（例如让写入函数返回 `WrittenLen`），而不是继续相信调用者。无论哪种修法，"报告永远看不到未写槽"都需要一道可执行检查来支撑。
- 结论：接受（采用 dirty 位方案；代价小，收益是把文档承诺变成断言）。

## FINDING DESIGN-D2-03（P2）

- 位置：`os/libs/minix-boot/src/kernel_info.rs:109-117`、`:458-461`；`os/libs/minix-boot/src/boot_storage.rs:379`；`os/boot-shim/src/lib.rs:126`；`os/kernel/src/lib.rs:2274-2276`
- 严重度：P2
- 分类：`[DEL]`
- 问题：`KernelInfo.syscall_entry` 在生产里不可能是真实的系统调用入口。两条构造路径（v1 发送侧的 `report()` 与直核面的 `build_kernel_info`）都写 `kern_virt_base`（`boot_storage.rs:379`、`boot-shim/src/lib.rs:126`），接收侧从 wire 原样复制；strict 门又断言它必须等于 `kern_virt_base`（`kernel_info.rs:458-461`）。真实入口由 `minix_arch::syscall_entry_va()` 提供：x86_64 是 `&x86_syscall_entry` 符号地址，aarch64 是 `vector_table_va() + 0x400`，riscv64 是 `&riscv64_user_trap_vector`（`os/arch/src/{x86_64,arm64,riscv64}/trap_stub.rs`）。字段自己的文档写"= kern_virt_base + offset"，实现里 offset 恒为 0。
- 影响：对内核它是纯噪声——唯一读者是它自己的等式断言与 `check_boot_postconditions` 的"在镜像内"检查（后者对 `kern_virt_base` 恒真）。对任何未来读取者它是一份**会撒谎的数据**：字段名与文档都说系统调用入口，值却是镜像基址。文档 §4.4（`:2678`）写"当前三个架构都按基址使用"，与源码不符：三个架构都用符号地址。
- 证据或推理：上述行号；C 的 `kinfo_t` 没有这个字段（`minix3/minix/include/minix/param.h:14-54`），所以它不是必须继承的 C 语义；`os/kernel/src/lib.rs:2274-2276` 的注释自己写着"KernelInfo.syscall_entry is reference-only boot metadata"。
- 最小修复：删除字段与信封中的 8 字节（wire 版本从 9 升到 10），把入口检查改为对 `minix_arch::syscall_entry_va()` 求值，或保留检查但不再携带数值；若必须保留字段，则写入真实入口并由接收侧按架构覆盖，同时改名（例如 `syscall_entry_ref`）并把文档改成实际语义。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若该字段的既定语义就是"镜像基址，供需要相对偏移的消费者自行加偏移"，那么它应该改名为 `image_virt_base` 并删除与 `kern_virt_base` 的重复；无论哪种解释，当前的名字与注释都是错的。
- 结论：需用户裁决（建议删除；代价是一次 wire 版本升级与五条腿构造点清理，收益是去掉一个信息量为零且语义错误的字段）。

## FINDING DESIGN-D2-04（P2）

- 位置：`os/kernel/src/lib.rs:839-844`（定义）、`:334-339`、`:446-451`、`:929-934`（三个调用点）
- 严重度：P2
- 分类：`[MODERN]`
- 问题：`boot_validate_and_prepare<A>(kern_virt_base: VirBytes, kern_phys_base: PhysBytes, kern_size: u64, kern_stack_top: VirBytes)` 把四个来自结构体的字段拆成四个位置实参。其中 `kern_virt_base` 与 `kern_stack_top` 都是 `VirBytes`，交换后仍然编译。三个调用点分别从 `KernelInfo` 和 `BootReport` 抄字段，交换属于纯抄写错误。
- 影响：与文档自己用 `PlacementPool` 修掉的"三个相邻裸 u64 互换"属于同一类（文档 §3.2 的 `PlacementPool` 论证），也与 F-R17-04 修掉的"图与声明两个可接反的实参"同类（文档 §3.4.1）。后果是内核按错误的虚拟基址或栈顶规划映射，现场与"页表建错"难以区分；对齐断言只能碰到一部分交换（页对齐的栈顶交换后仍可过对齐检查）。
- 证据或推理：上述签名与调用点；`kern_stack_top` 与 `kern_virt_base` 同为 `minix_types::VirBytes`。
- 最小修复：引入四字段命名结构 `BootScalars { kern_virt_base, kern_phys_base, kern_size, kern_stack_top }`，两个构造源（`KernelInfo`、`BootReport`）各给一个转换；函数只收这个结构。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若作者认为"这三个调用点都是同一结构体的字段、永远不会抄错"，那么 `PlacementPool` 的存在理由同样可以被这样否定；标准应一致。
- 结论：接受。

## FINDING DESIGN-D2-05（P2）

- 位置：`os/boot-shim/src/opensbi_helpers.rs:198-224`（修剪在 `:211-216`）；`os/boot-shim/src/uefi_helpers.rs:1247-1259`；`os/boot-shim/src/loader.rs:422-472`（`BootModule.len = data.len()` 在 `:465-468`）；`os/boot-shim/src/uboot_main.rs:361-401`（生产者侧的 `staged_elf_used_len` 修剪）；文档 `:1502`
- 严重度：P2
- 分类：`[MODERN]`
- 问题：`UbootFileLoader::read` 不只是字节来源：它对每个读到的文件跑 `minix_elf::segment_iter`，把长度修剪到最后一个 PT_LOAD 的文件尾，非 ELF 输入回退整个暂存槽。UEFI 的 `read` 返回固件给出的确切文件长度。于是同一个模块文件在两条腿产生不同的 `BootModule.len`（U-Boot 去掉 ELF 尾部的非装载内容，UEFI 保留），而这个长度会进入恒等窗口（`push_module_windows`）、VM 扣除记录（`classify` 的模块行）与模块 blob 的边界。文档用"两个实现都在 `read` 里只处理字节来源"支撑 `FileLoader` 是最小接缝的论证，这一条不成立。
- 影响：其一，抽象边界被机制细节穿透——"文件来源"接口里藏着 ELF 语义；其二，"同一份装载逻辑"产出的可观察结果依赖腿，跨腿比较模块跨度会得到两个值；其三，非 ELF 文件经此 loader 读出会带上整槽填充字节。当前 kernel 与 12 个模块都是 ELF，所以没有现行故障，但接口契约比文档承诺的窄。
- 证据或推理：`opensbi_helpers.rs:205-220`（`segment_iter` 与 `used` 计算）、`uboot_main.rs:361-401`（同一修剪逻辑的第二份实现）、`uefi_helpers.rs:1247-1259`、`loader.rs:465-468`。
- 最小修复：把修剪上移到共享消费者（`load_boot_modules_into` 或 `load_kernel_with_loader`），`read` 只返回原始字节；若保留现状，trait 文档必须写明"返回的是装载视图，不是文件原文"，并让 UEFI 侧执行同一修剪以消除腿间差异。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若设计意图是"FileLoader 返回的就是可装载视图，UEFI 腿的尾部字节无害"，那么腿间差异仍然存在，只是被接受；此时文档应改写为"U-Boot 腿的 `read` 额外做装载视图修剪"，而不是"只处理字节来源"。
- 结论：接受（至少改文档；代码上移可选，代价是 `loader.rs` 一处新增与 `opensbi_helpers.rs` 一处删除）。

## FINDING DESIGN-D2-06（P2）

- 位置：`os/libs/minix-boot/src/kernel_info.rs:631-648`（空的 getter 段落）、`:1092-1121`（测试）
- 严重度：P2
- 分类：`[DEL]`
- 问题：`KernelInfo` 里保留了一段"Getter methods (preferred API over direct field access)"的注释，但下面没有任何方法；`test_getters_return_field_values` 是 `assert_eq!(info.kern_virt_base, info.kern_virt_base)` 这一类恒真断言，注释写"Getter methods should return the same values as direct field access"，而 getter 不存在。测试名与注释声称验证"getter 与字段一致"，但等式两侧都不在场。
- 影响：一条永远不可能失败的测试被当成覆盖证据；读者会以为 `KernelInfo` 有推荐的 getter 接口。这是测试审查口径里的"虚构测试"（对账的双方不在场），会稀释真正的不变量覆盖。删除这两个残骸不改变任何行为。
- 证据或推理：`kernel_info.rs:631-648` 的注释后直接是 `impl` 的闭合花括号；`:1092-1121` 的每条断言左右操作数相同；全文件没有 `pub fn <field>` 形式的 getter。
- 最小修复：删除空注释段与恒真测试；若确实需要 getter，生成它们并在测试里比较 getter 与字段（两侧都来自同一个 `make_valid_info()` 不够，至少要有一个字段被改写后验证 getter 读到新值）。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若 getter 是被 `#[cfg]` 或宏生成的，那应该能在源码里看到生成点；当前文件没有。若测试的本意是"结构体可复制、字段可直接读"，那它应改名为 `kernel_info_is_copy_and_readable`，而不是声称验证 getter。
- 结论：接受（按虚构测试处理；代价为零，收益是恢复测试台账的可信度）。

## FINDING DESIGN-D2-07（P3）

- 位置：`os/kernel/src/lib.rs:427-431`（`prepare_kernel_image_v1` 返回 `(u64, u64)`）、`:510-511`、`:523-531`、`:546-582`（`jump_kernel_image_v1(entry: u64, arena_pa: u64)`）
- 严重度：P3
- 分类：`[MODERN]`
- 问题：`prepare_kernel_image_v1` 返回两个裸 `u64`，`jump_kernel_image_v1` 收两个裸 `u64`。第一个是虚拟入口（`report.kern_virt_base`，语义是 `VirBytes`），第二个是物理 arena（语义是 `PhysBytes`）；交换后仍然编译。
- 影响：交换的后果是跳到 arena 的物理地址。真机现场是取指故障，与"入口地址写错"属于同一类最难归因的启动故障。类型系统已经有 `VirBytes`/`PhysBytes`，这一处是少数没有用上的地方（`shim_windows: &[(u64, u64)]` 是文档允许的固件边界例外，本处不是）。
- 证据或推理：上述行号；`entry` 的来源是 `core::hint::black_box(report.kern_virt_base.0)`（`:510`）。
- 最小修复：返回一个私有 `PreparedJump { entry: VirBytes, arena_pa: PhysBytes }`，`jump_kernel_image_v1` 收该结构；三个架构的跳转臂各自取字段。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若作者认为这对值只在同一函数体内相邻使用、不会抄错，那么同样的理由也适用于 `PlacementPool`；标准应一致。
- 结论：接受。

## FINDING DESIGN-D2-08（P3）

- 位置：`os/libs/minix-boot/src/boot_storage.rs:399-420`（`BootReport` 的 `memmap_sources`/`startup_objects`）；`os/boot-shim/src/uboot_main.rs:484,490`
- 严重度：P3
- 分类：`[DEL]`
- 问题：这两个切片不是"报告"的通用字段，而是"U-Boot 腿的序列化输入"：在 UEFI 腿它们恒为空（记录由分类器直写 wire），在两个直核腿根本不使用 `BootReport`，全仓只有 U-Boot 主函数读它们。`BootReport` 同时被传给内核入口 `enter_kernel_image_v1`（`kernel/src/lib.rs:429,525`），而内核入口从不读这两个字段。
- 影响：`BootReport` 的"空切片"同时表示"没有记录"和"记录在 wire"两种语义，区分靠腿的静态约定（文档已声明）；一个新增的 `report` 消费者在 UEFI 腿会读到空列表而不是 wire 记录，且没有类型提示。它还让内核入口的参数类型携带了两个内核不关心的腿细节。
- 证据或推理：`boot_storage.rs:406-413` 的字段文档；`uefi_helpers.rs:1185-1187`（UEFI 腿的 storage 记录为空）；`uboot_main.rs:484,490`（唯一读取点）；全仓 grep `report.memmap_sources`/`report.startup_objects` 只命中这两行。
- 最小修复：把两个切片从 `BootReport` 移到独立的 `BootRecords<'a>`（或 `BootPrepareResult` 的一个新字段），`prepare_boot` 同时返回两者；`BootReport` 只保留内核入口真正读取的五类载荷与六个标量。这样"记录在 wire"这条腿约定不再污染内核入口的类型。
- 置信度：中高（不改变任何行为；需要确认 `prepare_boot` 的借用拆分能通过编译——这一点在设计上可行，因为报告与记录是同一 `&mut storage` 派生出的两组共享切片，可在 `prepare_boot` 内部一次构造后分别返回）
- 是否需源码核实：是（需要一次编译验证）
- 若我错了：若作者希望保留"一个报告携带全部引导语义"的统一形状以便将来有其他腿复用，那应把两个字段变成带来源标记的类型（例如 `RecordsView::InReport(&[..]) | RecordsView::OnWire`），而不是用空切片承担两种含义。
- 结论：接受（低成本类型收窄；若保留现状，文档必须写明"这两个字段只有 U-Boot 腿有意义"）。

## FINDING DESIGN-D2-09（P3）

- 位置：`os/libs/minix-boot/src/handoff_v1.rs:408-411`（`WIRE_STARTUP_CAP = 20` 与分区容量 1440 B）；`os/libs/minix-boot/src/kernel_info.rs:76-84`（字段文档）、`:516-532`（唯一读取点）；四个写入点（覆盖五条腿）：`uefi_helpers.rs:1144-1179`、`opensbi_helpers.rs:432-444`、`bootface.rs:376-403`、`bootface_a64.rs:348-373`
- 严重度：P3
- 分类：`[DEL]`
- 问题：startup-object 记录在 wire 上独占一个 20 槽分区（1440 B），在 `BootStorage` 里另占 20 槽（另 1440 B，在 UEFI 腿还闲置），四个写入点（UEFI 两条链共用一个）各写一遍；接收侧唯一读者是 strict 门里的一条 `any()`——"至少一条记录覆盖内核镜像跨度"。文档自己说"启动期之后没有读者"。
- 影响：为一条可以等价表达的断言，付出了约 2.9 KiB 内存、四个写入点与三处重复的追加逻辑（`BootStorage::push_startup` 方法、UEFI 腿的局部闭包、U-Boot 腿的 `push_startup_record` 辅助函数）。它还让 `validate_requiring_sources` 的失败消息把"生产者漏写 provenance"与"内核镜像没有记录"两类问题混在一条规则里。
- 证据或推理：上述行号；全仓 `startup_objects` 的非写入读取只有 `kernel_info.rs:520-532`；`startup_record()` 的用途只有分区写入与这条检查。
- 最小修复：二选一：(a) 删除该分区与字段，strict 门改为检查"内核物理跨度落在某条 `memmap` 区间或某条 `reserved_regions` 区间内"（五条腿上都成立：直核腿内核在 free 表内，UEFI 腿在保留表内）；(b) 保留字段但压缩为一条"内核已覆盖"的记录，wire 分区缩到 1 槽，`BootStorage` 的 20 槽随之删除。
- 置信度：中高
- 是否需源码核实：是（确认方案 (a) 的区间归属在五条腿上都成立）
- 若我错了：若 startup 记录的既定用途是"未来给 VM 或诊断提供引导对象的身份账本"，那么它不是死代码而是预留；文档应把这条未来用途写成明确的需求来源，而不是只说"保留身份"。在没有读者的当下，按最小化原则应删除。
- 结论：需用户裁决（这是一笔产品级的证据投资；删除的代价是 wire 版本升级与文档 §3.4.1/§4.5 的改写，收益约 2.8 KiB 与一处生成器收敛）。

## FINDING DESIGN-D2-10（P3）

- 位置：`os/libs/minix-boot/src/memmap_source.rs:591-617`（`InputObjectId` 三变体）；`os/libs/minix-boot/src/kernel_info.rs:175-184`（`authority` 字段）；`os/libs/minix-boot/src/handoff_v1_read.rs:162-166`（只接受 0/1）
- 严重度：P3
- 分类：`[MODERN]`
- 问题：`InputObjectId` 同时承担两个角色：记录来源标签（三值，`startup_record()` 必须用 `StartupObjects`，见 `memmap_source.rs:766-780`）与 `KernelInfo.authority` 域（生产只有两个值，wire 解码拒绝第三个）。因此"authority = 启动对象"在类型上可表示、在协议上被拒、在任何生产者里都不存在。文档 §3.4.1 专门用一句话提醒读者不要把记录身份当成 authority。
- 影响：类型没有把"不可能"编码进去，只能靠文档与解码器拦住。手工构造或未来新增的构造点可以写出一个"声明权威是启动对象"的内核信息；strict 门只在恰好携带相应记录时放行。
- 证据或推理：上述行号；`InputObjectId::from_tag` 接受 2（`memmap_source.rs:609-616`），而 `decode_envelope` 只把 0/1 映射成 authority（`handoff_v1_read.rs:162-166`）。
- 最小修复：为 `KernelInfo` 引入两变体 `RamAuthority { DtbBootInput, UefiMemoryMap }`，wire 解码时从 0/1 映射；记录层继续用 `InputObjectId`。构造点与 strict 门各改一行。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若未来的第三种 authority 是计划内的（例如 ACPI），那么应在文档里给出它成为 authority 的条件，并让解码器同时扩展；当前文档明确说它不是活通道，所以收窄是忠实的表达。
- 结论：接受。

## FINDING DESIGN-D2-11（P3）

- 位置：`os/arch/src/arch/pt_alloc.rs:165-176`（`free_pt_page` 未注册时静默 no-op）；三个架构的同形门：`os/arch/src/x86_64/paging.rs:776`、`os/arch/src/arm64/paging.rs:751`、`os/arch/src/riscv64/paging.rs:790`
- 严重度：P3
- 分类：`[MODERN]`
- 问题：`alloc_pt_page` 未注册时 fail-closed（断言，`pt_alloc.rs:124-141`），`free_pt_page` 未注册时静默走 `uninit_free` 空操作（`:71-74,171-176`）。三个架构的 `destroy` 路径各自复制"先查 `is_free_registered()` 再决定是否走完整回收"的分支。
- 影响：一个漏掉检查的新 `destroy` 路径不会失败，只会在启动期静默泄漏中间页表页；错误面比分配侧弱一档，且防护靠三处复制。当前所有调用点都做了检查，所以是防未来回归。
- 证据或推理：上述行号；`is_free_registered()` 的结果在 `free_pt_page` 里只被读取、不参与任何断言。
- 最小修复：让 `free_pt_page` 在未注册时断言（与分配侧对称），把 `is_free_registered()` 分支从三个 `destroy` 实现里删掉；或让它返回 `bool` 由调用方决定。前者的代价是确认启动期没有任何路径会对引导页表调用 `destroy`（当前 x86_64 的单元测试会调用，需要为测试注册归还钩子）。
- 置信度：中
- 是否需源码核实：是（需确认引导期 `destroy` 调用面）
- 若我错了：若保留 no-op 是为了让"销毁一个没有分配域的页表"成为合法操作（例如测试内核），那应在 `destroy` 的文档里写明"没有归还钩子时只清根"的泄漏语义，而不是只在 `free_pt_page` 的注释里写。
- 结论：拒绝（本期不动：改动会牵连测试内核的销毁路径，收益仅是防未来回归；保留现状但补一条文档说明）。

## FINDING DESIGN-D2-12（P3）

- 位置：`os/libs/minix-boot/src/handoff_v1_write.rs:158-169`（`write_envelope` 的注释与实现）；`os/libs/minix-boot/src/kernel_info.rs:537-547`（`source_audit`）
- 严重度：P3
- 分类：`[DEL]`
- 问题：`write_envelope(&KernelInfo)` 的注释说"receiver-side `KernelInfo` form; direct-core legs still use it"，但两个直核腿从不构造 `Serializer`（它们直接调用 `arch_boot`，见 `bootface.rs:427-442`），全仓只有测试调用它（`ebs_pair.rs:427,854`、`vm_handoff.rs:1057`、`handoff_v1_write.rs` 与 `handoff_v1_read.rs` 的测试）。`KernelInfo::source_audit()` 也只有一个测试调用者。
- 影响：文档/注释声称了一个不存在的生产消费者，读者会以为直核腿也走 wire；两个 API 的实际可用面只有测试，属于"为了测试保留的生产接口"。
- 证据或推理：上述行号；全仓 grep `write_envelope(` 与 `source_audit` 的命中。
- 最小修复：改写 `write_envelope` 的注释为"测试载体的构造便捷方法"并加 `#[cfg(any(test, feature = "test-support"))]`（如果测试跨 crate 使用），或删除它并让测试改用 `write_envelope_scalars`；`source_audit()` 同理标注为诊断 API 或删除。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若直核腿计划在未来接入 v1 序列化（例如统一三条腿的构造），那应把这个计划写进设计文档，而不是让注释先行；当前源码里没有这条路径。
- 结论：接受（改注释与可见性；代价近乎为零）。

## FINDING DESIGN-D2-13（P3）

- 位置：`os/libs/minix-boot/src/boot_storage.rs:366-380`（`report()` 的派生字段）；`os/boot-shim/src/lib.rs:124-126`（`build_kernel_info` 的派生字段）；`os/libs/minix-boot/src/kernel_info.rs:449-470`（strict 门的再派生）；`os/libs/minix-boot/src/handoff_v1_write.rs:147-152`（序列化器的 debug 断言）
- 严重度：P3
- 分类：`[MODERN]`
- 问题：三个派生标量的公式在四处独立出现：`user_sp = minix_types::USER_STACK_TOP`、`syscall_entry = kern_virt_base`、`kern_stack_top = kern_virt_base + kern_size`。两个生产者（`BootStorage::report` 与 `build_kernel_info`）各写一遍，strict 门再独立推一遍做比对，序列化器又用 `debug_assert_eq!` 推第三遍。
- 影响：派生契约一旦变化（例如栈顶从"镜像尾"改为"镜像内某个符号"，或系统调用入口改为真实地址，见 D2-03），需要四处同时改；两个生产者之间的漂移不会在编译期或单测里暴露，只有接收侧 strict 门在运行时才会抓到（而且 strict 门本身也是这些公式的一份副本）。
- 证据或推理：上述行号。
- 最小修复：在 `minix-boot` 提供一个 `DerivedScalars::from_layout(virt_base, size)`，两个生产者调用它；strict 门保留独立推演（防御性冗余是它的职责），序列化器的 debug 断言可以删除或改为调用同一函数。
- 置信度：中高
- 是否需源码核实：否（已核实）
- 若我错了：若作者认为"两处生产者的重复是刻意的冗余检查"，那它挡不住漂移——因为两个值会一起变错，strict 门也会跟着一起变错；冗余只有在独立来源时才有价值。这里两个生产者的来源相同（字段 + 常量），所以抽取是安全的。
- 结论：接受（小改；若与 D2-03 一起处理，可一次性收敛）。

## ARCH-PROPOSAL

### ARCH-PROPOSAL-01：aarch64 的引导形态判别从寄存器零值哨兵改为链接期分叉

- **扩张内容（改变 boot 是什么）**：`kernel-image` 的 aarch64 入口目前用 `x0 == 0` 选择"直核自引导"还是"v1 接收"（`os/kernel-image/src/main.rs:377-382`；文档 §4.8 明确它依赖"QEMU 观察到不设 `x0` 参数——非规范保证"）。提议把 boot 形态从运行期寄存器判别改成**链接期分叉**：像 riscv64 的 `fw-riscv64-uboot` 那样，为 aarch64 构建两个接收面变体（直核 `-kernel` 变体与 UEFI v1 变体），各自链接不同的入口符号；UEFI 链的 `kernel.elf` 与直核测试镜像不再共用同一个入口契约。若必须保留单镜像，则退而求其次在入口约定里增加一个显式的形态字（例如 `x1` 携带 magic + form），并把 `x0 == 0` 降级为兼容回退。
- **三处一致计划**：
  1. 文档：§3.7 的启动路径计数表与 §4.8 的 aarch64 分道段改为"形态由构建产物决定"；§3.1.1 的接线状态表补上两个 aarch64 变体。
  2. 设计契约：`kernel-image` 的 aarch64 入口 ABI（`_start` 与 `rust_image_main` 的签名）与链接脚本；`xtask image` 的 aarch64 产物清单。
  3. 代码：`os/kernel-image/src/main.rs` 的分道、`bootface_a64` 的触发点、`os/qemu-tests/test-smp-aps-aarch64.sh` 的镜像选择、UEFI 门脚本的产物路径。
- **不做会坏在哪**：当前直核判别的唯一依据是寄存器的零值，而零值不是任何规范承诺。任何在 `x0` 放非零值的加载器（aarch64 U-Boot、真板固件、QEMU 未来版本改变默认值）都会把直核启动送进 v1 解码路径，症状是 `kernel-image: handoff v1 envelope invalid` panic，而不是"入口形态没被识别"；panic 发生在 `early_console` 初始化之后，定位成本高。文档自己写了"真板 `-kernel` 行为以板级文档为准"，等于承认这条通道只在 QEMU 上被验证过。
- **代价**：新增一个构建变体与产物清单（与 riscv64 已有的两变体模式同构，不引入新机制）；改一个入口签名、一个链接脚本选择点、两个门脚本与三处文档。若采用 `x1` magic 的轻量方案，则只改 `main.rs` 与文档，但"加载器愿意设置 `x1`"仍是新的外部约定，收益小于链接期分叉。
- **裁决**：需用户裁决。若 aarch64 直核面只是 QEMU 测试通道且作者接受"x0=0"为测试契约，则拒绝本提案并把该契约写进 §3.1.1；若它要成为真板可用的引导形态，建议采纳链接期分叉。

## VERDICT

- **总体判断**：第 3、4 章的设计主张与源码在主干上是一致的：`BootShim` trait 已删、`FileLoader` 是真实的测试接缝、`HugePages` 没有把架构位带进语义层、`pt_alloc` 的写一次与 `map_dirty` 的半提交屏障都按文档落地、`KernelInfo` 的深拷与 `validate_requiring_sources` 的 strict 门可核。没有发现 P0 级设计错误，也没有发现文档把不存在的机制说成已实现这一类系统性问题。
- **必须处理（P2，接受）**：D2-01（startup 提交是死状态）、D2-02（五族半提交屏障缺失，setter 可暴露占位记录）、D2-04（四个松散标量中两个 `VirBytes` 可互换）、D2-05（`FileLoader` 不是纯字节来源，腿间 `BootModule.len` 有差异）、D2-06（恒真测试与空 getter 段落）。
- **需用户裁决**：D2-03（`syscall_entry` 是错误值，建议删除，代价一次 wire 版本升级）、D2-09（startup 记录分区只服务一条断言）、ARCH-PROPOSAL-01（aarch64 形态判别）。
- **建议本期拒绝**：D2-11（`free_pt_page` 静默 no-op 与三个架构的 destroy 门；改动会牵连测试内核，收益只是防未来回归）。
- **单实现 trait 复核**：boot 路径上不存在单实现 trait。`FileLoader` 3 个实现（2 生产 + 1 mock）、`HugePages`/`Paging`/`BootArch`/`HigherHalf`/`DirectMapArch`/`BootDmInstaller` 均有多架构实现或 mock 实现；`BootArch` 是类型级分派而非依赖注入。因此"删一个 trait"在本篇没有可执行项。
- **同 agent 验证局限声明**：本报告由单个会话完成，证据以源码行号与 grep 结果给出；未运行 `cargo test`/`cargo clippy`（只读约束）。`D2-08` 与 `D2-09` 的最小修复含"需编译或区间归属验证"的标注，属于未在本会话执行的部分；其余结论均以源码行可直接复核。
- **一致性自评**：文档主张与源码事实的逐项对账中，DIFF-TABLE 七行均给了两侧行号；FINDING 证据段的行号来自实际 `grep`/`sed`。按本项目 VERIFY-CHECK 口径，本报告没有独立验证者，故不宣称 CONVERGED；供下一轮交叉复核。

- 本次扫描：13 条 FINDING，1 条 ARCH-PROPOSAL。
