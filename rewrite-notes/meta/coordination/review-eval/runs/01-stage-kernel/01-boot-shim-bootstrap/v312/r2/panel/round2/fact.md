DOC 01-boot-shim-bootstrap.md

# D01 V3.12 R2 独立事实审查（fact 角色）

- 目标文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 文档 sha256：`19c0ad9a306cc1e950b657ad6c6d1c34f3a0aa18256f238b048e7dbd9efc83dc`（与任务书一致），2951 行
- 审查范围：机械门读数 + 工具不能判定的语义核对（C 引文逐行对源、Rust 签名/常量/字段/测试名与计数、阴性声明、反事实前提）
- 结论：未发现 P0/P1 事实错误；发现 1 个根因下的 P3 引文纪律问题（代码行在“代码引用”块里被合并/改写，块级豁免只声明了注释为译述）。另有若干历史性/观测性断言与外部事实列入 `## 未覆盖`。

## 机械门读数（先跑，直接引用）

| 门 | 命令 | 读数 |
|---|---|---|
| 符号锚点 | `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `anchors=86 resolved=86 zero-def=0 multi-def=0`，exit 0 |
| 关联代码存在性 | `tools/doc-code-map.sh … --check` | 共 49 个文件；存在性 49/49 通过，exit 0 |
| 表结构与内部链接 | `python3 tools/doc-structure-check.py …` | `1 个文件表结构与内部 .md 链接均可解析`，exit 0 |
| 内部章节引用 | `AWK=gawk tools/doc-ref-check.sh …` | `内部章节引用均可解析`，exit 0 |
| 文风 | `tools/doc-style-lint.sh …` | `error级=0`，exit 0 |
| 引文机械对账 | `python3 tools/doc-citation-check.py …` | `blocks=54 checked=20 failed=0 skipped=30 warned=4`，exit 0 |

工具局限（本轮实测）：`doc-citation-check.py` 对声明“译述/改写”的块整块豁免逐行检查，故下文的引文问题不被它拦下；`checked=20` 只覆盖 54 块中的 20 块。

## FINDING FACT-01（P3）

- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3.4.2（文档 1445–1466 行，`BootAlloc::alloc` 引文块）；源码 `os/kernel/src/boot_alloc.rs:140–170`
- 严重度：P3
- 问题：该块标注为“代码引用”，但有三处源码行被代码级合并/改写，块级括号只声明“块内中文注释为译述；英文原注释见源码”，未声明代码行本身不是逐字。
- 影响：读者若把该块当逐字引文，会以为源码里 `write_bytes` 与 `unsafe` 同行、英文注释只有被译述那一段；对“引文块是否可直接对照源码”的证据强度判断偏高。不影响任何机制结论。
- 证据或推理（E1，命令+源码）：
  - 文档块内只有一行 `unsafe { core::ptr::write_bytes(pa as *mut u64, 0, 512) };`；源码为三行：
    `unsafe {` / `core::ptr::write_bytes(pa as *mut u64, 0, 512)` / `};`（`rg -n 'write_bytes\(pa as \*mut u64' os/kernel/src/boot_alloc.rs` → 161–165 行区间）。
  - 文档用一句中文注释替换了源码四行英文注释（“Rebuild the shared bump-region value … former fetch_add/rollback pair.”），块级说明只说“中文注释为译述”，未标出英文原文被删的行号范围。
  - 同根因第二例：§4.2（文档 1976 行）`storage.fill_param(|buf| collect_param_buf_into(buf).bytes);` 在源码中不存在；实际是 `let mut param_stats = None; storage.fill_param(|buf| { let stats = collect_param_buf_into(buf); param_stats = Some(stats); stats.bytes });`（`rg -n 'fill_param' os/boot-shim/src/uefi_helpers.rs` → 1103–1107 行）。块级标注为“注释与排版有改写/删节”，未说明代码表达式被重写。
- 最小修复：把这两处块的括号声明改为明确列出代码级改写（例如“本块代码行为按语义重排，非逐字引文；逐字原文见源码行号”），或把行恢复成源码原文并加内联省略号。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：块级说明中的“改写/删节”“注释为译述”可能被作者有意解释为覆盖代码行；若评审口径接受这种块级豁免，本条应降为 SUGGESTION。

## SUGGESTION

- §4.6 `BootAlloc::resume` 的文档注释引文（文档 2484–2490 行）跳过了源码中的一整段（“The boot-shim builds the bootstrap page tables … post-build cursor.”，`os/kernel/src/boot_alloc.rs:86–97`），块前括号写着“后续 `resume` 的文档注释与省略段见源文件”。建议在引文内加一行 `/// …` 省略标记，使“连续引用”的外观与实际一致。属引文可读性，不构成事实错误。
- §2.2 对 `pre_init` 入口栈对齐的推演（`%esp ≡ 0` vs ABI 的 `≡ 12`）与本轮复算一致（`load_stack_start` 4096 对齐、净压 12 字节 + 返回地址 4 字节 = 16 字节，达到 `≡12` 需 20 字节）。文档已自标“本项目没有保存反汇编证据，只能作为风险提示”，证据等级声明准确，无需改。

## 已逐项核实的断言（阳性证据，摘要）

C 侧（全部 `sed`/`rg` 对源逐行核对，E1）：
- `head.S` 识别头块、`MINIX: jmp multiboot_init`、`multiboot_init` 五条指令、`load_stack: .space 4096`/`load_stack_start:`（93–96 行）逐字一致；`MULTIBOOT_HEADER_MAGIC=0x1BADB002`、`INFO_MAGIC=0x2BADB002`、`HAS_VBE=0x4`、`MULTIBOOT_MAX_MODS=20`、`PARAM_BUF_SIZE=1024`、`MULTIBOOT_MEMORY_AVAILABLE=1`、`HAS_MMAP=0x40`、`HAS_MEMORY=0x1`、`HAS_MODS=0x8` 全部一致（`minix3/sys/arch/i386/include/multiboot.h`）。
- `get_parameters`、`mb_set_param`、`overlaps`（i386/earm）、`add_memmap`、`cut_memmap`、`alloc_lowest`、`pg_alloc_page`、`copy_trampoline`、`arch_boot_proc` 的 VM blob 回收段、ARM `setup_mbi`、ARM `get_parameters` 的“内核当第 M+1 模块”段：引文与源码一致；删除扫描 off-by-one 的字节级推演（`A=1\0B=2\0\0`）复算成立。
- `exec.c` 行锚点：`snprintf` 736、`image_end` 页对齐 584、`bi->base` 595、`image_end += len` 617、`mi_mods_count/mi_mods_addr` 759–760，全部命中；`boot_modules_enabled` 默认 true；`load_mods` 反序时 `HAS_MODS` 仍置位、停在 `mi_mods_count > 0` 的推演与断言次序一致。
- `arch_system.c` 的 `arch_init` 末尾两条 `cut_memmap`（`BIOS_MEM_BEGIN/END=0/0x4FF`、`BASE_MEM_TOP/UPPER_MEM_END=0x90000/0xFFFFF`）；`kmain` 先 `cstart` 后 `smp_init`、`system_init` 后 `add_memmap(bootstrap_*)`；`kernel_may_alloc` 双份定义（`glo.h#L76`、`table.c` `_TABLE`、`pre_init.c#L33`）与 `bsp_finish_booting` 写 0 的位置全部一致。
- 跳板尺寸：按 `trampoline.S` 逐条指令与 `GDT_SIZE=7`、`IDT_SIZE=256` 复算得 `0x8a4 = 2212 B`，与文档“约 2212 B”一致；`TSS_INDEX=6+CONFIG_MAX_CPUS`、`CONFIG_MAX_CPUS=1` 成立。
- `_kern_phys_base=0x00400000`、`_kern_vir_base=0xF0400000`、`__kern_unpaged_start/end`、`__k_unpaged__kern_size = . - _kern_vir_base`；`Makefile.inc` 51/83 行与 `kernel.lds` `.unpaged_data` 18 行锚点全部命中；`earm/kernel.lds#L4 = 0x80200000`、`MB_MODS_BASE=0x82000000`、`MB_MODS_ALIGN=0x800000`，30 MiB 差值复算成立。
- 阴性声明复跑：`grep -rn rootdevname minix3/minix/kernel/` 0 命中；`rg alloc_lowest minix3/minix/kernel/arch/i386/` 恰为定义/原型/唯一调用 3 条；`rg PG_ALLOCATEME` 仅 `protect.c` 一处作实参、`pg_utils.c` 三处消费点；VM `MIN(MAXMEMMAP, NR_MEMS)=16` 截断与 `first fit` 注释属实。
- `rootdevname` 落链：`boot.cfg.default` 第一条 menu 行逐字一致；`update_bootcfg.sh:96` 的 `sed 's/\/dev\///'` 与 99 行注释属实；`sysenv` 走 `svrctl(PMGETPARAM)`；PM `main.c:169` 启动期 `sys_getmonparams`；`do_getinfo.c:143-146` 的 `GET_MONPARAMS` 拷出点；`mib/kern.c` 的 `root_device` 节点均属实。

Rust 侧（E1）：
- `KernelInfo` 恰 10 字段、顺序与引文一致；`BootHandoffV1` 18 字段逐字一致；`BOOT_HANDOFF_V1_VERSION=10`；信封 128 B、`ARENA_CAP=32768`、`WIRE_END=7872`、`WIRE_SOURCES_END=29184`、余量 3584 全部复算一致。
- 容量表逐项命中：`RESERVED_REGION_STORE_LEN=256`、`MEMMAP_REGION_STORE_LEN=128`、`PLATFORM_SOURCE_STORE_LEN=4`、`PARAM_BUF_STORE_LEN=1024`、`SOURCE_RECORD_STORE_LEN=WIRE_SOURCES_CAP=296`（296×72=21312）、`BOOT_MODULE_NAME_LEN=16`、`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`VM_BOOT_HANDOFF_MAX_DEDUCTED=47=3+32+12`（含编译期断言）；`BOOT_PT_BUMP_PAGES=1024`、`PT_BUMP_PAGES=256`。
- `MODULE_NAMES` 12 项与 C `table.c` 用户态段顺序逐项一致；`DS_PROC_NR=6` 与 `BOOT_MODULE_PROC_NRS[0]` 一致；`KERNEL_PATH`/`DUMPDTB_PATH`/`MODULES_DIR` 逐字一致。
- 函数签名与行为：`arch_boot_finish`、`arch_boot_impl`、`enter_kernel_image_v1`、`prepare_kernel_image_v1`、`jump_kernel_image_v1`、`PreparedJump`、`resume_additive`（含 aarch64 两行路标、x86/riscv 空串）、`arch_boot_resume_high_half`、`boot_validate_and_prepare`（签名、四条断言、`require_boot_region`、`is_registered` 守卫）、`land_and_store`/`store_kernel_info`/`store_boot_report`（一次性守卫）、`BootAlloc::init/resume/alloc`、`pt_alloc::register/alloc_pt_page`、`FileLoader` trait 与两个实现、`prepare_boot`/`BootPrepareResult`、`entry_path_eq`、`place_modules` 的六道检查与 `NAME_POOL` 编译期对账、`adjudicate` 五步+两道内联闸、`classify` 的整图拒绝与四类容量、`Serializer::new` 整段零化、`raw_serial`/`raw_serial_line`/`emit_byte`、`prepare_and_enter_kernel_image` 的九动作次序与全部串口文案、panic handler 引文，全部与源码一致。
- 校验门链：`arch_boot_resume_v1` 的 `validate_requiring_sources → check_boot_postconditions → resume_boot_pt_alloc → resume_after_v1_decode` 与 bootface 的 `validate_requiring_sources(DTB) → arch_boot` 次序属实；`check_boot_postconditions` 只剩 `kern_virt_base > u32::MAX`；strict 门的内核跨度覆盖由 `memmap`/`reserved_regions` 回答；`validate_before_tables` 不查 `boot_modules` 属实。
- 三架构 `inherit_supervisor_half` 均存在；`new_from_page(root_page, channel)` 两参签名属实；`LMEMAP` 无关项：`memmap.rs` 已不存在、`VM_BOOT_HANDOFF_MAX_IDENT`/`ident_regions`/`kern_ident` 无残留。
- 测试名对账：文档点名的 8 个测试全部存在（`generates_arch_and_board_first`、`rejects_internal_key_override`、`drops_tail_records_whole_at_capacity`、`splits_nul_and_whitespace_records`、`rejects_malformed_and_non_ascii`、`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`、`boot_windows_reject_reversed_ranges`、`boot_windows_reject_empty_ranges`）。
- 测试计数实测（本轮真跑）：`cargo test -p minix-boot --lib` = 188 passed / 0 failed；`-p boot-shim --lib` = 136/0；`-p minix-kernel --features mock --lib` = 866/0/8；`-p minix-elf --lib` = 27/0。与 §5.0/§5.2 的四个读数全部一致。
- `rustc --print target-list | grep uefi` 只有 x86_64/i686/aarch64，支持 §3.1.1 的“riscv64 UEFI 未接线是工具链事实”。
- QEMU 脚本 grep 级核对：x86 五个 MARK 与文档判据一致；aarch64 两固定标记 + `memmap source`/`DTB preflight failed` 两变体属实；`test-riscv64-uboot.sh` 的 PASS 串与 SKIP=2 属实；`test-riscv64-uboot-shim.sh` 六路标串存在；bootface 脚本的 `table holds 0x… entries`/`module placement failed`/`tail -20` 属实；六个脚本用 `fdtput` 与文档名单一致；`check-layout.sh` 的 L9/L10 存在且语义相符。
- Cargo 依赖表：`os/kernel` 无 `boot-shim`/`uefi`；`boot-shim` 与 `minix-boot` 的依赖清单与文档一致；`kernel-image` 的 bin 名为 `kernel`，支持 §6.4 的 GDB `kernel::rust_image_main`。

## 未覆盖

以下条目未能在时间盒内核实，按“未覆盖”列出，不计为通过：
1. 历史性/设计史数字：wire 160→128、`WIRE_STARTUP_END=30624`、20×72=1440 B、余量 2144→3584、184→160 的旧读数。当前源码只保留终态常量（128/29184/3584），历史值只做了算术自洽检查（29184+1440=30624、32768−30624=2144），无历史构建可复算。
2. §3.2.2.2 “C 侧至少四个活读取点”的字段计数（`dmp_kernel.c` 四字段、`procfs/root.c`、`pci.c complete_bars`、VM 侧 ≥11 字段）：只核对了 `do_getinfo.c` 与 `vm/utility.c`，未逐文件计数字段。
3. §3.2.4 两组真机零化读数（`res=127 zeroed=127`、15 条 conventional 全零）：文档自述“无归档日志”，无法复核，只能确认文档已按观测/预防性证据降级标注。
4. §3.4.1 的 `bootstrap_len = kern_phys_base.0` 险情与源码注释引文：注释已随字段删除，当前树中不存在，无法复核原文。
5. §4.2.1 动作 4 的 `alloc_arena()` 经 `allocate_pages(MaxAddress(bootstrap_alloc_limit()))`：只核对了 `bootstrap_alloc_limit`/`alloc_root_page` 引文，未逐行读 `alloc_arena`/`alloc_bump_region`。
6. §3.4.3 “`memory_init` 把它自增两次填进 `freepdes[]`” 与 §5.5 “kmain_verify 的 PASS 条件”“三份 jump_to_kmain 静态断言”：`kmain_verify` 符号存在，未核对其 PASS 判据细节与三份静态断言的具体内容。
7. §4.4 “`kernel_boot_stack_top` 是 64 KiB 引导栈数组自身的顶”：未在 `os/kernel-image` 与 `os/kernel/src/arch/*/higher_half.rs` 定位到该常量定义（可能定义在未读文件），未核实“64 KiB”。
8. 所有 QEMU/真机门只做了脚本文本与 grep 判据核对，未实际运行；因此“某条链能走到终点”本轮无实测证据。
9. §3.2.2.1 `release/version` 的 `OS_RELEASE`/`OS_VERSION` 常量与 config.h 对位未逐字核对。
10. 平台/工具外部事实：Multiboot 规范“前 8KB”与 `AllocateType::MaxAddress` 语义按公开规范理解，未在仓内取证。
