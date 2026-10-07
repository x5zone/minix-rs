# NK4-C 增量评审报告 R3.1（2026-09-30）

- **范围**：`956e4a57f..4e3cab8d5` = **145 笔**（截断面冻结于评审启动时刻；此后主线在制改动归 R3.2）
- **段构成**：评审线 SELF 4 笔（b4a02f0b4 / 17a989fa3 / 41d374476 R3 报告三连 + 899c5fb8d 交接件）；主线 AGT 141 笔
- **方法**：R3.1 协议（`NK4C-REVIEWER-SESSION-RECORD-20260930.md` §2）+ code-excellence 四件套（C 锚点核对 / 设计对比 / 非法态封堵 / 回归测试有效性）；P1 机械对账 + python 字节级权威复核双轨
- **状态**：**定稿**（P0-P5 全完成）
- **增量协议**：R3.2 = 自 `4e3cab8d5` 起，逐次追加；方法与本报告 §一/§二 相同
- **产物**：本报告 + `NK4C-R31-P1-AUDIT-TABLE-20260930.txt`（145 笔机器初筛表）

---

## 结论先行

1. **无虚构取证、无验证虚报**：145 笔 message 声明与 diff 事实对账零矛盾；`minix3/`、`AI-chats/daily.todo.md`、`os/target/`、规范源（CLAUDE.md/AGENTS.md/prompt/）**零触碰**；已提交 diff 中**探针残渣零命中**（全部 TEMP 探针确已滚回）。
2. **零 P0 / 零 P1-正确性**：27 笔深审（24 笔 os/ 生产码 + 3 笔 tools）全 PASS（19）/ PASS-with-NIT（8），无 FAIL。
3. **里程碑独立复证为真**：评审自建仓外 worktree @ `4e3cab8d5`，docker 三件套 **kernel 831 / arch 243 / vm 531 零失败**；x86_64 `-smp4` 真机 ×2 轮 **marker=2/1、panic=0、pfVM=0、vec6=0**。
4. **发现全部为 P2 级**（8 项 F1-F8 + 6 项 NIT）：文档同步、message 精确度、接线类测试缺口、卫生项；无一影响已达成行为。

---

## §一 P1 全量机械对账（145 笔）

### 1.1 方法

脚本逐笔 `git show --numstat`（merge 笔 fallback `diff-tree -m`），四判据：①message 声明（零代码/纯取证/tracked 净/探针已滚）vs diff 事实；②保护文件（minix3/、AI-chats/daily.todo.md、os/target/、os/.dockercargo）；③体量异常旗标（+>3000）；④段归属用 `git log --format=%h` 短哈希直迭代（无集合比对环节）。**v2 新增**：对全部非文档路径（notes/、tmp/、.review/ 之外）的新增行做探针残渣模式扫描（`nk[0-9][0-9]` / `NK[0-9]+-TEMP` / `-TEMP`）。

**工具链事故与补救（如实登记）**：本机 `grep` 实为 ugrep 7.8.4（`/usr/bin/grep --version` 实证），对含无效 UTF-8 的文件（初版表 `cut -c` 截断多字节所致）**静默吞掉全部输出**——旧机教训（R3 P6）在新机复现。补救：全量安全攸关扫描（保护路径 / 探针残渣 / 声明-事实对账）改用 **python3 字节级重跑**，结果与 shell 版一致且更可信。此教训已入评审线工具规范：本仓取证文本处理一律 python。

### 1.2 结果（python 权威扫描）

| 判据 | 结果 |
|------|------|
| 保护文件触碰 | 仅 378a52370 + 0859580eb 两笔涉及 `os/.dockercargo`（合法解除跟踪，纯删除 add:0/del:17716）；`minix3/`、`AI-chats/`、`os/target/` 零触碰 |
| 规范源触碰 | CLAUDE.md / AGENTS.md / prompt/ / .claude/ / .codex/ / .trae/ 零触碰 |
| 探针残渣（非文档新增行） | **0 命中**（145 笔全扫）——"探针已滚/tracked 净"声明全部属实 |
| docs 声明 vs diff | **0 矛盾**（含纯取证/纯文档/零代码/WORKLOG-only 各形态） |
| 体量异常 | 1 笔 BIG+3531（c63d4ae6d，纯迁移文档入库，裁决 ok） |
| merge 笔 | 1 笔（0859580eb，C-62 tools 分支合入，父 f866473ed+378a52370，增量清单核实干净） |

### 1.3 旗标行裁决（31 行全裁决，明细）

- **PROTECTED ×2**：378a52370（解除 .dockercargo 跟踪，158 项全 D，根 .gitignore 补行实归属 5aa2d74f0）+ 0859580eb（merge 携带该删除）——R3 待办①收尾，**ok**。
- **BIG ×1**：c63d4ae6d——`migrate_notes_plan/` 六份迁移笔记 + misc_concepts.md，纯文档，**ok**。
- **OS-CODE ×29**：即深审候选集（24 笔 os/ 实码 + 3 笔 tools + 2 笔 .dockercargo 重叠），归 §二。
- **CLAIM? ×1**：9f5d9249c——shell 正则误报（message 明写"回归断言入 test_vfs_init_messages"），**ok（误报澄清）**。
- **SELF ×4**：R3 报告三连 + 899c5fb8d 交接件，notes-only，**ok**。

---

## §二 深审判定表（27 笔）

> 行号锚点基准 = 截断面 `4e3cab8d5`（经仓外隔离树实读 + `git show <hash>` 双通道）；C 锚点全部打开 minix3 源核对。子代理证据 + 评审人 4 项载荷断言抽查（proc.c:571 / region.c:656 / main.c:912-914 / b09665415 disjoint 声明）全部证实。

### 2.1 批A x86 SMP 域（5 笔，全 PASS）

| commit | 内容 | 判定 | 关键证据 |
|--------|------|------|----------|
| 5730112ce 续-54 | ready queue 下沉真 per-CPU 数组 | **PASS** | C 锚点五处全实证：cpulocals.h:58-59、proc.c:1614（enqueue 按 rp->p_cpu）、:1688（enqueue_head）、:1738（dequeue）、:1801（pick_proc 本地 head）；旧"BKL 串行化故共享队列等价"错论断被显式推翻并全文档同步；NIT：`sched_idx`（proc_table.rs:692-697）release 下越界静默回退 BSP 仅 debug_assert——有 validate_cpu_param 边界守卫，可接受 |
| 6e3963ca9 续-56 | per-CPU root_phys + switch_address_space_idle | **PASS** | klib.S:618（`mov %cr3,%ecx`）、proc.c:161-171/186-196 实证；`try_smp_state` 容错启动期（首版 `smp_state_boot_unchecked` 灾难已修在提交码内）；`reset_root_phys_for_test` 双清；诚实前提注记（idle/cpu 同体性） |
| a251ba77c 续-72 | idle() 补 AP stop_local_timer | **PASS** | proc.c:194-196 逐字镜像；CONTRACT 注释完整登记三未还臂（arch_clock.c:351-369 实证）；`ncpus>1` 门与 C CONFIG_SMP 等价（单核必走 BSP 分支） |
| beda55e02 续-73 | schedctl cpu→BSP 过渡钳 | **PASS** | 纯函数+宿主测试（host 826→827 与链一致）；校验先于钳制保留 C 错误路径；可观测效果对位 C non-SMP（system.c:686-689 实证）；副作用（SCHED 账本分歧）已声明；两腿（syscall_process.rs:802/:1675 区）都汇入 sched_proc，钳制覆盖完整 |
| ad9d37429 §1.118 | SCHED pick 幽灵核（R3 初读转正审） | **PASS** | schedule.c:67 `c < machine.processors_count` 界语义忠实；回归测试 `test_phantom_seats_beyond_topology` 双断言在旧码（全数组迭代）下必失败，真钉 |

**批A 横向一致性**：续-72 CONTRACT arm#3 → 续-73 兑现；arm#1/#2 在 MIGRATION §5 仍诚实挂账。R3 NIT#4（SMP 债）会计链条闭环。

### 2.2 批B riscv64 桥（4 笔，全 PASS）

| commit | 内容 | 判定 | 关键证据 |
|--------|------|------|----------|
| b68e0fd44 续-75 | IPC 桥+park 机制全量（+859） | **PASS** | asm 三关键腿逐一核：park 分支 `la`+`jr`（R_RISCV_JAL ±1MiB 悬崖，S3）；内核腿尾声按 SPP 重锚 stvec（新发现缺口自闭，防 S 返回后 sscratch 交换毁内存）；`restore_to_user` sret 前 sscratch 重固定（KERNEL_TRAP_STACK_BASE no_mangle 格，protection.rs:81-111）；槽位映射带 4 判别测试（帧布局冻结/SPP/槽位互异/A1 车道）；EBADCALL/EFAULT 路径 BKL 释放纪律正确；ENOSYS_CODE 退役；门控三架构放宽；set_secondary_ipc_return→GP_A1（C arch_system.c:184-186 对位） |
| 3e465d3fd 续-76 | KernelUserCopy 丙案 [ARCH: user-copy-via-dm] | **PASS** | 两腿改 walk(root)→PA→DM 窗口（复用 senda 先例）；读腿 root=None→Err(PageFault)（C system.c:152-155 对位）；写腿保留 W 位校验（copy_via_root_pages 只校 U 的差异明示）；CodeReview N1 采纳可见（Message 类型化对齐） |
| 583dcee72 续-76b | riscv64 缺页腿 A.8 | **PASS** | PFEC 车道 12→0x15/13→0x01/15→0x03 由测试钉死且与 aarch64 转换器对账（trap_dispatch.rs:2561-2575）；ForwardToVm 共享腿镜像 aarch64 §1.116；sepc 不步进正确（非 ecall） |
| e25f3ee34 续-77a | write_pte_dm 通道门控 A.5 | **PASS** | sfence.vma 改 `if channel==KernelDm`（与 arm64.rs:246/x86.rs:190 先例同构）；安全论证三条件（map 拒 AlreadyMapped / walk_alloc 只建缺失 / V=0 不缓存）成立；present→X VmDm 残余缺口如实登记不加宽 |

**批B 横向**：riscv-reviewlog §A.5/A.6/A.7/A.8 四个登记缺口在本批全部实质闭合——**但 reviewlog 文件本身未更新状态标记**（见 F1）。

### 2.3 批C IPC/VFS/PM（8 笔：4 PASS + 4 PASS-with-NIT）

| commit | 判定 | 一级理由 |
|--------|------|----------|
| f866473ed fork 失败腿 detach_scheduler | **PASS** | C main.c:378-385 逐字（:380 `mp_scheduler = NONE`）；下游链（forkexit.c:425 + sched_stop.c:16-17 + exit.rs:505 门）完整；定序测试真钉 |
| 6e78f233a sendrec REPLY_PEND | **PASS** | proc.c:569-571 逐字（我本人复核）；Delivered 秒达腿缺口真实；快路径护栏断言防假阳性；Error 腿 clear 是文档化的有意偏离 |
| 9306614bf notify dst-REPLY_PEND 门 | **PASS** | proc.c:1142-1143/1148/1164-1165 全精确；测试三断言（不直投/RECEIVING 保持/位图 deferred）真钉 |
| 07c9e6649 sched_start 补校回复 m_type | **PASS-with-NIT** | sched_start.c:87 精确；测试真钉；NIT：魔数 208 内联（F9）；告警腿锚引 schedule.c:60-67 实为 44-47（F5） |
| 7a89f2b72 virtual_copy_f EDOM 前置 | **PASS** | memory.c:607-608（引 606-607 偏一行，F5）；顺序语义（检查先于任何解析）由"不可解析端点+0字节"测试真钉；memset 腿不波及与 C 对位 |
| e55057d1c VFS send_work 排水接线 | **PASS-with-NIT** | 缺口实证（修前 flush_send_queue 生产零调用）；comm.c:42 零开销门逐字；NIT：接线本身无回归测试（F6）+ 排水点位与 C 不同位（功能等价） |
| cd613370d service_pm 尾阻塞 send | **PASS-with-NIT** | main.c:912-915 逐字（我本人复核）；NIT：行为翻转零测试 + 单线程 .expect 全服务停摆面（注释自证，F12 观察）+ x86 3/4 run 未达 marker 未深挖（F6） |
| 2113be7fc commit_message_to_memory 屏障 | **PASS-with-NIT** | 机制核验成立（trap asm 标量传指针，LLVM 无溢出义务；compiler_fence 不够的论断正确）；三腿覆盖划分合理；NIT：padding 字节形式 UB（F10）+ 逐 trap 点手工插屏障的脆弱性（更原则化=asm memory clobber，登记 OQ 交主线斟酌） |

### 2.4 批D VM/页表（3 笔：2 PASS + 1 PASS-with-NIT）

| commit | 判定 | 一级理由 |
|--------|------|----------|
| 4cb8db458 续-18 switch_address_space 读活寄存器 | **PASS** | klib.S:618-624 逐字；skip 路径同步 ptproc 是文档化有意偏离（C 的 je 同跳过）+ 测试同步翻转；与续-56 不打架（决策读硬件不变，镜像收口 per-CPU）；NIT：缺"镜像≠硬件不得 skip"（false-skip 原始形态）直接回归针（F11） |
| 82abf5b64 续-36 pt_bind 对位腿 | **PASS** | exit.c:137 逐字命中；fail-closed 为文档化偏离（C 丢弃返回值）；audit.rs try_borrow_mut 化解双借；MockGateway 4 断言实钉 |
| 58c0a51d3 续-45 anon 帧清零 | **PASS-with-NIT** | region.c:645-656 + alloc.c:452 逐字（645/646 一行差，F5）；门位置正确（alloc 后 map 前）；NIT：host 侧 clear_phys_page 空 stub，字节级清零仅真机可验 + 无 to_alloc_flags 含 CLEAR 的 gate 断言（诚实披露，F11） |

### 2.5 批E 测试/脚本/卫生（4 笔 + tools 3 笔轻审）

| commit | 判定 | 一级理由 |
|--------|------|----------|
| b09665415 续-51 dm_coverage 验证链 | **PASS-with-NIT** | 3 个 mock 测试实存且断言有效（变异判定 M1/M2 记录在 WORKLOG）；**NIT：message 称"删除被证伪的 disjoint 断言"实际未删**（新文档化+保留，WORKLOG 自己登记）——message 精确度 F2 |
| 9f5d9249c 续-79c per-msg 断言 | **PASS** | 断言只锁纯逻辑面，与声明自洽（未声称抓运行期堆变异） |
| 7ccaf77e9 续-77g uboot 分区盘化 | **PASS-with-NIT** | bash -n 过；NIT：依赖预检漏 fdisk（无 fdisk 宿主 FAIL 而非 SKIP，破"宿主差异记 SKIP"姿态）+ count=28671 魔数无解释（F8） |
| 29b28f8c8 续-77f 脚本三连适配 | **PASS** | 与 77g 叠加自洽（77f 为 77g 祖先，无残迹冲突） |
| 5aa2d74f0 / 162b1cd07 / 396f1f5db tools | 2 PASS + 1 PASS-with-NIT | 触面仅 tools/+notes/ 零生产码；P2_TESTS 计数声明 7/1/15 与 python 实数逐一吻合（20→27→28→43 单调）；selftest 证据在 notes/…/evidence/ 实存；NIT：396f1f5db 的 6 个证据文件落仓库根 `evidence/` 偏离既有约定位置（F7） |

---

## §三 架构专项

1. **[ARCH: user-copy-via-dm] 三处一致**：code（os/kernel/src/ipc.rs:388/514/554）+ doc 层（NK4C-WORKLOG、NK4C-RESUME-PROMPT roadmap 2.2/3.2 销账、misc_concepts）一致。NK4-C 域无 stage 级 .design 文档，其文档层即 WORKLOG/RESUME-PROMPT——按域内约定判定合规。
2. **riscv64 桥不需要新 [ARCH]**：续-75 系镜像既定三架构设计（x86 int-33 / aarch64 §1.112），属补全非演进；续-76b/77a 同为既定形态镜像——判定正确未滥标。
3. **R3 §7.3 五条待办销账**：①.dockercargo **已闭**（378a52370+5aa2d74f0）；②1.12a 注释矛盾 **仍开**（截断面 ipc.rs:1150-1154：注释"满丢最旧/不覆盖最新" vs 代码满则拒写新目标=丢最新，F3）；③kernel-image 交付边界双架构 + `[ARCH: boot-handoff]` 字面标注 **仍开**（main.rs:17-23 未更新，两文件字面 0 命中，F4）；④B39 离散帧 mock 测试 **仍开**（增量内无相关 commit）；⑤task1-close 死探针全量裁决 **按计划仍开**。
4. **aarch64 续-78~86 取证链防虚构**：P1 层零矛盾 + 探针残渣零命中；续-84/86 两轮以"探针观测效应"收尾、续-85 对续-80 的标签错误公开翻案——自我纠正路径完整可追溯，无编造证据形态。登记方法论风险见 §六 Rule Discovery。

---

## §四 卫生审计

1. 保护文件与规范源零触碰（§1.2）。
2. 探针残渣零命中（§1.2）——十二轮取证全部干净收尾。
3. message 精确度：145 笔中 1 笔过度声明（F2）、3 处锚点行号偏移（F5：07c9e6649 schedule.c:60-67→44-47；7a89f2b72 memory.c:606-607→607-608；58c0a51d3 region.c:646→645）——三处偏移结论均不受影响，且整体锚点质量显著高于基线（27 笔深审中 22 笔锚点逐字精确）。
4. 评审线 SELF 4 笔 docs-only 合规。

---

## §五 独立验证（隔离 worktree）

**方法**：`git worktree add --detach /home/xzhao/github/minix-rs-r31wt 4e3cab8d5`（仓外，主线工作树零污染）；docker 测试用独立 `CARGO_TARGET_DIR` 防根属主混染宿主构建；QEMU 用 OVMF VARS 临时副本（跟踪本体未触碰）。

| 项 | 命令 | 结果 | 判据 |
|----|------|------|------|
| docker 三件套 | `docker run --rm -v "$PWD:/work" -w /work -m 2g minix-ci:1.94 cargo test -j 1 -p {minix-kernel,minix-arch,minix-vm}` | **kernel 831 / arch 243 / vm 531，0 failed** | 只增不减：820+ / 243 / 531+ ✅ |
| 镜像 | `ulimit -v 3145728; cargo run -q -p xtask -- image --arch x86_64 --release` | exit=0 | — |
| x86_64 -smp4 ×2 | q35 + OVMF 4M + `-smp 4 -m 512M`，串口落文件 | r1 **marker=2** panic=0 pfVM=0 vec6=0（14076 行）；r2 **marker=1** 同零（9129 行） | marker≥1、panic/pfVM/vec6=0 ✅ |

**结论**：目标① x86 分支与目标② echo/ls/cat 所依赖的 boot 链在截断面可独立复现，R3 里程碑在 +145 笔后未回归。

---

## §六 结论与移交

### 6.1 总判定

- **零 P0 / 零 P1-正确性 / 零违规**；深审 27 笔：19 PASS + 8 PASS-with-NIT + 0 FAIL。
- 主线 141 笔的修复质量延续 R3 结论：C 锚点真实（22/27 逐字精确）、无虚构取证、验证链申报与事实一致、失败与翻案诚实记录。
- 发现 8 项 P2 + 6 项 NIT，全部登记如下，交主线排期（评审线不修正确性、不动生产码）。

### 6.2 登记待办（滚动，含 R3 遗留）

| # | 级别 | 内容 | 锚点 |
|---|------|------|------|
| F1 | P2-doc | riscv-reviewlog §A.5/A.6/A.7/A.8 状态标记未随 续-75/76/76b/77a 更新（仍述开放），闭合事实只在 WORKLOG/message——建议补状态标注行 | riscv-reviewlog.md:299/331/382/428 |
| F2 | P2-doc(message) | b09665415 声称"删除被证伪的 disjoint 断言"实际保留（已文档化替代）；WORKLOG 有诚实副本 | dm_coverage.rs boot_module_candidates docstring |
| F3 | P2-doc | R3 待办②持续：注释"满丢最旧/不静默覆盖最新" vs 代码满则拒写新目标（实丢最新） | os/kernel/src/ipc.rs:1150-1154 |
| F4 | P2-hygiene | R3 待办③持续：kernel-image 交付边界节未更新三架构状态 + `[ARCH: boot-handoff]` 字面标注缺 | os/kernel-image/src/main.rs:17-23 |
| F5 | P2-doc | 锚点行号偏移 ×3（07c9e6649 / 7a89f2b72 / 58c0a51d3，见 §四.3） | 各 commit |
| F6 | P2-test | 接线类修复测试缺口：e55057d1c（排水接线无测试）、cd613370d（send_reply PM 腿翻转零测试 + x86 1/4 run 未达 marker 未追）、2113be7fc（host 结构性不可测） | 各 commit |
| F7 | P2-hygiene | 396f1f5db 证据文件落仓库根 evidence/，偏离 rewrite-notes/evidence/ 约定 | evidence/20260930-c64-pattern-gate/ |
| F8 | P2-hygiene | 7ccaf77e9 依赖预检漏 fdisk（FAIL 应为 SKIP）+ dd count=28671 魔数无解释 | os/qemu-tests/test-riscv64-uboot.sh:31-34 |
| NIT | — | 07c9e6649 魔数 208；2113be7fc read_volatile 触 padding 形式 UB + 逐点手工屏障脆弱性（OQ：是否改 asm memory clobber）；4cb8db458 缺 false-skip 回归针；58c0a51d3 host 无字节级清零验证；5730112ce sched_idx release 静默回退；cd613370d 单线程停摆面（注释已自证） | 见 §二 |

### 6.3 R3.2 增量协议

- 已审界推进为 `4e3cab8d5`；范围 = `git rev-list --count 4e3cab8d5..<届时 HEAD>`；方法与 §一/§二 相同；重点预告：主线在制的 riscv64 装机面（boot-shim/kernel-image，工作树在制中）与 aarch64 mt=8 后续轮。
- P1 对账脚本建议固化：本轮 v2（含探针残渣扫描 + python 权威复核）可作为评审线标准脚本入库。

### 6.4 Step 5.7 Rule Discovery（本轮新沉淀）

1. **ugrep 静默吞输出**（工具链）：无效 UTF-8 文件上 ugrep 全量吞输出且 -c 也不出数——新机复现证明非单机偶发。对策已落地（python 字节级权威扫描），建议升级为评审线硬规约。
2. **取证探针观测效应模式**（评审侧新）：探针时序扰动本身消除/迁移被观测故障（续-79c-f4、84、86 三度出现）。评审取证轮必须把"探针自身扰动"列为竞争解释；主线已在续-77f 探针纪律升级中部分自愈。
3. **接线类修复测试缺口模式**：修复代码与缺陷现场均在不可单测面（生产主循环/asm 边界），回归依赖真机。对策方向：接线点提取为可测函数（部分 commit 已用该法，e55057d1c 未用）。

### 6.5 验证局限（同 agent 披露）

本评审为单 session 同 agent（zcode/GLM）执行：P1 权威扫描独立于 shell grep（python 重跑）、深审证据由两个只读子代理收集后经评审人 4 项载荷断言抽查证实（proc.c:571、region.c:656、main.c:912-914、b09665415 disjoint 声明），独立验证（§五）为评审人自建环境实跑。跨 agent 交叉验证（Trae/Claude）本轮不可用，读者可按 §五 命令重放。

### 6.6 产物清单

- 本报告：`NK4C-REVIEW-REPORT-R31-20260930.md`
- P1 对账表：`NK4C-R31-P1-AUDIT-TABLE-20260930.txt`（145 笔机器初筛 + 旗标）
- 评审线进度账：`.review/zcode/edge5/REVIEW-R31-STATE.md`（本地 gitignored）

**lint 口径**：本报告 1 条 SL-4（标题日期行）与 R3 报告（:3，同形态 1 条）为同款先例基线；P1 表 3 条 SL-4 全部来自 verbatim 引用的 commit message（保真优先，不作改写）。doc-style-lint --diff 另命中的 3 条属主线在制 WORKLOG，非本评审产物。
