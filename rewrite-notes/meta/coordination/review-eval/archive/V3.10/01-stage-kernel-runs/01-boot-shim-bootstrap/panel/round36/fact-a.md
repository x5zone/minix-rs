# P-FACT-A 报告（Round 36）

范围：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §1–§3（第 17–2268 行）逐行核对；§4–§6 通读；测试名与测试计数对账覆盖全文。只读仓库，未修改任何源码/文档/轨迹。

## 机械门读数（先跑）

| 门 | 命令 | 读数 | 判定 |
|---|---|---|---|
| 锚点解析 | `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `anchors=108 resolved=108 zero-def=0 multi-def=0`，exit 0 | PASS |
| 引文对账 | `python3 tools/doc-citation-check.py <doc>` | `blocks=69 checked=29 failed=0 skipped=35 warned=5`，exit 0 | PASS（5 个 WARN 全是"声明为非逐字引文（structure-only）"，未做逐行对账；其中 2313–2402 正是 FACT-A-07 的 §4.2 块） |
| 文档-代码映射 | `tools/doc-code-map.sh <doc>` | 全部声明/引用路径存在 | PASS |
| 结构检查 | `python3 tools/doc-structure-check.py <doc>` | 表结构与内部 .md 链接均可解析 | PASS |
| 内部引用 | `AWK=gawk tools/doc-ref-check.sh <doc>` | 内部章节引用均可解析 | PASS |
| 文风 | `tools/doc-style-lint.sh <doc>` | error 级 0 | PASS |
| 承载门 | `tools/doc-code-density-check.sh <doc>` | 4 个可判章均有码/命令 | PASS |

工具判定项不重复手工推导；以下为工具不能判定的语义/计数对账。

## FINDING FACT-A-01（P0）
- 位置：§3.4.1 第 1625 行「`BOOT_HANDOFF_V1_VERSION` 9 → 10，收发两侧精确比较，v9 与 v10 互相拒收」；同文 §3.4.3 第 1713 行「（`BOOT_HANDOFF_V1_VERSION`，当前 9）」、第 1749 行「尺寸与信封版本 9 不变」、§4.6 第 2920 行「信封版本仍为 9（见 §3.4.3）」；§1.4 第 111–112 行「v10 / D-062 起不再是字段」
- 严重度：P0
- 问题：文档一处声称 wire 信封版本已升到 10、另一处写当前 9，而源码常量仍是 9；同一文件内自相矛盾，且与源码不符。
- 影响：读者会按"v10 与 v9 精确比较、互相拒收"去设计或核对收发两侧；实际两侧都写 9，版本比较无法把"v10 布局（128 B、无 startup 分区）"与"旧 v9 布局（160 B、含 startup 分区）"分开——旧镜像与新镜像都会通过 `version == 9`，然后按错位偏移解码。这是跨镜像 ABI 的静默错读风险，不是单纯数字笔误。
- 证据或推理：E1。`grep -n "BOOT_HANDOFF_V1_VERSION" os/libs/minix-boot/src/handoff_v1.rs` → `288:pub const BOOT_HANDOFF_V1_VERSION: u32 = 9;`；`git show 52110580f:os/libs/minix-boot/src/handoff_v1.rs | grep -n "BOOT_HANDOFF_V1_VERSION: u32"` 同样输出 `= 9`（该提交标题写 "版本 9→10"，但常量未改）。`BootHandoffV1` 当前 18 个字段、`size_of` 断言 `<= ENVELOPE_CAP`，结构已是删掉 `user_sp`/`kern_stack_top`/`syscall_entry`/`startup_byte_len` 后的形状；`grep -rn "WIRE_STARTUP_CAP" os/` 只剩 `handoff_v1_read.rs:763` 一条悬空注释。也就是说布局已按 v10 改、版本常量仍停在 9。
- 最小修复：二者只能留一个真相。若 D-062 的升版是本轮应落地项：改 `os/libs/minix-boot/src/handoff_v1.rs` 的常量 9→10（并补一条"旧版本被拒"的负控重跑）；若只允许改文档：把第 1625 行改为"版本常量当前仍为 9（D-062 的 9→10 未落地）"，同步删除"v9 与 v10 互相拒收"，并把 §1.4/§3.4.1/§3.4.3 的 "v10" 标注改为"v10 设计（常量待升）"。随后重跑 `anchor-resolve.sh`、`doc-citation-check.py`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能存在本仓外的补丁把常量改为 10，或 `BOOT_HANDOFF_V1_VERSION` 由构建脚本注入；但当前工作树与 HEAD 的源码都显示 9，且 `cargo test --list` 使用的就是这份源码。
- 对应判据：`prompt/review-rules/review-doc-checklist.md` §4「常量/版本号与源码一致」；Doc Code Sync 第 3 项（常量）；P0-fact（文档断言与源码冲突）。

## FINDING FACT-A-02（P1）
- 位置：§3.6 第 1872 行「`Paging` 本身已有 22 个成员（`const PAGE_SIZE` 与 21 个方法：…`adopt_active_root`…）」
- 严重度：P1
- 问题：成员数与成员名单都与现行 trait 不符。现行 `Paging` 是 24 个成员（1 个关联常量 + 23 个方法）；`adopt_active_root` 已并入 `from_active_root`，不存在；名单漏了 `is_supervisor_va`、`is_inherited`、`ensure_writable_range`。
- 影响：该句是"大页能力不塞进 `Paging`"这条设计论证的量化依据。读者按名单去找 `adopt_active_root` 会零命中，按"22 个成员"评估职责面会低估两个方法；用该名单做接口盘点会漏三项。
- 证据或推理：E1。`awk 'NR>=212 && NR<=625 && /^    (unsafe )?fn |^    const /' os/arch/src/arch/paging.rs | nl` 得 24 项（`PAGE_SIZE`、`phys_addr_limit`、`validate_virtual_range`、`new`、`new_from_page`、`from_active_root`、`inherit_supervisor_half`、`enable`、`destroy`、`is_supervisor_va`、`is_inherited`、`ensure_writable_range`、`map`、`remap`、`unmap`、`update_flags`、`query`、`split_huge`、`grant_user_walk`、`root_paddr`、`flush_tlb`、`flush_tlb_addr`、`map_range`、`unmap_range`）。`os/arch/src/arch/paging.rs` 第 193–195 行文档注释明写 "The VM-context handle (former `adopt_active_root`, merged into `from_active_root`)"。全仓 `grep -rn "fn adopt_active_root" os/` 零命中。
- 最小修复：把"22 个成员（`const PAGE_SIZE` 与 21 个方法…）"改为"24 个成员（`const PAGE_SIZE` 与 23 个方法）"，名单删 `adopt_active_root`、补 `is_supervisor_va`/`is_inherited`/`ensure_writable_range`；若不愿维护穷举名单，改为"20 余个成员，其中与本节直接相关的是 `new_from_page`/`from_active_root`/`map`/`unmap`/`query`/`enable`"。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能文中 `Paging` 指另一份同名的旧 trait 或某个 `#[cfg]` 变体；但全仓 `trait Paging` 只有 `os/arch/src/arch/paging.rs:212` 一处，且 `HugePages: Paging` 的锚点也指向这里。
- 对应判据：Doc Code Sync 第 5 项（符号锚点/成员枚举）；`review-doc-checklist.md` §4「函数/方法清单与源码一致」。

## FINDING FACT-A-03（P0）
- 位置（同一根因：D-062 的删除只回填了部分小节，以下站点仍按删除前的世界描述）：
  - startup 分区/落地区：§1.4 第 139 行（编号表"shim 九步 + 6b … EBS 后补写 startup 的 6b"）；§4.5 第 2837–2857 行（表行 `STARTUP_RECORD_STORE`、正文"七个落地区"、"startup 容量 16→20"）；§3.4 第 1585 行、§3.4.3 第 1749 行、§3.7 第 2059 行、§4.7 第 3027 行（"七个分区/七个载荷"）；§5.2.3 第 3297 行（"在 startup 写入与三道断言之后"）；§6.3 第 3453 行（"memmap/…/source/startup 七个分区"）
  - startup 补写动作：§4.2 第 2377、2383 行（"startup 分区写入段省略"、"startup 记录在 EBS 后由省略段直写 wire"）；§4.2.1 第 2408、2422–2424 行（"九个动作 + 一个 EBS 后补写（6b）"及整段时序推演）
  - startup 记录的所有权：§3.4.5 第 1761、1763 行（"startup 记录 20 项"、"五类 startup 记录…在返回前写齐"）、第 1764 行（"startup 走 push_startup 逐条追加"）
  - strict 门与后置条件：§4.4 第 2726 行（`check_boot_postconditions` 查 `syscall_entry`/`kern_stack_top`/用户栈）、第 2727 与 2746 行（`validate_requiring_sources` 要求 `startup_objects` 覆盖内核跨度、"核对三条派生标量"）、第 2737 行（"`boot_validate_and_prepare` + 共享校验体（三档都跑）" 查 `kern_stack_top` 16 字节对齐——共享校验体 `validate_impl_inner` 实际没有这条断言，只有 `boot_validate_and_prepare` 有）
  - 测试与覆盖：§5.1 M8 第 3157 行（"七枚超容量负控 … startup"）；§5.3 TODO-03 第 3317 行（"startup 21"）；§5.2.1 第 3253 行（"`test_postconditions_reject_*`（4 条）… = 31 条"）
- 严重度：P0
- 问题：文档在 §3.4.1/§3.4.3 已声明 v10 删除了 startup 分区、三个派生标量与 `startup_objects`，但 §4.2、§4.4、§4.5、§4.7、§5、§6.3 仍有至少十一处按"它们还活着"描述：容量表列出不存在的 `STARTUP_RECORD_STORE`、动作表保留 6b、strict 门仍说检查 `startup_objects` 与三条派生标量、后置条件仍说检查三个地址关系、覆盖账本仍列 startup 与 postconditions 负控。
- 影响：§4.5 自称全文交接容量的"唯一权威"，读者会据此认为内核仍有一个 20 槽 startup 落地区、wire 仍有第七个分区；§4.4 的校验门清单会让读者以为 `check_boot_postconditions` 还在防"syscall entry 越界/栈顶越界/用户栈越入内核半区"——这三条断言在 v10 已随数据源一起删除，现在只剩"`kern_virt_base` 在高半"一条；§5.1 M8/TODO-03/§5.2.1 把不存在的 `should_panic` 当成覆盖证据，覆盖账本失真。
- 证据或推理：E1。
  - `grep -rn "STARTUP_RECORD_STORE\|WIRE_STARTUP_CAP\|STARTUP_RECORD_STORE_LEN" os/ --include=*.rs` 仅命中 `os/libs/minix-boot/src/handoff_v1_read.rs:763` 一条悬空文档注释（其后无函数），`os/kernel/src/globals.rs` 无此静态量；`BootHandoffV1` 只有 6 个 `*_byte_len` 字段，解码函数也只有 6 个。
  - `grep -rn "push_startup\|startup_objects" os/ --include=*.rs` 零命中；`os/kernel/src/globals.rs` 无 startup store；`BootStorage` 的字段只有 memmap/reserved/modules/platform/param/memmap_sources 六族。
  - `grep -n "fn test_store_kernel_info_rejects" os/kernel/src/lib.rs` 得 6 条（reserved/memmap/boot_module/platform/param/source），加 1 条 landing 行为测试共 7 条；`cargo test -p minix-kernel --features mock --lib -- --list | grep "test_store_kernel_info"` 同样只有 7 条。
  - `os/libs/minix-boot/src/kernel_info.rs:548-566` 的注释写明 v10 删除了 `check_boot_postconditions` 原先的四条断言，函数体现在只剩 `assert!(self.kern_virt_base.0 > u64::from(u32::MAX), ...)`；`grep -rn "fn test_postconditions" os/` 零命中。
  - `os/libs/minix-boot/src/kernel_info.rs:488-506` 的 strict 分支用 `memmap`/`reserved_regions` 覆盖检查替换了 `startup_objects`；`validate_impl_inner` 的 strict 段不再比较 `user_sp`/`syscall_entry`/`kern_stack_top`（注释明说三处等式已删）。
- 最小修复：按同一份"v10 删除清单"逐处回填：删除 §4.5 的 `STARTUP_RECORD_STORE` 行与 "startup 容量 16→20" 段；把 §3.4/§3.4.3/§3.7/§4.7/§6.3 的"七个分区/七个载荷"统一改为"六个分区/六个载荷"；删除 §4.2.1 的 6b 与 §4.2 第 2377/2383 行的 startup 补写描述（或明确标为"v9 历史"）；把 §3.4.5 的 startup 记录与 `push_startup` 改为"v10 已删除"；把 §4.4 的 `check_boot_postconditions` 行改为"只查 `kern_virt_base` 在高半"、把 strict 门行改为"查 `memmap`/`reserved_regions` 覆盖内核跨度 + `MODULE_NAMES` 位置 + source 覆盖"，删掉 `startup_objects` 与三条派生标量；M8/TODO-03 改为六枚负控；§5.2.1 删除 `test_postconditions_reject_*` 并把 `kernel_info.rs` 计数改为 23（15 rejects + 3 accepts + 5 requiring_sources）。改后重跑 `doc-citation-check.py` 与 `anchor-resolve.sh`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 startup 存储/记录是通过宏或未纳入 `os/` 的 crate 生成；但全仓 `--include=*.rs` 检索零定义，`BootStorage` 字段与 `cargo test --list` 也没有对应项，反例不成立。`check_boot_postconditions` 的现状有源码注释与函数体双证。
- 对应判据：Doc Code Sync 第 3/5/7 项（常量、符号锚点、测试计数）；P0-fact（唯一权威表与源码冲突）；P0-test-missing（声称的 `test_store_kernel_info_rejects_startup_overflow`、`test_postconditions_reject_*` 不存在）。

## FINDING FACT-A-04（P1）
- 位置：§5.2.1 第 3184–3234 行（"同一天实跑""当前值以本逐文件表为准：minix-boot 195、boot-shim 136"）
- 严重度：P1
- 问题：逐文件测试计数与当前实测不符，且不是一两个文件的漂移：`boot-shim --lib` 声称 136、实测 146；`minix-boot` 声称 195、实测 184；`minix-elf` 声称 23、实测 27；`x86_64/paging.rs` 声称 17、实测（`--features "x86_64 runtime-window"`）20；`dm_coverage.rs` 声称 10、实测 11；分文件 `loader.rs` 22→31、`kernel_info.rs` 31→23、`memmap_source.rs` 74→75、`handoff_v1_read.rs` 21→17、`resv_claims.rs` 13→14。
- 影响：§5.2.1 用这些数字证明"覆盖现状"，并由 136 推导"默认 uefi-only 形态 125"、由 195 推导各文件加总；数字失真后这些推导和"与实测一致"的结论都失去依据。读者无法区分"测试丢了"与"文档没回填"。
- 证据或推理：E1。实测命令与读数：
  - `cd os && cargo test -p boot-shim --features test-all --lib -- --list` → `146 tests, 0 benchmarks`（loader 31、opensbi_helpers 11、uefi_helpers 12、ebs_map 45、ebs_pair 17、dtb_copy 15、resv_claims 14、lib 1）。
  - `cargo test -p minix-boot -- --list` → `184 tests`（kernel_info 23、platform 8、boot_params 8、memmap_source tests 72 + authority_filter_tests 3、handoff_v1 2、handoff_v1_write 10、handoff_v1_read 17、fdt_layout 29、lib 2、bump 5、boot_storage 5）。
  - `cargo test -p minix-elf --lib -- --list` → `27 tests`。
  - `cargo test -p minix-arch --features "x86_64 runtime-window" --lib -- --list | grep -c "paging::tests::"` → 20。
  - `cargo test -p minix-kernel --features mock --lib -- --list` 中 `dm_coverage::tests` 11 条；`boot_alloc` 10、`vm_handoff` 21 与文档一致。
  - 文档自己的计数法（`grep -c '#\[test\]'`）也给出同样方向：loader.rs 31、kernel_info.rs 23、handoff_v1_read.rs 17、resv_claims.rs 14、memmap_source.rs 75、dm_coverage.rs 11、x86_64/paging.rs 20。
- 最小修复：按上述命令重跑并整表替换数字；或者删掉易漂的逐文件精确数，改成"以 CI 的 `cargo test` 输出为准"并只保留稳定的文件名清单。若保留精确数，至少同步以下读数：boot-shim 136→146（默认 uefi-only 125→135、loader 22→31、resv_claims 13→14）、minix-boot 195→184（kernel_info 31→23、memmap_source 74→75、handoff_v1_read 21→17）、minix-elf 23→27、x86_64 的 `paging.rs` 17→20、`dm_coverage.rs` 10→11、`lib.rs` 的 store 系列 8→7。作为对照，`cargo test -p minix-kernel --features mock --lib` 仍是 866 passed + 8 ignored = 874，这一项文档正确，不要连同它一起改。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能文档的"实跑"发生在不同 feature 组合或不同 commit；但命令形态与文档给出的完全一致（`--features test-all --lib`、`-p minix-boot -p minix-elf`、`--features mock --lib`），且当前 `cargo test --list` 直接给出不同数字。
- 对应判据：`review-doc-checklist.md` §4「测试计数与实测一致」；Doc Code Sync 第 7 项；`prompt/review-rules/review-process.md` 的测试名对账门。

## FINDING FACT-A-05（P0）
- 位置：§5.2.1 第 3253 行「`kernel_info.rs:test_validate_rejects_*`（16 条）与 `test_validate_accepts_*`（3 条），另加 `test_validate_requiring_sources_*`（8 条）与 `test_postconditions_reject_*`（4 条）= 该文件 31 条」
- 严重度：P0
- 问题：该文件当前只有 23 条测试：`test_validate_rejects_*` 15 条、`test_validate_accepts_*` 3 条、`test_validate_requiring_sources_*` 5 条、`test_postconditions_reject_*` 0 条。`check_boot_postconditions` 仍是生产调用（`os/kernel/src/lib.rs:224/765/1860`），但没有任何宿主测试。
- 影响：这段是 `validate()`/strict 门/后置条件三条契约的覆盖账。把 4 条不存在的 postconditions 负控写成"已覆盖"，会让 review 与后续读者以为该契约有宿主负控；实际它只在启动路径上执行（失败即 panic），没有对应的宿主测试。
- 证据或推理：E1。`cargo test -p minix-boot -- --list | grep "kernel_info::"` 共 23 条，名单中无任何 `postconditions` 字样；`grep -rn "fn test_postconditions" os/` 零命中；`grep -rn "check_boot_postconditions" os/ --include=*.rs` 仅命中 `kernel_info.rs:554` 的定义与 `lib.rs` 三个调用点。`test_validate_rejects_*` 的 15 条与 `requiring_sources` 的 5 条可逐条列出。
- 最小修复：把该行改为实际 23 条（15 + 3 + 5），删除 `test_postconditions_reject_*（4 条）`；若该契约应有测试，补 4 条（或在 §5.1 M4/M5 的覆盖状态中如实写"后置条件无宿主测试"）。随后重跑计数。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 `test_postconditions_reject_*` 在另一个测试 crate（集成测试）里；但 `grep -rn "test_postconditions" os/` 全仓零命中，反例不成立。
- 对应判据：P0-test-missing（文档声称的测试不存在）；Doc Code Sync 第 7 项。

## FINDING FACT-A-06（P2）
- 位置：§3.4.2 第 1657 行「这 21 个是 C `kinfo_t` → Rust `KernelInfo` 的累计退场台账（C 有、Rust 不再保留的全部字段）」
- 严重度：P2
- 问题：`user_sp` 在 v10（D-062）已从 `KernelInfo` 删除，因此"C 有、Rust 不再保留"的字段是 22 个；21 行台账没有 `user_sp`，"全部字段"的表述不成立。
- 影响：这张表被定位为字段退场的完整台账；漏掉 `user_sp` 后，按表审计"每个 C 字段去哪了"会得到错误结论。`user_sp` 的去向只在 §3.4.1 的 v10 段与 §1.4 的栈顶表里出现，读者若以 §3.4.2 为台账会漏读。
- 证据或推理：E1。`minix3/minix/include/minix/param.h:15-54` 的 `struct kinfo` 共 26 个字段；§3.4.2 列的 21 个不含 `user_sp`，而 `os/libs/minix-boot/src/kernel_info.rs:22-155` 的 `KernelInfo` 没有 `user_sp`；`git log` 的 D-062 提交（52110580f）标题与 `handoff_v1_read.rs:69` 注释都写明 v10 删除 `user_sp`。
- 最小修复：把"全部字段"改为"21 个（`user_sp` 另见 §3.4.1 的 v10 删除段）"，或在表中补一行 `user_sp | 删除（v10 / D-062）：每腿都填 `USER_STACK_TOP`，接收侧改读常量 | minix_types::USER_STACK_TOP`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若把 `user_sp` 算作"改名/换载体"而非"删除"，但 `KernelInfo` 里确实没有对应字段，只能由常量推导，仍属"不再保留的字段"。
- 对应判据：`review-doc-checklist.md` §4「字段台账完整性」；Doc Code Sync 第 3/5 项。

## FINDING FACT-A-07（P1）
- 位置：§4.2 第 2358–2381 行的"代码引用"块（`storage.map_tables_mut()`、`storage.commit_map_tables(...)`）；§4.2.1 动作 7 第 2418 行（"提交…计数给 `BootStorage`（`commit_map_tables`）"）；§3.4.5 第 1761 行与第 1764 行（"`commit_map_tables` 一次提交""各自的 `set_*_len` 提交""`push_startup` 逐条追加"）
- 严重度：P1
- 问题：这些方法在当前源码里不存在。实际 API 是 `BootStorage::fill_map_tables(|memmap_slots, reserved_slots| { ... })`——配对循环在闭包**内部**，计数由闭包返回并在 `fill_*` 返回前提交；`fill_modules`/`fill_platform`/`fill_param`/`fill_memmap_sources` 同理。`map_tables_mut`、`commit_map_tables`、`set_*_len`、`push_startup` 全仓零命中。
- 影响：§4.2 的块被写作 `prepare_and_enter_kernel_image` 的"保留顺序与关键约束"引文，读者会按它去找两个不存在的方法，并误以为配对循环在 `fill_*` 之外、EBS 后还要单独 `commit_map_tables`。这与源码的真实顺序（循环在闭包内、闭包返回即提交）不同，也把 D-067 的核心不变量"写与提交是同一步"从实现描述里抹掉了。§3.4.5 自己的下一条 bullet 与 §4.2.1 动作 7 又互相矛盾（一个说 `fill_*`，一个说 `commit_map_tables`）。
- 证据或推理：E1。`grep -rn "commit_map_tables\|map_tables_mut\|set_modules_len\|set_platform_len\|set_param_len\|push_startup" os/ --include=*.rs` 全部零命中；`os/boot-shim/src/uefi_helpers.rs:1156-1218` 显示 `storage.fill_map_tables(|memmap_slots, reserved_slots| loop { ... break counts; })`，随后 `let classification = classification.expect(...)`，没有 `commit_map_tables`；`os/libs/minix-boot/src/boot_storage.rs:253-273` 的 `report()` 也只读 `fill_*` 提交后的长度。doc-citation-check 因块头含"改写/删节"而豁免了逐行对账，所以机械门没报。
- 最小修复：把 §4.2 的块改成真实的 `fill_map_tables` 结构（`let mut classification = None; storage.fill_map_tables(|memmap_slots, reserved_slots| loop { ... break counts; }); let classification = classification.expect(...)`），删掉 `map_tables_mut`/`commit_map_tables`；动作 7 改为"`fill_map_tables` 的返回值提交两表计数（写与提交同一步）"；§3.4.5 第 1761/1764 行改为 `fill_*`，删除 `set_*_len`/`push_startup` 字样。改后重跑 `doc-citation-check.py`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若这些方法是某个 trait 的默认方法、由 `use` 引入而未在 `os/` 文本中出现；但 Rust 方法调用须有定义，全仓 grep 零命中，且源码实际调用点写的是 `fill_map_tables`，反例不成立。
- 对应判据：Doc Code Sync 第 5 项（符号锚点）；`review-doc-checklist.md` §4「实现描述与源码 API 一致」；P0-fact 的"引文块外的方法名"子类（块因"改写"声明豁免，但语义仍是实现事实断言）。

## FINDING FACT-A-08（P1）
- 位置：§3.4.5 第 1764 行（"`BootStorage` 只承载 modules、platform、param、startup 四类；`memmap`、`reserved` 与 source 记录仍由 DTB 收集器的进程级静态量持有（`DTB_FREE`/`DTB_RESERVED`/`DTB_SOURCES`，自带一套一次性守卫）。因此直核腿在信任边界上有两个所有者，`BootStorage` 的 memmap/reserved/source 槽在直核腿留空（约 9 KiB 闲置容量）"）
- 严重度：P1
- 问题：当前直核腿已经把两张内存表与 source 记录写进 `BootStorage`：`os/kernel-image/src/bootface.rs:376` 与 `bootface_a64.rs:348` 调 `storage.fill_map_tables(...)`，`:384`/`:356` 调 `storage.fill_memmap_sources(...)`；源码注释明写 "I-33-甲 (D-086): commit the owned map tables into `storage` — the storage is the only owner"。`DTB_FREE`/`DTB_RESERVED`/`DTB_SOURCES` 已在 `memmap_source.rs:157` 的注释里记为删除项，全仓无定义。所谓"两个所有者/9 KiB 闲置/四类载荷"都不再成立。
- 影响：读者会按这条 bullet 认为直核腿仍有两个所有者、`BootStorage` 的 memmap/reserved/source 槽在直核腿留空，并据此评估信任边界与迁移状态；这与 D-086 已落地的"单一所有者"相反。紧接着的"若将来把这三类也搬进 storage"也成了已完成事项的待办。
- 证据或推理：E1。`grep -rn "DTB_FREE\|DTB_RESERVED\|DTB_SOURCES" os/ --include=*.rs` 只命中 `memmap_source.rs` 的历史注释；`grep -n "storage.fill_map_tables\|storage.fill_memmap_sources" os/kernel-image/src/bootface*.rs` 命中两个直核面；`BootStorage` 字段（`boot_storage.rs:86-104`）包含 memmap/reserved/modules/platform/param/memmap_sources 六族。
- 最小修复：把该 bullet 改写为"直核腿也经 `fill_map_tables`/`fill_memmap_sources` 把两表与 source 记录写进 `BootStorage`；`BootStorage` 是六类载荷的唯一所有者，DTB 收集器的进程级静态量已随 I-33-甲/乙（D-086）删除"，并删除"约 9 KiB 闲置容量"与"两个所有者"的表述。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 bootface 的 `fill_map_tables` 只是把旧静态量的指针再拷进去而不是转移所有权；但代码是 `copy_from_slice` 到 storage 槽，且注释明写 storage 是唯一所有者，反例不成立。
- 对应判据：Doc Code Sync 第 5 项；`review-doc-checklist.md` §4「所有权/实现状态与源码一致」；与 §3.4.1 的"三条 v1 发送腿写 BootStorage"描述一致性。

## FINDING FACT-A-09（P1）
- 位置：§5.1 M13 第 3162 行（"`build_memmaps` 生产不调用、其单表包装被 15 个测试内核调用"）；§5.2.1 第 3256 行（"它的单表包装 `build_memmap()` 被 15 个 QEMU 测试内核各自调用"）
- 严重度：P1
- 问题：当前测试内核里 `build_memmap()` 的实际调用点是 2 个（`test-memmap` 与 `test-memmap-aarch64`），不是 15 个；其余提到 `build_memmap` 的文件（共 14 个文件）绝大多数只是注释文字，没有调用。
- 影响：`build_memmap` 的宿主不可构造性论证依赖"它被大量测试内核调用、生产不调用"这一事实；把 2 写成 15 会高估该 helper 的测试覆盖面，也让"15 个测试内核"这类数字无法复核。
- 证据或推理：E1。`grep -rn "build_memmap()" os/qemu-tests/test-kernels` 只有 2 行调用（test-memmap:165、test-memmap-aarch64:29）；`grep -rln "build_memmap" os/qemu-tests/test-kernels | wc -l` 为 14，其中 12 个文件只在注释里出现。`build_memmaps()`（复数）在测试内核里也零调用。
- 最小修复：两处都改为"2 个测试内核（`test-memmap`、`test-memmap-aarch64`）调用 `build_memmap()`"；若想表达"15 个测试内核都会经过 UEFI 辅助物"，请改成实际的口径（例如"14 个文件引用该名字，其中 2 处是调用"），不要把文件数当调用数。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能"15 个测试内核"指某个构建清单里同时链接 `uefi_helpers` 的内核数；但句子明确说"调用"，且 `build_memmap()` 的调用点只有 2 处，按字面读是错的。
- 对应判据：`review-doc-checklist.md` §4「测试覆盖计数与实测一致」；Doc Code Sync 第 7 项。

## FINDING FACT-A-10（P1）
- 位置：§4.8 第 3094 行表格 aarch64 直核行（"无入参（`x0=0` 分道，DTB 从 RAM 基址固定位置读）"）
- 严重度：P1
- 问题：D-061 已把 aarch64 的形态判别从"运行期 `x0 == 0`"改成编译期特性分叉。直核产物（`fw-aarch64-none`）的 `rust_image_main(_arena_pa)` 忽略入参、直接调 `bootface_a64()`；只有 UEFI 接收产物（`fw-aarch64-uefi-recv`）才在 `arena_pa == 0` 时诚实停机。表中"x0=0 分道"描述的是已被删除的旧形态，且与同节第 3063 行"x0==0 只剩『门面没交接』这一种含义…不再被解释成该走直核自举"直接冲突。
- 影响：读者会以为直核镜像仍在运行期按 `x0` 分道，可能照此恢复 D-061 明令禁止的运行期判别；也会误读"无入参"与"x0=0 分道"同时成立的自相矛盾。
- 证据或推理：E1。`os/kernel-image/src/main.rs:3039-3060` 两个 cfg 臂：`fw-aarch64-uefi-recv` 分支有 `if arena_pa == 0 { ... halt }`，`not(feature = "fw-aarch64-uefi-recv")` 分支是 `fn rust_image_main(_arena_pa: usize) -> ! { ... bootface_a64::bootface_a64() }`；§3.1.1 第 1454 行也写明"形态由构建产物决定（D-061）"。
- 最小修复：把该格改为"无入参（直核产物只编 `bootface_a64` 臂，忽略 `x0`；UEFI-recv 产物才在 `x0 == 0` 时诚实停机），DTB 从 RAM 基址固定位置读"。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 D-061 只改了 UEFI 盘面而直核臂仍保留 x0 判别；但 `main.rs` 的直核臂参数名为 `_arena_pa` 且直接调 bootface，源码无判别，反例不成立。
- 对应判据：Doc Code Sync 第 5 项；与 §3.1.1/§4.8 同节描述的一致性；`review-doc-checklist.md` §4「实现状态与源码一致」。

## FINDING FACT-A-11（P2）
- 位置：§4.1 第 2303 行（"`os/kernel/src/vm_handoff.rs:605`"）；§4.7 第 3021 行（"`os/kernel/src/lib.rs:741`"）
- 严重度：P2
- 问题：两处手写行号都已漂移：`kernel_allocated_dynamic: boot_alloc_used_bytes()` 在 `vm_handoff.rs:663`（605 行是模块表填充）；`resume_after_v1_decode` 的定义在 `lib.rs:773/781/800`（741 行在接收侧解码段内）。此外，项目规则禁止手工维护行号锚点（只允许工具派生的 `（Lnnn，工具生成）`），这两处是手写行号。
- 影响：读者按行号跳转落到无关代码；行号还会随源码继续漂移。它也让"锚点纪律门"看起来通过了（anchor-resolve 不检查 `path:line` 形式），留下机械门覆盖不到的盲区。
- 证据或推理：E1。`grep -n "kernel_allocated_dynamic" os/kernel/src/vm_handoff.rs` → 663；`grep -n "fn resume_after_v1_decode" os/kernel/src/lib.rs` → 773/781/800；`sed -n '741p'` 落在 `decode_*` 调用序列中。
- 最小修复：把 `vm_handoff.rs:605` 改为 `vm_handoff.rs:fn build_vm_handoff`（或 `:fn boot_alloc_used_bytes` 的调用点符号锚），把 `lib.rs:741` 改为 `lib.rs:fn resume_after_v1_decode`；不要保留手写行号。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若行号是生成工具留下的派生后缀；但文本是 `os/kernel/src/...:741` 形式，不是 `（L741，工具生成）`，且 741 行内容与描述不符。
- 对应判据：`prompt/review-rules/review-process.md` §Step 1.0 锚点约定（行号只允许工具派生）；`anchor-resolve.sh` 的 `path:fn` 纪律。

## FINDING FACT-A-12（P2）
- 位置：§3.2 第 1528 行（"另有一组 `qemu-test-kernels` gate（8 处）在 `uefi_helpers.rs`，只门控测试内核辅助物（`UefiPoolAllocator`、`TestMapTables`、`build_memmaps`/`build_memmap`、`exit_boot_services`）"）
- 严重度：P2
- 问题：`uefi_helpers.rs` 里 `#[cfg(feature = "qemu-test-kernels")]` 实际有 10 处。文档列的五个条目（`UefiPoolAllocator` 的 struct+impl 2 处、`TestMapTables` 的 struct+impl+static 3 处、`build_memmaps` 1 处、`build_memmap` 1 处、`exit_boot_services` 1 处）正好是 8 处，所以"8"与名单自洽；问题是名单漏了另外两个同样被门控的测试辅助函数：`classify_map_bytes_into_test_tables`（500 行）与 `exit_boot_services_and_classify`（1302 行，公开的完整分类辅助函数）。
- 影响：这段是"生产构建不链接测试辅助物"的审计依据；少列两个 helper、少报两处 gate，会让按名单做的生产面审计漏项。
- 证据或推理：E1。`grep -n 'cfg(feature = "qemu-test-kernels")' os/boot-shim/src/uefi_helpers.rs` 得 10 行（157/160/403/415/418/427/500/573/1272/1302）；1302 行是 `pub fn exit_boot_services_and_classify()`，500 行是 `classify_map_bytes_into_test_tables`。
- 最小修复：把"8 处"改为"10 处"，名单补 `classify_map_bytes_into_test_tables` 与 `exit_boot_services_and_classify`；或明确写"下列五类共 8 处 gate，另有 2 处分类辅助函数"，避免读者按名单做生产面审计时漏项。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若其中两处是同一 item 的重复属性或 `not(feature)` 反向门；实际 10 行都是正向 `#[cfg(feature = "qemu-test-kernels")]`，且分别挂在 10 个不同 item 上。
- 对应判据：Doc Code Sync 第 3 项（常量/计数）；`review-doc-checklist.md` §4「feature 门控清单与源码一致」。

## FINDING FACT-A-13（P3）
- 位置：§4.5 第 2756 行注释头（"五个切片落地区收敛到 `land_slice`；保留 helper、五处调用…"）
- 严重度：P3
- 问题：`store_kernel_info` 里 `land_slice` 的实际调用是 4 处（`reserved_regions`、`memmap`、`platform_sources`、`memmap_sources`）；模块名与 `param_buf` 仍是内联代码，不存在"五处调用"。
- 影响：读注释头的人会预期 5 处 `land_slice`，按 5 处核对会以为漏了一处；影响小，但属于引文注释里的事实错误。
- 证据或推理：E1。`grep -n "land_slice(" os/kernel/src/lib.rs` → 1601、1613、1660、1681 四处。
- 最小修复："五处调用"改为"四处调用"。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若把 `land_slice` 的 helper 定义本身算作"一处"；但"调用"一词不含定义，且上下文说的是"五处调用"。
- 对应判据：`review-doc-checklist.md` §4「引文注释与源码一致」。

## FINDING FACT-A-14（P1）
- 位置：§5.2.3 第 3291 行（"内存快照之后经 `AllocatePages(MaxAddress(4 GiB − 4 KiB), LOADER_DATA)` 分配"）与第 3293 行第二层（"`test-kernel-map` 里的 `in_snapshot_free` 读的是该测试内核自己 `build_memmap()` 产出的 conventional-only 投影（在探针分配之前取得，且它没有生产侧的 EBS 配对/分类循环）"）
- 严重度：P1
- 问题：`test-kernel-map` 当前的实际顺序是：先 `alloc_root_page()`/`alloc_bump_region(64)`，再分配并写入探针页，然后调用 `uefi_helpers::exit_boot_services_and_classify()` 取得**最终快照并分类**，`in_snapshot_free` 是在 `kernel_info.memmap`（分类结果）上算的。因此：①探针页是在快照**之前**分配的（不是"快照之后"）；②`in_snapshot_free` 的来源不是 `build_memmap()` 的 conventional-only 投影，而是 `exit_boot_services_and_classify()`；③这个测试内核**有**生产侧同形的 EBS 配对/分类（由 `uefi` crate 的 `exit_boot_services` 完成 GetMemoryMap+ExitBootServices 配对，再用返回 map 分类）。
- 影响：§5.2.3 自称是深拷探针"判别力"的权威记录。它关于第二层的整个论证（"探针没有检验候选②，因为快照取在探针分配之前、且没有配对循环"）建立在过时的测试内核形态上；按现行代码，`in_snapshot_free=0` 是最终快照确实把探针页归为 LOADER_DATA 的结果，而不是"构造上不在投影里"。这会改变读者对"候选②是否被检验"的判断，也会让后续修复者按错误前提去改探针。
- 证据或推理：E1。`os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map/src/main.rs`：探针 `allocate_pages` 与 `probe_pre` 在 `exit_boot_services_and_classify()` 之前（第 76-98 行附近）；随后 `exit_boot_services_and_classify()`（第 104 行附近）返回 `memmap`；`in_free` 在 `kernel_info.memmap` 上计算（第 142-148 行）；`os/boot-shim/src/uefi_helpers.rs:1303-1321` 的 `exit_boot_services_and_classify` 调 `boot::exit_boot_services(MemoryType::LOADER_DATA)` 并对返回的 `owned` map 做分类。`grep -rn "build_memmap()" os/qemu-tests/test-kernels` 只在 `test-memmap`/`test-memmap-aarch64` 两处调用，`test-kernel-map` 没有调用。
- 最小修复：按现行代码重写这两段：①"探针页在最终 `GetMemoryMap` 之前分配"；②说明 `in_snapshot_free` 读的是 `exit_boot_services_and_classify()` 的最终分类表；③把第二层的"未检验候选②"理由改为与该形态一致的理由（例如"探针页作为 LOADER_DATA 在最终快照里本来就会被归入保留类型，因此 `in_snapshot_free=0` 只证明分类把它列为非 conventional，不证明历史快照/复用清零的时序"），并给出可复跑的探针命令。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 `test-kernel-map` 在探针之前还调用过一次 `build_memmap()` 并把结果留给 `in_snapshot_free`；但源码只在 `test-memmap`/`test-memmap-aarch64` 调用 `build_memmap()`，`test-kernel-map` 的 `memmap` 变量来自 `exit_boot_services_and_classify()`。
- 对应判据：`review-doc-checklist.md` §4「测试描述与测试代码双向对账」；Doc Code Sync 第 7 项；§5.2.3 自称的"判别力权威记录"。

## FINDING FACT-A-15（P1）
- 位置：§4.5 第 2855 行（"`store_kernel_info` 有三个调用点——`arch_boot_finish` 传原始 handoff（真正拷贝一次）；`arch_boot_resume_v1` 先把七个分区直接解码进这七个 store，随后 `resume_additive` 的 store 源就是 store 本身（同址，跳过）；`kmain` 拿到的是全局副本、再存一次也是同址。三个生产调用点现在都使用返回值：`kmain` 用 `let kernel_info = store_kernel_info(kernel_info);` 继续读同一槽。"）
- 严重度：P1
- 问题：`kmain` 已经不调用 `store_kernel_info`。D-071 / I-35(a) 明确删掉了这次重复存储：生产 `kmain` 的形参名是 `_kernel_info`，函数体只读全局 `.bss` 副本；源码注释写明 "I-35(a) (D-071): no re-store here … a second store would rewrite a live &'static slot (now a guard failure)"。实际生产调用点只有两处：`arch_boot_finish`（真拷贝一次）与 `resume_additive`（同址跳过）。"三个生产调用点都使用返回值"与 `land_slice` 的第三个同址场景都不存在。
- 影响：读者会按文档去找 `kmain` 里的 `let kernel_info = store_kernel_info(kernel_info);`，并以为 `store_kernel_info` 可以在 `kmain` 再调一次；按现行代码这样做会触发 one-shot 断言 panic。`land_slice` 的"同址跳过"论证也被写成三段，实际只有 `resume_additive` 一段（`arch_boot_finish` 是真拷贝，测试里另有同址场景）。
- 证据或推理：E1。`grep -rn "store_kernel_info(" os/kernel/src/lib.rs` 只有 225（`arch_boot_finish`）、347（`resume_additive`）与测试模块中的调用，没有 `kmain`；`sed -n '1806p'` 是 `pub fn kmain(_kernel_info: &KernelInfo) -> !`，`lib.rs:1869-1877` 的注释与代码明写"no re-store here"。
- 最小修复：把该句改为"`store_kernel_info` 有两个生产调用点：`arch_boot_finish` 真拷贝一次；`resume_additive` 的源已是 store（同址跳过）。`kmain` 不再存储（D-071 / I-35(a)），只读全局 `.bss` 副本；第二次存储会触发 one-shot 断言。"并删除 `kmain` 的 `let kernel_info = store_kernel_info(kernel_info);` 引文。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 rc 版本里 `kmain` 仍调 `store_kernel_info`，而当前源码是 D-071 之后的新版；doc 应以当前源码为准，且源码注释已直接说明删除理由。
- 对应判据：Doc Code Sync 第 5 项；`review-doc-checklist.md` §4「实现描述与源码一致」；与 §4.5 引文块（它确实没有 `kmain` 调用）一致性。

## SUGGESTION

1. **行号锚点纪律（流程项，交 process reviewer）**：全文有 33 处手写 `file:line` 形式（其中 31 处与当前源码上下文一致，2 处已漂移，见 FACT-A-11）。`prompt/review-rules/review-process.md` 的锚点约定要求行号只作为工具派生后缀 `（Lnnn，工具生成）`。建议把这 33 处转成 `path:fn/struct/const` 符号锚，或由生成工具统一加派生后缀。影响：`anchor-resolve.sh` 只认符号锚，`path:line` 不在门禁覆盖内，后续还会继续漂移。对应判据：review-process Step 1.0。
2. **引文块的"改写"豁免口径**：§4.2 与 §4.5 的代码块头用了"改写/删节"，`doc-citation-check.py` 因此整块跳过逐行对账，`commit_map_tables`/`map_tables_mut` 这类改名/幽灵方法没有被机械门拦住（FACT-A-07）。建议把"逐字引文"（只允许省略号）与"示意/改写"（显式声明、不声称是源码）拆成两种标注，或在引文头加 `source-symbol:` 让工具先核对符号存在；这样 ghost API 至少能被符号门抓到。
3. **§2.4.2 第 778 行的 VM 回收措辞**：`minix3/minix/servers/vm/main.c:511-520` 的 `exec_bootproc`/`free_mem` 循环对 VM 自身有 `if(ip->proc_nr == VM_PROC_NR) continue;`，VM 自己的 blob 是内核侧 `arch_boot_proc` 的 `add_memmap` 归还的，不是 VM 的 `free_mem`。建议把"随后 `exec_bootproc` 把模块内容…再用 `free_mem`…"限定为"其余 11 个模块"，避免读者以为 VM 也走这条回收路。这是措辞精度问题，不单列为 FINDING（证据：源码条件分支）。
4. **"v10" 术语双关**：第 1625 行同时用 "v10" 指 D-062 设计批次和 `BOOT_HANDOFF_V1_VERSION` 的值，而后者实际仍是 9。修 FACT-A-01 时建议把术语拆开：一次裁决写 "D-062（设计批次）"，一个 wire 计数器写 "`BOOT_HANDOFF_V1_VERSION` = 9"，避免同一句里出现两个"版本"。

## 阴性声明（扫了什么、发现数、N=0 的边界）

- **锚点与路径**：`anchor-resolve.sh --check` 扫描全部符号锚，读数 `anchors=108 resolved=108 zero-def=0 multi-def=0`；40 个直写 `os/`/`minix3/` 路径逐个 `test -e`，0 缺失；14 个花括号/通配批量路径（`doc_code_map` 与 §7.2/§7.3 清单）逐项展开后也 0 缺失；33 个手写 `file:line` 逐个核对上下文，31 个一致、2 个漂移（FACT-A-11）。N=0 不覆盖：`anchor-resolve` 不检查纯 backtick 符号、`path:line`、宏生成项；`doc-code-map` 只核对声明字段/正文引用路径存在，不核对语义。
- **Doc Code Sync 七项**：路径（0 缺失）、常量（约 40 个逐值核对，发现 wire 版本/startup 容量/Paging 成员数/feature 门控数等不符）、符号锚（0 unresolved）、workspace 前缀（全部 `os/`/`minix3/` 前缀正确）、内部 TODO（§5.3 十项逐项对源码/测试，发现 TODO-03 的 startup 项与 M8 负控数不符）、静态可变面（BootAlloc/PT_ALLOC/BootStorageCell 的原子/UnsafeCell 描述与源码一致，未发现把 `static mut` 说成安全的新问题）、测试计数（见 FACT-A-04/05）。N=0 不覆盖：没有构建 C 内核、没有运行 QEMU 门链，行为级断言（panic 输出、路标、探针读数）只与源码/脚本对照，未重放。
- **测试名与计数**：实测 `cargo test -p minix-boot -- --list` = 184；`cargo test -p boot-shim --features test-all --lib -- --list` = 146，default `--lib` = 135；`cargo test -p minix-elf --lib -- --list` = 27；`cargo test -p minix-kernel --features mock --lib` = 866 passed + 8 ignored = 874；`cargo test -p minix-arch --features "x86_64 runtime-window" --lib -- --list | grep -c 'paging::tests::'` = 20。文档声称的 195/136/23/17/10/22/31/74/21/13 与此不符；另外发现 `test_store_kernel_info_rejects_startup_overflow` 与 `test_postconditions_reject_*` 共 5 个声称的测试不存在。N=0 不覆盖：`--list` 只列编译进来的测试，feature/目标组合不同可能改变数量；没有逐条运行 874 条（只全量跑了 minix-kernel mock、boot-shim test-all、minix-boot、minix-elf）。
- **引文块**：`doc-citation-check.py` 读数 `blocks=69 checked=29 failed=0 skipped=35 warned=5`；5 个 WARN 全部是"声明为非逐字引文（structure-only）"（块 2106-2148、2152-2173、2313-2402、2593-2628、2939-2962），不是无标注块；其中 2313–2402 就是 FACT-A-07 的 §4.2 块。N=0 不覆盖：工具跳过纯注释、含中文行、省略号行与单符号行，不做语义等价/API 存在性检查；声明为"改写/示意"的块整块不逐行对账。
- **常量、签名、行为合同**：核对了 `BootHandoffV1` 18 字段、`KernelInfo` 10 字段、`KinfoStruct` 48 B 布局、`PlatformDescSource` 16 B、`enter_kernel_image_v1`/`prepare_kernel_image_v1`/`jump_kernel_image_v1`/`arch_boot_finish`/`build_kernel_info`/`prepare_boot` 的签名与返回类型、`pt_alloc`/`BootAlloc`/`BootStorage`/`BootStorageCell` 的注册与所有权；未发现签名层面的新错（错误集中在 D-062/D-067/D-071/D-086 的实现描述，见 FACT-A-03/07/08/15）。N=0 不覆盖：外部规范（Multiboot 0.6.96、UEFI 2.10、ARM/RISC-V 手册）无离线副本，按文档声明接受；未生成反汇编验证 SysV 栈对齐结论。
- **C 覆盖**：把 §1–§3 正文点名的 C 函数/宏/常量逐一对到 `minix3/` 源码；未发现"文档声称已解释但正文没有任何说明"的 C 函数。N=0 不覆盖：这不是 Minix3 全调用图穷举，只覆盖本篇正文点名的符号；`arch_init`、`do_readbios` 等只按引用核对，不做行为分析。
- **反事实前提**：核对了 11 处反事实（§2.2 push 对调、§2.4 `alloc_lowest` 参数错、§2.5 去 `-1`、§2.6 反排、§2.6.3 返回最后已用槽、§3.5 `bootstrap_len` 误设、§3.6 去 `AlreadyMapped`、§3.7 重建根、§3.9 借槽、§4.7 按已用扣除、§5.2.2 去 L5）；前提均能在源码中找到对应分支/注释，未发现虚构前提。N=0 不覆盖：历史险情（`bootstrap_len` 误设、`res=127 zeroed=127`、3/6 路标 panic）与真机探针读数是观测记录，本轮无法复现，只核对记录与现有源码/测试是否一致。
- **同 agent 局限**：本报告由同一 agent 在同一 session 完成，所有关键读数都已重放命令并抄录输出；没有第二个独立 agent 交叉复核。按项目约定这属于"同 agent 验证"：我自评 consistency 高，但不满足跨 agent 独立回读；若面板要求独立回执，建议把 FACT-A-01（wire 版本）、FACT-A-03（D-062 残留）、FACT-A-14（探针描述）交给另一名子 agent 重放命令。

## 完成声明

- **FINDING：15 条**。P0 3 条（FACT-A-01/03/05）；P1 8 条（FACT-A-02/04/07/08/09/10/14/15）；P2 3 条（FACT-A-06/11/12）；P3 1 条（FACT-A-13）。
- **SUGGESTION：4 条**。
- 机械门：anchor-resolve PASS（108/108）、doc-citation-check PASS（0 failed）、doc-code-map PASS、doc-structure-check PASS、doc-ref-check PASS、doc-style-lint PASS（error 0）、doc-code-density PASS。
- 所有 FINDING 均有位置、影响、E1 证据、最小修复、置信度与对应判据；未修改任何源码、文档或轨迹文件，唯一写入是本报告。
