# D02 follow-ups —— 本轮转出项与去处

| ID | 来源 | 事项 | 去处 |
|---|---|---|---|
| D02-F01 | DESIGN-03 | `jump_to_kmain(kinfo, stack_top)` 允许 `stack_top != kinfo.kern_stack_top`；删第二个参数或加 `debug_assert_eq!` | code batch；改动 3 个 impl + 4 个调用点 |
| D02-F02 | DESIGN-04 | `kern_stack_top == kern_virt_base + kern_size` 没有被校验；在 `check_boot_postconditions` 加等式或类型化 `KernelSpan` | 用户裁决（涉及跨镜像结构与 70+ 处夹具） |
| D02-F03 | DESIGN-05 | `os/kernel/src/arch/*/link.ld` 无生产链接读者，且与生产脚本有四处不等价（x86_64 首段 VMA 低 2 MiB、无 2 MiB 收口、保留 `k_initial_stktop`、缺收段） | 用户确认仓外无消费者后删除；否则修正并让锁步测试比较首段 VMA 与跨距 |
| D02-F04 | DESIGN-06 | `load_segments_into_buffer` 与 `load_segments_into_phys_memory` 是两份循环，被测的是缓冲区版，生产走物理地址版 | code batch：抽同一区间计算纯函数，两版共用 |
| D02-F05 | DESIGN-07 | `load_segments_into_phys_memory` 依赖调用者先跑 `compute_kernel_layout`，类型上不成立 | code batch：解析产出 `KernelImage`，摆放只接受已验证值 |
| D02-F06 | DESIGN-08 | 2 MiB 收口只有脚本检查与启动期 panic；三份生产链接脚本可加 `ASSERT((__bss_end - BASE) % 0x200000 == 0, ...)` | code batch（低风险，需一次构建确认 lld 求值次序） |
| D02-F07 | DESIGN-11 | `arch_boot_resume_high_half` / `resume_after_v1_decode` 三份几乎逐行相同 | code batch：收成泛型 `resume<A: BootArch, H: HigherHalf>`，三处 cfg 只选类型与打印 |
| D02-F08 | DESIGN-13 | `bootface.rs` 与 `bootface_a64.rs` 流程同构，`dtb_bounded`/`place_modules`/`say`/静态对象各有两份 | 用户裁决；代码侧抽共享流程，两个薄壳只给常量与入口参数 |
| D02-F09 | DESIGN-10/15 | `Paging`/`HugePages`/`BootArch` 保留；翻译残留分类清单作为 code-excellence 输入 | 无动作（拒绝项/分类项） |
| D02-F10 | READER-A/B/C 转出项 | 读者路径反例、§2.5 差异清单、§3.4 契约表、§5.4 三表一致性、gdb 命令、`delta` 角色互换等 36 条 | 下一篇闭环轮逐条复核；按下面三组分别处理，不合并成一个大桶 |
| D02-F10a | READER-A 10 条 | A-01/02/03/04/05/06/07/09/11/12：反例不可达、`push 0`、四条前置条件、gdb 命令、载荷存活、保护结构状态 | 闭环轮；优先 A-01/A-02（反例不可达）与 A-05（载荷存活表述） |
| D02-F10b | READER-B 13 条 | B-01/02/03/05/06/07/08/09/10/11/12/13/14：职责重叠、外部事实未锚、标题与正文、容量段、重复句 | 教学债批次；优先 B-01/B-03/B-07 |
| D02-F10c | READER-C 11 条 | C-04/06/07/08/09/11/12/13/14/16：悬空反例、两套机制表、四条路径、panic 前缀、外部符号、角色互换、2 MiB 档 | 闭环轮；优先 C-04/C-08/C-11 |
| D02-F11 | DESIGN-12 | `check-layout.sh` L9 只对 riscv64 做等值对账；aarch64 `bootface_a64.rs` 的 `KERN_SIZE` 只有上界断言 | ✅ CLOSED（按 `D-107`）：L9 按架构选文件，三架构 `check-layout.sh all` PASS |
| D02-F12 | 复验 | `cargo test -p minix-arch` 的测试模块编译失败（沿用 D01-F01） | test-audit / code-excellence 批次 |

## R1 设计 P3（suggestions，不阻断）

> D-107 终态：不再保留“不实施（建议级）”。

| ID | 摘要 | 处置 |
|---|---|---|
| RD1-DESIGN-D2-07 | `jump_to_kmain` 第二形参 | [CLOSED] implemented：F01 / 提交 `83e8cbb2b` |
| RD1-DESIGN-D2-08 | `HIGHER_HALF_CALLED` 等测试静态 | [CLOSED] implemented：D2-08 / 提交 `ecfef0ae9` |
| RD1-DESIGN-D2-09 | `virtual_end` 死变量 | [CLOSED] implemented：D2-08 / 提交 `ecfef0ae9` |
| RD1-DESIGN-D2-10 | x86 `arch_boot` 临时取证打印 | [CLOSED] implemented：R34 / 提交 `925c19fc0` |
| RD1-DESIGN-D2-11 | 三份无读者 arch `link.ld` | [CLOSED] implemented：D2-06 / 提交 `503ae169e` |
| RD1-DESIGN-D2-12 | `KernelInfo` 字段重复/派生 | [CLOSED] already-fixed：v10 删除派生字段（D-062） |
| RD1-DESIGN-D2-15 | 直核面静态与载荷 | [CLOSED] implemented：D-086 owned 路径 + F08 共享流程 / 提交 `6d0334dc6` |

## 本次状态核实（D-107 清零口径）

| ID | 当前状态 | 说明 |
|---|---|---|
| D02-F01 | [CLOSED] | `jump_to_kmain(kinfo)` 三实现从 `kinfo.kern_stack_top()` 现算；提交 `83e8cbb2b`；`cargo test -p minix-kernel --features mock --lib` 866/0/8 |
| D02-F02 | [CLOSED] dup-of | wire v10 / D-062 删除 `kern_stack_top` 字段，等式校验不再有 wire 对象 |
| D02-F03 | [CLOSED] dup-of | 三份 arch `link.ld` 按 D-107 删除（D2-06 / `503ae169e`） |
| D02-F04 | [CLOSED] | `segment_spans` 两版共用；`test_buffer_and_phys_loaders_share_segment_spans`；提交 `f50159798` |
| D02-F05 | [CLOSED] | `KernelImage`/`ValidatedKernelImage`；提交 `1460d4147` |
| D02-F06 | [CLOSED] | 三份 `.ld` 收口/上界 `ASSERT`；`check-layout.sh all` PASS；提交 `c376f5371` |
| D02-F07 | [CLOSED] already-fixed | `resume_additive` 已把三条 additive-resume 收成泛型 |
| D02-F08 | [CLOSED] | `run_self_boot` 共享流程，两壳只剩常量/横幅/console；提交 `6d0334dc6`；`check-layout.sh all` PASS |
| D02-F09 | [CLOSED] already-fixed | 保留 `Paging`/`HugePages`/`BootArch`（保留项不是缺陷，类型边界已有测试）；翻译残留清单作为 code-excellence 输入 |
| D02-F10 | [CLOSED] | 读者 36 条全部在 R4 表逐条 implemented/already-fixed/dup-of；见 `suggestions.md` |
| D02-F11 | [CLOSED] | L9 三架构等值对账；`check-layout.sh all` → 三条 L9 PASS |
| D02-F12 | [CLOSED] | `cargo test -p minix-arch` 266/0 |

### D02 §5.3 测试缺口（R02-T）

| ID | 当前状态 | 验收 |
|---|---|---|
| R02-T01 | [CLOSED] | `check-layout.sh` L10 机械对账 unpaged 名单与 `kernel.lds` 收段，断言 `mpx.o` 不在名单；提交 `fd456926a` |
| R02-T02 | [CLOSED] dup-of | `link_ld_lockstep.rs` 随 D2-06 删除，生产脚本门替代 |
| R02-T03 | [CLOSED] | riscv64 载体 `pc_at_high` 并入 PASS 条件；`run_qemu.sh` PASS（PC=0xffffffc000200146）；提交 `94d5f2379` |
| R02-T04 | [CLOSED] | 三份 `jump_to_kmain` 生产路径断言 `stack_top` 对齐，x86 另断言入口 RSP；提交 `550d8c0b8` |

## R5 重写基线（2026-11-21）

| ID | 来源 | 内容 | 去向 |
|---|---|---|---|
| F-R5-01 | G11 code-publication | 30 个关联文件有 73 处存量过程注释；本任务 `os/` 冻结，不改 | 下一代码批次 |
| F-R5-02 | G13 tutorial-rubric | 6/6 章缺 TR-1..TR-6（与旧 v3 基线同型，存量） | 教程化批次 |
| F-R5-03 | todo 哨兵 | `D02-EXTERNAL-D107` 仍 `blocked: true` | 外部线程完成并经用户确认后关闭 |
| F-R5-04 | QEMU | 本轮文档-only，未重跑 `test-higher-half` 与三架构门 | 下一代码批次 |
| F-R5-05 | 同模型 | 面板与 certify 均为 deepseek-v4.1-flash 冷启动实例 | 有条件时跨工具只读复核 |

| F-R5-06 | certify 观察 | `os/kernel/src/lib.rs` 的 Step 1 注释仍写“同尺寸重写返回 Ok；AlreadyMapped 只在 1 GiB 叶时出现”，与三份 `map_huge` 实现不符 | 下一代码批次清注释；本轮文档已按实现写 |
