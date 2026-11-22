# D01 P-FACT 独立事实审查报告（round 1，终版）

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2968 行）
- 关联代码清单：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/related-code.txt`（48 个文件；doc-code-map 存在性 48/48）
- 冻结决策参考：D-108 / D-110 / D-111 / D-112 / D-114 / D-115（只按编号与结论引用，未读历史论证）
- 审查方式：只读仓库；逐条回到 `minix3/` C 源码与 `os/` Rust 源码核对；未读取任何历史 scan/review/loop/defect-ledger/expected/old archive（一次 repo-wide grep 意外命中 `rewrite-notes/meta/.../archive/V3.10/` 的历史报告，未采信、未引用其中任何结论）。
- 测试读数为本机实跑（`cargo test`，rustc 1.94.1）。

## 0. 机械门读数（全部本机实跑，2026-11-22）

| 门 | 命令 | 读数 | 结论 |
|---|---|---|---|
| 锚点解析 | `bash tools/anchor-resolve.sh --check <doc>` | `anchors=96 resolved=96 zero-def=0 multi-def=0`（逐条 resolved 行） | PASS |
| 文码映射 | `bash tools/doc-code-map.sh <doc> --check` | 48 行「实现 / 是」，末行「（共 48 个文件；存在性 48/48 通过）」 | PASS |
| 结构 | `python3 tools/doc-structure-check.py <doc>` | `✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析` | PASS |
| 章节引用 | `bash tools/doc-ref-check.sh <doc>` | `✅ doc-ref-check: ... 内部章节引用均可解析` | PASS |
| 文风 | `bash tools/doc-style-lint.sh <doc>` | `命中汇总：| error级=0` | PASS |
| 引文 | `python3 tools/doc-citation-check.py <doc>` | `blocks=54 checked=21 failed=0 skipped=29 warned=4` | PASS（29 个跳过块不在机械门覆盖面，见 FINDING-1） |
| 承载门 | `bash tools/doc-code-density-check.sh <doc>` | 4 个可判章全 OK（第 2 章码 15、第 3 章码 14、第 4 章码 10、第 6 章命令 2）；`✅ 承载门阻断项通过` | PASS |

机械门全绿只证明「锚点可解析、文件存在、结构/文风/承载形态合格」，不证明引文与源码一致。FINDING-1 正是 `citation-check` 跳过的那类块。

---

## FINDING FACT-1（P0-fact）：`pt_alloc` 引文块与注册机制描述已与当前源码脱节

- 位置（文档）：
  - `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1407-1449`（§3.4.2 的 `pt_alloc.rs` 代码引用块）
  - `:1481`（表行「每个二进制各有一份 `PT_REGISTERED` 静态」）
  - `:1528`（「在 `PT_REGISTERED` 为真时仍可能从同一区间取页」）
  - `:1939`（「`alloc_pt_page` 看的是 `PT_REGISTERED`」）
  - `:2536`（§4.6「`PT_CLAIMED.swap(true, AcqRel)` 抢写权 … `PT_REGISTERED.store(true, Release)` 发布；`assert!(!claimed, ...)`」）
- 源码（当前树）：`os/arch/src/arch/pt_alloc.rs:46-95`
  - `static PT_ALLOC: AtomicUsize = AtomicUsize::new(0);`
  - `register()` 用 `PT_ALLOC.compare_exchange(0, alloc_fn as usize, Ordering::AcqRel, Ordering::Acquire).unwrap_or_else(|_| panic!("pt_alloc::register called twice"));`
  - `is_registered()` = `PT_ALLOC.load(Acquire) != 0`
  - `alloc_pt_page()` = `let raw = PT_ALLOC.load(Acquire); assert!(raw != 0, "pt_alloc::alloc_pt_page called before register"); … transmute(raw)`
  - 仓内（排除 `os/target`）`grep -rn "PT_CLAIMED\|PT_REGISTERED\|PtAllocSlot\|uninit_alloc" os/ --include=*.rs` 零命中；`PT_ALLOC` 仅命中上述文件与 `paging.rs` 注释。
- 严重度：P0-fact
- 问题：文档以「代码引用 / source-symbol: register/alloc_pt_page」的口吻给出了一段**当前源码中不存在的实现**：`PtAllocSlot(UnsafeCell::new(uninit_alloc))`、`PT_REGISTERED: AtomicBool`、`PT_CLAIMED: AtomicBool`、`PT_CLAIMED.swap`、`assert!(!claimed, ...)`、`PtAllocSlot.0.get()` 全部不存在。实际实现是单槽 `AtomicUsize` + `compare_exchange(0, …)`，第二次注册由 `unwrap_or_else` 里的 `panic!` 拒绝（不是 `assert!`）。§4.6 又在这段虚构实现之上做了一次反向推演（「去掉 is_registered() 守卫」），推演的前提状态（claim/publish 两标志 + UnsafeCell 裸指针）在当前代码里没有对应物。
- 影响：读者照 `source-symbol` 去 `grep PT_CLAIMED`/`PT_REGISTERED` 会零命中，无法复核；把「两标志发布协议」当成现行内存序契约会得出错误的并发/重入推理（实际只有一个原子的 CAS，发布语义由 `Acquire`/`AcqRel` 单槽承担）。结论「第二次注册在两种构建档都 panic」仍成立，但证据链指向不存在的代码。
- 证据：E1（源码全文 46-95；全仓 grep 零命中；git 历史显示 `c8d14a85a refactor(d01): 实施 DESIGN-04 栈上 ClaimSet 与 DESIGN-09 原子槽` 改了该文件，文档未跟）。
- 最小修复：用当前 `pt_alloc.rs:46-95` 替换 §3.4.2 引文块；把 §4.6 的推演改写成「`register` 先 `compare_exchange(0, addr)`，失败即 `panic!("pt_alloc::register called twice")`；`is_registered()` 读 `PT_ALLOC != 0`；`alloc_pt_page` 的 `assert!(raw != 0, ...)` 是缺失注册的现场」；同步把 1481/1528/1939 的 `PT_REGISTERED` 改为 `is_registered()`/`PT_ALLOC`。
- 置信度：高。
- 若我错了：若仓内还有第二份 `pt_alloc` 实现（例如按 target 分叉）使用 `PT_CLAIMED`，请给出路径；当前 `grep -rn`（排除 `os/target`）零命中。

## FINDING FACT-2（P2）：两个不同小节共用编号 `2.4.2`

- 位置：`doc:608` — `### 2.4.2 从记录里取址：两个消费者、两条取整规则与移交截断`；`doc:725` — `#### 2.4.2 模块内存与 unpaged 段的归属变更（下游延伸；§1.4 已声明的越界交代）`
- 严重度：P2
- 问题：两个内容完全不同的小节共用 `2.4.2`，第二处还是 `####`（比第一处低一级）却仍写父节编号。后文「§2.4.2 的『还引导段』行」指第二处（doc:733、769、1011 附近），「§2.4.2 末」指第一处（doc:723、1136、1170），同号两义。
- 影响：交叉引用歧义；`doc-ref-check` 只查被引编号存在，不查唯一性，故机械门不报。
- 证据：E1（`grep -n '^### 2.4.2\|^#### 2.4.2'` → 608、725）。
- 最小修复：第二处改独立编号（如 `### 2.4.3`），同步所有指向该表的「§2.4.2」。
- 置信度：高。
- 若我错了：若项目允许 `####` 复用父节号表示「同节延续」，需在 §1.4 的编号约定里写明；当前只定义了 `I-/D-/D2-/R02-T` 四类编号。

## FINDING FACT-3（P2）：`3.2.2.4` 标题连续重复两次

- 位置：`doc:1257` 与 `doc:1259`，两行文本完全相同：`##### 3.2.2.4 param_buf 的十条可观察分歧`
- 严重度：P2
- 问题：同一标题紧邻重复（1257 标题、1258 空行、1259 同文本标题），是编辑残留。
- 影响：锚点/目录出现两个同名条目；下游锚点生成器可能取错；违反出版标准「无迭代残留」。
- 证据：E1（`grep -n '3.2.2.4' <doc>` 两行同文本）。
- 最小修复：删 1257 或 1259 其中一行。
- 置信度：高。

## FINDING FACT-4（P2）：`authority` 字段的取值写成 `DtbBootInput`，与源码类型不符

- 位置：`doc:2310`（§4.4 字段表 `authority` 行）：「两个直核面传 `DtbBootInput`」
- 源码：`os/libs/minix-boot/src/memmap_source.rs:591-606` — `DtbBootInput` 是 `enum InputObjectId` 的变体；`RamAuthority` 是 `struct RamAuthority(InputObjectId)`，可取值是常量 `RamAuthority::DTB` / `RamAuthority::UEFI`（`:608-613`）。`build_kernel_info` 的参数类型是 `minix_boot::RamAuthority`（`os/boot-shim/src/lib.rs:113`）。
- 严重度：P2
- 问题：把 `InputObjectId::DtbBootInput` 写成了 `authority`（`RamAuthority`）的取值；同文档 §3.2（doc:1179）用的是正确的 `RamAuthority::DTB`/`RamAuthority::UEFI`，前后不一致。
- 影响：按此行构造调用会得到类型错误（`expected RamAuthority, found InputObjectId`），或误以为 `DtbBootInput` 可独立作为权威。
- 证据：E1（源码 grep；doc:2310 原文）。
- 最小修复：改为「两个直核面传 `RamAuthority::DTB`；v1 接收侧由 wire 的 `ram_authority` 解码写入，UEFI 腿发送侧声明 `RamAuthority::UEFI`」。
- 置信度：高。

## FINDING FACT-5（P2）：宿主测试计数 `minix-boot 187` 与实测 188 不符

- 位置：`doc:2605`（§5.0「读数是 187/0 与 866/0/8」）、`doc:2639`（§5.2「`cargo test -p minix-boot --lib` 187/0」）
- 实测（本机，`cd os && cargo test -p minix-boot --lib`）：`test result: ok. 188 passed; 0 failed; 0 ignored`。
- 同批实测：`boot-shim` 136/0（与文档一致）、`minix-elf` 27/0（一致）、`minix-kernel --features mock` 866/0/8（一致）。
- 严重度：P2
- 问题：四个计数中一个已陈旧（+1）。
- 影响：读者按文档核对会得到「多 1 个测试」的假阳性；计数是 §5.2 证据表的唯一量化锚点。
- 证据：E1（本机实跑输出，见上）。
- 最小修复：把两处 `187` 改为 `188`（或改为不写死数字、只给命令）。
- 置信度：高。

## FINDING FACT-6（P3）：`load_boot_modules_into` 引文块的注释被译述/压缩，但块头声明是「一处截断」

- 位置：`doc:2172-2228`（§4.3 引文块）；块头 `doc:2172`：「略去函数文档注释；`assert!` 的容量前置检查保留原样；行内注释有一处截断」
- 源码：`os/boot-shim/src/loader.rs:594-665`
  - 源注释是 9 行英文（「Trim to the ELF's runtime file length … A check of 36 module ELFs …」），文档换成了 2 行中文「修剪到 ELF 的运行期文件长度…」——不是「截断」而是译述+压缩。
  - 末尾源注释「`name` is already a `&'static str` from the `MODULE_NAMES` constant table — no heap allocation and leak needed.」在文档里译为「`name` 已是 `MODULE_NAMES` 常量表里的 `&'static str`——无需堆分配。」，丢了「and leak needed」。
- 严重度：P3
- 问题：引文保真度声明与块内容不一致；按文档自己的惯例（如 §3.4.2、§4.6 的块头写「中文注释为译述」）应显式标注译述。
- 影响：读者把中文注释当作源码原文引用时会与上游英文注释对不上；语义影响小（装载行为描述正确）。
- 证据：E1（源码 594-665 与文档 2172-2228 对照）。
- 最小修复：块头改为「行内英文注释译为中文、并有一处压缩」，或恢复英文原文。
- 置信度：高。

## FINDING FACT-7（P3）：`ebs_pair.rs:216-219` 行号锚点已漂移

- 位置：`doc:2749`（§6.1 里程碑表「`boot-shim: memory map left N range(s) out of both ordinary tables`」行，位置列 `ebs_pair.rs:216-219`）
- 源码：`os/boot-shim/src/ebs_pair.rs:219`（`raw_serial("boot-shim: memory map left ")`）与 `:221`（`" range(s) out of both ordinary tables\r\n"`）；`:216-218` 是注释。
- 严重度：P3
- 问题：诊断表给的行号区间不覆盖实际打印语句。
- 影响：按行号跳到 216 只会看到注释；排查时多一次搜索。
- 证据：E1（`grep -n "memory map left\|range(s) out of both ordinary"`）。
- 最小修复：改为 `ebs_pair.rs:219-221`。
- 置信度：高。

## FINDING FACT-8（P3）：`KinfoStruct` 的「五个有 Rust userland 读者」对 `user_sp` 不成立

- 位置：`doc:1242`（§3.2.2.2）：「只含七个语义字段 … 其中**五个有 Rust userland 读者**：`user_sp`、`nr_procs`、`nr_tasks`、`release`、`version`」
- 源码/使用者：
  - `KinfoStruct` 仅在 `os/kernel/src/misc.rs` 构造；用户态唯一消费者是 `os/servers/is/src/acquire.rs` / `dump_kernel.rs`，读 `nr_procs`、`nr_tasks`、`release`、`version` 四个字段（`dump_kernel.rs:572-575`）。
  - `user_sp` 在 VM 侧的读者来自 `VmBootHandoff.user_sp`/`USER_STACK_TOP`（`os/servers/vm/src/boot.rs:129,175,372`），不是 `KinfoStruct.user_sp`；仓内 `grep -rn "KinfoStruct"` 未发现第五个字段的读者。
- 严重度：P3
- 问题：把「语义量在用户态有读者」写成了「这五个 `KinfoStruct` 字段有读者」；`user_sp` 这一项严格读法为假。
- 影响：读者会以为 GET_KINFO 的 `user_sp` 仍被消费；实际它只是布局占位、值由 `USER_STACK_TOP` 派生（这一点 §4.4 已写对，两处口径不一致）。
- 证据：E1（`KinfoStruct` 引用清单；`dump_kernel.rs` 四个字段；VM 的 user_sp 来自另一个结构）。
- 最小修复：改为「四个 `KinfoStruct` 字段有 userland 读者；`user_sp` 的语义消费者走 `VmBootHandoff.user_sp`」。
- 置信度：中高（若把「语义量」而非「结构字段」作为主语，则原文可辩护；建议改词消歧）。

---

## 已核对通过的高风险事实（抽样记录，不计 FINDING）

C 侧（逐字/行号）：

- §2.1：`head.S:43-66` 识别头逐字段（`.balign 8`、`MULTIBOOT_FLAGS` 两位、checksum、五个 `.long 0`、video 字段）与 `multiboot.h:40-44,86-93,243-259` 常量全部一致；`exec.c:758-760`（HAS_MODS/count/addr）、`:584,595,617`（image_end 页对齐/base/+=len）、`:753`（mmo_end）一致；`boot2.c:146-153,453-458,588` 命令表一致。
- §2.2：`head.S:68-96` 入口指令、`load_stack`/`load_stack_start` 在 `.data`、`MINIX:`+`jmp multiboot_init` 位置一致；`Makefile.inc:51,83`、`kernel.lds:18` 行号一致；`_C_LABEL` 在 `asm.h:65-70`。
- §2.3：`get_parameters`（`pre_init.c:94-215`）三处省略位置、`mb_set_param`（`:35-85`）逐字、`cttyline`/`cttybaud`（`const.h:137-140`）、三处调用忽略返回值、`grep -rn rootdevname minix3/minix/kernel/` 零命中、`pm/main.c:167-170` 缓存起点、`sysenv.c:59` PMGETPARAM、`mib/kern.c:96-108` 全部成立。
- §2.4：`add_memmap`/`cut_memmap`/`alloc_lowest`/`pg_alloc_page` 引文逐字；`MAXMEMMAP=40`；`LIMIT=0xFFFFF000`；`alloc_lowest` 唯一调用点 `arch_smp.c:76`；`arch_init` 两条 cut（`arch_system.c:277-278`）与 `memory.h:29-33,118-119` 一致；`alloc_lowest(cbi, lowest, len)` 的参数错误推演成立。
- §2.5：`overlaps` 引文逐字；内核当第 M+1 模块的登记段（`pre_init.c` 后段）逐字；`MULTIBOOT_MAX_MODS=20`，`assert(k < MULTIBOOT_MAX_MODS)`。
- §2.7：ARM `head.S`、`setup_mbi`、`overlaps` 引文逐字；`MB_MODS_BASE=0x82000000`、`MB_MODS_ALIGN=0x00800000`、`earm/kernel.lds:4 _kern_phys_base=0x80200000`（30 MiB 差）成立；ARM `vm_enable_paging` 的 TTBCR/DACR/SCTLR 与 `dcache_clean` 调用点成立。
- §3.4：`kernel.lds:4-6,14-19,28-34` 引文与 `_kern_size` 导出、`__k_unpaged_*` 双前缀机制、`objcopy --prefix-symbols` 一致；`main.c:301` 的 `add_memmap(bootstrap_start,len)` 在 `system_init` 后、`bsp_finish_booting` 的 `kernel_may_alloc=0` 前。
- §2.4.2（第二处）：`protect.c:450-454`（add_memmap + 双字段置零 + vm_allocated_bytes）、`vm/main.c:486-520`（mem_add_total_pages / VM continue / free_mem）一致；`NR_MEMS=16`、`vm/utility.c:58-59` 注释一致。
- §2.4 / §2.8：`kernel_may_alloc` 两个定义（`glo.h:9-12,76`、`kernel.h:42`、`table.c:30`、`pre_init.c:33`、`main.c:105,142`）一致。
- §4.3：`table.c:44-64` 用户态段顺序 `ds,rs,pm,sched,vfs,memory,tty,mib,vm,pfs,mfs,init` 与 `MODULE_NAMES` 一致；`DS_PROC_NR=6`；`sed … | grep -o '"[a-z]*"' | wc -l` = 12；`load_boot_modules_into` 的 `(len+4095)/4096`、页尾清零、`BootModule.len` 语义成立。
- §6.3 C 两行：`arch_reset.c:119-130` 的串口互斥/刷屏等按键行为与文档一致。

Rust 侧（逐字/常量/测试名）：

- `KernelInfo` 恰好 10 个字段，与 §3.2.1 一致；`BootHandoffV1` 字段/类型与 doc:1279-1301 逐字一致；`BOOT_HANDOFF_V1_VERSION=10`、`ENVELOPE_CAP=256`、`WIRE_END=7872`、`WIRE_SOURCES_END=29184`、`ARENA_CAP=32768`、余量 3584、信封 128 B、startup 1440 B、v8/v9 数字全部自洽且与源码断言一致。
- 落地区容量：`RESERVED 256`、`MEMMAP 128`、`BOOT_MODULE 12`、`PLATFORM 4`、`PARAM 1024`、`SOURCE_RECORD 296`、`BOOT_MODULE_NAME 16`；`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`MAX_DEDUCTED=47`（3+32+12，源码编译期断言）；`WIRE_SOURCE_RECORD_SIZE=72`（296×72=21312）。
- 引文逐字：`arch_boot_finish`、`enter_kernel_image_v1`/`prepare_kernel_image_v1`/`jump_kernel_image_v1`/`PreparedJump`、`BootPrepareResult`、`FileLoader`、`UefiFileLoader`、`BootAlloc::alloc`、`BootAlloc::init/resume`、`store_kernel_info`/`land_slice`、`build_kernel_info`、`prepare_and_enter_kernel_image`、`raw_serial`、panic handler（含 256 B StackWriter）——与源码一致，省略处均有声明。
- 门链：`validate_before_tables`（3 项）、`check_boot_postconditions`（v10 后仅 `kern_virt_base > u32::MAX`）、`ValidationLevel::{BeforeTables,Standard,Strict}`、`validate_requiring_sources` 无参、`resume_additive` 内 `boot_validate_and_prepare` 先于 `store_kernel_info`、`arch_boot_resume_v1` 的 strict→postcond→resume_boot_pt_alloc 次序——全部与源码一致。
- `ebs_map` 五步 + 两内联闸：`LAST_KNOWN=UNACCEPTED=15`、`KNOWN` 16 常量 31 个位位置（python 复算 31）、`AMBIGUOUS`/`PROFILE_ONLY`/`TYPE_INDEPENDENT`、`checked_shape` 六种错误、`checked_len`→`OutOfPhysRange`、`OverlapsDescriptor`/四类容量拒绝——逐条一致。
- 里程碑串：§6.1 表中的 20 个字符串全部在源码中命中；`kernel-image: resuming high-half boot` 的三种后缀与 `main.rs:370,409,449` 一致；`step4` 两行仅 aarch64 壳打印（x86/riscv 传空串）。
- 测试名：`generates_arch_and_board_first`、`rejects_internal_key_override`、`drops_tail_records_whole_at_capacity`、`splits_nul_and_whitespace_records`、`rejects_malformed_and_non_ascii`、`boot_windows_reject_reversed_ranges`、`boot_windows_reject_empty_ranges`、`decoded_map_survives_arena_poison_and_reissues_the_arena_frames` 全部存在。
- 依赖方向：四份 `Cargo.toml` 与 §3.5.2 四行一致；`minix-kernel` 无 `boot-shim`/`uefi`；`boot-shim` 的 `uefi`/`uefi-raw` 为 optional 且 `uefi = ["dep:uefi","dep:uefi-raw"]`；`kernel-image` 的 riscv64/aarch64 target 表反向依赖 `boot-shim`（仅 `opensbi`）。
- `lib.rs` 的 7 处 `cfg(feature)`（6 uefi + 1 opensbi）与 `uefi_helpers.rs` 10 处 `qemu-test-kernels` 门控与 §3.5.1 一致；`BOOT_PT_BUMP_PAGES=1024`、`PT_BUMP_PAGES=256`、`NAME_SLOT=16`、`NAME_POOL` 12×16 与编译期逐字节对账成立。
- `rustc --print target-list | grep -i 'riscv.*uefi'` 零命中（rustc 1.94.1），支持 §3.1.1「riscv64 UEFI shim 未接线」的工具链理由。

## SUGGESTION

1. 建议把 §5.2 的四个测试计数改为「命令 + 当日读数（日期）」格式，或干脆只给命令；写死数字每次加测试都会陈旧（FINDING-5 即一例）。
2. §2.1 关于 `menu=` 的表述（「boot2.c 的命令表里逐条写着这三个名字」）严格说 `menu` 命令与 `menu=` 配置行由不同层解析；可改为「boot2 与其 bootcfg/bootmenu 层」。不影响结论。
3. §3.2.2.2 的 `user_sp` 口径建议与 §4.4 统一（见 FINDING-8）。
4. `doc-citation-check` 跳过的 29 个块（含 FINDING-1 命中的 `pt_alloc` 块）是当前事实审查的主要盲区；建议对该类块引入「source-symbol 存在性 + 关键标识符存在性」的机械检查（`grep` 每个反引号标识符至少一次命中）。

## VERDICT

**NOT CONVERGED（P-FACT 轨）**。文档的 C 侧事实密度与准确度很高：本次抽样的 `head.S`、`pre_init.c`、`pg_utils.c`、`exec.c`、`boot2.c`、`kernel.lds`、`Makefile.inc`、`arch_smp.c`、`arch_system.c`、`protect.c`、`vm/main.c`、`table.c` 引文与行号全部经源码核对通过。Rust 侧绝大多数引文与常量也逐字一致。阻断项只有一条但属 P0：§3.4.2 的 `pt_alloc` 引文块描述了一套当前源码中不存在的 `PT_CLAIMED`/`PT_REGISTERED`/`PtAllocSlot` 实现，且 §4.6 在其上继续推演；必须按当前 `AtomicUsize` + `compare_exchange` 实现重写该块与四处引用。其余 7 条为 P2/P3（编号重复、标题重复、`DtbBootInput` 类型错位、测试计数陈旧、引文译述未标注、行号漂移、`KinfoStruct.user_sp` 读者口径）。

- 发现总数：8（P0=1，P2=4，P3=3）
- 最高严重度：P0-fact（FINDING-1）
- 复核命令与输出：见各 FINDING 的「证据」行；机械门读数见 §0。
- 本报告为同 agent 只读审查；所有源码坐标均给出绝对路径或仓库相对路径 + 行号，可独立重放。
