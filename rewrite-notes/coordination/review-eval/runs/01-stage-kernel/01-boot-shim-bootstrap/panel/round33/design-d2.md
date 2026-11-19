# P-DESIGN-D2 独立设计审查报告（round33）

- 审查对象：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 第 3、4 章（设计决策与实现详解）
- 关联源码：§7.3 清单所列 `os/boot-shim`、`os/libs/minix-boot`、`os/kernel`、`os/arch`、`os/kernel-image`、`os/servers/vm`、`os/libs/minix-types` 各文件（逐条见各 finding 锚点）
- 审查维度：设计不对称、非法状态、所有权/接口顺序、抽象边界、最小化机会、文档与代码设计陈述不符
- 方法：只读。逐条把文档陈述对到源码行；文档已自认的取舍（如 DESIGN-D2-17 的裸 `(u64,u64)` 边界、`startup_committed` 的调用纪律）只在与项目自身标准冲突时才升级为 finding。
- 结果：FINDING 8 条（P1 × 1、P2 × 1、P3 × 6），SUGGESTION 4 条（纯偏好/可选优化，不计入收敛障碍）。

---

## FINDING RD33-DESIGN-D2-01

**位置**：文档 §4.6（rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2906）与 §4.2.1 动作 9 表行（同文件:2413）；源码 `os/boot-shim/riscv64-uboot.ld:62-80`、`os/boot-shim/src/uboot_main.rs:62-68` 与 `:496-497`、`os/kernel/src/lib.rs:1244-1252`。

**现象**：文档与源码注释都断言 U-Boot shim 腿的恒等窗口 `[SHIM_LINK_BASE, __bss_end)` 覆盖 shim 栈——§4.6 原文"栈在 `.bss` 内由同一窗口覆盖"，`uboot_main.rs:62-68` 原文"it covers the shim's text/rodata/data/bss, **including its stack and heap**"。但链接脚本把 `.stack (NOLOAD)` 段放在 `.bss` 之后，且 `__bss_end = .` 在进入 `.stack` **之前**定值（riscv64-uboot.ld:72 vs 77-79），所以栈区间 `[__bss_end, UBOOT_SHIM_STACK_TOP)`（64 KiB）实际落在窗口之外。`uboot_main.rs:496-497` 推窗口用的终点就是 `&__bss_end`。今天栈仍被映射，唯一原因是 Step 1 把每个窗口终点向上取整到 2 MiB piece（lib.rs:1244-1248）：64 KiB 栈只要 `__bss_end` 不恰好 2 MiB 对齐就被这次取整顺带盖住。同文件头部注释还留着已失效的布局数字"16 MiB SHIM_HEAP/lbss"（riscv64-uboot.ld:27-28），实际堆是 64 KiB（uboot_main.rs:82）——同一文件里布局陈述漂移的第二处。

**影响**：(1) 设计陈述与承载代码不符——"boot identity 窗口必须覆盖分页开启前仍在执行/读写的全部内存"是本篇的硬合同（§3.7 反向推演的基础），U-Boot 腿对栈的覆盖目前是取整的偶然产物，不是任何文本或断言钉住的合同；(2) 可预见的失败路径：镜像尺寸演化使 `__bss_end` 恰好落在 2 MiB 边界时，取整不扩窗，`enable()` 之后的第一次栈压入落在未映射页，且 panic 路径本身需要栈——串口上无输出，正属 §6.1 的"最早期静默停机"类；(3) 后续维护者按"栈在 .bss 内"的陈述改动窗口或段布局时，会沿一条错误的前提走。

**最小改法**：二选一，均 1-3 行：(a) 把 riscv64-uboot.ld 的 `__bss_end = .` 移到 `.stack` 段之后——入口清零循环会多清 64 KiB 栈区，时序在 `la sp` 之前，安全；(b) `uboot_main.rs` 给 `UBOOT_SHIM_STACK_TOP` 加 `unsafe extern "C"` 声明，窗口终点改用它。两条路径都须同步修正 uboot_main.rs:62-68 注释与文档 §4.6/§4.2.1 的表述；顺手更新 .ld 头部的 16 MiB 陈旧数字。

**严重度**：P1。

**反证路径**：若维护者认定"2 MiB 取整即覆盖机制"：则 (a) 该机制须写入文档与注释（现状两处都写的是"栈在 .bss 内"，与取整机制是两个不同命题）；(b) 须加一条链接期断言钉住 `__bss_end % 2MiB != 0`（否则取整退化为恒等映射、栈必失映射）。两者今天都不存在，故"窗口覆盖栈"在现行文本+脚本下不成立。可复算：`grep -n "__bss_end\|\.stack\|UBOOT_SHIM_STACK_TOP" os/boot-shim/riscv64-uboot.ld` 显示 `__bss_end` 定值行在 `.stack` 段之前。

---

## FINDING RD33-DESIGN-D2-02

**位置**：文档 §5.1 M2 行（01-boot-shim-bootstrap.md:3130，记载 D01-R1 D2-05 的"只收有序子集"决定）；源码 `os/libs/minix-boot/src/kernel_info.rs:288-315`（有序断言与"accepts only the sorted subset"注释）、`os/libs/minix-boot/src/memmap_source.rs:1417-1491`（`collect_arrays` 按 `/memory` 节点与 reg 的声明序 push，收尾无排序）。

**现象**：`DtbMemoryMap` 收集器把 `/memory` 节点（`all_nodes()` 遍历序）与节点内 `reg` 元组（属性序）直接 push 进 free/reserved 表，全文件无对这两张表的排序（仅 source 记录的 origins 在 ：2462 排序）。`KernelInfo::validate` 则要求两张表按基址严格升序，否则 panic（"未按基址升序"）。kernel_info.rs 注释自述这是有意取舍："DTB /memory's ordering is *not* spec-guaranteed, so this repository accepts only the sorted subset — an unordered (legal but pathological) DTB fails closed"。但 DT 规范确实不保证节点/reg 序，两个 `/memory` 节点乱序、甚至单节点内 reg 元组乱序，都是合法输入；重叠与完全重复已有独立检查（`OverlappingMemoryRanges`/duplicate 分支），排序不会掩盖任何一类异常。

**影响**：真板兼容性风险：一条合法但乱序的设备树（厂商 DT 源码里 memory 节点书写序无保证；U-Boot 的 FDT fixup 也可能追加节点）会在 `kmain` 的 validate 处拒绝启动，症状是启动最末端的 panic，与"固件坏了"不可区分。UEFI 腿的图天然有序，DTB 三腿（U-Boot shim、两个直核面）都暴露在同一风险面。防御成本与收益不对称：拒绝的输入里没有任何一类重叠/重复异常是排序才能拦的。

**最小改法**：在 `collect_arrays` 收尾（`commit_static` 之前）对 free/reserved 各做一次 ≤8 元素的稳定插入排序（按 `base` 升序；`carve_free` 保序追加，排序点一处即可）；validate 的有序断言降级为内部不变量保留。补一条宿主测试：乱序 `/memory` 节点的 fixture 能通过 collect 且 `validate` 通过。

**严重度**：P2。

**反证路径**：若"对合法输入也 fail-closed"是本仓的平台纪律（像 Step 1 的 `UnknownAttribute` 拒绝未知固件位那样），那么文档应明确列出"乱序 DTB 会被拒"这条可观察行为及其理由；现状文档只把有序断言记成生产者契约（M2 行），没有论证为什么有序性要由输入保证而不是由生产者达成。另外 `collect_owned`（U-Boot 腿）与 `collect_with_sources`（直核腿）共用同一收集路径，修复一处即覆盖三腿。

---

## FINDING RD33-DESIGN-D2-03

**位置**：文档 §3.4.5（01-boot-shim-bootstrap.md:1768："set_modules_len/set_platform_len/set_param_len/set_memmap_sources_len/set_startup_len 各自只允许提交一次，第二次调用 panic"）；源码 `os/libs/minix-boot/src/boot_storage.rs:89`（`startup_committed` 字段）、`:292-302`（`set_startup_len` 标注 `#[cfg(test)] pub(crate)`）、`:305-313`（`push_startup` 断言 `!self.startup_committed`）。

**现象**：文档把 `set_startup_len` 列为五个生产 one-shot 提交 API 之一；代码里它是 test-only（`#[cfg(test)]`），生产路径的 startup 记录只经 `push_startup` 逐条追加。结果是 `startup_committed` 在生产构建里永远为 false——`push_startup` 里的 `assert!(!self.startup_committed)` 是恒真断言（死不变量），"startup 族的一次性提交"在两个生产 DTB 腿上不存在。

**影响**：文档承诺的提交协议与生产可用的 API 不一致；死断言给读者"该族有 one-shot 保护"的假象。实际风险低（`BootStorageCell::take` 的一次性借用挡住了重入），但这正是 §3.4.5 自己批评过的"依赖调用纪律"模式——只是这次连纪律的载体都是空的。

**最小改法**：(b) 方案最小：删除 `startup_committed` 字段与 `push_startup` 里的恒真断言（3 行），文档 §3.4.5 把 startup 族改述为"只增追加、无一次性提交"，并把 `set_startup_len` 从五 API 列表移到 test-only 说明；(a) 方案（放开 cfg、让两个 DTB 腿 push 后各调一次 `set_startup_len`）语义更完整但给 UEFI 腿引入"直写 wire 不经 storage"的第二特例，收益不成比例。

**严重度**：P3。

**反证路径**：若主张 (a)：需同时回答 UEFI 腿（startup 分区 EBS 后直写 wire、storage 槽留空）如何在同一协议下"提交计数"；§3.4.5 现文没有给出该腿的答案，说明协议本身就没把 startup 当"槽+计数"族，(b) 与协议实态一致。

---

## FINDING RD33-DESIGN-D2-04

**位置**：文档 §3.4.5（01-boot-shim-bootstrap.md:1768：其余五族"半提交状态依赖调用纪律，不靠 report() 挡"）；源码 `os/libs/minix-boot/src/boot_storage.rs:356-381`（`report()` 只检查 `map_dirty`）与 `:85-89`（五个 `*_committed` 标志已存在）。

**现象**：数据结构已经为七族全部携带提交状态：内存表用 `map_dirty`，其余五族用 `*_committed`。但 `report()` 只拦 `map_dirty`——"槽已写、长度未提交"的视图对 modules/platform/param/sources/startup 五族仍可从 `report()` 流出。防线不是缺载体，而是载体未接线：`report()` 加一行"每族 `committed || len == 0`"即可闭合。

**影响**：当前生产路径（UEFI 腿四族立即提交、DTB 腿全部提交）无实际触发点，属潜在接口缺口；但它是文档明言接受的那个"调用纪律"缺口，而闭合成本约 5 行——在"非法状态不可表示"这个本仓反复引用的设计标准下，半开状态属可消除项。

**最小改法**：`report()` 内加五条断言（如 `assert!(self.modules_committed || self.modules_len == 0, "BootStorage::report: modules slots written but length not committed")`，余四族同式）；UEFI 腿的 sources/startup 槽留空（len==0、未提交）恰好通过，两条 DTB 腿全提交也通过。配一条半提交负控宿主测试。

**严重度**：P3。

**反证路径**：若认为"UEFI 腿 sources/startup 留空且永不提交"使断言语义不纯——`committed || len == 0` 正是为这两族设计的合法空态，不与 D-1 的所有权叙述冲突；反证只需举出一个"len>0 且未提交却应允许 report"的调用形状，目前没有。

---

## FINDING RD33-DESIGN-D2-05

**位置**：文档 §4.4 校验门表（01-boot-shim-bootstrap.md:2718-2734，自称"共十三行：十二行是门"的门枚举）；源码 `os/kernel/src/lib.rs:2633-2639`（`init_proc_and_boot` 的 `assert_eq!(kernel_info.boot_modules.len(), NR_BOOT_MODULES)`，注明 C main.c:160-162 对应）。

**现象**：门表枚举了 kernel 侧的十二道门，但漏了真实存在的第十三道：boot_modules 数量等式门（在 `kmain` 的 `init_proc_and_boot` 里执行）。同时 strict 门只做"按位核名 + 非空"（kernel_info.rs:474-481），不查数量等式——一个 byte_len 仍被 32 整除的截短模块表能通过 strict 门与 kmain 的 `validate`，要到 `init_proc_and_boot` 才被拦。发送侧（loader 恰好装载 12 个）与接收侧（数量等式在更晚一层）不对称：名字的接收侧复核进了 strict，数量的接收侧复核没有。

**影响**：主要是文档对账问题（表自称完整枚举）；次要是门位纵深不一致——strict 门的设计目的就是"伪造载荷在接收边界 fail-closed"（doc §4.4 表中"伪造了模块名"一类的同族 bug），短清单类伪造却滑到更晚的层，诊断信息（"expected 12, found N"）相同但时点更晚。

**最小改法**：二选一：(a) 文档补一行（门=数量等式，挡"短清单/截短 wire"）；(b) 把等式提前——strict 门内加 `assert_eq!(self.boot_modules.len(), MODULE_NAMES.len())`（约 4 行 + 2 条测试；测试内核走 `BeforeTables` 或直接 `arch_boot_impl`，不受影响）。(b) 更符合该门自称的拦截目标。

**严重度**：P3。

**反证路径**：若主张"数量等式属 Phase C 的进程表构造契约，不属交接校验"：C 侧 main.c:160-162 的确在进程初始化里查——但 Rust 侧已把 C 的"按位核名"（同属 boot 画像）上提进 strict 门，同一画像的两半分居两层没有原则性理由；反证需说明名字为何比数量更接近"交接边界"。

---

## FINDING RD33-DESIGN-D2-06

**位置**：源码 `os/boot-shim/src/opensbi_helpers.rs:494-507`（`install_boot_file_table` 的双安装检查是 `debug_assert!`，注释自认"release builds skip the check"）、`:545-551`（`install_dtb_ptr` 同式）；对照标准 `os/arch/src/arch/pt_alloc.rs:100-101`（`PT_CLAIMED.swap(true)` + `assert!(!claimed, ...)`，F-R23-05：两种构建档都 fail-closed）。

**现象**：两个 installer 的安全文档都写"Must be called exactly once"，但唯一的一次性防线在 release 构建里不生效（`debug_assert!`），双安装静默覆盖旧指针。这是与 `pt_alloc::register` 同类的不变量（"二次注册是 boot bug、静默保留/覆盖第一个会隐藏接线错误"），而后者在 F-R23-05 之后已按"release 也 fail-closed"的标准改造——同类不变量两套标准。

**影响**：当前调用点（`uboot_main` 各调一次）无实际触发；但 release 构建的防御与项目自身为同类问题定下的标准相悖，且 boot-shim 的 release 构建正是产线形态。

**最小改法**：两个 installer 各用 `static CLAIMED: AtomicBool` + `swap(true, AcqRel)` + `assert!(!claimed)`（与 pt_alloc.rs:100-101 同形，约 6 行）；`boot_file_table()`/`dtb_ptr()` 的读法不变。

**严重度**：P3。

**反证路径**：若理由是"安装点在 hart 启动最早期、一条 load 都嫌贵"：installer 每次启动只调两次，`swap` 的成本在 boot-shim 里不构成可测量开销；pt_alloc 的同类改造已确立该成本可接受。

---

## FINDING RD33-DESIGN-D2-07

**位置**：文档 §3.2（01-boot-shim-bootstrap.md:1517："U-Boot 侧的生产者在 `build_shim_table` 里按最后一个 PT_LOAD 的文件尾修剪槽长度，loader 的二次修剪是幂等 no-op"）；源码 `os/boot-shim/src/uboot_main.rs:266-318`（`staged_elf_used_len`：手写 ELF/程序头解析，约 50 行）与 `os/boot-shim/src/opensbi_helpers.rs:206-215`（`UbootFileLoader::read` 用共享解析器 `minix_elf::segment_iter` 做同一条规则）。

**现象**：同一条"按 PT_LOAD 文件尾修剪"规则在 U-Boot 腿有两份实现：`build_shim_table` 用手写解析器（自拼 `read_u16/u32/u64`、硬编码 ELF64 头偏移 32/54/56），`UbootFileLoader::read` 用共享的 `minix-elf` crate。`minix-elf` 是 boot-shim 的普通依赖且 `lib.rs` 再导出（`pub use minix_elf`），`uboot_main` 同文件已在使用 `alloc::format!`，不存在"不能引用"的障碍。两份解析器对同一批 staged 字节给出两个"文件尾"答案，漂移时 §3.2 所述"二次修剪是幂等 no-op"的前提破裂（手写版多算 → loader 版修剪救回；手写版少算 → `BootModule.len` 短记）。

**影响**：维护面翻倍：ELF 头偏移、phentsize 下限、PT_LOAD 判定这三处知识在手写版里各存一份，minix-elf 修复或收紧校验（如 filesz>memsz）不会传播到手写版。属可消除的重复，非行为缺陷。

**最小改法**：`staged_elf_used_len` 改为与 opensbi_helpers.rs:211-215 逐字同式的 `minix_elf::segment_iter(slice).ok().and_then(|iter| iter.map(|seg| (seg.offset + seg.filesz) as usize).max()).filter(|&end| end > 0 && end <= slot).unwrap_or(slot)`，删除手写解析器（净删约 45 行）；若保留手写版有理由（如 header 阶段不想承担完整校验），把理由写进注释——`segment_iter` 同样只读头部与表，无分配。

**严重度**：P3。

**反证路径**：运行 `grep -n "minix_elf" os/boot-shim/src/uboot_main.rs`（当前零命中）证实同一 bin 里两套解析并存；反证需举出手写版能而 `segment_iter` 不能处理的真实 staged 形态（如非 ELF 回退——`unwrap_or(slot)` 两版语义相同，不构成差异）。

---

## FINDING RD33-DESIGN-D2-08

**位置**：源码 `os/libs/minix-boot/src/handoff_v1_write.rs:158-169`（`write_envelope(info: &KernelInfo)` 的文档注释："receiver-side `KernelInfo` form; direct-core legs still use it"）；全仓调用面：`grep -rn "write_envelope\b" os/ --include=*.rs` 仅命中测试代码（ebs_pair.rs / vm_handoff.rs / handoff_v1_read.rs 的 `#[cfg(test)]` 段与 handoff_v1_write.rs 自身测试）。

**现象**：该注释声称"直核腿仍在使用"这个 KernelInfo 形态的信封写入器；实际上两个直核面根本不序列化 arena——`bootface`/`bootface_a64` 在进程内构造 `KernelInfo` 后直接调 `arch_boot`（bootface.rs:427-442），生产序列化只用 `write_envelope_report`（BootReport 形态）。这与文档 §3.4/§4.4 的权威陈述（"v1 发送腿不调用 builder，发送侧写 wire、接收侧解码"；发送侧不构造 `KernelInfo`）一致，是代码注释单方面滞后：`write_envelope` 实际是 test-only 便利入口。

**影响**：注释把"直核腿走这条编码路径"的假分工留在了协议层源文件里——读者顺着它去找直核腿的序列化点会扑空，并可能误判 `KernelInfo` 仍是跨镜像载体（正是 §3.4.3 花整节澄清过的那个误区）。

**最小改法**：注释改为"test-only convenience; production legs encode from `BootReport` via `write_envelope_report`; direct-core faces never serialize"（1 行）。若想更进一步，把 `write_envelope` 收进 `#[cfg(test)]` 并迁移三个测试调用点（净删一个公开 API 面）。

**严重度**：P3。

**反证路径**：`grep -rn "write_envelope\b" os/ --include=*.rs | grep -v test` 为空即证；若未来真出现"KernelInfo 直写 wire"的生产者（当前协议下无此腿），注释应随实现一起回归。

---

## SUGGESTION（纯偏好/可选优化，不计入障碍）

**S-1 信封携带三个派生标量的冗余**：`BootHandoffV1` 的 `user_sp`/`syscall_entry`/`kern_stack_top`（24 B）是收发两侧共同代码（`BootStorage::report`/`build_kernel_info` 的派生式）的纯函数，接收侧 strict 门再按同式重算并等值断言（kernel_info.rs:453-470）。它们唯一的作用是当作"发送侧用了同一派生式"的复核（顺带探测 arena 位翻转）。删除可省 24 B 并简化 strict 门，但需 bump 信封版本且损失那层复核；保留是合理选择。记录在此仅供权衡。

**S-2 派生点的裸加法**：`build_kernel_info`（os/boot-shim/src/lib.rs:125）与 `BootStorage::report`（boot_storage.rs:378）用裸 `kern_virt_base.0 + kern_size` 派生 `kern_stack_top`，而全链其余跨度算术已 `checked_add` 化。下游 strict 门会用 `checked_add` 拦住溢出，故无实际风险；在派生点直接 `checked_add` 可把失败从接收端提前到构造端，风格也更一致。

**S-3 配对循环扩容时旧缓冲泄漏**：`prepare_and_enter_kernel_image` 的 `NeedBiggerBuffer` 重试每次 `alloc_map_buffer` 新分配、旧缓冲不 `free_pool`（uefi_helpers.rs:1090-1136）。boot 一次性、页最终落 `ReservedMapped` 不进 free list，可接受；追求最小占用可在扩容分支先 `free_pool` 旧缓冲。

**S-4 休眠的 `StartupObjects` 权威通道**：`InputObjectId::StartupObjects`（tag 2）与 wire 解码里的对应分支是为"镜像自身活对象"预留的权威通道，当前无任何生产者（文档 §3.4.1 已如实声明"不是一条活的 authority 通道"）。保留待接线或删除均可；若保留，建议在 `WireSourceRecord::decode` 的 canonical 检查处注明该分支当前无生产者，避免被误读为活路径。

---

## 汇总

| 严重度 | 数量 | 编号 |
|---|---|---|
| P1 | 1 | RD33-DESIGN-D2-01 |
| P2 | 1 | RD33-DESIGN-D2-02 |
| P3 | 6 | RD33-DESIGN-D2-03 ~ 08 |
| SUGGESTION | 4 | S-1 ~ S-4 |

最严重三条摘要：

1. **RD33-DESIGN-D2-01（P1）**：U-Boot shim 腿的恒等窗口设计陈述（"栈在 `.bss` 内由 `[SHIM_LINK_BASE, __bss_end)` 覆盖"，文档 §4.6/§4.2.1 + uboot_main.rs 注释三处）与链接脚本事实矛盾——`.stack` 段在 `__bss_end` 之后，窗口实际不含 64 KiB 引导栈；今天的覆盖是 Step 1 的 2 MiB 向上取整偶然提供的，`__bss_end` 一旦恰好 2 MiB 对齐，分页开启后的第一次栈压入落进未映射页且无诊断输出。修复 1-3 行（挪 `__bss_end` 或窗口终点改 `UBOOT_SHIM_STACK_TOP`）+ 同步三处文本。
2. **RD33-DESIGN-D2-02（P2）**：DTB 收集器不排序，一条合法但乱序的设备树（`/memory` 节点序或 reg 序）会在 `kmain` 的 `KernelInfo::validate` 处拒启，与"固件损坏"不可区分；重叠/重复异常已有独立检查，排序在收集端是零信息损失的本地修复（≤8 元素插入排序一处 + 一条乱序 fixture 测试）。
3. **RD33-DESIGN-D2-03（P3，与 04 同族为文档-代码对账类之首）**：文档 §3.4.5 把 `set_startup_len` 列为五个生产 one-shot 提交 API，代码里它是 `#[cfg(test)]`；`startup_committed` 在生产构建永远为 false，`push_startup` 的一次性断言恒真——文档承诺的提交协议在生产路径上不存在载体，需删死不变量并改述文档（或真正接线提交，但需先回答 UEFI 腿直写 wire 的对应语义）。
