# RD03 · design-d2（P-DESIGN 第二遍）

- 审查者：独立设计审查（D2 遍），基线 =「文档主张 ↔ 源码事实」。
- 输入：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md` §3/§4（逐节）+ `related-code.txt` 清单内的实现文件（arch trait 两份、三架构 protection/trap_entry/trap_stub、kernel lib.rs/globals.rs/smp.rs/trap_dispatch.rs 的相关函数体）。
- 方法：批量 grep 关键类型与方法（`syscall_entry`/`cpu_count`/`Privilege`/`is_trap`/`kernel_may_alloc`/`with_protection` 调用点），再对命中函数体整读；不做通读。
- 纪律遵守：单实现 trait 不判坏（两 trait 均有三架构 + Mock 实现，是真实测试接缝）；bool 不一律换 enum（`user_accessible: bool` 只有两个合法值，见 F-06 的对照说明）。

---

## DIFF-TABLE（文档主张 ↔ 源码事实）

| # | 文档位置 | 文档主张 | 源码事实 | 判定 |
|---|---|---|---|---|
| D-01 | §4.2 | `init_protection` 次序：store → refresh_descriptors → load → program_entry_base → trap init → install_trap_stubs → configure_syscall → register → store → load | `os/kernel/src/lib.rs:2230-2282` 逐行一致 | 一致 |
| D-02 | §3.2 | 两 trait 各写顺序契约（GDT 先于 IDT） | `os/arch/src/arch/protection.rs:96-103`、`os/arch/src/arch/trap_entry.rs:107-113` 均有 | 一致 |
| D-03 | §3.3 | 三候选对比，选中全局 `SyncUnsafeCell<Option<…>>`，未初始化 panic | `lib.rs:2288-2297` 静态 + `with_protection*` 的 `expect`（`lib.rs:2318-2334`） | 一致 |
| D-04 | §3.7 | "`is_trap` 参数在 41 门上恒传 `false`" | 对 `set_handler` 成立（`os/arch/src/x86_64/trap_entry.rs:340` 恒传 false）；但 `init()` 在 9 个门上传 true（`:210-213,232-235`），文档 §4.4 未提 | 偏差（表述范围过窄被读成全局）→ F-01 |
| D-05 | §3.4 | `KernelInfo.syscall_entry` 只作元数据，`configure_syscall` 不读 | `trap_entry.rs:263-299` 确不读；字段全仓零运行期读者（写入点 `lib.rs:732`） | 一致（已声明）；字段本体 → F-15 |
| D-06 | §4.3 | anchor 槽注释含 `#GP(0x28)` 真实失败记录 | `os/arch/src/x86_64/protection.rs:49-60` 逐字一致 | 一致 |
| D-07 | §4.5 | STAR → LSTAR → SFMASK → EFER 次序 | `trap_entry.rs:283-298` 一致 | 一致 |
| D-08 | §4.6 | arm64 `init` 写 `TPIDR_EL1` 非 `SP_EL1`；`set_kernel_stack` 文档化 no-op | `os/arch/src/arm64/protection.rs:140-164,166-181` 一致；文档未提 `base==0` 哨兵的真实成因 → F-04 | 一致（有缺口） |
| D-09 | §4.6 | riscv64 状态只在 per-hart `sscratch`，无共享单元 | `os/arch/src/riscv64/protection.rs:57-77` 一致 | 一致 |
| D-10 | §3.6 | kmain 文档注释 Phase A–F 清单（逐字引用） | `lib.rs:1663-1670` 逐字一致；函数体内另有 Phase B.5 标记（`lib.rs:1810`）不在清单，文档未提 | 一致（小缺口） |
| D-11 | §4.1/§4.7 | `kernel_may_alloc` 闸门已删除，Rust 无对应物 | 删除属实（诚实记录 `lib.rs:3916`）；但 `lib.rs:3701,3706` 还留着描述已删机制的注释 → F-08 | 一致（代码注释残留） |
| D-12 | §4.1 | `store_kernel_info` 七块 .bss 落地 + `let mut k = *kernel_info` | `lib.rs:1453-1560` 一致 | 一致 |
| D-13 | §4.7 | "aarch64/riscv64：无状态实现（accessor 造哑实例）" | `lib.rs:2358-2375` 每次调用伪造 `init(0, 0)`；哨兵与变异丢弃未展开 → F-04 | 一致（已声明） |
| D-14 | §5.1 | 测试计数 16/13/11（x86_64 三文件） | trap_entry.rs 实数 13 = 2 结构 + 2 set_handler + 9 init 元数据，逐个核对 | 一致 |
| D-15 | §3.7 | 41 门清单 = 18+4+16+3；`user_accessible` = 3/4/32-35 | `trap_stub.rs:168-216`（41 项）、`:240-242` 一致 | 一致 |
| D-16 | §3.5 | `init_protection` 无任何 CR3/页表调用 | `lib.rs:2217-2283` 函数体无 `pg_*`/CR3 | 一致 |
| D-17 | §3.1/§4.7 | "`load_with_tss` 尾注的 'LDT descriptor is filled in the GDT' 是旧方案残留" | 残留仍在：`protection.rs:545-548` 尾注宣称 GDT 里有 LDT 描述符，而 `fill_flat_segments`（`:206-240`）不填任何 LDT → F-10 | 文档正确、代码未清 |

---

## FINDING RD03-DESIGN-D2-01（P2）
- **位置**：`os/arch/src/x86_64/trap_entry.rs:210-213,232-235`（init 的 is_trap=true）、`:52`（GATE_TYPE_TRAP）、`:333-341`（set_handler 恒 false）
- **严重度**：P2
- **分类**：[DEL]
- **问题**：`init()` 给向量 0/1/3/4/32-35 共 9 个门写 trap-gate 类型（`is_trap=true`），但这些门随后全部被 `install_idt_handlers → set_handler` 用 `is_trap=false`（中断门）整条重写；C 侧 `int_gate` 对全部 39 门统一写 `INT_GATE_TYPE`（`minix3/minix/kernel/arch/i386/protect.c:18,250`）。`GATE_TYPE_TRAP` 在活系统中没有任何可达写入路径。
- **影响**：死元数据造成两处误导：①读 `init()` 会以为最终门类型分两类，实际全为中断门（trap_stub.rs:33-36 的 stage invariant "every gate is an interrupt gate" 才是最终状态）；②文档 §4.4 只描述 PIC 行的 `false`，§3.7 的"恒传 false"只对 set_handler 成立，读者把两处合读会得出与 `init()` 源码矛盾的印象。13 条 init 测试无一断言门类型，此漂移不可测。
- **证据或推理**：`trap_entry.rs:210-213`（#DE/#DB/#BP/#OF true）、`:232-235`（软中断 true）、`:340`（set_gate false）、`protect.c:250`（C 统一 INT_GATE_TYPE）；对照 `os/arch/src/x86_64/trap_stub.rs:33-36`。
- **最小修复**：删 `set_gate`/`IdtEntry64::new` 的 `is_trap` 参数与 `GATE_TYPE_TRAP` 常量，`init()` 统一中断门；若要保留参数，须在 §4.4 补一句"init 的 true 值是死元数据、被 install 覆盖"。
- **置信度**：高（C 侧已核）。
- **是否需源码核实**：否。
- **若我错了**：若未来确有 trap-gate 需求（如 #DB 精确单步、共享断点），参数是预留——但当前它是被覆盖的死数据，预留应走文档声明而非隐性分支。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-02（P2）
- **位置**：`os/arch/src/x86_64/trap_stub.rs:168-216`（STUB_VECTORS bool 列）、`:219-224`（has_errcode）、`:271-317`（TRAPSTUB 字面量）、`:785-793`（errcode 测试）、`:796-814`（字节钉）
- **严重度**：P2
- **分类**：[DEL]
- **问题**："哪些向量 CPU 压错误码"这一份知识在代码里有三处编码：STUB_VECTORS 第二列、`has_errcode()` 的 match、`TRAPSTUB vec err` 的实例化字面量。唯一的交叉核对 `test_errcode_classification_matches_sdm` 检查的是 STUB_VECTORS ↔ `has_errcode`——两者互为副本，都不约束真源 TRAPSTUB；TRAPSTUB 的字节级钉住只覆盖 3/41 个向量（0、13、255）。
- **影响**：把 `TRAPSTUB 8 1` 改成 `TRAPSTUB 8 0` 不会被任何测试发现（8 的字节无钉），错误码槽归一化错位直接破坏 TrapFrame 布局——这正是 `test_stub_bytes_errcode_normalization` 注释里说"absolute byte contracts need byte pins"要防的那类事故。
- **证据或推理**：三处编码如上；`:796-814` 只构造 stub_0/stub_13/stub_255 三个切片。文档 §4.4 说"布尔值只被宿主测试使用"，属实，但未指出三处编码中两处互查、真源无钉的结构。
- **最小修复**：二选一——①用一个 `macro_rules!` 列表同时生成 TRAPSTUB 实例化与 STUB_VECTORS（符号名仍走 `stub_symbols!`）；②给全部 7 个 errcode 向量（8,10,11,12,13,14,17）补字节钉。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若宏方案因 global_asm! 混排不可行，走②即可；②无技术风险。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-03（P2）
- **位置**：`os/arch/src/arch/protection.rs:142-153`（trait 契约）、`os/arch/src/riscv64/protection.rs:152-161`、`os/arch/src/arm64/protection.rs:166-181`、`os/kernel/src/smp.rs:1162-1164`（唯一调用点，cfg x86_64）
- **严重度**：P2
- **分类**：[MODERN]
- **问题**：trait 契约写"cpu_id names the CPU whose entry state is being set"（C 语义：`tss_init(cpu,…)` 可写任意 CPU 的内存表），但寄存器承载的实现无法满足——riscv64 忽略 `_cpu_id`、写**当前 hart** 的 sscratch；arm64 整个 no-op。签名承诺了实现给不出的语义。当前唯一调用点在 `#[cfg(target_arch = "x86_64")]` 块内，掩盖了这条不一致。
- **影响**：未来任何非 x86 路径按契约传"目标 CPU"调用 `set_kernel_stack`，会静默把**调用者自己** hart 的 sscratch 改成别人的栈顶——正是 trait 文档自己说要防的那类 per-CPU 串写事故，只是换了方向。
- **证据或推理**：riscv64 实现注释自己承认"`_cpu_id` is implicit because the write targets the calling hart's own sscratch"（`riscv64/protection.rs:157-159`），与 trait 注释 `arch/protection.rs:144-150` 直接冲突。
- **最小修复**：在 trait 文档加一句架构分级契约："x86-64：cpu_id 可为任意 CPU；aarch64/riscv64：cpu_id 必须等于当前 hart（或被忽略）"，或把方法拆成 `set_kernel_stack_self()`。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若团队认为"smp.rs 的 x86-only cfg 已永久封闭调用面"，降 P3 记文档即可；契约冲突本身客观存在。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-04（P2）
- **位置**：`os/kernel/src/lib.rs:2358-2375`（非 x86 哑实例 accessor）、`os/arch/src/arm64/protection.rs:154-163`、`os/arch/src/riscv64/protection.rs:136-147,170-176`（base==0 哨兵）
- **严重度**：P2
- **分类**：[DEL]
- **问题**：为让 `init_protection` 一份函数体编译三架构，非 x86 的 `with_protection/with_protection_mut/with_trap_entry` 每次调用伪造 `CurrentProtection::init(0, VirBytes::new(0))` 哑实例；这个设计迫使 arm64/riscv64 的 `init`/`init_ap` 在**真实 API** 里引入"0 = 跳过硬件写"的哨兵分支（arm64 注释记录了一次因此产生的真实静默故障：首个用户 `svc` 在 -16 处存帧）。同时 `with_protection_mut` 在非 x86 上的变异被静默丢弃——与 §3.3 给 x86 accessor 选定的"未初始化时 panic，给出确定的失败点"原则相反。C 无此物（C 的架构差异由独立文件承载，没有哑实例层）。
- **影响**：①哨兵语义在 trait 契约里不可见，只活在各实现注释里；②"变异丢弃"没有失败点，靠读者读三个文件才能拼出全貌；③文档 §4.7 提了"造哑实例"但没提哨兵成因与静默丢弃这两个后果。
- **证据或推理**：`lib.rs:2361,2366-2369,2374`；arm64 `protection.rs:155-159`（"fabricates a throwaway `init(0, VirBytes::new(0))`… observed: first user `svc` stores its frame at `-16` → silent fault"）；riscv64 `protection.rs:137-144` 同构。
- **最小修复**：①在 `lib.rs:2358-2375` 三个非 x86 accessor 的文档注释里写明"实例是哑的、变异被丢弃、副作用只可能来自 CSR 写"；②在文档 §4.7 该行展开哨兵成因；③根治见 ARCH-RD03-A2。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若哨兵被视为"合法的'不触摸硬件'构造入口"而非 shim 产物，则只需做①②的文档化，删除级修复降级。
- **裁决建议**：接受（①②必做；③需用户裁决，见 ARCH-RD03-A2）。

## FINDING RD03-DESIGN-D2-05（P2）
- **位置**：`os/arch/src/arch/protection.rs:67-75,111,115-125`（Privilege + 关联类型机器）；三架构 `to_privilege/from_privilege/KERNEL_PRIVILEGE/USER_PRIVILEGE` 的实现（`x86_64/protection.rs:345-357` 等）
- **严重度**：P2
- **分类**：[DEL]
- **问题**：特权表达的关联类型机器（`type PrivilegeLevel` + 两个常量 + 两个转换函数 + `Privilege` enum）没有任何运行期消费者——全仓只有 roundtrip 测试读它。活的"用户/内核"表达是 `set_handler` 的 `user_accessible: bool`（`trap_entry.rs:339`）与 `user_accessible(vector)` 谓词（`trap_stub.rs:240-242`）。同一语义两套表达，一套死。C 无此物（C 直接写 DPL 位）。文档 §3.2 已自认"仅测试/接口预留"，但"预留"没有声明入口（无 TODO、无消费者路线图）。
- **影响**：最小化机会：每个新架构实现都要写 4 个无人调用的成员（本次三架构 ×4）；读者要自行判断哪套是活的。
- **证据或推理**：grep 全仓：`from_privilege/to_privilege/KERNEL_PRIVILEGE/USER_PRIVILEGE` 命中仅定义与三组 roundtrip 测试（`x86_64/protection.rs:587-604`、`arm64/protection.rs:219-236`、`riscv64/protection.rs:185-202`）。
- **最小修复**：删关联类型机器，等出现真实消费者（如调度器按特权过滤）再加；或最小保留 `Privilege` enum、删 `PrivilegeLevel` 关联类型与四成员。判据不是"bool 不够 expressive"——bool 在这里恰好封闭（两个合法值，非法状态不可表示），问题纯粹是无消费者。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若即将到来的异常/中断篇（14）需要跨架构读当前特权级（`CurrentEL`/`SPP`/CS.RPL），预留成立——那也应等该篇落地时随消费者一起进，而非先付四实现成本。
- **裁决建议**：需用户裁决（接口预留 vs 删）。

## FINDING RD03-DESIGN-D2-06（P3）
- **位置**：`os/arch/src/x86_64/protection.rs:186,383`、`os/arch/src/arm64/protection.rs:74-77,163`、`os/arch/src/riscv64/protection.rs:107-110,149`
- **严重度**：P3
- **分类**：[DEL]
- **问题**：三个架构的 `cpu_count` 字段注释均写 "Number of CPUs initialized (for SMP tracking)"，但没有任何运行期读者——唯一消费是各自文件里直接构字段的测试（`x86_64/protection.rs:729-732` 等）。x86 的 `init()` 还为它做了一次 `cpu_id + 1` 写。
- **影响**：注释承诺（"SMP tracking"）无兑现；结构体携带死字段。
- **证据或推理**：grep `cpu_count` 于 kernel/ 下零命中；arch 侧仅定义+自测。
- **最小修复**：删三处字段与三条测试（`init_sets_cpu_count`/`protection_has_cpu_count` ×2）；文档 §3.3 的结构体骨架视图同步少一行。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若 SMP 统计马上要读它，保留无害——但那应在 smp.rs 出现读者时再说。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-07（P3）
- **位置**：`os/arch/src/arch/trap_entry.rs:81-98`（向量常量块）、`os/kernel/src/trap_dispatch.rs:75`（`IPC_VECTOR_GATE: u8 = 33`）
- **严重度**：P3
- **分类**：[DEL]
- **问题**：arch 层的向量常量块（`DIVIDE_ERROR`…`PAGE_FAULT`、`KERN_CALL_VECTOR`、`IPC_VECTOR`、`COPROCESSOR_SEGMENT_OVERRUN`）在活代码里零使用——`KERN_CALL_VECTOR`/`IPC_VECTOR` 无读者，`COPROCESSOR_SEGMENT_OVERRUN`（向量 9）更是长模式保留向量、无门无 stub（`trap_stub.rs:781` 断言 `!has_stub(9)`）；而 kernel 侧真消费者 `trap_dispatch.rs:75` 自造魔数 `const IPC_VECTOR_GATE: u8 = 33`，同一知识两处、一处命名一处裸数字。
- **影响**：常量块给人"门表由这些常量驱动"的错觉（实际由 STUB_VECTORS 字面量驱动）；向量 33 的改号要同步两处且一处无编译期联动。
- **证据或推理**：grep `KERN_CALL_VECTOR|IPC_VECTOR` 非测试命中仅注释（`syscall.rs:778`）；`COPROCESSOR_SEGMENT_OVERRUN` 全仓仅定义行。
- **最小修复**：删 `COPROCESSOR_SEGMENT_OVERRUN` 与 `KERN_CALL_VECTOR`/`IPC_VECTOR`；`trap_dispatch.rs` 改用（保留的）`IPC_VECTOR`，或两侧互指同一常量。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若常量块被当作"向量号 ↔ C 宏名"的对照文档保留，则在块头注释写明"文档常量，活表在 STUB_VECTORS"。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-08（P3）
- **位置**：`os/kernel/src/lib.rs:3701,3706`（残留）；对照 `lib.rs:3916`（诚实记录）
- **严重度**：P3
- **分类**：[DEL]
- **问题**：`bsp_finish_booting` 的文档注释第 5 步仍写 "Set kernel_may_alloc = false"，并引 "Design decision D8 (08 §3): kernel_may_alloc uses AtomicBool"——该闸门已整体删除（本篇文档 §4.1/§4.7 正确记录"无对应物"），同函数体 `lib.rs:3916` 的 Step 8 注释才是删除后的诚实口径。AtomicBool 方案（D8）在代码里不存在。
- **影响**：同一函数内两条注释互相矛盾；读者按 3701 找 `kernel_may_alloc` 会扑空。
- **证据或推理**：grep `kernel_may_alloc` 于 os/ 仅 3701/3706/3916 三处注释（`os/target/fmtchk/*` 为构建副本）。
- **最小修复**：删 3701 行、改写 3706 行；或把两行并入 3916 的删除记录口径。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：不适用（纯注释事实）。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-09（P3）
- **位置**：`os/kernel/src/lib.rs:1689-1714`（rdi/rsp 证据块）、`:1770-1783`（GS_BASE + KERNEL_GS_BASE 探针）、`:1801-1809`（第二次 GS_BASE 探针）
- **严重度**：P3
- **分类**：[DEL]
- **问题**：kmain 生产体里留着自declared临时的 v3 证据采集：注释写 "Evidence capture (temporary, v3)" 与 "This shadow can be deleted once the diagnostics below confirm it"；两处 MSR 探针把 `GS_BASE`/`KERNEL_GS_BASE` 读进 `g_lo/g_hi/k_lo/k_hi` 后**全部丢弃**，每次 boot 白读 3 次 MSR。诊断块本身有 cfg(x86_64) 门（不破非 x86 构建），但没有 cfg(test/qemu) 门。
- **影响**：生产 boot 路径的噪声；文档 §4.1 把它描述为"诊断块留下的三个读数就是为此加的"，未提示其自declared临时身份。
- **证据或推理**：`:1689`、`:1686`（"can be deleted once…"）、`:1773-1782`（绑定未使用）、`:1804-1808`（同）。
- **最小修复**：三个块合并为 `#[cfg(any(feature = "qemu_test", debug_assertions))]`（或直接删除）。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若诊断确认仍需要（GS 编程问题复发过一次），移入 qemu_test 载体而非删除生产路径。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-10（P3）
- **位置**：`os/arch/src/x86_64/protection.rs:545-548`（尾注）；对照 `:206-240`（fill_flat_segments）
- **严重度**：P3
- **分类**：[DEL]
- **问题**：`load_with_tss` 尾注写 "The LDT descriptor is filled in the GDT for 32-bit compatibility but never loaded on 64-bit"——当前 GDT 根本没有 LDT 描述符（槽 0-6 为空/CS/DS/anchor/DS/CS）。本篇文档 §3.1/§4.7 已正确把它判为"旧方案残留"，但代码里的残留原句未清。
- **影响**：注释与 `fill_flat_segments` 的事实矛盾；新读者会去找那条不存在的描述符。
- **证据或推理**：`:546` 原句；槽位常量 `:46-63` 无 LDT 槽。
- **最小修复**：尾注改为"C 的 `lldt(LDT_SELECTOR)` 在 64 位不执行；GDT 无 LDT 描述符（§3.1）"。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：不适用（注释与同文件代码直接矛盾）。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-11（P3）
- **位置**：`os/arch/src/x86_64/protection.rs:501-512`（远跳 asm，`options(nostack)`）
- **严重度**：P3
- **分类**：[MODERN]
- **问题**：64 位无立即数远跳，CS 重载用 `mov/push/push/retfq` 替身（C 侧是 `ljmp` 直载）——替身本身正当；但该 asm 声明 `options(nostack)`（语义：asm 不向栈压数据），而序列实实在在压了两次栈（push/push 平衡后 retfq 弹回）。替身的契约声明与自身行为不符。
- **影响**：当前 push/pop 平衡、rsp 复原，实践无害；但按 Rust inline-asm 参考这是错误的 option 声明，且它向读者隐瞒"这段代码在栈上做了一次往返"。
- **证据或推理**：`:506-508` 两条 push、`:509-510` retfq、`:511` `options(nostack)`。
- **最小修复**：去掉 `nostack`（保留其余 options），或在 SAFETY 注释写明"push/push/retfq 平衡、rsp 复原，nostack 按'离场时 rsp 未变'口径声明"。
- **置信度**：中高。
- **是否需源码核实**：否（代码事实已核；`nostack` 语义按 Rust 参考口径）。
- **若我错了**：若 `nostack` 的官方口径是"离开 asm 时栈指针不变"而非"期间不压栈"，则本条降级为注释清晰度问题。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-12（P3）
- **位置**：`os/arch/src/x86_64/trap_stub.rs:387`（`push 32`）；对照 `:434-437`（user_cs/user_ss 的 const 绑定）
- **严重度**：P3
- **分类**：[MODERN]
- **问题**：syscall 入口 asm 给 `frame.vector` 压字面量 `32`（注释自注 "KERN_CALL_VECTOR"），而同一段 asm 的用户段选择子用 `const` 操作数绑定（`user_cs = const USER_CS_SELECTOR`）。同一"asm 读命名常量"的机制已就位，独这一处用裸数字；F-07 里的 `KERN_CALL_VECTOR` 常量恰在隔壁文件闲置。
- **影响**：向量号改号（若 32-35 区段调整）时此处的 32 不与任何表联动。
- **证据或推理**：`:387` vs `:434-437`；`arch/trap_entry.rs:97`（`KERN_CALL_VECTOR`）。
- **最小修复**：加操作数绑定 `kern_call_vector = const KERN_CALL_VECTOR.0`（或本地 `const`），`push {kern_call_vector}`。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：不适用（同文件内机制并存是直接对照）。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-13（P3）
- **位置**：`os/kernel/src/lib.rs:2313,2337`（"BKL-serialized callers" 注释）；对照 `lib.rs:3659-3663`（BklSection witness 范式）
- **严重度**：P3
- **分类**：[MODERN]
- **问题**：`with_protection/with_trap_entry` 的文档以 "(BKL-serialized callers)" 声明并发纪律，但签名无任何强制——同库已有的 `smp_state_with`/`ipc_filter_pool_with` 用 `&BklSection<'_>` 参数把这条纪律做进类型（Kernel = SMP + BKL 的既定表达），这两个 accessor 没跟上。调用点现实是安全的（boot 单线程窗口 + AP tail 与 BSP 无重叠写），但纪律的两种表达并存：一种在类型里、一种在注释里。
- **影响**：未来新增调用点时，witness 范式的保护不覆盖这两个入口；注释承诺可能静默失效。
- **证据或推理**：`lib.rs:2313-2345`（无 witness）vs `:3659-3663`（witness）。
- **最小修复**：注释改为准确口径（"boot 窗口单线程 + AP tail 只读；BKL 见证见 smp_state_with"）；或补 witness（会波及 init_protection/SMP 三个调用点，成本较大）。
- **置信度**：高（代码事实）；影响面评估为中。
- **是否需源码核实**：否。
- **若我错了**：若 AP tail 与 BSP 存在我未见的并发 `with_protection` 写路径，此条升级——但 x86 实现的 SAFETY 注释（`protection.rs:192-203`）与 smp.rs 的 BOOT_LOCK 结构均不支持该场景。
- **裁决建议**：接受（注释口径修正）。

## FINDING RD03-DESIGN-D2-14（P3）
- **位置**：`os/arch/src/x86_64/trap_stub.rs:684-698`（program_gs）、`:661-671`（GsAreas，硬编码 8）；对照 `os/arch/src/x86_64/protection.rs:244,390`（兄弟函数的 MAX_CPUS assert）
- **严重度**：P3
- **分类**：[MODERN]
- **问题**：`program_gs` 对 `(*GS_AREAS.0.get())[cpu_id as usize]` 直接索引，没有兄弟函数（`setup_tss_for_cpu`/`set_kernel_stack`）都有的 `assert!(idx < MAX_CPUS)`；越界会落在数组索引的裸 panic（消息无诊断），而非带 cpu_id 的断言。另外 `GsAreas` 的 `8` 与 protection.rs 的 `MAX_CPUS: usize = 8` 是两份独立魔数，注释只说 "One area per CPU slot"。
- **影响**：cpu_id 超限时失败点确定但诊断信息弱；两个 8 一旦调整（MAX_CPUS 变更）会静默失配。
- **证据或推理**：`:690` 索引无守卫；`protection.rs:83` `MAX_CPUS: usize = 8` 与 `trap_stub.rs:671` `; 8]`。
- **最小修复**：`program_gs` 加 `assert!(cpu_id < 8)`（或引 `MAX_CPUS` 常量——需要把 MAX_CPUS 提为 `pub(crate)`），索引失败即带上下文报错。
- **置信度**：高（守卫缺失）；（初稿疑虑的 UB 已排除：定长数组经裸指针索引仍走边界检查 panic）。
- **是否需源码核实**：否。
- **若我错了**：若 topology 已在更早层保证 nr_cpus ≤ 8 且有断言，则本条纯为一致性改进。
- **裁决建议**：接受。

## FINDING RD03-DESIGN-D2-15（P3）
- **位置**：`os/kernel/src/lib.rs:732`（写入）；`os/arch/src/lib.rs:370-376`（"reference-only metadata" 注释）；全仓无运行期读者
- **严重度**：P3
- **分类**：[DEL]
- **问题**：`KernelInfo.syscall_entry` 由 boot 侧传入、`store_kernel_info` 落地，但生产代码零读者——`configure_syscall` 用 `syscall_entry_va()`（内核自己的 asm 标签），注释自declared "reference-only metadata"。C 无此物（C 的入口地址从来来自自己的 `ipc_entry_*` 标签，`tss_init` 写 MSR 的就是标签地址）。
- **影响**：boot 契约（shim 传什么）与内核实际消费（自己算）出现一条永不闭合的通道；读者可能误以为改 shim 侧值能换入口。
- **证据或推理**：grep `.syscall_entry`：仅构造点 `lib.rs:732`、测试构造（`:5616` 等）、两处注释（`lib.rs:2268`、`arch/src/lib.rs:374`）。
- **最小修复**：字段保留与否受 01 篇（boot-shim 交接契约）管辖：若删，需同步 boot-shim 的 scalars 传递；若留，字段文档须写明"零消费者、仅链路观测用"。
- **置信度**：高。
- **是否需源码核实**：删字段需核 `os/boot`/shim 侧 scalars 结构（本次未读，超范围）。
- **若我错了**：若未来 AP/syscall 审计需要"引导方声称的入口 vs 实际装载的入口"对账，字段是观测缝——那应写成显式对账诊断，而非静默元数据。
- **裁决建议**：需用户裁决（跨篇契约，01 篇管辖）。

---

## ARCH-PROPOSAL

### ARCH-PROPOSAL RD03-A1（改变 boot 是什么：保护实例的就地构造）
**现状**：`init_protection` 是两段式——`CurrentProtection::init()` 在栈上构造完整 GDT/TSS 镜像 → `store_protection` 搬进全局 → `refresh_descriptors()` 按**新地址**重建全部 8 个 TSS 描述符 → 才能 `load()`（`lib.rs:2230-2240`；E1 注释 `lib.rs:2232-2238`）。`refresh_descriptors` 是为"值语义搬迁改变内嵌地址"这一自伤而生的专职修复方法（`protection.rs:291-299`）。
**提案**：把 `init` 改为**就地构造**：`init()` 直接把实例写进最终 resident 静态槽（`SyncUnsafeCell<Option<…>>` 初始化或等价的 pinned 槽）并返回 `&'static mut`/占位句柄，描述符从落笔起就按最终地址编码。`refresh_descriptors` 方法、E1 搬迁契约、以及"栈上构造→析构后 GDTR 指向复用栈内存"的整类故障（§3.3 第一候选）一起消失。
**代价/风险**：`init` 的签名不再返回 owned Self，trait 形状从值语义改为槽语义；三架构 + Mock 的 `init` 都要改形；宿主测试大量以 `X86_64Protection::init(0, …)` 取 owned 值，需配套 `#[cfg(test)]` 构造路径。这是 boot 形状的改变（初始化从"构造-搬-修"三拍变一拍），不是行为重写，故归 [ARCH]。
**影响面**：trait 契约（arch/protection.rs）、三架构实现、`init_protection`、§3.3 的设计论证需重写一节。
**建议**：需用户裁决。若裁决通过，建议与 R03-T02（refresh_descriptors 的真机覆盖缺口）合并处置——提案落地后该缺口连同其宿主不可测性一起消失。

### ARCH-PROPOSAL RD03-A2（改变 boot 是什么：init_protection 按 arch 分体，删除哑实例层）
**现状**：一份 `init_protection` 函数体服务三架构，代价是非 x86 的三个哑实例 accessor（F-04）+ 两个架构真实 API 里的 base==0 哨兵。同类问题已有既定解法：`smp_ap_tail` 就是三份 `#[cfg(target_arch = …)]` 分体（`smp.rs:1607-1730`），各体只保留共享骨架的架构差异。
**提案**：`init_protection` 同样拆为三份 cfg 分体：x86 保留现行全套；aarch64/riscv64 各自三行真实现（init → load → trap init/load），不经过 `with_protection*`。随后非 x86 哑 accessor 可删（或收进 `#[cfg(test)]`），arm64/riscv64 的 0-哨兵从真实 `init`/`init_ap` 路径退役（降级为测试构造的内部约定）。
**代价/风险**：三份函数体引入跨架构次序漂移的可能（现统一体保证 A–F Phase 次序单源）；缓解：把次序不变量抽成一个两边共调的小函数或以断言钉住。哨兵退役须先确认 `set_kernel_stack`（F-03）的契约文档先行落地。
**建议**：需用户裁决。若拒绝 F-04 的根治，至少落 F-04 最小修复的文档化两项。

---

## VERDICT

**总体判定：设计本体通过，残留清理项 15 条（P2×5、P3×10），ARCH 提案 2 条待裁。**

- **七维度结论**：状态表达（Option 全局 + panic 契约）与所有权（store-before-load、landing pads）两维设计与文档一致且优于 C 对应物；身份维度（SYSRET anchor、GS 区、`Tss64` 布局）有真实失败记录钉住；错误维度符合 errno/panic 契约；并发维度（BOOT_LOCK 内写 sp0、AP tail 只读、AtomicPtr 分发注册）自洽；抽象边界的主要问题集中在**trait 契约写出了实现给不出的语义**（F-03）与**哑实例层**（F-04）；可测试性维度的问题是知识多处编码而交叉核对未覆盖真源（F-02）与死元数据不可测（F-01）。
- **文档自身质量**：§4 的机制叙述与代码逐行吻合（DIFF-TABLE 16/17 行一致）；两处系统性缺口——init 元数据与最终门状态的差异（D-04）、非 x86 哑实例的后果链（D-13）——文档只写了前半句。
- **翻译残留画像**：本项目残留不是"C 写法照搬"，而是**统一抽象为兼容三架构而付出的隐性税**：哨兵参数、哑实例、预留的关联类型机器、死 cpu_count。C 侧语义淘汰（SYSENTER/LDT/页表重建/kernel_may_alloc）在本篇已清干净；剩下的残留集中在"为了编译通过"这一层。
- **收敛建议**：F-01/02/08/09/10 五条为低风险纯清理，可并入下一轮 style-fix/fix-guard 批次一次做完；F-03/04/05 与两条 ARCH 提案互相关联（A2 落地则 F-04 大半消解），宜合并一次裁决。

- 本次扫描：15 条 FINDING，2 条 ARCH-PROPOSAL。
