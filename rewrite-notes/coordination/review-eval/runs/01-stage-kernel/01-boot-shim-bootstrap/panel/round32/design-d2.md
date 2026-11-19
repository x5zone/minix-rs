# P-DESIGN-D2 · 独立设计审查报告（round32）

- 审查对象：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 第 3、4 章
- 关联源码：§7.3 清单（`os/boot-shim/src/*`、`os/libs/minix-boot/src/*`、`os/kernel/src/{lib,boot_alloc,globals,vm_handoff,misc}.rs`、`os/arch/src/arch/{pt_alloc,paging,paging_ext}.rs`、`os/kernel-image/src/{main,bootface,bootface_a64,bootface_common}.rs`）+ `AGENTS.md` Key Constraints
- 审查视角：设计不对称 / 非法状态 / 所有权与接口顺序 / 抽象边界 / 最小化机会 / 文档与代码设计陈述不符
- 约束：只读；未读作者解释与历史评审轮次；本轮未修改除本报告外的任何文件
- 日期：2026-11-11

## 汇总

| 严重度 | 数量 | 编号 |
|---|---|---|
| P1 | 0 | — |
| P2 | 2 | D2-01, D2-02 |
| P3 | 5 | D2-03, D2-04, D2-05, D2-06, D2-07 |

另附 SUGGESTION 三条（纯偏好，不计入发现）。

---

## FINDING RD32-DESIGN-D2-01
`build_kernel_info` 的 `memmap`/`reserved_regions` 是相邻同型参数，互换可静默通过全部校验门

- **位置**：文档 §4.4（构造入口表 + 「内核侧的校验按防线层次组织」链）；`os/boot-shim/src/lib.rs:101-132`（`build_kernel_info` 签名与构造体）；调用点 `os/kernel-image/src/bootface.rs:427-439`、`os/kernel-image/src/bootface_a64.rs:398-410`；校验体 `os/libs/minix-boot/src/kernel_info.rs:268-533`
- **现象**：`build_kernel_info` 的第 1、2 个参数都是 `&'static [minix_boot::MemoryRegion]`，第 9、10 个参数都是 `&'static [SourceRecord]`。第 1、2 参数在两个调用点分别传入 DTB 收集器的 free/reserved 两个输出。若调用点把两者写反：`validate_impl_inner` 的全部判据都不拦截——每张表内部的「升序 + 两两不相交」在互换后原样保持（kernel_info.rs:294-315，谓词只看相邻对）；「memmap 与 reserved 不相交」是**对称**谓词（kernel_info.rs:321-346，swap 不改变相交关系）；strict 门的 `audit_source_coverage_for` 审计的是两表并集被记录覆盖（kernel_info.rs:492-502，swap 不改变并集）；`check_boot_postconditions` 不涉及两表。第 9、10 参数互换则相反，会被 strict 门拦下（startup 记录盖不住投影内存图），说明同型相邻参数的防护是**不均匀的**。
- **影响**：`memmap`/`reserved_regions` 互换后内核照常启动，但语义完全倒置——物理帧分配器从固件保留区（UEFI 腿的 `ReservedMapped` 档、DTB 腿的 OpenSBI/固件前缀）里发页，而真正的 RAM 被当成「可用不可分配」只作 Direct Map 候选。这是信任边界上的非法状态，唯一的拦截点是不对称的运行期谓词，类型系统没有参与。本项目对同类「相邻裸参数互换后仍编译且看似合理」的缺陷已经裁决过一次：`bootface_common.rs:153-167` 的 `PlacementPool`（注释明言三个相邻裸 `u64` 互换后仍能编译并产生看似合理的放置，文档 §3.2 转述为 DESIGN-D2-14 类）。`build_kernel_info` 是同一缺陷类在共享构造面上的残留。
- **最小改法**：给两张表各引入一个裸包装 newtype（如 `FreeRegions<'a>` / `ReservedRegions<'a>`，或直接收一个两字段小结构），`build_kernel_info` 签名改收它们；`KernelInfo` 字段类型不变，两个调用点各改一行包装。第 9、10 参数可同样处理或维持现状（互换已被 strict 门拦下）。
- **严重度**：P2
- **反证路径**：逐条走 `validate_impl_inner` 证明 swap 后每条断言仍真（关键是确认 cross-table disjoint 与 source audit 的并集语义）；或直接在 `bootface.rs:428-429` 互换实参后跑宿主测试（`cargo test -p boot-shim`、`cargo test -p minix-kernel --features mock`）观察无一处失败；再 grep 全仓确认没有任何按「位置/来源」区分两表的检查点（`FreeRangeSet::from_memmap` 只在运行期消费，不校验）。

## FINDING RD32-DESIGN-D2-02
`BootStorage` 的「半提交不可流出」守卫只覆盖两张内存表；文档 §3.4.5 声称覆盖全部 `*_slots_mut`，且把仅测试构建存在的 `set_startup_len` 列为生产 one-shot 提交口

- **位置**：文档 §3.4.5（「**半提交状态由类型/断言挡住**：`map_tables_mut`/`*_slots_mut` 会置 `map_dirty`……`set_modules_len`/`set_platform_len`/`set_param_len`/`set_memmap_sources_len`/`set_startup_len` 各自只允许提交一次」段）；`os/libs/minix-boot/src/boot_storage.rs:143-156、173-176`（置 `map_dirty` 的只有 `memmap_slots_mut`/`reserved_slots_mut`/`map_tables_mut`）对照 `194-196`（`module_slots_mut`）、`224-226`（`platform_slots_mut`）、`255-257`（`param_buf_mut`）、`272-274`（`memmap_source_slots_mut`）、`288-290`（`startup_slots_mut`，均不置任何 dirty 位）；`356-365`（`report()` 只断言 `!map_dirty`）；`292-302`（`set_startup_len` 带 `#[cfg(test)]`）
- **现象**：两条主张都与代码不符。其一，五个非内存表槽族的 getter（modules/platform/param/sources/startup）不置任何脏位，`report()` 也不检查它们的 `*_committed` 标志——「槽已写、长度未提交」在这五个族上是**可表达**的：写完 `module_slots_mut()` 而忘调 `set_modules_len`，`report()` 照常返回空模块表的视图且不 panic。其二，`set_startup_len` 在生产构建里根本不存在（`#[cfg(test)]`），生产侧 startup 只有 `push_startup` 一条追加口，`startup_committed` 从未被置真，文档列举的「五个 setter 各自只允许提交一次」中有一个是测试专用函数。
- **影响**：文档把一条设计不变量（半提交视图不能流到消费者）说成了已由断言挡住，实际只挡了两张内存表。当前调用点恰好次序正确，所以是**文档-代码设计陈述不符 + 守卫缺口**的组合：未来任何新增写点只要漏掉一次 `set_*_len`，缺角的报告会静默序列化进 wire（模块空表还能被接收侧 strict 门的「非空」要求拦下，platform/param 为空则连这道门都没有，只能靠下游平台发现失败来暴露，现场离根因更远）。
- **最小改法**：二选一并同步文档：①在 `report()` 里把 `modules_committed`/`platform_committed`/`param_committed`/`sources_committed`/`startup_committed`（或等价的 dirty 位）一并断言，使文档句子成真；②把文档句子收窄到「两张内存表由 `map_dirty` 守卫，其余五族按 push/commit 协议约定」，并从 setter 清单里去掉 `set_startup_len` 或标注其为测试口。
- **严重度**：P2
- **反证路径**：读 `boot_storage.rs:143-365` 逐 getter 核对置位行为；写一个宿主测试：`storage.module_slots_mut()[0] = ...` 之后直接 `storage.report(...)`，观察不 panic 且 `boot_modules` 为空；`grep -n "set_startup_len" os/libs/minix-boot/src/boot_storage.rs` 确认 `#[cfg(test)]`。

## FINDING RD32-DESIGN-D2-03
`uboot_main.rs` 声称 `uboot_layout::MODULE_NAMES` 与规范表「经 opensbi_helpers 的编译期断言单一来源」，该断言不存在；U-Boot 腿因此带着一份无编译期钉住的名字表副本

- **位置**：`os/boot-shim/src/uboot_main.rs:190-192`（注释 "`must match boot_shim::loader::MODULE_NAMES` — single-source via the compile-time assert in opensbi_helpers"）、`243-244`（`const MODULE_NAMES: &[&str] = uboot_layout::MODULE_NAMES;`）、`371-400`（`build_shim_table` 按这份本地表写 12 个模块条目的路径与 staging 地址）；对照 `os/boot-shim/src/loader.rs:71`（既有单一来源模式：`pub use minix_boot::MODULE_NAMES`，`[&str; NR_BOOT_MODULES]` 定长钉住计数）；文档 §4.3（「boot-shim 三条链的"顺序"部分没有编译期检查……两个直核腿另有 NAME_POOL 与 MODULE_NAMES 的编译期逐字节对账」）
- **现象**：`grep -n "const _" os/boot-shim/src/opensbi_helpers.rs` 只命中池几何断言（93、585-604 行），没有任何涉及模块名/顺序的编译期断言；`uboot_layout::MODULE_NAMES` 是一份 `&[&str]` 切片副本（连长度都没有编译期钉住）。文档 §4.3 的陈述是诚实的（只有两个直核腿有编译期对账），但 `uboot_main.rs` 的注释向维护者断言了一个不存在的守卫——与文档相反、与现实相反。
- **影响**：改动 `minix_boot::MODULE_NAMES`（顺序或名字）时，review 者看到注释会误信 U-Boot 腿已有编译期兜底而放开改；实际后果是 `build_shim_table` 仍按旧名单旧顺序写表、按旧顺序分配 staging 槽位，装载循环按新名单查路径失败后以 panic 收场（fail-closed，但发生在 QEMU 门跑起来之后，且消息只有「找不到文件」不含「哪一份名单漂了」）。注释级的假守卫声明正是文档-代码同步纪律要拦的类型。
- **最小改法**：`uboot_main.rs` 改为 `use minix_boot::MODULE_NAMES`（与 `loader.rs:71` 同款单一来源），删除本地副本；或在 `opensbi_helpers.rs` 真的补上注释所声称的 `const _: () = assert!(...)` 逐字节对账（照抄 `bootface_common.rs:54-68` 的形状）。两种改法同时修正注释。
- **严重度**：P3
- **反证路径**：`grep -rn "assert" os/boot-shim/src/opensbi_helpers.rs | grep -i "module\|name"`（应零命中）；对两份名单做 diff 确认当前一致；把本地副本故意改乱一个名字后跑 `cargo check -p boot-shim --target riscv64...`（应仍编译通过，证明断言确实不存在）。

## FINDING RD32-DESIGN-D2-04
模块名落入 `BOOT_MODULE_NAME_STORE` 有两份实现、两套契约强度：`store_kernel_info` 截断到 15 字节并回退 `"?"`，v1 wire 解码收 16 字节并对非法 UTF-8 直接 panic；文档 §4.5 只把前者的语义当作名字池契约来写

- **位置**：文档 §4.5（「名字池是唯一允许截断的……取 `valid_up_to()` 的最长合法前缀」段）；`os/kernel/src/lib.rs:1492-1503`（`store_kernel_info` 名字块：`n = bytes.len().min(BOOT_MODULE_NAME_LEN - 1)`，`from_utf8` 失败回退 `"?"`）对照 `os/kernel/src/lib.rs:675-697`（`arch_boot_resume_v1` 解码块：`position(|&byte| byte == 0)` 或全长 16，`from_utf8(...).expect(...)` panic）
- **现象**：同一不变量（把 12 个模块名落到 16 字节名池并产出 `&'static str`）有两个写点、两种规则：长度上限不同（15 vs 16）、非法输入结局不同（静默改成 `"?"` vs fail-fast panic）。文档 §4.5 用「公开函数不能把安全性寄托在调用者内容上」为宽容路径辩护，却未提及 wire 路径在同一文件里已采用更严的契约，也没解释为什么同一条不变量允许两档强度。
- **影响**：目前两条路径的实际输入都安全（builder 侧是编译期对账的 ASCII 字面量，wire 侧解码后还有 strict 门按位核名），所以是**潜在不对称**而非现行缺陷；风险在于未来把 wire 形态的名字改经 `store_kernel_info` 落地时，静默的 15 字节截断 + `"?"` 回退会把「名字错」降级成「名字被改名」，而按位核名只在 Strict 档跑（`kmain` 复跑的是 Standard 档）——位置身份的破坏可能在生产入口校验里不再可见。
- **最小改法**：抽一个共享的 name-landing helper（NUL 扫描 + UTF-8 校验 + 越界策略作为唯一参数），两个写点都调它；或在 §4.5 明写「两条落地路径契约强度不同及理由」，把差异从隐式变显式。
- **严重度**：P3
- **反证路径**：对照两个代码块的长度与 UTF-8 处理；`grep -n "ValidationLevel" os/kernel/src/lib.rs` 确认 `kmain` 复跑的是 `validate()`（Standard，不含按位核名）；构造一个 16 字节非 NUL 结尾的 `WireBootModule.name` 走 `handoff_v1_read` 的 round-trip 测试观察两路径分别的结局。

## FINDING RD32-DESIGN-D2-05
固件指针一次性安装口用 `static mut` + 仅 debug 构建的重复安装检查，与同一仓内同类「写一次」机制的既定标准（两种构建档都 fail-closed、避免 `static mut`）不一致

- **位置**：`os/boot-shim/src/opensbi_helpers.rs:481、494-507`（`static mut BOOT_FILE_TABLE_PTR` + `install_boot_file_table` 的 `debug_assert!` 重复安装检查，注释自述「release 构建跳过检查以省一次 load」）、`534、545-551`（`static mut DTB_PTR` + 同款 debug-only 检查）；对照 `os/kernel/src/boot_alloc.rs:16-22`（「`AtomicU64` is used instead of `static mut` to avoid Rust 2024 edition UB」）、`os/arch/src/arch/pt_alloc.rs:97-101`（重复注册「fail closed in release too」）、`os/boot-shim/src/lib.rs:161-172`（`BootStorageCell::take` 一次性标志双档 panic）
- **现象**：同一种 bug 类（一次性安装口被调两次）在本仓有三种执行档位：fail-closed 双档（pt_alloc、BootStorageCell）、双档静默沿用（无）、release 静默覆盖（本条）。`install_boot_file_table`/`install_dtb_ptr` 的 SAFETY 论证只靠「single-writer during early boot」这一时序约定，而同 crate 的 `BootAlloc` 注释明确以 `AtomicU64` 替代 `static mut` 来消掉这类约定。
- **影响**：release 下第二次 `install_*` 静默覆盖指针；当前调用链各只调一次所以无现行故障，但这是信任边界输入口的**契约弱于仓内同类机制**的不对称——且 `uboot_main` 的入口 trampoline 是未来最可能被改动的区域（新增参数通道、重放保护），改动者从 pt_alloc/BootStorageCell 得到的「重复即停机」预期在这里不成立。
- **最小改法**：两个安装口改 `AtomicUsize`/`AtomicU64` + `compare_exchange(0, addr)`，抢占失败即 panic（与 `PT_CLAIMED` 同形，双档生效）；文档无需改（文档未对这两处做守卫承诺）。
- **严重度**：P3
- **反证路径**：读三处对照代码；确认 release 构建下 `debug_assert!` 展开为空（`cargo check --release` 后查无检查指令）；确认调用点目前各一次（`grep -n "install_boot_file_table\|install_dtb_ptr" os/boot-shim/src/uboot_main.rs`）。

## FINDING RD32-DESIGN-D2-06
DESIGN-D2-17 的裸 `(u64,u64)` 白名单与窗口组装层的实际形态不符：`push_common_boot_windows` 的 `bump: Option<(u64,u64)>` 是白名单之外的第四处裸元组签名，模块/平台/设备窗口组装继续以裸对喂给 `BootWindows::push`

- **位置**：文档 §4.4（「**窗口类型的边界规则**（DESIGN-D2-17 的取舍）：裸 (u64, u64) 只允许出现在固件/分配器边界——三个点名位置；进入内核内部一律 PhysRange」段）；`os/kernel/src/lib.rs:1159-1166`（`push_common_boot_windows(..., bump: Option<(u64, u64)>, ...)`，内核内部私有函数，不在白名单三点之内）、`1081`（`fn push(&mut self, start: u64, end: u64)`）、`1110-1151`（`push_module_windows`/`push_platform_windows`/`push_device_windows` 均以裸 `u64` 对调 `push`）
- **现象**：文档宣布的规则是「进入内核内部一律 PhysRange，裸对仅限三个固件/分配器边界」，但窗口组装层本身就停在裸对世界：`push_common_boot_windows` 第四处裸元组形参（值来自白名单点 `boot_alloc_region()` 的返回，转发一跳到 `push`），`push` 自身是裸对接口，模块/平台/设备窗口组装函数全部裸对。反向区间的拒绝发生在 `push` 内部的运行期 `assert`（`lib.rs:1082-1092`），不是类型层。
- **影响**：规则文档描述的不是实现的边界位置：真正的转换地平线是 `BootWindows::push`，而白名单句读起来像「push 之内全是 PhysRange」。后果是规则不可核查（按文档字面，`push_common_boot_windows` 违规；按意图，它合法），未来新增窗口类别时既可能照文档把参数改成 PhysRange 也可能沿用裸对，两种做法都「符合某一种解读」——规则失去了规范力。这是文档-代码设计陈述不符，非行为缺陷。
- **最小改法**：改文档一处措辞：把白名单句改为「裸对允许到 `BootWindows::push` 为止；push 是转换点（内含 `PhysRange::new` + 空区间拒绝），push 之后一律 `PhysRange`」，并把 `push_common_boot_windows` 的 `bump` 形参列入白名单（或顺手改成 `Option<PhysRange>`，一行改动）。
- **严重度**：P3
- **反证路径**：读 `lib.rs:1068-1186` 全段；对照文档 §4.4 白名单三点逐一核对 `push_common_boot_windows` 不在其中；确认 `PhysRange` 在 push 之后才首次出现（`ranges: [PhysRange; MAX_BOOT_WINDOWS]`）。

## FINDING RD32-DESIGN-D2-07
DTB 腿上 DTB blob 位于 conventional 内存、是交接后的活输入，却既不在 startup-object 规范集合、也不在扣除记录里；与 arena 的处理不对称——arena 有成文生命周期合同与 poison 测试，DTB blob 的「先读后死」合同无成文、无测试

- **位置**：文档 §3.4 `startup_objects` bullet（规范集合=内核/模块/root/bump/arena，五类，DTB 不在内）、§4.7（「**arena 生命周期合同**」段：`LastRead(arena) ≺ Publish(VMFreePages)` + poison 测试）、§3.5 误回收风险表（DTB 行把「内核在 kmain 之后解析 DTB 时读到被改写的属性」列为历史险情的先坏点）；`os/kernel-image/src/bootface.rs:367-403`、`os/kernel-image/src/bootface_a64.rs`（startup 记录四类，无 DTB）、`os/boot-shim/src/opensbi_helpers.rs:432-444`（五类，无 DTB）、`os/boot-shim/src/uefi_helpers.rs:1155-1176`（五类；UEFI 腿的 DTB 在固件 Configuration Table，不在 conventional，本条不适用）；`os/kernel/src/vm_handoff.rs:675` 起 `for_each_live_bootstrap` 与文件头扣除表（五个扣除来源，无 DTB 行）；DTB blob 在 conventional 的依据：`opensbi_helpers.rs:299-301`（`DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)` 只裁 `[DRAM_BASE, 0x8020_0000)` 前缀）、`uboot_main.rs:189`（`DTB_STAGE = 0x8c00_0000`，高于裁剪界）
- **现象**：U-Boot shim 腿与两个直核腿上，内核经 `PlatformDescSource{DTB, phys_addr}` 持有指向 conventional RAM 里 DTB blob 的物理地址，kmain 早期的平台发现要读它；这份内存没有 startup 记录（strict 门只要求覆盖内核镜像跨度）、没有扣除行、也没有 arena 那样的成文时序合同。它今天安全，靠的是「平台发现先于 `build_vm_handoff` 发布 VM 自由页」这一未成文时序——与 §4.7 给 arena 立的合同同构，却只给 arena 立了。
- **影响**：同类「活的引导输入躺在自由清单里」的隐患，arena 有合同、有 poison 测试、有「未来改动必须同时移动解码边界」的警告句；DTB blob 三者皆无。任何未来消费者（运行期重读 DTB 属性、日志转储、04 篇之外的延迟解析）只要晚于 VM 发布自由页就会读到被复用的帧，而且故障形态正是 §3.5 表里描述的「读到被改写的属性、硬件发现失败」——文档自己指出过这个故障类，却没有为本设计立防线。
- **最小改法**：在 §4.7 arena 合同旁补一段 DTB-blob 合同（`LastRead(DTB) ≺ Publish(VMFreePages)`，指出三条 DTB 腿适用、UEFI 腿不适用）；若要机制化，DTB 腿在 startup 集合追加一条 DTB 记录（容量 20 现用 16，余 4 槽足够）——但那会把 DTB 页从 VM 永久扣除，需先裁决是否值得。
- **严重度**：P3
- **反证路径**：`grep -n "push_startup" os/kernel-image/src/bootface.rs os/boot-shim/src/opensbi_helpers.rs os/boot-shim/src/uefi_helpers.rs`（确认无 DTB 记录）；`grep -n "for_each_live_bootstrap" -A 20 os/kernel/src/vm_handoff.rs`（确认扣除五源无 DTB）；`grep -rn "platform\|dtb" os/kernel/src/main.rs` 类路径确认 DTB 唯一读者是 kmain 早期平台发现（若发现存在晚于 VM 发布的读者，本条升 P2）。

---

## SUGGESTION（纯偏好，不属发现）

- **S-1**：`uefi_helpers.rs:1090-1141` 的配对循环每次重试 `alloc_map_buffer` 新分配一块池缓冲且不释放旧缓冲（`656-663` 注释自述 never freed）。有界（EBS_ATTEMPT_LIMIT=2 + 扩容断言），无行为风险；若追求干净，可在重试分支对旧缓冲调 `free_pool`（仍在 Boot Services 期内）。
- **S-2**：`UefiFileLoader::read`（`uefi_helpers.rs:1247-1258`）每次读文件重新 `get_image_file_system` + `FileSystem::new`，13 个文件共 13 次；缓存一个 FS 句柄可省 12 次协议往返。纯效率偏好，boot 期一次性成本。
- **S-3**：dumpdtb 调试通道只在 aarch64 腿存在（`uefi_helpers.rs:965-968`：x86 硬编码 `None`），而 `DUMPDTB_PATH`（`loader.rs:39`）是无架构门控的共享常量；要么给常量加对称注释说明「仅 aarch64 腿消费」，要么补齐 x86 实现。

## 覆盖说明

- 已逐段读文档第 3 章（§3.1–§3.10）与第 4 章（§4.1–§4.9），§7.3 清单中的文件全部至少过了结构与关键函数；文档引用的代码块（`BootHandoffV1`、`BootStorage`/`BootReport`、`pt_alloc::register/alloc_pt_page`、`BootAlloc::init/resume/alloc`、`build_kernel_info`、`store_kernel_info`/`land_slice`、`prepare_and_enter_kernel_image`、`prepare_boot`、`UbootFileLoader`、`load_boot_modules_into`、`build_bootstrap_root_and_enable` Step 1/2、`X86_64Paging::enable`、`plan_kernel_mapping`、`rust_image_main` 三臂、`jump_kernel_image_v1` 及 x86 global-asm stub、`envelope_offsets`/arena 常量）均与源码逐字或近逐字核对通过——未发现引文与源码的陈述性漂移（§3.4.3 对 handoff_v1 v9 注释「仍写 6144 B」的指控经核对源码 282-287 行为准确的版本变更记录，属文档措辞问题，未列为发现）。
- 未展开：`ebs_map.rs` 五步判定的逐位常量（文档 §4.2.2 与源码注释逐行对应，抽验 `LAST_KNOWN`/`checked_len` 一致）、`vm_handoff.rs` 测试层、`memmap_source.rs`（143 KB，超出本篇 §3.4 的责任边界，其政策层归属 I-29 专项）。
