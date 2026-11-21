# D02 suggestions —— 读者/设计 P3 建议桶

> 按 `review-eval/review-evals.md` §4.0c：读者与设计角色的 P3 建议以 SUGG 行入台账。
> **D-107 终态（2026-11-21）**：本节所有条目均为 implemented / already-fixed / dup-of；
> “登记不实施”“随批评估”“建议级”不再是出口。证据指向当前文档章节或代码提交。

## R3 确认轮（V3.4）

| 来源 | 数量 | 摘要 | 处置 |
|---|---|---|---|
| RD3-READER-A-S* | 19 | 措辞、指针、示例与术语类建议 | [CLOSED] 已由 R4 读者项与后续 edition 覆盖；目标文本均已在当前文档实现（见下表 A-S*） |
| reader-b.md 的 SUGGESTION | 15 | 节边界、重复指针化、术语定义、过程词类 | [CLOSED] 同上（见下表 B-S*） |
| reader-c.md 的 SUGGESTION | 10 | 复述一致性、GDB 输出多架构适配、覆盖声明 | [CLOSED] 同上（见下表 C-S*） |
| RD3-DESIGN-D2 的 P3 | 5 | 设计 P3 | [CLOSED] 已由 R4 的 DESIGN-D2-07/08/09 与 D-107 代码批覆盖；无遗留项 |

## R4 确认轮建议（V3.4）

> 来源：`panel/round4/` 的 37 条编号 SUGGESTION + 5 条 fact 未编号建议 + 3 条设计 P3。

| ID | 状态 | 证据 |
|---|---|---|
| READER-A-S01 | [CLOSED] implemented | 编号索引已补 D2-01/D2-06/D2-13/NK4-A 白话；测试清单补 `test_compute_kernel_layout_rejects_image_without_load_segments` 或注明“代表项” |
| READER-A-S02 | already-fixed | `push 0` 终止槽已说明“不在帧指针链上、只是初始栈终止槽” |
| READER-A-S03 | dup-of | §3.1「第 5 行」已由 READER-B-01/READER-C-02 修为「第 3 行」 |
| READER-A-S04 | [CLOSED] implemented | §3.5 两档条件改为互斥的“回退档受支持/不受支持” |
| READER-A-S05 | [CLOSED] implemented | 开头 `vm_enable_paging` 节号改 §2.2 |
| READER-A-S06 | dup-of | §5.2 `kern_size` 重复已由 FACT-07/DESIGN-D2-10 修复 |
| READER-A-S07 | [CLOSED] implemented | `init_protection` 引文限定为 Phase B 的第一条安装动作，Phase A 只校验 |
| READER-A-S08 | [CLOSED] implemented | §2.4 证据表第 2 行改为“排除定义在 `.text`”，低地址段留给第 3 行收段规则 |
| READER-A-S09 | [CLOSED] implemented | 归纳表补“外部装载器（GRUB 等）”，树内 boot monitor 的 `LOADADDR(p_vaddr)` 保留在 §1.2 |
| READER-A-S10 | [CLOSED] implemented | `AlreadyMapped` 段改为“两个方向都会被拦住；假如没有检查才会顶掉” |
| READER-A-S11 | [CLOSED] implemented | 自引导腿读者点名：U-Boot `bootelf` 读 `p_paddr`、`-kernel` 由固件放置、OpenSBI 只跳转 |
| READER-B-S01 | [CLOSED] implemented | §4.4 改为“§3.1 登记表是权威，本节只按装载路径展开，不重复” |
| READER-B-S02 | [CLOSED] implemented | 961 的映射粒度用途归 §3.5，§2.2 只留页目录下标语境 |
| READER-B-S03 | [CLOSED] implemented | §4.3 的“执行地址 = 链接地址”压成表注，不再逐句复述 |
| READER-B-S04 | [CLOSED] implemented | “取入口地址”保留表行，bullet 缩成一句、表后段落并入表注 |
| READER-B-S05 | [CLOSED] implemented | §4.4 历史叙述压成“副本落地的两条原因”，早期读数移入诊断注记 |
| READER-B-S06 | [CLOSED] implemented | §1.1 `USR_DATATOP` 细节压缩为“内核占高半、低地址留给用户”，`_MINIX_MAGIC` 取值降为括注 |
| READER-B-S07 | [CLOSED] implemented | §3.1 取舍表限定为“x86_64/aarch64 载体入口时低地址执行”，riscv64 低装高进另列 |
| READER-B-S08 | [CLOSED] implemented | 路径图把 aarch64 direct-core 单列为“生产路径 C”，与测试载体分开 |
| READER-B-S09 | [CLOSED] implemented | 三时刻指针改为“§2.1、§2.3、§4.1 与 §4.3” |
| READER-B-S10 | [CLOSED] implemented | §2.2 反事实先给“未定义符号→链接期失败”，再一句反向不等价 |
| READER-B-S11 | dup-of | §5.2 `kern_size` 重复已由 FACT-07 修复 |
| READER-B-S12 | [CLOSED] implemented | “组 KernelInfo（carrier）”改为“组 KernelInfo（v1 交接载荷）” |
| READER-B-S13 | [CLOSED] implemented | “只在这三处”改为“三处 + 另有两处不影响结论” |
| READER-B-S14 | dup-of | §2.5 ARM“待补”已由 FACT-09 的三处源码证据修掉 |
| READER-B-S15 | [CLOSED] implemented | §4.3/§4.4 自指括号改为“本节其二/下文” |
| READER-C-S01 | [CLOSED] implemented | 页表量级改为“约 1024 个页表项、约 5 张页表页（含各级目录）” |
| READER-C-S02 | [CLOSED] implemented | 前置表 fault site 改为“生产路径在复制形参所指载荷时页错误” |
| READER-C-S03 | [CLOSED] implemented | 补 x86_64 `mov r9, rbp`、aarch64 `mov x3, x29`、riscv64 `mv a3, s0` 三条帧指针捕获 |
| READER-C-S04 | [CLOSED] implemented | §3.2 字段表补 `p_type`（类型过滤）并注明 `p_align` 无读者不保留 |
| READER-C-S05 | dup-of | 并入 READER-A-S10（`AlreadyMapped` 方向） |
| READER-C-S06 | [CLOSED] implemented | §4.4 以“形参被溢出到内核栈、从被覆写栈槽读回”为主因，caller-saved 句降为次因 |
| READER-C-S07 | [CLOSED] implemented | §2.3 摘录补 `hang: jmp hang` 兜底循环 |
| READER-C-S08 | dup-of | 并入 READER-A-S07（Phase A/B 引文张力） |
| READER-C-S09 | [CLOSED] implemented | `head.o` 行列出 `.unpaged_text`/`.unpaged_data`/`.unpaged_bss` |
| READER-C-S10(a) | dup-of | §5.2 重复 `kern_size` 已由 FACT-07 修复 |
| READER-C-S10(b) | [CLOSED] implemented | 测试前缀统一为 `first_overlapping_pair_*` |
| READER-C-S10(c) | [CLOSED] implemented | “字段与另外两个派生标量（`user_sp`/`syscall_entry`）一起删除” |
| READER-C-S10(d) | [CLOSED] implemented | 加数错位改为“一位十六进制错位（`7ffe` 对 `7fe0`）”并给正确结果 |
| READER-C-S10(e) | dup-of | 并入 READER-B-S09（三时刻指针） |
| READER-C-S10(f) | [CLOSED] implemented | “本节后文（§2.3 末）”改为“本篇 §2.3 末” |
| READER-C-S11 | [CLOSED] implemented | “（U-Boot/OpenSBI）”改为“（U-Boot；OpenSBI 只提供 SBI，不摆放镜像）” |
| FACT-S1 | dup-of | 并入 READER-C-S10(c) |
| FACT-S2 | [CLOSED] implemented | §7.2 `pagetable.c` 注明为 07 篇背景阅读，不在本篇论证链 |
| FACT-S3 | already-fixed | arch `link.ld` 已按 D-107 删除，“全仓 grep 无读者”声明对象消失 |
| FACT-S4 | [CLOSED] implemented | §5.1 改为“解析/布局/缓冲区语义每个环节”，物理写入仍只由真机链证明 |
| FACT-S5 | [CLOSED] implemented | §3.2 畸形输入限定为“非 `PT_LOAD` 走 continue，其余以 `Some(Err)` 浮出” |
| FACT-OBS-01 | [CLOSED] implemented | §5.1 “三类畸形”改为“四类畸形”并列出四个 `ElfError` 变体 |
| DESIGN-D2-07 | [CLOSED] implemented | 提交 `83e8cbb2b`：`jump_to_kmain` 删 `stack_top` 形参，三实现从 `kinfo.kern_stack_top()` 现算 |
| DESIGN-D2-08 | [CLOSED] implemented | 提交 `ecfef0ae9`：删 `p_align`/`LoadSegment.align`/`HIGHER_HALF_CALLED`/`virtual_end` |
| DESIGN-D2-09 | [CLOSED] implemented | 提交 `74ea87c44`：删 `KernelInfo`/`MemoryRegion`/`BootModule` 的 `repr(C)` |

## R5 重写基线

- READER-C 的 10 条未编号 P3 建议已按面板原文逐条分诊：其中“§3.7 三者都去掉”“§2.5 栈顶交接”“§2.1 无 AT() 默认”“L0–L9 vs L10”“§4.3 pgeok 节号”“下表第 3 行”“§2.4 残句”“L9 x86 措辞”“§4.4 引文头”九条已在本轮正文修正；其余一条（§1.3 L0–L9 权威清单）已在 §1.3 统一为 L0–L10。
- 读者 A/B/C 的未编号建议不再另立 SUGG 行；有编号的 FINDING 已全量进台账。
