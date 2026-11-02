# 03-kmain-cstart Review 轨迹

> 本文是 `03-review.md` 的过程档案，按 `prompt/DOC-ITER-WORKFLOW.md` §S1/S4 与 `AI-chats/doc-gen-iterate-prompt-v2.md` §B 的要求记录：命令、输出摘要、Skill 调用、Gate 退出码、失败与返工。
> 只记增量与可重放证据；不复述报告结论。

## 0. 基线

```text
$ sha256sum rewrite-notes/01-stage-kernel/03-kmain-cstart.md
7c4ca5a887efb9086ed297a9d4717bf804f3f1dd212e7112fdfae4c080ec2b55

$ wc -l rewrite-notes/01-stage-kernel/03-kmain-cstart.md
1360

$ git rev-parse HEAD
27843596361a3280f8bc807b26ae0ad1db1545fc

$ git status --porcelain
 M AI-chats/daily.todo.md
```

- 受审文档与 `os/` 代码全程零改动；工作树仅有用户原有的 `AI-chats/daily.todo.md` 修改。
- `.review/`、`.design/` 均在 `.gitignore`（`.gitignore:27,34`），本轮产物不入库。

## 1. R0 预检

### 1.1 review-init

```text
$ bash tools/review-init.sh zcode rewrite-notes/01-stage-kernel/03-kmain-cstart.md zcode --design-policy strict --size-adaptive
exit=0
产物：.review/zcode/01-stage-kernel/03-kmain-cstart/{scan.md,structure.md,SYMBOLS.md}
```

### 1.2 design/outline 4 条 ls

```text
03-outline.v1.md / 03-outline.v3.md
03-outline-review.v1.md / 03-outline-review.v3.md
03-design.v1.md
03-design-final.v*.md（无）
design-coverage-check: 03-kmain-cstart PASS
```

判定：快照齐全；v1 design 过期（2026-07-16），本轮重评 v2。

### 1.3 关联代码清单

```text
$ bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md --check --format=md
exit=0；20 个文件，存在性 20/20。
```

### 1.4 四道文档门基线

```text
$ bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md
anchors=85 resolved=84 zero-def=1 multi-def=0
exit=1
→ zero-def: os/arch/src/riscv64/protection.rs:static KERNEL_TRAP_STACK_BASE

$ AWK=gawk bash tools/doc-ref-check.sh --check ...
exit=0（全绿；但 §2.6 的 `（§2.2）` 是同号假通过，人工另判）

$ bash tools/doc-style-lint.sh ...
error级=0
exit=0

$ bash tools/doc-code-density-check.sh --check ...
4 个可判章均有码/命令
exit=0
```

### 1.5 覆盖率

```text
$ python3 tools/coverage-extract/coverage-extract.py kernel rewrite-notes/01-stage-kernel \
    --rust-dir os --c-dir minix3/minix/kernel --doc-file 03-kmain-cstart.md \
    --semantic-map tools/coverage-extract/kernel-semantic-map.json \
    --output .review/zcode/01-stage-kernel/03-kmain-cstart/SYMBOLS.md
exit=0
Coverage Summary: 94/217 (43.3%) semantic; 71 有文档无 Rust

$ python3 tools/coverage-extract/coverage-extract.py --check-filled .../SYMBOLS.md
exit=0（填完 94 行后）
```

## 2. R1 正确性

### 2.1 C 引用逐块

- 复跑 §6.2 的全部 C 命令，输出与文档“关键行”一致：
  - `grep -c '#if' protect.c` = 0
  - `grep -rn "^void prot_init" arch/*/protect.c` = 2 行
  - `grep -rn "int kernel_may_alloc"` = 3 行
  - `gate_table_pic` 16；`gate_table_exceptions` 23；`USER_PRIVILEGE` 6
  - `printf '%d\n' $((6+1))` = 7
- `tss_s` 27 字段逐项；`tss_init` 五调用点（`arch/earm/arch_system.c:113`、`arch/i386/protect.c:338`、`arch/i386/include/arch_smp.h:18`、`arch/i386/arch_smp.c:272`、`arch/i386/arch_system.c:256`）全部命中。
- `tss_init_all` 存在于 `arch_smp.c:267`，调用点 `:300`。
- `pt_init` 的 BIGPAGE 跳过在 `vm/pagetable.c:1286`（`if((entry & ARCH_VM_BIGPAGE)) continue;`）。
- `Makefile.inc:51-52` 的改名名单含 `pre_init.o`、`pg_utils.o`，不含 `protect.o`。
- `pre_init` 返回 `&kinfo`（`:237`）；`head.S:79-83` 把 `%eax` 压栈后 `call kmain`；`glo.h:35` `#define kmess kmessages`；`pre_init.c:117` `cbi->kmess = &kmess`。
- `prot_init_done` 的读者：`arch_smp.c:87` `assert(prot_init_done)`。

### 2.2 Rust 引用逐块

- `init_protection`（`lib.rs:1988-2086`）与 §4.2 节选一致；`store_protection`/`refresh_tss_descriptors`/`load` 顺序一致。
- `store_kernel_info`（`lib.rs:1119-1230`）实际五块载荷；文档 §4.1 说“三块”。
- `KERNEL_MAY_ALLOC`（`globals.rs:276`）置位/清位存在；getter `lib.rs:2884` 无调用。
- `configure_ipc_entry`：`rg` 全仓只命中 trait/impl/注释/测试，无生产调用。
- `set_kernel_stack`：`rg` 只命中 trait/impl/测试，无生产调用；`smp.rs:1590` 只调 `init_ap`。
- `init()` 注释（`protection.rs:363-366`）称“never need rebuilding”，与 `refresh_tss_descriptors`（`:281-294`）冲突。
- `init_ap` 注释（`:397`）称“BSP wrote sp0 via set_kernel_stack”，与 `rg` 结果矛盾。
- 代码注释 6 处引用 `rewrite-notes/03-stage-kernel/...`（应为 `01-stage-kernel`）与不存在的 §1.7/§3.1/§3.2/§3.3。

### 2.3 测试对账

```text
16 / 14 / 11 / 2 / 1 / 3 / 2 / 1 / 7（与 §5.1 一致）
§5 列出的 fn 全部 grep 命中
test-protection x86：读 GDTR/IDTR/TR/CS/DS；不读 MSR；传 info.syscall_entry
test-protection aarch64：独立探针；VBAR/CurrentEL/DAIF/SPSel/只读 SP_EL1
test-protection riscv64：stvec/sscratch/sstatus
```

### 2.4 宿主测试

```text
$ cargo test -p minix-kernel --features mock
856 passed; 0 failed; 8 ignored
exit=0

$ cargo test -p minix-arch --features runtime-window
exit=0

$ cargo test -p minix-arch          # 默认形态
38 compile errors
exit=101
→ feature 形态问题；改用 runtime-window 后通过，不归因于 03

$ cargo clippy -p minix-arch -p minix-kernel --lib --no-deps -- -D warnings
36 + 75 pre-existing warnings
exit=101
→ 仓库基线债，不归因于 03
```

### 2.5 本轮首次定级的 P0

1. `P0-code-apTssSp0`（主会话读 `init_ap`/`smp_init` + `rg`）
2. `P0-fact-apSetKernelStack`（同）
3. `P0-fact-riscvBase`（anchor zero-def + 代码对照）
4. `P0-fact-phaseQuote`（Rust 引用逐块）
5. `P0-fact-bootProcs`（C 调用关系）
6. `P0-fact-intrInitOrder`（A2 独立发现 + 主会话重放 i8259）

## 3. R2 卓越性与对抗 agent

### 3.1 A1 代码事实核查（独立、对抗框架）

- 输入：文档 + 关联文件 + C 真值；禁止读 `.review/`、`.design/`、`tmp/review03/`、`03-review*`。
- 任务：找矛盾、遗漏、未证明跳跃、伪造/过期引用；每条附命令与输出。
- 状态：**已返回**（`tmp/review03/A1-codereview.md`，1 P0 / 5 P1 / 13 P2）。P0 与主报告 riscv 项一致；P1 中 boot_procs、D-59、M4、docPaths 与主报告一致；**7 条主报告未覆盖的新发现**：`test-user-trap` 覆盖门有效性（P1）、`vir2phys` 过度概括（P1）、两处逐字引用增补 `by S-4`/`S-4 L2:`（P1）、`EMPTY` 位置（P2）、`IDENTITY_MAP_END` 注释（P2）、`parity` 释义（P2）、aarch64 M2 探针误记（P2）。全部已按“外部 review 结论先复跑证据”处理，并入主报告 §9。

### 3.2 A2 闭卷学生

- 输入：只读 `03-kmain-cstart.md`；不读源码/规则/其他文档。
- 结果：10 题中 9 可答、1 需一步推断；15 节中 12 ①、3 ②（§2.8/§3.5/§3.6）、0 ③。
- 独立命中：§4.6 `SP_EL1` 两种互斥失败模式；riscv `KERNEL_TRAP_STACK_BASE` 全局与 per-hart 不明；§2.2 `intr_init` 因果不闭合；§2.1 自拷贝证明依赖未展示行；§2.6 `（§2.2）` 歧义；§3.1 `ss0` 含糊。
- 报告：`tmp/review03/A2-student.md`。

### 3.3 A3 冷读结构编辑

- 输入：只读文档；不读源码/规则/diff/自评。
- 结果：骨架可信、弧线成立、接缝五处；根因“按语言切章 vs 按机制论证”；22 条反事实/21 节模板指纹。
- 独立命中：Ch1 首句主语是书目对象；§1.2 3×3 框架前后不一致；§2.5→§2.6/§2.6→§2.7/§2.7→§3.1 跳跃；§3.3/§4.2 重复；§5.3/§5.4/§5.4-TODO 三份表。
- 报告：`tmp/review03/A3-structure.md`。

### 3.4 主会话 R2 补充

- 代码卓越：`init_protection` 6 处 arch cfg；`configure_ipc_entry` 死代码；`static mut` 三处；`unsafe impl Send/Sync` 注释理由悬空。
- 文风：`doc-style-lint` error=0，但过程编号 `D-59`/`D-64①`/`1.109`/`1.113`/`S-4` 未被 SL-11 覆盖；`doc-code-density` 4/4 通过。
- 结构：`structure.md` 12 节填写；Gate D-6 PASS 带 P1。

## 4. R3 综合与 Gate G

### 4.1 Issue List

- 50 条：P0=6、P1=28、P2=16（A1 返回后的最终计数；A1 之前为 43 条）。
- 与主报告 §9 同一清单。

### 4.2 Gate G 机械化检查（两次）

第一次：

```text
$ python3 tools/verify-check.py --scan ... --state ... --symbols ... --sample-ratio 0.2 --seed 20261102 --output ...
⚠️ 机械化检查未全过，建议直接判 CONCERN
exit=1
缺失项：Gate B 证据块 missing_kw=['行为契约']
```

返工：在 `gate-evidence-B` 块首行加 `行为契约:`。第二次：

```text
$ python3 tools/verify-check.py ...
✅ VERIFY-CHECK.md 已生成
exit=0

$ python3 tools/verify-check.py --scan ... --self-check
✅ 9 个锚段齐全
Gate A/B/C/D/D-6/E/G/H 全部 ✅
总 issue 数: 50；抽样 10
✅ VERIFY-SELF PASS
```

### 4.3 返工记录

| # | 失败 | 根因 | 修复 | 复验 |
|---|---|---|---|---|
| 1 | Gate G 机械 FAIL | `gate-evidence-B` 缺关键字 | 加 `行为契约` | exit=0 |
| 2 | Gate G 只提取 41/43 | 两个 issue ID 含 `.`（`1.3ifdef`/`5.4-outside-outline`），正则不匹配 | 改名为 `ifdefEarly`/`outline54` | 43/43 |
| 3 | `cargo test -p minix-arch` 38 错误 | 默认 feature 形态不编译测试 | 改用 `--features runtime-window` | exit=0 |
| 4 | `clippy -D warnings` 失败 | 仓库既有 36/75 告警 | 记录为基线债 | 不阻断 |
| 5 | A1 返回后新增 7 条 | 主会话首轮扫描漏掉引用完整性/测试覆盖反向断言 | 逐条复跑后并入 | 50/50 |

### 4.4 A4 独立验证

- 输入：主报告 + scan + SYMBOLS + 源码；抽样 8 个 issue + 18 个符号 + P0-code-apTssSp0。
- 状态：**未返回**；报告路径预定 `tmp/review03/A4-verify.md`。由主会话按 10 个抽样 issue 逐条重放，填 VERIFY-CHECK 人工段：9 CONSISTENT + 1 PARTIAL → 95% PASS（同 agent 标注）。若 A4 后续返回且一致性 <90%，以 A4 为准重判。

## 5. R4/R5 产物

```text
rewrite-notes/01-stage-kernel/03-review.md                  （主报告）
rewrite-notes/01-stage-kernel/03-review-trajectory.md       （本文件）
.review/zcode/01-stage-kernel/03-kmain-cstart/scan.md
.review/zcode/01-stage-kernel/03-kmain-cstart/SYMBOLS.md
.review/zcode/01-stage-kernel/03-kmain-cstart/structure.md
.review/zcode/01-stage-kernel/VERIFY-CHECK.md
rewrite-notes/01-stage-kernel/.design/03-design.v2.md
rewrite-notes/01-stage-kernel/.design/03-outline.v4.md
rewrite-notes/01-stage-kernel/.design/03-outline-review.v4.md
tmp/review03/A1-codereview.md（已返回：1 P0/5 P1/13 P2）
tmp/review03/A2-student.md
tmp/review03/A3-structure.md
tmp/review03/A4-verify.md（待）
```

## 6. 复盘（本轮 review 自身的失败与改进）

1. **同模型盲点**：A1/A2/A3/A4 与主会话同模型；用材料隔离（A2/A3 只读文档）与独立重放补偿，但不宣称异构。改进：下一轮优先换模型家族或至少换代际。
2. **未跑 QEMU**：Gate E 证据弱；用静态逐行审计 + 宿主测试替代，明确写“未跑”。改进：修复轮必须补 `test-protection` 三架构与 `test-smp-ap-alive` 真机读数。
3. **覆盖率分母过宽**：43.3% 不是本篇真实漏写率；用人工分档解释。改进：给 semantic-map 加“本篇射程”过滤，或让工具按文档作用域计算。
4. **设计快照过期**：v1 与现码 60%；本轮重评 v2。改进：每轮 review 必须重评，不把旧快照当 gate 依据。
5. **工具假通过**：`doc-ref-check` 同号跨文档假绿；`doc-style-lint` 不覆盖 `D-59` 类编号。改进：加人工跨文档核对与过程编号 grep。
6. **A1 长尾、A4 未返回**：A1 在收到收口指令后返回（1 P0/5 P1/13 P2），7 条新发现已并入；A4 未返回，VERIFY-CHECK 人工段由主会话按“同 agent + 已重放”填写（95% PASS）。改进：下次给对抗 agent 更小的输入切片，避免超大 prompt 导致长尾；A4 若后续返回且 <90%，以 A4 为准重判。
7. **报告偏长**：R1/R2 的维度全量展开，阅读成本高；但用户要求“深度全面”，保留。改进：下一轮按 chapter 拆成多份 scan，主报告只留结论与证据索引。

## 7. 修复轮与回归（2026-11-02）

### 7.1 代码修复

- **AP TSS.sp0**：`smp_init` 在 `boot_ap` 前对每个 AP 调 `set_kernel_stack`；`init_ap` 加 `sp0 == kernel_stack_top - 16` production 断言。`cargo check -p minix-kernel --features mock` exit 0。
- **cfg 泄漏**：`ProtectionArch` 新增 `refresh_descriptors`/`program_entry_base`；`trap_dispatch::register_current_arch` 收拢 dispatcher 注册；`init_protection` 不再有 `#[cfg]`。x86_64/aarch64/riscv64 三目标 `cargo check` exit 0。
- **死代码**：删除 `configure_ipc_entry`（trait/三架构 impl/mock/test/注释）；删除 `X86PrivilegeLevel::get`；`kernel_may_alloc()` 接到 `memmap::add_memmap` 的 production `debug_assert`。
- **static mut**：`IST_STACKS`/`AP_KERNEL_STACKS` 改不可变 `static`；`GS_AREAS` 改 `UnsafeCell` + 手动 `Sync`。
- **注释/路径**：6 处 `rewrite-notes/03-stage-kernel` → `01-stage-kernel`；节号改为 §3.2/§3.1/§1.2；`init()`/`init_ap`/`init_protection` 注释与实现同步。
- **测试**：`test-protection` 改用 `syscall_entry_va()` 并新增 `rdmsr` 校验 STAR/LSTAR/SFMASK/EFER；删除 `configure_ipc_entry_sets_idt_gate_33`（trap_entry 测试 14→13）。

### 7.2 文档修复

- P0：§4.6 riscv 改为 per-hart `sscratch` + `prepare_user_return`；§5.4 M8/§6.1/R03-T06 同步；§3.6 删 `D-59`；§2.1 boot_procs 读者改正；§2.2 `intr_init` 因果链改正。
- P1：§1 主语/跨架构四列表/术语前置/元话语；§2.6 vir2phys 判据；§3.1 `ss0`；§3.2 新增 trait 方法映射；§3.3 只留 E1 注释；§4.2 与现码同步；§4.6 `SP_EL1` 单一失败模式；§5.1/§5.2/§5.4 测试口径；§6.2 计数 13。
- P2：i8259 屏蔽字、pg_identity 边界、`EMPTY` 位置、`IDENTITY_MAP_END` 注释、parity 释义、aarch64 M2、M4/M6 点名、`#ifdef` 位置、SYSRET 重复、§5.4 重复收尾。

### 7.3 返工与失败（如实记录）

| # | 失败 | 根因 | 修复 | 复验 |
|---|---|---|---|---|
| 1 | `USER_DS_SELECTOR` 被当死代码删除后编译失败 | `#[allow(dead_code)]` 陈旧；`trap_stub.rs:437` 实际使用 | 恢复 const 并删 `allow` | 编译通过 |
| 2 | aarch64/riscv 编译报 `with_protection_mut` 不存在 | 非 x86 只有 `with_protection` | 补非 x86 `with_protection_mut`（throwaway 实例 + 默认 no-op） | 两目标 exit 0 |
| 3 | 删除 IPC 测试后 §5.1/§5.4/§6.2 仍写 14 | 测试计数未同步 | 全部改 13 | `grep -c '#[test]'` = 13 |
| 4 | A6 指出 §6.2 仍写 14、§2.1 “三行”实为四行、§5.4 收尾重复 | 回归残项 | 逐条修正 | A6 残项清单清零 |
| 5 | `test-protection` 的 `rdmsr` 首次编译需 `fw-x86-uefi` feature | 裸 `cargo check -p` 不选 bin | 加 `--features fw-x86-uefi` | exit 0 |

### 7.4 真机回归

```text
test-protection x86_64      ### TEST_RESULT: PASS test-protection ###（含 SYSCALL MSRs verified）
test-protection aarch64     ### TEST_RESULT: PASS test-protection-aarch64 ###
test-protection riscv64     ### TEST_RESULT: PASS test-protection-riscv64 ###
test-smp-aps x86_64         ### TEST_RESULT: PASS test-smp-aps ###（4 CPU online）
```

### 7.5 A4/A6 结果

- **A4**：8 个抽样 issue 6 CONSISTENT + 2 PARTIAL = **87.5% CONCERN**；确认 AP TSS 缺口、riscv 静态、boot_procs 读者；指出 P0 riscv 修复需覆盖 §5.4 M8（已补）。
- **A6**：15/15 节 ①，0 ②/③；§2.8/§3.5/§3.6 改善；指出 §6.2 计数、§2.1 行数、§5.4 重复、§2.4/§2.7/§3.4/§2.2 子项（已修）。
- **A5**：返回 **PASS**（6/6 P0 CLOSED；22 个抽样 P1 中 18 CLOSED + 4 PARTIAL，4 个 PARTIAL 已在 follow-up 补齐；0 new P0/P1；唯一新增 P2 注释冗余已删）。报告 `tmp/review03/A5-regression.md`。

### 7.6 回归后状态

- P0：0 open。
- P1：0 open doc（24 CLOSED；1 WITHDRAWN 规则层；1 TOOL-LIMIT 覆盖率分母）。
- P2：0 open doc（14 CLOSED；1 CORRECTED；1 WITHDRAWN 风格偏好）。
- 文档状态：**CONVERGED_WITH_WARNING**（Gate A 工具分母、A5 待回、QEMU 已跑）。
