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
