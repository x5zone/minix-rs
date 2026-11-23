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

## CASE D01-DESIGN-07 — RESOLVED（作者裁决：B）

**作者裁决（2026-11-22）**：B —— 保持现状，补充层次说明；不做全量区间类型统一。

1. 保留 `MemoryRegion`：它是 boot wire 表示，不与内部 `PhysRange` 合并。
2. 保留 `kpriv::MemRange`：它是用户 syscall ABI，不以内部类型替换。
3. 暂时保留 `vm_handoff::MemRange`：能否被 `PhysRange` 无损、低成本替代，属于局部代码简化，不构成当前架构缺陷，也不阻塞 D01。
4. 在相关类型定义处补充责任说明，明确 wire、syscall ABI、自由集内部表示和通用物理区间类型的区别。
5. 不启动 wire v11，不改变 syscall ABI，不建立全量统一设计批次。

**反方最强论据**：重复的区间结构体会增加维护成本，统一的 `PhysRange` 可以让更多不变量由类型保证。接受该论据，但不足以推翻决定：外部边界的不同表示有真实原因；内部类型是否重复要看实际代码复杂度，不看名字数量。

**回退条件**：未来出现真实的区间语义错误，或 `FreeRangeSet` 的局部实现证明可以用 `PhysRange` 明显减少代码且不损失表示能力时，可单独重开局部重构；不得因此自动扩展到 wire 与 syscall ABI。

**事实修正**：`PhysRange::new` 的条件是 `start <= end`，因此 `start == end` 是合法空区间；“`PhysRange` 不能表达 `len = 0`”不成立。仍待确认的是 `end = 2^64` 这类边界是否需要 `MemoryRegion` 的宽度；本轮不以此作为保留理由。

---

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

## CASE D01-DESIGN-08 — RESOLVED（作者裁决：B）

**作者裁决（2026-11-22）**：B —— 保留 72 B `SourceRecord`，不升级 wire v11。

1. `SourceRecord` 是启动来源事实和事后取证记录，不只是自动审计的输入表。
2. 保留原始固件类型、属性、虚拟地址、快照身份及输入来源定位信息。
3. 不以正常生产代码中的字段读取次数作为删除取证字段的依据。
4. 当前没有实际容量压力证明需要缩减 wire；不为理论上的节省引入协议升级与解码复杂度。
5. 在结构定义处补充职责说明，并确保异常诊断仍有途径访问这些记录。
6. 未来若出现真实容量压力，或某个字段被证明冗余且不再提供独立证据价值，重新局部裁决。

**反方最强论据**：保留大量没有自动消费者的字段会占据有限的引导期内存，也可能让协议变成无法维护的历史数据仓库。接受这个担忧，但解决办法应是有证据地做字段淘汰，而不是把正常路径的自动读取情况当成唯一标准。

**回退条件**：出现真实 arena 容量压力，或逐字段证明某字段不能回答任何诊断问题且不能从保留信息无损推导时，单独重开局部收窄；不自动升级 wire v11。

---

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

## CASE D01-R14-04 — RESOLVED（作者裁决：A′）

**作者裁决（2026-11-22）**：A′ —— D01 只补 Direct Map 的启动期概念入口，不承担完整专题讲解。

1. D01 §4.8 介绍 Direct Map 的基本概念、启动序列位置（`store_kernel_info` → `establish_boot_dm` → `jump_to_kmain`），以及“在活动 bootstrap root 上追加、不重建根”的事实。
2. 明确区分 Direct Map 覆盖能力与 VM 物理页分配资格；未被覆盖的候选页不得进入 VM 自由表。
3. 不在 D01 展开完整 `DirectMapLayout`、`plan_dm_policy`、三架构窗口差异；这些归独立内存管理主题。
4. 不解冻 D02，不改变其既定教学范围。
5. 专题最终放 Kernel 还是 VM 暂不裁决；当前略倾向 VM，但不冻结为决定。

**反方最强论据**：D01 是启动交接篇，DM 覆盖属于地址空间阶段，塞进 D01 会模糊主题边界。接受该论据，所以只写概念入口，不写机制专题。

**回退动作**：若未来独立专题落位，D01 §4.8 保留为入口指针；若作者判断 D02 才是自然归属，按解冻流程单独处理，不在本轮夹带。

---

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

## CASE D01-R37-05 — RESOLVED（作者裁决：免费模型；复核已完成）

**背景**：跨模型复核因本机 codex 的 OpenCode API key 过期（401）无法完成；候选是等作者提供有效凭据，或按「同 agent 验证 + 已重放命令」豁免。

**我的判断**：这是评审质量门的执行条件问题，不是技术事实或代码取舍。章程要求「AI 的分析只是待核实的信息」，且「同 agent 验证」有已知局限；是否豁免独立复核、是否接受同 agent 的替代证据，是作者对质量标准的决定。凭据也不在我可获取的范围。

**候选对比**：
1. 等凭据：复核独立性完整，但阻塞当前流程。
2. 同 agent 豁免：流程继续，但一致性结论的可信度下降；按项目规则必须显式标注「同 agent 验证 + 已重放命令」，且不得把未复核项标为 CONVERGED。

**反方最强论据**：如果被复核的只是局部代码事实、且命令输出可重放，独立模型并不是唯一证据来源；为了一个过期 key 阻塞整条流程，成本可能高于收益。

**为什么必须作者亲裁**：豁免质量门与接受替代证据属于流程授权，不在代裁范围。

**回退动作**：作者提供 key 后重跑跨模型复核；若作者书面豁免，则在对应 scan 中标注豁免依据、同 agent 验证与重放命令，且相关结论不得升为 CONVERGED。

**作者裁决（2026-11-22）**：跨模型复核只使用免费模型，避免烧光付费限额；免费模型跑不通就书面豁免一次；严肃复杂场景不硬判，转作者与网页端 GPT 讨论。

**执行结果（2026-11-22）**：`opencode/hy4-flash-free` 完成 6 条断言复核，6/6 CONFIRM，均带 `file:line`；C4 发现 §4.8 箭头链漏 `build_bootstrap_root_and_enable`，已修（见 `crosscheck/round38.md`），七道文档门全绿。R37-05 关闭。

---

## CASE D01-I-25 — RESOLVED（作者裁决：只修 D01 关联代码；门先修）

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

**作者裁决（2026-11-22）**：门要修；D01 直接涉及的 `.rs` 全修；未涉及的其他 stage 代码（如 PM server）不在 D01 范围，不构成 defer 或阻断。按序学习，05/18 stage 不能阻断 01 stage 的 doc-01。

**执行口径**：
1. 先修 `code-publication-check.py` 的两个门 bug：无花括号 `#[cfg(test)]` 导致整个文件剩余部分被当测试区跳过；7–40 位十六进制串误抓 `0170000` 这类数字常量。
2. 用修正后的门重跑 D01 的 48 个 related-code 文件；按**文件级**整改：D01 只要关联到文件内任一符号，整个文件的注释/过程编号/迭代叙事都清。当前已知 16 处可见 + `os/kernel/src/lib.rs` 13 处隐藏，另有新门暴露的项。非注释类 CP-1/2/5/6 单独报告，不混在“只改注释”里。
3. 全仓其余出版物债另立 repo-wide 条目，写清基线数字与 owner；它不是 D01 的 defer，不挡 D01 收敛，但旧 todo 的“全仓完成”必须改成“D01 关联代码完成”。

---

## CASE D01-I-43 / D01-rt-birth-sp — 已授权判别实验（证据待补）

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

**作者授权（2026-11-22）**：执行上述判别实验：A2 分类后打印自由集与 BSP/U-ecall 栈区间，检查交集；同时给出 re-entry 的复现步骤或失败日志。实验后若需要裁决，先由 me-agent 依 `prompt/me.md` 裁，不由作者直接接。

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
| D01-DESIGN-07 | RESOLVED（B）：保持现状 + 层次说明 | 无需再裁决；按 B 执行，不启动 wire/ABI 变更 |
| D01-DESIGN-08 | RESOLVED（B）：保留 72 B，不升 wire v11 | 无需再裁决；按 B 执行，补取证层注释 |
| D01-DESIGN-09 | 批准每槽一个原子量 | 追认后实施；跑 `pt_alloc_write_once` / `pt_alloc_concurrent` |
| D01-DESIGN-10 | 批准 `&[PhysRange]` 边界 | 追认后实施 |
| D01-DESIGN-11 | RESOLVED：文档修正已确认 | 无需动作；已关闭 |
| D01-R14-04 | RESOLVED（A′）：D01 补概念入口 | 无需再裁决；按 A′ 执行，不承担完整专题 |
| D01-R37-05 | RESOLVED：免费模型复核已完成 | 无需再裁决；后续复核仍只用免费模型 |
| D01-I-25 | RESOLVED：只修 D01 关联代码 | 门先修；48 文件清零；全仓另立条目 |
| D01-I-43 | 已授权实验 | 执行 A2 自由集与栈区间判别实验；需要裁决时先由 me-agent 裁 |

## SUMMARY

- 已判 8 条：DESIGN-01（驳回）、DESIGN-03（驳回）、DESIGN-02（批准删公开壳）、DESIGN-04（批准栈上 `ClaimSet`）、DESIGN-05（批准删 `BootScalars`）、DESIGN-06（批准改名并删转发层）、DESIGN-09（批准每槽一个原子量）、DESIGN-10（批准 `&[PhysRange]` 边界）。
- 阻断 0 条：I-43 已由 A2 判别实验关闭（根因是 DTB 未声明导致 OpenSBI 重启，不是栈污染）。R37-05 已由免费模型复核关闭；I-25 已执行完成。
- I-43 的 A2 判别实验已完成：BSP/U-ecall 栈区间与自由集交集为空，测试 PASS；重入来源为 OpenSBI 重启，BUMP_PTR 跨重启累加。I-25 已执行；R37-05 已完成；DESIGN-11 已关闭。
- 本批没有 P0-code-bug / P0-test-missing 判决，因此不附「改前 exit≠0、改后 exit=0」的判别命令；批准项均属局部、可逆的实现取舍，实施时仍须过对应的三架构启动门与 `pt_alloc_write_once` / `pt_alloc_concurrent`。

---

## 实施回执（主 session，2026-11-22）

- DESIGN-02：已实施——`PreparedJump`、`prepare_kernel_image_v1`、`jump_kernel_image_v1` 降为私有；仅 `enter_kernel_image_v1` 公开。`minix-kernel --features mock --lib` 866/0/8。
- DESIGN-05：已实施——删 `BootScalars`，`boot_validate_and_prepare` 直接收 `VirBytes`/`PhysBytes`/`u64`；同上测试绿。
- DESIGN-06：已实施——`requires_kernel_span_coverage` + 删 `validate_at`；`minix-boot --lib` 188/0。
- DESIGN-10：已实施——`shim_windows` 改 `&[PhysRange]`；`minix-kernel` 866/0/8、`boot-shim --lib` 136/0。
- DESIGN-04：已实施——`resv_claims::capture` 填调用方栈上 `ClaimSet`，删 5 个静态、`BootCell` 与自由函数 `records()`；`boot-shim` 136/0、`ebs_boundary_audit` 6/0。
- DESIGN-09：已实施——每槽一个 `AtomicUsize`（0=未注册），CAS 注册；删 `PT_CLAIMED`/`PT_REGISTERED`/占位函数/`UnsafeCell`；`minix-arch` 266/0、`pt_alloc_write_once` 2/0、`pt_alloc_concurrent` 1/0、`kernel mock` 866/0/8。
- DESIGN-01、DESIGN-03：按驳回关闭（非问题）。
- DESIGN-11：文档侧已确认 `globals.rs` 写者与 §3.7 `report_static` 修正，关闭。
- 批次 QEMU 复核：`test-shim-bootmarks-x86.sh` / `test-shim-bootmarks-aarch64.sh` / `test-riscv64-uboot-shim.sh` 见 `tmp/d01-r38/qemu-after-design.log`。

- DESIGN-07：已按 B 实施——`MemoryRegion`/`kpriv::MemRange`/`vm_handoff::MemRange`/`PhysRange` 四处补责任注释；`DECISIONS.md` D-110；未动 wire/ABI；`minix-boot` 188/0、`minix-kernel mock` 866/0/8。
- R37-05：已用免费模型 `opencode/hy4-flash-free` 完成 6 条断言跨模型复核，6/6 CONFIRM；C4 指出 §4.8 漏 `build_bootstrap_root_and_enable`，已补；`crosscheck/round38.md`；七道文档门绿。

---

## I-43 / rt-birth-sp 判别实验回执（2026-11-22）

- 已授权实验：在 `test-rt-birth-riscv64` 中加 A2 自由集与 BSP/U-ecall 栈区间打印。
- 先修了测试的基址错误：`kern_virt_base` 从 `DRAM_BASE` 改为 `KERNEL_LOAD_PA`，消除了 `kernel map: map_huge failed — kernel image mismatch: AlreadyMapped`。
- 实验未到达 A2 打印：`bump(256)` 在第二次进入 `rust_main` 时失败，`current=0x84120000`、`need=0x100000`、`end=0x84200000`；说明 `BUMP_PTR` 被推进/复用，或 `rust_main` 发生重入/重启。具体是哪一个，现有日志不足以判定。
- 根因已定位：测试的 `KernelInfo.platform_sources` 为空，DTB（0x8fe00000）不在任何 identity window；Phase A.5 的 `parse_by_kind` 触发 load page fault，OpenSBI 重启 payload。`.data` 中的 `BUMP_PTR` 不被 `_start` 的 BSS 清零，跨重启累加，最终 bump(256) 失败。
- 修正：把 DTB 声明为 platform source；A2 读取 user-accessible 的 handoff 页前设 `sstatus.SUM`；按生产顺序先 `store_kernel_info`。
- 结果：`bash os/qemu-tests/test-rt-birth-riscv64.sh` → PASS，五个用户态标记全命中。A2 自由集为 [0x80000000,0x80200000)、[0x80400000,0x81000000)、[0x81001000,0x82200000)、[0x82301000,0x8fe00000)；BSP 栈 0x803a2108..0x803b2108、U-ecall 栈 0x803ff000..0x80400000，交集为空。I-43/rt-birth-sp 关闭。

---

## D02 旧账阻断项裁决（2026-11-22，me-agent 冷启动，仅加载 prompt/me.md）

### CASE D02-RUN-C-04 — 关闭（historical）

- **decision**：`historical`，不要求补做复核。
- **basis**：v1 C 批次属已归档 v3 edition；C-11..16 无现存原文；v1 C-04 的“延迟页错误不可达”已被 R5 文本实质覆盖；§3.5 的 aarch64 值经核实为真且非承重。
- **evidence**：`panel/round5/reader-c.md` 只有 C-01..C-08；`git grep 'READER-C-1[1-6]'` 在 D02 run 零命中；`meta/02-higher-half-kernel-review.md:69` 记 v1 C-04 为延迟页错误；R5 42 根因全 ①。
- **最强反方**：关闭“闭环声明不可核”的行会让假回执留在账上。裁决：`historical` 是拒绝背书旧声明，不是认证它；现行权威是 R5 回执与 TODO 账本。
- **回退动作**：若找回 v1 C 原文并在现文复现活缺陷，或作者要求内联 aarch64 值，补 §3.5 三基址后按 closed 结案，不重开旧批次。

### CASE D02-RUN-D02-F10 — 关闭（historical）

- **decision**：`historical`。
- **basis**：36 是改类前旧数；现行 34 = A 11 + B 13 + C 10；R4 表是 `READER-*-S*` 另一族；“全部在 R4 表逐条”按字面为假；v3 edition 已归档，R5 重写重新覆盖。
- **evidence**：`follow-ups.md:14/:48`；`suggestions.md:16-72` 为 S* 族；`WORKFLOW-VERSIONS.md:142` 四次改类；R5 42 根因全 ①。
- **最强反方**：父行记录的是审计失败，关闭等于合法化假回执。裁决：`historical` 只是降为旧 edition 记录；若作者要清 `follow-ups.md:48` 的措辞，按“34 条转出（A 11 + B 13 + C 10）”改，不重开 review。
- **回退动作**：找回 v1 A/B/C 原件并在现文复现时，开现行 edition 的新行，不复活父行。

### CASE D02-RUN-D02-F10a — 补做具体复核（仅 A-11）

- **decision**：保留 open；只留 A-11（§6.2 第五组缺 `target remote :1234` 或未明示指向 01 §6.4 的连接行），其余 9 条按重写/已修关闭。
- **basis**：A-11 是唯一能在现文复现的活项；其余 9 条目标已改或不存在。一次 QEMU/GDB 重放即可判，不属于新增 defer。
- **evidence**：`follow-ups.md:15`；`meta/02-higher-half-kernel-review.md:69`；HEAD §6.2 第五组无 `target remote`；01 篇 §6.4 有连接行。
- **执行**：主 session 已补 `(gdb) target remote :1234` 并限定工件为带符号生产 ELF；`gdb -batch -ex 'file …/kernel' -ex 'info functions kmain'` 命中 `0xffff80000020bc10`。UEFI 载体 stripped PE 的限制已写明。
- **回退动作**：若重放仍不能连上或 `kmain` 断点不命中，按 §6.2 的地址法（`nm` 取地址下硬件断点）修正。

### CASE D02-RUN-D02-F10b — 关闭（historical）

- **decision**：`historical`。
- **basis**：v1 B 13 条原文不可恢复；优先类 B-01/B-03/B-07 已由 R5 的 §3.1/§4.5/§5 结构改动覆盖；无现行可复现目标。
- **evidence**：`follow-ups.md:16` 共 13 条；`meta/02-higher-half-kernel-review.md:41`；R5 的 B-01..B-16 全部 ①。
- **最强反方**：重写不等于新文本没有同类问题。裁决：优先类已逐处覆盖；剩余 P3 文风噪声无原文可核，不能把“不可核”当“有缺陷”。
- **回退动作**：找回具体 B 项并在现文复现时，开现行 edition 的 P3 行，写锚点与最小修复。

### 附带记录

- R5 defect-ledger 的 C 组 ID 错位：`RD-R5-030/032/034` 的来源 ID 与 panel/round5 对不上（实质已修）；按现行 edition 做一次台账 ID 校正，不重开 review。
- §3.5 的 aarch64 物理基址已内联为 `0x40200000`（另列 riscv64 `0x80200000`、x86_64 `0x200000`），满足教学自包含。

---

## D01 学习范围表代确认（2026-11-23，me-agent 冷启动，仅加载 prompt/me.md + 案件卷）

- **decision**：CONFIRM。D01 V3.12 学习范围表草案（S1–S6 + reader_profile）与文档边界一致；不扩大、不缩小。
- **basis**：`prompt/me.md` §我允许代裁的范围 第 1/4 项；作者对本次单一事项的明示授权；只做边界一致性核对，不新增教学方向。
- **evidence**：`tmp/review-loop/01-stage/cases/scope-d01.md` 与 `.../scope-d01-decision.md`；草案六章目标逐条对上边界（`pre_init`/第一次 `head.S` 跳转留本篇；`pg_*`/第二次跳转归 02；第 7 章不进 48 单元）。
- **最强反方**：`prompt/me.md` 不允许代裁教学范围，确认本身可能越权。裁决：作者明确授权本案；判决只核对一致性且可逆，记为代裁待追认。
- **回退动作**：作者推翻 → 判决标 SUPERSEDED，scope 回到未确认，重开确认；只否定某一条 → 最小修正该条后重新确认。
- **追认状态**：代裁待追认。作者追认后转入 `DECISIONS.md`；追认前不得把 CONVERGED 当作最终出版结论。
- **执行**：`review-units.py scope confirm <doc> --by me-agent-deputy`（`confirmed_by` 如实写代理身份，不写 author）。

---

## D01 Ch2 两轮不过案件卷裁决（2026-11-24，me-agent 冷启动，仅加载 `prompt/me.md` + 案件卷）

输入只有两份：`prompt/me.md`、`tmp/review-loop/01-stage/cases/D01-CH2-two-rounds.md`。未读 Ch2 正文、六视角报告、`PROMPT-merged-chapter-workflow.md`。因此下面对章内事实（953 行、5 处指针、2 条术语、各视角结论）一律按案件卷转述处理，证据等级 L3，未重放任何命令。

### Q1 选 A/B/C/D — BLOCKED（不代选）

- **决策**：BLOCKED。四条候选都落在 `prompt/me.md` §我不允许代裁的范围：
  - A 新增伴生篇 = 范围 + 已确立设计方向；案件卷 §三 A 自认「需作者批准」「违反旧裁决，需新案件卷授权」。
  - B 重写为 400–500 行 = 教学顺序 + 哪些内容留作正式教学概念；案件卷自认「等于重写一章」。
  - C 保留为参考章并给六视角加两遍读约定 = 教程叙事重心 + 判据本身；案件卷自认「若作者认为六视角必须线性通读，则 C 不成立」。
  - D 停在 1/6 章 = 范围与进度取舍。
  对应条款：「涉及方向、教学、范围和不可逆改变的事情，不能擅自替我拍板」；§我不允许代裁的范围第 2、5、6 项。
- **案件卷自身证据张力（这是不能照 §四 建议直接放行 C 的硬理由）**：§四 说「主因已从结构错误收敛到指针/术语卫生」，但 §一 第 2 轮表格不支持：
  - Reader-B 仍 FAIL：「2.4.2 的消费者与 16 条截断仍在 2.5 之前，主分叉仍在」「2.4.2 自己的读法要求跳读，承认物理节序不是依赖序」。这是结构/依赖序问题，不是卫生问题。
  - reference 仍 NOT-READABLE：除 5 处指针、2 条术语外，还有「细表/细档/粗档漂移」「2.5.1 重排残留」。
  - beginner-me / G18 / fact 第 2 轮仍「待落盘」；表里 beginner-me 与 G18 各重复两行，六视角实际只有 5 个结论可用。
  按「不接受听起来合理的解释」「AI 的分析只是待核实的信息」，§四 的收敛陈述不能作为代裁前提。
- **C 的最强反方（作者应正面回答的一条）**：C 有可能把「物理节序 ≠ 依赖序」这个结构缺陷，改写成一条「两遍读」的读取约定，即换尺子让评审通过；`prompt/me.md`「评审过程的语言不进正文」「非常反感评审者不断扩大责任边界」正指向这种退化。支持 C 的最强论据则是同文件「概念第一次出现的地方负责认识，真正展开的地方负责理解，两者不必是同一篇文章」——分置在作者教学观里有立足点。两边都成立，所以只能作者裁。
- **evidence**：`/home/xzhao/github/minix-rs/prompt/me.md` §我不允许代裁的范围、§我怎么想问题（反感低价值空转 / 评审者扩大责任边界）、§教学与文档审美（第 91 条）；`/home/xzhao/github/minix-rs/tmp/review-loop/01-stage/cases/D01-CH2-two-rounds.md` §一 第 2 轮表（30–38 行）、§三 A/C、§四。
- **回退动作**：作者亲裁前维持案件卷 §三 D 描述的现状（Ch1 冻结 `a1bfd8aed`、Ch2 不冻结、不新增文档、不重写、不开 D02）。这是等待态，不是我把 D 判成结论；D 的「任务未完成」代价仍由作者认领。

### Q2 C 的六视角输入约定 — BLOCKED

- **决策**：BLOCKED，不授权也不否决。它改的是六视角判据；判据总则在 `PROMPT-merged-chapter-workflow.md`，不在本次可读输入内，我无法判断「章级输入约定」是否与总则冲突。「六视角是否必须线性通读」是价值判断，案件卷 §三 C 亦如此认定。
- **需要作者亲裁的三件事**：(a) 六视角总则是否允许章级阅读约定；(b) 六视角是否要求第一遍线性可读；(c) 若允许，Ch2 的「主线 2.1→2.3→2.4.1→2.5→2.6→2.8，判例盒 2.4.2.3/2.5.1 第二遍」是否即作者认可的读法。
- **反方最强论据**：若允许 Ch2 例外，D01 其余五章会援引先例，六视角逐章被本地化，跨章可比性消失；这与「规则集靠 Step 5.7 自演进、不靠逐案豁免」的方向相反。
- **evidence**：案件卷 §三 C 末句、§四；`prompt/me.md` §我不允许代裁的范围第 2 项、§教学与文档审美第 91 条。
- **回退动作**：作者批准 → 写入 Ch2 六视角 brief 并在 D01 meta-review 单列追认，不追溯 Ch1；不批准 → C 不成立，Q1 回到 A/B/D。

### Q3 A 的伴生篇授权 — BLOCKED

- **决策**：BLOCKED。新增文档改变章节边界与交付物清单，且案件卷自认违反「不先创建 mainline 伴生篇」的旧裁决；推翻旧裁决属已确立设计方向与长期维护责任，需作者新裁决。
- **evidence**：案件卷 §三 A 成本行；`prompt/me.md` §我不允许代裁的范围第 2、5 项。
- **回退动作**：无论 A 是否获准，本轮不创建任何伴生篇；若获准，须同时指定新文档的锚点/单元/摘要重做范围与六视角覆盖方式，并说明旧裁决如何被取代。

### Q4 meta-review 前先修指针/术语并重跑 — 决策：允许，单次、封闭清单、修完封口

- **决策**：允许在 D01 meta-review 前对 Ch2 做**一次**封闭清单的卫生修，并在新哈希上重跑；这不算「第三轮无限重写」。但这次修**不构成 Ch2 通过**：Reader-B 的主分叉、reference 的结构性项不因它消解，Ch2 在 Q1 裁决前不得记 APPROVED/CONVERGED。
- **硬边界**：
  1. 清单封闭：仅 (i) reference 点名的 5 处指针落空/落错；(ii) `unpaged`、`overlaps` 两对术语统一；(iii)「细表/细档/粗档漂移」「2.5.1 重排残留」中**正确目标/正确术语由现有正文唯一确定**的部分。
  2. 判据：每条修必须能在现有正文里唯一确定正确目标；需要二选一语义的，一律不做，转 Q1/Q2 待作者项。
  3. 禁止夹带：不加 §2.0 路线图、不加「判例盒」标签、不调 2.4.2 与 2.5 的物理顺序、不改内容归属——这些属 Q1/Q2 的 A/C，未获授权。工作树里已存在的 §2.4 引导段与 §2.4.1.1 指针属新增正文，不在本次授权内：不要求回退，但必须在 diff 中单独标出，供作者在 Q1 裁决时决定去留。
  4. 只动 Ch2；Ch1 冻结区一字不改；不改 `os/`。
  5. 取证：先冻结工作树生成新哈希 → 六视角在新哈希上重跑/落盘（beginner-me、G18、fact 的第 2 轮「待落盘」必须补齐到同一哈希）→ 重跑期间不再编辑 → 任何结论不得引用旧切片。
- **依据条款**：`prompt/me.md` §我允许代裁的范围第 3 项（局部可逆）、第 4 项（有证据的局部修复建议）；§教学与文档审美（术语稳定、文档自包含、不假设读者读过其他文档）；§流程与质量原则（顺序迭代，冻结区不牵动）；§我怎么想问题（反感为措辞一致性反复开轮）——后者正是本判决定下「单次 + 封闭 + 封口」的理由。
- **反方最强论据**：一次换哈希就作废六视角全部结论（案件卷 §二.3），任何修都有高成本；且 reference 的「漂移」若实为跨章口径问题，Ch2 单方面修只会制造新的跨章不一致。裁决：正因为成本高，才只允许一次并封口；跨章口径项不在授权内，遇到即停。
- **evidence**：案件卷 §一 第 2 轮表、§二.2、§二.3；`prompt/me.md` 相应条款。
- **回退动作**：修的过程中出现必须二选一语义 / 必须动 Ch1 / 必须改结构 → 停手，该条转待作者项，已做的卫生修按 diff 回退或保留由作者定；若作者工作流把「任何换哈希的编辑」都计一轮 → 本次记为最后一轮，此后不再修，直接进 meta-review。
- **轮次口径**：本判决把「卫生修」与「重写轮」分开：卫生修是修既有标准下的缺陷，不是新增叙事。若作者的 workflow 不做此区分，操作结论不变——本次是 meta-review 前最后一次编辑。

### 附带：案件卷自身需先修的证据卫生

- §一 第 2 轮表把 beginner-me、G18 各列两次，实际可用结论只有 5 个视角，该表在 meta-review 计轮前应去重为六视角各一行。
- 同表 fact「待落盘」，与 Q4 要求的「同哈希落盘」直接冲突；不补齐则任何「两轮」计数都缺一环。

**追认状态**：Q4 为代裁，待作者追认；Q1/Q2/Q3 无代裁，等作者亲裁。Q4 的授权在追认前有效但可被推翻，推翻时按回退动作处理。本次判决不含 P0-code-bug / P0-test-missing，故不附 exit≠0/exit=0 判别命令；对应的文档侧证据是「新哈希 + 六视角报告同哈希」。

**SUMMARY**：3 BLOCKED（Q1/Q2/Q3，全是教学/范围/方向）+ 1 允许且封口（Q4，卫生修单次）。Ch2 未冻结这一状态不变。
