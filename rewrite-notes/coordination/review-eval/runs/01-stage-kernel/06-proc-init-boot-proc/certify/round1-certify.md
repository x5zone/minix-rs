# D06 R1 certify 独立回读回执（整改复核版，覆盖首轮回执）

- 结论：PASS
- 核验条数：12
- CLOSED：12；需整改项：0

## 1. §5.1 构建形态句（default 口径）
- 判定：CLOSED
- 证据：L2785 现为「`os/kernel/Cargo.toml` 的 `default = []`——SD-30/U-1 已摘默认形态，宿主测试必须显式 `--features mock`，裸跑 `cargo test -p minix-kernel` 会撞 `form_contract` 编译期绊线；生产形态走 `--no-default-features`」。对照 `os/kernel/Cargo.toml`：L31 注释「SD-30/U-1 已摘默认……`default = []`——不再有隐式形态」「裸 `cargo test -p minix-kernel` 会以生产形态编 test 目标而失败……必须带 `--features mock`」，L38 实际 `default = []`——句句对上。`grep -n 'default = \['` 全篇唯一命中即此行；`grep -n 'form_contract'` 该文档现命中 L2785（首轮为 0）。

## 2. §4.2 SD-17 引文的块注
- 判定：CLOSED
- 证据：L2480 块注现为「……`ARGC_ARGV_ENVP` 常量与它的文档注释；按当前源码原样引用，注释已无内部编号与日期，按原文保留。」——规定的「按当前源码原样引用，注释已无内部编号」在位；`grep -c 'SD-17'` 全篇 → 0。块注的实质断言为真：`os/arch/src/arch/boot.rs:434-444` 现行英文注释确无内部编号与日期（commit 0d235964a 英文化），「注释已无内部编号与日期」与引文、与源码三方一致，首轮「块注与引文自相矛盾」的问题解除。残留观察（不阻塞本条，供下一轮采纳）：引文本体 L2469-2477 仍是旧中文改写体，与现行英文源码注释逐行不同（连改写前的旧源也作「exec 腿」、引文作「exec 路径」），故「按当前源码原样引用」在字面层只成立于内容忠实、不成立于逐字；块注尾部「按原文保留」为旧句残留。建议下一轮把引文本体换成 boot.rs:434-444 现行英文注释。

## 3. §3.7 NK4-A 引文
- 判定：CLOSED
- 证据：L1892「through a stale mapping.」与 `os/arch/src/arch/boot.rs:132` 逐字一致（枚举引文整体与源码逐字对上，`sed -n '120,136p'` 比对）；块注 L1897 现为「（代码引用，`boot.rs` 的 [`enum VmLoadError`](os/arch/src/arch/boot.rs:enum VmLoadError)——按当前源码原样引用。）」，首轮那半句失效的「含仓库内部阶段编号与日期」已摘除（`grep -n '含仓库内部'` → 0 命中）。

## 4. §3.2 D-58 引文行
- 判定：CLOSED
- 证据：L1471「// C main.c:244: route the seed template mask through the」与 `os/kernel/src/kpriv.rs:1214` 起整段逐字一致；块注 L1481「……末段；按当前源码原样引用。」。C 源真值：`grep -n 'fill_sendto_mask(rp, &map)' minix3/minix/kernel/main.c` → 244。

## 5. §1.3「另外两个字段」
- 判定：CLOSED
- 证据：L134 现为「`struct boot_image` 的另外三个字段（`endpoint`/`start_addr`/`len`）此时是空的」，与 type.h 引文的 5 字段（2 静态写 + 3 回填）口径一致。按要求核对了 L2513：该处「另外两个字段」说的是 `sched` 与 `vm_request_queue`，确实只有这两个字段，表述正确、保持原样未动。

## 6. §2.1 开头 40 计数口径
- 判定：CLOSED
- 证据：L265「（口径：按 `CONFIG_SMP` 定义、`DEBUG_TRACE` 未定义的内核配置计）」；`minix3/minix/kernel/proc.h:36` `#ifdef CONFIG_SMP`（p_cpu_mask/p_stale_tlb 受守卫）、`minix3/minix/kernel/debug.h:33` `#define DEBUG_TRACE 0`（口径意图为不启用，计数 40 两写法一致）。

## 7. §2.1.2 p_stale_tlb 写点
- 判定：CLOSED
- 证据：L327 三处写点；`grep -rn 'p_stale_tlb' minix3/minix/kernel | grep -v 'proc\.h:'` → do_vmctl.c:159（`case VMCTL_VMINHIBIT_CLEAR:` 分支内）、do_update.c:162-163、i386/memory.c:180,184（UNSET_BIT；earm/memory.c:185,189 镜像）。

## 8. §2.1.6 boot 期 18 位
- 判定：CLOSED
- 证据：L474「boot 期这 18 个位全都不被写（`p_misc_flags` 保持 BSS 零）；唯一一次 MF 位写动作在运行期的槽回收——`do_clear` 清 `MF_FPU_INITIALIZED`」；`grep -n 'p_misc_flags' minix3/minix/kernel/main.c` → 0 命中；`do_clear.c:61` `rc->p_misc_flags &= ~MF_FPU_INITIALIZED;`。

## 9. §2.9 汇总表用户模块行
- 判定：CLOSED
- 证据：L1299「模块地址已登记（名字仅前 6 项有：其余 11 个用户模块 boot 期没有名字，见 §2.1.1）」；数字自洽（前 6 项 = 5 内核 task + VM；12 模块 − VM = 11）。

## 10. §3.8 排除集
- 判定：CLOSED
- 证据：引文数组 `VM_BOOT_EXCLUSION_CAP`（L1961）；散文 L1976「`VM_BOOT_EXCLUSION_CAP = NR_BOOT_MODULES + 3`（`lib.rs:2557`）」；`sed -n '2557p' os/kernel/src/lib.rs` → `const VM_BOOT_EXCLUSION_CAP: usize = crate::proc::NR_BOOT_MODULES + 3;`，行号与表达式双双命中。

## 11. lib.rs 两处 C 坐标注释
- 判定：CLOSED
- 证据：`git diff -- os/kernel/src/lib.rs` 恰两处：`main.c:226`→`main.c:253`（工作树 L2731）、`main.c:267-270`→`main.c:264-267`（工作树 L3009）。真值：`minix3/minix/kernel/main.c` 253 = `RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM);`；264-266 = `if(rp->p_nr != VM_PROC_NR && rp->p_nr >= 0)` 加两个 `RTS_VMINHIBIT`/`RTS_BOOTINHIBIT` 置位、267 = 收尾 `}`。文档侧引文 L1995、L2003 同步。

## 12. cargo check
- 判定：CLOSED
- 证据：本轮复核时在当前工作树重跑 `cd os && cargo check -p minix-kernel --features mock` → 退出码 0，尾行 `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`；全量日志 `grep -c '^error'` → 0（48 条 warning，日志中的 `warning[E0133]` 属 lint 告示，不是编译错误）。

---

备注：本轮复核用 `git diff` 的 17 个 hunk 逐一归账（4 条整改 + 首轮 13 处改动），无预期外改动；首回合已 CLOSED 的 8 条全部保持原判。条目 2 的残留观察已如上记录，属措辞精度层，不属本轮 12 条口径。

结论：回读验证通过，12 条逐条落证。
