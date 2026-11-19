# D01 回归审查 —— 01-boot-shim-bootstrap（FREEZE 后复核）

- **结论：PASS**
- 审查日期：2026-11-13
- 当前 HEAD：`a4afb4927`
- 冻结记录：同目录 `FREEZE.md`（冻结提交 `5c7134fbd`，确认轮 R34，工作流 edition V3.3）
- 审查方式：只读复核；除本文件外未修改仓库任何文件

## 1. FREEZE 记录复核

| 复核项 | FREEZE 记录 | 本次实测 | 结论 |
|---|---|---|---|
| 文档 sha256 | `3284d47a7f21529626e7a7b8933aa91a37c0501d4abd56244e45b3d503145b97` | 同值 | 一致 |
| 文档行数 | 3579 | 3579 | 一致 |
| `eval-trace-check.py --round R34` | PASS（12 PASS / 2 WARN / 0 FAIL） | PASS（12 PASS / 2 WARN / 0 FAIL） | 一致；两条 WARN 仍是 `R34-design-batch` 与 `F-R34-01`，`DECISIONS.md` 有 2026-11-11 的 D-049 判决行 |
| `doc-code-map.sh --check` | 54/54 | 54/54 | 一致 |
| citation 门 | `blocks=69 checked=29 failed=0 skipped=39 warned=1` | 同值 | 一致 |
| anchor 门 | 105/105 | `anchors=105 resolved=105 zero-def=0 multi-def=0` | 一致 |
| structure / ref 门 | 通过 | 两个工具均输出通过 | 一致 |
| style-lint（文件级） | error=0 | `error级=0` | 一致 |
| density 承载门 | 通过 | 4 个可判章均有码或命令，阻断项通过 | 一致 |
| 宿主测试 | 866 passed / 0 failed / 8 ignored | 库单元测试二进制 866 passed / 0 failed / 8 ignored；整条 `cargo test -p minix-kernel --features mock` 含集成测试与文档测试合计 873 passed / 0 failed / 12 ignored | 一致（FREEZE 的口径是库单元测试） |

复核用的命令与关键输出：

```text
$ sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
3284d47a7f21529626e7a7b8933aa91a37c0501d4abd56244e45b3d503145b97
$ wc -l rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
3579
$ python3 tools/eval-trace-check.py rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap --round R34
汇总: PASS 12 / WARN 2 / FAIL 0
结论: PASS
$ bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --check | tail -1
（共 54 个文件；存在性 54/54 通过）
$ python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
blocks=69 checked=29 failed=0 skipped=39 warned=1
$ bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | tail -1
anchors=105 resolved=105 zero-def=0 multi-def=0
```

代码基线核对：`git diff --stat 5c7134fbd..HEAD -- os/` 输出为空，冻结后到当前 HEAD 之间 Rust 代码没有任何改动；同一区间的文档改动只涉及后续篇目（`02-higher-half-kernel.md`、`03-kmain-cstart.md`、`04-platform-discovery.md`）与评审台账，不触碰本篇。

## 2. 抽样复验（10 条已修项）

抽样规则：优先 P0/P1/P2，覆盖事实、代码、读者三类；冻结轮 R34 取 7 条，早期轮次取 3 条，以检查历史修复没有在后续编辑中丢失。

| 序号 | 发现 ID（原严重度/处置类） | 复验命令与证据 | 结果 |
|---|---|---|---|
| 1 | `RD-R34-27`（P0 / ①，certify #1 新准入） | `grep -c "两种拼写检索均零命中" doc` → 0；`grep -c "boot_params.rs:137" doc` → 1；源码 `os/libs/minix-boot/src/boot_params.rs:137`（生成内部 `BOARD=`）与 `:174`（拒绝外部同名键）均在 | PASS |
| 2 | `RD-R34-01`（P1 / ①，事实） | `grep -c "仓内没有它的读者" doc` → 0；`minix3/minix/kernel` 全树 `grep -rn '"arch"'` → 0 命中；`minix3/minix/drivers/storage/ramdisk/rc:9`、`:46` 两处 `sysenv arch`；`pre_init.c:149` 只有写点 | PASS |
| 3 | `RD-R34-07`（P1 / ①，事实） | 旧串 ``grep -c "wire 信封版本 \`BOOT_HANDOFF_V1_VERSION = 9\`" doc`` → 0；`grep -c "VM_BOOT_HANDOFF_VERSION = 9" doc` → 1；源码 `os/libs/minix-types/src/types/boot.rs:100`、`os/libs/minix-boot/src/handoff_v1.rs:288` 确为两个独立计数器 | PASS |
| 4 | `DESIGN-06`（P1 / ①，代码；早期台账） | `grep -n "fn test_store_kernel_info_rejects_source_overflow\|fn test_store_kernel_info_rejects_startup_overflow" os/kernel/src/lib.rs` → 5998、6011；`grep -n land_slice os/kernel/src/lib.rs` → 1471/1483/1530/1549/1552 共五处公共落地区，加上 `boot_modules` 与 `param_buf` 两处专用落地区，合计 7 处；实跑 `cargo test -j 1 -p minix-kernel --features mock test_store_kernel_info_rejects_source_overflow` → 1 passed | PASS |
| 5 | `DESIGN-12`（P1 / ①，代码；早期台账） | `os/arch/src/arch/pt_alloc.rs:130` 有 `assert!(is_registered(), "pt_alloc::alloc_pt_page called before register")`；`os/libs/minix-boot/src/handoff_v1_read.rs:432` 的 `wire_module_to_boot_module` 返回 `Result<BootModule, HandoffV1ReadError>`；实跑 `cargo test -j 1 -p minix-boot` → 196 passed / 0 failed | PASS |
| 6 | `FACT-04`（P0 / ①，代码；Round 2 台账） | 文件 `os/kernel/tests/store_before_alloc_pin.rs` 存在；实跑 `cargo test -j 1 -p minix-kernel --features mock --test store_before_alloc_pin` → 1 passed / 0 failed | PASS |
| 7 | `RD-R34-10`（P2 / ④，读者 A） | `grep -c "excludes VM — C" os/kernel/src/vm_handoff.rs` → 0（新句为 `excludes VM as a Rust-side choice — C's`）；文档 §3.8 第 2218 行已写“C 的 `kinfo.module_list[]` 本身含 VM，VM 条目在回收后被置零”；源码 `minix3/minix/kernel/arch/i386/protect.c:450-451` 的 `mod->mod_end = mod->mod_start = 0;` 与“回收后置零”一致 | PASS |
| 8 | `RD-R34-12`（P2 / ④，读者 B） | 旧串 ``grep -c "镜像在 \`boot_validate_and_prepare\` 里既进不了" doc`` → 0；新串 ``grep -c "镜像在规划层 \`plan_boot_pages\`" doc`` → 1；源码 `os/kernel/src/boot_policy.rs:98` 的 `plan_kernel_mapping` 与 `:206` 的 `plan_boot_pages`，`:132` 返回 `KernelMapError::NoKernelPageSize` | PASS |
| 9 | `RD-R34-15`（P2 / ④，读者 C） | 旧串归零无法核对（该行只记新串）；新串 ``grep -c "非 strict 在 \`memmap_sources\` 非空时也查" doc`` → 1（台账记 2）；等价格雷 ``grep -c "（非 strict）在 \`memmap_sources\` 非空时也查" doc`` → 1，两处合计 2；源码 `os/libs/minix-boot/src/kernel_info.rs:503-513` 的 `else if !self.memmap_sources.is_empty()` 分支成立 | PASS（实质成立；台账记录的命令计数不复现，见 §4 附注 1） |
| 10 | `RD-R34-02`（P2 / ①，事实） | `grep -c "不是首条记录" doc` → 1；`minix3/minix/kernel/arch/i386/pre_init.c:47-59` 的删除扫描逐字核对：`while (*p++)` 结束后 `p` 已越过记录终止 NUL，随后多出的一次 `p++` 会跳过下一条记录的首字节，因此“`xboard=2` 在非首条时中段 `board=2` 被命中、留下 `xarch=i386`”的推演成立 | PASS |

补充的整门重放（与抽样互不替代）：

```text
$ python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
blocks=69 checked=29 failed=0 skipped=39 warned=1
$ cd os && cargo test -j 1 -p minix-kernel --features mock 2>&1 | grep "Running unittests" -A1
   Running unittests src/lib.rs (target/debug/deps/minix_kernel-409b9b04c1351287)
test result: ok. 866 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out
```

## 3. 关键事实再核对（3 条）

### 3.1 `kinfo` 的段归属是 `.unpaged_bss`

- **为什么关键**：§2.4 用“`kinfo` 是零初值的全局对象”解释 `else` 分支不写 `mmap_size` 仍能正确工作，这条前提同时支撑本篇对“低侧 `.unpaged_*` 与高侧普通段”的整个判据链。段归属如果错了，零初值论证与 `k_initial_stktop` 的落段结论都要重写。
- **文档位置**：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:499`
- **源码证据**：
  - `minix3/minix/kernel/arch/i386/pre_init.c:20`：`kinfo_t kinfo;`
  - `minix3/minix/kernel/arch/i386/Makefile.inc:51`：`.for unpaged_obj in head.o pre_init.o ...`，`pre_init.o` 被改名为 `unpaged_pre_init.o`
  - `minix3/minix/kernel/arch/i386/kernel.lds:19`：`.unpaged_bss ALIGN(4096) : { unpaged_*.o(.bss COMMON) }`
- **结论**：成立。`pre_init.o` 的 `.bss` 只能被 `.unpaged_bss` 的 `unpaged_*.o` 通配收走，落点唯一。

### 3.2 `arch=` 的内核侧写点与用户态读者链

- **为什么关键**：这是 R34 确认轮唯一一条 P1 事实修复的核心。文档据此把 `param_buf` 的遮蔽后果收窄为“内核不读 `arch=`，用户态 RAMDS 的 `rc` 经 `sysenv` 读第一条”，并把它与 `board=` 的板卡身份后果分开。若内核其实有 `arch=` 读者，整段因果推演都要改。
- **文档位置**：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:365`（同一事实在 457、465 行复述）
- **源码证据**：
  - 写点：`minix3/minix/kernel/arch/i386/pre_init.c:149`（`ARCHVARNAME` 唯一使用处之一，另一处是 earm 的同名写点）；`ARCHVARNAME` 定义为 `minix3/minix/include/minix/const.h:138` 的 `"arch"`
  - 内核读点：`grep -rn '"arch"' minix3/minix/kernel/` → 0 命中
  - 用户态读者：`minix3/minix/drivers/storage/ramdisk/rc:9`、`:46` 的 `sysenv arch`；`minix3/minix/commands/sysenv/sysenv.c:59` 的 `svrctl(PMGETPARAM, ...)`；`minix3/minix/servers/pm/main.c:169` 的 `sys_getmonparams` 缓存；`minix3/minix/servers/pm/misc.c:374` 的 `find_param`
- **结论**：成立。

### 3.3 `VM_BOOT_HANDOFF_VERSION = 9` 与 `BOOT_HANDOFF_V1_VERSION = 9` 是两个独立计数器

- **为什么关键**：§2.4.1.4 把 `kern_ident` 的删除归到 `VM_BOOT_HANDOFF_VERSION = 9`。两个版本号在冻结文档里都写成 9，混同会得出错误的 wire 契约（例如误以为 arena 信封同时删了 `kern_ident`，或误以为 v9 是同一个协议版本）。这是 R34 的 P1 修复。
- **文档位置**：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:740`
- **源码证据**：
  - `os/libs/minix-types/src/types/boot.rs:100`：`pub const VM_BOOT_HANDOFF_VERSION: u32 = 9;`，其上方注释第 96 至 99 行写明 v9 删除 `kern_ident`、`kern_ident_count` 与 `VM_BOOT_HANDOFF_MAX_IDENT`
  - `os/libs/minix-boot/src/handoff_v1.rs:288`：`pub const BOOT_HANDOFF_V1_VERSION: u32 = 9;`，其上方注释第 282 至 287 行讲的是 arena 语义工作分区删除、信封从 184 字节缩到 160 字节
- **结论**：成立。两个常量的删除项与变更理由互不相同。

## 4. 新发现

本次扫描没有发现冻结后新引入的 P0、P1、P2。以下两条是台账证据质量问题，不构成新的缺陷等级，也不改变 PASS 结论。

1. `RD-R34-15` 的复验串写的是“`grep -n "非 strict 在 \`memmap_sources\` 非空时也查" doc` → 2 命中”，当前冻结文档上该字面只命中 1 次；第 2 处在 §3.4.1（第 1625 行）写作“`KernelInfo::validate`（非 strict）在 `memmap_sources` 非空时也查”，多了“（非 strict）”限定，与 §4.4 的句子不是同一字面。另记的 `sed -n '2714p' doc` 因同轮后续插入已漂移到第 2726 行。实质断言两处都在，源码 `kernel_info.rs:503-513` 也支持，修复本身没有回退；缺的是“一条能在冻结文档上原样重放的命令”。
2. `RD-R34-11` 的复验串“两种拼写检索均零命中 → 1”在当前文档上为 0，因为该句已被同轮 certify #1 的 P0（`RD-R34-27`）删除并改写为点名 `boot_params.rs:137` 与 `:174` 的版本。这是修复链的正常取代，不是回归；登记在这里是为了说明为什么该行的复验证据不再逐字可重放。

## 5. 局限声明

- **同 agent 复核**：本次审查与冻结轮的面板、certify 使用同一模型。抽样命令与七道门全部重放，但没有做跨模型独立复核；同一模型对同一段文字的系统性误读无法由本方法发现。
- **未运行真机链**：QEMU 三架构引导链没有重跑，`FREEZE.md` 也明确本轮未跑（不触 arch/boot 行为门）。C 侧行为核对是源码直读（`minix3/`），没有编译或运行 Minix3。
- **宿主测试范围**：只跑了默认 `mock` 形态的 `cargo test -p minix-kernel --features mock`、`cargo test -p minix-boot` 与一个 `boot-shim --lib` 计数；跨架构镜像构建没有重跑。
- **行号漂移**：本文件引用的文档行号与源码行号都是 2026-11-13、HEAD `a4afb4927` 的实测值。冻结文档在同轮后续编辑中发生过行号移动，不能拿本文件的数字去反推历史台账的记录时刻。
- **结论含义**：`PASS` 表示“确认轮口径下没有 open 项、抽样复验与关键事实均未回退”，不是“零未知缺陷保证”。出现新的 P0/P1 事实或行为缺陷证据时仍应重开。

# D01 回归审查（R35 确认轮后）

- 结论：FAIL
- 审查日期基线：2026-11-19，HEAD `2e82208e2`
- 审查方式：只读复核；除本文件外未修改仓库任何文件
- 摘要：文档基线、六门、R35/R31/R34 轨迹门与制品存在性全部一致；FAIL 由 5 项轨迹内部不一致触发（`loop.md` 28 根因数与 `metrics.md`/台账 78 的矛盾、`自引入: 2` 的 origin 全为 R35、7 条设计 P3 不在 `OPEN-DEBT` 行、RD-R35-06 复验差一行、`certify 新准入 = 0` 无法独立复算）。

## 1. FREEZE 记录复核

| 复核项 | FREEZE 记录 | 本次实测 | 结论 |
|---|---|---|---|
| 审查基线 HEAD | — | `git rev-parse --short HEAD` → `2e82208e2` | 一致 |
| 文档 sha256（R35） | `1a71cb515d29db66735255a362f1804369c3d5854ce29185a615f57f3b4d68d7` | `sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 同值 | 一致 |
| 文档行数（R35） | 3582 | `wc -l` → `3582` | 一致 |
| R34 历史 sha256 / 行数 | `3284d47a7f21529626e7a7b8933aa91a37c0501d4abd56244e45b3d503145b97` / 3579 | `git show 5c7134fbd:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 接 `sha256sum` → 同值；接 `wc -l` → `3579` | 一致 |
| 冻结提交 | `5c7134fbd` | `git cat-file -t 5c7134fbd` → `commit` | 一致 |
| 关联代码 | `doc-code-map.sh --check` 54/54 | `（共 54 个文件；存在性 54/54 通过）` | 一致 |
| 面板计数 | FINDING 35 / SUGGESTION 37 / CLAIM 6 | `panel/round35/INDEX.md`：4+9+13+4+2+3 = 35；2+1+14+9+11 = 37；6 | 一致 |
| 严重度 | 无 P0，2 条 P1 | 台账 78 行：P1=2、P2=16、P3=60，P0=0 | 一致 |
| certify 回执 | fact 21/21 CLOSED、reader 12/12 CLOSED | 两文件头部同值，STILL-BROKEN 均为 0 | 一致 |
| `--round R35` | PASS（13 PASS / 3 WARN / 0 FAIL） | 同值，结论 PASS | 一致 |
| `--round R31` / `--round R34` | 均恢复 PASS | 两者均为 13 PASS / 3 WARN / 0 FAIL，结论 PASS | 一致 |
| 六门读数 | 见 FREEZE 第 50 行 | 见 §3，逐项同值 | 一致 |
| 归档审计 | PASS 24 / WARN 5 / FAIL 100，均在 R17–R30 | 同值；FAIL 分类为 v3-evidence 80 + round-closure 15 + v3-template 4 + metrics 1，无一行提及 R31+ | 一致 |
| 制品链 | `panel/round35/` 7 文件 + `INDEX.md`；certify 两回执；R31 冷读回执 | 目录有 8 项（7 个被索引报告 + `INDEX.md`）；certify 三文件均在 | 一致 |
| `OPEN-DEBT` 的 `R35-design-batch` | 有 D-060 判决行与 2026-11-19 日期 | `OPEN-DEBT.md:22` 有 `用户批准 2026-11-19（代理裁决 D-060）`；`DECISIONS.md:141` 有 D-060 行 | 核心一致；批内容枚举见 §2.1-3 |
| 自引入 2 条 | `metrics` 记 `自引入: 2` | `metrics.md:267` 同值；文档两处修复均在 | 数值一致；origin 口径见 §2.1-2 |

文档层与制品存在性全部一致。FAIL 来自冻结轨迹的内部计数与证据指针不一致（§2.1、§4）。

## 2. 轨迹门与制品链

```text
$ cd /home/xzhao/github/minix-rs
$ RUN=rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap
$ python3 tools/eval-trace-check.py "$RUN" --round R35 | tail -3
汇总: PASS 13 / WARN 3 / FAIL 0
结论: PASS

$ python3 tools/eval-trace-check.py "$RUN" --round R31 | tail -3
汇总: PASS 13 / WARN 3 / FAIL 0
结论: PASS

$ python3 tools/eval-trace-check.py "$RUN" --round R34 | tail -3
汇总: PASS 13 / WARN 3 / FAIL 0
结论: PASS

$ python3 tools/panel-index.py "$RUN/panel/round35" --check
panel-index: 7 个文件哈希与计数一致
```

R35 的三条 WARN 分别是 `R34-design-batch`、`F-R34-01`、`R35-design-batch` 的已批准延期提示；三者均有 `DECISIONS.md` 判决行。R31/R34 当前也是 13/3/0，比 R34 冻结时的 12/2 多一条 WARN，多出的是 R35 新登记的 `R35-design-batch`，属于时间演进，不是 R34 节的事实错误。

R31 补清点的冷读回执与 FREEZE 第 47 行一致：

```text
$ grep -n "核验条数\|CLOSED：\|STILL-BROKEN" "$RUN/certify/round31-backfill-coldread.md" | head -3
4:- 核验条数：53
5:- CLOSED：33；STILL-BROKEN：20
```

归档审计（不带 `--round`）：

```text
$ python3 tools/eval-trace-check.py "$RUN" | tail -2
汇总: PASS 24 / WARN 5 / FAIL 100
结论: FAIL

$ python3 tools/eval-trace-check.py "$RUN" | grep '^\[FAIL\]' | sed -E 's/^\[FAIL\] ([a-z0-9-]+):.*/\1/' | sort | uniq -c
      1 metrics
     15 round-closure
     80 v3-evidence
      4 v3-template

$ python3 tools/eval-trace-check.py "$RUN" | grep '^\[FAIL\]' | grep -E 'Round 3[1-9]|R3[1-9]|R35|R34|R33|R32|R31'
（无输出）
```

这与 FREEZE 第 53 行的 24/5/100 及分类分布一致。

制品链：

```text
$ ls "$RUN/panel/round35"
INDEX.md  design-d2.md  fact-a.md  fact-b.md  reader-a.md  reader-b.md  reader-c.md  refute.md
$ ls "$RUN/certify"
round31-backfill-coldread.md  round35-fact.md  round35-reader.md
$ ls prompt/EXECUTE-D01-D06-CONVERGENCE.md
prompt/EXECUTE-D01-D06-CONVERGENCE.md
$ git cat-file -t 562b9fe6e
commit
```

`FREEZE.md` 与 D-060 引用的用户指令文件与提交都存在；没有发现 FREEZE 声称但工作树缺失的制品。

### 2.1 FAIL 清单（轨迹层）

1. **R35 根因数与新准入计数自相矛盾。** `metrics.md:263-264` 写“唯一根因数 78（RD-R35-01..78，逐发现立根因）/ 本轮新准入 78”，`metrics.md:266` 写“重复合并: 0”；`loop.md:2079` 写“去重后 28 个根因（RD-R35-01..28）。合并 7 条：B-02 并入 RD-R35-01、C-02 并入 RD-R35-02、FACT-B-S01 并入 RD-R35-03、CLAIM-01..05 并为一行”，`loop.md:2104` 又写“与 `metrics.md` 的 `## R35 确认轮` 块一致：发现总数 78、唯一根因数 28、本轮新准入 28”。`defect-ledger.md` 的 R35 节实际是 78 行 `RD-R35-01..78`。28 与 78 只能有一个成立；28 恰是 R34 的发现数（`metrics.md` R34 块“发现总数: 28 / 唯一根因数: 28”），`loop.md` 的 §3/§6 像是未从 R34 模板更新。`--round R35` 仍 PASS，说明该门没有把 `loop.md` §6 的 28 与 `metrics.md` 的 78 对账。
2. **`自引入: 2` 与台账 origin 列不一致。** `loop.md:2082` 明写两条自引入“按 §4.0c 的 origin 规则记 R31 引入、R35 发现”；但 `defect-ledger.md` 的 78 行 origin 列全为 R35（awk 计数 `R35 78`），例如“四条架构”行 `defect-ledger.md:2357`（RD-R35-28）的 origin 是 R35。按 §4.0c，若 2 条 origin R31，则“本轮新准入”应为 76 而不是 `metrics` 的 78；台账也没有任何 `自引入` 标记。该指标无法逐行复算。
3. **`R35-design-batch` 的 7 条设计 P3 不在 `OPEN-DEBT` 行内。** `FREEZE.md:51` 写该批为“6 条设计 P2 + 7 条设计 P3 + 1 条 ARCH 提案”；`OPEN-DEBT.md:22` 只列 `RD35-DESIGN-D2-01..06` 与 `ARCH-PROPOSAL-01`；`DECISIONS.md:141`（D-060）把 6 条 P2 与 ARCH 记入该批、把“读者/设计 P3 进 suggestions”；7 条 P3 实际在 `suggestions.md:113`（`RD35-DESIGN-D2-07..13`）。台账 `RD-R35-20..26` 的复验却写“已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行”，对 DESIGN-D2-12 的实测 grep 在 `OPEN-DEBT.md` 与 `DECISIONS.md` 均无命中（rc=1）。
4. **抽样行 RD-R35-06 的 path:line 差一行，且无对应 certify 条目。** `defect-ledger.md:2335` 写“改动点见文档 `01-boot-shim-bootstrap.md:2584`（§4.3 `num_pages = ...` 条目）”；实测 2584 是下一条“分配出来的页里…”，`num_pages` 条目在 2583。`certify/round35-fact.md` 也没有 FACT-B-02 的对应小节（grep 无命中），与台账“由 certify … 对应条目回读确认”的写法不符。
5. **`certify 新准入 = 0` 无法独立复算。** `metrics.md:265` 写 0；两份 certify 回执只写“CLOSED 21 / STILL-BROKEN 0”“CLOSED 12 / STILL-BROKEN 0”，没有新准入字段或新根因清单。台账也没有 certify 准入列。因此这一项只能引用 metrics 的自报值，不能按任务要求逐行复算。

## 3. 六门读数

| 门 | FREEZE 读数 | 实测命令 | 实测输出 | 结论 |
|---|---|---|---|---|
| anchor-resolve | 108/108 | `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `anchors=108 resolved=108 zero-def=0 multi-def=0`，EXIT=0 | 一致 |
| citation | `blocks=69 checked=29 failed=0 skipped=35 warned=5` | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 同值，EXIT=0 | 一致 |
| structure | 通过 | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析`，EXIT=0 | 一致 |
| ref | 通过 | `bash tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `doc-ref-check: ... 内部章节引用均可解析`，EXIT=0 | 一致 |
| style-lint（文件级） | error=0 | `bash tools/doc-style-lint.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `命中汇总：` 后 `error级=0`，EXIT=0 | 一致 |
| density | 通过 | `bash tools/doc-code-density-check.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `承载/诊断章(可判) 4 / 全部章 7；豁免 0`；`承载门阻断项通过（4 个可判章均有码/命令）`，EXIT=0 | 一致 |
| doc-code-map | 54/54 | `bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --check` | `（共 54 个文件；存在性 54/54 通过）`，EXIT=0 | 一致 |

style-lint 按任务口径只跑文件级；`--diff` 级未跑，FREEZE 的 `error=0` 也是文件级口径。

## 4. 未闭环与债务

`metrics.md` 的 R35 块（第 260–280 行）关键行：

```text
发现总数: 78（FINDING 35 + SUGGESTION 37 + CLAIM 6...）
唯一根因数: 78（RD-R35-01..78，逐发现立根因）
本轮新准入: 78
certify 新准入: 0
重复合并: 0（逐发现立根因；跨轮同根关系由面板原文与 OPEN-DEBT 指针表达）
自引入: 2（§4.1 的「四条架构」与 §4.6 的 mock 分叉段...）
处置: ① 17 / ② 13 / ④ 8 / ⑤ 0 / ⑥ 0 / FP 5 / SUGG 35
未闭环: 0
轮型: confirmation
轮结果: CONVERGED
```

台账 R35 节的逐行聚合：

```text
$ awk 'NR>=2329 && NR<=2407 && /^\| RD-R35-/ {n++; split($0,a,"|"); gsub(/^ +| +$/,"",a[8]); c[a[8]]++} END {print "rows="n; for (k in c) print k, c[k]}' defect-ledger.md
rows=78
SUGG 35
① 17
② 13
④ 8
FP 5

$ awk 'NR>=2329 && NR<=2407 && /^\| RD-R35-/ {split($0,a,"|"); gsub(/^ +| +$/,"",a[6]); s[a[6]]++} END {for (k in s) print k, s[k]}' defect-ledger.md
P2 16
P3 60
P1 2

$ awk 'NR>=2329 && NR<=2407 && /^\| RD-R35-/ {split($0,a,"|"); gsub(/^ +| +$/,"",a[4]); c[a[4]]++} END {for (k in c) print k, c[k]}' defect-ledger.md
R35 78

$ awk 'NR>=2329 && NR<=2407 && /^\| RD-R35-/ {split($0,a,"|"); gsub(/^ +| +$/,"",a[8]); if (a[8]=="②") {n++; if ($0 ~ /D-060/ && $0 ~ /2026-11-19/) ok++}} END {print "② rows="n", with D-060+date="ok}' defect-ledger.md
② rows=13, with D-060+date=13

$ awk 'NR>=2329 && NR<=2407 && /^\| RD-R35-/ {split($0,a,"|"); gsub(/^ +| +$/,"",a[8]); if (a[8]=="⑤") n++} END {print "⑤ rows="n}' defect-ledger.md
⑤ rows=
```

`发现总数` 可由处置七项复算：17 + 13 + 8 + 0 + 0 + 5 + 35 = 78，与台账行数一致。

`未闭环 = 0` 的复算结论：

- `⑤ = 0`；13 条 `②` 行的处置文本都含 `D-060` 与 `2026-11-19`；`eval-trace-check --round R35` 输出 `[PASS] open-debt: OPEN-DEBT 无未销账行` 与 `[PASS] metrics: 未闭环 = 0`。
- 但“逐行严格复算到 `OPEN-DEBT`”不成立：7 条设计 P3（`RD-R35-20..26`，DESIGN-D2-07..13）只在 `suggestions.md:113` 的批次行，`OPEN-DEBT.md` 的 `R35-design-batch` 行不含它们；它们的台账复验指向不存在的 `OPEN-DEBT` 条目（§2.1-3）。因此 `未闭环=0` 只在“计数/工具口径”成立，不能按每一条 `②` 行的复验指针原样落账。

`certify 新准入 = 0` 的复算结论：

- `certify/round35-fact.md` 头部：`核验条数：21`、`CLOSED：21；STILL-BROKEN：0`。
- `certify/round35-reader.md` 头部：`核验条数：12`、`CLOSED：12；STILL-BROKEN：0`。
- 两份回执都没有“新准入”字段，也没有列出新根因；台账没有 certify 准入列。**无法独立复算**，只能引用 `metrics.md:265` 的自报值。

`自引入 = 2` 的复算结论：

- `metrics.md:267` 有值 2，文档两处修复可验证（`grep -nE '四(条|个|种)架构|四个架构' doc` 无命中；doc:2933 有“两处语义分叉是本仓测试替身的已知缺口”）。
- 但台账 78 行 origin 全为 R35；`loop.md:2082` 要求两条自引入记 origin R31。按 §4.0c，`本轮新准入` 应为 76；台账无法复算 78。**该指标与 origin 列不一致。**

`OPEN-DEBT` 与 `DECISIONS`：

```text
$ grep -n "R35-design-batch" OPEN-DEBT.md
22:| R35-design-batch | P2 | **用户批准 2026-11-19（代理裁决 D-060）延期至下一 design batch**：RD35-DESIGN-D2-01..06 六项设计改进（...）与 ARCH-PROPOSAL-01（...）...

$ grep -n "D-060" rewrite-notes/coordination/review-eval/DECISIONS.md | cut -c1-160
141:| D-060（2026-11-19，代理裁决，依用户指令文件授权） | ... ③ R35 确认轮的全部新准入按 §3.6 处置：... 6 条设计 P2 与 ARCH-PROPOSAL-01 记 `R35-design-batch` 延期（§4.0f 出口 2），读者/设计 P3 进 suggestions ...
```

FREEZE 的核心声明“`OPEN-DEBT` 的 `R35-design-batch` 有 D-060 判决行与 2026-11-19 日期”成立；不成立的是 `FREEZE.md:51` 把 7 条设计 P3 也算进该 `OPEN-DEBT` 行的枚举内容。按 D-060，这 7 条 P3 的去向是 `suggestions.md`，`OPEN-DEBT` 行只承担 6 条 P2 + 1 条 ARCH。

## 5. 一致性抽查（5 行）

抽样方法：在 `defect-ledger.md` 的 R35 节按处置类分层随机抽取，①/②/④/SUGG 各 1 行，第 5 行从 FP 行随机抽取。

| # | 台账行（严重度/处置类） | 台账“证据/复验”列的记录 | 重放结果 |
|---|---|---|---|
| 1 | RD-R35-06（P3/①，`defect-ledger.md:2335`） | 改动点 `doc:2584`（§4.3 `num_pages` 条目）；由 `certify/round35-fact.md` 对应条目回读 | **未命中**：2584 是下一条；`num_pages` 在 2583；certify 无 FACT-B-02 小节 |
| 2 | RD-R35-25（P3/②，`defect-ledger.md:2354`） | “已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行” | **未命中**：OPEN-DEBT 与 DECISIONS 均无 `DESIGN-D2-12`；仅 `suggestions.md:113` 的 `07..13` 批次行覆盖 |
| 3 | RD-R35-33（P1/④，`defect-ledger.md:2362`） | §3.4.1 第 3 层；由 `certify/round35-reader.md` 回读 | **命中**：doc:1648 与 certify 第 1 节的 `doc:1648` 摘录一致 |
| 4 | RD-R35-45（P3/SUGG，`defect-ledger.md:2374`） | 已登记 `suggestions.md` 的 R35 段；镜像条目见 `reader-a.md` | **命中（按范围）**：`reader-a.md:115` 有 `## SUGGESTION A-07`；`suggestions.md:110` 的 `RD35-A-01..A-14` 覆盖它，但 R35 段未逐条列 `A-07` |
| 5 | RD-R35-74（P3/FP，`defect-ledger.md:2403`） | 反证失败；源码锚见 `panel/round35/refute.md` 的 CLAIM-02 | **命中**：CLAIM-02 的 `grep -rn "alloc_lowest" minix3/` 正好 4 个命中，`pg_utils.c:65-84` 与 `:81` 的 `cut_memmap(cbi, lowest, len)` 一致 |

重放命令与关键输出：

```text
# 1) RD-R35-06
$ awk 'NR==2584 {print NR": "$0}' doc
2584: - 分配出来的页里，`data.len()` 之后的余量被显式清零...
$ awk 'NR==2583 {print NR": "$0}' doc | cut -c1-120
2583: - `num_pages = (data.len() + 4095) / 4096`：向上取整到页。... A=4095 ...
$ grep -n "FACT-B-02" certify/round35-fact.md
（无输出，rc=1）

# 2) RD-R35-25
$ grep -n "DESIGN-D2-12" OPEN-DEBT.md
（无输出，rc=1）
$ grep -n "DESIGN-D2-12" rewrite-notes/coordination/review-eval/DECISIONS.md
（无输出，rc=1）
$ grep -n "DESIGN-D2-07..13" suggestions.md
113:| RD35-DESIGN-D2-07..13 | 设计 P3 七项（...） | 随 `R35-design-batch`（D-060）评估 |

# 3) RD-R35-33
$ awk 'NR==1648 {print NR": "$0}' doc | cut -c1-160
1648: 3. **OS 内存语义分类层**：... **现行实现把 Special 展开成两档落点**：MMIO 类走 `DevicePath`...
$ grep -n "1648" certify/round35-reader.md
9:- 证据：`...01-boot-shim-bootstrap.md:1648` 原文摘录：「...」

# 4) RD-R35-45
$ grep -n "A-07" panel/round35/reader-a.md
115:## SUGGESTION A-07
$ grep -n "A-07" suggestions.md
40:| RD32-READER-A-07 | ... | 已修 |
55:| RD33-READER-A-07 | ... | 已修 |
$ sed -n '110p' suggestions.md
| RD35-A-01..A-14 | 读者 A 的 14 条措辞/指针/示例建议 | 登记不实施（读者 P3） |

# 5) RD-R35-74
$ grep -rn "alloc_lowest" minix3/
minix3/minix/kernel/arch/earm/include/arch_proto.h:37:phys_bytes alloc_lowest(kinfo_t *cbi, phys_bytes len);
minix3/minix/kernel/arch/i386/arch_smp.c:76:	trampoline_base = alloc_lowest(&kinfo, tramp_size);
minix3/minix/kernel/arch/i386/pg_utils.c:65:phys_bytes alloc_lowest(kinfo_t *cbi, phys_bytes len)
minix3/minix/kernel/arch/i386/include/arch_proto.h:204:phys_bytes alloc_lowest(kinfo_t *cbi, phys_bytes len);
$ awk 'NR>=63 && NR<=85 {print NR": "$0}' minix3/minix/kernel/arch/i386/pg_utils.c | grep cut_memmap
81:	cut_memmap(cbi, lowest, len);
```

抽样结果：2 行完全命中（RD-R35-33、RD-R35-74），1 行按范围命中（RD-R35-45），2 行未按台账原样命中（RD-R35-06 差一行；RD-R35-25 的指针物不在 `OPEN-DEBT`/`DECISIONS`）。

## 6. 局限声明

- **同 agent 复核**：本次审查与冻结轮的面板、certify 使用同一模型，没有跨模型独立复核；同一模型对同一段记录的系统性误读无法由本方法发现。
- **未跑行为门**：没有重跑 QEMU、`cargo test` 或三架构构建；本轮审查对象是冻结记录/轨迹/worktree 的一致性，不是文档正文的再评审，也不是行为回归。
- **未重新发现文档缺陷**：本次没有对文档正文作新的 P0/P1 判定；所有 FAIL 项都是记录一致性（计数、origin、指针、回执）问题。文档层基线、六门与制品存在性均与 FREEZE 一致。
- **行号时效**：本文件引用的行号是 2026-11-19、HEAD `2e82208e2` 的实测值；后续编辑会使台账里的 path:line 漂移，RD-R35-06 的 2584/2583 之差就是这类漂移的一个实例。
- **无法独立复算的两项**：`certify 新准入 = 0` 没有回执字段或台账列可核；`自引入 = 2` 与 origin 列不一致，按 §4.0c 无法从台账复算出 78 个新准入。`未闭环 = 0` 可在处置类计数与工具口径复算，但不能把 7 条设计 P3 逐行复算到 `OPEN-DEBT` 行。
- **冷启动派发不可验证**：两份 certify 回执的文件时间（21:32、21:39）晚于面板文件，但“本轮新派发的冷启动子 agent”是过程声明，文件内容本身不能证明；本次只核回执内容与计数。
- **结论边界**：若只审 `FREEZE.md` 的文档层断言（sha、行数、六门、制品存在性），全部一致；本次 FAIL 的对象是包含 `loop.md`、`metrics.md`、`defect-ledger.md` 的冻结轨迹包。轨迹包内的 28/78 矛盾与 origin 矛盾必须先消解，才能把 R35 的 `CONVERGED` 作为可复核结论接受。
- **写入范围核对**：`git status --porcelain` 中本任务涉及的改动只有本文件（`M .../REGRESSION.md`）；仓库另有审查前已存在的未跟踪文件（`prompt/review-loop-r26-r29-report.md`、`rewrite-notes/01-stage-kernel/01-design-learn.md`、`rewrite-notes/coordination/review-eval/meta/01-boot-shim-bootstrap-r26.md`），本次未触碰。

## R35 回归审计的补修记录（主 agent，append-only）

上面这一节是独立审计者的原始结论（FAIL，5 项轨迹不一致）。按 §4h「P2 及以下当场修复 + 回归节记录」，5 项已全部处置，提交 `94a254231`：

| # | 审计发现 | 处置 | 复验 |
|---|---|---|---|
| 1 | `loop.md` 写 28 根因/28 新准入，与 metrics/台账的 78 不一致 | loop §3 改为「逐发现立根因 78 个」并说明不合并的理由（面板 ID 与历史轮重名）；§6 数字同步 | `python3 tools/eval-trace-check.py <run-dir> --round R35` → PASS 13 / WARN 3 / FAIL 0 |
| 2 | 两条自引入项的 origin 全记 R35，新准入应为 76 | RD-R35-28 与 RD-R35-38 的 origin 改 `R31；last_seen: R35`；metrics `本轮新准入: 76` | 同上门的 `new_roots` 校验通过 |
| 3 | FREEZE 称 OPEN-DEBT 含 7 条设计 P3，实际只在 suggestions | OPEN-DEBT 的 `R35-design-batch` 补入 RD35-DESIGN-D2-07..13 七项 | `grep -n "七项设计 P3" OPEN-DEBT.md` 命中 |
| 4 | RD-R35-06 证据指针 `doc:2584`，实际 2583 | 台账改为 2583 | `sed -n '2583p' <doc>` 命中反事实段落 |
| 5 | `certify 新准入 = 0` 无字段可复算 | loop §5 补计算口径：certify 输入闭集 = 21+12 个修改点，回执全 CLOSED、无新增修改点，故准入根因数 0；台账无 certify 角色行是该口径的结果 | 口径说明见 loop §5 |

补修后重新实测：`--round R31/R32/R33/R34/R35` 五轮逐轮 PASS；anchor 108/108、citation `checked=29 failed=0`、doc-code-map 54/54、panel-index 7 文件哈希一致。
