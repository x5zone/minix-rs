# NK4-C 增量评审报告 R3.5（2026-10-08）

- **范围**：`6ae34356c..e24963e55` = **101 笔**（截断面冻结于评审启动时刻；此后主线在制改动归 R3.6）
- **⚠ 基础事实变更**：仓库经**历史重构 + 笔记树迁移**——旧 hash（ad14ced56 等）全部不存在；评审文档/WORKLOG 迁至 `rewrite-notes/coordination/`；本报告的边界在新历史重锚（`6ae34356c` = 旧 ad14ced56 对位）。规范源亦已更新：**跨架构强制判定栏（§16.7）** 首次进入本轮射程。
- **段构成**：评审线 SELF 4 笔（0021ac290 R3.4 报告 + C-68 三连）；主线 97 笔，其中 **52 笔触 os/**（深审候选）
- **方法**：R3.4 协议延续 + §16.7 判定栏；P1 机械对账（python 权威扫描）+ 三子代理收证 + 评审人载荷亲核（4 项）
- **状态**：**定稿**（P0-P5 全完成）
- **增量协议**：R3.6 = 自 `e24963e55` 起；主线在制（工作树 8+ 脏文件 + `claim/C-69-zcode-glm` worktree）出界
- **产物**：本报告 + `NK4C-R35-P1-AUDIT-TABLE-20261008.txt`

---

## 结论先行

1. **无虚构取证、无违规**：101 笔保护路径与规范源零触碰、docs 声明零矛盾；**H1 验收通过**（`os/tmp`+`os/target_smp` 在截断面 tracked=0，145 件退跟踪完成）。
2. **深审 44 项判定 = 32 PASS + 11 PASS-with-NIT/flag + 1 FAIL**（89a85a515 "全 hart 进 payload" 定谳被在树 OpenSBI 源码否证——**判断级 FAIL**，功能面无残损）；零 P0 / 零 P1-正确性。
3. **T8 探针清场验收**：具名探针族整删成立（净删 2926 行、六门绿），但树级显式计数为 **nk4a: 1 处（刻意留档）+ nk4c: 14 处（6 文件：产品 OOM 面 + 后续 SMP WIP 诊断）**——"全清"口径过宽，宜三分类（具名族清零/产品诊断/SMP WIP）（K5）。
4. **独立验证（clean checkout @ e24963e55）**：docker **kernel 833 / arch 族 298 / vm 539 全绿**（arch +41 = 新审计测试面）；x86 marker=2/1 三零；riscv boot-full PASS；**riscv ATF -smp 2 rc=0 36/36（34P+2S）+ cmd-smoke -smp 2 PASS**；**SMP 双门 PASS**（riscv 门自打印 "BSP chain incomplete" 仍绿——K2 的现场实证）；aarch64 门见 §五（35/36 停滞间歇第 2 次复现，签名与主线续-384 首跑逐字同）。
5. **跨架构强制判定栏（§16.7）首次适用**：本轮 SMP 战役跨架构接线**零 `[ARCH:]` 标注**、两 arch 孪生实现 + hart 身份/交付模型各存静态——按 §16.7 应登记架构裁决项（K4）。
6. **评审线自披露（K8）**：我的 P1 探针正则 `probe_re` 不匹配 nk4a:/nk4c: 命名族，本轮"残渣 0"为**过早误报**，由子代理纠正——计数法须与探针命名族同步（R3.2 Rule Discovery #2 的第一次再犯，已修正工作流）。

---

## §一 P1 全量机械对账（101 笔）

| 判据 | 结果 |
|------|------|
| 保护文件触碰 | 零 |
| 规范源触碰 | 零（注意：prompt/ 与 .claude/ 有工作流线在制改动——出界未评） |
| docs 声明 vs diff | 零真实矛盾（1 项 e6b4883fd CLAIM? 归批C 定性=门判据弱背书，K2） |
| 体量异常 | 2 笔 BIG（d29b0f191 +7901 迁移 Phase 0 清单/8c7844a3d +14277 四方审计逐条回应——均迁移文档，裁决 ok） |
| merge | 1 笔（f84145323 doc-boot-chain 分支合入，纯文档 15 路径，ok） |
| 树级探针残渣 | **口径修正**：机器探针正则漏 nk4a:/nk4c: 族（K8）；显式计数 = nk4a: 1 / nk4c: 14（K5 三分类） |

---

## §二 深审判定（44 项）

### 2.1 批A T8 清场+三架构齐平（5 项）

| 项 | 判定 | 要点 |
|----|------|------|
| 1745925ee 续-385 T8 滚除 | **PASS-with-NIT** | 具名族整删成立（六判别探针/NK4C_LEG_COUNTS/CLI 魔术串/flags_mark 全零，bootmark→vmdm_bridge 改名保生产机制）；**NIT=残留未登记**：trap_dispatch.rs 6 处匿名热路径计数器（941-953/971-973 等，空体/近似空体 fetch_add 仍在执行）+ pdmv_before 死读 + minix-rt OOM-RT 注释与已删内核臂不同步（K5）；"nk4a/nk4c/vr 全清"过宽 |
| be8abac2a 续-387 A64-01 | **PASS-with-NIT** | aarch64 中间页表回收（level>=3 收口）机制与 riscv 同族结构对位；NIT=无宿主测试（架构门，自陈）+ 含当日门间歇上下文 |
| a7e50fe96 续-386 齐平开工 | **PASS** | run_all.sh 构建失败真报红（P-ALL-09）+ arm64 两形状测试文件（11 测试）；PADCONF 属 TI ARM32 板级（C padconf.h:6-11 实证）——"无对应物"注释成立 |
| f4df846d5 续-388 P-ALL-05 | **PASS-with-NIT** | 未知 VMCTL→EINVAL（C arch_do_vmctl.c 收尾实证）；errno 负号契约全文件对齐；NIT=锚点漂移（引 :56 实 :66）+ 计数口径（正文"41+8"实 43+10） |
| c98f7ebeb 续-390 死契约面移除 | **PASS-with-NIT** | 移除正确（死面零调用方实证）；NIT=**21 笔 servers/vm cfg(test) E0407 回归窗口**（至 b377263f8 修复，K6）+ 三 paging.rs 残留空格行（未 fmt）+ 悬空注释 |

### 2.2 批B 审计迁移+防御+清理（7 项）

5bfac71f4（清单冻结审计：86 门/20 文件独立复算零出入 + paging_encoding_pins 18 条字面全实测）**PASS**；400a4d5ee（SI 常量收敛 C sysinfo.h 逐值）**PASS**；a0b0d5741+3c123fd82+edb6c3308（三批宿主审计 28 测试逐字面实测 + C 锚 SC_MAGIC/MF_CONTEXT_SET/INIT_TASK_PSR 全命中；NIT=riscv64 handler 契约钉弱于注释宣称）**PASS-with-NIT**；4add4053d（emit_byte 0x3f9→0x3fd IER/LSR 勘误实证 + tx_wait 有界化）**PASS**；d34d2b2b5（x86/aarch64 入口 .bss 清零，清零先于立栈次序正确）**PASS**；b377263f8（断链修复；NIT=hash 引用 55aa0eda8 不存在，应 c98f7ebeb）**PASS-with-NIT**；79e896715 补记 **PASS**。

### 2.3 批C SMP 战役（12 项）——本轮最高技术风险

| 项 | 判定 | 要点 |
|----|------|------|
| 27a6c8757 续-398 AP 桩 | **PASS** | SBI HSM 入口态形状对位；no_mangle 迟一笔（P2 记录） |
| 3ede7ff37 续-399 负结果 | **PASS** | 负结果如实；dumpdtb 必须同 -smp 的拓扑真值教训有价值 |
| **89a85a515 续-400 "全 hart 进 payload" 定谳** | **FAIL（判断级）** | **与在树 OpenSBI 源码相反**（fw_base.S:429-500 非 boot hart 停 M-mode HSM wait；-6 实为自打）；该错误模型未随后续纠正而撤回，伪前提入库顺延（b919afc03 的 park 路永不可达 + AP_GO 全仓零生产者） |
| 04edd0534+ecc48685c 续-401 | **PASS-with-flag** | 证据链真；AP_ARRIVED 判据语义错（`>logical`/`!=0` 对 ≥3 hart 拿陈旧 1 冒名，latent，K7） |
| b919afc03 续-402 | **PASS-with-flag** | WIP 如实；park 邮箱零生产者死代码 + message 叙述与事实冲突 |
| 5b6707633 续-403 破案 | **PASS** | A3 写 KDM 窗 VA 未映射 stval 定谳与 OpenSBI switch_mode 双证（sbi_hart.c:767）；但"全 hart"模型未宣布撤回 |
| b099e2567 续-404 选举收口 | **PASS-with-flag** | 功能正确（选举格读 a0 真值）；机理叙述错（STOPPED 直投被误读为 park 拽出） |
| c9ba97d22 续-405 aarch64 半 SMP | **PASS-with-flag** | PSCI conduit 管线为正解；UART 标记承诺与终码冲突（头注 vs :148-154） |
| 6d944912c 续-406 aarch64 门翻绿 | **PASS-with-flag** | 门真绿 + 常量修复真（0x40200000）；**同 commit 两套互斥引导模型**（QEMU 源码判 kernel 侧对）；AP_GO 在 aarch64 两链皆死代码 |
| e6b4883fd 续-407 P-RV-01 达成 | **PASS-with-flag（CLAIM? 半实）** | 门脚本改动实为 -smp 1→2 两处 + dumpdtb 配对（关键）；**ATF/cmd-smoke 门判据不查 ap-arrived/sbi-hs/ret——与被断言事项不相交**（亲核脚本确认），SMP 证据只在串口（K2） |
| e835f3d3a/e7a5c441b | **PASS** | P-ALL-08 边界已声明；盘点计数为时点值（截断面 11 crate/16 站点） |
| 三门（smp-aps×2/smp-ipi） | **PASS-with-flag** | riscv aps 门"到达即绿"（BSP chain incomplete 亦 PASS，本轮实测打印现场）；两 aps 门未入 run_all/CI；判据对 timeout/panic/ret 无负向断言（K2） |

### 2.4 批D P-ALL-08 SEF 信号链（13 项）

全 PASS。要点：ba81df360 T1（SigSetBits 16B 与 C ipc.h:1714-1719 逐字节对位 + __sigword/__sigmask 位基 signif-1 实证）；e7a5c441b T1（升序逐位 + 管理器臂，C sef_signal.c:96-133 对位）；e835f3d3a（永久 `match{_=>0}` 假断言换实断言）；7a64c85fc IS 站（C is/main.c:85,107-116 对位）；c3f95db05（恒真断言修真 + 死锁雷登记两处落地）;ee950cd3e VM 站（SIGKMEM=71 单一权威上收）；c63c4f4f5（自我否证：撤 debug_assert，前提在 status 字——判断正确）；**41cf76921 fs-rt 站（"C 里没有 FS driver 注册信号 handler"假陈述纠正——mfs/pfs/ptyfs/vtreefs 四处注册逐行实证为真纠正）**；b84d9e7ff（五站空闭包明证 + rs/ipc 两真站自我纠错）；93a77d554 ipc-server（三臂与 C main.c:101-118 对位；NIT=锚点 off-by-one :113→:114）；aa4ef3f68 RS 站；2fe6243b2/1bdc807f4 T7/T6；b49b67319（CLAIM? 澄清：**"零生产语义"为真**，全落 cfg(test) 与文档）。

### 2.5 批E 冻结决议 PD 系列+八站出生臂（8 项）

全 PASS。53da928d9 PD-27（SEF 逃生门：SefCancel 去全局化 + sef.c:155-162/289-301 对位 + 15 调用点编译期强制）；f8afd07bb PD-26（**前提"devman 未注册 handler"被真源证伪为真**（vtreefs.c:57 实证）+ 改行忠实；NIT=锚点三处偏 1-2）；d77fe3040 PD-25（SignalAction 上收 + 同构表）；96293d84f PD-20（**109 成员严格 ==56 断言 + 发现并修复 DumpCore 真溢出**：旧写 [_padding[4..20]]=绝对 44..60 越 56 边界 4B——真 bug 修复）；993873745 PD-34（sys_statectl 线形包装 C libsys 逐件对位 + SYS_STATECTL=0x637 实证）；adcd36b6e+3a357281a+46763abbd+f916965dc（**八站九臂实测 9 调用点/8 站**；vm 异步特例 documented deviation 完整登记；IS 借用冲突教学笔）；f84145323 merge（纯文档 15 路径）。

---

## §三 架构专项与自审抽查

1. **跨架构强制判定栏（§16.7）首次适用（K4）**：SMP 战役 11 笔 diff 零 `[ARCH:]` 标注；`ApBootstrap`（40B repr(C)）是唯一上收的跨 asm ABI 类型，而 **HartId/选举/停车邮箱/到达标记全走每 arch 静态 + #[cfg] 平铺**（kernel smp.rs 两个近乎孪生 60 行块、bootface 485 行镜像）。按 §16.7：跨架构接口判定（hart 身份与交付模型放哪层）未做——建议登记架构裁决项（Linux riscv 直接从 a0 取 boot hart 无选举可作对照）。
2. **T8 清场三分类（K5）**：具名族清零 ✓ / 产品 OOM 诊断（OOM-RT/OOM-KERN 存活） / SMP WIP 诊断（ap-arrived/psci/sbi-hs ret 等后轮新增）——文件注释与 WORKLOG 顶部"18+ boot 零腐蚀"叙事按新模型核对无冲突，但"全清"措辞需修。
3. **冻结决议线的流程事件（PD-26）**：前提被真源证伪 → 按用户口径改行 → 勤实登记——**诚实性样本级**；同族另有 41cf76921（假陈述纠正）与 c63c4f4f5（自我否证）——本轮"先信后证伪"纠偏文化密度高。
4. **前轮待办消费**：H1 ✓（bb8a90e05）；T8 ✓（续-385，验收见 §二.1）；J1/J2/J3 无正式登记笔（沿 J3 观察）；R3.4 的 H9 复现性在 R3.5 环境维持（riscv ATF 本采样直接绿）。
5. **主线自审抽查**：续-415 盘点时点值引用注意（11 crate/16 站点为截断面实态）；"评审 agent 配额改自评"线本轮续见评审回执轮（续-416/418/421/422/425 五轮回执，零 P0/P1——回执文化反而变密）。

---

## §四 卫生审计

1. 保护路径/规范源零触碰；H1 tracked=0 验收通过。
2. 探针残渣：具名族清零 + 分类留存（K5）；显式计数口径修正（K8 自披露）。
3. message 精确度：SMP 链叙述错误群（K3，含一处判断级 FAIL）+ 锚点漂移 5+ 处 + 计数口径 2 处——叙述层是本轮主要薄弱面（功能层无损）。
4. 迁移笔（1102 文件纯移动 0 增删）+ 引用重写（138 处）+ 工作流重构（review-scan 收编，纯规则面出界）如实。

---

## §五 独立验证（隔离 worktree @ e24963e55）

| 项 | 结果 | 判据 |
|----|------|------|
| docker 三件套 | **kernel 833 / arch 族 298（249 lib + 49 审计新面）/ vm 539，exit=0 ×3** | 只增不减 ✅ |
| x86_64 -smp4 ×2 | marker=2/1，panic=0 pfVM=0 vec6=0 | ✅ |
| riscv64 boot-full | PASS（marker reached） | ✅ |
| **riscv64 ATF -smp 2** | **rc=0：36/36（34P+2S，0F/B）** | 多核门复证 ✅ |
| riscv64 cmd-smoke -smp 2 | PASS（18-stage） | ✅ |
| **SMP 双门** | riscv `ap-arrived` PASS（**打印 "BSP chain incomplete" 仍绿**——K2 实证）；aarch64 "secondary cpu reached convergence" PASS | ✅（附 K2） |
| aarch64 ATF | **本轮两采样均为 35/36 停滞（33P+2S，签名与主线续-384 首跑逐字同）——间歇族第 2/3 次复现且本环境复现率 2/2** | 间歇（升级观察项，见下注） |
| H1 验收 | `os/tmp`+`os/target_smp` tracked = **0** | ✅ |

**说明（H7 家族第 2/3/4 次）**：aarch64 门硬编码 `mktemp` WORK 目录（无环境覆盖），本轮两采样串口均被 cleanup 删除；第三采样外挂串口镜像监视器仍被竞态（门退出后监视器取到旧目录串口）覆写——同一间歇的证据第四次流失。**判定升级**：①"aarch64 ATF 门稳定绿"建议从回归地板降级为**观察项**（本环境 2/2 停滞；主线环境亦曾首跑停滞）；②门脚本两条必修——`WORK` 环境覆盖（照 RS_ATF_WORK 先例）+ 停滞判据与末案完成的竞速修（tail 显示末两案均 passed 而计数 33+2=35，疑末案终态行在判定后到达）；③证据保留纪律（失败即保留 WORK）连续四次断链，建议列入下一轮验收。

---

## §六 结论与移交

### 6.1 总判定

- **零 P0 / 零 P1-正确性 / 零违规**；深审 44 项：32 PASS + 11 PASS-with-NIT/flag + 1 FAIL（89a85a515 判断级，伪前提未撤回）。
- 本区间 = **T8 清场 → 三架构齐平 → SEF 信号链八站接通 → 冻结决议落地 → 八站出生臂编排面统一 → SMP 战役第一波（拓扑可见 + AP 到达）**；riscv/aarch64 多核门诚实标注"到达≠并行调度"。
- 技术亮点：SEF 链全部带 C 逐行锚（两处"假陈述纠正"经核为真）；PD-20 顺手修出 DumpCore 4B 溢出真 bug；PD-26 的"前提证伪改行"是流程样本。
- 薄弱面集中在 **SMP 链的叙述与门判据**（K1-K4）：功能真实、文档/判据未跟上——建议下轮随 SMP 第二波一并收敛。

### 6.2 登记待办

| # | 级别 | 内容 | 锚点 |
|---|------|------|------|
| K1 | **FAIL→待撤** | 89a85a515 "全 hart 进 payload" 模型被在树源码否证，从未正式撤回——建议在 WORKLOG 与 kernel-image/smp 注释处显式撤回，并清点其衍生死前提（b919afc03 park 路不可达、AP_GO 零生产者） | fw_base.S:429-500；sbi_hsm.c:339-352；main.rs:143-260 |
| K2 | P2-test | 续-407 "达成"的门判据与被断言事项不相交：ATF/cmd-smoke 不查 ap-arrived/sbi-hs ret/负向；aps 门"到达即绿"且未入 run_all/CI——建议生产门加 SMP 双断言（ap-arrived + ap-timeout=0）并挂 CI | test-atf-riscv64.sh:226-268 |
| K3 | P2-doc | SMP 机理叙述错误群（park 拽出/两套互斥模型/AP_GO 叙事）——按真模型改写 | b099e2567、6d944912c、b919afc03 |
| K4 | P2-design(模式85) | SMP 跨架构接线零 [ARCH:] 标注 + 孪生实现——登记架构裁决项（hart 身份/交付模型放哪层） | os/kernel/src/smp.rs；os/arch/*/ap_early_entry.rs |
| K5 | P2-hygiene | T8 "全清"口径过宽：残留 6 处匿名热路径计数器 + pdmv 死读 + OOM-RT 注释腐化；建议三分类记账并清残留 | trap_dispatch.rs:941-974/1677/1688/2102；minix-rt/lib.rs:151-154 |
| K6 | P2-流程 | c98f7ebeb→b377263f8 的 21 笔 servers/vm E0407 回归窗口（教训已入库） | pagetable/sim.rs |
| K7 | P2-design | AP_ARRIVED 判据语义错（≥3 hart latent） | os/kernel/src/smp.rs:1119-1126 |
| K8 | 评审线自披露 | P1 探针正则家族缺口（nk4a:/nk4c: 漏匹配）——已修正工作流；建议 P1 脚本钩子化时把探针族清单做成显式常量 | /tmp/r3*_p1_verify.py |
| H1(已闭) | — | bb8a90e05 退跟踪 145 件，验收 tracked=0 | ✅ |
| H7(升级·四度断链) | **P2-hygiene（本轮必修建议）** | a64 门串口被 cleanup 删除累计 4 次（R3.4 panic、本轮两次停滞、监视器竞态）——门加 `WORK` 环境覆盖 + 停滞判据竞速修 + 失败即保留；"aarch64 门稳定绿"降级观察项 | test-atf-aarch64.sh:38 |
| NIT | — | 锚点漂移 5+ 处（vmctl.c:56→66 / ipc main.c:113→114 / vtreefs.c:59→57 等）；续-415 时点值；b49b67319 口径；c98f7ebeb 空格行；hash 引用 55aa0eda8→c98f7ebeb | 各笔 |

### 6.3 R3.6 增量协议

- 已审界推进为 `e24963e55`；范围 = `git rev-list --count e24963e55..<届时 HEAD>`；方法同本轮（含§16.7 判定栏）。
- 重点预告：SMP 第二波（若启动——K1-K4 收敛点）；八站出生臂后的 "SEF 收信取消通道" 真机激活验证；C-69 线（claim worktree）；PD 系列续笔。

### 6.4 Step 5.7 Rule Discovery

1. **验证工具与命名族同步**：P1 探针正则漏 nk4a:/nk4c: 家族导致"残渣 0"误报（K8）——工具判据的匹配模式必须与源码命名族演进同步（R3.2 Rule Discovery #2 的首次再犯；修正=族清单显式化）。
2. **"达成"类 commit 的门判据独立性**：P-RV-01"双 PASS"的门判据与被断言性质不相交（K2）——建议评审引入"判据交集检查"：commit 若声称性质 P，其门必须至少有一条断言直接测 P，否则"达成"降级为"弱背书"。
3. **间歇族的证据保留纪律**：同一 35/36 停滞三度出现（主线 1 次、评审线 2 次），每次串口都被 cleanup 删除（H7）——失败即保留 WORK 的纪律缺一行环境覆盖就断一次证据链。

### 6.5 验证局限（同 agent 披露）

单 session 同 agent（zcode/GLM）：深审证据由 3 个只读子代理收集（含 OpenSBI 在树源码规格核验），评审人载荷抽查 4 项（残渣实数/AP_GO 生产者/门判据/计数器）全部亲核，其中 1 项纠正了评审人此前的工具误报（K8）；独立验证为评审人自建环境实跑（含 SMP 双门与串口保留采样）。跨 agent 交叉验证本轮不可用。

### 6.6 产物清单

- 本报告：`rewrite-notes/coordination/NK4C-REVIEW-REPORT-R35-20261008.md`
- P1 对账表：`rewrite-notes/coordination/NK4C-R35-P1-AUDIT-TABLE-20261008.txt`
- 评审线进度账：`.review/zcode/edge5/REVIEW-R31-STATE.md`（R3.5 段，本地 gitignored）

**lint 口径**：标题日期行 1 条 SL-4 与 R3~R3.4 报告同款先例基线。
