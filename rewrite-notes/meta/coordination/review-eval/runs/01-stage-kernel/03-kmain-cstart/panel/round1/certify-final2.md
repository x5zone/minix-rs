# D03 R1 二次最终认证（闭集）

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 规模：1384 行
- 身份：`sha256 = 3fc8dd7d884e5eda45c82862a9325f001aa2fbfd6d1f9d692ff21feac98a556f`（与任务给的 `3fc8dd7d` 前缀一致；这是工作树版本，`git hash-object` = `26168a144c110b61e5641b95887696a9bcdfc221`，HEAD 的 blob 为 `8ab89133e`）
- 范围：只核对下列 2 项及其直接邻句；未重扫全文，未改文档，只新建本认证文件。
- 日期：2026-11-12

## 一、逐条核对

| # | 核对项 | 结论 | 证据 |
|---|---|---|---|
| 1 | §1.2 已无“三份答案”“三问”“三个问题”，统一为“四类入口” | **CLOSED**（§1.2 范围） | §1.2 为 L38–L58。`sed -n '38,58p' 文件 \| grep -nE '三份答案\|三问\|三个问题'` 无输出（exit 1）。标题 L38 为“三张表与四类入口”；L40“四类入口落在三张表与系统调用路径上”；L55“它不回答上面四类入口中的任何一类”；L57“三张表/系统调用路径与它们回答的四类入口”；§1.4 L90“保护结构 = 三张表与四类入口”；§4.6 L1174、L1176、L1180 同口径。 |
| 2 | §5.1 写“373 个测试/54 文件、87 个/21 文件、9 文件子集 55” | **CLOSED** | L1223 原文：“这 9 个文件是与本篇直接相关的子集（按锚定 `#[test]` 去重后合计 55；`riscv64/trap_stub.rs` 的一条注释命中已剔除）；arch crate 全量 `src/**/*.rs` 是 373 个测试/54 文件，`os/arch/tests/*.rs` 另有 87 个/21 文件。” 独立复算与原文一致，见下。 |

### 第 1 项的全文范围附注（不计入准入）

按任务原文“§1.2 是否已无……”判 CLOSED。全文 grep 仍有 2 处 `三份答案`：L10（篇首引入段）与 L16（§1.1 标题）；`三问` 与 `三个问题` 全文为 0。L10/L16 说的是“出事时需要预先知道的三份答案落在 GDT/IDT/TSS 三张表里”，与 §1.2 的“四类入口”不是同一个枚举（后者多出系统调用 MSR/`svc`/`ecall` 这一入口）；上一轮 FINDING RD3-CERTF-01 的修法也把“只在 §1.2/§4.6 使用四类口径”列为可接受的最小方案。若本轮判据要求全文 grep 为 0，则第 1 项应改判 REOPEN，原因只在 L10/L16 两处，§1.2 本身已清零。另有 L22、L25 的“三件事”，不在任务点名的三个旧词内，同样属于 §1.1 的三表叙述。

### 第 2 项的证据

```text
$ grep -rE '^\s*#\[test\]\s*$' os/arch/src --include='*.rs' | wc -l
373
$ grep -rlE '^\s*#\[test\]\s*$' os/arch/src --include='*.rs' | wc -l
54
$ grep -rE '^\s*#\[test\]\s*$' os/arch/tests --include='*.rs' | wc -l
87
$ grep -rlE '^\s*#\[test\]\s*$' os/arch/tests --include='*.rs' | wc -l
21
$ 九个文件的锚定计数求和
55
```

九个文件里唯一有差额的是 `os/arch/src/riscv64/trap_stub.rs`：`grep -c '#\[test\]'` 为 7，锚定 `^\s*#\[test\]\s*$` 为 6（差额是文件内一条注释里的 `#[test]` 字样），与 L1223“一条注释命中已剔除”一致。§5.1 的循环命令 L1211 仍用非锚定 `grep -c '#\[test\]'`，所以屏幕上九文件和为 56，紧接着的 L1223 已写明剔除 1 条并给出 55；命令口径与文字口径之间有一次手工校正，但文字已披露，不构成新的计数错误。§5.1 内没有残留的 374、95 或合计 56。

## FINDING RD3-CERTF2-01（P2）

**位置**：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:46`（§1.2 新增的“系统调用入口”行）。

**事实**：C i386 格写“SYSENTER/SYSCALL MSR（不是 IDT 门）”。但 Minix3 C 的 libc 系统调用/IPC 主路径走的是 IDT 软中断门：

- `minix3/minix/include/arch/i386/include/ipcconst.h:4-5`：`KERVEC_INTR 32`（syscall trap to kernel）、`IPCVEC_INTR 33`（ipc trap to kernel）。
- `minix3/minix/lib/libc/arch/i386/sys/_ipc.S:23,35,49,60,72,84`：六条 IPC 入口都是 `int $IPCVEC_INTR`；`_do_kernel_call_intr.S:7` 是 `int $KERVEC_INTR`。
- 这两条都在 IDT 的 `gate_table_exceptions[]` 里，文档自己的 §2.3 L590 也写了“4 条软中断（32/33/34/35）”。
- SYSENTER/SYSCALL 是 usermapped 快路径：`archconst.h:172-173` 把 `KTS_SYSENTER`/`KTS_SYSCALL` 注为 usermapped；`i386/protect.c:183-194` 写对应的 MSR。

Rust x86-64 同样两套都在：`os/arch/src/x86_64/trap_stub.rs:289-291` 装了 `TRAPSTUB 32/33`（连同 34/35），`:240` 的 `user_accessible` 明确允许 3/4/32/33/34/35；原生快路径是 `SYSCALL/SYSRET + LSTAR`。所以“（不是 IDT 门）”如果读成“该架构的系统调用入口不走 IDT”，对 C 和 Rust 都不成立；如果只想修饰 MSR 路径，应把否定范围写清。

**影响**：§1.2 是全文“四类入口”的定义表，读者会把 C 侧 syscall 入口误读成只有 MSR 一条，漏掉 libc 实际使用的 `int $32/$33` 门。§2.3 与 §2.4 分别覆盖了软中断门与 MSR 两条路径，机制结论不受影响，故判 P2（表述完整性），不判 P0-fact。

**建议修法**：C 格改成“SYSENTER/SYSCALL MSR；libc 主路径另走 `int $32/$33` 软中断门”；Rust 格改成“SYSCALL/SYSRET + `LSTAR`；另装 32–35 软中断门（C 兼容）”；或至少把“（不是 IDT 门）”改成“（MSR 路径不是 IDT 门）”。

## FINDING RD3-CERTF2-02（P2）

**位置**：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:51,53`（§1.2 表加第四行后未重编号）。

**事实**：§1.2 的表 L42–L47 现在是四行：第一行当前特权级、第二行异常/中断入口、第三行系统调用入口、第四行陷入内核用哪根栈。L51 仍写“三张表的一个差别是：②是逐条目不同的……①和③各只有一套（DPL 存在少数几个描述符里、`sp0` 每 CPU 一个字段）”，L53 仍写“③ 还带一个‘内核态里又出事用哪根栈’的子问题”。这里的 ①②③ 实际指 §1.1 代码块 L26–L28 的三项（①特权级、②IDT、③TSS），不是 §1.2 表的行序；加行后表里第三行成了“系统调用入口”，L53 的“③……栈”与 L51 的“③……`sp0`”在按表行序阅读时都会错位。

**影响**：读者若顺着 §1.2 的表往下读，会把“系统调用入口”当成“栈”的载体；若回到 §1.1 的代码块读，则没有矛盾。这是新增行带来的表述清晰度问题，不涉及 C 源码事实，判 P2。

**建议修法**：在 L51/L53 的序号前加“§1.1 的”，或把 §1.2 表的四行显式标成 ①–④ 并同步改 L51/L53（若表行序是新的规范枚举，L53 的“③”应改成“④”，L51 的“③……`sp0`”也要改成第四行）。

## 二、结论

- 第 1 项：CLOSED（§1.2 内三个旧词为 0，已统一为“四类入口”；全文 L10/L16 的“三份答案”见附注）。
- 第 2 项：CLOSED（373/54、87/21、九文件子集 55 均已写入并与锚定计数一致）。
- 新准入：P2 × 2（§1.2 新增 syscall 行漏掉 `int $32/$33` 软中断门；§1.2 加行后 ①②③ 序号与表行序错位）。

最终认证新准入 = 2

## 三、RD3-CERTF2-01/02 复核（2026-11-12）

本节为本轮复核结论，末行为当前计数。

- 复核身份：`sha256 = 093de2311fb54746c3841dfbe7f17dbb97719a60cf86f1f304ca88b6c861faa7`（与任务给的 `093de231` 前缀一致；1384 行）。
- 复核范围：只重读 §1.2 L46 表行与 L51/L53 两处编号句；未重扫全文。
- RD3-CERTF2-01：**CLOSED**。L46 C 格现为“SYSENTER/SYSCALL MSR 与 `int $32/$33` 软中断门（libc 主路径；后者是 IDT 门）”，Rust 格现为“SYSCALL/SYSRET + `LSTAR`，另装 32–35 软中断门”。事实复验：`ipcconst.h:4-5` 的 `KERVEC_INTR 32`/`IPCVEC_INTR 33`；`_ipc.S` 六处 `int $IPCVEC_INTR`；`trap_stub.rs:290-293` 的 `TRAPSTUB 32`–`35`。
- RD3-CERTF2-02：**CLOSED**。L51 现为“§1.1 的 ② … §1.1 的 ① 和 ③ …”，L53 现为“§1.1 的 ③ …”，序号明确指向 §1.1 的三表编号，不再与 §1.2 四行表错位。
- 本轮在复核范围内未发现新 P0/P1/P2。

最终认证新准入 = 0
