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
