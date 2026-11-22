# 02-higher-half-kernel · 事实审查（P-FACT，C 侧 + Rust 侧合并）

## 基线与机械门读数

| 项 | 读数 | 判定 |
|---|---|---|
| 行数 | `wc -l` = 1275 | 与基线一致 |
| sha256 | `2bb4a585597ffbd27cf4495292e2e85f026e1782b08fe5268426f374357e5485` | 前缀 `2bb4a585597ffbd2` 一致 |
| `python3 tools/doc-citation-check.py <doc>` | `blocks=32 checked=1 failed=0 skipped=31 warned=0` | 通过（注意：32 个代码/引文块中 31 个被跳过，逐字核对不覆盖） |
| `tools/anchor-resolve.sh --check <doc>` | `anchors=59 resolved=57 zero-def=2 multi-def=0`；两处 ZERO-DEF：`loader.rs:fn load_boot_modules_with_loader`、`bootface.rs:fn place_modules` | 工具自身标"按 P0-fact 处理"，见 RD1-FACT-01/07 |
| `python3 tools/doc-structure-check.py <doc>` | `1 个文件表结构与内部 .md 链接均可解析` | 通过 |
| `tools/doc-ref-check.sh --check <doc>` | `内部章节引用均可解析` | 通过 |
| `tools/doc-code-map.sh <doc> --check` | 27 个文件，存在性 27/27 | 通过 |
| `tools/doc-style-lint.sh <doc>` | `error级=0` | 通过 |

补充取证（本轮实跑，非文档声称）：

- `cd os && cargo test -p minix-elf -- --list` → `23 tests`，与文档 §5.1 的 23 一致。
- `cd os && cargo test -p boot-shim --lib -- --list` → `125 tests`；其中 `loader::tests::` 22 条。文档 §5.1/§5.4 写 18 与 113，见 RD1-FACT-03。
- `SKIP_BUILD=1 bash os/kernel-image/check-layout.sh all` → 三架构 L0–L9 全 PASS，`CHECK-LAYOUT=PASS（all）`。L4 平移量实测 `0xffff800000000000` / `0xffff7fffbfe00000` / `0xffffffbf7fe00000`，与 §6.2 逐字一致；L9 实测 `0x400000` = `KERN_SIZE`。文档 §5.4 M12 的"三架构实跑全 PASS"在本轮工件的复跑下成立（但"本轮"本身不可从文档内自证）。
- `readelf -lW` 三工件：x86_64 5 条 LOAD、aarch64/riscv64 各 4 条；首段 PhysAddr 分别为 `0x200000`/`0x40200000`/`0x80200000`；x86_64 第 2 段是 `.lrodata.*`、第 3 段是 `.rodata + .eh_frame_hdr`、第 5 段是 `.bss`（NOBITS）。§6.2 第一组的全部读数成立。
- `printf` 复算：`0xF0400000/0x400000=961`、`0xF0400000-0x00400000=0xf0000000`、`(0xFFFFFFC000000000>>30)&0x1FF=0x100`、`(0xFFFFFC0000000000>>30)&0x1FF=0x0`、`(0xFFFFC00000000000>>30)&0x1FF=0x0`，与 §2.1/§4.1 一致。
- `grep -c ': AT(' os/kernel-image/*.ld os/kernel/src/arch/*/link.ld` → 4/4/4/0/0/0；放宽成 `AT(` → 4/5/5/0/2/0，与 §6.2 第二组逐字一致。

---

## FINDING RD1-FACT-01（P0）

- 位置：文档 §3.2 第 406 行、§5.1 第 1108 行；源码 `/home/xzhao/github/minix-rs/os/boot-shim/src/loader.rs:422`
- 严重度：P0
- 问题：文档两次引用 `fn load_boot_modules_with_loader`，该符号在现行源码中不存在；现名是 `pub fn load_boot_modules_into`。
- 影响：读者按锚点 grep 找不到函数，"模块按原字节整块拷贝、不经 ELF 解析"这条论证失去可核对的代码落点；§5.1 说两条模块装载测试"走 `load_boot_modules_with_loader`"，实际测试调用的是 `load_boot_modules_into`（`test_load_boot_modules_loads_all_in_contract_order`、`test_load_boot_modules_panics_on_missing`）。
- 证据或推理：`tools/anchor-resolve.sh --check` 对该锚点输出 `ZERO-DEF（符号不存在/已改名：fn load_boot_modules_with_loader in os/boot-shim/src/loader.rs）→ 按 P0-fact 处理`；`rg -n "fn load_boot_modules" os/boot-shim/src/loader.rs` 只命中 `422:pub fn load_boot_modules_into<L: FileLoader>(`。E1。
- 最小修复：两处改成 `load_boot_modules_into`；重跑 `tools/anchor-resolve.sh --check` 与 `cargo test -p boot-shim --lib -- --list`。
- 置信度：高
- 是否需源码核实：否
- 若我错了：如果仓库另有 `load_boot_modules_with_loader` 的薄包装或别名——`rg` 全仓未命中（除历史笔记）。
- 对应判据：文档-代码同步七项之"符号锚点"；anchor-resolve 的 ZERO-DEF 硬规则。

## FINDING RD1-FACT-02（P0）

- 位置：文档 §4.4 第 1037–1053 行（代码引用块）；源码 `/home/xzhao/github/minix-rs/os/kernel-image/src/bootface.rs:427-439`、`/home/xzhao/github/minix-rs/os/boot-shim/src/lib.rs:101-112`
- 严重度：P0
- 问题：引文中的 `build_kernel_info(...)` 是旧签名：10 个实参，含 `PhysBytes(0)` 与 `0`；现行签名是 11 个形参，实际调用是 `build_kernel_info(memmap, reserved, VirBytes(KERN_VIRT_BASE), PhysBytes(KERN_PHYS_BASE), KERN_SIZE, modules, platform_sources, param_buf, sources, startup_objects, minix_boot::InputObjectId::DtbBootInput)`。
- 影响：读者据此认为 `sources`/`startup` 记录是在调用之外另行组装的旁支（引文头注也这么写："省略号处为 …sources/startup 记录组装"），实际上它们是 `build_kernel_info` 的实参；`PhysBytes(0), 0` 在现签名里没有对应位置。§4.4 的"组装 KernelInfo"链路失去可核对落点。
- 证据或推理：`os/boot-shim/src/lib.rs:101` 起的形参表为 `memmap, reserved_regions, kern_virt_base, kern_phys_base, kern_size, boot_modules, platform_sources, param_buf, memmap_sources, startup_objects, authority`；bootface 调用处多出 `sources, startup_objects, InputObjectId::DtbBootInput` 三个实参；`rg -n "PhysBytes\(0\)" os/kernel-image/src/bootface.rs` 在该调用附近无命中。E1。
- 最小修复：按现行 11 实参重写节选（或把 `...` 放在实参列表内部并注明形参已扩），同时改掉头注里"sources/startup 记录组装"的省略说明。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若存在另一个同名 `build_kernel_info` 重载/旧版本被条件编译——`rg -n "build_kernel_info" os` 只有 `lib.rs` 一处定义。
- 对应判据：引文块逐字一致 + 省略必须标注（D-045）；文档-代码同步。

## FINDING RD1-FACT-03（P1）

- 位置：文档 §5.1 第 1106 行、第 1097 行、§5.4 第 1156 行（M7 的"loader 18 测试"）；源码 `os/boot-shim/src/loader.rs`、`os/libs/minix-elf/src/lib.rs`
- 严重度：P1
- 问题：三处测试计数过期：`loader.rs` 写 18（实际 22 个 `#[test]`，文档自己的命令 `grep -c '    fn test_'` 输出 21）；`cargo test -p boot-shim --lib` 写 113（实际 125）；文档 §5.4 M7 又复述"loader 18 测试"。
- 影响：§5.1 的"复算"命令与正文互相矛盾——读者复制命令即得到不同数字，整段"测试覆盖"论述的核对价值降为零；§5.4 M7 的覆盖结论引用的基数（"loader 18 测试"）不可信。
- 证据或推理：`grep -c '    fn test_' os/boot-shim/src/loader.rs` → `21`；`grep -c '#\[test\]'` → `22`（多出 `source_destination_overlap_stops_before_any_segment_write`，无 `test_` 前缀）；`cargo test -p boot-shim --lib -- --list` → `125 tests`，`grep -c '^loader::tests::'` → `22`。minix-elf 的 23 与 `cargo test -p minix-elf -- --list` 一致。E1。
- 最小修复：18→22、113→125，或把"复算"命令改成 `grep -c '#\[test\]'` 并同时改正文与 M7；改完重跑 `cargo test -- --list` 对账。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若文档的"默认特性"指某个隐式 `--no-default-features` 组合——我按无额外 flag 跑得 125；即使按 `--no-default-features` 也需实跑取证，不能是 113 与 18 同源。
- 对应判据：文档-代码同步七项之"测试计数"；测试名对账。

## FINDING RD1-FACT-04（P1）

- 位置：文档 §5.1 第 1106、1108 行；源码 `os/boot-shim/src/loader.rs:874-970`
- 严重度：P1
- 问题：文档两次说"五个 `first_overlapping_pair_*`"，实际有六个：`disjoint`、`overlap`、`abutting`、`ignores_zero_length`、`returns_first`、`empty`。
- 影响："区间判交叠"的测试清单漏掉零长段用例；读者据此认为"零长段不算重叠"没有测试，与 §3.2/§4.2 反复强调的 `memsz == 0` 跳过逻辑恰好相反——这是该纯函数最容易被改坏的分支。
- 证据或推理：`grep -n 'fn test_first_overlapping_pair' os/boot-shim/src/loader.rs` 命中 6 个；`cargo test --list` 的 `loader::tests::test_first_overlapping_pair_*` 也是 6 条。文档列举的五项（不相交、真重叠、相邻不算、首个命中、空表）缺 `ignores_zero_length`。E1。
- 最小修复：改"五个"为"六个"，补 `test_first_overlapping_pair_ignores_zero_length`；同段的"十二个"相应改为"十三个"或改写为"其中与本节直接相关的是……（列举）"。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若把 `ignores_zero_length` 算进"两个 first_source_destination_overlap_*"那组——名字前缀不同，按文档自己的模式 `first_overlapping_pair_*` 它确实命中。
- 对应判据：测试名对账。

## FINDING RD1-FACT-05（P1）

- 位置：文档 §4.3 第 928–933 行（riscv64 代码引用块）；源码 `/home/xzhao/github/minix-rs/os/kernel/src/arch/riscv64/higher_half.rs:92-97`
- 严重度：P1
- 问题：引文写成 `kinfo.kern_virt_base()` 与 `kinfo.kern_phys_base()`，现行源码是字段访问 `kinfo.kern_virt_base.0`、`kinfo.kern_phys_base.0`；`KernelInfo` 没有这两个同名访问器方法。
- 影响：引文标注"保留 delta 计算"，但代码与现行源码不一致；读者若按引文去搜方法定义会一无所获，且会误以为类型上存在访问器封装（这正是该篇反复强调"按值交接/字段语义"时要避免的误导）。
- 证据或推理：源码：
  `let delta = kinfo.kern_virt_base.0.checked_sub(kinfo.kern_phys_base.0)...`；
  `rg -n "fn kern_virt_base|fn kern_phys_base" os` 无命中（`kernel_info.rs` 只有 `pub kern_virt_base: VirBytes` 字段）。E1。
- 最小修复：把两处 `()` 去掉，按源码逐字重录；或明确标注"此处为示意，现行源码用字段访问"。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若 `Deref`/扩展 trait 提供了同名方法——`rg` 未见；且引文里的 `.0` 紧跟在 `()` 后，即使有方法也不是这个形状。
- 对应判据：引文块逐字一致（D-045）；文档-代码同步之符号/签名。

## FINDING RD1-FACT-06（P1）

- 位置：文档 §4.2 第 838–861 行（`load_segments_into_phys_memory` 引用块）与第 863 行；源码 `os/boot-shim/src/loader.rs:286-316`
- 严重度：P1
- 问题：引用块头注写"代码引用（保留函数体）"，但函数体里 `first_source_destination_overlap` 的整段源/目标交叠检查（11 行，命中即 `panic!("boot-shim: kernel ELF segment … source overlaps … destination LMA")`）被删掉且未用省略号标注；随后正文也说"三件事按顺序"，只列了收集 `Vec`、`first_overlapping_pair` 重叠检测、逐段拷贝。
- 影响：读者会认为物理地址版只有一处重叠防线（返回 `InvalidSegment`），而实际上还有一处"源字节区间 × 目标 LMA 区间"交叠的 panic 防线；§4.2 末尾"去掉重叠检测会怎样"只讨论前者，覆盖不到后者。这是被审函数行为合同的一部分，不是排版省略。
- 证据或推理：源码 292–302 行在 `first_overlapping_pair` 之后、拷贝循环之前，含 `src_base = elf_data.as_ptr() as u64`、`first_source_destination_overlap(...)`、`panic!(...)`；引用块直接从 `if let Some(_) = first_overlapping_pair` 跳到 `for seg in &segments`。E1。
- 最小修复：在引用块的对应位置加 `...`（并改头注为"节选"），或在正文"三件事"后补第四件并给出 `:fn first_source_destination_overlap` 锚点。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若该段是近期新增、文档基线早于它——文档要求在现基线核对，仍应改；且"保留函数体"的措辞使它成为未声明省略。
- 对应判据：引文块逐字/省略标注（D-045）；行为合同（错误路径）覆盖。

## FINDING RD1-FACT-07（P1）

- 位置：文档 §4.4 第 1055 行；源码 `/home/xzhao/github/minix-rs/os/kernel-image/src/bootface_common.rs:177`（定义）、`bootface.rs:296` / `bootface_a64.rs:272`（调用）
- 严重度：P1
- 问题：文档说"模块字节的搬运用 `bootface.rs` 的 `fn place_modules`"，该函数不在 `bootface.rs` 里定义，而是 `bootface_common.rs` 的 `pub(crate) fn place_modules`；`bootface.rs` 只有 `bootface_common::place_modules(...)` 调用。
- 影响：anchor-resolve 判 ZERO-DEF；读者在 `bootface.rs` 里找不到函数体，且 §7.3 的 Rust 源码清单完全没有 `bootface_common.rs`，覆盖清单漏掉模块放置逻辑的真正宿主。
- 证据或推理：`rg -n "fn place_modules" os/kernel-image/src` → 只有 `bootface_common.rs:177`；`rg -n "place_modules" os/kernel-image/src/bootface.rs` → 仅 `296: if bootface_common::place_modules(`。`tools/anchor-resolve.sh` 输出 `ZERO-DEF（fn place_modules in os/kernel-image/src/bootface.rs）`。E1。
- 最小修复：路径改成 `bootface_common.rs:fn place_modules`（或写"`bootface.rs` 调用的 `bootface_common::place_modules`"），并把 `os/kernel-image/src/bootface_common.rs` 补进 §7.3。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若 `bootface.rs` 里有一个同名 `use` 别名使锚点解析失败——`use` 不带 `fn` 定义，anchor-resolve 仍会 ZERO-DEF。
- 对应判据：符号锚点（ZERO-DEF）；路径同步；覆盖率清单。

## FINDING RD1-FACT-08（P1）

- 位置：文档 §4.2 第 867 行；源码 `os/kernel-image/src/bootface.rs`、`os/kernel-image/src/bootface_a64.rs`、`os/boot-shim/src/opensbi_helpers.rs:574,606-617`
- 严重度：P1
- 问题：这一段把 `bootface.rs` 的 `MODULE_TABLE`、`PLATFORM_SOURCE` 与 `bootface_a64.rs` 的 `MODULE_TABLE`、`PLATFORM_SOURCE` 列为 `.bss` 里的 `static mut`——这四个引用在现行两个文件里全部不存在（模块/平台记录已改由 `BootStorageCell` 的 `STORAGE` 承载）；它同时把"bump 分配器的游标"列为 `.bss` 里"从非零值起步"的例子，但 riscv 侧的游标是 `opensbi_helpers.rs:574` 的 `static BUMP_NEXT: AtomicU64 = AtomicU64::new(MODULE_REGION_BASE)`（非零初值 → `.data`），a64 侧是 `bootface_a64.rs:111` 的 `static mut BUMP_PTR: u64 = BUMP_BASE`（同样非零初值 → `.data`），两者的初值都由镜像的 `.data` 文件字节写入，不依赖 BSS 清零。
- 影响：这一段论证"为什么 BSS 必须清零"依赖具体落点举例；被点名的 4 个符号 grep 不到，"bump 游标首分配返回乱址"的场景也不成立（游标在 `.data`，装载器按 `filesz` 写过初值）。结论（`.bss` 必须清零）仍可由 `IMAGE_HEAP`（全零初值 → `.bss`）与内核侧启动全局支撑，但现有举证链需要重写，否则读者按例子核对会得出相反结论。
- 证据或推理：`rg -n "MODULE_TABLE|PLATFORM_SOURCE" os/kernel-image/src` 无命中；`rg -n "static mut" os/kernel-image/src/*.rs` → `main.rs:521 static mut IMAGE_HEAP: [u8; IMAGE_HEAP_LEN] = [0; …]`、`bootface_a64.rs:111 static mut BUMP_PTR: u64 = BUMP_BASE`；`opensbi_helpers.rs:574 static BUMP_NEXT: AtomicU64 = AtomicU64::new(MODULE_REGION_BASE)`；`rg -n "const BUMP_BASE" os/kernel-image/src/bootface_a64.rs` → `DRAM_BASE + 0x0400_0000`（非零）。E1。
- 最小修复：删掉不存在的 `MODULE_TABLE`/`PLATFORM_SOURCE` 与"bump 游标"例子，改用 `IMAGE_HEAP` 与确实位于 `.bss` 的内核启动全局；若要保留 bump 场景，需先证明该游标确实落在 `.bss`。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若链接器把非零初值的 `AtomicU64`/`u64` 放进 `.bss`（不会：非零初值必须在文件里有字节）；或若某个宏展开再生成 `MODULE_TABLE`——`rg` 全目录无命中。
- 对应判据：文档-代码同步之"静态可变"与符号锚点；行为论证的证据链。


## FINDING RD1-FACT-09（P1）

- 位置：文档 §2.3 第 229 行（逐行解码第 2 条）、§4.3 第 897 行；源码 `/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/head.S:70-90`、`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/pre_init.c:217-238`
- 严重度：P1
- 问题：文档断言"换栈后调用 `kmain` 时，它的序言保存的是 `pre_init` 的帧指针（跨过换栈点、指向后来会被回收的低侧栈），不是 0"，并据此对比 Rust 的 `xor rbp, rbp` "位置不同，效果也不同"。这与 i386 SysV ABI 冲突：`head.S` 在 `call pre_init` **之前**已 `mov $0, %ebp`，EBP 是 callee-saved，`pre_init` 正常返回时恢复为 0；因此 `kmain` 序言压入的保存槽同样是 0，链同样在 0 处终止。
- 影响：§2.3 第 2 条与 §4.3 "位置不同，效果也不同"的核心对比失去依据；读者会以为 C 侧首帧必须靠防环判据（`hbp <= bp`）才能停，从而高估 Rust `xor rbp` 与 C 的差异，也误解"压进栈的 0 槽不在帧指针链上"这句话在 C 侧的对位。
- 证据或推理：`head.S:70` `mov $0, %ebp`；`head.S:77` `call _C_LABEL(pre_init)`；`head.S:84-87` 换栈后 `mov $k_initial_stktop, %esp`/`push $0`/`push %eax`/`call _C_LABEL(kmain)`；`pre_init` 是普通 C 函数（`pre_init.c:217`，返回 `kinfo_t *`，`:237 return &kinfo;`），ABI 要求恢复 EBP。E3（未反汇编 Minix3 内核；仓内无 C 构建产物）。
- 最小修复：改为"C 在进入 `pre_init` 前清 EBP，`pre_init` 返回后 EBP 仍为 0，`kmain` 的保存槽同样是 0；Rust 的差别只是把清零放到换栈之后"；或删掉该对比，只保留"两侧都在 0 处终止"。
- 置信度：中
- 是否需源码核实：是（反汇编 Minix3 内核 `pre_init` 尾部，确认 EBP 恢复）
- 若我错了：若 Minix3 的 `pre_init` 被编译成不遵守 callee-saved EBP，或在尾部直接跳入 `kmain` 而不返回——这需要反汇编证据，现有源码与 ABI 都不支持。
- 对应判据：事实断言/行为合同；反事实前提真实性。

## FINDING RD1-FACT-10（P1）

- 位置：文档 §3.3 第 519 行；源码 `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/boot_storage.rs:356-380`、`os/kernel/src/lib.rs:427-433`
- 严重度：P1
- 问题：文档说栈顶公式"在 boot-shim 的共享 builder 里……没有第二份实现"；但 `kern_stack_top = kern_virt_base + kern_size` 在 `minix_boot::BootStorage::report()` 里还有一份实现，v1 发送路径消费的正是它（`BootReport`），不是 `build_kernel_info`。
- 影响：读者会认为"栈顶只有一个公式落点"；实际是 v1 面 `minix-boot::BootStorage::report`、direct 面 `boot-shim::build_kernel_info` 两处各算一次，必须同步维护——这恰是 §3.3 论证"单一来源"时最该点明的例外。§4.4 的"组 KernelInfo（carrier）→ 写 v1 arena"也把 v1 侧算式的宿主略过了。
- 证据或推理：`boot_storage.rs:378` `kern_stack_top: VirBytes(kern_virt_base.0 + kern_size),`（同一文件 353 行文档注释重复该公式）；`prepare_kernel_image_v1(report: &minix_boot::BootReport<'_>, …)` 消费 `report.kern_stack_top`（`handoff_v1_write.rs:145-166` 把它写进 arena）。E1。
- 最小修复：把"没有第二份实现"限定为"`build_kernel_info` 在两族 boot-shim 间只有一份"；另补一句 v1 发送侧的 `BootStorage::report` 是同一公式的第二处，二者必须锁步。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若 v1 生产路径不使用 `BootStorage::report`——`prepare_kernel_image_v1` 的形参类型就是 `&BootReport`，且 UEFI/U-Boot 生产入口构造它。
- 对应判据：文档-代码同步；事实断言。

## FINDING RD1-FACT-11（P2）

- 位置：文档 §1.1 第 48 行；源码 `/home/xzhao/github/minix-rs/minix3/share/mk/sys.mk:25-29`、`/home/xzhao/github/minix-rs/minix3/minix/kernel/const.h:33-43`
- 严重度：P2
- 问题：文档写"`USR_DATATOP` 在 `_MINIX_MAGIC` 构建下取 `0xE0000000`（该宏由 `minix3/share/mk/sys.mk` 定义）、否则取 `0xF0000000`"，省略了定义条件：`_MINIX_MAGIC` 只在 `MKMAGIC=yes` 或 `MKASR=yes` 时定义，两者默认 `no`；下一句"默认构建的用户数据顶是 `0xF0000000`"正依赖这个条件。
- 影响：按字面"sys.mk 定义该宏"会推出默认构建取 `0xE0000000`，与文档自己给出的默认值结论矛盾；读者无法判断"默认构建"到底走哪个分支。
- 证据或推理：`sys.mk:25` `.if ${MKMAGIC:Uno} == "yes" || ${MKASR:Uno} == "yes"`，下一行 `CPPFLAGS+= -D_MINIX_MAGIC=1`；`const.h:34-38` 的 `#ifndef _MINIX_MAGIC` 分支给 `0xF0000000`。E1。
- 最小修复：改为"该宏由 `sys.mk` 在 `MKMAGIC=yes`/`MKASR=yes` 时定义（两者默认 no）"。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若发行构建把 `MKMAGIC=yes` 设为默认——仓内默认是 `Uno`；发行配置不在本仓，文档若指发行版应写明来源。
- 对应判据：常量/构建条件同步。

## FINDING RD1-FACT-12（P2）

- 位置：文档 §2.1 第 124 行；源码 `/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/usermapped_glo_ipc.S:10`、`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/usermapped_data_arch.c:4,14,24`
- 严重度：P2
- 问题：括号里写"`i386/usermapped_glo_ipc.S` 与 `i386/usermapped_data_arch.c` 里带 `__section(".usermapped")` 的三个 `minix_ipcvecs_*` 对象"——三个 `__section(".usermapped")` 对象只在 `usermapped_data_arch.c`；`usermapped_glo_ipc.S` 用的是 `.section .usermapped_glo`，供的是另一个输出段。
- 影响：读者按 `__section(".usermapped")` 去 `usermapped_glo_ipc.S` 找会落空；两个 usermapped 段的供料方被混成一句，正好抹掉 `.usermapped_glo` 的存在——而它是五处 `AT()` 的第一处，也是"`.text` 的 VMA 严格大于 `0xF0400000`"推理里的一段。
- 证据或推理：`usermapped_glo_ipc.S:10` `.section .usermapped_glo, "wax", @progbits`；`usermapped_data_arch.c` 的三个 `struct minix_ipcvecs … __section(".usermapped")`。E1。
- 最小修复：拆成"`usermapped_glo_ipc.S` 供 `.usermapped_glo`；`usermapped_data_arch.c` 的三个 `minix_ipcvecs_*` 供 `.usermapped`"。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若 asm 宏展开后另有 `.section .usermapped` 字样——`rg` 该文件只有 `.usermapped_glo`。
- 对应判据：路径/符号与事实；引文归属。

## FINDING RD1-FACT-13（P2）

- 位置：文档 §4.3 第 910 行（aarch64 asm 引用块内注释）；源码 `/home/xzhao/github/minix-rs/os/kernel/src/arch/aarch64/higher_half.rs:47`
- 严重度：P2
- 问题：引用块写 `"isb", // drain write buffer before br`，源码是 `"isb", // defensive barrier; enable() already synchronized TTBR/SCTLR`；代码行一致，注释被改写，且块头注只说"省略号处为注释"，未声明注释被改写。
- 影响：读者会把 `isb` 的理由记成"排空写缓冲"，而 §4.3 正文与源码都把它定位为"防御性屏障，正确性已由 `enable()` 的 `dsb/isb` 保证"；同一节内两种理由并存。
- 证据或推理：源文件第 47 行逐字如上；文档第 910 行。E1。
- 最小修复：逐字抄源码注释，或删掉该注释只留省略号。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若源码注释近期改过而文档是旧基线——按现基线仍须同步。
- 对应判据：引文块逐字（D-045）。

## FINDING RD1-FACT-14（P2）

- 位置：文档 §3.1 表第 372 行、§4.4 表第 1030 行；源码目录 `/home/xzhao/github/minix-rs/os/qemu-tests/test-kernels/kernel/bootstrap/`
- 严重度：P2
- 问题：表格把 x86_64/aarch64 测试载体归为"载体单地址脚本"，但这两个载体目录没有 `link.ld`（只有 riscv 载体有），它们沿用工具链默认布局；同节第 356 行自己写的是"其中一部分目录带自己的 `link.ld`，其余沿用工具链默认布局"。
- 影响：读者按表去 `test-higher-half/link.ld` 找脚本会落空；"三种链接脚本形态"的分类在这一格与本节自己的说明不一致。
- 证据或推理：`find os/qemu-tests/test-kernels/kernel/bootstrap -maxdepth 2 -name link.ld` 只命中 11 个 riscv 载体目录；`ls test-higher-half test-higher-half-aarch64` 只有 `Cargo.toml` 与 `src`。E1。
- 最小修复：该格改为"载体单地址（工具链默认布局）"，或把有脚本/无脚本拆成两行。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若这两个载体的 `Cargo.toml` 通过 build script 传 `-T` 指向别处脚本——两目录均无 `build.rs`（`ls` 未见）。
- 对应判据：路径与事实；表内自洽。

---

## SUGGESTION

1. §2.1 第 112 行"不给 `AT()` 的输出段，它的装载地址与虚拟地址相同"作为通则不准确：GNU ld 对没有 `AT` 的输出段是"继承前一个同 region 输出段的 VMA−LMA 差"，不是恒等于 VMA。本脚本里无 AT 的段都在首个 AT 之前，结论成立。建议加限定"在该段之前没有带 `AT()` 的输出段时"。
2. §2.2 第 157 行"名单里的库对象（`klib.o`、`io_inb.o`、`assert.o` 等）"把本目录普通对象（`klib.o`、`io_inb.o`）称作库对象；建议改为"名单里的对象（`assert.o` 等库对象，以及 `klib.o`/`io_inb.o` 等本目录对象）"。
3. §5.1 第 1103 行把 `test_entry_point`、`test_machine_type` 归入"段字段"族，二者实际测的是 `Elf64Ehdr` 字段；建议族名改"头字段与段字段"。
4. §4.1 第 356 行"把检索面扩到全仓的 `*.rs`/`*.toml`/`*.sh`/Makefile 后仍无命中——只有注释引用脚本里的常量"：`os/kernel/tests/link_ld_lockstep.rs` 以文件读取方式引用这两份脚本（不是链接输入，但也不是注释）；建议改成"无构建/链接读者；只有注释与锁步测试读取"。
5. §6.2 第五组 GDB 的"预期输出"依赖具体构建产物，属一次性现场指引；建议标注"读数随工件变化，按 `nm`/`readelf` 现场对账"。

---

## 覆盖率缺口（文档声称覆盖 vs 实际未覆盖）

| 被声称覆盖的对象 | 缺口 | 关联 |
|---|---|---|
| `bootface.rs` 的 `place_modules` | 真正宿主 `bootface_common.rs` 不在正文锚点、不在 §7.3、不在 doc-code-map | RD1-FACT-07 |
| `KernelInfo.kern_stack_top` 的算式 | v1 发送侧 `minix_boot::BootStorage::report` 是第二处，未进 §3.3 的"单一来源"论证 | RD1-FACT-10 |
| v1 交接写侧 | §4.4 正文隐含 `handoff_v1_write`（写 arena），§7.3 只列 `handoff_v1_read.rs`；写侧是链路的一半 | 建议补清单 |
| `pg_clear`/`pg_identity`/`pg_load`/`vm_enable_paging` | §2.2/§2.3 只作"五个函数"列举；按 §1.3，页表建立归 01 篇，属声明范围外，不算缺口 | — |
| `tss_init`、`k_boot_stack`、`k_stacks_start/end` | §2.4 一句带过；与本节"落点判定"弱相关，可接受 | — |

---

## 阴性与覆盖声明（扫了什么 / 发现数 / N=0 不覆盖什么）

1. **C 侧常量与宏**：扫了 i386/earm 的 `kernel.lds`、`Makefile.inc`（`.for` 块 + 三个清单）、`head.S`、`mpx.S`、`pre_init.c`、`pg_utils.c`、`protect.c`、`main.c`、`const.h`、`proto.h`、`asm.h`、`arch_proto.h`、`vm.h`、`sys.mk`、`vm/pagetable.c`、`libsys/stacktrace.c`、`exception.c`、两个 usermapped 文件。发现数 = 2（RD1-FACT-11/12）+ 1（RD1-FACT-09 的 C 帧指针，归行为断言）。N=0 的部分：五处 `AT()`、`961`、`0xF0000000`、`13/3/2` 与 27 个对象、`_kern_phys_base=0x80200000`、`I386_PAGE_SIZE=4096`、`LABEL/_C_LABEL/_LABEL` 三跳、`K_STACK_SIZE`、`USR_DATATOP/STACKTOP`、`kmain` 宏改名、`tss_init(0,&k_boot_stktop)`、`util_stacktrace`/`proc_stacktrace_execute` 的 `hbp<=bp`、`pt_init` 的 BIGPAGE 跳过、`add_memmap` 位置与注释——全部逐字核对为真。**不覆盖**：未点名对象的同族行为（如其它 i386 栈、未引用的 Makefile 条件分支、earm 与 i386 未提及的差异）。
2. **Rust 常量/签名/行为**：扫了 3 份生产 ld、3 份 arch ld、`boot_policy.rs`、四份 `higher_half.rs`、`kernel/src/lib.rs` 的 `arch_boot*`/`boot_validate_and_prepare`/`store_kernel_info`/`kmain_verify`/`init_protection`/`init_clock_and_interrupts`/`resume_additive`、`loader.rs` 全函数、`minix-elf` 全解析路径、`bootface.rs`/`bootface_a64.rs`/`bootface_common.rs`、`build.rs`、`main.rs`、`check-layout.sh`、`link_ld_lockstep.rs`、`test-higher-half` 三份 main.rs、`run_qemu.sh`。发现数 = 6（RD1-FACT-02/05/07/08/10/14）。N=0 的部分：三架构 `HUGE_PAGE_SIZE=1GiB`/fallback=2MiB、`plan_kernel_mapping` 阶梯与两条错误消息、`kern_stack_top` 16 字节断言、x86/a64/riscv 的 asm 序列与参数寄存器、`kmain_verify` 三条断言与 FAIL/INFO 串、`check-layout.sh` 的 L0–L9 编号与 L4 消息、`readelf` 的 5/4/4 条 LOAD 与段名、`check-layout.sh all` 全 PASS。**不覆盖**：页表实现内部（`paging.rs`/`direct_map.rs` 只核了常量与 `AlreadyMapped` 的存在）、QEMU 实机行为、非当前工件的其它构建配置。
3. **测试名与计数**：`cargo test -p minix-elf -- --list` = 23；`cargo test -p boot-shim --lib -- --list` = 125（`loader::tests::` 22）；源码计数 23 / 21（`fn test_`）/ 22（`#[test]`）。发现数 = 2（RD1-FACT-03/04）。N=0 的部分：minix-elf 四族 23 个测试名全部存在；文档点名的 loader 测试全部存在（但 `first_overlapping_pair_*` 实际有 6 个，文档写 5 个，见 04）；`test-higher-half` 的 PASS/FAIL 串存在。**不覆盖**：其它 crate 的测试、真机测试实际运行结果。
4. **引文块逐字**：机械门只检查 32 块中的 1 块；本轮人工比对了 19 个代码/汇编/脚本引用块（`kernel.lds`、`Makefile.inc` ×2、`head.S` ×2、`mpx.S`、`pg_mapkernel`、`x86_64.ld`、`riscv64.ld`、`build.rs`、`parse_ehdr`、`SegmentIter`、`compute_kernel_layout`、`load_segments_into_phys_memory`、x86/a64/riscv 三份 asm、`kmain_verify`、`bootface`）。发现数 = 4（RD1-FACT-02/05/06/13）。**不覆盖**：其余被 citation-check 跳过的 31 块（多为表格与 `text` 示意图），未逐字比对。
5. **路径**：doc-code-map 27/27 存在；另核 §7.2 的全部 C 路径与 §7.3 的全部 Rust 路径均存在。发现数 = 1（§7.3 缺 `bootface_common.rs`，并入 RD1-FACT-07）。**不覆盖**：存在性不等于内容对应（内容见各 finding）。
6. **反事实前提**：逐条核了 §2.1 两处、§2.2 两处、§2.3 三处、§2.4 一处、§3.1 一处、§3.2 一处、§3.4 一处、§3.5 一处、§4.1 两处、§4.3 三处。发现数 = 1（RD1-FACT-09）。其余前提均真实存在，详见下表。
7. **工作区前缀与内部 TODO**：文档内全部 `os/...`、`minix3/...` 引用均以仓库根为前缀，与 anchor-resolve / doc-code-map 的解析一致（27/27 命中）；未发现 `os/os/` 或相对路径混用。内部 TODO 编号 `R02-T01/T03/T04` 在 §5.4 引言、表内与"新 TODO"清单三处一致，未发现编号漂移；按任务禁读约束，**未**对照 todo 台账核对登记状态（N=0 不覆盖登记真实性）。

### 反事实前提核对表

| 位置 | 前提 | 判定 |
|---|---|---|
| §2.1 | 五处 `AT()`；`.bss` 的 `p_filesz=0` | 真（lds 5 处 AT、`.bss` NOBITS） |
| §2.1 | 删掉全部 `AT()` 后 LMA=VMA | 真（所有带 AT 的段一起删时，前序无 AT 段平移为 0） |
| §2.2 | `pg_utils.o` 在改名名单；`pre_init` 依次调 `pg_clear/pg_identity/pg_mapkernel/pg_load/vm_enable_paging` | 真（Makefile.inc `.for` 首行；pre_init.c:230-234） |
| §2.2 | `kinfo.bootstrap_start/len` 覆盖 unpaged 段；`kmain` 在 main.c | 真（pre_init.c:114-116） |
| §2.3 | `hang: jmp hang` 在 `call kmain` 下；`load_stack` = 4096 字节 | 真（head.S:87-96） |
| §2.4 | `mpx.o` 不在改名名单；`.unpaged_bss` 只收 `unpaged_*.o(.bss COMMON)` | 真（Makefile.inc、kernel.lds:19） |
| §3.1 | `os/kernel/src/arch/*/link.ld` 无 `AT()`、无链接读者 | 真（`grep -c ': AT('` = 0；检索无构建读者） |
| §3.2 | `SegmentIter::next` 用 `e_phoff + index*e_phentsize`；`parse_phdr` 用 entsize | 真（lib.rs:355-395、246-270） |
| §3.4 | `jump_to_kmain` 返回 `!`；x86 `call` 无运行时检查 | 真 |
| §3.5 | `.bss` 末尾 `ALIGN(0x200000)`；收口前 `__bss_end` 非 2MiB 对齐 | 真（三工件 nm：`__bss_end - base` = `0x28fa00`/`0x2745b0`/`0x25fc10`） |
| §4.1 | `0xFFFFFC0000000000`/`0xFFFFC00000000000` 的 VPN[2]=0；恒等映射覆盖低 4GiB（VPN[2]=0..3） | 真（`printf` 复算 0x0；1GiB/槽 × 4） |
| §4.1 | `.lbss` 在 `.bss` 通配里；`KEEP(*(.text.boot))` 在 `.text` 首位 | 真 |
| §4.3 | riscv 两条腿一条满足 PC=链接地址、一条不满足；源码注释读数 `0xffffffc00000477a` 与 `0xffffffbf7fe00000` 相加得 `0xffffff7f7fe0477a` | 真（`python3` 复算；注释里的 `0xffffffbf7ffe0000` 确实多一个 `f`，文档的更正正确） |

N=0 的总体边界：以上核对只覆盖文档点名或可机械复算的对象；未覆盖未点名的同族代码路径、未构建配置、以及需要真机运行才能观测的行为。

### 取证边界（自披露）

- 为定位两个 ZERO-DEF 符号，我执行过一次全仓 `rg -n "load_boot_modules_with_loader|place_modules" .`；该命令的输出顺带命中了历史笔记/台账里的若干行（`rewrite-notes/01-todo.md`、`rewrite-notes/coordination/…`）。未打开这些文件、未把其中任何评审结论用作证据；本报告对这两个符号的结论全部来自 `os/` 源码与 anchor-resolve 的工具输出。
- 本次未读取任何历史 scan.md、评审报告、expected/ 目录或其他面板角色的输出。

