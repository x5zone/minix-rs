# READER-C（因果一致性）· 第 34 轮

读法：只读本篇正文与读者画像，按读者验收清单逐条追关键因果主张在概述、小结、设计、实现之间的复述。以下每条都附原文摘录；没有摘录的条目已删除。

## FINDING RD34-READER-C-01（P2）

- 位置：§3.3 问题二末句（"它的定位是**平行的装机面**：与 UEFI 独立 shim 共享同一套取件库与 `arch_boot` 收敛点——统一发生在层内（类型、取件库、收敛函数），引导形态层不造伪统一"）
- 严重度：P2
- 问题：两处说法冲突——§3.3 说直核装机面与 UEFI 独立 shim 共享同一个 `arch_boot` 收敛点；§3.7 与 §4.8 说收敛点按腿分两个，跨镜像腿收敛在 `arch_boot_resume_v1`，并且发送侧根本不经过 `arch_boot`。同一份文档对"UEFI 腿的内核侧入口是哪一个函数"给出两个答案。
- 影响：我无法判断 UEFI 链的内核入口是 `arch_boot` 还是 `arch_boot_resume_v1`，也无法判断 §3.3 用"共享收敛点"论证"层内统一、形态层不造伪统一"这条论据是否成立。若实际共享的是 `jump_to_kmain` 而不是 `arch_boot`，论据的对象就要改；若确实共享 `arch_boot`，则 §3.7/§4.8 的"发送侧没有收敛到 arch_boot"需要撤回。两条读法会影响我对"两个收敛点"这个架构结论的信任。
- 证据或推理：§3.3："与 UEFI 独立 shim 共享同一套取件库与 `arch_boot` 收敛点"。§3.7："三条对外入口分别是 `arch_boot`（直核/测试）、`enter_kernel_image_v1`（跨镜像 v1 发送，三条腿：两条 UEFI 链 + riscv64 U-Boot shim；它不在本镜像内收敛，终点是跳独立镜像的入口指令）与 `arch_boot_resume_v1`（接收）"；"直核家族收敛在 `arch_boot_finish`……跨镜像三腿的接收侧收敛在 `arch_boot_resume_v1` → `resume_additive`"。§4.8："发送侧没有'收敛到 arch_boot'——它的终点是跳独立镜像的入口指令"；"三者在 调用序合并后都委托到泛型体 `resume_additive`……最后落到同一个 `jump_to_kmain`"。
- 最小修复：把 §3.3 的"`arch_boot` 收敛点"改成"内核引导收敛层（直核 `arch_boot_finish` / 跨镜像 `arch_boot_resume_v1`，两者最终同落 `jump_to_kmain`）"，或删去该短语并加指针"收敛点按腿分两处，见 §3.7 与 §4.8"。
- 置信度：中
- 是否需源码核实：否（三处引文互相冲突，文档内即可判定）
- 若我错了：我可能把"与 UEFI 独立 shim 共享"误读成同时修饰"取件库"和"`arch_boot` 收敛点"；作者可能只想说"两个直核面彼此共享 `arch_boot` 收敛点，并与 UEFI shim 共享取件库"。若如此，把这句话拆成两句就能消除歧义，语义不必变。
- 命中清单：3、4、9

## FINDING RD34-READER-C-02（P2）

- 位置：§3.4.1 "新增"行（"`KernelInfo::validate`（非 strict）在 `memmap_sources` 非空时也查'来源记录覆盖投影内存图'"）与 §4.4 第 4 项及校验门表
- 严重度：P2
- 问题：两处说法冲突——§3.4.1 说非 strict 的 `KernelInfo::validate` 在 `memmap_sources` 非空时也查来源覆盖；§4.4 第 4 项只说 `KernelInfo::validate`"复核非地址不变量、检查生产内核的 `boot_modules` 非空，并复核……半开区间不重叠"，把"按 `KernelInfo.authority` 查 `memmap_sources` 覆盖投影内存图"只列在 `validate_requiring_sources`（strict 变体）名下；§4.4 的门表对 `KernelInfo::validate` 也只列 `kern_size`/`memmap`/`param_buf` 与 `boot_modules`，没有来源覆盖。
- 影响：我无法判断"来源记录缺失或不覆盖投影内存图"这件事在非 strict 的 `kmain` 入口是否会被挡下。这直接决定 §4.4 标题"一条校验门链"的分工：按 §4.4 读，provenance 只在 strict 门查；按 §3.4.1 读，非 strict 也查一部分。两条读法会给出不同结论——一个漏写 source 记录的生产者是否能走到 `kmain`。
- 证据或推理：§3.4.1："`memmap_sources` 与 `startup_objects` 是 v1 source/startup 记录，供启动期 `validate_requiring_sources`（strict 门）审计，`KernelInfo::validate`（非 strict）在 `memmap_sources` 非空时也查'来源记录覆盖投影内存图'；strict 另要求 `startup_objects` 盖住内核镜像"。§4.4 第 4 项："前者复核非地址不变量、检查生产内核的 `boot_modules` 非空，并复核内核镜像与各模块、模块与模块的半开区间不重叠……后者按结构体自带的 `authority` 要求 `memmap_sources` 覆盖投影内存图"。§4.4 门表："`KernelInfo::validate`（`kmain` 入口） | 复核 `kern_size`/`memmap`/`param_buf` 非地址不变量；`boot_modules` 非空（仅生产内核；QEMU 测试内核合法为空） | 同前几行，在更晚的入口再挡一次；模块装载失败"。
- 最小修复：二选一，并在一处加指针——其一，在 §4.4 第 4 项和门表补上"非 strict 在 `memmap_sources` 非空时也查覆盖"；其二，删掉 §3.4.1 的这半句，把 provenance 明确写成 strict-only。无论选哪条，另一节都要有一句回指，避免读者按两套口径各读一次。
- 置信度：中
- 是否需源码核实：是（最终以 `kernel_info.rs` 的 `validate` 实现为准）
- 若我错了：§4.4 第 4 项用的"复核"可能只是举例，门表也只声明"按门列出各校验点挡的 bug 类"而非穷举；作者可能认为 §3.4.1 已经给出完整口径，§4.4 只列主项。若如此，至少在 §4.4 加一句"非 strict 还做条件性的来源覆盖检查，见 §3.4.1"即可，不必改结论。
- 命中清单：3、4、9

---

以下为清单内但未达到"卡住"门槛的 P3 项，以及清单外的措辞/指针改进机会。按本轮纪律一律记为 SUGGESTION，不声称是缺陷，不进缺陷台账。

## SUGGESTION RD34-READER-C-S01（指向同一动作的两个位置，P3）

- 位置：§4.6 Step 2b 段末（"Step 3 末尾的 `use_high_mmio_window()` 配套它"）与 §4.8 的 aarch64 入口引文（`early_console::use_high_mmio_window();` 出现在 `arch_boot_resume_v1` 之前）
- 问题：§4.6 把这次调用定在"Step 3 末尾"；§4.8 展示的调用点在接收镜像的 `rust_image_main` 里，而 aarch64 UEFI 腿的 Step 3 按 §4.8 的表是 shim 的 `enter_kernel_image_v1` 内的 `enable`。两处对"谁在什么时候把控制台切到高半别名"给出不同位置；若 shim 与接收镜像各调一次，文档没有说明。
- 影响（非卡住）：主因果"TTBR0 被换掉前必须有一个高半别名"不受影响，但追"切换后串口为何还能输出"时找不到唯一的调用点，必须自己假设"两处都调，或者只有一处"。
- 最小修复：把 §4.6 的定位改成"Step 3 之后、第一次 `switch_address_space` 之前（接收腿在 `rust_image_main` 里调用，见 §4.8）"；若 shim 侧也有一次调用，明说两处。
- 命中清单：5、9

## SUGGESTION RD34-READER-C-S02（例子缺一个前提，P3）

- 位置：§2.3 "同一个 off-by-one 的第二种表现"（"例如 `param_buf` 里先有 `xboard=2`，内核再写 `board=` 时，中段 `board=2` 会被删掉，后一条 `arch=i386` 的前半截被补进原位，留下伪记录 `xarch=i386`"）
- 问题：删除扫描的第一轮从缓冲区首字节开始比对，只有对非首条记录才会从它的第二个字符起匹配（这正是前一段字节图推出来的规则）。若 `xboard=2` 恰好是第一条记录，中段 `board=2` 不会被命中，例子不成立；原文没有说明 `xboard=2` 不是首条。
- 影响（非卡住）：按字面复现会得到"没有删除"，与例子结论相反；我需要自己补一条前置记录才能跑通。
- 最小修复：在例子里补一句"`xboard=2` 不是第一条记录（它前面还有一条别的键）"。
- 命中清单：3

## SUGGESTION RD34-READER-C-S03（指针没有落到可命名的目标，P3）

- 位置：§4.6 缓存属性条目（"现注释已改写为 `map_huge` 错误集口径，见下文'忽略错误'一条"）
- 问题：下文的六条里没有标题为"忽略错误"的条目；对应内容是小标题为"Step 1 的 `map_huge(...)` 用 `match` 区分两类结果"的那一条。指针没有落在一个可命名的目标上。
- 影响（非卡住）：我需要把"错误集口径"与随后的 `match` 条目自己对上，能对上，但要多走一步。
- 最小修复：改成"见下文第五条（`map_huge` 的 `match`）"，或给该条加一个"忽略错误"式的小标题。
- 命中清单：5

## SUGGESTION RD34-READER-C-S04（两处列举的次序不一致，P3）

- 位置：§3.7 `resume_additive` 引文头（"加性序列枚举 `validate（内含 pt_alloc 注册）→ DM plan → record root → store → register_panic_diagnostic → establish DM → jump`"）与 §4.8（"validate/落 KernelInfo/记录根/establish DM 的同一组加性阶段在其体内"）
- 问题：§3.7 的显式序列是 record root 在 store 之前；§4.8 的斜杠列表把"落 KernelInfo"写在"记录根"之前。若读者把 §4.8 的列表当顺序读，会得到相反次序。
- 影响（非卡住）：精确次序只在 §3.7 有权威，§4.8 是概括；两处次序不一致会让我在追"根何时被记录"时回头核对。
- 最小修复：把 §4.8 的列表改成与 §3.7 同序，或明说"此处只列举，不表顺序"。
- 命中清单：4

## SUGGESTION RD34-READER-C-S05（失败模式行混入另一条失败信号，P3）

- 位置：§6.3 表"bump 区间耗尽"行（"`BootAlloc::alloc` 返回 `AllocationFailed` | 内核侧 `map_huge`/`boot_pt_alloc` 失败，或 `enter_kernel_image_v1: bump region not registered`"）
- 问题：`bump region not registered` 是"区间未注册"（Step 0 的 fail-closed，见 §4.1 与 §4.7），不是"耗尽"的现场信号；把它列在同一行会让排障方向从"预算不够"偏到"注册缺失"。
- 影响（非卡住）：按这一行排查耗尽时，会先去找一条与耗尽无关的消息。
- 最小修复：把它拆到单独一行，或删去并在 §4.7 的"两种进入方式"处保留。
- 命中清单：2、6

## SUGGESTION RD34-READER-C-S06（riscv64 的裸串口写法没有交代，P3）

- 位置：§6.1 首段（"U-Boot shim 与两个直核面没有 EBS 这一步，只有裸串口"）与 `raw_serial`/`raw_serial_line` 的差别段
- 问题：两种写法的差别只按 x86_64（`out dx, al`、LSR bit5）与 aarch64（共用带节流发射器）描述，没有交代 riscv64 的实现落在哪一条。§3.4.2 已给 `EarlyConsole` 三架构的 `write_byte`（riscv64 SBI ecall），但 `raw_serial` 是另一条函数。
- 影响（非卡住）：在 riscv64 上按 §6.1 判断"哪一行会等发送侧腾空、哪一行会补 `\r`"时，没有依据。
- 最小修复：补一句 riscv64 的实现（例如走 SBI ecall，与 aarch64 同形或与 x86_64 同形），或加指针到给出该实现的节。
- 命中清单：2、9

## SUGGESTION RD34-READER-C-S07（清单外：首段的限定重复，读不出主干）

- 位置：§1.1 首段（"而 BIOS 上的引导器（boot2 或 GRUB）在本仓核对到的路径上交棒（**关着分页**这一状态对 GRUB 类合规引导器由 Multiboot 规范支撑；boot2 的 `CR0.PG` 实际值未逐例核实）；`CR0.PG` 的实际值只对 GRUB 类合规引导器有规范支撑，boot2 未逐例取证……"）
- 问题：同一限定（GRUB 有规范支撑、boot2 未取证）在一个句子里说了两遍，句子靠分号和括号串接，第一遍读不出主干"BIOS 引导器交棒时关着分页"。
- 影响（非卡住）：需要回读一次才能拆出主张与限定。
- 最小修复：拆成两句——先给主张，再给范围限定；删掉重复的那一遍。
- 命中清单：清单外（文风）

