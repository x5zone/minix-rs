# D02 回归审查 —— 02-higher-half-kernel（FREEZE 后复核）

- **结论：PASS**
- 审查日期：2026-11-13
- 当前 HEAD：`a4afb4927`
- 冻结记录：同目录 `FREEZE.md`（冻结提交 `809ac212b`，确认轮 R2，工作流 edition V3.3）
- 审查方式：只读复核；除本文件外未修改仓库任何文件

## 1. FREEZE 记录复核

| 复核项 | FREEZE 记录 | 本次实测 | 结论 |
|---|---|---|---|
| 文档 sha256 | `8b3680a1901bd0af3758d3b905545062c24abf38f50f85b690375adeb8f34994` | 同值 | 一致 |
| 文档行数 | 1280 | 1280 | 一致 |
| `eval-trace-check.py --round R2` | PASS（11 PASS / 1 WARN / 0 FAIL） | PASS（11 PASS / 1 WARN / 0 FAIL） | 一致；WARN 仍是 `certify-fact.md` 无 `## FINDING` 行，`loop.md` 的豁免行存在 |
| `doc-code-map.sh --check` | 28/28 | 28/28 | 一致 |
| citation 门 | `32 块 / 1 检查 / 0 失败 / 31 跳过` | `blocks=32 checked=1 failed=0 skipped=31 warned=0` | 一致 |
| anchor 门 | 55/55，zero-def=0 | `anchors=55 resolved=55 zero-def=0 multi-def=0` | 一致 |
| structure / ref 门 | 通过 | 两个工具均输出通过 | 一致 |
| style-lint（文件级） | error=0 | `error级=0` | 一致 |
| density 承载门 | 通过 | 4 个可判章均有码或命令，阻断项通过 | 一致 |
| 宿主测试数量 | `boot-shim --lib -- --list` 125、`loader.rs` 的 `#[test]` 22、`minix-elf` 的 `#[test]` 23 | 125 passed / 22 个 `#[test]` / 23 passed | 一致 |

复核用的命令与关键输出：

```text
$ sha256sum rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
8b3680a1901bd0af3758d3b905545062c24abf38f50f85b690375adeb8f34994
$ wc -l rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
1280
$ python3 tools/eval-trace-check.py rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel --round R2
汇总: PASS 11 / WARN 1 / FAIL 0
结论: PASS
$ bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md --check | tail -1
（共 28 个文件；存在性 28/28 通过）
$ python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
blocks=32 checked=1 failed=0 skipped=31 warned=0
$ bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md | tail -1
anchors=55 resolved=55 zero-def=0 multi-def=0
$ cd os && cargo test -j 1 -p boot-shim --lib
running 125 tests
test result: ok. 125 passed; 0 failed; 0 ignored
$ cd os && cargo test -j 1 -p minix-elf
running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored
$ grep -c "#\[test\]" os/boot-shim/src/loader.rs
22
```

代码基线核对：`git diff --stat 809ac212b..HEAD -- os/` 输出为空，冻结后到当前 HEAD 之间 Rust 代码没有任何改动；同一区间内本篇文档也没有变化（sha256 一致）。

## 2. 抽样复验（10 条已修项）

抽样规则：优先 P0/P1，覆盖事实、设计、读者三类；R1 取 7 条，R2 确认轮取 3 条，同时检查历史修复没有在后续轮次中丢失。

| 序号 | 发现 ID（原严重度/处置类） | 复验命令与证据 | 结果 |
|---|---|---|---|
| 1 | `RD1-01`（P0 / ①，事实） | `grep -c "load_boot_modules_with_loader" doc` → 0；`grep -c "load_boot_modules_into" doc` → 2；源码 `os/boot-shim/src/loader.rs:422` 为 `pub fn load_boot_modules_into<L: FileLoader>(`；同文件旧名 0 命中 | PASS |
| 2 | `RD1-02`（P0 / ①，事实） | `grep -c "PhysBytes(0)" doc` → 0；文档 §4.4 的 `build_kernel_info` 节选为 11 个实参；源码 `os/kernel-image/src/bootface.rs:427-439` 的调用点与 `os/boot-shim/src/lib.rs:102-115` 的函数签名同为 11 个参数，含 `memmap_sources`、`startup_objects`、`authority` | PASS |
| 3 | `RD2-01`（P1 / ①，事实 A） | `grep -c "外部 multiboot 装载器" doc` → 2（第 58 行射程声明、第 404 行 §3.1）；源码 `minix3/sys/arch/x86/include/loadfile_machdep.h:40` 的 `LOADADDR(a)` 为 `(((u_long)(a)) & 0x07ffffff) + offset`；`minix3/sys/lib/libsa/loadfile_elf32.c:336` 的 `READ(fd, phdr[i].p_vaddr, ...)` 与 `:363` 的 `BZERO((phdr[i].p_vaddr + ...))` 都走 `p_vaddr`，全文 `p_paddr` 只出现在打印函数（163 至 173 行） | PASS |
| 4 | `RD2-02`（P1 / ①，事实 A） | 旧串 `grep -c "链接器不会为此报错" doc` → 0；新串 `grep -c "现行 GNU ld 对 LMA 重叠报错" doc` → 1；独立实验：本机 `GNU ld 2.42` 对两个 LMA 同为 `0x400000` 的输出段直接报错并拒绝产出，退出码 1（实验脚本见下方代码块） | PASS |
| 5 | `RD2-12`（P1 / ①，设计 D2） | `grep -c "注释有删改" doc` → 0；`grep -c "注释有改写" doc` → 4；四处节选头都显式声明注释经过改写 | PASS |
| 6 | `RD2-24`（P1 / ①，读者 certify） | `grep -c "riscv64 测试载体物理视图" doc` → 1；`grep -c "两条路径的起点不同" doc` → 0；前置知识已按腿分列起点 | PASS |
| 7 | `RD2-25`（P1 / ①，读者 certify） | `grep -c "上一处坏在取指" doc` → 0；`grep -c "上一处是停在 hang" doc` → 1；§2.3 的“其一”与“其二”现在同口径 | PASS |
| 8 | `RD1-05`（P1 / ①，事实） | `grep -c "kern_virt_base()" doc` → 0；源码 `os/kernel/src/arch/riscv64/higher_half.rs:93-97` 为字段直读（`kinfo.kern_virt_base.0`、`kinfo.kern_phys_base.0`）；文档中 `kern_virt_base.0` 现有 3 处（台账记 1，属计数漂移，实质修复成立） | PASS（实质成立；计数不复现，见 §4 附注 1） |
| 9 | `RD1-09`（P1 / ①，事实） | `grep -c "pre_init\` 的帧指针" doc` → 0；`grep -c "不保存 \`%rbp\`" doc` → 3；源码 `os/kernel/src/arch/x86_64/higher_half.rs:47` 的 `xor rbp, rbp` 位于 `mov rsp` 之后；`minix3/minix/kernel/arch/i386/head.S:69-70` 先建引导栈再清零 `%ebp`，`:84` 切到 `k_initial_stktop` 后 `call kmain`；`os/.cargo/config.toml` 的全部 target 段与 `os/Cargo.toml` 的 profile 段都没有 `force-frame-pointers`，内核 crate 指纹里的 `rustflags` 为 `[]` | PASS |
| 10 | `RD1-48`（P2 / ①，读者） | 测试名 `test_compute_kernel_layout_rejects_image_without_load_segments` 在 `os/boot-shim/src/loader.rs:604`；`grep -c` 文档 → 1；实跑 `cargo test -p boot-shim --lib` → 125 passed，该测试在册 | PASS |

`RD2-02` 的独立复现脚本（在 `/tmp` 新建目录执行，不触碰仓库）：

```text
$ ld --version | head -1
GNU ld (GNU Binutils for Ubuntu) 2.42
$ ld -T t.ld -o out.elf a.o b.o
ld: section .data LMA [0000000000400000,0000000000400003] overlaps section .text LMA [0000000000400000,000000000040000e]
exit=1
```

其中 `t.ld` 把 `.text` 与 `.data` 两段的 LMA 都写成 `0x400000`，模拟“五处 `AT()` 都写成 `AT(_kern_phys_base)`”的形状；链接器在写出 ELF 之前就报错，`out.elf` 没有生成。这与 §2.1 的“现行 GNU ld 对 LMA 重叠报错并拒绝产出 ELF”一致。

## 3. 关键事实再核对（3 条）

### 3.1 C 侧装载者模型：树内 boot monitor 按 `p_vaddr` 装载，`p_paddr` 只对外部 multiboot 装载器成立

- **为什么关键**：这是 R2 确认轮的 P1 修正，也是全文“三件事必须对齐”中间一列的前提。如果树内加载器其实按 `p_paddr` 摆放，§2.1 的双地址推导、§2.2 的段归属判据和 §4.4 的四条装载路径都要改，而且会直接影响读者判断“谁的装载地址真正生效”。
- **文档位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:58`（射程声明）、`:404`（§3.1 复述）
- **源码证据**：
  - `minix3/sys/arch/x86/include/loadfile_machdep.h:40`：`#define LOADADDR(a) ((((u_long)(a)) & 0x07ffffff) + offset)`
  - `minix3/sys/lib/libsa/loadfile_elf32.c:336`：`nr = READ(fd, phdr[i].p_vaddr, phdr[i].p_filesz);`
  - 同文件 `:363`：`BZERO((phdr[i].p_vaddr + phdr[i].p_filesz), phdr[i].p_memsz - phdr[i].p_filesz);`
  - 同文件 `:163-173` 的 `p_paddr` 只出现在 `I32`/`I64` 打印路径，不在装载路径
- **结论**：成立。树内 boot monitor 的装载地址来自 `p_vaddr` 的低 27 位加偏移；`p_paddr` 的读者只在外部 multiboot 装载器（如 GRUB）一侧，文档的射程声明准确。

### 3.2 现行 GNU ld 对 LMA 重叠报错，拒绝产出 ELF

- **为什么关键**：§2.1 的反事实段用这条工具行为区分“现行工具链拒绝”与“旧工具链静默产出被覆盖的镜像”。如果现行链接器其实不报错，反事实段的后果链（后写段覆盖先写段、`kmain` 入口落在被覆盖的字节上）就缺了前半句，读者会按错误的失败模式排查。
- **文档位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:126`
- **源码与实验证据**：本机 `GNU ld 2.42` 的最小复现（见 §2 的脚本）对两个 LMA 重叠的输出段报 `section .data LMA ... overlaps section .text LMA ...` 并以退出码 1 结束；三份生产脚本的 `AT()` 计数分别为 4 条（`os/kernel-image/x86_64.ld`、`aarch64.ld`、`riscv64.ld`），与 §6.2 的“三份生产脚本各 4 条”一致
- **结论**：成立。文档引用的行为在当前工具链上可复现。

### 3.3 Rust 侧帧指针链在 `kmain` 入口的终止条件

- **为什么关键**：§2.3 与 §4.3 都主张“两侧都在 0 处终止”，并进一步用“生产 `kmain` 不保存 `%rbp`”解释为什么没有“第一个带序言的帧把旧帧指针压进保存槽”这一步。如果本仓其实开启了帧指针，或者 `xor rbp, rbp` 不在换栈之后，这条因果链和 panic 回溯的诊断结论都会失真。
- **文档位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:229`（§2.3）、`:902` 与 `:998`（§4.3）
- **源码证据**：
  - `os/kernel/src/arch/x86_64/higher_half.rs:47`：`xor rbp, rbp`，位于 `mov rsp, {stktop}`、`and rsp, -16` 之后
  - `minix3/minix/kernel/arch/i386/head.S:69-70`：`mov $load_stack_start, %esp` 之后 `mov $0, %ebp`；`:84` 切到 `k_initial_stktop` 后 `call kmain`，即 C 侧在 `pre_init` 返回前已把 callee-saved 的 EBP 恢复为 0
  - `os/.cargo/config.toml` 的全部 target 段没有 `force-frame-pointers`；`os/Cargo.toml` 的 `[profile.dev]` 与 `[profile.release]` 只有 `panic = "abort"`；内核 crate 的构建指纹 `rustflags` 为 `[]`
- **结论**：成立。文档对清零位置与保存槽差异的描述与两侧源码一致。

## 4. 新发现

本次扫描没有发现冻结后新引入的 P0、P1、P2。以下一条是台账证据计数问题，不构成新的缺陷等级，也不改变 PASS 结论。

1. `RD1-05` 的复验串记的是“`grep -n "kern_virt_base.0" doc` → 1”，当前冻结文档上 `grep -c "kern_virt_base.0"` 得 3。旧串 `kern_virt_base()` 确为 0，字段直读的修复本身成立，源码位置也对；差的是“一条仍能逐字给出记录中那个计数的命令”。这不是文档回退，属于台账的字面量与后续同篇编辑不同步。

## 5. 局限声明

- **同 agent 复核**：本次审查与冻结轮的面板、certify 使用同一模型。抽样命令与七道门全部重放，但没有做跨模型独立复核；同一模型对同一段文字的系统性误读无法由本方法发现。
- **未运行真机链**：QEMU 三架构引导链没有重跑。§4.4 的“aarch64 direct-core 低地址进入来自固件按装载地址平移入口”属于固件行为，本次只核对了文档给出的可复现路径（QEMU `-d in_asm` 第一条记录）与代码侧的低地址假设，没有实际执行这条观测。
- **C 装载者模型的证据边界**：树内 boot monitor 的装载行为是源码直读（`loadfile_machdep.h`、`loadfile_elf32.c`），没有编译运行 Minix3 的 boot monitor；`RD2-02` 的重复实验用的是等价的最小链接脚本，不是 Minix3 的真实 `kernel.lds` 全量构建。
- **宿主测试范围**：只跑了 `cargo test -p boot-shim --lib`（125）、`cargo test -p minix-elf`（23）与 `loader.rs` 的测试计数（22）；跨架构镜像构建与 `check-layout.sh` 的三架构实跑没有重跑。
- **行号漂移**：本文件引用的文档行号与源码行号都是 2026-11-13、HEAD `a4afb4927` 的实测值。
- **结论含义**：`PASS` 表示“确认轮口径下没有 open 项、抽样复验与关键事实均未回退”，不是“零未知缺陷保证”。出现新的 P0/P1 事实或行为缺陷证据时仍应重开。

# D02 回归审查（R3 确认轮后）

- **结论：FAIL**
- **基线提交**：`efb179a9f`（`git rev-parse --short HEAD`）
- **审查日期**：2026-11-20
- **审查对象**：D02 `02-higher-half-kernel` 的 R3 冻结记录与最终工作树
- **审查方式**：只读复核；除向本文件追加本节外，未修改任何文件
- **判定口径**：以 HEAD 工作树为实测基线。`FREEZE.md`、`loop.md`、`metrics.md`、台账与 `DECISIONS.md` 的读数或文字与实测命令输出不一致，即逐条记 FAIL。文档技术内容是否仍成立不在本轮重审范围；本轮只核记录、制品链、机械门与抽样复验。

**FAIL 摘要**：

1. `FREEZE.md` 记 `doc-code-map 29/29`，实测 `30/30`。
2. `FREEZE.md` 与 `metrics.md` 记 refute 末尾追加 `## CLAIM 1..6`，实测 `## CLAIM 11..16`。
3. 台账 18 条 SUGG 行（49 条发现）的复验写「已登记 `suggestions.md` 的 R3 段」，但 D02 run 目录与全仓均无此文件。
4. `loop.md` 的 R3 汇总保留旧读数（89 / 34 / ①12 / CLAIM 0），与 `metrics.md` 和台账（95 / 37 / ①13 / CLAIM 6）不一致。
5. `FREEZE.md` 记「9 条读者 P1」，面板实数是 7 条 P1 FINDING，加上 1 条 P1 级 SUGGESTION 也只有 8 条。
6. 台账若干复验行号已漂移：`RD-R3-37` 记 `doc:1072`，该行现在为空；目标串在 `doc:1075`。
7. `DECISIONS.md` 的 D-061 出现两次，D01 的 ARCH 裁决与 D02 的重开裁决共用同一编号。
8. `FREEZE.md`、`metrics.md` 与 `DECISIONS.md` 记 ARCH 提案 ID `RD3-ARCH-01/02`，面板实际标题是 `RD3-A01` 与 `RD3-A02`。

## 1. FREEZE 记录复核

| 复核项 | FREEZE 记录 | 实测 | 结论 |
|---|---|---|---|
| 文档 sha256 | `4f096ea9997eae5aae99ad648967d2b8001256d22e7a63c017d6ea9de16804ce` | 同值（`sha256sum`） | 一致 |
| 文档行数 | 1283（R3 时点） | 1283（`wc -l`） | 一致 |
| R3 发现总数 | R3 节未单列；`metrics.md` 记 95 | 台账 95（FINDING 41 + SUGGESTION 48 + CLAIM 6） | 一致 |
| 唯一根因数 | `metrics.md` 记 37 | 台账 37 行（`RD-R3-01..37`） | 一致 |
| 本轮新准入 | `metrics.md` 记 37 | 台账 37 行 | 一致 |
| 关联代码门 | 29/29 | 30/30 | **不一致（FAIL）** |
| refute 索引形态 | `## CLAIM 1..6` | `## CLAIM 11..16` | **不一致（FAIL）** |
| 读者 P1 数 | 9 | 7 条 P1 FINDING，另有 1 条 P1 级 SUGGESTION | **不一致（FAIL）** |
| 回执实质错误数 | 3 处 | `loop.md` 记 4 处；certify 第 12 条可证 panic 消息顺序错误 | **不一致（FAIL）** |
| ARCH 提案 ID | `RD3-ARCH-01/02` | 面板标题是 `RD3-A01`、`RD3-A02` | **不一致（FAIL）** |
| panel 制品 | 6 文件 + INDEX | 6 文件 + INDEX | 一致 |
| certify | 存在且终判 PASS | 存在；`结论：PASS`、`CLOSED：19` | 一致 |
| D-061 | 有判决行与 2026-11-20 日期 | `DECISIONS.md:143` 有 | 一致（同号冲突见 §4） |

命令与读数：

```
$ sha256sum rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
4f096ea9997eae5aae99ad648967d2b8001256d22e7a63c017d6ea9de16804ce  rewrite-notes/01-stage-kernel/02-higher-half-kernel.md

$ wc -l rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
1283 rewrite-notes/01-stage-kernel/02-higher-half-kernel.md

$ bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md --check
...
（共 30 个文件；存在性 30/30 通过）
exit=0

$ grep -n '^## CLAIM' rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/panel/round3/refute.md
7:## CLAIM RD3-CLAIM-01 — 双地址链接的必要性，以及「谁读 p_paddr」决定脚本形态
28:## CLAIM RD3-CLAIM-02 — 段体按 p_paddr 摆放，BSS 按 memsz 清零
48:## CLAIM RD3-CLAIM-03 — `k_initial_stktop` 落在高侧，`__k_unpaged_` 前缀不是判据
68:## CLAIM RD3-CLAIM-04 — `kern_stack_top` 的算式、2MiB 收口与页粒度策略
89:## CLAIM RD3-CLAIM-05 — 三架构成对跳转的寻址方式、psABI 对齐与屏障
110:## CLAIM RD3-CLAIM-06 — 跳转完成时 PC 的落点与测试载体的视图
137:## CLAIM 11
140:## CLAIM 12
143:## CLAIM 13
146:## CLAIM 14
149:## CLAIM 15
152:## CLAIM 16

$ grep -h '^## FINDING.*（P1）' panel/round3/reader-a.md panel/round3/reader-b.md panel/round3/reader-c.md | wc -l
7
$ grep -h '^## SUGGESTION' -A5 panel/round3/reader-a.md panel/round3/reader-b.md panel/round3/reader-c.md | grep -c '严重度.*P1'
1
```

`doc-code-map` 的 30/30 不是新增文件缺失：30 个路径全部存在并通过。差异来自 R3 文档同时新增了 `os/arch/src/riscv64/paging.rs` 与 `os/arch/src/x86_64/paging.rs` 两个引用，而 `loop.md` 的轮中读数只记了前者（28 → 29），`FREEZE.md` 抄了轮中值，没有按最终文档重算。

CLAIM 编号差异来自最终提交 `efb179a9f`：该提交把 refute 末尾的 `## CLAIM 1..6` 改成 `## CLAIM 11..16`，同步改了台账的 CLAIM 发现 ID 与 `INDEX.md` 的 refute 哈希，但没有回写 `FREEZE.md` 与 `metrics.md`。

## 2. 轨迹门与制品链

本节与 §3 的占位符：`<run-dir>` = `rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel`，`<doc>` = `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`。

| 检查 | 实际命令 | 汇总读数 | 结论 |
|---|---|---|---|
| R3 轨迹门 | `python3 tools/eval-trace-check.py <run-dir> --round R3` | `PASS 11 / WARN 0 / FAIL 0`，结论 `PASS` | 通过 |
| R1 轨迹门 | 同上，`--round R1` | `PASS 11 / WARN 0 / FAIL 0`，结论 `PASS` | 通过 |
| R2 轨迹门 | 同上，`--round R2` | `PASS 11 / WARN 1 / FAIL 0`，结论 `PASS` | 通过；与 FREEZE 的 R2 记录一致 |
| 面板索引 | `python3 tools/panel-index.py <run-dir>/panel/round3 --check` | `panel-index: 6 个文件哈希与计数一致`，exit 0 | 通过 |

R3 的 11 项检查逐行为：

```
[PASS] panel: 面板报告解析到 95 条发现（含轮次子目录）
[PASS] loop-sections: loop.md 九个固定标题齐全
[PASS] round-closure: Round 3 收口小节齐全（### L6 / ### L6.5）
[PASS] ledger: 台账解析到 292 条处置
[PASS] coverage: 所有面板发现都有处置行
[PASS] coverage: 台账没有孤儿行
[PASS] open-debt: OPEN-DEBT 无未销账行
[PASS] metrics: 发现总数与处置之和一致（95）
[PASS] metrics: metrics 各分档与台账本轮逐 ID 聚合一致
[PASS] metrics: 未闭环 = 0，本轮可关闭
[PASS] edition-trial: edition V3.3 试验窗口 1/3
```

`edition-trial` 读的是 V3.3 修改轮的计数（确认轮按设计不计入），所以 R3 标 V3.4 与该行不矛盾；它不是本轮的新增失败项。

FREEZE 声称的产物逐项存在：

| 声称产物 | 实测 |
|---|---|
| `panel/round3/` 6 文件 | `fact.md`、`design-d2.md`、`reader-a.md`、`reader-b.md`、`reader-c.md`、`refute.md` 均存在 |
| `panel/round3/INDEX.md` | 存在；`文件数: 6`；refute 哈希 `c8bd00fc…6331` 与实测一致 |
| `certify/round3-certify.md` | 存在；头部 `结论：PASS`、`核验条数：19`、`CLOSED：19；STILL-BROKEN：0` |
| D-061 | `DECISIONS.md:143` 存在，日期 2026-11-20 |

制品链的缺口不在 FREEZE 的产物清单里，而在台账引用的 `suggestions.md`，见 §4。

## 3. 六门读数

任务列了 7 个文档门，全部执行；每条均附退出码。

| 门 | 命令 | 实测读数 | 结论 |
|---|---|---|---|
| anchor | `bash tools/anchor-resolve.sh --check <doc>` | `anchors=55 resolved=55 zero-def=0 multi-def=0`，exit 0 | 通过 |
| citation | `python3 tools/doc-citation-check.py <doc>` | `blocks=32 checked=25 failed=0 skipped=4 warned=3`，exit 0 | 通过 |
| structure | `python3 tools/doc-structure-check.py <doc>` | `✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析`，exit 0 | 通过 |
| ref | `bash tools/doc-ref-check.sh --check <doc>` | `✅ doc-ref-check: ... 内部章节引用均可解析`，exit 0 | 通过 |
| style | `bash tools/doc-style-lint.sh <doc>` | `命中汇总：error级=0`，exit 0 | 通过 |
| density | `bash tools/doc-code-density-check.sh --check <doc>` | `✅ 承载门阻断项通过（4 个可判章均有码/命令）`，exit 0 | 通过 |
| code-map | `bash tools/doc-code-map.sh <doc> --check` | `（共 30 个文件；存在性 30/30 通过）`，exit 0 | 门通过，但 FREEZE 的 29/29 记错 |

## 4. 未闭环与债务

`OPEN-DEBT.md` 的 R3-design-batch 行存在，且带判决与日期：

```
| R3-design-batch | P2 | **用户批准 2026-11-20（代理裁决 D-061）延期至下一 design batch**：RD3-DESIGN-D2-01/02（P1：...）、D2-03..D2-11（P2：...），以及 5 条设计 P3。确认轮不实施 ⑥（§3.6），登记后销账 |
```

- 设计 P1/P2 计数：台账 `②` 共 4 行，发现 ID 为 2 + 3 + 3 + 3 = 11，与 OPEN-DEBT 的 `D2-01/02 + D2-03..11` 一致。
- 设计 P3 计数：台账 2 行 SUGG，发现 ID 为 3 + 2 = 5，与 OPEN-DEBT 的「5 条设计 P3」一致。
- 未闭环：`eval-trace-check --round R3` 报 `metrics: 未闭环 = 0，本轮可关闭`，与 FREEZE 一致。

但存在三个记录问题：

1. **`suggestions.md` 不存在**。台账 18 条 SUGG 行（合计 49 条发现）的「复验」列都写「已登记 `suggestions.md` 的 R3 段」。实测：

```
$ test -f rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/suggestions.md; echo $?
1
$ find . -path ./minix3 -prune -o -name 'suggestions.md' -print
./rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/suggestions.md
./rewrite-notes/coordination/review-eval/runs/01-stage-kernel/03-kmain-cstart/suggestions.md
```

D02 的 SUGG 处置因此只有台账自述，没有可重放的登记文件。发现本身在 `panel/round3/` 与台账里可见，但「已登记」这一句无法验证，按记录不一致记 FAIL。

2. **`loop.md` 的 R3 汇总是旧读数**：

| 项 | `loop.md` | `metrics.md` / 台账 | 结论 |
|---|---|---|---|
| 发现总数 | 89（`:339`） | 95 | 不一致 |
| 唯一根因数 | 34（`:320`、`:339`） | 37 | 不一致 |
| 本轮新准入 | 34（`:339`） | 37 | 不一致 |
| 处置 ① | 12（`:339`） | 13 | 不一致 |
| 制品 CLAIM 数 | 0（`:309`） | 6（INDEX） | 不一致 |

`:339` 的原句是「与 `metrics.md` 的 `## R3 确认轮` 块一致：发现总数 89、唯一根因数 34、本轮新准入 34、certify 新准入 0、处置 ① 12 / ② 11 / ④ 17 / SUGG 49、未闭环 0」。这句话声称与 metrics 一致，但 metrics 的最终块是 95 / 37 / 37 / ①13 / FP5。旧句还漏掉了 CLAIM 6 与 FP 5 两个分档。

3. **D-061 同号冲突**。`DECISIONS.md:142` 是 D01 的 ARCH 裁决（2026-11-19），`:143` 是 D02 的重开裁决（2026-11-20）。两行都叫 D-061。OPEN-DEBT 的「代理裁决 D-061」靠日期和上下文才能确定指向 D02 行；编号本身不唯一。

关于任务卡里的「①12」：`metrics.md` 与台账的逐 ID 聚合都是 `①13 / ②11 / ④17 / SUGG49 / FP5`，总和 95。`①12` 与其余分档相加只有 94，和同一句里的「发现总数 95」自相矛盾。本轮以 `metrics.md` 和台账的实测聚合为准，判定「metrics 与台账一致」为通过；`①12` 视为任务卡笔误或旧读数，不记入 FREEZE 记录失败项。

## 5. 一致性抽查（5 行）

| # | 根因 ID | 处置类 | 重放要点 | 结论 |
|---|---|---|---|---|
| 1 | `RD-R3-01` | ① | 旧错误短语 0 命中；`§4.4` 表行落在 `:1036`；aarch64 实现是 `ldr` 取绝对值后 `br` | 内容通过；台账记的 `:1033` 是表分隔行，行号漂移 |
| 2 | `RD-R3-05` | ② | `store_kernel_info` 返回 `&'static KernelInfo`；没有 one-shot 标志；OPEN-DEBT 有 D-061 与 2026-11-20 | 通过（按设计延期） |
| 3 | `RD-R3-11` | ④ | 文档 `:92-94` 有三行基址定义；C `kernel.lds:4-6` 同值；citation 块 `90-110` 为 PASS/节选/14 行 | 通过 |
| 4 | `RD-R3-13` | SUGG | 文档 `:8` 仍写「§3.1 五腿分类表」；表体 6 行；`suggestions.md` 不存在 | 文本未回退（P3 延期），但复验登记不可重放，记 FAIL |
| 5 | `RD-R3-37` | ① | 目标串命中 `:1075`；台账记的 `doc:1072` 是空行 | 内容通过；行号记录失效，记 FAIL |

实际命令与输出：

```
$ grep -c '低地址进入（`ldr` 取高半绝对值跳）' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
0
$ sed -n '1036p' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
| aarch64 direct-core（`bootface_a64` → `arch_boot`） | 低地址进入、物理视图 | 生产镜像（带 `AT()`） | 高半 VMA（`ldr` 取绝对值） | 不需要 |
$ sed -n '40,56p' os/kernel/src/arch/aarch64/higher_half.rs
...
"ldr x1, ={kmain}",
"br x1",
...

$ grep -n 'fn store_kernel_info' -A18 os/kernel/src/lib.rs | head -1
1453:pub fn store_kernel_info(kernel_info: &KernelInfo) -> &'static KernelInfo {
$ grep -n 'one.shot\|claim\|taken\|frozen' os/kernel/src/lib.rs | grep -i kernel_info
（无输出）

$ sed -n '92,94p' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
_kern_phys_base = 0x00400000;	/* phys 4MB aligned for convenient remapping */
_kern_vir_base =  0xF0400000;	/* map kernel high for max. user vir space */
_kern_offset = (_kern_vir_base - _kern_phys_base);
$ sed -n '4,6p' minix3/minix/kernel/arch/i386/kernel.lds
_kern_phys_base = 0x00400000;	/* phys 4MB aligned for convenient remapping */
_kern_vir_base =  0xF0400000;	/* map kernel high for max. user vir space */
_kern_offset = (_kern_vir_base - _kern_phys_base);
$ python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md --json
...（首行摘要）
90-110 PASS 节选 14

$ grep -n '五腿' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
8:> **前置知识**：Rust 生产路径的起点按腿不同（详见 §3.1 五腿分类表与 §4.4 路径图）...
$ sed -n '369,376p' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md | grep -c '^|'
8
$ test -f rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/suggestions.md; echo $?
1

$ grep -n -o '高半 VMA（`ldr` 取绝对值跳）' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
1075:高半 VMA（`ldr` 取绝对值跳）
$ sed -n '1072p' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
（空行）
```

第 4 行的「表体 6 行」来自 `sed` 范围内的 8 条 `|` 行：表头 1 + 分隔 1 + 数据 6。数据行是 5 条执行形态加 1 条 `link.ld`「无生产链接读者」，与读者报告指出的形态一致。该条是 P3 SUGGESTION，按 §4.0c 允许延期；失败点是「已登记 `suggestions.md`」这句没有对应制品，不是文档回退。

## 6. 局限声明

- **同 agent 复核**：本次审查与 R3 的面板、certify 使用同一模型。命令与门全部重放，但没有做跨模型独立复核；同一模型对同一段文字的系统性误读无法由本方法发现。
- **未重审技术内容**：本轮只核 FREEZE 记录、制品链、机械门与 5 条抽样。文档的技术正确性以 R3 的六角色面板与 certify 回执为输入，本次没有重新逐条验证 95 条发现。
- **未跑真机链与宿主测试**：R3 声称无代码行为改动；本次没有重跑 QEMU、`check-layout.sh` 或 `cargo test`。宿主测试与引导链的读数不在本轮判定范围内。
- **只读边界**：除本文件追加本节外，未修改任何文件；缺失的 `suggestions.md` 没有补建，也没有改台账。
- **行号时点**：本节的文档与源码行号都是 2026-11-20、HEAD `efb179a9f` 的实测值。R3 台账与 FREEZE 里的行号是更早时点，二者已经漂移。
- **FAIL 的性质**：8 条失败项都是记录或制品链不一致，不是文档 sha 回退，也不是六门机械失败。文档 sha 与行数仍与 FREEZE 相同，七道门全部通过（code-map 是 30/30 通过，只是 FREEZE 记成 29/29）。
- **结论含义**：本轮 `FAIL` 表示「R3 冻结记录不能逐字对上最终工作树」，不表示文档技术结论被推翻。修正记录后可以重跑同一套命令复查。

## R3 回归审计的补修记录（主 agent，append-only）

上面是独立审计者的原始结论（FAIL，8 项）。按 §4h，8 项已全部处置（提交见下）：

| # | 审计发现 | 处置 | 复验 |
|---|---|---|---|
| 1 | FREEZE 记 doc-code-map 29/29，实测 30/30 | FREEZE 改为 30/30 | 审计读数即实测 |
| 2 | FREEZE/metrics 记 refute 索引 `CLAIM 1..6` | 改为 `CLAIM 11..16`（与文件实际一致） | `grep -c "^## CLAIM" panel/round3/refute.md` → 6 |
| 3 | 18 条 SUGG 行引用 `suggestions.md`，D02 目录无此文件 | 新建 `suggestions.md`，含 R3 段与四类来源 | `ls runs/.../02-.../suggestions.md` 存在 |
| 4 | loop §2/§6 仍是 89/34/①12/CLAIM 0 | 改为 95/37/①13/CLAIM 6，并重写 VARIANCE 1 | `--round R3` PASS |
| 5 | FREEZE 记「9 条读者 P1」，实为 7 条 P1 FINDING + 1 条 P1 级 SUGG | 改为准确计数 | 面板 `reader-*.md` 的 P1 标题计数 |
| 6 | 台账行号漂移（doc:1072 为空行、:1033 是表分隔行） | RD-R3-37 改 `sed -n '1075p'`；RD-R3-01 的证据改引 `grep -n "高半 VMA（\`ldr\` 取绝对值跳）"` | `sed -n '1075p'` 命中目标串 |
| 7 | D-061 重复（并发会话已用 D-061/D-062 裁决 D01 的两项） | 本目录的判决行改 **D-063**，五处引用同步 | `grep -c "D-063" DECISIONS.md` → 1；`grep -c "D-063" FREEZE.md` → 3 |
| 8 | ARCH 提案写作 RD3-ARCH-01/02，面板标题为 RD3-A01/A02 | 五处统一为 RD3-A01/A02 | `grep -rn "RD3-ARCH" runs/.../02-...` → 0 |

补修后实测：`--round R1/R2/R3` 三轮均 PASS；anchor 55/55、citation `checked=25 failed=0`、doc-code-map 30/30、panel-index 6 文件哈希一致。

> 并发提示：本轮期间有另一会话在同一仓库提交 `5b15e5e26`（D-061）与 `0ac7477af`（D-062），两份判决都针对 D01 的 R35 发现；本目录的判决行因此顺延为 D-063，避免编号冲突。
