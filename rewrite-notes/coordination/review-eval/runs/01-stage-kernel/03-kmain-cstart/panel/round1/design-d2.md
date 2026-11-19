# 03-kmain-cstart · D03 R1 独立设计审查（P-DESIGN-D2）

- 基线：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`，1382 行，sha256 前缀 `469727d74e1ec63c`（本轮 `sha256sum` 复核一致）
- 审查范围：文档第 3、4 章的设计陈述，对照 `tools/doc-code-map.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md --check` 列出的 21 个文件（存在性 21/21）
- 只读纪律：未读作者解释/V1 段/其他面板输出/expected；仅用本面板允许的输入与只读命令
- 七维定义（本轮口径，便于复核）：
  1. 事实保真：文档陈述/引文与活代码是否逐字对得上
  2. 职责与所有权：谁持有表/状态，生命周期由谁保证
  3. 状态与不变量：是否存在哨兵值、占位值、release 下失守的断言
  4. 抽象层级：语义接口是否藏住机制，是否存在无消费者的抽象
  5. 三架构同构性：同一接口在三架构上的真实差异是否被文档如实计入
  6. 复杂度与最小化：同一事实的信源个数、可折叠的仪式
  7. 可验证性：陈述能否被现有测试/读数证实，还是只能靠注释自证

## DIFF-TABLE

### 1. 七维差异表

| # | 维度 | 文档陈述（位置） | 实现事实（锚点） | 差异判定 | 处置 |
|---|---|---|---|---|---|
| 1 | 事实保真 | §4.1：`KERNEL_MAY_ALLOC.store(true, Ordering::Release)`，`fn kernel_may_alloc` 无生产调用者（:964）；§4.7 行指向 `globals.rs` 的 `static KERNEL_MAY_ALLOC`（:1187） | 活树无该 static、无 store、无 getter；`os/kernel/src/lib.rs` 仅 3701/3706/3916 三行注释，Step 8 明说"Rust 侧没有对应物"（D01-F08，commit `c03a26875` 删除 C 形状 memmap 账本） | **FAIL**：落点已淘汰，文档仍写成"保留（改型为原子量）" | 接受（RD3-DESIGN-D2-01） |
| 2 | 事实保真 | §3.5：`build_bootstrap_root_and_enable` 的 Step 1 注释写着 `Maps 1024 × 4MB = 4GB (C) or 4 × 1GB (x86-64)…`（:890） | 该函数 Step 1 注释是另一段：`C maps 4 GiB with pg_identity because the 32-bit kernel runs inside that window; the Rust cross-image leg only needs…`（`lib.rs:1221-1229`）。`Maps 1024` 全树只命中 `os/target/fmtchk/kernel_src_lib.rs`（构建产物） | **FAIL**：引文不在活代码里 | 接受（RD3-DESIGN-D2-02） |
| 3 | 事实保真 | §4.1：诊断块留下 `rdi`、全局槽指针、`rsp`、`bootstrap_len` 四个读数（:968） | `kmain` 诊断块只打印三项：`entry_rdi`、`kernel_info` 指针、`rsp`（`lib.rs:1696-1714`）；`bootstrap_len` 只出现在 1691 行注释 | **FAIL**（三项 vs 四项） | 接受（RD3-DESIGN-D2-12） |
| 4 | 事实保真 | §3.1 GDT 槽 0–5 + TSS 从 6 起、表长 `6 + 8×2 = 22`；TSS 104 字节；IDT 256 项 | `GDT_USER_ANCHOR_INDEX=3`、`USER_DS=4`、`USER_CS=5`、`GDT_TSS_FIRST_INDEX=6`、`GDT_SLOTS_PER_TSS=2`、`MAX_CPUS=8`、`GDT_ENTRIES=22`；`TSS64_SIZE=104`；`IDT_ENTRIES=256`（`x86_64/protection.rs:46-87`、`trap_entry.rs:114`） | PASS | 保持 |
| 5 | 事实保真 | §4.3 访问字节 `0x9A/0x92/0xFA/0xF2`；代码段 `L=1`、数据段 `B=1`；第 3 槽 anchor | 常量逐字一致（`protection.rs:105-120`）；`fill_flat_segments` 写 null + 5 个描述符（`protection.rs:206-240`） | PASS（"填五个描述符"指五个非空槽，不含 null） | 保持 |
| 6 | 事实保真 | §4.5 四个 MSR 写入次序 STAR → LSTAR → SFMASK → EFER（:993、:1119-1125） | `configure_syscall` 逐条同序（`trap_entry.rs:283-298`） | PASS | 保持 |
| 7 | 事实保真 | §4.6 aarch64 `init` 写 `TPIDR_EL1`、`load` 只发 `isb`；riscv64 写 `sscratch`、`load` 空实现；两架构入口腿切换 | `arm64/protection.rs:140-197`、`riscv64/protection.rs:132-168`、`riscv64/trap_entry.rs:101-118`、`riscv64/trap_stub.rs:535-536` | PASS | 保持 |
| 8 | 职责与所有权 | §3.3：实例先搬进全局、再 load；`refresh_descriptors` 修描述符基址（:808-841） | x86 路径逐字成立（`lib.rs:2230-2240`）；但 non-x86 `store_protection` 直接丢弃、`with_protection` 每次 `init(0, 0)` 造哑实例（`lib.rs:2351-2375`） | **PARTIAL**：契约只对 x86 成立，文档未限定 | 接受（RD3-DESIGN-D2-04） |
| 9 | 职责与所有权 | §4.7：`prot_init_done` → 全局 Option + `with_protection/with_trap_entry` 的 panic 契约（:1195） | 同上；aarch64/riscv64 既无 Option 也无 panic 路径 | **PARTIAL**：summary 表把 x86 机制写成三架构共性 | 接受（RD3-DESIGN-D2-04） |
| 10 | 状态与不变量 | §3.7/§4.4：`TrapEntryArch::init()` 写元数据（DPL、IST、门类型、占位 0），`install_trap_stubs` 只把真实入口写进向量（:929-935、:1064） | `set_handler` → `set_gate` → `IdtEntry64::new(..., dpl, 0, false)` **重写整条记录**：DPL 来自第二张表 `user_accessible()`、IST 归零后 2/8 再补、`is_trap` 一律 false。生产路径（`init_protection` 先 install 再 load）里 init 的 0/1/3/4/32-35 `is_trap=true` 到不了 `load()`；唯一例外是 `test-protection` 载体不调 `install_trap_stubs`（`trap_stub.rs:613-633`、`trap_entry.rs:210-258`） | **FAIL**：文档写的分工不是代码的分工；init 的门元数据在生产路径上是死数据 | 接受（RD3-DESIGN-D2-03） |
| 11 | 状态与不变量 | 无文档陈述（实现自述） | `init` 对**所有** MAX_CPUS 槽调 `setup_tss_for_cpu(slot, BSP 栈顶)`，AP 槽的 `sp0` 在 BSP `set_kernel_stack` 之前与已赋值不可区分；消费点 `init_ap` 用 assert 兜底（`protection.rs:374-383`、`smp.rs:1160-1166`、`protection.rs:442-450`） | **RISK**：占位符不是类型上可辨别状态 | 需裁决（RD3-DESIGN-D2-11） |
| 12 | 状态与不变量 | 无文档陈述（实现自述） | riscv64 `set_kernel_stack` 对 0 只有 `debug_assert_ne!`；release 构建会静默把 0 写进 `sscratch`，与注释"loud stop instead of silently poisoning"相反（`riscv64/protection.rs:152-160`） | **RISK**：生产构建失守 | 需裁决（RD3-DESIGN-D2-11） |
| 13 | 抽象层级 | §3.2：两 trait 拆分让"每个实现只回答自己那组问题"；接口对三架构表达"特权级 + 栈"（:779-786） | `Privilege`/`PrivilegeLevel`/`to_privilege`/`from_privilege`/`KERNEL_PRIVILEGE`/`USER_PRIVILEGE` 零生产消费者：只有 trait 定义、impl、单元测试与一处宿主审计测试的"存在性"文本断言 | **DEAD**：抽象只对测试说话 | 需裁决（RD3-DESIGN-D2-05） |
| 14 | 抽象层级 | 同上 | `AArch64Protection.cpu_count` / `Riscv64Protection.cpu_count` / `X86_64Protection.cpu_count` 写而不读（测试直接构造读取 x86 的字段） | **DEAD STATE** | 接受（RD3-DESIGN-D2-06） |
| 15 | 抽象层级 | §3.2 把 `idt_reload` 映射到 `TrapEntryArch::load` / `load_ap`（:798） | 三个架构的 `load_ap` 实现体全部是 `self.load()`，无一架构有独立语义（`x86_64/trap_entry.rs:329-331`、`arm64/trap_entry.rs:113-117`、`riscv64/trap_entry.rs:120-124`） | **REDUNDANT**：方法名暗示差异，实现零差异 | 接受（RD3-DESIGN-D2-07） |
| 16 | 抽象层级 | §3.7：`set_handler` 是 trait 面的一部分（:929-935） | 唯一调用者是 boot 安装器 `install_idt_handlers`；没有任何运行期动态注册；aarch64/riscv64 实现为 no-op | **OVER-WIDE**：为"未来动态注册"预支接口 | 需裁决（RD3-DESIGN-D2-08） |
| 17 | 三架构同构性 | §4.6 明确列出三架构承载物差异（:1175-1179） | 差异本身对得上；但代价没进文档：`TrapEntryArch` 5 个方法里 `configure_syscall` 与 `set_handler` 在 aarch64/riscv64 上是 no-op，`load_ap` 在三个架构上都只是 `load` 的别名；`ProtectionArch::set_kernel_stack` 在 aarch64 是文档化 no-op | **PARTIAL**：无谎言，但"接口统一"的收益与成本未计 | ARCH（RD3-DESIGN-D2-16） |
| 18 | 复杂度与最小化 | §4.4 自述：`STUB_VECTORS` 的 errcode 布尔"只被宿主测试使用；汇编入口由 `TRAPSTUB vec err` 字面量实例化，不读这个布尔值"（:1109） | 同一事实三处信源：`STUB_VECTORS` 第二列、`has_errcode()`（零生产调用者）、`global_asm!` 的 41 行字面量；另有 `stub_symbols!` 第三份符号清单（`trap_stub.rs:168-236`、`219-224`、`271-317`、`452-464`） | **DUPLICATE** | 接受（RD3-DESIGN-D2-09） |
| 19 | 复杂度与最小化 | §3.3 三候选表（栈上 / Box::leak / 全局 Option） | 候选集缺"就地初始化"：搬到全局后再 `refresh_descriptors` 是自造的二次修复，E1 整类缺陷可由"先落位再初始化"消除（`lib.rs:2230-2240`、`protection.rs:286-299`、`386`） | **INCOMPLETE**：设计空间少一维 | 需裁决（RD3-DESIGN-D2-16） |
| 20 | 可验证性 | §3.7：`gate_present` 的交叉核对由宿主测试执行，`load()` 不检查；真机载体不调 `install_trap_stubs`（:948） | 与实现一致：`gate_present`/`has_errcode`/`has_stub` 均只在 `#[cfg(test)]` 或宿主测试被调（grep 零生产调用者）；载体 `test-protection` 只 `init`+`configure_syscall`+`load`，门入口保持 0 | PASS（披露充分）；但由此暴露第 10 行的死元数据无测试 | 保持 + 见 RD3-DESIGN-D2-03 |

证据命令（本节所有"零生产调用者/不存在"陈述的可复现读数）：

```
$ grep -rn "KERNEL_MAY_ALLOC\|fn kernel_may_alloc" os/kernel/src os/arch/src --include=*.rs
os/kernel/src/lib.rs:3701:/// 5. Set kernel_may_alloc = false
os/kernel/src/lib.rs:3706:/// Design decision D8 (08 §3): kernel_may_alloc uses AtomicBool.
os/kernel/src/lib.rs:3916:    // Step 8: (D01-F08) C 的 kernel_may_alloc = 0（main.c:105）在 Rust 侧
$ grep -rn "Maps 1024" os/kernel/src os/arch/src
（无输出）
$ grep -rn "has_errcode\|has_stub(\|gate_present" os --include=*.rs | grep -v target
os/arch/src/x86_64/trap_stub.rs:219:pub const fn has_errcode(vector: u8) -> bool {
os/arch/src/x86_64/trap_stub.rs:227:pub const fn has_stub(vector: u8) -> bool {
os/arch/src/x86_64/trap_stub.rs:774:            let present = entry.gate_present(vector);
os/arch/src/x86_64/trap_stub.rs:777:                has_stub(vector),
os/arch/src/x86_64/trap_stub.rs:781:        assert!(!has_stub(9) && !has_stub(15), "reserved vectors must stay stub-free");
os/arch/src/x86_64/trap_stub.rs:788:        // with has_errcode — the TRAPSTUB instantiation reads the second
os/arch/src/x86_64/trap_stub.rs:791:            assert_eq!(has_errcode(vector), err, "vector {vector:#04x} errcode flag");
os/arch/src/x86_64/trap_entry.rs:153:    pub fn gate_present(&self, vector: u8) -> bool {
```

（`set_gate_ist` 的唯一调用者是安装器的 2/8 两个分支：`grep -rn "set_gate_ist" os --include=*.rs` 命中 `trap_entry.rs:144` 定义与 `trap_stub.rs:627-628` 两处调用。）

### 2. 翻译残留六类

| 类 | 残留实例 | 锚点 | 判定 | 处置 |
|---|---|---|---|---|
| R1 命名残留 | `sp1`/`sp2` 以 C 的"每特权级栈"命名保留，实为零值格式槽；文档 §3.1 已注明"零值、无读者" | `x86_64/protection.rs:126-128` | 可接受（TSS 格式要求偏移） | 拒绝改（保留格式） |
| R1 命名残留 | `X86_64_STACK_TOP_RESERVED = 2 * size_of::<u64>()`：C 的第一个字放"当前进程指针"，Rust 侧该字无写者、无读者；只有第二个字（CPU 号戳记）被 `init_ap` 自校验 | `protection.rs:89-101`、`316-324`、`455-463` | 结构完整但语义半空 | 拒绝改（AP trampoline 常量与 C parity 同时依赖 16 字节；记档即可） |
| R2 结构残留 | `cpu_count` 字段在三个 Protection 实现里都写而不读（C 侧是 `CONFIG_MAX_CPUS`/cpu 计数账本） | `protection.rs:186`、`arm64/protection.rs:76`、`riscv64/protection.rs:109` | 死状态 | 接受（RD3-DESIGN-D2-06） |
| R3 控制流残留 | C 的 `prot_init` 是一条顺序调用；Rust 的 `init → store → refresh_descriptors → load` 四步是 move 语义的补丁，不是 C 对应物。文档 §3.3 把它写成"选中"的必然形态 | `lib.rs:2230-2240` | 自造复杂度 | 需裁决（RD3-DESIGN-D2-16） |
| R3 控制流残留 | non-x86 的 `store_protection` 丢弃 + `with_protection` 每次 `init(0, 0)`：C 的"一次性初始化"语义在寄存器架构上退化成"每次访问都再造一次" | `lib.rs:2351-2375` | 接口形状强加 | 需裁决（RD3-DESIGN-D2-04/16） |
| R4 注释残留 | `ProtectionArch` 映射表还写着 `load()` = `lgdt, lldt, ltr, reload segment registers`；`lldt` 已淘汰 | `arch/protection.rs:91` | 陈旧 | 接受（RD3-DESIGN-D2-10） |
| R4 注释残留 | `load_with_tss` 注释："The LDT descriptor is filled in the GDT for 32-bit compatibility but never loaded"——Rust GDT 里没有 LDT 描述符（槽 3/5 是用户 CS 与 anchor） | `x86_64/protection.rs:545-548` | 事实错误 | 接受（RD3-DESIGN-D2-10） |
| R4 注释残留 | `arm64/protection.rs` 模块文档说 `msr SP_EL1` 在 `SPSel=1` 下"architecturally UNDEFINED and traps"；`fn init` 注释说真实崩溃不是 UNDEFINED。两处互斥，文档 §4.6 引的是后者 | `arm64/protection.rs:28` vs `:141-146` | 自相矛盾 | 接受（RD3-DESIGN-D2-10） |
| R4 注释残留 | `gate_present` 注释自称 "Load-safety half of the 'no empty vector' stage invariant"，但 `load()` 不调用它、生产路径零调用者 | `x86_64/trap_entry.rs:148-155` | 过度自称（文档 §3.7 已澄清） | 接受（RD3-DESIGN-D2-10） |
| R5 死代码残留 | `Privilege`/`PrivilegeLevel` 全套 | `arch/protection.rs:59-125` + 三 impl | 零生产消费者 | 需裁决（RD3-DESIGN-D2-05） |
| R5 死代码残留 | `load_ap` 三实现全等于 `load` | 见七维表 #15 | 冗余方法 | 接受（RD3-DESIGN-D2-07） |
| R5 死代码残留 | `InterruptVector` 在 protection.rs 的 "backward compatibility" 再导出 | `arch/protection.rs:54-57` | 无外部消费者的兼容层 | 接受（RD3-DESIGN-D2-14） |
| R5 死代码残留 | `unsafe impl Send/Sync for X86_64Protection`：结构体全是 plain data（自动 Send+Sync），`SyncUnsafeCell` 的 `Sync` 只要求 `BklProtected`；两个手写 impl 不改变任何可用性 | `x86_64/protection.rs:192-203`、`globals.rs:56` | 冗余 unsafe 面 | 接受（RD3-DESIGN-D2-15） |
| R6 文档残留 | `KERNEL_MAY_ALLOC` 落点、`Maps 1024` 引文、"四个读数"、`§4.2` 省略号说明 | 见七维表 #1/#2/#3 与 RD3-DESIGN-D2-13 | 文档与活码脱节 | 接受（RD3-DESIGN-D2-01/02/12/13） |

### 3. 死抽象（trait-audit 交叉核对）

`trait-audit.md` 的单行扫描对本篇核心 trait 的判定是 `ProtectionArch` MULTI-IMPL（4 impl / 33 ref）与 `TrapEntryArch` MULTI-IMPL（4 / 69），即"不是死 trait"；死的是 trait 内部的成员与外围类型：

| 对象 | 事实 | 判定 |
|---|---|---|
| `PrivilegeLevel` 关联类型 + `Privilege` 枚举 + 4 个转换/常量 | 生产调用者 0；`arch_small_files_audit.rs:45-49` 只断言"存在 `to_privilege(...)` 文本" | 死抽象候选（P2，需裁决） |
| `ProtectionArch::cpu_count` 字段（三个实现） | 写而不读；唯一读者是 x86 单元测试直接构造 | 死状态（P3） |
| `TrapEntryArch::load_ap` | 三实现 = `load()`，无差异 | 冗余方法（P3） |
| `TrapEntryArch::set_handler`（aarch64/riscv64 实现） | 两个 no-op 实现；唯一调用者是 boot 安装器 | 过宽接口（P3，需裁决） |
| `install_trap_stubs`（aarch64/riscv64 分支） | 两个文档化 no-op | 与上一行同源；可保留为 OS 层统一调用点（拒绝删除） |
| `InterruptVector` 的 protection 再导出 | "backward compatibility" 无消费者 | 死兼容层（P3） |
| `BklProtected`（trait-audit 记 NO-IMPL / 31 refs） | 宏 `bkl_protected_impls!` 生成的 impl 被单行扫描漏计；实际有实现 | **非死抽象**（audit 假阳性，记录以免误删） |

### 4. 非法状态

| 状态 | 如何产生 | 现有防线 | 缺口 |
|---|---|---|---|
| 非 BSP 的 TSS 槽 `sp0` 指向 BSP 栈 | `init()` 对全部 MAX_CPUS 槽用同一个 `kernel_stack_top` 初始化（`protection.rs:374-376`） | `init_ap` 断言 `sp0 == 本 CPU 栈顶 - 16` 且戳记等于 cpu_id（`protection.rs:442-463`） | 占位值与已赋值在类型上不可区分；若 `set_kernel_stack` 漏调，错误要等到 AP 执行 `init_ap` 才暴露，而不是在写入点 |
| non-x86 `init(0, 0)` 哨兵 | `with_protection` 为拿到 `&Self` 造临时实例，用 `base == 0` 表示"不要写真寄存器"（`arm64/protection.rs:154-162`、`riscv64/protection.rs:136-147`） | 注释 + `if != 0` | 真实调用者传 0 也会被静默跳过；契约靠魔法值而非类型 |
| riscv64 `sscratch = 0` | `set_kernel_stack(0)` 在 release 下通过 `debug_assert_ne!`（`riscv64/protection.rs:156`） | debug 构建断言 | release 构建静默毒化；注释承诺的"loud stop"在生产形态不存在 |
| `TrapEntryArch::init` 的门元数据与上线态不同 | `install_idt_handlers` 用第二张 DPL 表重写每条门（`trap_stub.rs:613-633`） | 无（最终态恰好与 C 对齐） | 生产路径上 init 的 `is_trap=true` 不生效且无测试；两处 DPL 信源漂移不会被发现 |

### 5. 最小化机会

| 机会 | 预期收益 | 风险/处置 |
|---|---|---|
| 删 `Privilege`/`PrivilegeLevel` 全套（trait 关联类型 + 常量 + 4 方法/架构 + 往返测试） | 约 120–150 行（含三架构 impl 与测试）；`ProtectionArch` 的方法面从 8 个（6 方法 + 2 转换）降到 6 个 | 若视为未来扩展点则保留，但需在 §3.2 说明"非运行时用"——需裁决 |
| 删三个 `cpu_count` 字段与 `cpu_id + 1` 记账 | 约 15 行；`AArch64/Riscv64Protection` 变 ZST | 接受 |
| `load_ap` 改为 trait 默认实现 `self.load()` 或直接删除 | 3 份重复实现消失 | 接受 |
| 错误码单源：用一张声明表驱动 `global_asm!` 与符号清单（`concat!`+`stringify!` 可生成符号名），删 `has_errcode`/`has_stub`/第二列/手写符号清单 | 约 60–80 行与三处漂移面 | 接受（P3；若是本轮成本约束可下轮） |
| `install_trap_stubs` 只写 handler 偏移、保留 init 的 DPL/IST/门类型 | 消除死元数据与 DPL 双源，符合文档 §3.7 的既定分工 | 接受（RD3-DESIGN-D2-03） |
| 就地初始化替代 store→refresh（ARCH 提案） | 删 `refresh_descriptors` trait 方法 + x86 实现 + 调用点 + E1 整类缺陷 | 需裁决（RD3-DESIGN-D2-16） |
| non-x86 用静态 ZST 实例替代每次 `init(0,0)` | 删两处 `base == 0` 哨兵与长注释；`init` 恢复"只调一次" | 需裁决（并入 RD3-DESIGN-D2-16） |

## FINDING RD3-DESIGN-D2-01（P0）[DEL]

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md` §4.1（:964）、§4.7 表（:1187）
- 陈述：Rust 侧存在 `KERNEL_MAY_ALLOC.store(true, Ordering::Release)` 闸门，`fn kernel_may_alloc` getter 当前无生产调用者；§4.7 把 C 的 `kernel_may_alloc` 处置写成"保留（改型为原子量）"，并锚定 `os/kernel/src/globals.rs:static KERNEL_MAY_ALLOC`。
- 实现事实：活树既无该 static，也无 `.store`/getter。`os/kernel/src/lib.rs` 只剩三行注释（3701、3706、3916）；3916 起的 Step 8 注释明说："C 的 kernel_may_alloc = 0 在 Rust 侧没有对应物——该开关唯一的读者是已删除的 add_memmap 断言；账本换成 FreeRangeSet 后……不需要运行期布尔。" 该删除来自 D01-F08（commit `c03a26875`，标题 "fix(01): D01-F08——C 形状 memmap 账本删除，FreeRangeSet 接管扣减"）。
- 差异定性：P0-fact。文档把一个已被冻结决策删除的落点写成现行机制，且引用了不存在的 anchor；读者按 §4.1 去 `globals.rs` 找不到任何符号，按 §4.7 会误以为"闸门语义保留"。
- 证据：`grep -rn "KERNEL_MAY_ALLOC\|fn kernel_may_alloc" os/kernel/src os/arch/src --include=*.rs` → 仅三行注释；`grep -n "KERNEL_MAY_ALLOC" os/kernel/src/globals.rs` → 无输出。
- 建议：§4.1 删除 store/getter 两句，改写为"Phase A 的 alloc 闸门随 D01-F08 淘汰；引导期写权限由调用图与 FreeRangeSet 保证"；§4.7 该行处置改"淘汰"，落点改指 `lib.rs:3916-3919` 的 Step 8 注释。与 §4.1 已有的"Phase A.2 为空"叙述合并成一段，避免同一淘汰事实分散两处。
- 处置：**接受**（必须修，P0）。

## FINDING RD3-DESIGN-D2-02（P0）[DEL]

- 位置：§3.5（:890）
- 陈述："`build_bootstrap_root_and_enable` 的 Step 1 注释写着它与 C 的对应（`Maps 1024 × 4MB = 4GB (C) or 4 × 1GB (x86-64)…`）"。
- 实现事实：该函数（`os/kernel/src/lib.rs:1212`）的 Step 1 注释是另一段话："C maps 4 GiB with `pg_identity` because the 32-bit kernel runs inside that window; the Rust cross-image leg only needs its shim image/stack, the kernel LMA span, the bootstrap pool…"（1221-1229）。被引的 `Maps 1024 × 4MB…` 全树只出现在 `os/target/fmtchk/kernel_src_lib.rs`（构建沙箱产物，不是源码）。
- 差异定性：P0-fact。引号内的文字与活代码不一致；这正是本项目"引文只许省略不许改写"的硬门所禁止的形态（D-045 同族）。此外该段还把 `IDENTITY_MAP_END` 说成"在 lib.rs 里保留"——它在 lib.rs 是以 `BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END` 重导出（:79），定义在 `os/arch/src/lib.rs:217`，文档后半句是对的，前半句应改为"重导出"。
- 证据：`grep -rn "Maps 1024" os/kernel/src os/arch/src` → 无输出；`sed -n '1221,1229p' os/kernel/src/lib.rs` → 上述另一段注释。
- 建议：换成现注释的首句（"C maps 4 GiB with `pg_identity`…"），或删掉引文只保留"Step 1 注释记录了它与 C 的差异"；`IDENTITY_MAP_END` 的措辞改"重导出"。
- 处置：**接受**（必须修，P0）。

## FINDING RD3-DESIGN-D2-03（P1）[DEL]

- 位置：§3.7（:929-935）、§4.4（:1064-1065）、§4.7 门表行（:1196）
- 陈述：分工是"`TrapEntryArch::init()` 写元数据（DPL、IST、门类型、占位 0），`install_trap_stubs` 把真实入口写进每一个向量，然后才 `load()`"；§4.7 说 `STUB_VECTORS` 41 项 + `install_idt_handlers` 是 `gate_table_*` 39 条的改型。
- 实现事实：`install_idt_handlers` 对每个向量调用 `set_handler`，后者走 `set_gate` → `IdtEntry64::new(handler, KERN_CS_SELECTOR, dpl, 0, false)`，**整条 16 字节门重写**：
  - DPL 不取自 `init()`，而取自第二张表 `user_accessible(vector)`（`3 | 4 | 32..=35`，`trap_stub.rs:240-242`）；
  - IST 一律写 0，随后只对向量 2/8 用 `set_gate_ist` 补回 1/2；
  - `is_trap` 一律 `false`（中断门），而 `init()` 对向量 0/1/3/4/32-35 写的是 `true`（陷阱门）。
  因此在生产路径上 `init()` 的 DPL 表、IST 表（2/8 除外）与 `is_trap` 全部是死数据；上线态完全由 `install_idt_handlers` 决定（`test-protection` 载体不调安装器，是唯一例外，见 §5.2）。宿主测试 `idt_init_breakpoint_has_dpl3`、`idt_init_double_fault_uses_ist2`、`idt_init_sets_syscall_ipc_vectors` 等断言的是装载前的中间态，不是 CPU 实际持有的门。最终态恰好与 C 的 `int_gate`（全中断门、DPL 来自表）一致，所以行为没错——错的是文档描述的分工与代码里两份 DPL 信源。
- 差异定性：P1（设计陈述不符 + 死元数据 + 双信源）。若按 P0-design-deviation 的严格口径可升为 P0，但最终行为与 C 对齐，本轮按 P1 计。
- 证据：`sed -n '333,342p' os/arch/src/x86_64/trap_entry.rs`（set_handler 重写全门）；`trap_entry.rs:210-258`（init 的 is_trap/IST）与 `:144-146`（set_gate_ist）；`sed -n '613,633p' os/arch/src/x86_64/trap_stub.rs`（安装器）。
- 建议：二选一。A（推荐，小改）：`install_trap_stubs` 只写 offset 三段，保留 `init` 的 DPL/IST/门类型——与文档 §3.7 现述分工一致，并让 `idt_init_*` 测试真正断言上线态；B：接受 install 全量重写，则 §3.7/§4.4 改述为"init 只登记门的存在性与初始值，install 决定全部字段"，并把 `init` 的 `is_trap=true`/DPL 去掉（只留 presence），同时把 DPL 单源化到 `user_accessible`。
- 处置：**接受**（文档与代码至少一侧改；建议选 A）。

## FINDING RD3-DESIGN-D2-04（P2）[DEL]

- 位置：§3.3（:808-841）、§4.7 表 `prot_init_done` 行（:1195）
- 陈述：Rust 的选择是"实例先搬进全局、再装载；accessor 在未初始化时 panic，给出确定的失败点"；§4.7 将 `prot_init_done` 的 Rust 落点写成"全局 `Option` + `with_protection`/`with_trap_entry` 的 panic 契约"。
- 实现事实：这套契约只存在于 x86。aarch64/riscv64 分支里 `store_protection`/`store_trap_entry` 直接 `let _ = ...` 丢弃；`with_protection`/`with_protection_mut`/`with_trap_entry` 每次调用都 `CurrentProtection::init(0, VirBytes::new(0))`（或 `CurrentTrapEntry::init()`）造一个临时实例再传引用（`lib.rs:2347-2375`）。因此：不存在"未初始化 panic"的失败点；`ProtectionArch::init` 的文档"Called once during `cstart()`"在两个架构上被违反（每次 accessor 都调一次）；`TrapEntryArch::load` 装载的实例不是 `store_trap_entry` 保存的那个（后者被丢弃）。
- 差异定性：P2（文档未限定架构，把 x86 的存储模型写成三架构共性；实现侧用哑实例兜住 trait 形状）。这也是 §3.3 三候选表的隐含前提——"表收进结构体、搬移会改变地址"只在 x86 成立。
- 建议：§3.3 首段加限定"以下只适用 x86-64（GDT/TSS/IDT 是内存表）；aarch64/riscv64 的保护状态是寄存器，不进入全局"；§4.7 该行限定 x86，或加第三列说明 non-x86 是"无状态实现"。代码侧与 RD3-DESIGN-D2-16 一并处理。
- 处置：**接受**（文档限定语）。

## FINDING RD3-DESIGN-D2-05（P2）[DEL]

- 位置：`os/arch/src/arch/protection.rs:59-75`（`Privilege`）、`:104-125`（`PrivilegeLevel`、`KERNEL_PRIVILEGE`、`USER_PRIVILEGE`、`to_privilege`、`from_privilege`）；三架构 impl 的同名四块；§3.2（:786）"接口对三个架构都表达'特权级 + 栈'"
- 陈述：两 trait 拆分的收益之一是"`ProtectionArch` 的接口对三个架构都表达'特权级 + 栈'"。
- 实现事实：OS 层没有任何调用点查询或转换特权级。全树 grep（排除 `target/`）的消费者只有：trait 定义、四个 impl、各架构 `privilege_level_roundtrip` 单元测试，以及一处宿主审计测试的文本存在性断言（它断言源码里"包含 `to_privilege(...)` 字样"，不调用生产路径）。也就是说这套类型只为测试与"接口看起来完整"存在。
- 差异定性：P2 死抽象。`trait-audit.md` 把 `ProtectionArch` 判为 MULTI-IMPL 是正确的，但 audit 的"引用数"统计的是 trait 名，不是成员；成员级的事实需要本行。
- 建议：删除 `Privilege`、关联类型、两个常量、两个转换方法与三个 impl 的对应块（连带删除 `privilege_level_roundtrip` 与 audit 的文本断言）。若团队判定它是异常/陷阱层未来要用的扩展点，则保留但在 §3.2 明确写"当前无运行期消费者，仅作为接口预留"，避免读者误以为 OS 层真的按特权级分支。
- 处置：**需裁决**（删除 vs 保留为扩展点；删除是默认建议）。

## FINDING RD3-DESIGN-D2-06（P3）[DEL]

- 位置：`x86_64/protection.rs:186`、`arm64/protection.rs:76`、`riscv64/protection.rs:109`
- 陈述：无文档陈述；三处实现的 `cpu_count` 字段注释均写"Number of CPUs initialized (for SMP tracking)"。
- 实现事实：三个实现只在 `init`/`init_ap` 里写 `cpu_count`（`cpu_id + 1`），全树无读者；x86 的 `init_sets_cpu_count` 测试读它的私有字段，两处 non-x86 测试直接构造 `{ cpu_count: 1 }` 再读回。没有任何 trait 方法或 OS 层路径消费它。
- 差异定性：P3 死状态。注释声称的"SMP tracking"功能不存在——SMP 计数在 `SmpState`/`topo.nr_cpus` 里，不在这里。
- 建议：删字段与写入（`AArch64Protection`/`Riscv64Protection` 退化为 ZST；x86 删除 `cpu_count` 与相关断言）；若保留，至少改注释说明"仅记录，未消费"，不能让读者以为 SMP 依靠它。
- 处置：**接受**。

## FINDING RD3-DESIGN-D2-07（P3）[DEL]

- 位置：`os/arch/src/arch/trap_entry.rs:143-147`；三架构实现 `x86_64/trap_entry.rs:329-331`、`arm64/trap_entry.rs:113-117`、`riscv64/trap_entry.rs:120-124`
- 陈述：§3.2 映射表把 `idt_reload` 同时映到 `TrapEntryArch::load` / `load_ap`，暗示 AP 侧有独立语义。
- 实现事实：三个实现的 `load_ap` 函数体都是 `self.load()`，一字不差；trait 文档自己说 x86 "the IDT is shared across CPUs, so this just reloads the IDTR"——与 `load` 完全相同。没有任何架构需要"AP 专用装载"。
- 差异定性：P3 冗余方法。额外的 trait 方法只增加 impl 负担与读者疑问。
- 建议：在 trait 里给默认实现 `fn load_ap(&self) { self.load() }`，删除三个 impl 的重复；或直接删除 `load_ap`，把 `smp.rs` 的三处 `trap.load_ap()` 改成 `trap.load()`（语义等价、调用点更诚实）。推荐前者（保留 AP 侧可被未来架构覆写的接缝，但不再有重复体）。
- 处置：**接受**。

## FINDING RD3-DESIGN-D2-08（P3）[DEL]

- 位置：`os/arch/src/arch/trap_entry.rs:149-161`；`arm64/trap_entry.rs:119-129`；`riscv64/trap_entry.rs:126-136`
- 陈述：§3.7 把"按需逐条 `set_handler`"列为被否掉的候选（:934），只保留"41 个向量一次写齐"。
- 实现事实：`set_handler` 仍留在 trait 上并被公开；唯一调用者是 boot 安装器 `install_idt_handlers`（x86），aarch64/riscv64 是文档化 no-op。没有任何驱动/运行期路径注册 handler——"动态 handler 注册"只存在于 trait 文档里。
- 差异定性：P3 过宽接口。一个只被 boot 调用一次的安装动作，被建模成三架构 trait 方法，其中两个实现是空的。
- 建议：把 `set_handler` 降为 `X86_64TrapEntry` 的 inherent 方法（安装器已经拿的是具体类型 `&mut X86_64TrapEntry`），trait 只保留 `init`/`configure_syscall`/`load`/`load_ap`；或保留但把两份 no-op impl 换成 trait 默认空实现 + 文档注明"仅为未来动态注册预留"。
- 处置：**需裁决**（收窄 API vs 保留扩展点；收窄是默认建议）。

## FINDING RD3-DESIGN-D2-09（P3）[MODERN]

- 位置：`os/arch/src/x86_64/trap_stub.rs:158-236`（`STUB_VECTORS`）、`:219-224`（`has_errcode`）、`:227-236`（`has_stub`）、`:255-317`（`global_asm!` 的 41 行 `TRAPSTUB n e`）、`:444-464`（`stub_symbols!` 的 41 个符号名）
- 陈述：§4.4 已诚实披露"布尔值只被宿主测试 `test_errcode_classification_matches_sdm` 使用；汇编入口由 `TRAPSTUB vec err` 字面量实例化，不读这个布尔值"。
- 实现事实：同一组事实有四处副本：向量集合（`STUB_VECTORS` 第一列 / `init` 的 41 次 `set_gate` / asm 的 41 行 / 符号清单的 41 个名字）与 errcode 标志（`STUB_VECTORS` 第二列 / `has_errcode` / asm 第二参数）。测试只把前两处与 `has_errcode` 对起来，asm 字面量只被两条字节级测试抽查了 3 个 stub。`has_errcode`/`has_stub` 零生产调用者。
- 差异定性：P3 多源重复。文件注释说"macro_rules cannot paste identifiers, so the full symbol names are listed here"——但在 `global_asm!` 里符号是字符串，`concat!(".globl x86_trap_stub_", stringify!($vec))` 可以生成，无需手写第三份清单。这是可消除的复杂度，不是硬件约束。
- 建议：一张声明表（向量, err）驱动：`global_asm!` 生成 `TRAPSTUB` 行、`stub_symbols!`/`stub_va` 由宏展开、`has_errcode` 由表推导；删除第二列与 `has_stub`（或保留 `has_stub` 作为表查询）。若本轮成本不允许，至少在 §4.4 记一条 TODO，避免"三处漂移"被读者当成已完成设计。
- 处置：**接受**（建议下轮实现；本轮可只登记）。

## FINDING RD3-DESIGN-D2-10（P2）[DEL]

- 位置与陈述（四处陈旧注释）：
  1. `os/arch/src/arch/protection.rs:91`：`ProtectionArch` 映射表写 `load()` = "lgdt, lldt, ltr, reload segment registers"。`lldt` 已随 LDT 淘汰，x86 实现的 `load_with_tss` 明确不执行 `lldt`（`x86_64/protection.rs:545-548`）。
  2. `x86_64/protection.rs:545-548`："The LDT descriptor is filled in the GDT for 32-bit compatibility but never loaded on 64-bit." Rust GDT 里没有 LDT 描述符：槽 3 是用户 CS 副本（anchor）、槽 5 是用户 CS，`GDT_ENTRIES` 从 6 起全是 TSS 两槽。
  3. `arm64/protection.rs:28`：模块文档称 `msr SP_EL1` 在 `SPSel=1` 下"architecturally UNDEFINED at `SPSel=1` and traps"；同文件 `fn init` 注释（:141-146）说真实崩溃来自替换活动栈指针后的函数尾声，**不是** UNDEFINED。两处互斥；§4.6 引用的是后者。
  4. `x86_64/trap_entry.rs:148-155`：`gate_present` 注释自称 "Load-safety half of the 'no empty vector' stage invariant"；`load()` 不调用它，生产路径零调用者（§3.7 已如实披露）。
- 实现事实：以上四处均为注释/文档串，无行为影响；但第 2 条把 Rust 的 GDT 布局说错，第 1 条把已删除的指令写进映射表，第 3 条在同一文件内自相矛盾。
- 差异定性：P2 注释残留（R4）。这类错误会被下一轮"引用对账"继续传播。
- 建议：删 `lldt` 字样或改为"lgdt, ltr, reload segment registers"；第 2 条改为"64-bit mode does not use LDT; no LDT descriptor exists in this GDT"；第 3 条统一为 `fn init` 的口径（UNDEFINED 说法删除）；`gate_present` 注释改为"test-visible accessor for the coverage cross-check；`load()` does not consult it"。
- 处置：**接受**。

## FINDING RD3-DESIGN-D2-11（P2）[DEL]（非法状态）

- 位置：`x86_64/protection.rs:374-383`、`:442-463`、`smp.rs:1160-1166`；`riscv64/protection.rs:152-160`
- 陈述：文档无对应陈述（§4.3 只描述 `setup_tss_for_cpu` 的字段写入）。
- 实现事实：
  1. `X86_64Protection::init` 对 `0..MAX_CPUS` 每个槽都调 `setup_tss_for_cpu(slot, kernel_stack_top)`——所有 AP 槽的 `sp0` 初始等于 BSP 的可用栈顶，`ist[0..2]` 也按各槽自己的 IST 栈填好。BSP 槽随后再写一次。AP 槽的正确值要等 `smp_init` 在 BOOT_LOCK 内调 `set_kernel_stack`；`init_ap` 用 `assert_eq!(actual_sp0, expected_sp0)` 与戳记校验兜底。占位值与"已正确赋值"在数据上不可区分，防线在消费端而非生产端。
  2. `riscv64::set_kernel_stack` 对 0 参数只有 `debug_assert_ne!`；release 构建（QEMU 生产镜像就是 release）静默 `csrw sscratch, 0`，第一次用户陷阱会把 `sp` 换为 0 再减帧，落到非规范地址。注释承诺的"loud stop instead of silently poisoning"只在 debug 成立。
- 差异定性：P2 非法状态。两点都不是当前必现缺陷（调用图目前正确），但错误注入后的表现分别是"延迟到 AP 启动才炸"与"静默毒化"，不符合项目"未初始化是确定失败点"的既定取向。
- 建议：① 非 BSP 槽初始化为 0 或引入 `sp0_valid: [bool; MAX_CPUS]`（零是自明非法值，且 `init_ap` 已有断言可立刻抓住漏调）；或在 §4.3 明说"全槽同值占位是有意为之，消费端断言是唯一防线"。② riscv64 `set_kernel_stack` 改 `assert_ne!`（boot 一次调用，成本可忽略），或删掉该 debug 断言并改注释承认 release 不设防。
- 处置：**需裁决**（涉及运行时断言强度与占位约定；最小改动是 ② 的 assert 与 ① 的文档一句）。

## FINDING RD3-DESIGN-D2-12（P3）[DEL]

- 位置：§4.1（:968）
- 陈述："诊断块留下的四个读数（`rdi`、全局槽指针、`rsp`、`bootstrap_len`）就是为此加的。"
- 实现事实：`kmain` 的诊断块只输出三项：`entry_rdi`、`kernel_info` 地址、`rsp`（`lib.rs:1696-1714`）。`bootstrap_len` 只出现在注释（1691）与 Phase F 的历史说明（1860）里，既没有读也没有打印；且 D01-F03 已把 `bootstrap_start/len` 从 `KernelInfo` 移除（`lib.rs:1858-1863`）。
- 差异定性：P3 事实不符（读者会以为有一个 `bootstrap_len` 读数可用于诊断）。
- 建议：改为"三个读数"并列出实际三项；若要保留 `bootstrap_len` 的取证记忆，改写为"该字段已随 D01-F03 移除"。
- 处置：**接受**。

## FINDING RD3-DESIGN-D2-13（P3）[DEL]

- 位置：§4.2（:975）
- 陈述：节选的"两处省略号分别为 `program_entry_base` 的 BSP 调用与 `register_current_arch` 的注册细节"。
- 实现事实：第一处省略号确实是 `program_entry_base` 的 BSP 调用块（`lib.rs:2249-2254`，含 topology 查询，代码量远超一个调用）；第二处省略号在源码里只覆盖说明注释——`trap_dispatch::register_current_arch();` 本身就是节选里保留的那一行，后面紧跟 `store_trap_entry`，没有"注册细节"被省。
- 差异定性：P3 引文说明不准确（不影响节选内容本身；两处省略号的位置与省略范围都是允许的）。
- 建议：改为"第一处省略号为 `program_entry_base` 的 BSP 调用（含 BSP logical id 解析）；第二处为说明注释"。
- 处置：**接受**。

## FINDING RD3-DESIGN-D2-14（P3）[DEL]

- 位置：`os/arch/src/arch/protection.rs:54-57`
- 陈述：注释 "Re-export InterruptVector from trap_entry module for backward compatibility... re-exported here because some protection code references it."
- 实现事实：`pub use crate::trap_entry::InterruptVector;`。全树没有通过 `protection::InterruptVector` 路径引用的外部/下游代码；`os/arch/src/lib.rs:103` 同时从 `protection` 再导出 `InterruptVector`，而 `trap_entry` 自身就是它的定义处。所谓 "protection code references it" 在当前源码里不成立（protection 模块内无 `InterruptVector` 使用）。
- 差异定性：P3 死兼容层。项目未发布 1.0，也没有外部消费者，"backward compatibility" 是没有对象的理由。
- 建议：删除该 `pub use` 与注释；若 `lib.rs` 的再导出需要保留，改为 `pub use trap_entry::InterruptVector;`（单一路径）。顺带删除 `crate::protection::InterruptVector` 这条二传路径。
- 处置：**接受**。

## FINDING RD3-DESIGN-D2-15（P3）[DEL]

- 位置：`os/arch/src/x86_64/protection.rs:192-203`
- 陈述：注释 "X86_64Protection is Send because... Sync because..."，随后两个 `unsafe impl`。
- 实现事实：结构体字段是 `[u64; GDT_ENTRIES]`、`[Tss64; MAX_CPUS]`、`u32`、`u32`；`Tss64` 是 `#[repr(C, packed)]` 的纯 POD。Rust 自动 trait 已经给出 `Send + Sync`，手写 impl 不改变任何 bound。放入 `static` 所需的 `Sync` 由 `globals::SyncUnsafeCell` 的 `unsafe impl<T: BklProtected + ?Sized> Sync` 提供（`globals.rs:56`），也不要求 `T: Sync`。因此这两个 `unsafe impl` 既冗余又扩大 unsafe 面（将来给结构体加 `Cell`/`UnsafeCell` 字段时会让编译器不再报 `!Sync`，掩盖真实风险）。
- 差异定性：P3 冗余 unsafe。项目有 unsafe 审计门（`tools/` 的 unsafe 自测），这类"无消费者的 unsafe"应在审计视野内。
- 建议：删除两个 `unsafe impl` 与对应 SAFETY 注释；若未来字段引入内部可变性，让编译器报错，再按需补有理由的 impl。
- 处置：**接受**。

## FINDING RD3-DESIGN-D2-16（P2）[ARCH]

- 位置：§3.3 三候选表与 E1 论证（:808-841）；`lib.rs:2230-2240`、`:2351-2375`；`x86_64/protection.rs:286-299`、`:386`；`arm64/protection.rs:154-162`；`riscv64/protection.rs:136-147`
- 陈述：§3.3 把"栈上构造 / `Box::leak` / 全局 `SyncUnsafeCell<Option<...>>` 装载前搬入"三候选比较后选中第三个，并把"搬移导致描述符基址失效"写成必须由 `refresh_descriptors` 修补的契约（E1）。
- 实现事实：第三个候选确实成立，但它是"先构造、再搬移、再修地址"的路径。设计空间里还有第四个候选未列出：**先在最终位置落位、再就地初始化**（`PROTECTION` 槽先放一个空白实例，然后 `init(&mut self, cpu_id, stack_top)` 在 static 地址上算 `&self.tss[idx]`）。走这条路，描述符从第一次写入起就指向最终地址，E1 描述的那类"搬移后基址失效"不可能发生，`refresh_descriptors` 整个方法（trait 默认 + x86 实现 + OS 层调用 + E1 注释）可以删除。现码的 `refresh_tss_descriptors` 已经证明"按最终地址重算两半"是确定性的地址计算（`protection.rs:286-299`），把它换成"在最终地址上首次计算"不需要新机制。
- 同时暴露的第二个面：non-x86 没有表，却被 trait 的 `init() -> Self` + store 模型迫使 `with_protection` 每次 `init(0, 0)` 造哑实例，并让两个架构各自维护 `base == 0` 哨兵（见 RD3-DESIGN-D2-11）。同一次契约修正可以一并消掉。
- 差异定性：P2 [ARCH]（设计空间不完整 + 自造二次修复 + 寄存器架构被迫模拟存储架构）。不是行为缺陷；是对 §3.3 选中理由的边界挑战，且与 D-001（四章教学骨架不是优化对象）不冲突——改的是实现契约，不是教学骨架。
- 证据：`sed -n '286,299p' os/arch/src/x86_64/protection.rs`（按最终地址重算描述符的纯函数）；`sed -n '359,386p'`（init 先按栈上地址建描述符，再由 refresh 重算）；`sed -n '2351,2375p' os/kernel/src/lib.rs`（non-x86 哑实例）。
- 处置：**需裁决**（见下 ARCH-PROPOSAL；按 D-047，未经判决账行不构成硬阻断）。

## ARCH-PROPOSAL

### A-1 保护状态的落位与初始化顺序（对应 RD3-DESIGN-D2-16）

**问题一句话**：现设计先在栈上构造 GDT/TSS 镜像，搬进全局后再用 `refresh_descriptors` 修正描述符里的自指地址；这一步修的是自己上一步制造的失效，而不是硬件要求。non-x86 没有镜像，却被迫用 `init(0, 0)` 哑实例走同一形状。

**提案（选项 A，推荐）**：把"构造"与"落位"合并为一次就地初始化。

1. `ProtectionArch` 增加一个不依赖最终地址的构造入口：`fn new_blank() -> Self;`（或 `Default`，但 `Tss64` 的 `iobase` 需要 0x8000，用 `new_blank` 更直白）。
2. 把现有 `fn init(cpu_id, kernel_stack_top) -> Self` 改为 `fn init(&mut self, cpu_id, kernel_stack_top)`——在实例已经位于最终地址之后填表。测试便捷构造变成调用方两行：`let mut p = CurrentProtection::new_blank(); p.init(0, top);`。
3. OS 层顺序变为：`store_protection(CurrentProtection::new_blank())` → `with_protection_mut(|p| p.init(0, kern_stack_top))` → `with_protection(|p| p.load())`。
4. 删除 `refresh_descriptors`（trait 默认方法、x86 实现、`lib.rs` 调用点、E1 注释）与 §3.3 对它的叙述；§3.3 的三候选表增加"就地初始化"一行，并把选中理由改为"地址从第一次写入起就是最终地址，无需搬移后修复"。

**收益**：删掉 `refresh_descriptors` 与 E1 整类"描述符基址过期"缺陷（注释记录的那次真实故障不再有发生条件）；`§3.3` 的"地址稳定"从"契约 + 修补"变成"构造即稳定"。trait 方法净增 0（`-refresh_descriptors +new_blank`，`init` 只是换接收者）。

**代价/风险**：`init` 的签名变化触及三个测试载体与 `lib.rs` 调用点；x86 实现要拆成"零值构造 + 填表"两段。若评审认为 `new_blank` 暴露了不该暴露的中间态，可保留 `init(...) -> Self` 并让 `init_in_place` 成为第二个方法（净增 1），代价是测试路径继续走"构造后搬移"。

**不做的影响**：现设计仍正确（`refresh_descriptors` 在 `load` 前执行），但读者会继续把 E1 当成硬件的必然，而它其实是所选候选的副作用。

### A-2 寄存器架构不应模拟存储架构（对应 RD3-DESIGN-D2-04/11）

**提案（选项 B，可与 A-1 独立）**：non-x86 的 `with_protection`/`with_protection_mut`/`with_trap_entry` 不再每次 `init(0, VirBytes::new(0))` 造哑实例，而是各架构提供一个静态/常量实例（`AArch64Protection`/`Riscv64Protection`/`AArch64TrapEntry`/`Riscv64TrapEntry` 都是无状态 ZST，删除 `cpu_count` 后可直接 `static`）。真实初始化仍由 `init_protection` 里的 `CurrentProtection::init(0, kernel_stack_top)` 承担（写 `TPIDR_EL1`/`sscratch`），返回值仅用于当次 `load`。

**收益**：删除 `arm64/protection.rs` 与 `riscv64/protection.rs` 的 `base == 0` 哨兵与两段长注释；`ProtectionArch::init` 恢复"每次启动只调一次"的文档契约；`with_*` 不再有副作用（现在每次调用都在写/尝试写寄存器状态）。

**代价**：每个架构分支多一个静态实例（4 个 ZST，零存储成本）；`store_protection` 的 non-x86 分支仍丢弃（这没问题，因为状态在寄存器）。

### A-3 不提案的部分

- 不挑战两 trait 拆分本身：`int_gate` 固定写 `KERN_CS_SELECTOR` 的依赖是真实的，§3.2 的论证成立。
- 不挑战 `install_trap_stubs` 作为 OS 层统一调用点（non-x86 no-op 是诚实的）。
- 不挑战 `X86_64_STACK_TOP_RESERVED = 16`（AP trampoline 常量与 C parity 同时依赖，收益为零）。
- 不挑战 `Privilege` 的保留决定（见 RD3-DESIGN-D2-05，交由用户裁决；若保留，本提案不依赖它）。

### 裁决请求

请就以下三项各给一行裁决（按 D-047，未获判决账行前本提案不构成硬阻断）：

1. A-1 就地初始化：采纳 / 拒绝（拒绝则 §3.3 补第四候选与拒绝理由）。
2. A-2 non-x86 静态实例：采纳 / 拒绝（拒绝则删除两处 `base == 0` 哨兵的注释承诺，或改 assert）。
3. RD3-DESIGN-D2-11 的断言强度：`riscv64 set_kernel_stack` 改 `assert_ne!` / 保持 `debug_assert_ne!`。

## VERDICT

**NOT PASS**（设计面板 D2，R1）

- 统计：16 条 FINDING——P0 2 条、P1 1 条、P2 5 条、P3 8 条；其中 [DEL] 14 条、[MODERN] 1 条、[ARCH] 1 条（RD3-DESIGN-D2-11 以 [DEL] 记非法状态，不另立标签）。
- 阻断项：RD3-DESIGN-D2-01（`KERNEL_MAY_ALLOC` 落点不存在）与 RD3-DESIGN-D2-02（`Maps 1024…` 引文不存在）是 P0-fact，必须修完才可进入 CONVERGED。两条都在文档侧，不涉及行为改动。
- 设计主干的判定：§3.1 的 64 位三表处置、§3.2 的两 trait 拆分理由、§3.4 的 SYSCALL 单路径、§3.5 的页表重建淘汰、§3.6 的里程碑拆分、§4.2–§4.6 的落地顺序，逐条与实现对得上（七维表 #4–#7、#20 为 PASS）；没有发现非法硬件语义或与 Minix3 真值源冲突的设计偏离。
- 需要在合并前处理的设计面：RD3-DESIGN-D2-03（init/install 的门元数据分工与实现不符、DPL 双源）与 RD3-DESIGN-D2-04（store/panic 契约未限定 x86）——两条都是"文档说得比代码整齐"，建议与 P0 同批修。
- 需用户裁决：RD3-DESIGN-D2-05（Privilege 死抽象删/留）、RD3-DESIGN-D2-08（`set_handler` 收窄/保留）、RD3-DESIGN-D2-11（非法状态防线强度）、RD3-DESIGN-D2-16（ARCH 提案 A-1/A-2）。
- 被本条审查拒绝的改动（记录以免下轮重复提出）：删除 `X86_64_STACK_TOP_RESERVED` 的第一个保留字（AP trampoline 常量与 C parity 双依赖）、删除 `install_trap_stubs` 的 non-x86 no-op（OS 层统一调用点有价值）、把 `BklProtected` 判为死抽象（`trait-audit.md` 的 NO-IMPL 是单行扫描漏计，宏实现存在）。
- 与冻结决策的关系：未与 D-001（四章教学骨架不优化）、D-032/D-033（BootWindows 语义）、D-040（CheckedWindows token 保留）、D-042（ranges 用 PhysRange）、D-045（引文纪律）冲突；D-001 只保护教学骨架，本轮的 P0/P1 都落在"陈述与活码对账"和实现契约上。D-047 的 [ARCH] 判决账行要求在 ARCH-PROPOSAL 里以"裁决请求"记录，未冒充已批准项。
- P3 的 8 条全部按 SUGGESTION 记录（不阻断、可与后续轮次合并处理）；P2 的 5 条里 2 条需裁决、3 条接受。
- 一致性口径：本轮为同 agent 只读复核，所有"不存在/零调用者"结论均附可重放 grep；未做跨工具二次复核，故不给出百分比型 consistency 结论，只给逐条证据。剩余风险集中在"文档引文与活码的逐字对账"（两条 P0 都属于这一类），建议下一轮把 §3.5/§4.1 的所有引号段落全部重放到活码。
