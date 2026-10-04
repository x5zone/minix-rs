# NK4-C 增量评审报告 R3.3（2026-10-05）

- **范围**：`92fa446c3..5cf88a552` = **78 笔**（截断面冻结于评审启动时刻；此后主线在制改动归 R3.4）
- **段构成**：评审线 SELF 4 笔（8f8d6e027 R3.2 报告 + C-66 三连 c5bee908d/ecd1778bf/5cf88a552 pattern-gate 补扫+增量合一轮）；主线 AGT 74 笔，其中 **26 笔触 os/ 生产码**（深审候选）
- **方法**：R3.2 协议延续 + code-excellence 四件套；P1 机械对账（python 权威扫描）+ 分批深审（2 个只读子代理收证 + 评审人裁决与 4 项载荷抽查）
- **状态**：**定稿**（P0-P5 全完成）
- **增量协议**：R3.4 = 自 `5cf88a552` 起；主线在制（WORKLOG 1 脏文件 + `.wt/docfix-boot-chain` 分支未合）出界
- **产物**：本报告 + `NK4C-R33-P1-AUDIT-TABLE-20261005.txt`（78 笔机器初筛表）

---

## 结论先行

1. **无虚构取证、无验证虚报**：78 笔 message 声明与 diff 对账零真实矛盾；保护路径（minix3/、AI-chats/、os/target/、os/.dockercargo）与规范源零触碰；TEMP 探针回滚干净（截断面树 nk4a: 残渣 0；a2d-leaf 落地→回滚闭环）。
2. **深审 29 项判定 = 23 PASS + 5 PASS-with-NIT + 1 FAIL（判读链级·批内已自纠，链终态无残留缺陷）**；零 P0 / 零 P1-正确性。
3. **里程碑独立复证——三目标①② 在截断面三架构全数成立**：
   - docker 三件套 **kernel 833 / arch 243 / vm 535，0 failed**（基线只增不减 ✓）
   - x86_64 `-smp4` marker=2/2、panic=0/pfVM=0/vec6=0
   - **riscv64 boot-full：marker reached（exit=0 PASS）——目标① riscv 独立复证 ✅**
   - **test-cmd-smoke-riscv64.sh：PASS——目标② riscv 独立复证 ✅**
   - test-atf-aarch64.sh rc=0（36/36=34P+2S）+ aarch64 cmd smoke PASS（回归地板 ✅）
   - test-atf-riscv64.sh：clean 重建重跑 **boot leg panic**（非法指令 @ 用户栈区 PC），与主线在制树时期的 run1/run2 读数不一致——不推翻「套件跑起来」声明，但截断面 clean 可复现性未成立（H9 证据来源缺口）
4. **Sv39 定案支柱独立核实**（续-338 的 ground truth）：Sv39 VA=39 位、规范要求 va[63:39] 全等于 va[38]；旧栈顶 `0x7fff_ffff_f000`=2^47−0x1000 不满足 → CPU 走表未始即 fault（QEMU `masked_msbs=0x1FF` → TRANSLATE_FAIL）；新栈顶 `0x3f_ffff_f000`=2^38−0x1000 合规，`USER_STACK_TOP` 收敛为 minix-types 单源 `cfg!` 常量。
5. **发现全 P2 级（H1-H9 + NIT）**，其中 H1（批量误入库）为本轮唯一流程纪律事件——且 tools 线 C-66 已同步建立 P17 防线对冲复发；H9（atf-riscv founding 证据未钉构建来源 + clean 重建 boot leg panic）是目标③ riscv 冲刺前的最新前沿——建议主线 HEAD clean 重跑钉 md5。

---

## §一 P1 全量机械对账（78 笔）

### 1.1 方法

R3.2 v2 方法复用（`git show --numstat` 四判据 + merge fallback + python 权威复核 + 截断面树级探针残渣扫描）。本轮新增特判：**os/tmp + os/target_smp 批量入库事件定性**（见 H1）。

### 1.2 结果（python 权威扫描）

| 判据 | 结果 |
|------|------|
| 保护文件触碰 | 零（os/target/ 正主未动；被入库的是 os/target_smp/ 旁名目录） |
| 规范源触碰 | 零 |
| TEMP 探针残渣（截断面树） | nk4a: 模式 **0 命中**；新 `nk4c:` 族 12 文件/24 处（见 H3） |
| docs 声明 vs diff | 零真实矛盾 |
| 体量异常 | **1 笔 BIG+475,559（adb5976ca，154 文件 b66）**= H1 事件本体 |
| merge 笔 | 0 |

### 1.3 H1 事件定性（本轮头号发现）

**adb5976ca（续-311 旁路终判笔）未声明批量误入库三处**：
1. `os/tmp/nk4a/` — 23 文件 / 29.3 MiB（nk69 时代取证串口与 .int 转储，含 205,625 行 nk69-s2b.int）；
2. `os/target_smp/` — 120+ 文件 cargo 构建产物目录（debug/、.rustc_info.json、CACHEDIR.TAG、66 个二进制）——构成 +475,559 行的主体；
3. `os/vmtext_a.bin` + `os/vmtext_b.bin` — os/ 根下 8KB 双二进制（agent 发现，message 未点名）。

三者均无 message 声明；性质与既有纪律「git add 用明确路径、禁 -A」及记忆判例「.int/.img/.fd 无忽略规则」完全同型（疑似宽路径 add 扫入；os/target_smp 在 R3.1 开局时已是 untracked 陈物）。**建议处置（交主线）**：`git rm -r --cached os/tmp os/target_smp os/vmtext_a.bin os/vmtext_b.bin` + .gitignore 补 `os/target_smp/`、`os/tmp/`、`/os/vmtext_*.bin`。**对冲事实**：tools 线 C-66 同日落地 P17「构建缓存/取证产物跟踪防线」（report 式，实测 145 文件）——同类复发已被门拦截，收编 P12 基线 +3（lib.rs:3842 sfence 调用图门）。P2_TESTS 稳定 64。

### 1.4 旗标行裁决（29 行）

OS-CODE ×26 = 深审候选集；BIG ×1 = H1；PROBE? ×2 = 续-311 旁路 PteWrite/PteZero（声明仪表，ok）；SELF ×4 = notes/tools-only，ok。

---

## §二 深审判定（29 项）

> 行号锚点基准 = 截断面 `5cf88a552`（仓外隔离树 + `git show` 双通道）。载荷抽查（评审人亲核）：b37a76daf 遮蔽缺陷与 635398d55 自纠 diff、USER_STACK_TOP 单源、vmtext 双 bin 在位、nk4c: 计数——全部证实。

### 2.1 批A riscv (A) 取证+旁路轮（17 项：11 PASS + 5 PASS-with-NIT + 1 FAIL）

**深审 8 笔**：

| commit | 判定 | 要点 |
|--------|------|------|
| c114abf9c 续-311 旁路落地 [ARCH: riscv-vmddm] | **PASS-with-NIT** | wire/dispatch/arch 钩子/VM 注册四件套齐；NIT=内核 PteRead 臂无 pa RAM 界校验（H5）+ reply<0 静默映射 PTE=0 |
| adb5976ca 续-311 终判+回滚 | **PASS-with-NIT** | 旁路回滚逐文件互逆、bootmark 字节级复原；krewalk 保留如声明；NIT=第三处误带入 vmtext 双 bin（H1） |
| 5bbece7d2 续-313 三腿重落 | **PASS-with-NIT** | PteWrite/PteZero 与 KernelDm 门控形态对齐；NIT=**message 过认领 setaddr-root**（实为 722ed6c30 落地，引 hash 亦错，H4）+ 旁路重落后截断面 VmDm 读/写/零填全内核代执行（H3） |
| 1ac7250a0 续-335 findmut-flip+重试×8 | **PASS-with-NIT** | "成修部分有效"声明与 diff 一致（retry 块+打点）；NIT=**重试×8 是行为变更置探针族叙事下**（无退避/无单测，H6——(A) 清场单独裁决） |
| 5ff8556c5 续-298 krewalk v1 | **PASS-with-NIT** | KDM 三级重走机制合法；保留依据=族级台账 |
| b37a76daf 续-298-b krewalk v3 | **FAIL（判读链·批内自纠）** | `pa = ptroot` 被紧随的未删 `let mut pa = root` 重绑定遮蔽=根切换死代码，v3 实际仍走 VM 自根（同义反复）；message 据此宣告"帧生命周期 bug"定性。**同日 635398d55 删遮蔽行+注释自认"同义反复误读根因"**——评审人亲核两笔 diff 证实。现存码零残留；链终态 PASS。三轮评审首例单笔 FAIL，按 R3.2 Rule Discovery#2（止血-收窄链按终态判定）框架定级为判读链 FAIL、代码面无残损——H2 |
| 4457c3cd9 续-316 elfchk | **PASS** | 纯读探针；"pfs/mfs/init 非 ELF"自反点破 halt-dump 工具产物陈旧=工具伪影非内核事实——判读面拨正 |
| 3a009c508 续-334 大笔 | **PASS-with-NIT** | os/ 实际：finish_and_restore 补 riscv64 `sfence.vma zero,zero`（规格正确、过刷安全、已被 P12 基线收编钉住）+ 4 处诊断；docs ≈2825 行（BUG 文档第 10 章等）；NIT=SLOW_N"用后还原"未兑现（H3） |

**快速核 9 项全 PASS**：bdb4fe525（a2d-leaf 落地，批内被 e04e88632 滚净）/ e04e88632（-87 精确逆滚）/ 23dcddac6 / b2451b02a / 635398d55（自纠笔）/ 722ed6c30 / 513373d30+7881b1e88+3f56831ba / 1c7c226a5——纯探针/诊断，声明保留者均在 T8 族级台账。

**横向**：nk4a: 33 文件 193→196（+3：setaddr-root/no-region-pf/fill-root 第二位点）；**新 `nk4c:` 族 12 文件/24 处逃逸 nk4a:-only 计数法**（Phase E 清场必须扩匹配模式，否则漏滚约半数新探针——H3）；[ARCH: riscv-vmddm] 标注 8 处/5 文件且旁路本体在截断面是活体（H3）。

### 2.2 批B Sv39 定案修复（3 笔：全 PASS）

| commit | 判定 | 要点 |
|--------|------|------|
| 5e1a5fa5c 续-338 定案+修复 | **PASS** | 规格支柱独立复算成立（§结论先行.4）；USER_STACK_TOP 单源 `cfg!`（riscv=0x3f_ffff_f000 / 其余=0x7fff_ffff_f000）+ 三消费点全改引；残余字面量逐一核对全为 cfg(test)/惰性填充；p5 探针滞留（声明未滚，H3 族） |
| 452751df8 续-338b | **PASS** | 自曝 P0 真实（宿主 test-all 取非 riscv 值 ≥2^39 撞断言）；修法对称（接线断言 assert_eq + 责任移交编译期不变式）；P2×4 落地 |
| a5c615879 续-338c | **PASS** | <2^39→<2^38 收紧，非 riscv 分支不受影响 |

### 2.3 批C riscv 里程碑门（4 笔：全 PASS）

| commit | 判定 | 要点 |
|--------|------|------|
| ad34e94ec 续-339 cmd smoke 门 | **PASS** | 三判据（marker 全日志/rc shebang+命令行/容错按序 listing）；全日志判据相对 aarch64 门的有意偏离有成文理由（marker 首命中可落 cat 回显行内）；存证 serial 先于提交且经门自身判据复判三项全 True |
| 5e58a9caf 续-339b | **PASS** | P1 假绿防（rm -f）落点火前；「同族同修 ATF 门」兑现（340 出生即带）；NIT=SKIP 路径缺镜像新鲜度校验与 ATF 门不对称（H8） |
| 66058491f 续-340 atf 上机腿 | **PASS** | collect_atf_face 单源重构真成立（arch 参数化，aarch64/riscv 两路同函数）；riscv 出生腿「stub 早已备好」有 startup-minix.c:144 锚；门判据与 aarch64 逐条对映 |
| e34714af4 续-340b | **PASS** | P2×4（AtfFace 结构/stdout 契约/删不可达回退/SUITE_PROGS=18+SKIP mtime 链）全部落进现码 |

**里程碑声明审计**：续-339/340 的声明-证据链完整（串口存证早于提交、读数与门口径逐字吻合）；atf-riscv 门从未 PASS 而台账如实写「实跑中」——无过度声明。续-338 marker 里程碑由本轮独立复跑直接证实（§五）。**两项保留**：①（H7）原始串口证据全在 /tmp（atf_rs64_run1/2、rs64_smoke_run1、cmdface.serial），WORKLOG 只记读数不记路径——/tmp 清理即断链；②（H9）**atf-riscv founding 证据的构建来源未钉死**：run1/run2 串口取于 续-340 提交前 15-30 分钟的在制树（二进制未钉 md5/commit——正是主线自己在续-290 立的方法论）；评审线以 clean checkout @ `5cf88a552` 重建重跑单样本即 boot leg panic（非法指令 @ 用户栈区 sepc 0x3fffffedfa）。「套件跑起来」声明不被推翻（其证据真实），但**截断面 clean 重建的可复现性未成立**——建议主线 HEAD clean 重建重跑并钉 md5：复现 panic = 真回归需查（形状与未结案的 TRANSIENT-PTE 视图层族一致，可作其活体新证据）；通过 = 记一次彩票样本。

### 2.4 批D tools/杂项（5 项：全 PASS）

- **C-66 三连**：P17 新检查（构建缓存/取证产物跟踪防线——对 H1 的同日对冲）+ 报告 §11（历史缺口 8 处闭合）+ 基线收编（P12 lib.rs:3842 sfence 门 +3）；P2_TESTS 稳定 64（逐名解析复核）；证据仍在仓库根 evidence/（F7 位置偏离延续，附记 H8）。
- **1a3b965e7 续-341 T13 地址常量清扫**：纯审计零生产码（声明一致）；MMAP_TOP=2^41 等登记未修（声明一致）。
- **5013a8094 misc_concepts 增量五**：148→160 条，riscv 收官叙事入库。
- **续-290 勘误笔**：批次误标（z 批=探针二进制）诚实勘误 + 观测者效应定量（探针在场 0/19 vs 缺席 12/16≈75%）+ rust 可复现构建实证（同源 md5 一致）——诚实性样本级。

---

## §三 架构专项与自审抽查

1. **[ARCH: riscv-vmddm] 三处一致**：标注在代码（8 处/5 文件）+ WORKLOG/BUG 文档（结案裁决条款）一致；**旁路本体在截断面是活体**（c114abf9c 落地→adb5976ca 回滚→5bbece7d2 重落）——「结案时裁决去留」语义自洽，但活体每操作一次 kernel-call 的成本与 H5 的 pa 无界校验须随裁决一并处置。
2. **R3.2 G1-G10 消费核对**：无登记笔（79a1523ed 先例未沿用）——H8 登记；G1/G2（探针台账/RegionMap 兜底）未清场且 (A) 未结案，逻辑一致；本轮 G1 实质演进=nk4c: 族出现使清场匹配模式问题提前显形（H3）。
3. **主线自审抽查（防虚构）**：续-290 批次误标勘误（z 批=探针二进制错标"零扰动基线"）+「批次钉 md5 前提是同次构建」方法论入库——诚实性样本级；续-334 BUG 文档第 10 章大更新与 os/ hunks 分节可对账。
4. **三目标声明与证据链**：见 §2.3——①② 声明由本轮独立复跑直接证实；③ riscv 在途态声明无过度。

---

## §四 卫生审计

1. 保护文件与规范源零触碰；os/target/ 正主未动（被入库的是旁名 os/target_smp/）。
2. **H1 三处未声明误入库**（§1.3）——本轮唯一流程纪律事件；对冲=P17 防线同日落地。
3. 探针三类形态照旧可对账（TEMP 已滚/入库仪表/台账待滚），但 **nk4c: 前缀逃逸**使「台账待滚」的清场匹配面需要修订（H3）。
4. message 精确度：78 笔中 1 笔过认领（H4）+ 1 笔判读链宣告错误（H2，批内自纠）——判读类笔（探针读数→定性宣告）的 message 是本轮新的薄弱面。

---

## §五 独立验证（隔离 worktree @ 5cf88a552）

**方法**：`git worktree add --detach` 仓外隔离；拷主树 Cargo.lock + CARGO_NET_OFFLINE + docker 挂 host registry（R3.2 沉淀配方）；atf 门需先 `tools/build-libatf-c.sh <arch>` 预构建库（riscv64 腿首轮缺件失败，补构建后重跑）。网络已恢复（主线续-290 登记的同一阻塞解除）。

| 项 | 结果 | 判据 |
|----|------|------|
| docker 三件套 | **kernel 833 / arch 243 / vm 535，exit=0 ×3** | 只增不减（833+/243+/535+）✅ |
| x86_64 -smp4 ×2 | marker=2/2，panic=0 pfVM=0 vec6=0 | 回归地板 ✅ |
| **riscv64 boot-full** | **exit=0 PASS（marker reached）** | **目标① riscv 独立复证 ✅** |
| **riscv64 cmd smoke** | **exit=0 PASS（marker + ls /bin + cat /etc/rc，VFS IPC）** | **目标② riscv 独立复证 ✅** |
| test-atf-riscv64.sh | **重跑 FAIL——boot leg panic**：`riscv64 user-leg trap: scause 0x2 stval 0x8020 sepc 0x3fffffedfa`（非法指令、PC 落在用户栈区），未达用例阶段；与主线 run1/run2 读数不一致——见 H9 证据来源缺口 | 现态复现未成立（见 H9） |
| aarch64 双门 | cmd smoke PASS + **atf rc=0（36/36=34P+2S）** | 回归地板 ✅ |

---

## §六 结论与移交

### 6.1 总判定

- **零 P0 / 零 P1-正确性 / 零违规**；深审 29 项：23 PASS + 5 PASS-with-NIT + 1 FAIL（b37a76daf 判读链级，批内自纠，链终态无残留缺陷——三轮评审首例单笔 FAIL，按链终态框架不改变总体结论）。
- 本区间是**三目标的 riscv 收官段**：续-338 定案（Sv39 非规范栈顶——架构规范级 ground truth 独立复核无误）→ ①② 三架构全通（本轮独立复证）→ ③ riscv 在途（诚实申报）。(A) 取证链以旁路实验/指纹稳定性/观测者效应定量三线收窄，判读纪律在续-290/305 两度自我纠偏。
- 发现全 P2 级（H1-H8+NIT）；H1（误入库）与 H3（探针清场匹配面）是 (A) 结案清场的强制项；H2 是判读类 message 的新薄弱面信号。

### 6.2 登记待办

| # | 级别 | 内容 | 锚点 |
|---|------|------|------|
| H1 | P2-hygiene(流程) | adb5976ca 三处未声明误入库：os/tmp/nk4a（23 文件 29.3MiB）+ os/target_smp（120+ 文件构建产物，+475,559 行主体）+ os/vmtext_a/b.bin；建议 `git rm -r --cached` + .gitignore 三条；对冲=P17 防线已立 | adb5976ca；C-66 P17 |
| H2 | P2-doc(判读链) | b37a76daf v3 遮蔽死代码→"帧生命周期 bug"误宣告；635398d55 自纠+注释自认；现存码零残留——判读类笔的 message 定性需"判读所依赖的代码路径逐行可复核"纪律 | b37a76daf/635398d55；trap_dispatch.rs |
| H3 | P2-hygiene(探针) | nk4c: 族 12 文件/24 处逃逸 nk4a: 计数法——Phase E 清场匹配模式须扩为 nk4a:\|nk4c:\|krewalk\|LAST_FILL\|无前缀内核探针；SLOW_N"用后还原"未兑现；vmddm 旁路截断面活体（含 PteRead/Write/Zero 臂）随"结案裁决"一并清点 | syscall.rs:2585-2634 等 |
| H4 | P2-doc(message) | 5bbece7d2 过认领 setaddr-root（实为 722ed6c30 落地，引 hash 亦错） | 5bbece7d2 |
| H5 | P2-design | 内核 PteRead/Write/Zero 臂对 VM 传入 pa 无 RAM 界校验（KDM 任意物理读）+ KDM 常量多处本地复制（续-341 已登记同族） | syscall.rs Pte 三臂 |
| H6 | P2-arch | 1ac7250a0 重试×8=行为变更置于探针族叙事——(A) 清场时单独裁决去留 | region_map.rs:102-123 |
| H7 | P2-hygiene(证据) | 里程碑原始串口证据全在 /tmp（atf_rs64_run1/2、rs64_smoke_run1、cmdface.serial）——WORKLOG 只记读数；建议关键证据入库 tmp/nk4a/ 或 evidence/ | /tmp 路径清单 |
| H8 | P2-doc | R3.2 G1-G10 无登记笔（建议按 79a1523ed 先例补登记）；cmd-smoke SKIP 路径缺镜像新鲜度校验（与 ATF 门不对称）；evidence/ 根位置偏离延续 | git log；cmd-smoke 脚本 |
| H9 | P2-hygiene(证据来源) | atf-riscv founding 证据二进制来源未钉（run1/run2 取于续-340 提交前在制树，无 md5/commit 绑定）；clean checkout @ 5cf88a552 重建重跑 boot leg panic（scause 0x2 @ 栈区 PC）未能复现读数——违反主线自立的「批次钉 md5/构建 commit」方法论（续-290）；建议 HEAD clean 重跑钉 md5（panic=真回归且为 TRANSIENT-PTE 族活体新证据 / 通过=彩票样本入账） | /tmp/atf_rs64_run*/serial.log；§五 |
| NIT | — | ENOSYS 常量/字面量混用；bootmark CRLF 重写噪声；p5 等探针滞留（族级台账）；T13 登记项未修（声明一致）；riscv 构建面 warning 群（unexpected cfg mock / unsafe_op_in_unsafe_fn / E0133 mutable static，boot-shim+minix-rt，存量） | 各处 |

### 6.3 R3.4 增量协议

- 已审界推进为 `5cf88a552`；范围 = `git rev-list --count 5cf88a552..<届时 HEAD>`；方法同本轮。
- 重点预告：(A) 最终定谳与结案清场轮（H1/H3/H6 验收点 + G1/G2 R3.2 项）；riscv ATF 冲刺（H9 重跑钉 md5 → t_memcpy 攻坚 → 门全绿）；docfix-boot-chain 分支合入。

### 6.4 Step 5.7 Rule Discovery

1. **判读链可复核性**：探针读数→定性宣告的 message（续-298-b）可以建立在与作者意图不符的代码路径上（遮蔽死代码）且单笔内不可自见——判读类 message 的证据标准应升级为"判读所依赖的代码路径逐行引用"，批内自纠（续-305）证明了链式纠偏有效但不及时。
2. **探针计数法随探针语言演化失效**：nk4a:→nk4c: 前缀逃逸使 R3.2 建立的清场计数法失效——清场工具的匹配模式必须与探针命名族同步演进（P17 的"report 式"防线是正确方向）。
3. **验证环境的预构建依赖面**：atf 门依赖 build-libatf-c.sh 预构建 + vendored 工具链 + Cargo.lock——建议主线把「评审可复现包」（lock+vendor+libatf 预构建清单）文档化一页，否则每个新验证者都要重走 R3.2/R3.3 的环境事故路径。

### 6.5 验证局限（同 agent 披露）

单 session 同 agent（zcode/GLM）：P1 权威扫描 python 重跑；深审证据由 2 个只读子代理收集，评审人载荷抽查 4 项（b37a76daf 遮蔽/USER_STACK_TOP 单源/vmtext bin/nk4c: 计数）全部亲核证实；Sv39 规格支柱经子代理独立复算 + 评审人常量数学复核双重确认；独立验证（§五）为评审人自建环境实跑。跨 agent 交叉验证本轮不可用；§五 命令可重放（预构建依赖见 Rule Discovery #3）。

### 6.6 产物清单

- 本报告：`NK4C-REVIEW-REPORT-R33-20261005.md`
- P1 对账表：`NK4C-R33-P1-AUDIT-TABLE-20261005.txt`（78 笔机器初筛 + 旗标）
- 评审线进度账：`.review/zcode/edge5/REVIEW-R31-STATE.md`（R3.3 段，本地 gitignored）

**lint 口径**：本报告标题日期行 1 条 SL-4 与 R3/R3.1/R3.2 报告同款先例基线；P1 表 SL-4 全部来自 verbatim 引用的 commit message（保真优先）。
