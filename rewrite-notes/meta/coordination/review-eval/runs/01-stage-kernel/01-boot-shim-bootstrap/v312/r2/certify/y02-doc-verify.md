# y02 R2 文档独立复验（D01 R2）

VERDICT: **PASS（复检 `fcc992c6` 后；初检 `34bee52f` 为 FAIL，原因与逐条证据保留在下文）**

- 时间：2026-11-23（本机 CST）
- 目标文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 文档 sha256：初检 `34bee52fc837f16f4f9685fa2cc3d34ee311fad66ceaad70dfa2b6527aced55c`（2,951 行；与任务给定基线一致）；复检 `fcc992c685e841ae0441fa0280afa2f7c2a716f69b34cd9fb4334cae6fe155d7`（2,951 行，见文末复检节）
- 基线 commit：`76ac760b4`；修复 commit：`11a33354c`
- 台账：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r2/defect-ledger.md`（25 行：① 23、⑤ 2）
- 面板：同 run `panel/round2/`（6 份报告，INDEX 记录 25 FINDING）
- 方法：只读文档、台账、面板与源码；独立重跑台账的每条 `复验` 命令与 8 个文档门；未修改目标文档或任何代码。唯一写入是本报告。复验前后文档 sha 不变。
- 协调层说明（2026-11-23）：R2 台账 `open=3`（DESIGN-01/02 的 ⑤ 行）是预期状态；**初检的 FAIL 不以 open 数或“⑤ 仍 open”为依据**，两个 `⑤` 行本身判 PASS（保持 open 且指向案件卷）。初检 FAIL 的依据只有下文 N-01、N-02、N-03 三处：R2-13 的新段落内矛盾、R2-23 的新未用导入警告、R2-23/R2-24 的 `复验` 命令不成立；这三处在复检 `fcc992c6` 上已闭合，见文末。
- 工作区并发写入说明：复验期间出现未跟踪文件 `…/v312/r2/R2-COLD-READ-CASE.md`（非本 verifier 创建）；本报告不依赖它。冷读 NOT-READY 由另案记录，不在本报告射程。

## 总判定依据

**文档内容侧的 8 个机械门全部通过**（读数见“机械门”节），**21/23 条 `①` 行按其处置落地并可由源码复核**，但按任务的验收口径不能给 PASS：

1. **R2-23 与 R2-24 的 `复验` 命令不成立**：两条命令都写“期望 0”，实际分别返回 1 与 2 处命中。按 `review-evals.md` §4.0d 的“100% 复验（旧串全篇 grep 归零 / 测试名+输出 / 源码逐字对照 / 冷读回执，不抽样）”，台账不得在这两条上写“已修”。其中 R2-24 的残余是两条**准确描述现状**的注释（不是误导性残留），属于台账命令口径错误；R2-23 还新引入一条 `unused import` 警告。
2. **R2-13 的修复引入新的段落内矛盾**：`§3.5.1` 改写后同一句写“理由有三条”，随后用分号列出四项；并且该段仍把 `RiscvUbootBootShim` 写成“唯一实现”，而该类型与 `BootShim` trait 在当前 `os/` 中均不存在（新发现 N-01，P3）。
3. 两个 `⑤` 行处置正确：DESIGN-01/02 保持 open，指向 `R2-DESIGN-CASE.md`；代码与文档都没有假装修完（源码仍是两张表、`kmain` 仍走非 strict `validate()`）。

因此初检（`34bee52f`）给 **FAIL**：不是 8 个文档门失败，而是“23 个 `①` 全部落地 + 每条 `复验` 可重放”这一台账级断言不成立。**协调层随后修复了这三处阻断**；复检（`fcc992c6`）确认 N-01/N-02 与 R2-23/R2-24 的台账命令均已闭合，最终判定改为 **PASS**，逐条证据见文末“复检（fcc992c6）”节。R2 仍因 ⑤ 设计裁决保持 `BLOCKED`，本 PASS 只覆盖文档/代码内容侧。

## 逐根因

`DOC` = `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`。所有命令在仓库根执行；命中行号为复验时的实际行号。

| 行 | `复验` 命令与关键输出 | 判定 |
|---|---|---|
| R2-01 | `rg -n 'v1 接收面在 \`resume_additive\` 内是' DOC` → 1 处，L2545。源码核对：`os/kernel/src/lib.rs:300-345` 的 `resume_additive` 实际次序为 `boot_validate_and_prepare` → `plan_dm_policy` → `set_current_root_phys` → `store_kernel_info` → `register_panic_diagnostic` → `establish_boot_dm` → `jump_to_kmain`；文档列出的四个节点次序正确，且明写不调用 `arch_boot_impl`/`build_bootstrap_root_and_enable` | PASS |
| R2-02 | `rg -n '\`arch_boot_finish\` 在前置门之后把发送侧' DOC` → 1 处，L2402。源码：`lib.rs:228-232` 先 `validate_before_tables()`、`check_boot_postconditions()`，再 `store_boot_report` | PASS |
| R2-03 | `rg -n '窗口构造点见 §3.6' DOC` → 1 处，L1933；`rg -n '§3.3 的 source 规范集合' DOC` → 1 处，L1297。指针落地核对：§3.6 的 `enter_kernel_image_v1` 引文含 `BootWindows::new`/`push` 窗口组装（L1705-1740）；§4.4 有 `BootWindows::check` 与窗口边界规则（L2370、L2372、L2388）；§3.3 是来源覆盖规则的定义节（L1335） | PASS |
| R2-04 | `rg -n 'boot-shim 自身；被装载的 \`kernel.elf\` 仍是 ELF' DOC` → 1 处，L1086。事实核对：UEFI 固件装载的是 PE/COFF 的 boot-shim；内核由 `loader::KERNEL_PATH` 读出后按 ELF 解析（§4.2，`uefi_helpers.rs:fn prepare_and_enter_kernel_image`） | PASS |
| R2-05 | `rg -n '三种交接形态（同镜像内调用' DOC` → 1 处，L1149。§3.6 计数表 L1835 给同三个形态名，且说明当前生产腿只有跨镜像与直核两类 | PASS |
| R2-06 | `rg -n '这正是 §1.1 那条依赖的根据' DOC` → 1 处，L350。§1.1 L72 的权威口径是“两者只有一条依赖”，两处一致 | PASS |
| R2-07 | `rg -n 'aarch64 直核镜像 \`fw-aarch64-none\`' DOC` → 1 处，L1383；L1387 的判据结论已把 aarch64 生产直核镜像纳入。名称在源码/构建中存在：`os/kernel-image/Cargo.toml:28`、`os/xtask/src/image.rs:108`、`os/qemu-tests/test-smp-aps-aarch64.sh:75,80` | PASS |
| R2-08 | `rg -n '正常分配下反序：表仍会建' DOC` → 1 处，L242。C 侧核对：`minix3/sys/arch/i386/stand/lib/exec.c:742-758` 在 `btinfo_modulelist` 非空时建表并置 `HAS_MODS`；`module_init` 在表分配失败时 `return`（`:565-570`），`btinfo_modulelist` 保持空；`pre_init.c:167-169` 先查标志、再查 `mi_mods_count > 0`。文档拆出的两条路与源码一致 | PASS |
| R2-11（DESIGN-01） | ⑤，未实现也未假装修完。台账 L19 明写“待作者裁决”并指向 `R2-DESIGN-CASE.md`；代码仍是两张表（`os/libs/minix-boot/src/kernel_info.rs:673` 的 `MODULE_NAMES` 与 `os/kernel/src/proc.rs` 的 `BOOT_MODULE_PROC_NRS`，仓内无合并表）；文档 §4.3 L2181/L2286 描述的是两张表的现状与反事实 | PASS（保持 open） |
| R2-12（DESIGN-02） | ⑤，未实现也未假装修完。台账 L20 明写“待裁决”；代码 `os/kernel/src/lib.rs:1864` 仍是 `kernel_info.validate()`（非 strict），只有 v1 接收侧 L747 调 `validate_requiring_sources()`；文档 §4.4.1 L2379、门表 L2393 仍是 `kmain` 的 `validate` | PASS（保持 open） |
| R2-13 | `rg -n '\`prepare_boot\` 是不属于任何 trait 的自由函数' DOC` → 1 处，L1549，命中本身成立。但该段改出了新矛盾：`理由有三条：唯一实现是 U-Boot 腿的 \`RiscvUbootBootShim\`；没有 \`dyn BootShim\`；\`prepare_boot\` 是不属于任何 trait 的自由函数，不参与对象分派；测试接缝实际由 \`FileLoader\` 提供` 实际是四个分号项；且 `rg -n 'RiscvUbootBootShim' os` 与 `rg -n 'trait BootShim|struct RiscvUboot' os` 均为 0 命中，`RiscvUbootBootShim`/`BootShim` trait 已不存在。见 N-01 | **FAIL（P3 新矛盾）** |
| R2-14 | `rg -n '字段计数的完整链条是 16' DOC` → 1 处，L1175。链条 16→13（删 `free_upper_idx`/`bootstrap_start`/`bootstrap_len`）→14（加 `authority`）→10（v10 删四个）与 L1160-1171 的 10 字段结构一致，也与 §3.2.2 L1222 的独立复述一致 | PASS |
| R2-15 | `rg -n '一次性守卫在 land_and_store 入口' DOC` → 1 处，L2405。源码核对：`os/kernel/src/lib.rs:1582-1587` 的 `land_and_store` 入口确有 `assert!` 一次性守卫；引文块从 `fn land_and_store` 开始但省略该断言，注释准确 | PASS |
| R2-16 | `rg -n '\*\*先分清三套数字\*\*' DOC` → 1 处，L1222。三套口径为：22（C→Rust 累计退场）、16→13→14→10（Rust 结构字段数、现役 10）、16→13→14（§3.4.1/§3.4.3 的具体删除裁决）；交集四项与 §3.2.2.1 的表一致 | PASS |
| R2-17 | `rg -n '接收侧没有 ELF 头可读' DOC` → 1 处，L69。语义核对：三条 v1 接收腿确实不读内核 ELF 头，跨度由发送侧解析后交来；直核面用链接期常量，C 侧同样本地可得 | PASS |
| R2-18 | `rg -n 'UEFI 腿这条是空操作' DOC` → 1 处，L2507。源码：`os/kernel/src/vm_handoff.rs:22-23` 的 bump 行是整段扣除；UEFI 腿的 bump 区由固件分配为 `LOADER_DATA`、不在 conventional 列表，扣除对 free list 为空操作；自引导/U-Boot 腿的 bump 在 free list 内，实际扣除 | PASS |
| R2-19 | `rg -n 'WIRE_MEMMAP_CAP = 128' DOC` → 1 处，L2443。源码：`os/libs/minix-boot/src/handoff_v1.rs:356-357` 的 `WIRE_MEMMAP_CAP = 128`、`WIRE_RESERVED_CAP = 256` | PASS |
| R2-20 | `rg -n '已知诊断边界，已裁决不增打印' DOC` → 1 处，L2751。§5.5 L2647 的回指与之一致（“冻结的裁决明确不为此增加打印”） | PASS |
| R2-21 | `rg -n '四个计数的成员不同' DOC` → 1 处，L2606。四个成员集合与 §5.4 表一致：五条生产腿；§5.4 五条门链（含直载测试链、不含 aarch64 直核）；四条腿有启动门链；aarch64 直核走 `test-smp-aps-aarch64` 的 SMP 门 | PASS |
| R2-22 | `rg -n '重复提交 → 走 \`fill_\*\` 的腿第二次调用 panic' DOC` → 1 处，L1851；§3.7 L1847 同时写“半提交（写而未提交）在现行 API 中不可表示”，两处一致 | PASS |
| R2-23 | 命令原文：`rg -n 'pub fn source_audit\|impl Default for BootStorage\|impl Default for BootAlloc' os`（期望 0）→ **实际 1 处**：`os/libs/minix-boot/src/kernel_info.rs:561: pub fn source_audit`。按处置“改 `#[cfg(test)]`”核对：L560 有 `#[cfg(test)]`，唯一使用点 L776 在测试内；两个 `Default` 实现已 0 命中。命令的“期望 0”与处置自相矛盾（cfg(test) 不会让符号消失）。新副作用：`cd os && cargo check -p minix-boot --offline` → `libs/minix-boot/src/kernel_info.rs:8:28: warning: unused imports: InputObjectId and SourceAuditError`；`SourceAuditError` 因方法改为仅测试而变成非测试构建的未用导入（新发现 N-02） | **FAIL（复验不成立；处置主体落地）** |
| R2-24 | 命令原文：`rg -n 'startup-object\|named by a startup' os/libs/minix-boot/src/handoff_v1_write.rs os/boot-shim/src/uboot_main.rs`（期望 0）→ **实际 2 处**：`uboot_main.rs:517`、`handoff_v1_write.rs:391`。逐条核对：基线三处误导性残留中，`uboot_main.rs:485` 的 “named by a startup record” 已改为 “described by the v1 envelope”，`handoff_v1_write.rs:391` 的 “deliberately not cleared” 已改为 “v10 has no startup-object partition”，`prepare_boot` 蓝本的 startup 段已删；剩余两处都是“没有 startup 分区”的准确现状陈述，其中 `uboot_main.rs:517` 是基线就存在的准确注释。处置主体落地，但“期望 0”的复验命令不成立；`(d73)` 串口串保留，`os/qemu-tests/test-riscv64-uboot-shim.sh:206,208` 确实依赖它 | **FAIL（仅复验命令；内容 PASS）** |
| R2-25 | `rg -n '代码行按语义重排\|代码表达式也按语义重排' DOC` → 2 处，L1446、L1943。`doc-citation-check` 对四个声明式改写块给 WARN（1411-1441、1445-1466、1942-2041、2253-2282），原因是“声明为非逐字引文；source-symbol 已核对，内容未逐行对账”，不是 FAIL | PASS |
| R2-26 | `rg -n '校验先于消费（接收侧）' DOC` → 1 处，L83。§5.1 I4 行 L2613 已写“v1 发送侧的窗口消费（`prepare_kernel_image_v1`）不在本门链内，见 §4.2”，并限定“只覆盖接收侧已构造的负例” | PASS |
| R2-27 | `rg -n 'names 与初始分工归 02\|`fill_source_records` 在 U-Boot shim 与两个直核腿使用' DOC` → 1 处，L1847（第二支）。第一支是台账笔误：文档 L115 写“名字与初始分工归 02”，不是“names”；手工命中 L115。`fill_source_records` 作用域源码核对：`os/kernel-image/src/bootface_common.rs:539`（两个直核腿）与 `os/boot-shim/src/opensbi_helpers.rs:317`（U-Boot shim），UEFI 腿 0 命中 | PASS（台账命令第一支笔误，见 N-03） |

台账行号 R2-09/R2-10 不存在；25 个 FINDING 对应 25 行，编号跳跃不造成遗漏，但会让“R2-01..R2-27”的机械遍历出现空号，见 N-03。

## 重点 P2 复核

| 重点项 | 证据 | 判定 |
|---|---|---|
| §4.8 v1 链 | L2545 现写 `boot_validate_and_prepare → store_kernel_info → establish_boot_dm → jump_to_kmain`，且限定“在 `resume_additive` 内”；源码 `lib.rs:300-345` 的节点从 `boot_validate_and_prepare` 开始、`store_kernel_info` 在 `establish_boot_dm` 之前，顺序正确；中间省略的 `plan_dm_policy`/`set_current_root_phys`/`register_panic_diagnostic` 不改变所列节点的先后 | PASS |
| §4.5 first step | L2402 已改为 `arch_boot_finish` 在“前置门之后”深拷；源码 `lib.rs:228-232` 先 `validate_before_tables`/`check_boot_postconditions`，再 `store_boot_report`；与 §3.7/§4.4.1 的门链一致 | PASS |
| §4.1 / §3.2.3 指针 | L1933 指 §3.6 的 `enter_kernel_image_v1` 组装与 §4.4 的 `BootWindows::check`/窗口边界；L1297 指 §3.3 的 source 规范集合。两个目标节都含目标内容（§3.6 L1705-1740；§4.4 L2370/2372/2388；§3.3 L1335） | PASS |
| §3.1 PE/ELF | L1086 明确“boot-shim 自身 PE/COFF；被装载的 `kernel.elf` 仍是 ELF”，与 §4.2 的 `KERNEL_PATH`/ELF 段解析一致 | PASS |
| §3.2 三形态 | L1149 给出三个形态名“同镜像内调用 / 跨镜像跳转 / 直核自引导”，与 §3.6 计数表 L1835 逐字一致 | PASS |
| §1.1 依赖数 | §1.1 L72 写“只有一条依赖”；§2.3 L350 已改为“那条依赖”，并按同一句补了禁止反向（框 1 不进入框 2）；两处计数一致 | PASS |
| §3.4.1 “enum”（生产镜像枚举） | 文内无 `enum` 关键字，任务所指应为 READER-C-03 的“生产镜像清单”。L1383 已补 `fw-aarch64-none`，L1387 的判据结论已覆盖五条生产腿；名称在源码/构建中存在（见 R2-07） | PASS（按枚举口径） |
| §2.1 load_mods | L242 已拆成“正常分配：表仍建、`HAS_MODS` 已置、停在条数断言”与“模块表分配失败：`module_init` 提前返回、`HAS_MODS` 未置、停在查标志断言”两条；与 `exec.c:565-570,742-758`、`pre_init.c:167-169` 一致 | PASS |
| I4 接收侧限定 | L83 标题与判据均加“接收侧”；§5.1 L2613 写明 v1 发送侧 `prepare_kernel_image_v1` 的窗口消费不在本门链内，并给出 §4.2 指针 | PASS |

## 机械门

命令均在仓库根执行，`DOC` 同上；完整输出保存在 `/tmp/y02-*.out`。

| 门 | 命令 | 读数 / 结果 |
|---|---|---|
| anchor | `tools/anchor-resolve.sh --check DOC` | exit 0；`anchors=86 resolved=86 zero-def=0 multi-def=0` |
| code-map | `tools/doc-code-map.sh DOC --check --format=plain` | exit 0；49 个 `os/*.rs` 路径，存在性 49/49 |
| ref | `AWK=gawk tools/doc-ref-check.sh DOC` | exit 0；`✅ … 内部章节引用均可解析` |
| style | `tools/doc-style-lint.sh DOC` | exit 0；`命中汇总：| error级=0` |
| publication | `python3 tools/doc-publication-check.py DOC` | exit 0；`无新增违规`（基线模式） |
| rubric | `python3 tools/tutorial-rubric-check.py DOC` | exit 0；`检查 6 章，0 章缺项` |
| citation | `python3 tools/doc-citation-check.py DOC` | exit 0；`blocks=54 checked=20 failed=0 skipped=30 warned=4`；4 个 WARN 都是“声明式改写 + `source-symbol` 已核对”，块 1411-1441/1445-1466/1942-2041/2253-2282 |
| density | `tools/doc-code-density-check.sh DOC` | exit 0；`✅ 承载门阻断项通过（4 个可判章均有码/命令）`；章 2/3/4/6 分别有码 15/14/10 与命令 2 |

补充：对本次改动的 5 个代码文件跑 `python3 tools/code-publication-check.py …` → exit 0，`无新增违规`。对 `minix-boot` 跑 `cargo check -p minix-boot --offline` → exit 0，但带新警告（见 N-02）。

## 新发现

### N-01（P3，P0-fact 类内容；复检已清）：§3.5.1 改写后“三条理由”实为四条，且引用不存在的 `RiscvUbootBootShim`/`BootShim` trait

- 位置：`DOC` L1549。
- 证据：
  - `rg -n '理由有三条' DOC` → L1549；该句分号项为：①“唯一实现是 U-Boot 腿的 `RiscvUbootBootShim`”；②“没有 `dyn BootShim`”；③“`prepare_boot` 是不属于任何 trait 的自由函数，不参与对象分派”；④“测试接缝实际由 `FileLoader` 提供，不是它”。四项目，标签写三。
  - `rg -n 'RiscvUbootBootShim' os` → exit 1（0 命中）；`rg -n 'trait BootShim|struct RiscvUboot' os` → exit 1（0 命中）。`BootShim` 在 `os/` 只剩两处与代码无关的文字（`os/qemu-tests/test-riscv64-uboot-shim.sh:6` 的注释、`os/libs/README.md:21,90` 的过时 README），没有类型定义。
- 影响：读者会去找一个不存在的类型/ trait；该段把已删除 trait 的“唯一实现”理由与现行自由函数设计混在同一句，正好落入出版标准禁止的“旧形态叙事”（虽然机械 publication 门因无编号/无时间词未命中）。
- 最小修复：整段重写为现状陈述，例如“`prepare_boot` 是 `opensbi_helpers` 的自由函数，由 `uboot_main` 直接调用；没有对象分派，也没有需要保留的 trait 接缝。测试接缝由 `FileLoader` 提供。”删掉 `RiscvUbootBootShim`/`dyn BootShim`/“单实现 trait”这些已不存在对象的叙述；若要保留设计史的教学价值，改写成反事实（“若曾抽成单实现 trait，它既不是接缝也不是扩展点，因此不值得保留”），并同步把“理由有三条”改成实际条数。

### N-02（P3，代码清理；复检已清）：`source_audit` 改 `#[cfg(test)]` 后留下未用导入警告

- 位置：`os/libs/minix-boot/src/kernel_info.rs:8`、`:561`。
- 证据：`cd os && cargo check -p minix-boot --offline --message-format short` → `libs/minix-boot/src/kernel_info.rs:8:28: warning: unused imports: InputObjectId and SourceAuditError`。`SourceAuditError` 的唯一使用点是 L561 的 `#[cfg(test)] pub fn source_audit`（测试调用 L776）；基线该方法非 `cfg(test)`，因此 `SourceAuditError` 不是未用导入，这条警告是本次修复新引入的。`InputObjectId` 的未用导入在基线就存在（测试用全限定路径），不归本次。
- 影响：非测试构建的警告面扩大；`check-clippy-ratchet` 若只看 clippy 不一定抓 rustc 的 unused_imports，但代码出版标准要求干净。
- 最小修复：把 `SourceAuditError` 从顶层 `use` 移到测试模块，或在方法签名写 `crate::memmap_source::SourceAuditError`；同时顺手把 `InputObjectId` 的存量未用导入清掉（可另记，不混入本条判定）。

### N-03（P3，台账/证据完整性；R2-23/R2-24 命令复检时已改，剩余项见文末）：两条 `复验` 命令的期望值与处置不一致，另有编号/指针瑕疵

- R2-23：`复验` 期望 0，但处置明确允许 `#[cfg(test)]` 保留 `pub fn source_audit`，两者互斥。应改为两条：`rg -n '#\[cfg\(test\)\]\n *pub fn source_audit' -U os/libs/minix-boot/src/kernel_info.rs` 期望 1（或改用直接删除并把测试一并处理），加 `rg -n 'impl Default for BootStorage|impl Default for BootAlloc' os` 期望 0。
- R2-24：`复验` 期望 0，但 `uboot_main.rs:517` 的“No startup-object partition is written”是基线就存在的准确注释，`handoff_v1_write.rs:391` 是改写后的准确现状句；真正的三处误导性残留（`uboot_main.rs:485`、`prepare_boot` 的 startup 段、`handoff_v1_write.rs:391` 的旧句）已清零。应把 `复验` 改成针对旧串的窄模式（如 `rg -n 'named by a startup|startup from the argv|deliberately \*not\* cleared' …` 期望 0），或在文档/代码里统一改为“v10 无 startup 分区”的措辞并接受非零命中。
- R2-27：`复验` 第一支写 `names 与初始分工归 02`，文档 L115 实际是“名字与初始分工归 02”，该支永不命中；第二支命中，行判定仍成立。应改中文“名字”。
- R2-12：`单元` 列写 `4.4.2`，但文档只有 §4.4 与 §4.4.1，没有 §4.4.2（`rg -n '^#### 4\.' DOC` 只返回 4.2.1/4.2.2/4.4.1）。案件卷/面板也沿用了这个不存在的节号。
- 台账行号跳过 R2-09/R2-10；25 个 FINDING 对应 25 行无遗漏，但“逐条 R2-01..R2-27”的机械遍历会出现空号。
- 台账 R2-24 记 `(d73)` 为 FP，但 `metrics.md` 与台账头部的 `处置: … FP 0` 没有把该子项计入。协调层确认 `open=3` 是预期状态，因此本报告不把 open 计数或 FP 口径差列为判定依据，只记作元数据口径风险。`metrics.md` 的 `自引入: 3` 也没有明细，按 `review-evals.md` §4.0c 自引入缺陷的 `origin` 应记引入轮。

## D1/D2 案件卷

`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r2/R2-DESIGN-CASE.md`（37 行）核对：

- **完整性：可用**。DESIGN-01 与 DESIGN-02 各有：现象（含模块表/进程号表与 `kmain` 非 strict 的具体位置）、选项 A/B、D2 推荐、不做后果、请求作者裁决；状态明写“待作者裁决；me-agent 不代替作者决定设计方向”，并写“裁决前 R2 保持 BLOCKED”。
- **未假装修完**：代码核对仍支持两处“待裁决”描述——`BOOT_MODULE_PROC_NRS` 与 `MODULE_NAMES` 仍是两张表（`rg -n 'BOOT_MODULE_PROC_NRS' os` 命中 `os/kernel/src/proc.rs` 等，且无合并表）；`kmain` 仍是 `kernel_info.validate()`（`lib.rs:1864`），不是 `validate_requiring_sources()`。
- **瑕疵（不影响完整性）**：DESIGN-02 的台账/面板“单元 4.4.2”指向不存在的节（见 N-03）；案件卷本身没有引用这个节号。案件卷比 R1 的 `DESIGN-02-CASE.md` 轻（没有单独的“事实/证据”节与代码行号），但用于作者选 A/B 已足够。

## 残余风险

- 本报告只做 R2 文档复验与台账核对；R2 仍应保持 `BLOCKED`，不能据此写 `ROUND-CLOSED` 或 `CONVERGED`（DESIGN-01/02 未裁决）。
- 4 个 citation WARN 是“声明式改写、`source-symbol` 已核对、内容未逐行对账”的块；它们不是失败，但意味着这四块的代码文字没有逐字对账，属于已知弱证据。
- 机械门全绿不等于 G18 冷读通过；本 verifier 未执行冷读（任务未要求）。
- “§3.4.1 enum”在文中没有 `enum` 关键字；本报告按 READER-C-03 的“生产镜像枚举”理解并核过。若任务原意是另一对象，需要再指认。
- `R2-23` 的 `cfg(test)` 方案依赖测试模块能访问 `source_audit`；当前 `cargo check` 只证非测试构建可编译（带警告），未重跑 `cargo test -p minix-boot --lib`。台账/loop 记录的 188/0 是修复者读数，本 verifier 未重放。
- 工作区在复验期间有并发写入（新增未跟踪 `R2-COLD-READ-CASE.md`）；本报告不依赖其内容。

## 复检（fcc992c6）

RECHECK: **PASS**

- 新文档 sha256：`fcc992c685e841ae0441fa0280afa2f7c2a716f69b34cd9fb4334cae6fe155d7`（2,951 行），与协调层给定一致。相对初检，`git diff HEAD -- DOC` 只改 §3.5.1 两行：段内“理由有三条”→“理由有两条”、删去 `RiscvUbootBootShim`/`BootShim` 提及、签名补生命周期。
- **N-01 PASS**：
  - `rg -n 'RiscvUbootBootShim|BootShim' DOC` → exit 1（0 命中）。
  - `rg -n '理由有两条' DOC` → L1549，1 处；两个分号项为“没有 `dyn` 分派，`prepare_boot` 是自由函数，不参与对象分派”与“测试接缝实际由 `FileLoader` 提供，不是任何引导 trait”，与“两条”一致。
  - `rg -n -F "签名是 \`prepare_boot<'a>(&'a mut BootStorage, bump_pages) -> BootPrepareResult<'a>\`" DOC` → L1551；源码 `os/boot-shim/src/opensbi_helpers.rs:241-244` 的 `storage: &'a mut minix_boot::BootStorage, bump_pages: usize) -> minix_boot::BootPrepareResult<'a>` 与之一致。
- **N-02 PASS**：
  - `sed -n '8,10p' os/libs/minix-boot/src/kernel_info.rs` → `use crate::memmap_source::{InputObjectId, RamAuthority, SourceRecord};` 加 `#[cfg(test)] use crate::memmap_source::SourceAuditError;`。
  - `cd os && cargo check -p minix-boot --offline --message-format short` → exit 0；只剩 `kernel_info.rs:8:28: warning: unused import: InputObjectId`（基线既有，非本次引入）与 `memmap_source.rs:39`（基线既有）；`SourceAuditError` 警告消失。
  - `cargo clippy -p minix-boot --offline --message-format short` → exit 0；两个 `Default` 恢复后没有新增 `new_without_default`，输出中的 8 条为存量。
- **R2-23 台账命令 PASS**：
  - `rg -n -B1 'pub fn source_audit' os/libs/minix-boot/src/kernel_info.rs` → `562- #[cfg(test)]`、`563: pub fn source_audit`（1 处；上一行即 `#[cfg(test)]`）。
  - `rg -n 'impl Default for (BootStorage|BootAlloc)' os` → 2 处（`os/libs/minix-boot/src/boot_storage.rs:108`、`os/kernel/src/boot_alloc.rs:41`），与“期望 2，保留”一致。
- **R2-24 台账命令 PASS**：
  - `rg -n 'deliberately \*not\* cleared|named by a startup record|param/startup' os/libs/minix-boot/src/handoff_v1_write.rs os/boot-shim/src/uboot_main.rs` → exit 1（0 命中），与“期望 0”一致。
- 变更段未新增/删除锚点、代码块、章节标题或容量数字；按协调层“无需重跑 8 门全部”的要求，补跑三个廉价门作回归：`AWK=gawk tools/doc-ref-check.sh DOC` → `✅ 内部章节引用均可解析`；`tools/doc-style-lint.sh DOC` → `error级=0`；`python3 tools/doc-publication-check.py DOC` → `无新增违规`。anchor/code-map/rubric/citation/density 的初检读数仍适用于同一文档主体。
- 复检后仍存在的非阻断元数据项（不改变 PASS）：
  - 台账头部仍写 `FP 0`，而 R2-23 行把两个 `Default` 标为 FP（子项）；建议把口径统一成“R2-23 主体 ①、`Default` 子项 FP”，或把 FP 计 1。
  - `metrics.md` 的 `自引入: 3` 仍无明细；按 `review-evals.md` §4.0c 应给引入轮与条目。
  - 协调层确认 `open=3` 是预期状态；本报告不把 open 计数作为判定项。
  - R2 仍 `BLOCKED` 于 DESIGN-01/02 的作者裁决；本 PASS 不表示 `CONVERGED`。

`RECHECK: PASS` 的含义仅限：初检列出的三处阻断（N-01、N-02、R2-23/R2-24 复验命令）在 `fcc992c6` 上已闭合；8 门初检读数与“两个 ⑤ 保持 open”的结论不变。
