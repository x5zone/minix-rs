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

## 三、P3 增量评审（123 笔）——进行中

（G1 x86_64 收官链 / G2 aarch64 bring-up / G3 散件；下一工作单元填充）

## 四、P4 架构专项——进行中

## 五、P5 卫生审计——进行中

## 六、P6 独立验证——进行中

## 七、结论与移交——待 P3-P6 完成
