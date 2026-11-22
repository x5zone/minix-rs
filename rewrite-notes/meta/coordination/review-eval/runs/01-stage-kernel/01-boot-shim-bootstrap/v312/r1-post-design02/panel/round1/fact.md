DOC 01-boot-shim-bootstrap.md

# 事实审查报告 — D01 V3.12 R1（post-DESIGN-02）

- 目标：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- sha256：`996ff161d02ef867151e780adb7243e49e7dc249b5b161d6d0c5ed229b5f22ec`（与 prompt 一致，2946 行）
- 角色：独立事实审查者（只读；本文件是唯一写入）
- 证据强度记号：E1 = 工具/命令输出；E2 = 源码逐行对照；E3 = 推理。
- 本轮相关代码清单已用 `tools/doc-code-map.sh` 复算为 49 个文件（含 `os/servers/rs/src/trap_api.rs`）。

## FINDING FACT-01（P2）

- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
  - §4.4 开头（第 2284 行）：“接收侧统一走 `store_boot_report`，先把切片深拷进内核 `.bss`……”
  - §4.0 小结（第 1888 行）：“接收侧 `store_boot_report` 落 `.bss` 后生成唯一的 `KernelInfo<'static>`”
  - §4.4 字段表 `source_records` 行（第 2324 行）：“接收侧 `store_boot_report` 落 `.bss`”
  - §4.4 字段表 `authority` 行（第 2325 行）：“接收侧 `store_boot_report` 参数；v1 接收侧由 wire 解码”
- 严重度：P2
- 问题：文档说“五条腿的接收侧统一走 `store_boot_report`”，但 v1 接收面（跨镜像三条腿共用的接收入口）实际调用的是 `store_kernel_info`，不是 `store_boot_report`；二者是 `land_and_store` 的两个并列包装，`store_boot_report` 只被直核面调用。
- 影响：读者按 §4.4 去找 v1 接收侧的落地调用点会找不到 `store_boot_report`；也会误以为接收侧只有一条包装路径，从而看不清文档自己在 §4.5 强调的“两个 store 调用点、同址跳过”的时序。该句还与 §3.7（第 1840 行）、§4.5（第 2397/2456 行）的“直核面 `store_boot_report` / v1 接收面 `store_kernel_info`”自相矛盾。
- 证据或推理（E1 + E2）：
  - `grep -n 'let kinfo = store_kernel_info\|let kinfo = store_boot_report' os/kernel/src/lib.rs` 输出：
    - `230:    let kinfo = store_boot_report(report, authority);`（在 `arch_boot_finish`，直核面）
    - `325:    let kinfo = store_kernel_info(kernel_info);`（在 `resume_additive`，v1 接收面）
  - `grep -n 'fn store_kernel_info\|fn store_boot_report\|fn land_and_store' os/kernel/src/lib.rs`：1565 / 1571 / 1578；`land_and_store` 是两个入口的共同实现。
  - `arch_boot_resume_v1` 构造 `KernelInfo` 后调用 `resume_after_v1_decode` → `resume_additive` → 第 325 行 `store_kernel_info`，全链没有 `store_boot_report`。
- 最小修复：把上述四处改为“接收侧统一经 `land_and_store` 落 `.bss`：直核面入口是 `store_boot_report`，v1 接收面入口是 `store_kernel_info`”；或至少把 §4.4 首段的“统一走 `store_boot_report`”改成“统一落 `.bss`（两条入口见 §4.5）”。改后重跑 `tools/doc-citation-check.py` 与 `tools/anchor-resolve.sh --check`。
- 置信度：高
- 是否需源码核实：是（已核）
- 若我错了：唯一可能的反读是把 `store_boot_report` 当作“接收侧落地这一步”的泛称而不是函数名；但文档对 `store_kernel_info` 的使用是逐函数指名的（§4.5 引文块），且 §4.4 所在小节的标题就是“接收侧 `KernelInfo` 构造”，因此按函数名读是唯一自然读法。

## SUGGESTION

- 引文标注形态与工具的识别口径不一致：本文件有 15 个 C/asm/ld 引文块把标注写成多行 `/* … 代码引用/节选，path。 */`（如 171、246、287、356、459、495、550、619、684、748、783、813、873、908、1338 行），而 `tools/doc-citation-check.py` 的 `C_ANNOTATION_RE` 只认单行 `/* kind，path。 */`，因此这 15 块被计为 `SKIP（无引文标注）`，机械读数 `checked=20 warned=4 skipped=30` 并不覆盖它们。本轮已把这 15 块逐行与源码对照（全部一致，见下“5. 引文块”），所以不影响本轮任何 FINDING；但若要让“引文已对账”的读数真实覆盖，建议把标注改成单行形式（或在工具侧支持多行注释）。

## 机械门读数（E1，均在仓库根运行）

| 门 | 命令 | 读数 |
|---|---|---|
| 锚点解析 | `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `anchors=87 resolved=87 zero-def=0 multi-def=0`，exit 0 |
| 关联代码存在性 | `bash tools/doc-code-map.sh <doc> --check` | `共 49 个文件；存在性 49/49 通过`，exit 0 |
| 文档结构 | `python3 tools/doc-structure-check.py <doc>` | `✅ 1 个文件表结构与内部 .md 链接均可解析`，exit 0 |
| 章节引用 | `bash tools/doc-ref-check.sh <doc>` | `✅ 内部章节引用均可解析`，exit 0 |
| 文风 | `bash tools/doc-style-lint.sh <doc>` | `命中汇总：error级=0`，exit 0 |
| 引文对账 | `python3 tools/doc-citation-check.py <doc>` | `blocks=54 checked=20 failed=0 skipped=30 warned=4`，exit 0；4 个 WARN 均为“声明为非逐字引文、source-symbol 已核对、内容未逐行对账”，无 FAIL |

## 1. 锚点与路径

- 扫了什么：文档全部 87 个符号锚点（工具枚举）＋正文 15 个 `file:line` 式引用（`glo.h#L76`、`glo.h#L9-L12`、`kernel.h#L42`、`pre_init.c#L33`、`earm/head.S:22-24`、`earm/kernel.lds#L4`、`trap_api.rs:223-230`、`trap_api.rs:430`、`ebs_pair.rs:219-221`、head.S/kernel.lds 的 93–96/18/33–34 行等）＋§7.2/§7.4 的 C 与脚本路径。
- 发现数：0。87/87 解析；手工抽核的 15 个行号引用全部命中（glo.h 9–12/76、kernel.h 42、pre_init.c 33、earm/head.S 22–24/52–55、earm/kernel.lds 4、head.S 93–96、kernel.lds 18/33–34、ebs_pair.rs 219–221、trap_api.rs 223–230/430）。§7.2 的 C 文件与 §7.4 的脚本全部存在。
- N=0 时不覆盖：工具锚点只校验“符号存在”，不校验同一符号是否被用在正确的语义位置（例如 `fn main` 有多个同名实现时工具按文件分辨）；行号引用的正确性只抽核了上述 15 处，未穷举全文所有数字（正文里没有更多 `file:line` 形态）。

## 2. 文档-代码同步七项

1. **静态可变**：扫 `static mut`、`SyncUnsafeCell`、`PANIC_DIAG_ACTIVE`、`PT_ALLOC/PT_FREE`。文档 §4.5/§3.4.2 对 write-once 静态与一次性守卫的描述与源码一致（`land_and_store` 的 `KERNEL_INFO.is_none()` 断言、`pt_alloc::register` 的 `compare_exchange` panic、`TestMapTables` 的 `AtomicBool`）。发现 0。
2. **路径**：`doc-code-map --check` 49/49；另手工核了 §7.4 的 12 个脚本与 `check-layout.sh`、`os/boot-shim/riscv64-uboot.ld`。发现 0。
3. **常量**：逐项核对了文档出现的全部关键数字/常量（见下“4. 常量与签名”），无一处与源码冲突。发现 0。
4. **内部 TODO**：`grep -n 'TODO\|FIXME' <doc>` → 0 命中（文档正文无 TODO 标记）。发现 0。N=0 不覆盖：代码内的 `TODO`/历史注释不在本项射程。
5. **符号锚点**：见第 1 项，0。
6. **工作区前缀**：文档所有 Rust 路径都以 `os/` 起、C 路径以 `minix3/` 起；无裸 `kernel/src/...` 形态。发现 0。
7. **测试计数**：见第 3 项，四组读数与文档完全一致。发现 0。

## 3. 测试名对账

- 扫了什么：文档点名的 8 个宿主测试 + 4 个测试套读数 + §5.4/§6.4 的 5 个真机脚本及其判据串。
- 发现数：0。
  - `generates_arch_and_board_first`（boot_params.rs:216）、`rejects_internal_key_override`（:237）、`drops_tail_records_whole_at_capacity`（:276）、`splits_nul_and_whitespace_records`（:229）、`rejects_malformed_and_non_ascii`（:257）、`boot_windows_reject_reversed_ranges`（kernel lib.rs:5940）、`boot_windows_reject_empty_ranges`（:5947）、`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`（vm_handoff.rs:1088）——全部存在（`rg -n "fn <name>" os/`）。
  - 实测计数（`cargo test`）：`minix-boot --lib` 188 passed/0 failed；`boot-shim --lib` 136/0；`minix-kernel --features mock --lib` 866/0/8；`minix-elf --lib` 27/0。与 §5.2、§5.0 的 188/0、136/0、866/0/8、27/0 逐项一致。
  - §5.4 判据串：`test-shim-bootmarks-x86.sh` 的 5 个 MARK、`test-shim-bootmarks-aarch64.sh` 的 MARK1/2/3 + `DTB preflight failed`、`test-riscv64-uboot-shim.sh` 的 6 个 grep 标记、`test-riscv64-uboot.sh` 的 `### TEST_RESULT: PASS hello-boot-riscv64 ###`、`test-kernel-image-riscv64-bootface.sh` 的 `table holds 0x0000000000000000 entries`/`module placement failed` 全部在脚本里逐字存在。
- N=0 时不覆盖：真机门未实跑（无 QEMU/固件）；本项只对账“脚本里有没有这个判据串”，不证明脚本当前能通过。

## 4. 常量、签名、行为合同

- 扫了什么：文档里全部可数值化的常量与函数签名（约 60 项）。抽核结果：
  - C 侧：`MULTIBOOT_HEADER_MAGIC=0x1BADB002`、`MULTIBOOT_INFO_MAGIC=0x2BADB002`、`HAS_MEMORY=0x1`、`HAS_MODS=0x8`、`HAS_MMAP=0x40`、`MEMORY_AVAILABLE=1`、`MULTIBOOT_MAX_MODS=20`、`MAXMEMMAP=40`、`NR_MEMS=16`、`MULTIBOOT_PARAM_BUF_SIZE=1024`、`SERVARNAME/SERBAUDVARNAME="cttyline"/"cttybaud"`、`GDT_SIZE=6+CONFIG_MAX_CPUS=7`、`IDT_SIZE=256`、`DESC_SIZE=8`、`BIOS_MEM=[0,0x4FF]`、`BASE_MEM_TOP=0x90000`、`UPPER_MEM_END=0xFFFFF`、`PG_PAGETABLES=6`、`_kern_phys_base=0x400000`、`_kern_vir_base=0xF0400000`、earm `MB_MODS_BASE=0x82000000`/`MB_MODS_ALIGN=0x800000`/`_kern_phys_base=0x80200000`（差 30 MiB）——全部与 `minix3/` 源码一致。
  - Rust 侧：`BOOT_HANDOFF_V1_VERSION=10`、`ARENA_CAP=8*4096`、`ENVELOPE_CAP=256`、`WIRE_END=7872`、`WIRE_SOURCES_END=29184`、余量 3584、`WIRE_MEMMAP_CAP=128`、`WIRE_RESERVED_CAP=256`、`WIRE_MODULE_CAP=12`、`WIRE_PLATFORM_CAP=4`、`WIRE_PARAM_CAP=1024`、`WIRE_SOURCES_CAP=296`、`WIRE_SOURCE_RECORD_SIZE=72`、`MEMMAP_REGION_STORE_LEN=128`、`RESERVED_REGION_STORE_LEN=256`、`PLATFORM_SOURCE_STORE_LEN=4`、`PARAM_BUF_STORE_LEN=1024`、`BOOT_MODULE_NAME_LEN=16`、`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`VM_BOOT_HANDOFF_MAX_DEDUCTED=47`、`MAX_BOOT_REGIONS=32`、`NR_BOOT_MODULES=12`、`DTB_SOURCE_CAP=40`、`BOOT_PT_BUMP_PAGES=1024`、`PT_BUMP_PAGES=256`、`KERN_PHYS_BASE=0x80200000`/`0x40200000`、直核池 `0x8200_0000..0x8400_0000`/`0x4400_0000..0x4600_0000`、`KinfoStruct` 48 B（`user_sp` offset 8）、`KernelInfo` 10 字段、C `kinfo_t` 26 字段 → 22 删除台账——全部一致。
  - 签名：`store_kernel_info(&KernelInfo) -> &'static KernelInfo`、`store_boot_report(&BootReport, RamAuthority)`、`arch_boot_finish(&BootReport, RamAuthority, PhysBytes) -> !`、`enter_kernel_image_v1(PhysBytes, &BootReport, PhysBytes, &[PhysRange]) -> !`、`prepare_boot(&mut BootStorage, usize) -> BootPrepareResult<'a>`、`cut_memmap(cbi,start,end)`、`pg_map(phys,vaddr,vaddr_end,cbi)`、`alloc_lowest(cbi,len)`、`pg_alloc_page(cbi)`——全部一致。
- 发现数：1（即 FACT-01，属于“接收侧调用哪个函数的合同”而不是常量错误）。
- N=0 时不覆盖：未逐位复算 `MULTIBOOT_HEADER` 结构体在内存中的 8 字节对齐与栈指针对齐的 ABI 结论（文档自己也只把它们作为风险提示，且声明“没有保存反汇编证据”）；未编译 C 侧来实测 `__trampoline_end - trampoline` 是否恰为 2212 B（文档写“约 2212 B”，本轮只核到 2048+56+24 的构成与 <4096 的结论）。

## 5. 引文块

- 扫了什么：54 个 fenced block；其中 42 个声明了“代码引用/逐字引用/节选”。工具逐行对账覆盖 20 个（4 个 WARN 为声明式改写/译述）；被工具计为 SKIP 的 15 个 C/asm/ld 引文块（171/246/287/356/459/495/550/619/684/748/783/813/873/908/1338 行）由我按工具的同一子序列规则手工逐行复核（脚本见下注）。
- 发现数：0。15 个块的每一行（去注释/中文/省略号后）都能在对应源码中按序找到；`head.S` 的识别头与 `multiboot_init`、`get_parameters` 三段、`mb_set_param`、mmap 收集段（含原文件的混合制表符/空格缩进）、`add_memmap`、`cut_memmap`、`alloc_lowest`+`pg_alloc_page`、`copy_trampoline`、`protect.c` 的 VM 回收段、`overlaps`、`get_parameters` 后段、ARM `head.S`、`setup_mbi`、`kernel.lds` 节选——全部逐字一致；省略处都有 `...`/`…` 或显式“节选/略去”声明。
- 复核方法（只读）：自写 Python 以 `re.sub(r'\s+','',line)` 归一化后，按序在源码行集合中做子序列匹配；15 块 0 未命中。剩余 6 个“块内顺序与源码不同”的 rust 块均为文档显式声明的拼装/节选（§3.6 `enter_kernel_image_v1` 组装、§4.4 trait 跨文件拼接、§4.5 wrapper 重排），不属逐字引文。
- N=0 时不覆盖：被工具 `SKIP` 的非引文块（bash 命令、ASCII 调用树、字节图、`boot.cfg` 文本、GDB 会话）不在“引文一致性”射程；4 个 WARN 块的注释是被文档声明为“译述”的，其英文注释未逐字对账。

## 6. 覆盖率缺口

- 扫了什么：§1.4 的范围声明 vs 正文实际覆盖；§2 的 C 调用树（`multiboot_init`/`pre_init`/`get_parameters`/`mb_set_param`/`add_memmap`/`cut_memmap`/`overlaps`/`alloc_lowest`/`pg_alloc_page`/`alloc_pagetable`/`arch_boot_proc`/`setup_mbi`）与 §3–§4 的 Rust 落点。
- 发现数：0（在声明范围内）。文档对每个它声称覆盖的 C 函数都给了实现说明；`pg_clear/pg_identity/pg_mapkernel/pg_load/vm_enable_paging` 只保留调用位置并明确归 02 篇，符合 §1.4 的接缝声明；`pre_init` 之后的 `kmain` 保护结构归 03 篇。§7.2 列出的每个 C 文件在正文至少被引用一次。
- N=0 时不覆盖：`module_init`（boot2 `exec.c` 的模块读取/物理地址登记）只在 §2.1 以“`exec_multiboot` 内部步骤”一句带过，未展开其代码；这是文档声明的“模块装载检查”而非“装载实现”边界。`pg_map` 之外的 `pg_utils.c` 辅助函数（`pg_rounddown`/`print_memmap`）未覆盖，但它们不在文档的声明主题内。

## 7. 反事实前提

- 扫了什么：全文所有“如果反过来想/若…会怎样/把…换成…会怎样”类反事实（约 23 处）。
- 发现数：0。逐条核对的前提：
  - `load_mods` 反序 → 模块表为空但 `HAS_MODS` 仍置（`boot_modules_enabled` 默认开、`module_init` 在 `exec_multiboot` 内）——真。
  - 两条 `push` 对调 → `magic=ebx`、`ebx=0x2BADB002`、首条断言失败——真（head.S 与 pre_init 断言均实存）。
  - 删掉 `mb_set_param` 的“已存在先删” → 同名键双记录、`get_value` 返回首条——真（源码逐行推演与本轮重演一致）。
  - `cut_memmap` 不取整 → 只有“退化且未对齐”请求会漏切——真（源码推演）。
  - `alloc_lowest` 把长度当结束地址 → 三行分档；四前提（嵌套/最低基址非零页对齐/roundup(tramp)≤最低基址/唯一调用点）均实测成立（`rg alloc_lowest` 3 命中；`BIOS/arch_init` 切除；`CONFIG_MAX_CPUS=1`）。
  - `OVERLAP` 去掉 `-1` → 相切误报 panic——真（半开区间语义）。
  - 去掉 `overlaps` → 账本不被记错、只有内容互相覆盖——真（`cut_memmap` 幂等）。
  - ARM 闭区间判定 → 仅当内核长到 30 MiB 才误报——真（0x82000000−0x80200000=0x1E00000）。
  - C 的 6 页页表池搬到 4 级页表 → 第三个 PML4 槽耗尽——真（3 页/槽算术）。
  - 把 `PT_ALLOC` 注册挪到进内核之后 → Step 1 `map_huge` 断言“called before register”——真（asssert 串在源码）。
  - 互换 `tty`/`mib` → v1 三腿静默、直核腿编译失败——真（`bootface_common` 的 `const _` 逐字节对账；v1 strict 门自比）。
  - 删掉 `[raw] entering kernel image v1` → 三档停点读法——真（三处打印点与 `step1+2` 的发送侧归属均实存）。
- N=0 时不覆盖：历史性反事实（`bootstrap_len` 误设事故、wire 7→8/8→9 的字节数、`RESERVED_REGION_STORE_LEN` 的 127 条真机读数）依赖已删除的代码/未归档日志，本轮只能核对文档自洽与现存的地址/符号锚点（`[0,0x200000)`、`0x8020_0000`、`reserved 127 zeroed 127` 的源码注释），不能独立复跑。

## 8. 阴性声明汇总

| 检查 | 扫了什么 | 发现数 | N=0 时不覆盖 |
|---|---|---|---|
| 锚点 | 87 工具锚点 + 15 行号引用 | 0 | 不证明锚点的语义位置正确 |
| 路径 | 49 个 doc-code-map 文件 + 12 脚本 + 4 文档 | 0 | 不证明脚本可运行 |
| C 常量 | 20 项 | 0 | 不证明编译期展开后的值 |
| Rust 常量 | 28 项 | 0 | 不证明跨架构构建下的值 |
| 测试名 | 8 个函数 | 0 | 不证明测试断言足够 |
| 测试计数 | 4 个包实跑 | 0 | 宿主读数，非真机 |
| 引文 | 42 个引文块（20 工具 + 15 手工 + 4 声明改写 + 3 其他） | 0 | 见 §5 的“不覆盖” |
| 反事实 | 23 处 | 0 | 历史事故类无法复跑 |
| 真机门判据串 | 5 个脚本 | 0 | 未实跑 QEMU |
| rootdevname 无内核读者 | `grep -rn rootdevname minix3/minix/kernel/` | 0 命中 | 不覆盖用户态读者 |
| `impl BootShim` 零命中 | `rg 'trait BootShim\|impl BootShim' os/` | 0 命中 | 不覆盖历史提交 |
| `VM_BOOT_HANDOFF_MAX_IDENT`/`ident_regions` | `rg` 全仓 | 0 命中 | 不覆盖文档自身以外的历史 |
| `STARTUP_RECORD_STORE` | `rg` 全仓 | 0 命中 | 同上 |
| BOARD 消费者 | `rg 'BOARD\|board=' os/kernel os/libs/minix-boot` | 0 生产消费者（仅 boot_params 生成/拒绝 + 测试字面量） | 不覆盖 C 侧 `get_value` |
| PG_ALLOCATEME | `rg` i386 | 1 实参点（protect.c）+ 3 消费点（pg_utils.c）+ 1 定义 | 不覆盖其他架构 |
| alloc_lowest 调用点 | `rg` i386 | 1 调用 + 1 定义 + 1 原型 | 不覆盖 SMP 篇未来实现 |
| fdtput 注入表址 | `grep -l fdtput os/qemu-tests/*.sh` | 恰 6 个脚本，与文档名单一致 | 不证明脚本实跑 |
| aarch64 直核构建形态 | `test-smp-aps-aarch64.sh` | `fw-aarch64-none` + `-kernel`，与文档一致 | 未实跑 |
| Rust UEFI 目标 | `rustc --print target-list \| grep uefi` | `aarch64/i686/x86_64`，无 riscv64 | 目标列表随工具链版本变化 |

## SUPPLEMENT trap_api

协调者修正：`related-code.txt` 已补齐 `os/servers/rs/src/trap_api.rs`（doc-code-map 复算 49 条）。本轮对该文件的核对结果：

- 文档 §3.2.2.2 的两处引用 `os/servers/rs/src/trap_api.rs:223-230`、`:430` 均精确命中：223–230 是 `fn user_sp(&self) -> Result<u64, Errno>` 的文档注释 + 函数体；430 是 `exec` 路径里的 `let user_sp = self.user_sp()?;`。
- “`KinfoStruct.user_sp`（offset 8）”的断言成立：`trap_api.rs` 的 `user_sp()` 读 `GET_KINFO` 返回缓冲的 `buf[8..16]` 并按 `i64` 解；`os/libs/minix-types/src/types/kinfo.rs` 的 `#[repr(C)] KinfoStruct` 里 `user_sp: i64` 位于 offset 8（测试 `test_kinfo_layout_frozen` 断言 48 B 与各 offset）。
- 该文件不改变任何 FINDING：它是 RS 的消费点，不参与五条启动腿的发送/接收链；FACT-01 的 store 入口判定仍只涉及 `os/kernel/src/lib.rs`。
- 附带核对：文档称“`nr_procs`/`nr_tasks`/`release`/`version` 由 IS 读”，`os/servers/is/src/dump_kernel.rs:572-575` 与 `acquire.rs` 的 `get_kinfo` 证实；`vir_kern_start`/`freepde_start` 在 Rust userland 无读者（`rg` 仅命中 kernel 侧填充与 minix-types 定义）——与文档一致。

## 未覆盖

- 所有真机门与 QEMU 脚本未实跑；只核对了脚本存在性、判据字符串与 `run_all.sh` 注册关系。
- 历史性声明（wire v7→v8/v8→v9 的 envelope 字节数、`bootstrap_len` 误设的那次事故、真机 `res=127 zeroed=127` 与“15 条 conventional 条目”读数）没有可复跑证据，本轮只能核对文档自洽、现存地址锚点与源码里保留的取证注释。
- C 侧未编译：`__trampoline_end - trampoline` 的 2212 B、`load_stack_start` 的 4096 对齐构建链、`unpaged_*.o` 的 link 顺序均按源码/构建脚本推演，未做实际链接验证。
- 未逐条重跑 `tools/doc-citation-check.py` 的 4 个 WARN 块内部所有英文注释的逐字一致性（文档已声明注释为译述）。
- 未核 §5.1 证据矩阵中“负控覆盖已知变体”的完备性（只核了测试函数存在与计数）。
- 未评估 D-119 的实现质量与性能，只核对了文档对其形状的描述（BootReport/BootInfo、wire v10、六分区）。
