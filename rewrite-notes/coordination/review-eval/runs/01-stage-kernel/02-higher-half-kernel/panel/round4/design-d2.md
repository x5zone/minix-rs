# D02 R4 独立设计审查 P-DESIGN D2（回退形态）

> **D1 未运行（D-024），本报告为回退形态。** 第一遍独立设计审查按 D-024 停跑；本报告改为「文档主张 ↔ 源码事实」的独立第二遍：以目标文档 §3「Rust 设计决策」与 §4「实现详解」为对照面，逐项核对现有 Rust 实现。
>
> 输入范围：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`（通读 1289 行，重点第 329–1103 行）、`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/related-code.txt` 的 29 个文件、`tools/doc-code-map.sh` 输出的 30 个文件、`tmp/review-eval/01-stage-kernel/02-higher-half-kernel/panel/round4/trait-audit.json`、`AGENTS.md`、`tmp/review-eval/01-stage-kernel/02-higher-half-kernel/panel/round4/decisions-snippet.md`。
>
> 未读取：历史评审结论、其他面板输出、任何 `expected/` 目录。唯一写入是本文件。
>
> 纪律：单实现 trait 不一定坏；`bool` 不一定换 `enum`；不为「更 Rust」改外部行为；先减法后加法。类型级提案按 `prompt/review-rules/review-code-checklist.md` §3.5 三问回答；可信链上的防护优先 `assert!` 而不是新增类型。

## DIFF-TABLE

| 维度 | 文档主张 | 现有实现 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 | §3.3（doc:518–557）：栈顶由 `kern_virt_base + kern_size` 在消费点现算，公式只留在 `KernelInfo::kern_stack_top()` 一处；§3.4（doc:563–599）：`-> !` 加四条安全前置；§3.5（doc:601–657）：三档粒度阶梯，失败发生在任何页表写入之前。 | `KernelInfo::kern_stack_top()` 存在（`os/libs/minix-boot/src/kernel_info.rs:200–206`）；`boot_validate_and_prepare` 为对齐断言再次展开 `checked_add`（`os/kernel/src/lib.rs:938–940`）；`HigherHalf::jump_to_kmain` 返回 `!`（`os/kernel/src/boot/higher_half.rs:48`）；`plan_kernel_mapping` 的三档阶梯与实现一致（`os/kernel/src/boot_policy.rs:98–140`）。 | 公式在断言路径存在第二份展开；`jump_to_kmain(kinfo, stack_top)` 的 `stack_top` 形参是文档自己承认的可表示冗余（doc:582）；「只调用一次」实际上已被 `-> !` 从同线程调用图上排除，文档仍按「类型系统不区分调用次数」表述（doc:580）。 | 接受。冗余形参按已登记设计项删除（编号归属见 DESIGN-D2-07）；文档把「调用次数」措辞收紧为「从引导 CPU 调用、不得重入」。 |
| 身份 | §3.1（doc:389–401）：四个量由装载方从 PT_LOAD 现算，`kern_virt_base` 的两种约定（x86_64 含物理基址、aarch64/riscv64 不含）不能混用；§4.4（doc:1069）：`KernelInfo` 由共享 builder `build_kernel_info` 组装。 | `KernelLoadResult` 与 `KernelInfo` 的字段命名清楚（`os/boot-shim/src/loader.rs:75–86`、`os/libs/minix-boot/src/kernel_info.rs:22–160`）；`PreparedJump`（`os/kernel/src/lib.rs:426`）、`PlacementPool`（`os/kernel-image/src/bootface_common.rs:160`）、`BootScalars`（`os/kernel/src/lib.rs:865–912`）已经把同型相邻实参收进命名结构；但 `build_kernel_info` 的 10 个位置实参里前两个都是 `&'static [MemoryRegion]`（`os/boot-shim/src/lib.rs:105–129`），两个直核面各做 6 次裸指针重建（`os/kernel-image/src/bootface.rs:408–422`、`os/kernel-image/src/bootface_a64.rs:389–401`）。 | 「用命名结构消除身份混淆」的做法没有贯彻到共享 builder：`memmap` 与 `reserved_regions` 交换后仍能编译，运行时一个把保留区当空闲内存、另一个把空闲内存当保留区，都属于「看似合理」的静默坏结果。 | 需修。见 DESIGN-D2-04：复用现有 `BootReport` 命名视图，不新增类型。 |
| 所有权 | §3.1（doc:365）：v1 接收侧只做 additive 阶段，把载荷落进本镜像 `.bss`；§4.4（doc:1073）：两条生产路径在跳转前都把 `KernelInfo` 落进 `.bss` 副本，原因是 shim 堆页在高半切换后不再被映射。 | `store_kernel_info` 深拷贝 `reserved_regions`、`memmap`、`boot_modules`、`platform_sources`、`param_buf`，并有一次写入断言（`os/kernel/src/lib.rs:1575–1693`，一次性断言在 `:1581–1584`）；`BootStorage::report` 已经提供全部命名切片（`os/libs/minix-boot/src/boot_storage.rs:253–311`）。 | 所有权语义正确；但 `KernelInfo` 的 `&'static [T]` 字段迫使两个直核面在调用点用 `from_raw_parts` 把借用「洗白」成静态寿命，`report()` 的命名视图反而没有被使用。 | 需修。与身份问题同根：同一次改造可以同时消掉 12 处 `unsafe` 与两个同型位置实参（DESIGN-D2-04）。 |
| 错误 | §3.2（doc:457–501）：迭代器对畸形表项/越界段 `continue` 跳过，`ElfError` 共九个变体，D2-01/D2-05 未修；§4.2（doc:816–847）却已经按 I-48 / D-084 描述「畸形程序头是 `Err` 项，不是静默跳过」。 | `ElfError` 实际 12 个变体（`os/libs/minix-elf/src/lib.rs:157–203`）；`SegmentIter::next` 对表范围溢出、表项不可读、段文件越界、`filesz > memsz` 都返回 `Some(Err(..))`（`os/libs/minix-elf/src/lib.rs:393–440`）；缓冲区装载器在 I-47 后先验后写，超界返回 `TargetBufferTooSmall`（`os/boot-shim/src/loader.rs:266–340`）。 | §3.2 与 §4.2 互相矛盾，§3.2 与实现相反；§5.4 M6（doc:1169）仍写「九个变体」。`InvalidSegment` 的归一化（丢现场）与源/目标重叠走 panic 的不对称仍是真实残留，对应设计批 D2-06。 | 文档 P1 漂移，本轮修（DESIGN-D2-01）；D2-01 收窄为「`SegmentIter::new`/`parse_phdr` 仍有未检查加法」，D2-05 可销账。 |
| 并发 | §3.4（doc:572–580）：只从引导 CPU 调用一次；§4.4（doc:1077–1087）：跳转完成时分页已开、保护结构未建。 | `KERNEL_INFO` 是 `SyncUnsafeCell`，引导期一次写入后只读（`os/kernel/src/globals.rs:281`）；`store_kernel_info` 第二次写入直接 panic（`os/kernel/src/lib.rs:1581–1584`）；`HigherHalf` 只由 `arch_boot_finish`/`resume_additive` 的 `-> !` 尾部调用。 | 运行时语义一致，没有并发缺陷面；文档对「只调用一次」的类型强度估计偏低。 | 接受。把该行改写为「从引导 CPU 调用，不得重入；同线程二次调用已被 `-> !` 排除」，不新增 token 类型。 |
| 抽象边界 | §3.4（doc:559–599）：`HigherHalf` trait 加三份架构实现，前置条件单一落点；§3.2（doc:405–409）：`minix-elf` 被五个 crate 依赖；§3.1（doc:333–403）：三种链接脚本形态，`os/kernel/src/arch/*/link.ld` 无生产链接读者。 | `trait-audit.json` 对 `HigherHalf` 记 `impls=4, refs=32, dyn=0, MULTI-IMPL`（三架构实现加 `os/kernel/src/lib.rs:6185` 的 mock）；`FileLoader` 记 `impls=3, refs=16, dyn=1, MULTI-IMPL`；三份 arch `link.ld` 的唯一读者是 `os/kernel/tests/link_ld_lockstep.rs`；`grep -rn 'src/arch/.*link\.ld' os --include=*.rs --include=*.toml --include=*.sh` 共 6 处命中，其中 5 处是注释、1 处是测试头注释。 | trait 边界成立，不是单实现依赖注入；arch `link.ld` 不是「预期布局」，而是过期副本：三份文件都没有 `ENTRY(_start)`、没有 `.text.boot` 的 `KEEP`、没有 64 KiB 引导栈符号、没有 2 MiB 收口，栈仍是 16 KiB 的 `k_initial_stktop`。 | trait 保留；arch `link.ld` 与锁步测试删除（DESIGN-D2-06，需用户裁决）。 |
| 可测试性 | §5.1（doc:1107–1126）：`minix-elf` 23 个测试、`loader.rs` 22 个测试，buffer 孪生覆盖拷贝/清零；§5.2（doc:1128–1152）：`qemu_test` 裸 `kmain` 的三条断言，PC 不入成败判定；§5.4（doc:1158–1178）：R02-T03/T04 已知缺口。 | 测试实际为 `minix-elf` 27 个（`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` 输出 27）、`loader.rs` 31 个（`grep -c '#\[test\]' os/boot-shim/src/loader.rs` 输出 31）；x86_64 载体的 `KernelInfo` 用 `image_base` 与 `image_end - image_base`（`os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:51–62`）；x86_64 工件有 5 个 `PT_LOAD`，其中一个是 `.lrodata.*` 孤儿段。 | 计数、测试名与载体取值过期；`.lrodata` 孤儿让 x86_64 多一个只读段，且落点由链接器版本决定，现有 L0–L9 不断言它。 | 文档 P2 漂移（DESIGN-D2-10）；`.lrodata` 收进 `.rodata`（DESIGN-D2-05）。 |

## SUBTRACTION-TABLE

| 加法提案 | 减法替代 | 减法拦不住的理由 | 保障强度对比 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| 新增 `MemoryTables { memmap, reserved }` 值对象承载两张内存表 | 复用现有 `BootReport` 命名视图；把 `BootStorage` 的静态所有权写进类型（`take(&'static self)`）后调用现有 `report()` | `BootReport` 已经给六个载荷命名，生产与测试构造点齐全；不需要再发明一个只装两张表的类型 | 编译期字段名加删除 12 处 `from_raw_parts`，强于现状 | 0 类型，1 处生命周期收紧 | 采（DESIGN-D2-04） |
| 新增 `StackTop` newtype 或给 `jump_to_kmain` 加重载 | 删除 `stack_top` 形参，实现内取 `kinfo.kern_stack_top()` | 两个生产调用点传入的表达式完全相同，没有合法用例需要不同值 | 运行时等价，非法状态消失 | −1 形参，0 类型 | 采（DESIGN-D2-07） |
| 给三份 arch `link.ld` 补齐 `ENTRY`、`.text.boot`、64 KiB 栈、2 MiB 收口，并保留锁步测试 | 删除三份 arch `link.ld` 与 `link_ld_lockstep.rs`，生产脚本加 `check-layout.sh` L0–L9 是唯一真相 | 保留就要长期同步两份布局；教学价值可以由文档文字承担 | 生产工件加 `readelf` 实测，强于常量锁步 | −3 文件，−1 测试 | 需用户裁决（DESIGN-D2-06） |
| 为 `.lrodata` 增加独立输出段或新增一条对齐断言 | 把 `.lrodata .lrodata.*` 收进 `.rodata`，与 `.ldata`/`.lbss` 同族处理 | 断言只是把风险变成红灯；收进 `.rodata` 同时消除孤儿与第 5 个 `LOAD` | 段数从 5 降到 4，且不再依赖链接器版本 | 0 | 采（DESIGN-D2-05） |
| 把 VFS 的 `ElfHead` 合并进 `minix-elf` 的共享解析器 | 保持两套解析器，各自测试 | 策略不同：VFS 接受 `ET_DYN`、以首块缓冲为界，`minix-elf` 只接受 `ET_EXEC`、以完整镜像为界；合并要参数化共享解析器，新增 API 并改 VFS 契约 | 保障强度不增，因为两边各有完整测试 | +1 参数化面 | 否 |
| 给 `HigherHalf` 增加 `dyn` 支持或把 `H` 绑成 `A` 的关联类型 | 保持静态泛型 trait 加私有 `arch_boot_finish<A, H>`/`resume_additive<A, H>` | trait 有 3 个真实实现加 1 个 mock；调用点已按架构指定；没有运行时多态需求；`HigherHalf` 在 kernel crate、`BootArch` 在 arch crate，关联类型会引入反向依赖 | 等价 | 0 | 否（拒绝加法） |
| 给 `place_modules` 引入 `Result<(), PlaceModulesError>` | 保持 `Option<()>`，诊断在检测点打印 | 物理视图下 panic 与格式化路径带 baked VMA（`os/kernel-image/src/bootface.rs:127–133` 的源码注释），结构化错误需要新的无格式化消息面 | 现状已是 fail-stop；改枚举只增加概念 | +1 枚举 | 否（延后） |
| 把 `KernelInfo` 改成 `KernelInfo<'a>` 生命周期参数 | 保持 `'static` 加一次性静态存储；只在 `BootStorage` 上把静态性类型化 | 存储确实是进程级静态，`'static` 是事实而不是谎言；参数化会把借用寿命传染到 wire、global 与五个 crate 的测试 | 等价 | +1 生命周期参数，大范围改动 | 否 |
| 给 `KernelInfo` 加私有构造或 builder，让非法状态不可表示 | 保持 `pub` 字段加 `validate()`/`check_boot_postconditions()` 双点 fail-fast | 跨 crate、跨镜像与测试都要直接构造；builder 覆盖不到 wire 解码路径，运行时断言仍是必需 | 等价 | +builder | 否（可延后） |
| 给 `rust_image_main` 增加 `HandoffMode` 枚举 | 保持 `arena_pa == 0` 判定并诚实停机 | 0 不是合法 arena 物理地址，判据充分；枚举会改入口 asm 的寄存器约定 | 等价 | +1 枚举 | 否 |

## FINDING DESIGN-D2-01（P1）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:457–501`（§3.2 迭代器与错误枚举）、`:1169`（§5.4 M6）；对照 `os/libs/minix-elf/src/lib.rs:157–203`、`:393–440`。
- **严重度**：P1。
- **问题**：文档 §3.2 说迭代器「命中这些条件时 `continue`（不产出该段）」，把畸形表项、越界段都描述成静默跳过，把 `ElfError` 写成九个变体，并把 fail-closed 记为「设计批 D2-05」；实现已经按 I-48 / D-084 改成「畸形程序头以 `Err` 项浮出」，枚举有 12 个变体，`filesz > memsz` 由迭代器内的 `FileszExceedsMemsz` 拒绝，段文件越界由 `SegmentOutOfBounds` 拒绝。同一篇文档的 §4.2（`:816–847`）已经按新行为描述，§3.2 却停留在旧行为，两章互相矛盾。
- **影响**：读者会按「段比预期少」设计调用方，而实际实现要求调用方在第一个 `Err` 上短路；`compute_kernel_layout` 的 `seg_result?` 与 `load_segments_into_phys_memory` 的 `collect::<Result<..>>()` 都依赖这条新契约。测试对账门会被陈旧的「23 个测试」「22 个测试」「`test_segment_iter_skips_segment_data_out_of_bounds`」绊住（见 DESIGN-D2-10）。D2-01/D2-05 的登记状态与代码不符，会让后续评审重复立项。
- **证据**（本会话已重放）：
  - `grep -n 'return Some(Err' os/libs/minix-elf/src/lib.rs` → 399、403、410、421、424、432 六处。
  - `sed -n '157,203p' os/libs/minix-elf/src/lib.rs` → `TooShort`、`BadMagic`、`Not64Bit`、`NotLittleEndian`、`NotExecutable`、`PhdrOutOfBounds`、`PhdrEntryTooSmall`、`NoLoadSegments`、`InvalidSegment`、`SegmentOutOfBounds`、`FileszExceedsMemsz`、`TargetBufferTooSmall`，共 12 个。
  - `grep -n 'seg_result?' os/boot-shim/src/loader.rs` → `compute_kernel_layout` 与物理装载器都短路 `Err`。
  - 文档 `:819` 已经写「I-48 / D-084: a malformed program header is an `Err` item, not a silent skip」，与 `:457` 的 `continue` 说法直接冲突。
- **最小修复**：重写 §3.2 的「迭代器行为」段与九个变体清单：按当前 12 个变体分组，明确三类语义（正常过滤 `PT_LOAD`、表项畸形返回 `Err`、段内容畸形返回 `Err`）；把 D2-01 收窄为「`SegmentIter::new` 与 `parse_phdr` 的表范围算式仍是未检查加法」并保留为真实残留；把 D2-05 标为已由 D-084 落地；同步 §5.4 M6 的「九个变体」与 §5.1 的测试计数/测试名。
- **减法替代**：纯文档删改，不需要新增代码或类型。把「设计缺口」从清单里删掉，比新增一个「错误模型说明」小节更符合最小化。
- **三问**：非类型级提案，不适用。
- **教学概念账**：新增 0 个概念；删除陈旧清单一项，读者需要记住的错误分类从「三类 continue」变为「两类 `Err` 加一类正常过滤」，概念数下降。
- **分类**：[MODERN]（文档对齐已落地的现代化实现，不改变外部行为）。
- **裁决**：接受。理由：同篇 §4.2 已按新实现写，§3.2 是唯一还在描述旧行为的地方；代价是纯文档改写，0 行代码改动。
- **置信度**：高（事实层面）。
- **是否需源码核实**：已核实；命令与行号见上。
- **若我错了**：如果本轮评审的基线是 D-084 之前的树，§3.2 在历史上是对的；但同篇 §4.2 已引用 D-084，文档内部仍不自洽，且当前工作树就是 12 变体版本。

## FINDING DESIGN-D2-02（P2）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:407–409`（§3.2 的「用途按调用点分四类」）；对照 `os/boot-shim/src/loader.rs:227–243`、`:485–517`。
- **严重度**：P2。
- **问题**：文档说「模块是按原字节整块拷贝的，不经解析——`loader.rs` 的 `fn load_boot_modules_into` 里没有段迭代」。D-082（I-36）之后，这个函数会先调用 `elf_file_end(data)`，用 `minix_elf::segment_iter` 找到最后一个 `PT_LOAD` 的文件尾，再把模块字节修剪到该长度；这是解析器在文档所列四类用途之外的第五类用途。
- **影响**：解析器的消费者清单少一类，读者会以为模块字节连 `.comment`/`.symtab`/`.debug_*` 都进内存；也会低估 `minix-elf` 的调用面，做接口裁剪时容易误删 `segment_iter`。
- **证据**：
  - `os/boot-shim/src/loader.rs:227–243` 的 `elf_file_end` 内调用 `minix_elf::segment_iter`。
  - `os/boot-shim/src/loader.rs:517` 起 `let used = elf_file_end(data).unwrap_or(data.len()); let data = &data[..used];`，随后按页分配并清零尾部。
  - `decisions-snippet.md:14` 的 D-082 明确「`loader.rs` 的 `load_boot_modules_into` 加一行 `let used = elf_file_end(...)`」。
- **最小修复**：把 §3.2 的「四类」改为「五类」，在 boot-shim 那一类里补一句「内核镜像按段摆放；模块按 ELF 文件尾修剪后整块拷贝，不重定位、不逐段映射」；删掉「没有段迭代」。
- **减法替代**：纯文档修改。把「模块不经解析」这句删掉，比新增一段「模块装载说明」更小。
- **三问**：非类型级提案，不适用。
- **教学概念账**：不新增概念；把「四类」改成「五类」，读者多记住一个调用点，但这是实现事实，不能靠删文档回避。
- **分类**：[MODERN]（文档对齐 D-082 的既有实现）。
- **裁决**：接受。理由：模块修剪是 D-082 的既有行为，文档的「不经解析」会让接口裁剪误删 `segment_iter`；代价是 §3.2 一段文字与一张用途表，0 行代码改动。
- **置信度**：高。
- **是否需源码核实**：已核实；`grep -n 'elf_file_end' os/boot-shim/src/loader.rs` 显示定义与唯一调用点。
- **若我错了**：如果 D-082 被回滚，`load_boot_modules_into` 会回到整块拷贝；但当前工作树与决策摘要都指向 D-082 已落地。

## FINDING DESIGN-D2-03（P2）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:365`（§3.1 生产交接形态）、`:1018`（§4.4 路径 A）；对照 `os/libs/minix-boot/src/handoff_v1.rs:270–296`、`os/libs/minix-boot/src/handoff_v1_read.rs:74–89` 与 `:123–131`、`os/libs/minix-boot/src/boot_storage.rs:300`。
- **严重度**：P2。
- **问题**：文档两处把 v1 arena 描述为「七块载荷（memmap/reserved/modules/platform/param/sources/startup）」。wire v10（D-062）已经删掉 startup-object 分区：`BOOT_HANDOFF_V1_VERSION = 10`，`HandoffScalars` 只有 `kern_virt_base`、`kern_phys_base`、`kern_size` 三个布局标量，`HandoffDescriptors` 只有六个字节长度字段，`BootReport` 的注释明确写「v10 removed the startup-object slice」。
- **影响**：读者按七块载荷去核对 `arch_boot_resume_v1` 的六次 `decode_*` 调用，会以为少了 startup 解码，进而误判为缺陷；wire 版本演进（D-014 的版本拒绝负控）也会被误解成七分区模型。
- **证据**：
  - `os/libs/minix-boot/src/handoff_v1.rs:296` 的版本常量是 10；同文件 `:290–296` 的注释写「the startup-object partition left the envelope」。
  - `os/libs/minix-boot/src/handoff_v1_read.rs:74–89` 的 `HandoffScalars` 只有三个布局标量；`:123–131` 的 `HandoffDescriptors` 是六个字段。
  - `os/kernel/src/lib.rs:680–770` 的 `arch_boot_resume_v1` 依次解码 memmap、reserved、modules、platform、param、sources 六块。
- **最小修复**：两处把 `startup` 删掉，改成「六块载荷」；如果保留历史说明，另起一句「v10 删除了 startup 分区（D-062）」。
- **减法替代**：纯文档删除。把「七块」改成「六块」并删掉一个名词，比新增一段 wire 版本说明更小。
- **三问**：非类型级提案，不适用。
- **教学概念账**：概念数从七降到六，减少一个已不存在的分区。
- **分类**：[MODERN]（文档对齐 wire v10）。
- **裁决**：接受。理由：wire v10 的六个分区与版本注释是硬事实；代价是两处文档各删一个名词。
- **置信度**：高。
- **是否需源码核实**：已核实；版本常量与六次解码调用都在本会话读过。
- **若我错了**：如果存在一个未在 `handoff_v1_read` 中解码、但仍留在 arena 的 startup 分区，那么文档是对的；但 `HandoffDescriptors` 与六次解码都不含它，且 wire 版本注释明确说它已退出信封。

## FINDING DESIGN-D2-04（P2）

- **位置**：`os/boot-shim/src/lib.rs:105–129`（`build_kernel_info` 的十个位置实参）、`os/kernel-image/src/bootface.rs:386–434`、`os/kernel-image/src/bootface_a64.rs:389–416`（两个直核面的裸指针重建）；文档锚点 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1069`（「`KernelInfo` 由共享 builder `build_kernel_info` 组装」）。
- **严重度**：P2（按 `review-code-checklist.md` §3.5 的代价门槛：静默性与可混淆面成立，但调用点改动超过「≤3」门槛，故记 P2 建议而不是缺陷）。
- **问题**：`build_kernel_info` 的前两个形参都是 `&'static [minix_boot::MemoryRegion]`（`memmap`、`reserved_regions`），交换后仍能编译，运行时不 panic、类型不报错，属于「同型相邻实参」的可混淆面。两个直核面为了把 `BootStorage::take()` 得到的非静态借用交给要求 `&'static` 的 `KernelInfo`，各自用 6 次 `core::slice::from_raw_parts` 重建切片，再按位置传参；`BootStorage::report()` 已经提供同一组命名切片，却没有被使用（`os/libs/minix-boot/src/boot_storage.rs:253–311`）。
- **影响**：最坏后果是 `reserved_regions` 被当成 `memmap`：VM 会把 boot-shim 的 LOADER_DATA、模块字节、ACPI 页当成空闲内存发出去，表现为引导中后段的静默数据损坏；反方向是 `memmap` 被当成保留区，VM 拿不到空闲页，在 `servers/vm/src/boot.rs` 的「no free memory regions」处早停。两个方向都符合「看似合理的坏结果」：启动代码有日志、没有现场，定位成本高。项目已有同类判例：`PreparedJump` 的注释（`os/kernel/src/lib.rs:418–426`）把 `entry`/`arena_pa` 交换称为「close to the worst possible boot failure」，`PlacementPool`（`os/kernel-image/src/bootface_common.rs:153–167`）记录的是同一类「三个相邻 `u64` 交换仍能编译」。
- **证据**：
  - `sed -n '105,129p' os/boot-shim/src/lib.rs` → 十个位置形参，前两个同为 `&'static [minix_boot::MemoryRegion]`。
  - `grep -c 'core::slice::from_raw_parts' os/kernel-image/src/bootface.rs os/kernel-image/src/bootface_a64.rs` → 两个文件各 6 次。
  - `os/libs/minix-boot/src/boot_storage.rs:253–311` 的 `report()` 返回 `BootReport<'_>`，字段名为 `memmap`、`reserved_regions`、`boot_modules`、`platform_sources`、`param_buf`、`memmap_sources`，并携带三个布局标量。
  - `grep -rn 'build_kernel_info(' os --include=*.rs` → 5 个调用点：2 个生产（`bootface.rs`、`bootface_a64.rs`）+ 3 个测试（`opensbi_helpers.rs` 两处、`uefi_helpers.rs` 一处）。
- **最小修复（复用现有类型，不新增类型）**：
  1. 把 `BootStorageCell::take` 的接收者从 `&self` 收紧为 `&'static self`，返回 `&'static mut BootStorage`；四个调用点（`uboot_main.rs:475`、`uefi_helpers.rs:1075`、`bootface.rs:148`、`bootface_a64.rs:156`）全部是模块级或函数内 `static`，类型上可以满足（`os/boot-shim/src/lib.rs:161`）。
  2. `build_kernel_info` 改为 `pub fn build_kernel_info(report: &minix_boot::BootReport<'static>, authority: minix_boot::InputObjectId) -> minix_boot::KernelInfo`，函数体内只按字段名取值。
  3. 两个直核面在 `storage.fill_*` 之后调用 `storage.report(KERN_VIRT_BASE, KERN_PHYS_BASE, KERN_SIZE)`，把 `&report` 传给 builder；删除 12 处 `from_raw_parts` 与 `let _ = storage;` 的寿命断点。
  4. 三个测试调用点构造静态数组的 `BootReport` 字面量，字段名让交换在编译期变成显式选择。
- **减法替代**：不新增 `MemoryTables` 或 `KernelInfoBuilder`；`BootReport` 已经是那组命名视图，只差把它的静态寿命在 `take()` 上写出来。新增概念 0 个类型，只收紧一处生命周期。
- **三问（§3.5）**：
  - **① 被防的故障由谁制造？** 本仓可信链上的调用点写错（同一签名的两个同型实参互换），不是外部攻击者。它属于「同一调用点用错」而不是「跨函数所有权移交」；`assert!` 拦不住，因为两张表各自都是合法、互不相交的列表，交换后没有任何单一断言能识别语义错位。用命名视图把「哪个是哪个」写在调用点，是这里唯一可判的手段。
  - **② 是否已被 borrow checker / 类型系统 / 现有类型拦住？** 没有。两个形参类型相同，交换可编译；`KernelInfo` 的字段虽然同名，但构造发生在 builder 内部，调用点不经过字段名。`BootReport` 是现有类型，复用它不引入新类型，只把字段名提前到调用点。
  - **③ 代价判据。** 最坏后果：保留区被当空闲内存发出，静默数据损坏；同类判例是 `PreparedJump`（D-073）与 `PlacementPool`（`bootface_common.rs:153–167`）。计数：新增字段 0，API 表面从 10 个位置形参降到 2 个（倍数小于 1，不是扩张），调用点改动 5 个（2 生产 + 3 测试），超过 §3.5 的「≤3」门槛，因此判 P2 建议而不是 P1；但改造净删除 12 处 `unsafe` 与 8 个形参，理解成本是下降的。
- **教学概念账**：新增类型 0，新增字段 0；删除 12 处 `from_raw_parts`、8 个位置形参、两个直核面各一段寿命断点。读者需要理解的概念从「两个同型切片 + 手动寿命洗白」变成「一份命名的 `BootReport`」。
- **分类**：[MODERN]（复用现有命名值对象的纯内部重构，不改变任何外部行为与 C 语义）。
- **裁决**：接受为 P2 建议，进入下一 design/code batch，不阻断本轮文档收敛。理由：静默交换的后果严重且已有 `PreparedJump`/`PlacementPool` 判例；代价是 5 个 `build_kernel_info` 调用点、4 个 `take()` 调用点与 1 处生命周期收紧，净删除 12 处 `unsafe` 与 8 个位置形参。
- **置信度**：缺陷高；修法的借用检查细节（`&'static mut` 之后如何再取共享切片）需要编译器验证，必要时退化为「`take()` 返回 `&'static BootStorage`，写阶段用一次性 `&mut` 完成后封存」。
- **是否需源码核实**：已核实签名、调用点与 `report()` 形状；未在本会话编译改造后的代码。
- **若我错了**：如果存在某个调用方必须先构造 `KernelInfo` 而没有 `BootStorage` 或 `BootReport`（例如只拿到三个标量的测试辅助），则保留 builder 但把前两个同型参数换成一个 `MemoryTables` 值对象；那是 +1 类型，代价仍小于现状。

## FINDING DESIGN-D2-05（P2）

- **位置**：`os/kernel-image/x86_64.ld:63`（`.rodata` 的 `*(.rodata .rodata.*)`）、`:68–70`（`.data` 收 `.ldata`）、`:81–83`（`.bss` 收 `.lbss`）；文档锚点 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:736`、`:1204`。
- **严重度**：P2。
- **问题**：链接脚本已经为 rustc 的大数据拆分节 `.ldata`、`.lbss` 做了同族归并，却漏掉 `.lrodata`。x86_64 工件里 `.lrodata.*` 成为孤儿节，被链接器单独摆成一个只读 `PT_LOAD`，共 986 个节；aarch64 与 riscv64 当前工件为 0 个，但同一类 codegen 变化随时可能在另两个架构出现。文档承认这个孤儿「落在收口之前，所以 L5 仍通过」，但没有给出为什么不修的判据；落点由链接器版本决定，下一次链接器升级就可能把它排到 `ALIGN(0x200000)` 之后，把跨距推成非 2 MiB 整数倍，让 `plan_kernel_mapping` 在任何页表写入前 panic。
- **影响**：当前工件仍是 5 个 `PT_LOAD`（`.text`、`.lrodata.*`、`.rodata`、`.data`、`.bss`），比 aarch64/riscv64 多一个段；镜像尾的收口依赖链接器把孤儿放在收口之前，而不是脚本自己保证。这是「靠链接器行为而不是靠脚本」的布局不变量，和 §3.5 想建立的「布局要满足映射粒度」正好相反。
- **证据**（本会话已重放）：
  - `readelf -SW os/target/x86_64-unknown-none/release/kernel | grep -c '\.lrodata'` → 986。
  - `readelf -lW os/target/x86_64-unknown-none/release/kernel | grep -c '^  LOAD'` → 5；第二条 `LOAD` 的 Section to Segment mapping 全是 `.lrodata*`。
  - `readelf -SW os/target/aarch64-unknown-none/release/kernel | grep -c '\.lrodata'` → 0；riscv64 同样为 0。
  - `SKIP_BUILD=1 bash os/kernel-image/check-layout.sh x86_64` → L3a 实得 5 段，L5 仍 PASS；说明现有断言不禁止这个孤儿。
- **最小修复**：三份生产脚本的 `.rodata` 通配都加上 `.lrodata .lrodata.*`（riscv64 那份与 `.srodata`/`.sdata2` 并列）；重建三架构工件，重跑 `check-layout.sh all` 与 §5.2 的 `test-higher-half`；更新文档 §4.1 与 §6.2 的 `readelf` 预期（x86_64 从 5 个 `LOAD` 变为 4 个，删除孤儿节说明）。若跨距因去掉对齐空隙而缩短，`kern_size`/`stack_top` 的示例数字按实测更新；riscv64 的 L9 会重新对账 `bootface.rs` 的 `KERN_SIZE`。
- **减法替代**：不新增输出段、不新增对齐断言；把 `.lrodata` 收进已有的 `.rodata`，是删除孤儿处理的减法。新增概念 0 个。
- **三问**：非类型级提案，不适用（这是布局脚本的归并，不是类型级防御）。
- **教学概念账**：读者需要记住的「拆分节家族」从 `.ldata`/`.lbss`/`.lrodata` 三处归并统一为一处说明；`readelf` 段数从 5 降到 4，示例更短。
- **分类**：[MODERN]（把同一脚本已经采用的归并规则补齐到遗漏的兄弟节，不改变外部行为与 C 语义）。
- **裁决**：接受，进入下一 code batch。理由：孤儿落点由链接器版本决定，现有断言不禁止它，而脚本已经为 `.ldata`/`.lbss` 建立了同一归并规则；代价是三份脚本各一行通配、重建三架构工件并重跑 `check-layout.sh`。
- **置信度**：高。
- **是否需源码核实**：已核实工件、脚本与断言输出；未在改造后重建工件。
- **若我错了**：如果 LLVM 有意把 `.lrodata` 独立成段以便内核给它不同的页权限，那么收进 `.rodata` 会削弱权限区分。但当前内核按 VMA 范围建表，`harden_kernel_text` 只把 `.text` 段改成可执行，其余非文本页统一读写，没有任何代码读取 `PT_LOAD` 的 `p_flags` 来给 `.lrodata` 单独设权限；因此这个反例在当前实现里不成立。

## FINDING DESIGN-D2-06（P2）

- **位置**：`os/kernel/src/arch/x86_64/link.ld`、`os/kernel/src/arch/aarch64/link.ld`、`os/kernel/src/arch/riscv64/link.ld`、`os/kernel/tests/link_ld_lockstep.rs`；文档锚点 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:359`（第三种形态）、`:1209–1213`（§6.2 的 `grep -c ': AT('`）、`:1278`（§7.3 源码清单）。
- **严重度**：P2。
- **问题**：三份 arch `link.ld` 没有生产链接读者，唯一的读者是 `link_ld_lockstep.rs`；该测试只比较两份脚本里的两个常量，不证明这些脚本能被链接。三份脚本的内容已经与生产布局脱节：没有 `ENTRY(_start)`，`.text` 没有 `KEEP(*(.text.boot))`，栈是 16 KiB 的 `k_initial_stktop`（生产是 64 KiB 的 `kernel_boot_stack_bottom/top`），没有末尾 2 MiB 收口，没有 `__bss_end` 符号，`kern_size`/`k_initial_stktop` 的 `PROVIDE` 符号在 Rust 侧没有读者。文档把它们称为「记录预期布局」，但它们连当前布局的入口段与栈宽都没有记录。
- **影响**：读者按 §3.1 的「第三种形态」去读这些脚本，会得到与实际生产脚本冲突的栈大小、入口段与收口规则；维护者为了「保持常量锁步」要同时改两份布局。它是「同型复制 + 只锁两个常量」的维护面，不是语义层。
- **证据**：
  - `grep -rn 'src/arch/.*link\.ld' os --include=*.rs --include=*.toml --include=*.sh` → 6 处命中，5 处是注释（`os/kernel/src/lib.rs:6329`、`:6360`、`os/arch/src/arch/direct_map.rs:244`、`:296`、`:350`），1 处是 `link_ld_lockstep.rs` 的测试头注释；没有构建输入。
  - 三份脚本都没有 `ENTRY(`、`KEEP(*(.text.boot))`、`kernel_boot_stack_*`、`ALIGN(0x200000)`；`grep -n 'k_initial_stktop' os --include=*.rs --include=*.ld` 只命中这三份脚本本身。
  - `os/kernel-image/check-layout.sh` 的 L0–L9 只读 `os/kernel-image/*.ld` 与实际工件，不读 arch 脚本。
- **最小修复（两个方案，优先减法）**：
  - **删**：删除三份 arch `link.ld` 与 `link_ld_lockstep.rs`；文档 §3.1 删掉「第三种形态」那一节与表格对应行，§6.2 的第二组命令改为只统计 `os/kernel-image/*.ld`，§7.3 删除三项。生产脚本加 `check-layout.sh` L0–L9 已经覆盖常量、首段 VMA/LMA、平移一致、2 MiB 收口、入口与引导栈；没有可执行不变量丢失。
  - **留并修**：若要保留教学样例，必须补齐 `ENTRY(_start)`、`.text.boot` 的 `KEEP`、64 KiB 引导栈符号、2 MiB 收口，并把 `k_initial_stktop`/`kern_size` 的旧符号删掉；工作量大于删除，而且之后仍要维护两份布局。
- **减法替代**：删除整个「无读者脚本 + 常量锁步测试」层次，不新增任何替代结构。生产脚本本身是唯一真相。
- **三问**：非类型级提案，不适用。
- **教学概念账**：删除 3 个文件与 1 个测试；文档的「三种链接脚本形态」简化为「生产镜像带 `AT()`」与「载体单地址」两种真实形态，读者少记一种没有运行读者的形态。
- **分类**：[DEL]（纯删除死文件与只锁常量的测试）。
- **裁决**：需用户裁决。理由：事实层面无生产读者，删除不改变任何可执行不变量；代价是三份脚本与一个测试文件的删除，以及文档 §3.1、§6.2、§7.3 的改写。若项目把它们当教学材料，则应挪到文档附录而不是留在源码树里继续冒充布局。
- **置信度**：高（事实层面）；删除是否可接受取决于项目是否把它们当教学材料，故裁决交给用户。
- **是否需源码核实**：已核实无构建读者、脚本内容与测试读者。
- **若我错了**：如果仓库外有工具（发布脚本、教学材料生成器）读取这三份脚本，删除会破坏外部读者；本仓内没有这样的工具，且文档 §3.1 自己说「测试只锁常量，不证明这些脚本被链接」。若需要外部教学材料，可以把它们移到 `book/` 或文档附录，而不是留在源码树里冒充链接输入。

## FINDING DESIGN-D2-07（P3）

- **位置**：`os/kernel/src/boot/higher_half.rs:48`（`unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> !`）；调用点 `os/kernel/src/lib.rs:232` 与 `:370`；文档锚点 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:572–582`。
- **严重度**：P3。
- **问题**：`stack_top` 形参与 `kinfo` 里的派生值同源：两个生产调用点都写 `H::jump_to_kmain(kinfo, kinfo.kern_stack_top())`。这是一个类型上可表示、语义上非法的状态——调用者可以传一个与 `kinfo` 无关的栈顶，而类型、断言与代码检查都不拦。文档在 §3.4 末尾已经承认它「是可表示的冗余值」，并把它归到「设计批（D2-04）」。
- **影响**：当前没有实际故障，因为两个调用点都写对了；风险在未来的第三个调用点或测试 mock 可能传入任意值，而 `arch_boot_finish` 与 `resume_additive` 的「栈顶 = 镜像尾」不变量只存在于调用点的写法里。
- **证据**：
  - `grep -n 'jump_to_kmain' os/kernel/src --include=*.rs` → 生产调用只有 `lib.rs:232`、`:370` 两处，都是 `kinfo.kern_stack_top()`；trait 定义在 `higher_half.rs:48`。
  - 文档 `:582`：「两个生产调用点都从 `kinfo.kern_stack_top` 取值，单独传一个不同的值不会被类型或代码检查拦住；把它并回 `kinfo` 属于设计批（D2-04）」。
- **最小修复**：删掉 `stack_top` 形参，trait 方法变成 `unsafe fn jump_to_kmain(kinfo: &KernelInfo) -> !`；三份实现的第一条指令前取 `let stack_top = kinfo.kern_stack_top();`（或给 trait 加一个默认方法，内部调用私有 `jump_with_stack(kinfo, kinfo.kern_stack_top())`）。两个调用点同步删掉第二个实参。安全段把「只调用一次」改写为「从引导 CPU 调用，不得重入；同线程的第二次调用已被 `-> !` 排除」。
- **减法替代**：这是纯减法（删一个形参、两个实参表达式），不新增类型。若未来真的需要与镜像尾不同的栈顶（例如预留 guard gap），那时再加一个 `StackTop` token 并由 `KernelInfo` 唯一构造；当前没有这样的用例，不提前加。
- **三问**：非新增类型，不适用；但按 §3.5 ② 自查：borrow checker 与类型系统都拦不住「第二个实参随便传」，删除形参是唯一不增加概念的拦法。
- **教学概念账**：新增 0；读者少记一个形参，trait 的安全契约从「四条前置」变成「三条前置 + 一条运行时不可重入说明」。
- **分类**：[MODERN]（纯内部签名简化，不改变外部行为）。
- **裁决**：接受为已登记项。理由：两个生产调用点同源，删除形参不改变任何运行时行为；代价是 3 份实现与 2 个调用点各改一行，收益是非法状态消失。实施前需要用户确认它归入哪个设计批：`decisions-snippet.md:8` 的 D-049 把 D2-04 记为「删 wire 派生标量」。
- **置信度**：高（事实与减法可行性）；编号归属需要用户确认：`decisions-snippet.md:8` 的 D-049 把 D2-04 记为「删 wire 派生标量」，而文档 `:582` 把「把 `stack_top` 并回 `kinfo`」也记为 D2-04，两者是不是同一个设计批需要核对。
- **是否需源码核实**：已核实两个调用点与 trait 签名。
- **若我错了**：如果某个测试载体需要一个与 `kinfo` 不同的栈顶来验证「非法栈顶会被拒绝」，那说明当前契约还需要一条负控，而不是保留形参；负控应放在 `kmain_verify` 的 `sp_ok` 断言上。

## FINDING DESIGN-D2-08（P3）

- **位置**：`os/libs/minix-elf/src/lib.rs:123–124`（`p_align`）、`:150`（`LoadSegment::align`）、`:441`（拷贝）、`:596`（唯一断言）；`os/kernel/src/lib.rs:6179–6198`（`HIGHER_HALF_CALLED` 与 mock 实现）；`os/kernel/src/boot_policy.rs:107–120`（`virtual_end` 死计算）。
- **严重度**：P3。
- **问题**：三处「零读者」代码。
  1. `Elf64Phdr.p_align` 被解析、存进结构体、再拷进 `LoadSegment.align`，生产代码没有任何读者；唯一读者是测试 `os/libs/minix-elf/src/lib.rs:596` 的 `assert_eq!(buf[0].align, 0x1000)`。文档 §3.2（`:445`）自己写「`p_align` 保留了但当前没有读者」，与本篇「字段只保留本仓要读的」的设计规则冲突。
  2. `HIGHER_HALF_CALLED` 只在 `MockHigherHalf::jump_to_kmain` 里写一次，而该方法从未被调用；`test_higher_half_trait_is_implementable` 只做类型约束检查。mock 的文档注释还写着旧签名 `fn jump_to_kmain(kinfo: &KernelInfo) -> ! (no &self)`，与当前带 `stack_top` 的签名不符。
  3. `plan_kernel_mapping` 里 `virtual_end` 只用于 `checked_add` 溢出检查，随后 `let _ = virtual_end;`；`A::Paging::validate_virtual_range(layout.virtual_base, layout.size)` 已经做同样的 `checked_add` 与 canonical 检查，这个局部值是死计算。
- **影响**：读者会以为 `p_align` 参与了布局或权限决策；会以为 mock 真的验证了调用契约；`virtual_end` 让读者以为虚拟区间有独立的溢出检查。三处都不改变运行时行为，但增加理解成本。
- **证据**：
  - `grep -rn '\.align' os --include=*.rs | grep -v os/target` → 只有 `minix-elf` 测试的 `buf[0].align` 一处，其余是无关的 `Layout::align`/asm `.align`。
  - `grep -n 'HIGHER_HALF_CALLED' os/kernel/src/lib.rs` → 定义 `:6179`、写入 `:6193`，没有读取点。
  - `sed -n '107,120p' os/kernel/src/boot_policy.rs` → `let virtual_end = ...; let _ = virtual_end;`。
- **最小修复**：删除 `Elf64Phdr.p_align`、`LoadSegment.align`、`parse_phdr` 的 8 字节读取与 `test_segment_iter_valid` 的 `align` 断言；删除 `HIGHER_HALF_CALLED`、mock 方法体里的写入与过期签名注释（保留空实现的类型检查）；删除 `virtual_end` 与 `let _ = virtual_end;`，把溢出检查留给 `validate_virtual_range` 与 `physical_end` 分支。
- **减法替代**：纯删除，不新增任何替代结构。
- **三问**：非新增类型，不适用。
- **教学概念账**：减少三个读者需要理解的「看起来有用」的字段/变量；解析器的输出结构少一个字段，`kernel` 的启动策略函数少一个局部值。
- **分类**：[DEL]（零读者字段、死变量、死测试脚手架）。
- **裁决**：接受，低优先。理由：三处都无生产读者，删除不改变外部行为；代价是解析器结构少一个字段、一条测试断言与两处死变量，需要同步更新 §3.2 的「保留 p_align」一句。
- **置信度**：高（事实）；删除 `p_align` 是否会影响未来要加的「段对齐校验」属于推测，当前没有该需求。
- **是否需源码核实**：已核实 grep 与源文件。
- **若我错了**：如果 `p_align` 被计划用于用户 ELF 的 `PT_LOAD` 对齐检查（当前 `load_elf_into` 没有用），那应保留字段并补测试与调用点，而不是以「将来可能用」为由留在结构体里；「将来可能用」正是 §3.5 要拒绝的推测性设计。

## FINDING DESIGN-D2-09（P3）

- **位置**：`os/libs/minix-boot/src/kernel_info.rs:13–20`（注释明确说不是 ABI，但结构体是 `#[repr(C)]`）、`:571`（`MemoryRegion`）、`:610`（`BootModule`）。
- **严重度**：P3。
- **问题**：`KernelInfo` 的注释写「`#[repr(C)]` pins the field order … It is NOT a cross-image layout contract」，`MemoryRegion` 与 `BootModule` 也没有跨镜像读者；wire 布局由 `os/libs/minix-boot/src/handoff_v1.rs` 的 `#[repr(C)]` 结构体单独承担。三处 `#[repr(C)]` 是没有 ABI 读者的表示约束，属于「只为将来可能的 FFI 保留」的推测性设计。
- **影响**：不改变运行时行为；但会让读者以为这三个结构与 wire/FFI 有布局契约，也会阻止 rustc 做字段重排优化（`KernelInfo` 含多个胖指针，重排可能减少填充）。真正需要 `repr(C)` 的 `handoff_v1.rs` 与 `platform.rs` 的 wire 类型不受影响。
- **证据**：
  - `grep -rn 'offset_of!.*KernelInfo\|transmute.*KernelInfo\|KernelInfo.*as_bytes' os --include=*.rs` 无命中；`KernelInfo` 没有按字节序列化/反序列化的读者。
  - `os/libs/minix-boot/src/handoff_v1.rs` 的 wire 结构体保留 `#[repr(C)]`，与 `KernelInfo` 是两套类型。
- **最小修复**：删除 `KernelInfo`、`MemoryRegion`、`BootModule` 的 `#[repr(C)]`；保留 `handoff_v1.rs` 中真正按字节跨镜像的 `BootHandoffV1` 与 payload 结构。`PlatformDescSource`（`platform.rs:123`）是否按字节跨镜像需要单独核实，本报告不把它计入删除面；若它只是内存中的纯数据值，也可以一并去掉。若调试器需要稳定字段顺序，改为在文档里写字段顺序，不用 ABI 属性表达。
- **减法替代**：纯删除三个属性，不新增替代。
- **三问**：非新增类型，不适用。
- **教学概念账**：减少「这个结构是 ABI」的误解；读者只需要在 wire 类型上理解 `repr(C)`。
- **分类**：[DEL]（无读者的表示约束）。
- **裁决**：接受，低优先。理由：仓内没有按字节读取这三个结构的代码，删除不改变行为；代价是三个属性与一句注释，若外部调试器依赖字段偏移则改为拒绝。
- **置信度**：高（仓内无读者）；中（若仓库外调试脚本依赖字段偏移，则不能删）。
- **是否需源码核实**：已核实仓内无 `offset_of!`/`transmute` 读者。
- **若我错了**：如果外部调试器或崩溃转储解析器读取 `KernelInfo` 的固定偏移，保留 `#[repr(C)]` 并把注释改成「调试 ABI」；当前仓内没有这样的读者，先删除更符合最小化。

## FINDING DESIGN-D2-10（P2）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:522`（§3.3「只带两个基元值」）、`:1111` 与 `:1120`（§5.1 测试计数与测试名）、`:1148`（§5.2 载体取值）。
- **严重度**：P2。
- **问题**：三组剩余的事实漂移。
  1. §3.3 说「交接仍然只带两个基元值——`KernelInfo.kern_virt_base` 与 `KernelInfo.kern_size`」。v1 wire 实际带三个布局标量：`kern_virt_base`、`kern_phys_base`、`kern_size`（`os/libs/minix-boot/src/handoff_v1_read.rs:74–77`）；`KernelInfo` 也保留 `kern_phys_base`，riscv64 的 `delta` 与内核建表都要读它。正确说法是「栈顶派生只需要两个值」。
  2. §5.1 的复算命令输出已经变化：`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` 输出 27（文档写 23），`grep -c '#\[test\]' os/boot-shim/src/loader.rs` 输出 31（文档写 22）。测试表里的 `test_segment_iter_skips_segment_data_out_of_bounds` 已不存在，现在的名字是 `test_segment_iter_reports_segment_data_out_of_bounds`，另新增了 `test_segment_iter_filesz_exceeds_memsz_is_err`、`test_segment_iter_rejects_filesz_greater_than_memsz`、`test_segment_iter_truncated_mid_segment_is_err`、`test_elf_file_end_*` 与 `test_load_boot_modules_into_trims_elf_to_file_end` 等。
  3. §5.2 的 x86_64 载体示例把 `kern_size: 0x200_000` 写了两遍，且两个值都已过期：`os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:51–62` 现在是 `kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`，`image_base` 来自 `uefi_helpers::shim_image_window()` 的实际镜像窗口，不是固定的 `0x200_000`。
- **影响**：读者按文档的计数与测试名去核对测试会找不到目标；按 §3.3 的「两个值」理解 wire 会漏掉 `kern_phys_base`；按 §5.2 的固定 `kern_size` 去复现真机日志会与载体实际打印不符。
- **证据**（本会话已重放）：
  - `grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` → 27；`grep -c '#\[test\]' os/boot-shim/src/loader.rs` → 31。
  - `grep -n 'fn test_' os/libs/minix-elf/src/lib.rs` → 名称列表，不含 `..._skips_segment_data_out_of_bounds`。
  - `sed -n '51,62p' os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs` → `kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`。
  - `sed -n '74,77p' os/libs/minix-boot/src/handoff_v1_read.rs` → 三个布局标量。
- **最小修复**：三处文档改写；§5.1 的两个计数改成实测值，测试名按当前 `grep` 输出重抄；§5.2 的示例改成 `kern_phys_base: PhysBytes(image_base)` 与 `kern_size: image_end - image_base`，并说明这是 I-43 之后按真实镜像窗口取值。另建议把 §3.3 的「公式只留在访问器一处」改成「生产消费点只经访问器；`boot_validate_and_prepare` 为对齐断言保留一次本地展开」（`os/kernel/src/lib.rs:938–940` 确实是第二份展开）。
- **减法替代**：纯文档修改；测试计数与测试名以命令输出为准，不新增清单文件。
- **三问**：非类型级提案，不适用。
- **教学概念账**：不新增概念；读者看到的测试数与测试名与仓库一致，减少一次「按文档找不到测试」的排查。
- **分类**：[MODERN]（文档对齐当前实现与测试）。
- **裁决**：接受，本轮修文档。理由：三处都是命令输出可直接证伪的事实；代价是 §3.3、§5.1、§5.2 的少量改写，0 行代码改动。
- **置信度**：高。
- **是否需源码核实**：已核实；命令与行号见上。
- **若我错了**：如果文档的计数来自某个特性组合下的测试运行（例如默认特性与 `--all-features` 不同），应以文档自述的复算命令为准；当前按文档给出的命令执行就得到 27/31，说明文档的「输出 23/22」已经过期。

## ARCH-PROPOSAL

本轮无 ARCH 级提案。

理由：上述十条发现都可以锚定到现有 C 语义或本仓已有的设计意图——文档同步、删除死文件、补齐 `.lrodata` 的既有归并规则、复用 `BootReport` 的命名视图、删除冗余形参与零读者字段。没有一条替换机制或新增责任，因此不触发 D-047 的 ARCH 硬阻断条件。若后续有人要把 `KernelInfo` 生命周期参数化或把三套启动形态统一成运行时状态机，那属于机制替换，应另开 ARCH 提案并附三处一致标注与用户裁决，不在本轮范围。

## VERDICT

- **总体判断**：文档 §3 的五条设计决策与 §4 的实现主体是一致的：`HigherHalf` 的 trait 边界、`kern_stack_top` 的派生公式、`p_paddr` 摆放、page-size 阶梯、`.bss` 副本交接都能在源码里找到对应物；`trait-audit.json` 也证明 `HigherHalf`（4 个实现、32 处引用）与 `FileLoader`（3 个实现、16 处引用）不是单实现依赖注入。主要问题不是设计方向错，而是文档与源码在 D-082、D-084、wire v10 之后没有同步，以及两处可以纯减法收掉的维护面。
- **本轮建议处置**：
  - 接受并本轮修文档：DESIGN-D2-01（P1，§3.2 错误模型）、DESIGN-D2-02（P2，模块修剪）、DESIGN-D2-03（P2，startup 分区）、DESIGN-D2-10（P2，计数/取值/措辞）。
  - 接受并进入下一 design/code batch：DESIGN-D2-04（P2，复用 `BootReport` 消身份混淆与 12 处 `from_raw_parts`）、DESIGN-D2-05（P2，`.lrodata` 归并）。
  - 需用户裁决：DESIGN-D2-06（P2，删除三份无读者 arch `link.ld` 与 `link_ld_lockstep.rs`，或保留并补齐）。它不阻断文档收敛，但决定源码树是否继续维护一份过期布局。
  - 接受为已登记项、低优先：DESIGN-D2-07（P3，删 `stack_top` 形参；编号归属需核对 D-049 的 D2-04 定义）、DESIGN-D2-08（P3，零读者字段/死脚手架）、DESIGN-D2-09（P3，`repr(C)` 无 ABI 读者）。
- **拒绝的加法**：不给 VFS 的 `ElfHead` 合并共享 parser；不给 `HigherHalf` 加 `dyn`/关联类型；不给 `place_modules` 加结构化错误枚举；不把 `KernelInfo` 生命周期参数化；不给 `KernelInfo` 加 builder；不给 `rust_image_main` 加 `HandoffMode` 枚举。理由都在 SUBTRACTION-TABLE 的「减法拦不住的理由」列：这些加法要么改变既有契约，要么增加概念而不提高保障。
- **同 agent 验证局限**：本报告是单 agent 的只读审查，不是跨工具 VERIFY-CHECK。所有 grep、`sed`、`readelf`、`check-layout.sh` 命令都在本会话重放过；未运行 QEMU 真机链（`os/qemu-tests/run_qemu.sh` 需要对应固件与工具链），也未在改造后重新编译。自评一致性 92%，判 PASS（同 agent 局限已标注）。剩余 8% 主要来自 `build_kernel_info` 修法的借用检查细节与 `.lrodata` 归并后的实际跨距，需要编译或重建工件才能定案。
- **若本轮只能做一件事**：先修 DESIGN-D2-01。它是唯一一处「文档描述的安全行为与代码相反」的 P1；其余发现要么是文档计数，要么是已登记或需裁决的减法项，不会误导读者对引导路径安全性的判断。

## 完成声明

- 已读输入：目标文档全文（1289 行，重点第 329–1103 行）、`related-code.txt` 的 29 个文件、`tools/doc-code-map.sh` 的 30 项、`trait-audit.json`（按 30 项范围过滤后 23 条，其中 `HigherHalf` 与 `FileLoader` 为本篇相关项）、`AGENTS.md`、`decisions-snippet.md`（D-013/D-014/D-015/D-024/D-047/D-049/D-055/D-063/D-064/D-067/D-071/D-082/D-083/D-084/D-085/D-086/D-088/D-089/D-090/D-094/D-095/D-104）。
- 未读输入：历史评审结论、其他面板输出、任何 `expected/` 目录。未修改任何源码、文档或测试；唯一写入是本文件。
- 发现统计：FINDING 10 条（P1×1、P2×6、P3×3），ARCH-PROPOSAL 0 条；减法提案 10 行（采纳 3 行、需用户裁决 1 行、拒绝 6 行）。
- 命令复核：`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs`（27）、`grep -c '#\[test\]' os/boot-shim/src/loader.rs`（31）、`readelf -SW ... | grep -c '\.lrodata'`（x86_64 986、aarch64 0、riscv64 0）、`readelf -lW ... | grep -c '^  LOAD'`（x86_64 5、另两个 4）、`SKIP_BUILD=1 bash os/kernel-image/check-layout.sh {x86_64,aarch64,riscv64}`（三者全 PASS）、`grep -rn 'src/arch/.*link\.ld'`（6 处，全部为注释或测试），均在本会话重放。
- 未完成/受限：未运行 QEMU 真机链；未在改造后编译 `build_kernel_info` 或 `.lrodata` 归并；`BootStorageCell::take(&'static self)` 的借用形状需要编译器确认（若不成立，按 FINDING DESIGN-D2-04 的退化方案改为 `MemoryTables` 值对象）。
- 结论：D1 未运行（D-024），本报告为回退形态；设计主体与实现一致，不阻断文档收敛；P1 文档项本轮可修，P2 减法项进入下一批次，唯一需用户裁决的是三份无读者 arch `link.ld` 的存废。
