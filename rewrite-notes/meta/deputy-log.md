# Deputy Log — 2026-11-22

冷启动 me-agent 依据 `prompt/me.md` 代裁。只读源码，不读其他项目文档；未修改任何代码。
判决分两类：**决策**（替作者判，待 FREEZE 前追认）与**阻断**（正式案件卷，等作者亲裁）。
追认通过后转入 `DECISIONS.md`；被推翻则按各案「回退动作」执行。

---

## CASE D01-DESIGN-01 — 决策：驳回（按非问题关闭）

**决策**：不采纳「分类器只落 storage，EBS 成功后由 report 一次编码 wire」。保留现行单次分类、同一次 `commit` 同时写 wire 与语义表的形状。

**依据条款**（`prompt/me.md`）：
- 「我先怀疑方案本身：这个问题真的存在吗？」——评审前提「两表示无对账」与源码不符。
- 「深模块优于浅模块」「不接受只为转发调用、制造间接层」。
- 「我重视事实链，而不是听起来合理的解释」；「能测量就测量」。

**关键源码证据**：
- `os/boot-shim/src/ebs_map.rs:485-496`：`classify_into_handoff` 文档写明「the same entry value is written to the wire and to these arrays, so the sender's report and the receiver's decode read one classification, not two」。这是单次判定、双目的地，不是两次独立分类。
- `os/boot-shim/src/ebs_map.rs:792-870`：`commit` 的闭包对同一 `region` 先 `serializer.push_memmap(*region)` / `push_reserved(*region)`，随后 `memmap_out[i] = *region` / `reserved_out[i] = *region`；值来自同一处，没有第二条推导路径。
- `os/boot-shim/src/ebs_map.rs:1526-1531`：测试直接断言调用方语义表收到的条目与 wire 字节相同（`(memmap[0].base.0, memmap[0].len) == wire`），并注明「the wire is their encoding」。
- `os/boot-shim/src/ebs_pair.rs:510-535`：另一测试解码 wire 并跑 `audit_source_coverage`，覆盖关系以 wire 为准。
- 两个表示各有生产消费者：wire 被内核 `arch_boot_resume_v1` 解码（`os/kernel/src/lib.rs:653-741`）；storage 被 shim 自己在 EBS 后的断言消费（`os/boot-shim/src/uefi_helpers.rs:1232-1247` 的 `assert_bootstrap_outside_memmap`、`cross_validate_config_dtb`），并作为直接跳转腿的 `report` 传入 `enter_kernel_image_v1`。
- 候选方案的隐藏成本：源记录（`SourceRecord`，72B）当前只进 wire；`BootStorage` 的源记录容量 `DTB_SOURCE_CAP = 40`（`os/libs/minix-boot/src/memmap_source.rs:223-233`），而 wire 侧 `WIRE_SOURCES_CAP = 296`（`os/libs/minix-boot/src/handoff_v1.rs:359-364`）。若改成先落 storage 再编码 wire，必须给 `BootStorage` 增加 296×72 ≈ 21 KiB 静态容量，或者 EBS 后重新遍历描述符再判定一次；后者正是现行注释明确避免的「needless post-EBS failure path」（`os/boot-shim/src/uefi_helpers.rs:1232-1236`）。

**反方最强论据**：双写毕竟给了同一事实两个物理副本，若将来有人修改 `commit` 只改一边，编译器不会报错；纯从「单一真源」原则看，先落语义表再从语义表编码确实更干净。测试只覆盖了 `memmap`/`reserved` 的等值，没有覆盖每个源记录的字段等值。

**回退动作**：若作者仍要消除双写，最小可行边界是只让 `SourceRecord` 先落 `BootStorage`（容量从 40 扩到 296，增加约 18 KiB `.bss`），并在 EBS 后用已证明容量的无失败编码写 wire；不得在 EBS 后重新解析描述符。该改动涉及 shim 静态内存预算，需在真实 UEFI 与 U-Boot 两个腿上重新跑启动门。

---

## CASE D01-DESIGN-03 — 决策：驳回（按非问题关闭）

**决策**：不抽 `publish_landed`。两条落库路径已经在发布点汇合于 `store_kernel_info`，不存在「两份发布实现」。

**依据条款**：
- 「我先怀疑方案本身」；「不要为了让代码看起来模块化而拆散本来完整、容易理解的机制」。
- 「抽象语义，不抽象机制」的边界条款：解码到 `.bss` 与从任意指针拷贝到 `.bss` 是两个不同动作，不是同一动作的两份实现。

**关键源码证据**：
- `os/kernel/src/lib.rs:653-741`：v1 接收侧把每个 payload 直接解码进内核 `.bss` 的 store（`decode_memmap(arena, &scalars, memmap_store)` 等），随后构造 `KernelInfo`，再走 `resume_after_v1_decode`。
- `os/kernel/src/lib.rs:299-322`：`resume_additive` 内调用 `store_kernel_info(kernel_info)`；v1 接收侧与直核侧在这一点汇合。
- `os/kernel/src/lib.rs:1553-1696`：`land_slice` 与 `store_kernel_info` 是唯一的「从外部指针落进 `.bss` 并发布全局槽」实现；`1571-1573` 的注释写明生产只有两个调用点。
- `os/kernel/src/lib.rs:723-740`：v1 侧构造六个 `from_raw_parts` 指针以形成 `KernelInfo`；这与 `store_kernel_info:1690-1694` 的指针重建确有重复，但重复的是约六行地址推导，而不是整段落库逻辑。

**反方最强论据**：v1 侧解码后构造 `KernelInfo`，`store_kernel_info` 又重建一遍同样的指针；`land_slice` 的 same-address 跳过分支只为吸收 v1 路径而存在。把「发布」抽出来可以让两个路径各自只做自己的动作，并删掉那个指针相等守卫。

**回退动作**：若作者认可上述反方，最小改动是只提取「把已落在 store 的长度发布为 `KERNEL_INFO`」这一步（一个接收六个长度与三个标量的函数），`store_kernel_info` = 拷贝 + 调用它，v1 = 解码 + 调用它；不得把解码逻辑并入发布函数，也不得在 v1 路径引入第二次拷贝。

---

## CASE D01-LOWRISK — 逐项决策

### DESIGN-02 — 决策：批准删除公开壳

**决策**：批准把 `prepare_kernel_image_v1` / `jump_kernel_image_v1` 收敛进 `enter_kernel_image_v1`，并删除 `PreparedJump` 这一公开搬运类型；保留 `VirBytes` / `PhysBytes` 两个带类型局部量。若实施者认为拆分仍有阅读价值，可保留为私有函数，但不再作为跨 crate API。

**依据条款**：「最好的代码是没写的代码」「不要提前设计没有调用需求……的未来模式」；类型安全只在提供真实编译期保证时才有价值。

**关键源码证据**：
- `os/kernel/src/lib.rs:399-404`：`PreparedJump` 的两个字段分别是 `VirBytes` 与 `PhysBytes`；注释描述的历史事故是「两个相邻的裸 `u64`」可互换。现在两个字段类型不同，位置互换已不能编译，结构体不再提供超出类型的额外保证。
- `os/kernel/src/lib.rs:427-433`、`525-533`、`548`：两个函数只被 `enter_kernel_image_v1` 调用。
- 全仓 grep `prepare_kernel_image_v1|jump_kernel_image_v1|PreparedJump` 在 `os/kernel/src/lib.rs` 之外无命中：boot-shim 只调用 `enter_kernel_image_v1`（`os/boot-shim/src/uefi_helpers.rs:1271`、`os/boot-shim/src/uboot_main.rs:529`）。

**反方最强论据**：`jump_kernel_image_v1` 的拆分让各架构的跳转寄存器约定集中在一个函数里，也让 shim 将来能在建根之后、跳转之前打印里程碑（注释中的 TODO-09）。删除后若真需要该里程碑，要重新拆一次。

**回退动作**：保留 `PreparedJump` 为 `enter_kernel_image_v1` 内的私有载体，仅把两个函数改为私有；若后续出现真实的外部调用点，再恢复公开壳。

### DESIGN-04 — 决策：批准改栈上 `ClaimSet`

**决策**：批准把 `DTB_RECORDS`、`DTB_RECORDS_N`、`CLAIMS`、`CLAIMS_N`、`DTB_RAW` 五个静态收成一个栈上对象，由 `capture` 拥有，`records()` 借用它，`check_current` 接收它。

**依据条款**：「看到谁创建、谁修改、谁释放，应该能理解资源生命周期」；「不要提前设计……」；单线程启动阶段，栈所有权比进程级静态更清楚。

**关键源码证据**：
- `os/boot-shim/src/resv_claims.rs:261-283`：五个 `BootCell` 静态及各自的写一次契约。
- `os/boot-shim/src/resv_claims.rs:292-358`：`capture` 清零并写静态，`records()` 从静态拼切片，`check_current` 读静态。
- `os/boot-shim/src/ebs_pair.rs:194-201`：三个调用点全部在 `pair_and_classify` 的同一次循环迭代内，先 `capture`，再 `push_dtb_sources(records())`，再 `check_current`；没有任何跨迭代或跨函数的所有权需求。
- 容量可计量：`MAX_CLAIMS = 64`（`resv_claims.rs:27`），`DTB_SOURCE_CAP = 40`，`SourceRecord` 72B → 最坏约 40×72 + 64×24 + 16 ≈ 4.4 KiB 栈。U-Boot shim 的 `.stack` 为 64 KiB（`uboot_main.rs` 注释与链接脚本），QEMU 测试载体亦为 64 KiB（`test-rt-birth-riscv64/src/main.rs:216-220`）。

**反方最强论据**：5 个静态换成栈对象后，UEFI 固件栈的余量不在源码里，无法从本仓证明；若固件栈很小，4.4 KiB 的局部量会变成真实风险。静态的写一次契约已有注释和 `BootCell` 封装，不是隐蔽的 `static mut`。

**回退动作**：若 UEFI 真机/固件栈余量无法确认，保留静态，但把五个 `BootCell` 合并为一个 `ClaimSet` 静态（仍单线程），至少消掉分散的写一次契约；这是比现状小、比栈方案保守的一步。

### DESIGN-05 — 决策：批准删除 `BootScalars` 与两个构造器

**决策**：删除 `BootScalars`，让 `boot_validate_and_prepare` 直接接收 `kern_virt_base: VirBytes`、`kern_phys_base: PhysBytes`、`kern_size: u64` 三个不同类型的参数；三个调用点直接传字段。

**依据条款**：「错误的前提必须撤回，不要为了维护旧结论而修改措辞」；「不接受只为转发调用、制造间接层」；「最好的代码是没写的代码」。

**关键源码证据**：
- `os/kernel/src/lib.rs:840-885`：`BootScalars` 的注释说它防止「两个 `VirBytes` 参数互换」；但当前结构只有三个字段，类型分别是 `VirBytes`、`PhysBytes`、`u64`，两两不同。原先的第二个 `VirBytes`（`kern_stack_top`）已在 wire v10（D-062 / I-31）移除，注释中的事故前提不再成立。
- `os/kernel/src/lib.rs:307`、`446`、`980`：三个调用点。删除后每个调用点写三个字段名，仍然不可能静默互换。
- `os/kernel/src/lib.rs:888-890`：`boot_validate_and_prepare` 只消费这三个字段。

**反方最强论据**：结构体让「名字 + 值」在同一行出现，比函数实参更主动；两个构造器把 `KernelInfo` 与 `BootReport` 两种来源的字段抽取集中在一处，删掉后三个调用点各写一遍抽取代码。

**回退动作**：保留 `BootScalars` 但删除两个构造器、改为 `impl From<&KernelInfo>` / `impl From<&BootReport>`；若作者更看重调用点的显式字段名，则按本决策删除。

### DESIGN-06 — 决策：批准改名；批准删除 `validate_at`

**决策**：把 `requires_startup_kernel_record` 改名为表达「内核镜像跨度覆盖」的名字（如 `requires_kernel_span_coverage`）；删除 `validate_at`，三个公开校验入口直接调用 `validate_impl_inner`。

**依据条款**：「术语必须稳定」；「不接受只为转发调用……增加的抽象」；错误的前提必须撤回。

**关键源码证据**：
- `os/libs/minix-boot/src/kernel_info.rs:171-173`：函数体只判断 `Strict`；名字里的 `startup_kernel_record` 来自已删除的 startup-object 类（`kernel_info.rs:499-523` 的检查实际是内核镜像跨度落在 `memmap`/`reserved_regions` 内）。
- `os/libs/minix-boot/src/kernel_info.rs:217-250`：`validate`、`validate_before_tables`、`validate_requiring_sources` 三个入口分别调用 `validate_at`；`validate_at:222-224` 的函数体只有一行 `self.validate_impl_inner(&level)`，是纯转发层。
- `os/libs/minix-boot/src/kernel_info.rs:252`：真正的校验体是 `validate_impl_inner`。

**反方最强论据**：`validate_at` 是三个入口的共同名字，保留它让「所有校验都经过同一个入口」在调用图上可见；删除后三个入口各自直呼 `validate_impl_inner`，入口的集中性从代码上消失。

**回退动作**：若作者要保留调用图上的单一入口，删除本决策的删除部分，仅执行改名；改名本身无争议。

### DESIGN-09 — 决策：批准每槽一个 `AtomicUsize`

**决策**：把分配与释放两个槽各收成一个 `AtomicUsize`（0 表示未注册，非 0 为函数指针），用 `compare_exchange` 实现写一次，用 Release/Acquire 发布；删除两个 `UnsafeCell` 包装、两个 `Sync` 实现和三个布尔标志。

**依据条款**：「unsafe 范围尽量小，安全前提必须具体」；「最好的代码是没写的代码」；kernel 是 SMP + BKL 执行模型，写一次后只读。

**关键源码证据**：
- `os/arch/src/arch/pt_alloc.rs:61-69`：`PT_ALLOC`、`PT_REGISTERED`、`PT_CLAIMED` 三个静态；`89-91` 与 `98-148` 的注册/读取/双重注册断言。
- `os/arch/src/arch/pt_alloc.rs:73-80`、`171-183`：释放侧同构的三个静态与 `free_pt_page`。
- 合计六个静态，每个槽的「数据 + 已注册 + 已认领」三份状态可用一个原子量同时表达；`is_registered()` 的 Relaxed 读与 `alloc_pt_page()` 的 Acquire 读可以合并为一次 Acquire 读。

**反方最强论据**：函数指针存进 `AtomicUsize` 需要 `as usize` / 反向转换，Rust 不保证函数指针与整数可逆；当前 `UnsafeCell<fn>` 保持类型，`unsafe` 块各自有具体前提。把指针变整数会引入一个新的、无法用类型表达的假设。

**回退动作**：若作者不接受函数指针与整数互转，改用 `AtomicPtr<()>` 加 `transmute` 到 `fn`，同样能把三静态收成一静态；或保留 `UnsafeCell`，只把 `REGISTERED` 与 `CLAIMED` 合并为一个三态原子（未认领/已认领未发布/已发布），也能少一个静态。

### DESIGN-10 — 决策：批准 `shim_windows` 改 `&[PhysRange]`

**决策**：把 `enter_kernel_image_v1` / `prepare_kernel_image_v1` 的 `shim_windows: &[(u64, u64)]` 改为 `&[PhysRange]`，调用点在构造窗口时用 `PhysRange::new(...).expect(...)`。

**依据条款**：「类型可以编码不变量，但前提是它真的提供了编译期保证」；`PhysRange` 的构造器拒绝反向区间，确实把 `start <= end` 变成可检查的边界。

**关键源码证据**：
- `os/kernel/src/lib.rs:427-433`、`525-533`：公开边界是裸 `(u64, u64)` 对。
- `os/kernel/src/lib.rs:1173-1210`：`BootWindows::push` 已经断言 `start < end` 并构造 `PhysRange`；内部表示本来就是 `[PhysRange; MAX_BOOT_WINDOWS]`，`CheckedWindows(&[PhysRange])` 也是 `PhysRange`。
- `os/boot-shim/src/uefi_helpers.rs:1255-1272` 与 `os/boot-shim/src/uboot_main.rs:528-529`：调用点共两处，都是已知有效的半开区间；改成 `PhysRange` 后反向区间在 shim 侧就被拒绝，而不是等到 `BootWindows::push`。

**反方最强论据**：`PhysRange` 的构造器返回 `Option`，调用点必须 `expect`，把一个「不可能失败」的路径写成可能 panic 的表达式；当前 `BootWindows::push` 已经有同样的断言，边界类型化只是把断言前移，收益有限。

**回退动作**：若作者认为收益不足，保留 `(u64, u64)` 边界，但在 `enter_kernel_image_v1` 入口加一条断言 `start <= end`，使失败点从内部 `push` 前移到 API 边界。

### DESIGN-11 — 阻断（证据不在我可读范围）

**阻断理由**：该项的内容是「已在文档修正，检查是否可关闭」，其判据在文档文本里。我的章程只允许读源码与锚点，不允许读其他项目文档；用源码无法证明一个文档修正是否落地。按「证据不足不硬判」阻断。

**作者需要做的**：在文档侧运行出版检查与该篇的引用对账，确认修正点存在；若确认，直接关闭该项。回退动作：无代码动作。

---

## CASE D01-DESIGN-07 — 阻断（正式案件卷）

**背景**：评审认为同一「物理区间」有三套表示，提议统一到 `PhysRange` 并删除另外两个，允许改动 wire 边界。

**源码证据**：
- `os/libs/minix-boot/src/kernel_info.rs:576-581`：`MemoryRegion { base: PhysBytes, len: usize }`，是 `KernelInfo` / `BootReport` / handoff v1 的 wire 与语义表条目，生产消费者是 VM 分类、DM 覆盖、启动断言。
- `os/kernel/src/vm_handoff.rs:304-311`：私有 `MemRange { base: u64, end: u64 }`，只服务 `FreeRangeSet` 的排序、合并与切割；数组用 `[MemRange { base: 0, end: 0 }; FREE_SET_CAP]` 常量初始化。
- `os/kernel/src/kpriv.rs:37-52`：`MemRange { base: u64, limit: u64 }`，字段是**闭区间上界 `limit`**，不是半开 `end`；`os/kernel/src/syscall.rs:2135-2148` 把用户内存直接 `copy_struct_from_user` 进这个结构，布局对应 C `struct minix_mem_range`（`minix3/minix/kernel/priv.h:56-57` 的 `s_mem_tab`）。
- `os/libs/minix-types/src/types/address.rs:196-230`：`PhysRange` 是半开 `[start, end)`，私有字段，构造器拒绝 `end < start`。

**候选对比**：
1. 统一到 `PhysRange`、删除两者、改 wire 到 v11。代价：`kpriv::MemRange` 是用户可见的 syscall 结构布局，替换即改变外部行为；`MemoryRegion` 出现在 v1 wire 与所有启动腿的语义表，替换需要协议版本升级与全部生产者/消费者同步；`FreeRangeSet` 的常量初始化与就地改写要改成 `Option<PhysRange>` 或额外 `EMPTY` 常量。
2. 只在内部自由集使用 `PhysRange`，保留 `MemoryRegion` 与 `kpriv::MemRange`。代价较小，但 `FreeRangeSet` 的 `insert`/`cut` 每次改写 `end` 都要重新验证不变量，收益仅是字段名统一。
3. 保持现状，只在注释里写清三者的层次（wire 条目 / 内部自由集 / C 特权 ABI）。代价：读者仍需分清三个名字。

**反方最强论据**（支持统一）：三个名字都表示「一段物理内存」，读者第一眼会以为可以互换；`PhysRange` 已经有构造器不变量，若所有边界都用它，反向区间与字段顺序错误可以从类型上消失。项目第一原则是「抽象语义」，统一区间语义似乎正是该原则的应用。

**为什么必须作者亲裁**：这不是等价实现取舍。候选 1 删除 `kpriv::MemRange` 会改变 syscall ABI，候选 1 的 wire v11 会改变启动协议；「删除整个抽象层/模块边界」与「改变核心语义」明确不在代裁范围。候选 2/3 的取舍则取决于作者对「三个概念是否应在教学上分开」的判断，属于教程叙事与术语决策。

**回退动作**：若作者决定不动，关闭该项并在三处定义旁补一行层次说明；若决定统一，先做候选 2（内部自由集），wire 与 ABI 的替换另开设计，不在 D01 内进行。

---

## CASE D01-DESIGN-08 — 阻断（正式案件卷）

**背景**：`SourceRecord` 有 11 个字段、wire 大小 72B；评审称生产审计只消费 4 个字段，提议收窄到 32B 并升级 wire v11。

**源码证据**：
- `os/libs/minix-boot/src/memmap_source.rs:510-545`：11 个字段：`id`、`kind`、`origin`、`range`、`input_object`、`local_source`、`snapshot_id`、`attribute`、`virtual_start`、`address_domain`、`raw_type`。
- `os/libs/minix-boot/src/handoff_v1.rs:367-384`：`WIRE_SOURCE_RECORD_SIZE = 72`，`WIRE_SOURCES_CAP = 296`，源记录分区位于 arena 末尾。
- 审计实际读取：`os/libs/minix-boot/src/memmap_source.rs:352-429` 的 `audit_source_coverage_for` / `records_cover` 使用 `address_domain`、`input_object`（经 `admitted`）、`kind`、`range`；`os/libs/minix-boot/src/kernel_info.rs:465-477` 另用 `input_object` 与 `authority` 对账。
- 生产侧写入：`os/boot-shim/src/ebs_map.rs:849-871` 写入 `attribute`、`virtual_start`、`raw_type`、`snapshot_id`、`origin`、`local_source`；`os/boot-shim/src/resv_claims.rs:90` 读 `address_domain`；DTB 侧 `os/kernel-image/src/bootface_common.rs:545`、`os/boot-shim/src/opensbi_helpers.rs:313` 也填源记录。

**候选对比**：
1. 收窄到 32B，删除 `attribute`、`virtual_start`、`raw_type`、`snapshot_id`、`origin`/`local_source` 中的大部分，wire 升 v11。收益：省 296×40 ≈ 11.8 KiB wire 空间（当前 arena 32 KiB，`WIRE_SOURCES_END` 之后仍有 3584B 余量）；代价：丢失固件原始类型、属性、虚拟地址与快照身份，这些是诊断「为什么这条区间被判成 reserved」的唯一现场数据。
2. 保持 72B，把「只消费 4 字段」视为现状而非缺陷：审计是当前唯一自动消费者，但记录层的设计目的就是保留固件事实，供启动失败后的取证。收益：不制造协议变更；代价：每个 arena 多占约 11.8 KiB（仍在 8 页预算内）。
3. 折中：保留字段，但把 wire 编码改成变长/压缩（例如空字段省略）。收益：省空间且不丢事实；代价：解码复杂度上升，且与固定偏移的布局门冲突。

**反方最强论据**（支持收窄）：未被任何生产代码读取的字段就是未使用的状态；项目原则「最好的代码是没写的代码」，且 wire 是跨镜像协议，越窄越难写错。当前没有容量压力不等于保留 40B/条是合理设计。

**为什么必须作者亲裁**：这是「保留取证事实」与「缩小 wire」之间的价值判断，且候选 1 改变启动协议版本；是否值得为约 11.8 KiB 的余量放弃固件原始字段，属于作者对诊断能力与协议经济的取舍，不在代裁范围。

**回退动作**：若作者决定收窄，先统计每个候选删除字段在最近一次真实启动日志中的实际使用；若决定保留，关闭该项并把「源记录是取证层，不是审计输入」写进定义处注释，避免再次被当作死字段。

---

## CASE D01-R14-04 — 阻断（正式案件卷）

**背景**：Direct Map 覆盖建立的深讲缺失，01 与 02 两篇互相指向；候选是补到 D01 §4、解冻 D02 补、或关为范围外。

**源码证据**（供作者写讲解时引用）：
- `os/kernel/src/dm_coverage.rs`：`establish_boot_dm`（启动期在活的 bootstrap root 上追加覆盖）与运行期 DM 覆盖检查；文件内注释区分「建立」与「验证」。
- `os/kernel/src/vm_handoff.rs:513`：DM 窗口可表示性是 A2 自由表的裁剪条件（`VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap`）。
- `os/kernel/src/lib.rs:299-341`：`resume_additive` 在 `store_kernel_info` 之后调用 `establish_boot_dm`，是「先落库、再补覆盖、再跳 kmain」的顺序。

**候选对比**：
1. 补到 D01 §4：保持顺序迭代，当前篇可改；但 D01 的主题是启动交接，DM 覆盖建立属于地址空间阶段，篇幅与重心会冲突。
2. 解冻 D02 补：教学内容归位更自然；但解冻前序篇需要作者人工批准，且会打断当前篇的收敛。
3. 关为范围外：承认两篇都不深讲，读者需自行读源码；与「教程应让读者理解机制为什么存在」的目标冲突。

**反方最强论据**：DM 覆盖是启动与地址空间的交叉点，放在哪一篇都能自圆其说；「两篇互相指向」本身可能是有意的分层，不一定是缺陷。

**为什么必须作者亲裁**：这是教程叙事重心与教学顺序，明确属于「我不允许代裁」的范围；且候选 2 涉及解冻前序篇，是流程性决定。

**回退动作**：若作者选择任一位置，先在该篇写清三件事：DM 覆盖在启动序列中的位置（`resume_additive` 的调用点）、覆盖范围的计算输入（`DirectMapLayout` 与 `plan_dm_policy`）、以及「未覆盖的 conventional 页不会进 VM 自由表」的后果；不要只写「见另一篇」。

---

## CASE D01-R37-05 — 阻断（流程/凭据）

**背景**：跨模型复核因本机 codex 的 OpenCode API key 过期（401）无法完成；候选是等作者提供有效凭据，或按「同 agent 验证 + 已重放命令」豁免。

**我的判断**：这是评审质量门的执行条件问题，不是技术事实或代码取舍。章程要求「AI 的分析只是待核实的信息」，且「同 agent 验证」有已知局限；是否豁免独立复核、是否接受同 agent 的替代证据，是作者对质量标准的决定。凭据也不在我可获取的范围。

**候选对比**：
1. 等凭据：复核独立性完整，但阻塞当前流程。
2. 同 agent 豁免：流程继续，但一致性结论的可信度下降；按项目规则必须显式标注「同 agent 验证 + 已重放命令」，且不得把未复核项标为 CONVERGED。

**反方最强论据**：如果被复核的只是局部代码事实、且命令输出可重放，独立模型并不是唯一证据来源；为了一个过期 key 阻塞整条流程，成本可能高于收益。

**为什么必须作者亲裁**：豁免质量门与接受替代证据属于流程授权，不在代裁范围。

**回退动作**：作者提供 key 后重跑跨模型复核；若作者书面豁免，则在对应 scan 中标注豁免依据、同 agent 验证与重放命令，且相关结论不得升为 CONVERGED。

---

## CASE D01-I-25 — 阻断（范围决策）

**背景**：`os/` 全仓注释语言与锚点整改：related-code 的 50 个 `.rs` 已 CLOSED 0，但全仓仍有大量 CJK 注释与过程编号。候选是按 D01 范围关闭，或开全仓代码批。

**我的独立测量**（只读源码，命令与结果如下）：
- `grep -rP '^\s*//.*[\x{4e00}-\x{9fff}]' os --include=*.rs --exclude-dir=target | wc -l` → `226`
- `grep -rP '//.*[\x{4e00}-\x{9fff}]' os --include=*.rs --exclude-dir=target | wc -l` → `230`
- `grep -rPhoE '\b(D[0-9]{2,3}-[A-Z]?[0-9]{1,3}|DESIGN-[0-9]+|I-[0-9]+|TODO-[0-9]+|D-[0-9]{3})' os --include=*.rs --exclude-dir=target | wc -l` → `247`
测量口径与评审的「233 行 / 301 处」不同（我的正则更窄，且未含测试与文档字符串），但量级一致：全仓确有大量过程编号与 CJK 注释。

**候选对比**：
1. 按 D01 范围关闭：不动范围外代码，符合顺序迭代；但「全仓还有」这一事实会让 D01 的「已清零」结论显得局部。
2. 开全仓代码批：一次性清完；但这是扩大评审责任边界，会把当前篇的冻结与进度拖进全仓改造，且注释语言风格属于出版标准，需逐文件判断哪些注释是「解释机制」而非「过程叙事」。

**反方最强论据**：注释里的过程编号与 CJK 混用会直接违反出版标准，越晚清理成本越高；全仓批可以一次性建立基线，避免每篇重复发现。

**为什么必须作者亲裁**：是否把当前批扩成全仓批，是范围与长期维护责任的决定；且「哪些 CJK 注释应翻译、哪些应删除」涉及教学措辞，不是机械替换。

**回退动作**：若作者选择按 D01 关闭，在 D01 的结论里明确「本批只覆盖 related-code 的 50 个文件」，并把全仓数字留作独立批次；若选择全仓批，先按目录分批并给出每批的判别命令与基线数字。

---

## CASE D01-I-43 / D01-rt-birth-sp — 阻断（证据不足）

**背景**：`test-rt-birth-riscv64` 的 `bump(256)` / re-entry 残项；A2 sp 污染源。要求证据不足时阻断，不猜。

**我核到的源码事实**：
- `os/qemu-tests/test-kernels/kernel/bootstrap/test-rt-birth-riscv64/src/main.rs:498-506`：`bump(1)` 取根页，`bump(256)` 取 1 MiB 的 PT/映射池；注释解释 8 页预算在 `load_vm_elf` 中途耗尽（`MappingFailed`），所以扩到 256 页。这是有记录的设计选择，不是隐藏的魔法数。
- 同文件 `645-660`：`bump` 的起止为 `DRAM_BASE + 0x0200_0000` 到 `DRAM_BASE + 0x0400_0000`（32 MB 偏移起，64 MB 止）。
- 同文件 `172-175`：手写 `MEMMAP` 只有一条 `[0x8000_0000, 0x9000_0000)`（256 MB）。
- 同文件 `527-535`：`KernelInfo.kern_size = 0x200_000`，`kern_phys_base = 0x8020_0000`，所以声明的内核镜像跨度是 `[0x8020_0000, 0x8220_0000)`；而 bump 池从 `0x8200_0000` 开始，前 2 MiB 与该声明跨度重叠。
- 同文件 `578-589`：U-ecall 处理器的内核栈顶设为 `KERNEL_LOAD_PA + 0x200_000 = 0x8220_0000`，向低地址生长，落在上述重叠区的顶部附近。
- `os/libs/minix-rt/src/crt0.rs:315-371`：`rt_birth` 读 `ps_strings`、初始化运行时、发布 `BIRTH`、调用 `main`、`exit`；没有显式栈切换，也没有重入路径。
- `os/kernel/src/smp.rs:1025-1040`：`cpu_id_for_stack_pointer(sp)` 用 AP 栈区间识别 CPU，BSP 的栈不在这些区间内；`smp.rs:1598-1623` 的 AP 栈为每 CPU 16 KiB 的静态数组。
- `os/arch/src/x86_64/ap_early_entry.rs:190-210`：AP 的 `rsp` 从记录字段装载，并特意落在 `sp0` 而非原始栈顶，以免覆盖 TSS 头部的 CPU-id 标记。

**我无法从源码确认的部分**：
1. 「re-entry 残项」具体指哪条重入路径。测试的 `rust_main` 只被调用一次；U-ecall 处理器没有屏蔽中断的代码，但该载体不启用定时器（`main.rs` 注释明确 `boot_init_timer` 不调用），所以「重入」若指中断重入，当前没有开中断的证据。
2. 「A2 sp 污染源」具体指哪个跨度。A2 分类在 `vm_handoff.rs` 的自由集扣减里没有显式的「bsp 栈」扣减项；测试的 BSP 栈是 `.bss` 内的 64 KiB 静态数组，落在声明的内核镜像跨度内，按理被镜像扣减覆盖。但 `kern_size` 与 bump 池的 2 MiB 重叠是事实，是否因此让某个栈页进入自由表，需要实际运行 A2 分类并打印自由集才能确认。
3. `bump(256)` 是否与「re-entry」是同一个问题，还是两条独立残项。案件描述把它们并列，但没有给出触发症状或失败日志。

**候选对比**：
1. 把 `kern_size` 改成真实镜像大小，或把 bump 起点移到 `0x8220_0000` 之后，消除名义重叠，再跑测试。风险：真实镜像大小与链接脚本相关，改错会让 `load_vm_elf` 或镜像窗口断言失败。
2. 只在 A2 分类中显式扣减 BSP 栈与 U-ecall 栈顶区间。风险：若它们已在镜像跨度内，这是重复扣减；若不在，说明前面的跨度声明有误，应该先修声明。
3. 保持现状，把 `bump(256)` 的注释补成完整的预算表（8 页为什么不够、256 页花在哪），把「re-entry」与「A2 sp」各自的触发条件写清。

**反方最强论据**：`bump(256)` 的注释已经把失败症状（`MappingFailed`）与预算来源写清，`cpu_id_for_stack_pointer` 也有明确的 BSP/AP 区分；也许这两条残项只是台账里未勾掉的记录，而不是现行缺陷。反过来说，若真有 sp 污染，它会在 A2 自由表里表现为「本应保留的页被发布给 VM」，这是会损坏运行中内核的严重缺陷，不能凭注释判定不存在。

**为什么阻断**：章程要求「证据不完整时，明确说不知道；不能用详细的推理掩盖缺失的事实」。我无法确定残项指向的具体重入路径与污染跨度，硬判会把猜测包装成结论。

**回退动作**：作者或执行者先给出一个判别实验：在 A2 分类完成后打印自由集与 BSP 栈、U-ecall 栈顶区间，检查交集；同时给出「re-entry」的复现步骤或失败日志。若交集为空且无重入症状，关闭两项；若有交集，按候选 1 或 2 修，并以该实验作为改前/改后的红绿证据。

---

## 追认清单

| 案件 | 处置 | 需要作者做的事 |
|---|---|---|
| D01-DESIGN-01 | 驳回，关闭 | 追认；若要改，按回退动作扩 `BootStorage` 源记录容量 |
| D01-DESIGN-03 | 驳回，关闭 | 追认；若要改，只提取发布步骤，不合并解码 |
| D01-DESIGN-02 | 批准删除公开壳 | 追认后实施；保留类型化局部量 |
| D01-DESIGN-04 | 批准改栈上 `ClaimSet` | 追认后实施；跑三架构启动门 |
| D01-DESIGN-05 | 批准删除 `BootScalars` | 追认后实施 |
| D01-DESIGN-06 | 批准改名 + 删除转发层 | 追认后实施 |
| D01-DESIGN-07 | 阻断 | 作者决定是否统一区间类型、是否动 wire/ABI |
| D01-DESIGN-08 | 阻断 | 作者决定取证字段与 wire 大小的取舍 |
| D01-DESIGN-09 | 批准每槽一个原子量 | 追认后实施；跑 `pt_alloc_write_once` / `pt_alloc_concurrent` |
| D01-DESIGN-10 | 批准 `&[PhysRange]` 边界 | 追认后实施 |
| D01-DESIGN-11 | 阻断 | 文档侧确认修正已落地后关闭 |
| D01-R14-04 | 阻断 | 作者决定教学位置 |
| D01-R37-05 | 阻断 | 作者提供凭据或书面豁免 |
| D01-I-25 | 阻断 | 作者决定是否扩大为全仓批 |
| D01-I-43 | 阻断 | 先做 A2 自由集与栈区间的判别实验 |

## SUMMARY

- 已判 8 条：DESIGN-01（驳回）、DESIGN-03（驳回）、DESIGN-02（批准删公开壳）、DESIGN-04（批准栈上 `ClaimSet`）、DESIGN-05（批准删 `BootScalars`）、DESIGN-06（批准改名并删转发层）、DESIGN-09（批准每槽一个原子量）、DESIGN-10（批准 `&[PhysRange]` 边界）。
- 阻断 7 条：DESIGN-07、DESIGN-08、DESIGN-11、R14-04、R37-05、I-25、I-43。
- 必须作者亲裁：DESIGN-07（区间类型统一与 wire/ABI 变更）、DESIGN-08（取证字段与 wire 大小的取舍）、R14-04（Direct Map 深讲的教学位置）、R37-05（质量门豁免或凭据）、I-25（是否扩大为全仓批）。DESIGN-11 需要文档侧取证（我的章程不读文档）；I-43 需要先做 A2 自由集与栈区间的判别实验，证据补齐后才能判。
- 本批没有 P0-code-bug / P0-test-missing 判决，因此不附「改前 exit≠0、改后 exit=0」的判别命令；批准项均属局部、可逆的实现取舍，实施时仍须过对应的三架构启动门与 `pt_alloc_write_once` / `pt_alloc_concurrent`。

---

## 实施回执（主 session，2026-11-22）

- DESIGN-02：已实施——`PreparedJump`、`prepare_kernel_image_v1`、`jump_kernel_image_v1` 降为私有；仅 `enter_kernel_image_v1` 公开。`minix-kernel --features mock --lib` 866/0/8。
- DESIGN-05：已实施——删 `BootScalars`，`boot_validate_and_prepare` 直接收 `VirBytes`/`PhysBytes`/`u64`；同上测试绿。
- DESIGN-06：已实施——`requires_kernel_span_coverage` + 删 `validate_at`；`minix-boot --lib` 188/0。
- DESIGN-10：已实施——`shim_windows` 改 `&[PhysRange]`；`minix-kernel` 866/0/8、`boot-shim --lib` 136/0。
- DESIGN-04、DESIGN-09：批准但本轮未实施；保留为设计批剩余项。
- DESIGN-01、DESIGN-03：按驳回关闭（非问题）。
- DESIGN-11：文档侧已确认 `globals.rs` 写者与 §3.7 `report_static` 修正，关闭。
- 批次 QEMU 复核：`test-shim-bootmarks-x86.sh` / `test-shim-bootmarks-aarch64.sh` / `test-riscv64-uboot-shim.sh` 见 `tmp/d01-r38/qemu-after-design.log`。
