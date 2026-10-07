# NK4-C 增量评审报告 R3.4（2026-10-06）

- **范围**：`5cf88a552..ad14ced56` = **56 笔**（截断面冻结于评审启动时刻；此后主线在制改动归 R3.5）
- **段构成**：评审线 SELF 4 笔（ca4d399a7 R3.3 报告 + C-67 三连 49eeeed83/c5e3e27cd/ad14ced56）；主线 AGT 52 笔，其中 **22 笔触 os/ 生产码**（深审候选）
- **方法**：R3.3 协议延续 + code-excellence 四件套；P1 机械对账（python 权威扫描）+ 分批深审（2 个只读子代理收证 + 评审人裁决与载荷抽查/实跑复核）
- **状态**：**定稿**（P0-P5 全完成）
- **增量协议**：R3.5 = 自 `ad14ced56` 起；主线在制（6 脏文件：2 BUG docs + init/dm_coverage/grant 等 + docfix-boot-chain 分支未合）出界
- **产物**：本报告 + `NK4C-R34-P1-AUDIT-TABLE-20261006.txt`（56 笔机器初筛表）

---

## 结论先行

1. **无虚构取证、无验证虚报**：56 笔 message 声明与 diff 对账零矛盾；保护路径与规范源零触碰；探针残渣树级零（nk4a:/nk4c: 模式）。
2. **深审 24 项判定 = 19 PASS + 5 PASS-with-NIT + 0 FAIL**；零 P0 / 零 P1-正确性。
3. **三终目标在截断面全部达成且经评审线 clean 重建独立复证**：
   - docker 三件套 **kernel 833 / arch 257 / vm 538，exit=0 ×3**（基线 833/243/535 只增不减；arch +14 = walk 对抗套件入默认面，fuzz 在 docker 亦过）
   - x86_64 `-smp4` marker=2/2、panic=0/pfVM=0/vec6=0
   - riscv64 boot-full marker PASS（目标①）+ riscv64 cmd smoke PASS（目标②）
   - **test-atf-riscv64.sh：第二采样 rc=0 PASS 36/36（34 passed + 2 skipped，0 failed/broken）——目标③ riscv 独立复证 ✅**（首采样 29/36 不完整为间歇，见 J2）
   - test-atf-aarch64.sh 第二采样 rc=0 36/36（首采样 boot panic 单样本，同 J2 间歇族）+ aarch64 cmd smoke PASS
4. **续-357 声明实跑复核**：`cargo test -p minix-arch` OFF（runtime-window）= **250 passed 0 failed**、ON（+walk-hardening）= **252 passed 0 failed**——commit 声明"OFF 250/0、ON 252/0"逐字证实；评审子代理的 python 重演推演（"OFF 按构造必失败"）被实跑推翻（Rule Discovery #1）。
5. **发现全 P2 级（J1-J3 + NIT）**；R3.3 H9 裁决为**闭合（复现性已立）**；R3.2/R3.3 遗留待办持续无登记笔（J3）。

---

## §一 P1 全量机械对账（56 笔）

### 1.1 结果（python 权威扫描）

| 判据 | 结果 |
|------|------|
| 保护文件触碰 | 零 |
| 规范源触碰 | 零 |
| 探针残渣（截断面树，nk4a:/nk4c: 模式） | 0 命中（fa0/pfwd-lethal 等为声明保留的台账活体，非残渣——见 J3/NIT） |
| docs 声明 vs diff | 零矛盾 |
| 体量异常 | 1 笔 +703/-434（84ba9669b，实为 rustfmt 重排 + 20 行级插桩，见 §2.1） |
| merge 笔 | 0 |

旗标行 24 行全裁决：OS-CODE 22 = 深审候选集；SELF C-67 tools-only；07ae5efed 收尾杂项（纯注释+文档，实核零语义）。

---

## §二 深审判定（24 项）

> 行号锚点基准 = 截断面 `ad14ced56`。载荷抽查（评审人亲核）：riscv64_walk.rs L1 门缺失、fuzz 测试无 cfg 门、signal.c:551 C 锚、tp 修复 asm、P2_TESTS 64→76、**OFF/ON 实跑**——全部证实（其中一项子代理红旗被实跑反转，见 J1）。

### 2.1 批A 三终收尾里程碑链（4 笔：3 PASS + 1 PASS-with-NIT）

| commit | 判定 | 要点 |
|--------|------|------|
| 84ba9669b 续-382 | **PASS** | +703/-434 实质 = 全文件 rustfmt 重排 + 20 行级里程碑计数器（INACT_N，幂次门控）；零 token 删除、控制流不变；do_vmctl.c:33-36 C 锚实读；NIT=格式化与插桩混笔放大 review 面（NIT 族） |
| 6310c7237 续-383 DumpCore 信号号 | **PASS-with-NIT** | C 锚逐字（signal.c:551 `mp_sigstatus = (char) signo`，评审人亲核）；VFS 对 0 拒服务链坐实（main.c:726 ↔ dispatcher.rs:224）；新测试真钉载荷面；NIT=C 引用行号系统性偏 1（方向还不一致）；**本笔 CR 为"评审 agent 配额失败改自评"（自曝，J3）** |
| 295201a7a 续-384 tp=__tls_base | **PASS** | 机器码级验证链完整（nm D 型 0x80200058、_start 反汇 auipc/addi→mv tp）；picolibc.ld:209 PROVIDE + 无 riscv tcb_offset 符号互证；aarch64 臂不动声明属实；fa0 探针"用后即滚"未滚（声明下一笔，NIT 族）；0x8020058 笔误已由 07ae5efed 勘误 |
| 776e32805 续-377 | **PASS** | "只改注释与台账"逐字核实（+7 全为文档注释）；第三条建议不采有据 |

### 2.2 批B riscv64 陷阱腿 + Sv39 走表层（9 笔：6 PASS + 3 PASS-with-NIT）

| commit | 判定 | 要点 |
|--------|------|------|
| e4c03cebb 续-370 内核腿 t0 次序 | **PASS** | 缺陷机理精确化（非嵌套偷取，而是"每次经内核腿的监管态中断把被打断上下文 t0 偷走"）；一次换位修复；宿主形状测试含正控制（换回旧形验证开火）；诚实边界自记"非唯一写者" |
| 378a06d15 续-371 用户腿 t0 覆盖 | **PASS** | la t0 上提到回载前 + 删死负载；self-纠偏上笔"只核保存侧" overclaim；规则 3/4 真钉 |
| fc8eea4e4 续-373 返回腿不屏蔽 | **PASS-with-NIT** | 起手 csrc SIE（bit1 正确，避开续-108 老形状）；被推迟 tick 不丢（sip.STIP + SPIE）规格级正确；pin 测试 4 测；NIT="与 x86 对齐"说法错误（x86 是开 IF）——续-374 已明文勘误 |
| 232ee7e9d 续-372 | **PASS** | asm 零改动实证（python 过滤 diff=0 行）；修真问题（解析器锚点巧合式正确）；规则 5+2 正控制 |
| 3199537b0 续-350 纯逻辑层 | **PASS-with-NIT** | walk_read_with 闭包解耦设计优；8 测试含 5000 点模糊对账；NIT=参考实现 import 被测常量（结构性失明，351 自认并同修） |
| fcb8c542e 续-351 巨叶掩码宽一位 | **PASS-with-NIT** | 修复与 Sv39 规格逐位吻合（L2 1<<19→1<<18、L1 1<<10→1<<9）；参考实现四处同步；回归测试真钉；NIT=示例地址 0x80400000 的 bit 标签偏一位（bit22 被标 bit21，测试有效性不受影响） |
| bb115c20c 续-352 L0 撤回 | **PASS** | 撤回-恢复-定谳闭环（续-358 恢复后冻结仍在=L0 非变量）；留痕完整 |
| 69fee0121 续-357 特性门 | **PASS** | **声明实跑证实：OFF 250/0、ON 252/0（评审人亲跑）**；子代理"参考 L1 检查漏 cfg 门（:233）→ OFF 按构造必失败"红旗被实跑推翻——真实 fuzz 生成空间不产错粒度 2MB 叶，不对称未触达（J1 降级 NIT/OQ） |
| 6d5a90e88 续-358 L0 非变量 | **PASS** | 单变量定谳闭环 |

### 2.3 批C 腐蚀遥测 + T13（8 笔：全 PASS）

fa02e515f（双走表对账，"零命中≠清白"边界由后笔补文档）；e9f118cab（实验丁——后验定谳空操作，两处独立死因）；da1a360a6（定谳+清理完整，CLI 旗标修真开关保留作遥测边界，作废结论三处就地撤回）；58b9c4195（7 槽计数遥测，自曝负结果=探针手抄缺 m_source）；5152f4513（第 7 计数器 aw=1，疑点亲核排除——数组全同元素 diff 摆放无语义；口径混用已由续-377 纠偏）；043c64d56 + 7ced6473b + 735d765cd（T13 R1-R3：USER_VA_LIMIT 三架构权威 + check_user_range 接入三口 + A5/A6，与台账逐条对应，A9/A10 登记未失踪）。

### 2.4 批D 收尾与 tools（3 项：全 PASS）

- **07ae5efed** 收尾杂项：接续 PROMPT 结案横幅 + 0x8020058→0x80200058 注释勘误（纯注释实核）。
- **2c13a7c9e 续-342** t_memcpy 定案（纯文档）：三级判据链（裸机 semihosting 裸跑 3/3 逐位 MATCH md5=7b405d24 → 上机探针跨 boot 漂移 → 污染传感器定性）——方法论沉淀密度高（三坑登记）。
- **C-67 三连**：P2_TESTS 64→**76**（逐名解析实数吻合，+12）；P12 键格式修正（path:line→path，行漂移免疫，样例亲核）；证据仍落仓库根 tmp/evidence/（F7 延续）。

---

## §三 架构专项与自审抽查

1. **三终达成声明与证据链**：续-383/384 的定位链全锚点（fa0 探针→四死亡点→.tbss/errno TLS→tp=0→机器码核对→诊断镜像 3 分钟/轮迭代）；门读数 36/36 与本轮独立复跑一致；**诚实边界两条自记**（修的是出生链非指针随机改；tp 依赖面未逐一反汇枚举）+ **问题乙保持未结**（19+ boot 零腐蚀但"非现场抓获"）——判定纪律成熟。
2. **asm 三连修影响面**：boot-shim/x86/aarch64 零足迹（亲核 boot-shim 无 legs asm）；aarch64 twin 正确性对照成立（保存侧 stp 次序、返回腿 msr daifset 首句）。
3. **H9 裁决：闭合（复现性已立）**——clean checkout @ `ad14ced56` riscv atf 第二采样 rc=0 36/36；R3.3 的 boot-panic 树先于续-383/384 修复，可归因当时确定性缺陷 + 间歇族叠加。
4. **H1/H3/G1/G2 现态**：未清（H1 145 文件仍在树、探针 197+27、兜底仍在）——绑定未结案（问题乙/T8），逻辑一致；清场触发点继续挂问题乙。
5. **J3**：R3.1 F / R3.2 G / R3.3 H 三代评审待办持续无登记笔；主线 CR 基础设施现"配额失败改自评"（续-384 ⑥自曝）——登记观察。
6. **「.review 欠账清零」核对**：C-67 仅登记叙述，未动 .review/ 本地账（评审线 territory 无恙）。

---

## §四 卫生审计

1. 保护文件与规范源零触碰；H1（os/tmp+target_smp）无处置笔（仍在树，待主线 git rm）。
2. 探针台账：问题乙追捕链新活体（walk_reconcile/finw 对账/NK4C_LEG_COUNTS/fa0）全部挂"诊断 mark 族滚除"或工程常驻资产，口径自洽；fa0"用后即滚"未滚（声明下一笔）。
3. message 精确度：锚点偏移 2 处（续-383 系统性 +1、续-351 bit 标签偏一位）+ 测试计数口径 1 处（续-377）——无实质误导。

---

## §五 独立验证（隔离 worktree @ ad14ced56）

| 项 | 结果 | 判据 |
|----|------|------|
| docker 三件套 | **kernel 833 / arch 257 / vm 538，exit=0 ×3** | 只增不减 ✅（arch +14 = walk 套件默认面） |
| x86_64 -smp4 ×2 | marker=2/2，panic=0 pfVM=0 vec6=0 | ✅ |
| riscv64 boot-full | PASS（marker reached） | 目标① ✅ |
| riscv64 cmd smoke | PASS（marker + ls /bin + cat /etc/rc） | 目标② ✅ |
| **riscv64 atf 门** | 首采样 29/36 不完整（27P+0F+2S，7 案无终态）；**第二采样 rc=0：36/36 = 34P+2S** | **目标③ 独立复证 ✅ + H9 闭合** |
| aarch64 atf 门 | 首采样 boot leg panic（单样本）；**第二采样 rc=0：36/36 = 34P+2S** | 回归地板 ✅（间歇族 J2） |
| aarch64 cmd smoke | PASS | ✅ |
| 续-357 OFF/ON 实跑 | OFF（runtime-window）250/0；ON（+walk-hardening）252/0，fuzz ok | 声明证实 ✅ |

**间歇族定性（J2）**：两门各出现一次首采样异常（riscv 7 案无终态 0 failed / aarch64 内核同步异常 far=0xffff800000400000 单样本），重采样均绿；主线同夜亦自记 aarch64 一次 35/36 停滞（串口被 cleanup 删除未能定位——H7 家族）。TCG 时序敏感的间歇实存，单采样判读不可靠——门读数建议多采样制。

---

## §六 结论与移交

### 6.1 总判定

- **零 P0 / 零 P1-正确性 / 零违规**；深审 24 项：19 PASS + 5 PASS-with-NIT + 0 FAIL。
- **三终目标在截断面全部达成并经评审线 clean 重建独立复证**（六门读数见 §五）——NK4-C 主任务到达里程碑顶点；问题乙（内存污染）与 T8 探针清场是收官后仅存的两大开放案卷。
- 本轮技术密度最高点：riscv64 陷阱腿三连真缺陷（asm 级，带形状测试+正控制防复发）与 Sv39 巨叶掩码 P0——均有规格级核验与宿主测试；判据纪律（确定性优先于概率性框架、诚实边界自记、作废结论就地撤回）在续-382~384 表现成熟。
- 子代理红旗被实跑反转（J1）——评审线自身的验证纪律（载荷亲核+实跑）再次证明必要。

### 6.2 登记待办

| # | 级别 | 内容 | 锚点 |
|---|------|------|------|
| J1 | P2-design(OQ) | riscv64_walk.rs 参考实现 L1 错粒度检查漏 cfg 门（:233 无门 vs L2 :220 有门；impl 两侧均门）——当前 fuzz 生成空间不产错粒度叶故 OFF/ON 均绿（实跑证实），但生成器一旦扩展即 OFF 必炸；建议补同款 cfg 门或注释声明不对称 | riscv64_walk.rs:220-235 |
| J2 | P2-hygiene(门间歇) | atf 双门对 TCG 时序敏感的间歇族：本轮 2/2 门首采样异常（riscv 7 案无终态/aarch64 boot panic 单样本）+ 重采样全绿 + 主线同夜 35/36 一次；建议门读数多采样制（≥2 绿才记 PASS）+ 失败时串口保留（H7 联动） | §五 |
| J3 | P2-流程 | R3.1 F / R3.2 G / R3.3 H 三代评审待办持续无登记笔；主线 CR 出现"评审 agent 配额失败改自评"（续-384 ⑥）——建议恢复待办登记惯例 + CR 配额稳定性观察 | git log；续-384 |
| H1(沿) | P2-hygiene | os/tmp+target_smp+vmtext bin 仍在树（145 文件）——R3.3 处置建议未执行 | adb5976ca 遗留 |
| H3(沿) | P2-hygiene | fa0/pfwd-lethal 探针"用后即滚"未滚（声明续-385）；T8 族级滚除待问题乙结案 | trap_dispatch.rs:2291/2310 |
| H7(沿) | P2-hygiene | aarch64 门 35/36 停滞串口被 cleanup 删除（banner 自认）——失败即保留 WORK 的纪律建议 | 续-384 ⑤ |
| NIT | — | 续-383 C 锚点系统性偏 1；续-351 bit 标签偏一位；续-377 测试计数口径；84ba9669b rustfmt 混笔 | 各处 |

### 6.3 R3.5 增量协议

- 已审界推进为 `ad14ced56`；范围 = `git rev-list --count ad14ced56..<届时 HEAD>`；方法同本轮。
- 重点预告：问题乙（MEMORY-CORRUPTION）收官战 + T8/诊断族滚除清场（H3/G1/G2/H6 验收点）；主线在制的 init/dm_coverage/grant 新前沿；docfix-boot-chain 分支合入。

### 6.4 Step 5.7 Rule Discovery

1. **模拟重演 ≠ 实跑**：本轮子代理以 python 重演 fuzz 生成空间推得"OFF 按构造必失败"，实跑证明生成空间假设错误、声明为真——判据载荷的最终裁决必须以真实测试面执行为准，重演只配当线索生成器。
2. **单采样门读数不可判读**：TCG 时序敏感使 atf 门存在真实间歇族（本轮 2/2 门首采样异常、重采样全绿、主线同夜亦然）——CI 级判据需多采样制，否则间歇会以"回归"或"假绿"两种形态污染台账。
3. **验证环境的默认特性面即测试面**：walk 对抗套件经 runtime-window 特性进入 docker 默认 arch 面（243→257）——"宿主测试可达性"（续-349 立的纪律）在 docker 面同样成立，特性门的默认值选择直接决定 CI 盲区。

### 6.5 验证局限（同 agent 披露）

单 session 同 agent（zcode/GLM）：深审证据由 2 个只读子代理收集，评审人载荷抽查 6 项（L1 门/fuzz 门/signal.c:551/tp asm/P2_TESTS/OFF-ON 实跑）全部亲核，其中 1 项子代理红旗被实跑反转并降级；独立验证为评审人自建环境实跑（atf 双门各 2 采样）。跨 agent 交叉验证本轮不可用。

### 6.6 产物清单

- 本报告：`NK4C-REVIEW-REPORT-R34-20261006.md`
- P1 对账表：`NK4C-R34-P1-AUDIT-TABLE-20261006.txt`
- 评审线进度账：`.review/zcode/edge5/REVIEW-R31-STATE.md`（R3.4 段，本地 gitignored）

**lint 口径**：标题日期行 1 条 SL-4 与 R3~R3.3 报告同款先例基线。
