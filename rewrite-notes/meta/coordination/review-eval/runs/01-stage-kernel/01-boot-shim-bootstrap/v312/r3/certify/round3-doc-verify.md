RECHECK: PASS
VERDICT: PASS

# 01-boot-shim-bootstrap R3 文档复验（RECHECK）

- 目标文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 当前 sha256：`67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8`
- 行数：`wc -l` = 3085
- 面板：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round3/`
- 台账：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/defect-ledger.md`
- 本轮范围：上一轮 FAIL 的第 15、18 项与 NEW-01/02/03；同时复核第 8、14 项的顺带修正；重跑 8 道文档门。
- 结论：五项指定修复均已落到当前正文，且与源码/上下文一致；8 道门全部 exit 0；未发现新的阻断矛盾。仅保留若干非阻断 P3 观察与台账检查串覆盖不足（见文末）。

## 一、本轮修复复核（PASS）

| 项 | 位置（当前行号） | 正文证据 | 可重放命令 | 判定 |
|---|---|---|---|---|
| 15 · §3.7 直核腿六类载荷 | §3.7 所有权段 L1935；I3 L114 | L1935：`两个直核腿各拥有一份栈上的 BootStorage，六类载荷齐全（包括经 fill_source_records 写入的 source 记录）`；L114 仍保留`两个直核腿的六类载荷归各自栈上的 BootStorage（source 走 fill_source_records）` | `rg -n '两个直核腿各拥有一份栈上的\|两个直核腿的六类载荷' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | PASS |
| 18 · §4.4 视图完整性限定 | §4.4 开头 L2410；§3.2.1“收敛点”L1241 | L2410：`在数据形状上收敛（形状收敛，不保证发送侧视图完整；UEFI 腿的 source_records 视图见 §3.2.1/§3.7；…）`；L1241 原有同义限定仍在 | `rg -n '形状收敛，不保证发送侧视图完整' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 2，L1241/L2410） | PASS |
| NEW-01 · §4.4.1 时间序表 | 表 L2504–2511；同节正文 L2500；源码 `os/kernel/src/lib.rs` | 表 v1 列：第 4 行 `resume_boot_pt_alloc`、第 5 行 `resume_additive` 内的 boot_validate、第 6 行 `store_kernel_info（同址跳过字节拷贝）→ jump_to_kmain`；L2500 正文为 `strict → postconditions → resume_boot_pt_alloc → boot_validate → … → store_kernel_info`；源码 `resume_additive` 在 318 行先 `boot_validate_and_prepare`、335 行才 `store_kernel_info` | `sed -n '/按时间顺序把两条腿并排看/,/下表按门/p' 文档`；`nl -ba os/kernel/src/lib.rs \| sed -n '316,336p'` | PASS |
| NEW-02 · §3.3 审计入口 | §3.3 L1411 | L1411：`库函数 minix_boot::memmap_source::audit_source_coverage/audit_source_coverage_for 是这层检查的两个入口；KernelInfo::validate_requiring_sources 在 strict 分支里调用带 authority 过滤的后者` | `rg -n 'audit_source_coverage_for' 文档`；`nl -ba os/libs/minix-boot/src/kernel_info.rs \| sed -n '528,533p'` | PASS |
| NEW-03 · `RamAuthority` 形状 | §1.1.1 L104 | L104：`RamAuthority：单字段包装，内部包一个输入对象标识；DTB/UEFI 是两个关联常量（不是枚举变体）` | `sed -n '104p' 文档`；`nl -ba os/libs/minix-boot/src/memmap_source.rs \| sed -n '570,576p'` | PASS |

## 二、23 项总表（当前 sha）

| # | 面板项 | 位置（当前行号） | 原文证据 | 可重放命令 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 1 | READER-A-01 / B-01 | §1.0 小结 L41；术语 L59；§1.1 L74 | L41：`内核位置记录（框 2）的区间裁剪可用内存清单（框 1）；框 1 不进入框 2。` | `sed -n '41p;59p;74p' 文档` | PASS | 首用即用同位语定义两词；台账新检查串 `可用内存清单（框 1）` 已命中 L41。 |
| 2 | READER-A-02 | §3.7 L1939 | `五类错误各有落点：缺来源记录 → …；权威与记录不匹配 → …；重复提交 → …；半提交…；跨镜像指针 → …` | `sed -n '1939p' 文档` | PASS | 五类与五个分号项一致。 |
| 3 | READER-A-03 | §3.2.1 L1211 | `22 是 C kinfo_t 的累计退场数，不是本结构的字段数；本结构的字段数就是上面的 16 → 13 → 14 → 10（现役 10）` | `sed -n '1211p' 文档` | PASS | 22 与结构字段数分开。 |
| 4 | READER-A-04 / B-02 | §3.4.3 L1601；§1.0 L57；§3.2.2.2 L1291 | L1601：`TTBR1 是 AArch64 承载高半地址翻译的页表基址寄存器；Sv39 是 RISC-V 的 39 位三级页表方案；satp 是 RISC-V 的根页表基址寄存器`；L57：PM/RS/IS 全称 | `rg -n 'TTBR1 是 AArch64\|Sv39 是 RISC-V\|satp 是 RISC-V' 文档`；`rg -c 'IS 是信息服务（information service' 文档` | PASS | 定义与全称均在；非阻断注：`satp` 在定义括注前一个分句已出现（同句），严格说不是定义先于首用。 |
| 5 | READER-A-05 / C-S04 | §3.6 L1876；§4.0 L1987；§4.5 L2595 | L1876：`store_kernel_info 包装跳过字节拷贝（源与目标同址），但仍构造 KernelInfo 并写 KERNEL_INFO 槽；顶部一次性守卫使第二次调用 panic`；L1987：`首次同址跳过复制，但带一次性守卫，第二次调用 panic` | `rg -n 'store_kernel_info' 文档` | PASS | 三处语义一致。 |
| 6 | READER-A-06 | §2.4.2.5 L807 | `见本节上表的“还 VM 镜像”行` | `rg -n '本节上表' 文档` | PASS | 自指已消除。 |
| 7 | READER-A-07 | §3.5.2 L1721、L1738 | `本节论点里…`；`本节论证的单向依赖原则本身` | `rg -n '标题里\|本节标题\|单向依赖原则' 文档` | PASS | 旧“标题”指代消失。 |
| 8 | READER-A-08 / C-S03 | §5.1 L2740 | `I3 的真机证据按覆盖深度写成三条陈述（第二条限定第一条中的一个成员）…另有一条边界：riscv64 bootface 的门只验证 fail-closed 错误路径` | `sed -n '2740p' 文档` | PASS | 上一轮“三条互不重叠”的措辞已改；四个成员无歧义；riscv64 `bootface` 的失败路径边界已补。 |
| 9 | READER-B-03 | §3.2.2.3 标题 L1295；§3.2.2 L1256；bullets L1301–1304 | 标题：`两种控制台与五处字段补充`；五个字段；L1256 四个读者任务 | `sed -n '1256p;1295,1304p' 文档` | PASS | 标题“五处”与五个字段一致；非阻断注：五个字段只占四个 bullet（`kern_stack_top` 与 `syscall_entry` 合述）。 |
| 10 | READER-B-04 | §2.4.2.4 L769 | `VM_BOOT_HANDOFF_VERSION，当前 9`；与信封 `BOOT_HANDOFF_V1_VERSION`（当前 10）分开 | `rg -n 'VM_BOOT_HANDOFF_VERSION' 文档`；`rg -n 'VM_BOOT_HANDOFF_VERSION: u32 = 9' os/libs/minix-types/src/types/boot.rs` | PASS | 源码常量确为 9。 |
| 11 | READER-B-05 | §2.3 L451；§3.2.2.4 L1317 | L451 空键 `namelen = 0` 字节推演；L1317 回指 | `sed -n '451p;1317p' 文档` | PASS | 推演与回指一致。 |
| 12 | READER-C-01 | §3.2.2.4 L1310；§2.3 L488 | `非首条记录还可能被中段匹配误删（§2.3 的 xboard=2 → xarch=i386）` | `rg -n '中段匹配误删\|xboard=2' 文档` | PASS | 与 §2.3 一致；非阻断注：“十条”是否应改成十一条仍未明说。 |
| 13 | READER-C-02 | §1.1 L71；§3.2.1 L1220；§4.4 L2445 | `三条 v1 腿的接收侧没有 ELF 头可读…；两个直核腿同镜像，用自身 KERN_* 常量` | `sed -n '71p;1220p;2445p' 文档` | PASS | 已限定三条 v1 腿，直核腿用常量。 |
| 14 | READER-C-A01 | §3.2.1 L1252；§3.6 L1909；§4.4 L2445 | L1252 与 L2445：`两个生产直核面与 x86_64 测试直核入口的跨距`；§3.6 x86_64 行 `无对应形态` | `rg -n 'x86_64 测试直核入口' 文档`（2 处） | PASS | §3.2.1 与 §4.4 均已限定；与 §3.6 的生产形态不冲突。 |
| 15 | READER-C-A02 | §1.2 L114；§3.7 L1935 | L114 与 L1935 均给出两个直核腿六类载荷归栈上 `BootStorage` | `rg -n '两个直核腿各拥有一份栈上的\|两个直核腿的六类载荷' 文档` | PASS | 本轮已补 §3.7；I3 行仍一致。 |
| 16 | REFUTE-I4 | §3.7 L1933；§5.1 I4 L2747 | L1933：`接收侧校验在消费…之前完成。发送侧的建根只依赖装载/分类门…不在接收侧门链内` | `rg -n '接收侧校验在消费' 文档` | PASS | 接收侧限定与发送侧分工一致。 |
| 17 | REFUTE-OBS1 | §4.5 L2595 | `守卫是 check-then-write，不是原子 CAS；它依赖启动路径在 BKL/SMP 之前的单线程前提` | `rg -n 'check-then-write' 文档`；`rg -n 'single-threaded \\(before BKL' os/kernel/src/lib.rs` | PASS | 源码 SAFETY 注释支持该前提。 |
| 18 | REFUTE-OBS2 | §3.2.1 L1241；§4.4 L2410 | 两处均有 `形状收敛，不保证发送侧视图完整` | `rg -n '形状收敛，不保证发送侧视图完整' 文档`（2 处） | PASS | §4.4 开头已补；与 §3.2.1 一致，无矛盾。 |
| 19 | FACT-01 | §4.3 L2300 | 复算命令输出 `12` | `sed -n '/pub const BOOT_MODULE_IDENTITIES/,/^];/p' os/libs/minix-boot/src/kernel_info.rs \| grep -o '"[a-z]*"' \| wc -l` → 12 | PASS | 旧 `MODULE_NAMES` 命令为 0，新命令正确。 |
| 20 | FACT-02 | §2.1 L262 | `[exec_multiboot](…:exec_multiboot) 里的 snprintf(...)` | `tools/anchor-resolve.sh --extract rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md \| grep exec.c`；`grep -n 'snprintf(cmdline' minix3/sys/arch/i386/stand/lib/exec.c` → 737（函数 713–796 内） | PASS | 符号锚点已取代 `:736`；非阻断注：L263 仍有散文本行号“759-760 行”（实测正确）。 |
| 21 | FACT-03 | §3.3 L1409 | `共 11 个字段：…id/local_source/snapshot_id…range/address_domain/virtual_start…kind/origin/input_object…attribute/raw_type` | Python 比对 `memmap_source.rs:492-517` 的 11 个 `pub` 字段 | PASS | 字段集合完全相等。 |
| 22 | 冷读可选 | §1.0 L44；§1.1.1 L97–104；§3.6 L1746–1748 | 类型速查、三行调用图、第一次读路线 | `sed -n '44p;97,104p;1746,1748p' 文档` | PASS | `RamAuthority` 已按源码改为单字段包装 + 两个关联常量；非阻断注：三行图是骨架，v1 接收实际经 `resume_after_v1_decode`/`arch_boot_resume_high_half` 壳，v1 发送行只列 UEFI 入口，U-Boot 腿实际从 `prepare_boot` 进入（§4.6、§3.5 已交代）。 |
| 23 | 文档门 | 全文 | 8 道门读数见下节 | 见下节 | PASS | 全部 exit 0。 |

## 三、8 道文档门（本轮重跑）

| 门 | 命令 | 读数 | exit |
|---|---|---|---|
| 锚点 | `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `anchors=87 resolved=87 zero-def=0 multi-def=0` | 0 |
| 关联代码 | `tools/doc-code-map.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --check` | 49 个文件，存在性 49/49 通过 | 0 |
| 章节引用 | `tools/doc-ref-check.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 内部章节引用均可解析 | 0 |
| 文风 | `tools/doc-style-lint.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `error级=0` | 0 |
| 出版 | `python3 tools/doc-publication-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `无新增违规` | 0 |
| 教程 | `python3 tools/tutorial-rubric-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `检查 6 章，0 章缺项` | 0 |
| 引文 | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `blocks=56 checked=19 failed=0 skipped=33 warned=4` | 0 |
| 结构 | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 1 个文件表结构与内部 `.md` 链接均可解析 | 0 |

## 四、非阻断观察（不影响 RECHECK PASS）

1. **第 4 项**：`satp` 在 §3.4.3 的定义括注前一个分句已出现（“当前 CR3/当前 satp”）；同句随后给出定义，语义可读，但严格说不是“定义先于首用”。
2. **第 9 项**：标题“五处字段补充”对应五个字段，但正文只占四个 bullet（`kern_stack_top` 与 `syscall_entry` 合述）；按“处”数会得到四。
3. **第 12 项**：§3.2.2.4 其一说中段误删“不改变上面两支的结论”，但未明说该现象是否计入“十条可观察分歧”之外；计数仍有轻微歧义。
4. **第 20 项**：§2.1 L263 仍有散文本行号“759-760 行”，实测指向正确语句，但不是工具派生的符号锚点；后续可换符号。
5. **第 22 项**：§3.6 三行调用图是刻意骨架；v1 接收省略了 `resume_after_v1_decode`/`arch_boot_resume_high_half` 壳，v1 发送行只列 UEFI 入口 `prepare_and_enter_kernel_image`，未列 U-Boot 的 `prepare_boot`。§4.6 与 §3.5 已给出完整分派，不足以构成事实冲突。

## 五、台账检查串（本轮已更新四项，仍有三处覆盖不足）

已核对通过：
- `可用内存清单（框 1）` → L41（小结行）。
- `TTBR1 是 AArch64` / `Sv39 是 RISC-V` / `satp 是 RISC-V` → 各 1 次。
- `五处字段补充` / `四个读者任务分组：字段去向` → 各 1 次。
- anchor 期望已改为 87/87，与本轮实测一致。

仍建议更新（不影响文档判定）：
- READER-A-04 行第二条检查串 `IS（information service|RS（reincarnation|PM（process` 仍零命中；正文实际是“IS 是信息服务（information service…”。建议改为 `IS 是信息服务` 等。
- READER-A-08 行检查串 `四条腿（x86 UEFI、…）` 仍零命中；正文实际是“有启动门链的四条腿 是 x86 UEFI、…”。建议删去括号或改为范围匹配。
- READER-C-A02 行检查串 `两个直核腿的六类载荷` 只命中 I3（L114），不覆盖本轮新增的 §3.7 L1935；建议加一条 `两个直核腿各拥有一份栈上的` 或 `六类载荷齐全`。

## RECHECK

- 已重放第 15 项：`rg -n '两个直核腿各拥有一份栈上的'`（L1935）与 I3 `rg -n '两个直核腿的六类载荷'`（L114）。
- 已重放第 18 项：`rg -n '形状收敛，不保证发送侧视图完整'` → L1241、L2410 两处。
- 已重放 NEW-01：`sed -n '/按时间顺序把两条腿并排看/,/下表按门/p'`（表 v1 列第 4 行 `resume_boot_pt_alloc`、第 5 行 boot_validate、第 6 行 `store_kernel_info → jump_to_kmain`）与 `os/kernel/src/lib.rs:318/335`；与 L2500 正文一致。
- 已重放 NEW-02：`rg -n 'audit_source_coverage_for'`（L1411）与 `os/libs/minix-boot/src/kernel_info.rs:528`。
- 已重放 NEW-03：`sed -n '104p'`（L104）与 `os/libs/minix-boot/src/memmap_source.rs:570-576`。
- 已重跑 8 道文档门，读数见第三节，全部 exit 0。
- 未重跑 `cargo test`/QEMU 真机门（本次只验文档；面板 fact 报告的宿主读数仍为 `minix-boot 189/0`、`boot-shim 136/0`、`minix-kernel 866/0/8`、`minix-elf 27/0`）。
- 未修改目标文档；本轮只更新本报告。目标 sha 仍为 `67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8`，`wc -l` = 3085。
- 同 agent 局限：本次复验由单一 subagent 完成，关键命令与输出已写入本文，可外部重放。
