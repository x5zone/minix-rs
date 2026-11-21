# D02 R4 面板 · READER-A 冷读记录

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 读者画像：`/home/xzhao/github/minix-rs/tmp/review-eval/01-stage-kernel/02-higher-half-kernel/reader-profile.md`
- 阅读范围：只读上述两个文件；未读源码、其他文档、diff、历史版本、工作流规则、作者解释、其他 agent 输出、`expected/`。
- 清单口径：`review-evals.md` §4.0e 属工作流规则，按任务书禁读；下列条目的“清单条目”按语义归类，编号请持有规则文件的面板补。

## FINDING READER-A-01（P2）

- 位置：§3.2「迭代器在 `next()` 里做类型过滤与范围检查，命中这些条件时 `continue`（不产出该段）；但 `SegmentIter::new` 与 `parse_phdr` 的表范围算式是未检查加法，畸形输入会 panic 而不是 `Err`（代码修复登记设计批 D2-01）」；同节代码 `let phdr = match parse_phdr(self.image, offset, self.ehdr.e_phentsize) { Ok(p) => p, Err(_) => continue, };`；同节「第三类是越界检查，它的动作也是跳过——一个段声明的文件数据超出了镜像长度时，迭代器不报错、不截断，直接不产出这个段。」；§4.2 代码注释「I-48 / D-084: a malformed program header is an `Err` item, not a silent skip; `filesz > memsz` is rejected inside the iterator.」；§4.2 正文「循环体内先取 `seg_result?`（I-48 / D-084：畸形程序头由迭代器以 `Err` 项浮出，`filesz > memsz` 的守卫也已收进迭代器）」。
- 严重度：P2。
- 问题：同一批“畸形/越界”输入，§3.2 给出三种互斥结局——表项解析错误走 `continue` 跳过、表范围算式可能 panic、段数据越界也走 `continue` 跳过；§4.2 却说“畸形程序头”以 `Err` 项浮出、不是静默跳过。我无法判断 `compute_kernel_layout` 里的 `seg_result?` 到底会不会被触发，也无法判断“段比预期少”是当前设计的已知缺口（§3.2 的 D2-05 段）还是已被 I-48/D-084 修掉。
- 影响：§3.2 的“三类 `continue`”解释与 §4.2 的 `found_load`/`?` 解码互为前提；两处读法不同，读者对“装载器遇到坏段会怎样”的判断会完全相反。§5.1 的“越界段被跳过”与 §5.4 M6 的“每种畸形输入返回对应变体”也分别站在两边，读者无法用测试清单消除歧义。
- 证据或推理：只引文档内句子。§3.2 的 `next()` 摘录明确把 `parse_phdr` 的 `Err` 写成 `Err(_) => continue`，并把段数据越界写成 `continue`；§4.2 的代码注释明确写 `a malformed program header is an Err item, not a silent skip`，正文又写“畸形程序头由迭代器以 `Err` 项浮出”。这两句不可能同时描述同一个 `parse_phdr` 失败。可能的调和是“`filesz > memsz` 是唯一被包装成 `Err` 的畸形”，但文档没有画出这条边界；它在 §3.2 用“畸形输入”一词同时覆盖了 panic 与 continue，在 §4.2 又用“畸形程序头”覆盖 Err。另，§3.2 还说 `SegmentIter::new`/`parse_phdr` 的未检查加法会 panic 而不是 `Err`，这使“畸形输入会怎样”出现第三种结局。
- 最小修复：在 §3.2 把条件拆成三类并各给一个名字：(a) 表项范围/解析失败 → `continue`（说明为什么这是防御性跳过）；(b) `filesz > memsz` → 迭代器产出 `Err` 项；(c) 表范围算式溢出 → panic（D2-01 待修）。然后把 §4.2 的注释改成“`filesz > memsz` 的段是以 `Err` 项浮出的，不是静默跳过”，并把 §5.1 的“越界段被跳过”限定为“文件数据越界的段”，把 §5.4 M6 的“每种畸形输入返回对应变体”改成“三类畸形各按上述路径处置”。若 `test_compute_kernel_layout_rejects_image_without_load_segments` 确实存在，也应加入 §5.1 的十三个测试清单，或注明它在十三项之外。
- 置信度：高（0.85）——文档内部措辞已足以判定两处不能同时为真；具体哪一句是旧版留存，需要源码核实。
- 是否需源码核实：需要。要确定当前真实行为，必须读 `os/libs/minix-elf/src/lib.rs` 的 `SegmentIter::new`/`next`/`parse_phdr` 以及 `os/boot-shim/src/loader.rs` 的 `compute_kernel_layout`；这超出本读者的禁读范围，留给有源码权限的角色。
- 若我错了：如果 §3.2 的 `next()` 摘录是旧版，而当前 `next()` 已把 `parse_phdr` 的错误改成 `Some(Err(...))`（即 `Err(_) => continue` 已不存在），那么 §3.2 的代码摘录与“三类 continue”正文需要更新；读者仍需要文档标明版本，否则无法从文本判断。
- 读者验收清单条目：§4.0e 不可读（工作流规则在禁读范围）；按语义归类为「代码与正文一致 / 错误语义可判定」；编号请持有规则文件的面板补。

## FINDING READER-A-02（P2）

- 位置：§6.2「L4 的平移量按架构不同：x86_64 是 `0xffff800000000000`，aarch64 是 `0xffff7fffbfe00000`，riscv64 是 `0xffffffbf7fe00000`」与同节「riscv64 测试载体不是 `nm` 值本身：它链接在物理地址上，`$rip` = `nm` 值 + `delta`（`delta = kern_virt_base − kern_phys_base`），例如加载基址 `0x80200000` 时 `nm` 给出 `0x80200160`，而 `$rip` 是 `0xffffffc000200160`」；§4.3「按视图差 `kern_virt_base − kern_phys_base = 0xffffffbf7fe00000` 复算」。
- 严重度：P2。
- 问题：按文档自己给出的 delta 做加法，`0x80200160 + 0xffffffbf7fe00000 = 0xffffffc000000160`，与文档写的 `0xffffffc000200160` 相差 `0x200000`。我按第五组配方在 GDB 里断到 `kmain`，会看到前者；文档让我核对后者，我会以为跳转目标或装载基址错了。
- 影响：§6.2 第五组是“跳转与切栈都已生效”的现场证据；预期值错 `0x200000` 会让读者在本来正确的现场里追一个不存在的故障。它还会让读者怀疑 §4.3 里同一个 delta 的数值，进而怀疑 `delta` 表的整行。
- 证据或推理：只引文档内句子。§6.2 明写 `delta = kern_virt_base − kern_phys_base`，且 riscv64 的平移量是 `0xffffffbf7fe00000`；§4.3 又写“视图差 `kern_virt_base − kern_phys_base = 0xffffffbf7fe00000`”。按 §4.3 表里 `PA = VMA − delta` 的等价式 `VMA = PA + delta`，把加载基址 `0x80200000`、`nm` 值 `0x80200160` 代入，得到 `0xffffffc000000160`。文档的 `0xffffffc000200160` 对应的 `nm` 值是 `0x80400160`，不是同句里的 `0x80200160`。
- 最小修复：把例子里的 `$rip` 改成 `0xffffffc000000160`；若实际读数确实是 `0xffffffc000200160`，则把 `nm` 值改成 `0x80400160` 并把加载基址写成 `0x80400000`，同时检查 §4.3/§6.2 的 delta 是否应改。三处（加载基址、`nm` 值、`$rip`）必须满足同一条加法，建议在句末附一行算式。
- 置信度：高（0.9）——加法本身确定；具体哪一个数字是笔误，需要一次 GDB 读数确认。
- 是否需源码核实：纯算术不需要源码；要确定真实的 `nm`/`$rip` 读数，需要跑一次 QEMU/GDB（超出本读者禁读范围，留给有源码权限的角色）。
- 若我错了：如果 `nm` 给出的 `0x80200160` 不是 `kmain` 而是另一个符号，或加载基址实际是 `0x80400000`，则文档可能自洽；但原文把这三个数放在同一句里，按原文读法我无法得到 `0xffffffc000200160`。
- 读者验收清单条目：§4.0e 不可读（工作流规则在禁读范围）；按语义归类为「命令与预期输出可复现 / 数字一致」；编号请持有规则文件的面板补。

## SUGGESTION READER-A-S01

- 位置：§4.2「这段差异是本篇承认的可测试性缺口，见 §5.1 与 follow-ups 的 D02-F04」「直接调用这个 `pub unsafe fn` 的读者仍必须先校验布局（D02-F05）」；同节「缓冲区版在 I-47 后是两循环」；§4.4「真机读数：同一 VA 读回全零，E2 现象」；§5.4 M11「NK4-A 的寄存器覆写修复与注释」；§3.2「代码修复登记设计批 D2-01」「这是已知设计缺口（设计批 D2-05）」「属于设计批 D2-06」；§3.3「v10（D-062）之前」；§4.4「删除或加 feature 门属于设计批（D2-13）」。
- 问题：这些 ID 在全文没有定义、没有索引；`follow-ups` 在本文里也没有对应章节。我能把它们当作不透明标签跳过，但它们常被用来表达“已修 / 待修 / 范围外”，跳过之后就无法判断某个行为是当前实现还是待办。
- 影响：读者在 §3.2、§4.2、§4.4 反复遇到无法解析的指针，读到这里只能选择忽略；这会让“已知设计缺口”与“已修复”两条线混在一起。
- 证据或推理：只引文档内句子。上面五组引文都在正文出现，但没有一处给出 ID 的含义或状态；`follow-ups` 只在 §4.2 出现一次，§7 的参见也没有它。另，§3.2 提到 `NoLoadSegments` 有 `test_compute_kernel_layout_rejects_image_without_load_segments`，而 §5.1 的“十三个”直接相关测试清单没有这个名字，读者按 §5.1 对账会以为它不存在。
- 最小修复：在 §1.3 或 §7 加一张“本文引用的外部 ID”小表（ID / 一句话含义 / 状态：已修/待修/范围外 / 首次出现节），或在每个 ID 首次出现处括注一句白话。把 `follow-ups` 换成实际章节或文件名；把 `test_compute_kernel_layout_rejects_image_without_load_segments` 补进 §5.1 的清单，或注明它在“十三个”之外。
- 置信度：高（0.9）——这些字符串在文档内确实没有定义；具体每个 ID 的含义需要源码/设计批档案核实。
- 读者验收清单条目：§4.0e 不可读（工作流规则在禁读范围）；按语义归类为「术语/指针首次出现有定义」；编号请持有规则文件的面板补。

## SUGGESTION READER-A-S02

- 位置：§2.3「源码注释写作 `Terminate stack`——它是初始栈底的终止槽，与回溯链的终止点是两件事……压进栈的这个 0 槽不在帧指针链上（Rust 侧保留它的理由见 §4.3）」；§4.3「`push 0` 仍是初始栈的终止槽（C 注释 `Terminate stack`），不在帧指针链上」。
- 问题：文档两次说这个 0 槽“不在帧指针链上”，也两次把“为什么保留”推给下一处；但 §4.3 只重复“它是终止槽、不在链上”，没有说谁消费这个槽。若它只是 `sub rsp, 8` 之前的 8 字节垫片，直接说“它当前只承担对齐，不承担回溯终止”即可；若它服务于某种扫描式回溯，应点名那个回溯器。
- 影响：我在 §2.3 的逐行解码里被引到一个不闭合的解释，容易怀疑自己漏了某个调用栈机制；这会削弱“设栈 + 跳转必须成对”这条主线的说服力。
- 证据或推理：只引文档内句子。§2.3 明写“与回溯链的终止点是两件事”，随后又说“本仓两侧的回溯器都沿帧指针链走”，并说“压进栈的这个 0 槽不在帧指针链上”；§4.3 的对应句只重复“仍是初始栈的终止槽……不在帧指针链上”。两处都没有给出消费者，也没有说“没有消费者”。
- 最小修复：在 §4.3 的 `push 0` 解释后补一句，二选一：①“当前两侧帧指针回溯都不读这个槽；保留它只为与 C 侧形制一致，实际可观测作用只有 `sub rsp, 8` 之前的 8 字节垫片”；②“它由 X 读取（X 是栈扫描/调试器/…），帧指针回溯不经过它”。
- 置信度：中高（0.75）——这是解释链的缺口，不是事实错误；具体消费者需要源码核实。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「解释链闭合 / 不变量-前提-证据三问」；编号请面板补。

## SUGGESTION READER-A-S03

- 位置：§3.1「riscv64 测试载体虽然也直接调 `arch_boot`，但它链接在物理地址上、`direct leg` 加 `delta`，跳转完成时 PC 落在高半 VMA（下表第 5 行）」；紧随的表第 5 个数据行是「x86_64/aarch64 测试载体（inline `arch_boot`，UEFI helpers） | 低地址视图 | 低地址」，riscv64 测试载体是第 3 个数据行。
- 问题：按读者通常的“数据行”计数，第 5 行是 x86_64/aarch64 测试载体，结论是“低地址”，与句子的“高半 VMA”相反；只有把表头与分隔行也算进去，riscv64 测试载体才恰好是表格源码的第 5 行。这个“第 5 行”没有说明计数口径。
- 影响：读者按指针核对会看到相反结论，进而怀疑整张“三套分类”表；这正是 §3.1 最需要读者信任的地方。
- 证据或推理：只引文档内句子。句子说“riscv64 测试载体……PC 落在高半 VMA（下表第 5 行）”；表的第 3 个数据行写“载体单地址脚本 | riscv64 测试载体（直接 `arch_boot`） | 物理视图 | 高半 VMA（direct leg 加 `delta`）”；第 5 个数据行写“载体单地址（工具链默认布局） | x86_64/aarch64 测试载体…… | 低地址视图 | 低地址”。
- 最小修复：把“（下表第 5 行）”改成“（下表 riscv64 测试载体那一行）”，或“（下表第 3 个数据行）”；若要保留行号，明确写“含表头的第 5 行”。
- 置信度：高（0.9）——指针指向的内容与句子相反，按数据行读法确定。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「交叉引用正确」；编号请面板补。

## SUGGESTION READER-A-S04

- 位置：§3.5 三档判据表「| 两者都不整除 | 无 | panic（消息只报 `no kernel page size satisfies the layout`，不点名三个量），发生在任何页表写入之前 |」与下一行「| 两者都不整除且回退档不受支持 | 无 | `FallbackUnsupported`（消息 `fallback page size … is not supported`），不是 `NoKernelPageSize` |」；同节代码 `} else if !fallback.supported() { return Err(KernelMapError::FallbackUnsupported(fallback.size())); } else { return Err(KernelMapError::NoKernelPageSize); }`。
- 问题：第 3 行的条件“两者都不整除”包含第 4 行的条件“两者都不整除且回退档不受支持”。按行序读，第 3 行会先命中并 panic；按代码读，更具体的第 4 行返回 `FallbackUnsupported`。表没有说“第 4 行优先”。
- 影响：读者按表诊断时，遇到回退档不受支持的镜像会预期 `no kernel page size satisfies the layout`，但实际消息是 `fallback page size … is not supported`；两行给出的是不同的排查方向。
- 证据或推理：只引文档内句子。表第 3 行的条件比第 4 行宽；代码的 `else if !fallback.supported()` 在最后的 `else` 之前，说明具体分支先判。这两处只有在“第 4 行是第 3 行的例外”时才能同时成立，但表没有写这个优先级。
- 最小修复：把第 3 行的条件改成“两者都不整除且回退档受支持”，或把第 4 行移到第 3 行之前并注明“先判更具体的回退档不受支持”；也可以在两行之间加一句“第 4 行是第 3 行的例外，优先命中”。
- 置信度：高（0.85）——条件包含关系与代码分支顺序都是文档内可判的；具体错误消息以代码为准。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「表格与代码一致 / 条件互斥」；编号请面板补。

## SUGGESTION READER-A-S05

- 位置：开头前置知识段「C 侧 i386 自举从“分页未开”开始（§2.3 的 `vm_enable_paging` 正是开分页的那一步）」；全文 `vm_enable_paging` 实际出现在 §2.2 的 `pg_utils.o` 反向推演里：「调用点在 `i386/pre_init.c` 的 `pre_init` 的函数体中部——那里依次调用 `pg_clear/pg_identity/pg_mapkernel/pg_load/vm_enable_paging` 五个函数」。§2.3 全节讲 `head.S` 的换栈与跳转，没有出现 `vm_enable_paging`。
- 问题：按开头的指针去 §2.3 找“开分页的那一步”，找不到；这个词只在 §2.2 出现。读者在全文第一段就遇到一个错指针。
- 影响：读者会怀疑后续所有 `§` 指针的可靠性，尤其在 §2.3/§2.4 这种依赖精确节号的判据链上。
- 证据或推理：只引文档内句子。开头写“§2.3 的 `vm_enable_paging`”；§2.3 的正文与代码摘录没有这个词；§2.2 的 `pre_init` 五步调用里第五个正是 `vm_enable_paging`。
- 最小修复：把开头的“§2.3”改成“§2.2”；若想强调“分页开启发生在 `pre_init` 返回之前”，可补一句“`pre_init` 的五步建表在 §2.2 末尾，`head.S` 的换栈在 §2.3”。
- 置信度：高（0.95）——节号与词的出现位置都是文档内可判的。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「交叉引用正确」；编号请面板补。

## SUGGESTION READER-A-S06

- 位置：§5.2「测试载体的 `KernelInfo` 是合成的（各载体同形、取值按架构；以 x86_64 载体为例：`main.rs` 里 `kern_virt_base: VirBytes(0xFFFF_8000_0000_0000)`、`kern_size: 0x200_000`、`kern_size: 0x200_000`），栈顶由 `kern_stack_top()` 现算为两者之和」。
- 问题：同一个字段名 `kern_size` 在同一个括号里出现了两次。读者会问第二个是不是 `kern_phys_base`，还是复制粘贴多了一份；若是前者，合成结构体的字段清单就少了一项；若是后者，`KernelInfo` 的字段顺序/取值被写重了。
- 影响：读者无法照这句复现合成 `KernelInfo`；而 §5.2 的 `sp_ok` 判据依赖 `kern_virt_base + kern_size`，字段写错会让读者对栈顶读数产生怀疑。
- 证据或推理：只引文档内句子。括号内三个条目依次是 `kern_virt_base`、`kern_size`、`kern_size`；下一句说“栈顶由 `kern_stack_top()` 现算为两者之和”，说明作者本意是两个量（基址 + 跨距），不是两个同名字段。
- 最小修复：确认第二个字段；若是 `kern_phys_base`，写成 `kern_phys_base: PhysBytes(0x200_000)`（或实际值）；若是重复，删掉一份。顺手在句末加一句“合成值不必等于生产值，只用于让 `kern_stack_top()` 落在高半”。
- 置信度：高（0.9）——重复是文档内可见的；具体第二个字段名需要源码核实。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「代码/数据摘录忠实」；编号请面板补。

## SUGGESTION READER-A-S07

- 位置：§4.4 状态表「保护结构 | **未建立**：`kmain` 的早期阶段（Phase A，完整划分见 `03-kmain-cstart.md`）只做校验与信息收存，保护结构 `init_protection`（GDT/TSS 或 VBAR_EL1/stvec）是 Phase B 的首件事——在那之前任何异常都无法处理 | `fn init_protection` 的文档注释（`This must be the very first thing called in kmain, because without valid GDT/TSS … any exception will cause an unrecoverable triple fault`）」。
- 问题：引文说 `init_protection` 必须是 `kmain` 里“第一件被调用的事”，正文说 Phase A 的校验在它之前、它是 Phase B 的首件事。读者不知道 Phase A 是否也在 `kmain` 内，也不知道“very first thing”是字面次序还是“第一条安装动作”。
- 影响：这张状态表的核心结论是“跳转完成时异常还不能被接住”；引文与正文的字面冲突会让读者怀疑这个结论是否只在某个版本的 `kmain` 里成立。
- 证据或推理：只引文档内句子。正文写“`kmain` 的早期阶段（Phase A……）只做校验与信息收存”“`init_protection`……是 Phase B 的首件事”；同格引文写“This must be the very first thing called in kmain”。若 Phase A 在 `kmain` 内且先执行，则引文的“very first”在字面上不成立；若 Phase A 在 `kmain` 之外，正文的“`kmain` 的早期阶段”又不成立。
- 最小修复：把引文换成只支持“保护结构未建立”的短句（例如只保留 `without valid GDT/TSS … any exception will cause an unrecoverable triple fault`），或加一句“Phase A 的校验不安装保护结构；`init_protection` 是 Phase B 的第一条安装动作，源码注释的 `very first` 指安装动作的次序，不是 `kmain` 的第一条语句”。
- 置信度：中（0.6）——这是引文与正文的字面张力，可能由“Phase A 不在 `kmain` 内”或“very first 指安装动作”解释；需要读 `kmain`/`init_protection` 才能定。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「引文与正文一致」；编号请面板补。

## SUGGESTION READER-A-S08

- 位置：§2.4 证据表第 2 行「栈定义在 `mpx.S` 的 `.bss` 段里（i386）；`earm/mpx.S` 的等价物在 `.data`（`k_initial_stack` 前后有 `.data`/`.bss`/`.data` 切换） | `k_initial_stack`（栈底标签所在处） | 排除“定义在某个专门的低地址段”——`.bss` 由链接脚本的哪一条输出段规则收，取决于对象名」；第 3 行「`kernel.lds` 的 `.unpaged_bss` 只收 `unpaged_*.o(.bss COMMON)`，通用 `.bss` 排在 `. += _kern_offset` 之后 | … | 落点唯一：`mpx.o` 的 `.bss` 只能进通用 `.bss`，也就是位置计数器加过偏移之后的段」。
- 问题：第 2 行的证据只是“栈定义在 `.bss`”，它不能排除“存在一条专门收 `mpx.o` 的 `.bss` 的低地址输出段”；真正排除这一点的是第 3 行（`.unpaged_bss` 只收 `unpaged_*.o(.bss COMMON)`）。第 2 行的“排除了什么”与它给出的证据不匹配。
- 影响：读者按行读证据时，会觉得第 2 行推出了它推不出的结论，从而削弱整张“三处证据”表的可信度；而 §2.4 的结论正依赖这张表的逐行排除。
- 证据或推理：只引文档内句子。第 2 行的证据是“栈定义在 `.bss` 段里”，结论是“排除定义在某个专门的低地址段”；`.bss` 是段名，不是收段规则，单凭段名无法排除另一条输出段规则。第 3 行才给出收段规则（`.unpaged_bss` 只收 `unpaged_*.o(.bss COMMON)`）。
- 最小修复：把第 2 行的“排除了什么”改成“排除‘它根本不在 `.bss` 里’”，把“专门低地址段”的排除明确留给第 3 行；或在第 2 行补一句“段名只说明它属于哪类内容，收段规则在第 3 行”。
- 置信度：高（0.8）——这是表内证据与结论的对应关系问题；具体 `kernel.lds` 的收段规则以文档第 3 行为准。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「证据支撑结论」；编号请面板补。

## SUGGESTION READER-A-S09

- 位置：§2.5 本章归纳表「装载摆放 | `Makefile.inc` 的改名规则 + 装载器按 `p_paddr` 写段体 | 段体落在低一侧；哪些对象进低一侧由改名名单决定」；§1.2 的射程声明「树内 boot monitor 走 `LOADADDR(p_vaddr)` 的低 27 位（`minix3/sys/arch/x86/include/loadfile_machdep.h:40`），`p_paddr` 只对外部 multiboot 装载器（如 GRUB）成立；下文的 `p_paddr` 读法按外部装载器写。」
- 问题：归纳表把“装载器按 `p_paddr` 写段体”写成 C 侧的通则，但射程声明说树内 boot monitor 不读 `p_paddr`，只读 `LOADADDR(p_vaddr)` 的低 27 位。读者在 §2.5 会丢掉这个限定，把“C 侧装载器 = 读 `p_paddr`”记成普遍事实。
- 影响：§2.1 的两处反向推演（“AT() 写成常量”与“删掉 AT()”）都假定了一个读 `p_paddr` 的装载器；若读者把这个假定带到树内 boot monitor 的语境，就会推出错误的故障形态。
- 证据或推理：只引文档内句子。§1.2 明写“树内 boot monitor 走 `LOADADDR(p_vaddr)` 的低 27 位……`p_paddr` 只对外部 multiboot 装载器（如 GRUB）成立”；§2.5 的归纳格只写“装载器按 `p_paddr` 写段体”，没有“外部”限定。两处对“装载器”的范围不同。
- 最小修复：在 §2.5 的“装载摆放”格加限定：“外部装载器按 `p_paddr`；树内 boot monitor 按 `LOADADDR(p_vaddr)` 的低 27 位；两者都把段体放到低侧”。或者在归纳表下方加一句“本表的‘装载器’指 §1.2 射程声明里的外部装载器”。
- 置信度：高（0.85）——两处措辞的范围差异是文档内可见的。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「结论的适用条件」；编号请面板补。

## SUGGESTION READER-A-S10

- 位置：§4.1「**把基址写成 `0xFFFFFC0000000000` 会怎样**：内核高半映射要写的是 VPN[2] = 0 那个槽，而引导期的恒等映射已经在那一槽落下 2 MiB 叶子（Step 1 固定用 2 MiB，不产生 1 GiB 叶）；真正的风险是**映射粒度不一致**：若这里改写成 1 GiB 叶，后写的叶条目会把整槽的 2 MiB 项一并顶掉，恒等映射的覆盖范围从「若干 2 MiB 片」缩成一块；反过来若上层已是 1 GiB 叶、这里写 2 MiB，x86_64 与 riscv64 的 `map_huge` 都会以 `AlreadyMapped` 拒绝 1 GiB 叶降级……有了这道检查，同一个错误会在建表时以 `AlreadyMapped` 提前报出来。」
- 问题：这段描述了两个方向：①内核写 1 GiB 叶，顶掉恒等映射已有的 2 MiB 项；②内核写 2 MiB，碰到上层已有的 1 GiB 叶。只有方向②被 `map_huge` 的“命中叶子”检查以 `AlreadyMapped` 拒绝；方向①面对的是表指针，不是叶子，检查不一定触发。最后一句“有了这道检查，同一个错误会在建表时以 `AlreadyMapped` 提前报出来”很容易被读成“两种方向都会被提前报出来”。
- 影响：读者会高估 `map_huge` 的保护范围：如果实际发生的是方向①，内核可能先把恒等映射的 2 MiB 项顶掉，再在低半取指时崩溃，而不是在建表时得到 `AlreadyMapped`。这与 §4.1 想论证的“错误会在建表时暴露”相反。
- 证据或推理：只引文档内句子。段落自己说方向①“后写的叶条目会把整槽的 2 MiB 项一并顶掉”，方向②才是“`map_huge` 都会以 `AlreadyMapped` 拒绝 1 GiB 叶降级”。最后一句没有限定方向，主语“同一个错误”指代不明。`map_huge` 的“命中叶子”检查针对的是当前层已有叶子条目；方向①当前层是下级页表指针，不在“命中叶子”的字面范围内。
- 最小修复：把最后一句改成“方向②会被 `map_huge` 的叶子检查提前拒绝；方向①不会被这道检查拦住，它会把恒等映射的 2 MiB 项顶掉，表现为低半取指故障——所以基址常量本身仍要靠 L0/脚本对账保证”。若 `map_huge` 实际上也检查表指针，请在文中补一句“命中表指针也拒绝”，并给出可观测错误。
- 置信度：中（0.65）——这是对 `map_huge` 行为的推断，需要源码核实；文档内部的双方向叙述与“一道检查”措辞确实存在指代不清。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「因果链 / 保护范围可判定」；编号请面板补。

## SUGGESTION READER-A-S11

- 位置：§3.1「生产镜像（含 riscv64 的自引导形态）则必须有 `AT()`——它的 `p_paddr` 有读者：自引导面按最低物理装载地址被跳入，U-Boot 的 `bootelf` 也按它摆放」；§4.4 路径 B「OpenSBI → 镜像 `_start`（清 `.bss`、立栈）→ `rust_image_main`」。
- 问题：按字面，“自引导面按最低物理装载地址被跳入”描述的是“跳入”动作，不是“读 `p_paddr` 摆放”的动作；读者无法判断这条腿的 `p_paddr` 读者到底是 U-Boot 的 `bootelf`、QEMU 的 `-kernel` 直载，还是 OpenSBI。§4.4 的路径 B 从 OpenSBI 直接开始，也没有写谁把镜像放到物理地址上。
- 影响：§3.1 的核心判据是“有谁按 `p_paddr` 摆放到与符号地址不同的位置，才需要低地址的 LMA”；如果 riscv64 自引导腿的摆放者不读 `p_paddr`，这条腿就不满足判据，`AT()` 的必要性需要另一条理由。读者在这里无法把“判据”与“生产镜像必须有 AT()”的结论接上。
- 证据或推理：只引文档内句子。§3.1 说“它的 `p_paddr` 有读者：自引导面按最低物理装载地址被跳入，U-Boot 的 `bootelf` 也按它摆放”；“跳入”与“按它摆放”是两种不同动作，句子把它们并列成同一个“读者”。§4.4 路径 B 的第一环是 `OpenSBI → 镜像 _start`，没有出现 `bootelf`/`-kernel`/装载器。
- 最小修复：把每条腿的摆放者点名：`bootelf` 读 `p_paddr`；`-kernel` 直载由固件/加载器按约定放置；OpenSBI 只负责跳转，不解析 ELF。若自引导腿的 `p_paddr` 没有读者，就明确说“这条腿保留 `AT()` 是为了与同一镜像的 `bootelf`/v1 腿共用脚本，而不是因为自引导面自己读它”。
- 置信度：中（0.7）——判据与例子的接口不清；具体每条腿的装载者需要源码/构建脚本核实。
- 读者验收清单条目：§4.0e 不可读；按语义归类为「判据可执行 / 角色明确」；编号请面板补。

## 完成声明

- 本轮以 READER-A（初学者路径）身份冷读 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 全文 1290 行，只使用文档正文与 `reader-profile.md`；未读源码、其他文档、diff、历史版本、工作流规则、作者解释、其他 agent 输出、`expected/`。
- 产出：2 条 `## FINDING`（READER-A-01、READER-A-02）与 11 条 `## SUGGESTION`（S01–S11）。
- FINDING 的共同点：READER-A-01 是 §3.2 与 §4.2 对 ELF 迭代器“畸形输入”处置的三处互斥描述（continue / panic / Err）；READER-A-02 是 §6.2 的 riscv64 GDB 例子与同节/§4.3 的 `delta` 加法相差 `0x200000`。两条都会让读者在“按文档核对现场”时得到相反或错误的结论。
- SUGGESTION 集中在：未定义 ID / 悬空 `follow-ups`、`push 0` 终止槽的解释不闭合、§3.1 “下表第 5 行”指针、§3.5 两档条件重叠、开头 `vm_enable_paging` 节号、§5.2 重复的 `kern_size`、§4.4 `init_protection` 引文与 Phase A 的字面张力、§2.4 证据表第 2 行的“排除了什么”与证据不匹配、§2.5 归纳表丢失 `p_paddr` 的射程限定、§4.1 `AlreadyMapped` 保护范围的指代、§3.1 自引导腿 `p_paddr` 读者角色不清。
- 清单口径：`review-evals.md` §4.0e 属工作流规则，按任务书禁读；所有条目的“读者验收清单条目”均按语义归类，未伪造编号。请持有规则文件的面板按 §4.0e 十条补编号并判定清单内外。
- 自检：没有为了让条目显得多而把“换一种讲法更好”写成 FINDING；两条 FINDING 都附了文档内原文摘录与可复算/可对照的推理；11 条 SUGGESTION 每条都附了原文摘录与最小修复。
