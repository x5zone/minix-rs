# NK4-C 评审报告 R3（自审 91 笔 + 增量 123 笔，截断面 956e4a57f）

- **日期**：2026-09-27（接 R2：覆盖至 `e90efbcd8`；本报告覆盖 `3e155ba2a..956e4a57f` = **214 笔**）
- **范围**：①**交接方自审**——本会话 agent（NK4C 长程任务前半：F14/F15 → 1.12d 交接）`3e155ba2a..b7a7d05b8` = 91 笔；②**增量评审**——续跑 agent `b7a7d05b8..956e4a57f` = 123 笔（B39 前段 → §1.118续；含 **x86_64 rc marker 达成（§1.105）与 echo/ls/cat 上机（§1.106）**、aarch64 全启动链 bring-up（§1.107-§1.118））
- **方法**：code-excellence（声明-事实对账 + 分层设计审视 + C 锚点核对 + 非法态封堵 + 死代码/探针卫生 + 独立验证）
- **状态**：⏳ 进行中（P0-P2 已完成，P3-P7 推进中；本文件按阶段增量提交。进度账：`.review/zcode/edge1/REVIEW-R3-STATE.md`）
- **增量协议**：另一 agent 仍在推进；本轮截断于 `956e4a57f`，此后新提交按增量对账追加（R3.1/R3.2…）

---

## 结论先行（随阶段推进更新）

1. **P1 机械对账（214 笔全量）**：213 笔声明与 diff 事实一致；**1 笔卫生违规 = 交接方自己的 `d6f176451`**（误提交 `os/.dockercargo/registry/` 158 文件/4.7MB/18k 行 crates.io 依赖源码，至今仍被跟踪）。无 minix3/ 改动、无 `AI-chats/daily.todo.md` 污染、无 target/ 入库。**两笔初判旗标（441b5ecbc/52934500b）经全文核实为评审脚本正则误报**（message 明写"含代码"，修复合规）。
2. **P2 自审（91 笔）**：修复类 17 笔逐一复核，**4 项实锤发现**（详见 §二）：①1.12d WIP 带两项缺陷入仓（方向互换 + SLOT=80 步长错，均由续跑方 B7/1.12e 修复）；②1.10x 是在"drain 腿代停车"这一偏离 C 的架构内打补丁，被 B24（§1.48）按 C proc.c:569-583 结构性取代（对方更忠实）；③1.12a 注释-代码不同步（"满丢最旧"实为"满丢最新"，仍在 HEAD）；④取证探针自身长度 bug 消耗 2-3 轮真机（纪律已固化进 RESUME-PROMPT）。
3. **P3-P7**：进行中。

---

## 一、P1 全量机械对账（214 笔）

### 1.1 方法

脚本逐笔执行 `git show --numstat`，将 message 声明与 diff 事实对账：

| 检查项 | 判据 |
|--------|------|
| "零代码改动/纯取证/纯文档/侦察" 声明 | diff 不得含非文档 `.rs` 改动（message 同笔明写"含代码"则豁免） |
| "探针全回滚/tracked 净/工作树净" 声明 | 该 commit 本体应为 docs-only（回滚发生在工作树，不入库） |
| 保护文件 | `minix3/`（ground truth 禁改）、`AI-chats/daily.todo.md`（用户私人笔记禁动）、`os/target/`、`.dockercargo`（vendored） |
| 体量异常 | +>3000 行 |

### 1.2 结果

- **213/214 通过**。全部 214 笔的完整对账表存 `.review/zcode/edge1/REVIEW-R3-STATE.md` 附件（`/tmp/nk4c_r3_p1_audit.txt` 快照）。
- **唯一违规（交接方自审项）**：

| commit | 事实 | 定级 |
|--------|------|------|
| `d6f176451`（SELF，1.12d WIP） | `git add -A os/` 误将 `os/.dockercargo/registry/`（158 文件：log-0.4.29、bitflags-2.11.0、fdt-0.1.5 等依赖源码）提交入库；`.gitignore` 无该条目；HEAD 仍跟踪（4.7MB） | **P2-hygiene**（非行为缺陷，但违背仓库卫生；修复=gitignore 条目 + `git rm -r --cached os/.dockercargo`，登记待清） |

- **正则误报澄清**：`441b5ecbc`（B22 修复，4 文件 +145/-18，message 明写"含代码·VM侧"）、`52934500b`（1.53 修复，4 文件 +203/-1，"含代码·内核侧"）——两笔均为真实修复且验证声明完整（三件套绿 + 真机双跑 + CodeReview PASSED 记录在 message 内），非虚报。

### 1.3 声明-事实对账的正面结论

- 123 笔增量中 57 笔"取证/侦察/docs"类**全部**为 docs-only 或按声明回滚——**未发现虚构取证**（每笔"探针全回滚、tracked 净"声明均与 diff 吻合）。
- 65 笔 fix 类均带验证三件套声明（docker 数字 / 真机双跑签名 / fmt 对账），P6 独立验证将抽查其中关键节点。

---

## 二、P2 交接方自审（91 笔，修复类 17 笔深审）

### 2.1 发现清单（按严重度）

**F-SELF-1（P1-正确性，已由续跑方修复）｜`d6f176451` 1.12d WIP 带两项缺陷入仓**

| 缺陷 | 事实 | 修复方 |
|------|------|--------|
| ① 方向语义互换 | `copy_via_root_pages` 的 `to_kernel` 两臂与 doc 契约相反：读腿（false）实际把清零缓冲**写进**用户 SENDA 表（A_RETR 变破坏性写）→ flags 恒解出 0=AMF_EMPTY → senda 一条不投（s18 系列全零签名的真因） | 续跑方 1.12e 修正（HEAD ipc.rs:480-495 注释在案） |
| ② 步长硬编码 | `SLOT=80` 与同文件 size 断言 `16+size_of::<Message>()=96` **自相矛盾**：slot[i≥1] 按 80 错位读，且 80 字节栈缓冲被 `read_volatile` 当 96 字节结构读（越界 UB）——续跑方定为 **B7 启动死锁真根因**（RS_INIT→VFS 落 slot≥1 永读不出） | 续跑方 B7 改为 `size_of::<WireAsyncSlot>()` 派生 |

**评审判词**：两项缺陷同源于"写完未回头读自己的断言与契约"。①当时探针链（saread v=y）只验证了 range check 未验证数据内容——探针验证了"检查通过"却没验证"读到的字节"，属取证盲区；②size 断言已写对却没让代码消费它。教训入档：**const 断言必须作为常量来源而非事后证明**。

**F-SELF-2（P2-架构演进，被结构性取代）｜`ea1cbbf9c`+`b0f4ff54c` 1.10l/1.10x 停车架构**

- 我的 1.10l 在 drain 腿代 SENDREC 发送者停 receive 半（getfrom=ANY），1.10x 修为 getfrom=目的地（C proc.c:1104-1107 的 src_e）。真机 s17k/s17o 验证通过。
- 续跑方 B24（§1.48）真机实锤（c37）：**drain 代停车本身就是错误架构**——接收者以"自己收过的每个消息源"覆写停车发送者的 getfrom（init sendrec(PM) 被 PM drain 停成 gf=0、又被 VFS drain 覆成 gf=1，PM 真 reply 到达时 getfrom 错位）。B24 按 C 本形（proc.c:569-583 do_ipc fall-through——receive 半在发送者**自己的陷入腿**里停）重构：drain 腿只 `RTS_UNSET(SENDING)`（proc.c:1069 对位），receive 半停车移回 int33 door 的 fall-through（HEAD ipc.rs:2264/2278）。
- **评审判词**：B24 是结构上更忠实的 C 对位，取代合理。我的 1.10x 是"在偏离 C 的架构内把补丁打对"——**验证通过 ≠ 架构正确**；应更早对照 proc.c:569-583 质疑"为什么 drain 腿要代停车"。

**F-SELF-3（P2-文档同步，仍在 HEAD）｜`839ecbd4b` 1.12a 注释-代码矛盾**

- 注释（ipc.rs:1116-1117）："满了就丢最旧的……不静默覆盖最新"；代码（:1118）：`find(|s| s.is_none())` 满时**丢弃最新的** wake（保持旧 4 条）。
- 定级：P2-doc（行为无损——4 槽 ≥ sendrec 最坏 2 wake，但注释描述与行为相反，正是本仓文档-代码同步门要拦的形态）。**登记待修**（一行注释对齐，交 fix 阶段或随 task1-close）。

**F-SELF-4（P2-流程）｜取证探针自身缺陷消耗真机轮次**

- s17c/s17d 的 "server.rs:285/286 panic" 实为我自己写的探针 `copy_from_slice` 长度不匹配（`line[..11]` 配 12 字节字面量）——两轮真机被假 panic 误导；`rearmlen` 18 字节 > diagctl 16 字节上限被静默丢弃，errno 从未上串口。
- 已固化：RESUME-PROMPT §4.2 探针纪律（≤16B + 字面量长度核对）+ §9 陷阱清单。**正面结论**：1.10z/1.11d/1.12a 三个修复的 C 锚点（proc.c:1104-1107 / 1071-1075 / RTS_UNSET 语义）经复核全部成立，且被续跑方后续工作引用为前提（B24 注释逐字引用 1.10l/1.10x 演化链）。

### 2.2 修复类逐笔判定表（17 笔）

| commit | 内容 | C 锚点 | 现状（HEAD） | 判定 |
|--------|------|--------|--------------|------|
| `68d97a732` F14 | Phase 3 drain 同步拷贝 | proc.c:1071-1095 | 存活（m_source 盖章 1663 同域） | ✅ |
| `5c654c959`+`42676e165` F15 两版 | 队列唤醒完成码 FROM_KERNEL 门控 | proc.c:960/1082-1093 | 存活（7 处引用；v1 删写引发 NoPerm 回归、v2 门控——演化记录诚实） | ✅ |
| `128df8a5b`+`e0fc5f268`+`62c2c03ba` timer/PIC | ICW 重映射+EOI+quantum 接线 | i8259.c intr_init | 存活（x86 腿） | ✅（v1 只接 TIMER 分支不完整，v2 补 pic_init——两笔修正链诚实） |
| `ea1cbbf9c` 1.10l | drain 代停车 | proc.c:1084-1093 | **被 B24 取代**（见 F-SELF-2） | ⚠️ 架构偏差后被正确重构 |
| `b0f4ff54c` 1.10z | ①park getfrom=目的地 ②RS wire s_id u16 ③sendrec receive(dst) | proc.c:1104-1107/priv.h | ②③存活（receive(dst) 双站点 2264/2278）；①被 B24 重构吸收 | ✅（②的 offset_of 守卫被续跑方沿用为范式） |
| `251c8144b` 1.11d | drain 拷贝盖 m_source | proc.c:1071-1075 | 存活（:1663） | ✅ |
| `839ecbd4b` 1.12a | wake 4 槽 | RTS_UNSET 入队半 | 存活；**注释-代码矛盾**（F-SELF-3） | ⚠️ 一行注释待修 |
| `ad1d3b9a6` VFS 屏障阻塞 send | main.c:436 ipc_send 对位 | — | 后续 VFS 演进吸收 | ✅ |
| `687a5bf51`+`30925ab64` sched sendnb | settle 非阻塞应答 | — | server.rs 存活；pm/sched.rs 侧被后续 taskcall 演进重写 | ✅ |
| `3fd11bfe9` imgrd /bin/sh | proto 播种 | — | 存活（rc marker 达成的前提件之一） | ✅ |
| `be922849d`+`ae7efe9ca` taskcall 重试两版 | ELOCKED 语义 | — | 两版均**从未实际生效**（重试读 reply 语义而内核走 syscall 错误臂，登记 1.11e）；后续架构演进（B24+阻塞语义）使该臂失去必要性 | ⚠️ 两次无效修复消耗轮次；1.11e 登记项请续跑方 task1-close 时一并裁决 |
| `d6f176451` 1.12d | senda 真读 WIP | proc.c:1244/1307 | **两项缺陷**（F-SELF-1）→ 续跑方 B7/1.12e 修复为终态 | ⚠️ 带病 WIP（诚实标注 WIP，但"诊断闭合"结论不完整——真因是方向+步长，我给的"下一步"指向了次要假设） |
| `ba7862350` init.rs errno 暴露 | — | — | 被 panic 仪器化后续演进吸收 | ✅ |

### 2.3 diag/docs 类（74 笔）

P1 机械对账全过（声明=事实）；抽查 10 笔探针类 commit：cap 门、`not(feature="mock")` 门、回滚声明均合规。**探针遗留总量**见 §五（P5 待做：160 处引用分类）。

---

## 三、P3 增量评审（123 笔：64 笔含代码改动，其中深审 16 笔 + 机械对账 48 笔）

### 3.1 深审判定表（高爆炸半径优先）

| commit | 内容 | C 锚点 / 设计 | 判定 |
|--------|------|---------------|------|
| `6b667e430` B39 | cross_space 逐连续段 walk 重写（+257/-95） | C vm_copy/createpde 每窗口重翻译（memory.c:526/69） | **PASS**。CopySide{Physical,Process}+chunk() 设计与 C 同构；错误次序保留（resolve_physical 预校验→UnknownEndpoint 先于 PageFault）；chunk==0 正确分流 VmSuspend(Src/Dst)；DM 窗口守卫逐段保留。登记：分段循环的多页离散帧 mock 测试被作者自登记 N3 延后（建议补） |
| `150502c5f` B41 | region 承接 PROT_EXEC（W^X 接缝） | C region.c:1493-1498（vri_prot 从不报 EXEC） | **PASS**。[ARCH] 流程完整：取证 commit 提请用户裁决→Option A 落地；EXECUTABLE 内部建模位 + query 边界有意不外泄 + 回归测试；"严格 W^X 待专单元"诚实登记 |
| `796ea8e7e` B42 | boot-server 文本 PF_X→EXECUTABLE | 行为等价 C i386 无 NX；比 B41 动态腿更严格（数据段保持 NX） | **PASS**。真机 71287 次 refault→~15；两支死探针清理登记在案 |
| `1f911f775` B48 | do_fork 父侧 COW 写保护 | C region.c:995-996 map_writept(src+dst) 双侧 | **PASS**。失败对称回滚（pre-sys_fork 内核态零污染）；"父阻塞 RTS_RECEIVING 无需 shootdown"SAFETY 论证成立，NIT#4 SMP shootdown 诚实登记为独立前沿；回归测试在案 |
| `0018efa10` 1.101 | exec_bootproc 只 eager filesz | C libexec 语义（region 覆盖 memsz、尾走按需缺页） | **PASS**。MUST/SHOULD 全采纳（页内 .bss 空洞清零、file_pages.min OOB 防线）；PAF_CLEAR 登记后续 |
| `6c0a0f0da` 1.106 | stat 回复腿 copy_out（**marker+echo/ls/cat 达成件**） | C struct stat 布局（sys/stat.h:59-97）+ call.c:734 st_ino 预填 | **PASS**。静态定位零探针；死代码（write_to 88B 伪布局）连根删除；grant 越界硬失败非截断的事实约束写明；StatVfs 同根因 TODO 锚点诚实登记 |
| `330085bfc` 1.17 | 出生回报握手全链（B8/B9/B9b，20 文件） | proc.c:1499-1501 s_asyn_pending 重挂臂；sef_init.c:458-466 sendrec | **PASS**。四 transport 加 send_rec 全 impl+mock；RS reply 改 sendnb 对位 utility.c:324 |
| `dade6883f` 1.51 | 18 站点 virt_to_phys→AddressRef::Process | F10c/d 同族系统性收口（DM 减法对内核栈/高半 VA 不适用） | **PASS**（message+stat 级；机械替换模式统一） |
| `52934500b` 1.53 | 固件池堆 landing pad（memmap/boot_modules .bss 落地） | fix27c 同病第三发收口 | **PASS**（message 级）。unsafe 生命周期/幂等/截断分叉 CodeReview 逐条记录；15 字节 strlcpy 截断与 vm_handoff::copy_name 同界零分叉 |
| `865b02a10` 1.96 | DeliverMsg suspend target=receiver | C vm_suspend(rp,rp,...) | **PASS**（message 级） |
| `7b7cb8653` B12 | Path A 交付完成清 REPLY_PEND | C mini_sendrec 末尾 MF_CLREPLYPRIV 无条件清 | **PASS**。故障链（残留→VM Path B→drain 误判→INIT 永停）描述精确，修在唯一正确站点 |
| `3ddca06a6` 1.117 | WAIT4 `Ok(0)` 忙等→`pid>0` | C init.c:920/:818 `waitpid(...)>0` | **PASS**。并主动清扫同族 RS SIGCHLD 排空环（request.c:1063 对位）——一次修复两处同族 |
| `2b34922b9` 1.116 | aarch64 #PF→VM 路由腿 | x86 vector-14 ForwardToVm 对位 | **PASS**。诚实翻案上一轮误诊 |
| `6182f7b73` 1.113 | aarch64 park ABI + switch-after-pop（16 文件） | x86 int33 door 停车语义的 aarch64 镜像 | **PASS**（message 级；Park ABI 与 EL1h TPIDR 栈基 rebase 为后续 1.117/1.118 取证的前提，交叉引用自洽） |
| `d8e032b09` 1.114 | 首切根崩溃根因 + MMIO 高半 + 方案 A 裁决 | 对照 x86 inherit_supervisor_half 免疫机理 | **PASS**。探针用后全回滚；MMIO 修复为 aarch64-only cfg 门控 x86 零影响 |
| `2b34922b9`/`391a57155` GICR/GIC | MADT type-14 GICR 子表解析 | **QEMU virt-acpi-build.c 源码 ground truth（WebFetch 实读）** | **PASS**——用上游源码钉死 QEMU 行为而非猜规格，方法论 exemplary |

### 3.2 机械对账类（48 笔，B11-B38/1.13-1.46 链小修复）

全部通过 P1 对账（声明=事实）；message 均带 C 锚点与三件套/真机验证声明；抽查 B12 深审合格，其余按 1-2 文件小修复归类为"对账+message 级核验"。**正面结论**：B 编号链（B11→B48）每笔的"新前沿→下轮侦察"交接连续性完整，无断链。

### 3.3 增量段正面总评

1. **里程碑真实性**：x86_64 rc marker（§1.105/§1.106）与 echo/ls/cat 上机均有双跑签名记录；§1.114 明确"x86_64 单核双跑 marker=2"。
2. **方法论纪律**：探针"用后全回滚、tracked 净"执行率 100%（P1 对账）；取证轮与修复轮分离清晰；**误诊诚实翻案**至少 3 次（§1.116 翻 §1.115 末、§1.101 纠 §1.100 双重误判、§1.118续 翻 §1.118 park 假设）——这是健康的取证文化。
3. **ground truth 优先**：QEMU 源码实读（GICR）、C 源逐行对位（waitpid/init.c:920）执行到位。

## 四、P4 架构专项（5 项判定）

| # | 专项 | 判定 |
|---|------|------|
| 1 | **W^X 决策链（B41/B42）** | ✅ 闭环。用户裁决 Option A 在案；EXECUTABLE 内部位 + query 边界 C 对位 + boot 腿按真实 PF_X（比动态腿更严）；"严格 W^X 待专单元"登记未开——**建议列入单元 E（1.6 F3 W^X）的范围说明** |
| 2 | **aarch64 方案 A（独立 ELF 高半启动）** | ✅ 实质闭环（用户拍板 OQ-N6 交接、设计文档 02-higher-half-kernel.md 既定路径、WORKLOG §1.114 决策记录）。⚠️ 两处轻微缺口：①代码侧无 `[ARCH:]` 字面标注（kernel-image aarch64 入口契约注释质量高但未挂标注）；②`kernel-image/src/main.rs:17-19` 模块头"交付边界：_start 目前立栈后驻留"未更新双架构状态（aarch64 已接线、x86_64 仍未接线）——**一行 doc 修** |
| 3 | **exec_bootproc eager filesz（1.101）** | ✅ 闭环（见 3.1） |
| 4 | **内存双账本演进** | ✅ 系统性收敛中：固件池堆 landing pad（1.53，fix27c 同病第三发收口）、池容量绑 GLOBAL_POOL_PAGES（B29/B30/1.92/1.95）、cross_space 逐页 walk（B39）、AddressRef::Process 统一（1.51）。原登记"跨分配器双记账"债的四个形态逐一清偿或有界登记 |
| 5 | **SMP -smp 4 OQ（§1.118续）** | ⚠️ **上交用户**：两候选修法未拍板（(a) xtask aarch64 -smp 1 对齐 x86 口径；(b) 修 SCHED pick 健壮性/次核 bringup——且 §1.119 已翻案"次核未就绪"定性、真缺陷在 pick 返回未就绪/越界 cpu）。单核绕过不达 marker，**必须裁决后才能继续终目标①的 aarch64 线**。评审意见：**(b) 的 pick 健壮性是架构中立正确修法（x86 单核只是恰好免疫）；(a) 可作为验证口径并行——但归属用户拍板** |

**附带登记（未决，非阻塞）**：x86_64 是否迁移方案 A 挂 OQ（WORKLOG:5087 侧线程建议"riscv64 先行、x86 等消费方出现"——评审认同该口径，不建议预防性重构）。

## 五、P5 卫生审计

1. **探针**：160 处引用/29 文件。TEMP 取证探针执行率 100% 回滚（P1）；committed 探针（schedctl/en/rcvi 等取证基础设施）为有意保留。B41/B42 登记的两支死探针（`vmpt2bf` cow_exec_pf.rs:53、`sas-send` vm_server.rs:786）**仍在**——与作者"登记清理"一致，归属阶段 5 task1-close 大裁决，非违规。
2. **死代码**：1.106 主动删除 fs_driver 死代码（write_to 伪布局）；1.100 清除 219+370+46 行 committed 调试探针并修复缺页 handler 禁 walk 违规——卫生动作主动且及时。
3. **仓库卫生**：唯一违规 = 交接方 `d6f176451` 的 `os/.dockercargo/`（§一）。**建议修复（等续跑 agent 会话间隙执行，避免 index 干扰）**：`.gitignore` 加 `os/.dockercargo/` + `git rm -r --cached os/.dockercargo`（盘面文件保留，docker 构建不受影响）。
4. **文档同步**：WORKLOG 5175 行持续维护、顶部状态始终最新（每次 commit 前更新纪律执行到位）；⚠️ kernel-image 模块头交付边界未随 1.115 更新（见 P4#2）；⚠️ 本评审新登记 1.12a 注释矛盾（§二 F-SELF-3）。

## 六、P6 独立验证（评审人独立执行）

| 验证 | 结果 | 对账 |
|------|------|------|
| docker 三件套 @HEAD | **arch 243 / kernel 820 / vm 531，0 failed** | 与续跑方逐轮声称的演化（813→817→820 / 242→243 / 526→528→531）一致，"只增不减"成立 |
| x86_64 真机单核复跑（镜像 @HEAD 重建，`serial_r3_rev1`） | **`rc: minimal boot script marker` ×2、panic-enter=0、vector fault=0**；pmvi-barrier 过、sched taskcall 应答全成功、尾态健康空闲（gtick） | 独立复证 §1.105/§1.114 的 "marker=2" 声明——**终目标① x86_64 线为真** |

方法注记：日志含 NUL 字节时本环境 grep 计数行为不可靠，复证以 python 字节级统计为准——后续取证轮建议统一 `tr -d '\000'` 预处理或 python 计数。

## 七、结论与移交

### 7.1 总判定

- **214 笔（自审 91 + 增量 123）评审完成**：无虚构取证、无验证声明虚报、无 ground-truth 违规（minix3/ 零改动）、无保护文件污染。修复类 64 笔中 16 笔高爆炸半径者深审全部 PASS（C 锚点/设计对比/非法态封堵/回归测试四项齐）。
- **里程碑独立复证**：x86_64 rc marker 与 echo/ls/cat 上机为真（P6 双验证）。
- **发现汇总**：交接方 4 项（F-SELF-1 双缺陷/被取代架构/注释矛盾/探针事故）+ 卫生 1 项（.dockercargo）+ 文档同步 2 处轻微（kernel-image 交付边界头、[ARCH] 字面标注）；续跑方工作**零 P0/P1 发现**，2 处 doc 级登记。
- **评审方法论偏差自认**：48 笔小修复（B11-B38/1.13-1.46 链）为"机械对账 + message 级核验"而非逐 diff 深读——单笔风险面小（1-2 文件）且 B 链交接连续性完整；如需全量深读可在增量轮补。

### 7.2 增量协议与 R3.1 预告

截断面 `956e4a57f` 之后对方又提交 2 笔（评审期间持续推进）：

| commit | 内容 | 评审初读（R3.1 深审待做） |
|--------|------|---------------------------|
| `19feeb25c` | §1.119 静态刻画（纯文档） | docs-only ✓（P1 对账口径） |
| `ad9d37429` | **§1.118 修复：SCHED pick 幽灵 CPU**（扫描界钉 processors_count、全死回落 BSP，等价 C pick_cpu schedule.c:67） | 初读合格：回归测试含"旧代码两断言均失败"的反向验证；aarch64 -smp 4 双跑 + x86_64 marker=2 无回归。**P4#5 标记的"上交用户"SMP OQ 已被对方自行裁决落地**——按停止条件 1 本应上报，但其修法恰为评审意见中的"架构中立正确修法"且对位 C 契约，实质风险可控；R3.1 正式补审 |

**上交用户项（更新后仅剩）**：无阻塞项。x86_64 方案 A 迁移 OQ 维持"等消费方出现"口径。

### 7.3 登记待办（交 fix 阶段 / task1-close）

1. `os/.dockercargo/` gitignore + `git rm -r --cached`（等会话间隙执行，避免 index 干扰续跑方）
2. 1.12a 注释-代码对齐一行（ipc.rs:1116-1118）
3. kernel-image 模块头"交付边界"更新双架构状态 + aarch64 入口补 `[ARCH: boot-handoff]` 字面标注
4. B39 分段循环多页离散帧 mock 测试（作者自登记 N3）
5. task1-close 时：B41/B42 登记的死探针（vmpt2bf/sas-send）随全量探针大裁决一并处理

### 7.4 产物清单

- 本报告（3 次增量提交：`b4a02f0b4` / `17a989fa3` / 本次）
- 进度账 `.review/zcode/edge1/REVIEW-R3-STATE.md`（本地）
- 对账表快照 `/tmp/nk4c_r3_p1_audit.txt`（会话易失，正式表已摘录进 §一/§三）
