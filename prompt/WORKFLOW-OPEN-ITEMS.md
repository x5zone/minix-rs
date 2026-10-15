# WORKFLOW-OPEN-ITEMS — 工作流开放项与裁决承接台账

> **性质**：本文件是 `agents-workflow-optim.md`、`todo_plan.md`、`EXECUTION-LOG-workflow-optim.md` 三份已归档规划/取证文件，**以及历轮 review / 人工审阅中产生的工作流层开放项**的承接台账（2026-10-08 建立；三文件完成使命后删除，全文可从 git 历史回捞）。
> 承接原则：只搬**仍未闭环的事项、被活规则引用的格式定义、以及不可重演的用户裁决**；已落地能力的规范源本来就在 `prompt/review-rules/` 与 `tools/`，不重复。
> 分流约束：本文件只收**工作流层**（规则、门、派生同步、流程裁决）的开放项。**文档内容层的待办不放本文件**——某篇要拆节、要补定义、要续扫同类条目，进各 stage 的 `todo.md`（如 `rewrite-notes/01-stage-kernel/todo.md`）；开发/缺陷与跨阶段联动项进 `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`。

## 一、开放项（需要行动或用户裁决）

### W-1 存量正文锚点降噪回炉批次（原 TODO-1，状态 READY）

- 能力已具备：`tools/doc-style-lint.sh` 的 SL-9（新增行阻断）与 SL-10（告警），判定口径与四类形态在 `prompt/skill/cmds/style-bible/SKILL.md`「硬裁决：读者体验优先于审查便利」；锚点底账格式 `.review/{tool}/{stage}/{doc-stem}.anchors.md`（移出与正文修改同一笔提交；**底账缺失的移出 = P0**，`review-doc-checklist.md` §3.95）。
- 未做：存量 774 篇逐篇回炉。当时基线（2026-10-06 实测，按纪律以现跑为准）：SL-9 命中 441 处、SL-10 命中 3085 处；热区 `02-stage-vm` 之外最密为 `02-rs-process-table.md` 178 处、`01-rs-boot-init.md` 109 处。
- 步骤 0（跨文档调研清单）从未产出，批次启动时先做。历史自包含反例（抄录的原文行）见 git 历史 `prompt/agents-workflow-optim.md` §2。

### W-2 `missing_docs` 配置与逐 crate 升级（原 todo_plan G2）

- 落点：`os/` workspace 各 crate 顶层文档注释完备化。启动前**必须先统计存量警告数量级**（`cargo build` 开 `#![warn(missing_docs)]` 试跑），属代码批次，未排期。
- 关联：原 G1（`tools/code-style-lint.sh`）已作废（2026-10-07 判定，能力并入本项与既有工具）；`gate-evidence-code` 的"代码可读性增量"行现挂钩 `cargo clippy -- -D warnings` + `tools/unsafe-audit.sh --diff`（已写入 review-process.md Step 3.6）。

### W-3 卓越样章双达标（原 todo_plan H4）

- 验收物 = 用户认可一篇"教科书文档 × redox 级代码"样章；候选与判据见 `review-doc-excellence.md` / `review-code-excellence.md`。未排期。

### W-4 Open Questions 台账（原 todo_plan 十二，逐条判定现状）

| OQ | 议题 | 现状 |
|---|---|---|
| OQ1 | 头部字段统一名 `Rust 实现` | **已闭环**（`doc-code-map.sh` 强制执行，缺失新文档判 P1） |
| OQ2 | 关联代码维度用 Step 3.6 而非新增 Gate I | **已闭环**（review-process.md Step 3.6 在位） |
| OQ3 | 文风 lint 存量零容忍？ | **已闭环**（增量阻断 `--diff` + 存量只报告） |
| OQ4 | 演练文档用 `02-stage-vm/15-ipc-dispatch.md` | **作废**（流程已由 vfs/08 干跑 + pm/09 全量实跑两轮替代） |
| OQ5 | pre-commit / CI 钩子 | **裁决点到**（原条件"跑两轮 review 后再评估"已满足）。默认推荐**继续不做**：门都在 review 流程内触发，钩子会把卫生项批量修逼进提交路径。待用户表态 |
| OQ6 | 编辑战役（存量清洗）启动时机 | **已闭环**（用户表态延后；批次计划承接于 W-1） |
| OQ7 | `[ARCH:]`/`TODO`/`DEFERRED` 保留正文 | **已闭环**（保留，项目硬要求/既有流程管理） |
| OQ8 | QEMU 门触发面 `os/kernel|arch|boot-shim/**` | **未落地**（规范源 grep 无挂钩），属代码批次，待实施 |
| OQ9 | unsafe 基线冻结 vs 逐月递减 | **已闭环**（`unsafe-baseline.txt` + `--diff` 只拦新增；递减留待治理批次） |

### W-5 安全提示：`.claude/settings.local.json` 中段通配授权（已复验关闭）

EXECUTION-LOG §2.5 曾发现形如 `Bash(rg "...*" notes/rewrite/...)` 的中段通配授权（≈ 允许在该位置插入任意参数而不再询问）。
2026-10-08 复验：`python3 -c` 枚举 permissions.allow 全部 71 条，含 `*` 的 12 条均为尾部 `Bash(cmd *)` 标准形式，**中段通配已不存在**（随文件后续演进消失）。本项关闭；今后若再现中段通配，按原提示清理即可。

### W-6 读者视角的执行装置缺口：卡点预判与可读性机器判据（状态 OPEN，2026-10-12 登记）

- 缘起：人审 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 第 1 章时连出 8 处修正（过渡句方向反、引言塞细节、残句、压缩简写、恒等与高半映射讲不清、跨篇重复取证、带圈数字渲染不清、引用标签含糊），**其中没有一处是被工具挡下的**，全部靠人读出来；而形式类噪声（工单号、日期、坐标噪声）早就有 SL-1～SL-11 挡着。结论：规则文本不缺，缺的是把"读者视角"变成可执行装置。
- 规则已有、无机器判据（本轮实测）：`prompt/skill/cmds/style-bible/SKILL.md` 第 57 行明令禁"压缩简写"，而"清单又越过它直接供给建表那一步"这类句子 `doc-style-lint.sh` 零次报警（判据：该句改成"页表本身也住在内存里，它要的页只能从清单的可用区间里拿"才读得通）。
- 规则真空（需先写规则再谈门）：① 引号风格与本文件主导风格不一致——本轮 agent 三次引入弯引号，门全绿（`grep -o '“' 该文档 | wc -l` 修前 15、修后 0，`git show HEAD:` 同文件为 0，即全属新引入）；② 含糊尾巴"及邻近字段/相关部分"；③ 跨篇共享取证链的判据归属（本轮 01 篇 §1.4 与 02 篇 §2.4 曾互标"这条依据链与本处同一条"，导致一头把四步证据压成一句嵌套长句）；④ "预判读者会卡在哪"——本台账所在的规则库里仍无这一条，而本轮人审又两次撞上它（改完的段落 agent 自评估得通，人一读仍卡）。候选落地形态已记在下一条（复述测试产物形态强制、新手问题预演）；这两条最早出自第一轮多模型分头评审的共识，但那份评审稿尚未纳入版本库，所以此处不写文件路径——等那批评审文件入库后再补上精确出处。
- 候选三条（按成本递增，均需走 `prompt/README.md` 三端同步与四道派生门）：一、`doc-style-lint.sh` 增两条机械判据（引号主导风格一致性、含糊尾巴禁用），self-test 补正反例，当天可上；二、把 structure.md §11 的"整段复述 + 卡点清单"从产物要求升级为 Blocker Gate（缺则 FAIL），治的是"同一个 agent 自认为读得通"这一根本盲区，与 AGENTS.md 已记载的"VERIFY-CHECK 同 agent 局限"同源；三、定向吸收外部成熟条目进模式库（Diátaxis 象限纯度治"引言塞细节"、Google/Microsoft 开发者文档风格指南的句长与示例可复制条目、Strunk & White"一句一义"治压缩长句），必须写明与本项目"教学性必须展开"的适用边界。
- 本轮裁决承接：先只登记本条，不动规则源与工具；存量文档暂不扫，只防新增（见第三节 R5）。本轮已落地的两件与此相关：01 篇新增 §1.3「把第一章收成五句话」（各篇第一章末节固定为范围表，故骨架小结插在其前）；02 篇 §2.4 与 01 篇 §1.4 的互指改为单向（判据只在 owning 篇给全，他篇只引结论）。

### W-7 术语「腿」是仓库自造比喻，需登记为受控比喻（状态 OPEN，2026-10-12 登记）

- 缘起：人审 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §2.2 时问了一句"腿这个术语在内核开发、尤其中文内核开发里用得很多吗"。查证结论：**不是通用术语**，中文通行的是路径/通路/通道/侧/半（fast path = 快路径；sendrec 的 send 半与 receive 半）；本仓的"腿"是英文 "leg"（the read leg / the write leg）的直译。
- 实测分布（可重跑：`grep -ro 腿 os/ rewrite-notes/ prompt/ --include=*.rs --include=*.md | wc -l` 得 3416 次，分布在 178 个文件；含该字的行合计 2243，其中 1371 行（六成的量）在 `rewrite-notes/coordination/` 的案卷与台账里，主要是 `NK4C-WORKLOG.md`）——真正需要治理的是代码与正式文档里已经固化的用法，集中在两处：`os/kernel/src/ipc.rs` 32 行（写腿、读腿、发送腿、接收腿、drain 腿、陷入腿、完成腿、自返腿）与 `rewrite-notes/08-stage-is/04-is-data-acquisition.md` 4 行（"VFS 两腿"）；文件名层面也有：`os/arch/tests/riscv64_trap_leg_shape.rs`、`os/arch/tests/riscv64_return_leg_pin.rs`。
- 为何不建议一刀切废除：它在这些位置承担了真实语义——"一个复合操作里可独立成败的一段路径"，比"半"精确（sendrec 本来两个半）、比"分支"少歧义（分支已被 fork/branch 占用）。需要治的是**无约束扩散**，不是这个词本身。
- 要立的两条规则（都要走 `prompt/README.md` 三端同步与四道派生门）：① 「腿」列为受控比喻，**不得在概述章/入门章首次引入**，并在首次出现处要给定义；② 同一段落不得把"腿"与"通路/通道/出口"混用——讲的是"输出有两个去处"这类分支选择时用"出口"，不套用"腿"。可执行形态：写进 `prompt/skill/cmds/style-bible/SKILL.md` 的词汇约束，或给 `tools/doc-style-lint.sh` 增一条**报告级**判据（同段混用两类词时出清单，不阻断，沿用 W-1 的"增量阻断 + 存量报告"口径）。
- 本轮已做的单点修正：`01-boot-shim-bootstrap.md` §2.2 的"串口这一腿"（该文件**唯一**一处「腿」，由本会话引入）已改为"串口这个出口"，并把该节两段合并简化（复述 §2.1 那条 assert→panic→刷屏链的部分删掉，按"简述即可"口径）；同句里的箭头速记也已改散文化。

- 2026-10-13 归口：本条的判据已并入 **W-9**（中文惯用法优先：不生造、不直译），后续落地以 W-9 为准；本条保留作为实例与已修记录。

### W-8 反事实构件的命名：引导句要语义化，套路句只作 fallback（状态 OPEN，2026-10-13 登记）

- 缘起：人审 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 时对粗体构件 `**如果反过来想**` 连提三次意见——（一）名字不贴切：它被用在**契约／顺序被违反**的场合，那里根本没有"另一种设计"可反过来；（二）全篇三四十处读下来像模板，还出现过与前后文无关的插入项把"其一/其二"配对打断；（三）要求"尽量去掉"。本轮 01 篇已清零（现测：`grep -c 如果反过来想 rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 为 0），改法是**按内容给每一处起一个具体引导句**。
- 要立的规则（进 `prompt/skill/cmds/style-bible/SKILL.md` 或 `prompt/review-rules/review-doc-checklist.md`，需走 `prompt/README.md` 三端同步与四道派生门）：
  1. **首选：引导句点明本段真正改动的那个对象**，读者不看正文就能知道在权衡什么。本轮实例：`把这三个数字改成由调用者传入会怎样`、`不做深拷、把 KernelInfo 的切片按原指针存进内核的全局槽会怎样`、`把缺件处理改回静默跳过会怎样`、`若恒等映射只覆盖可用内存、或地址对称时照做第二步`。
  2. **套路句只作为 fallback**：`若 X 不这样做会怎样`、`少了这道防线会怎样`、`次序错了会怎样`、`破坏约定会怎样` 这四类句式，只在确实概括不出更具体对象时使用，**不得当默认标签**。
  3. 一段内含多个反事实时各起各的引导句；若共用一句，句里必须能看出**分支数与各自对象**（形如"……（两处）："，下文再用"其一/其二"点名具体做法）；并且**不得在两个配对分支之间插入无关内容**（本轮修过一次：一段讲两个取页方向的文字插在了"其一"与"其二"之间）。
  4. 旧标签按 W-1 的"增量阻断 + 存量报告"口径处理：改到相关段落顺手换，不单独开批量清理；换完要 grep 检查是否有交叉引用该构件名（本轮 §1.2 指向 §2.6 那句就需要同步）。
  5. **反事实段必须先验证"这条退路在代码里真的可能被写出来"**（本轮新增，源于一个严重错例）：人审 `01-boot-shim-bootstrap.md` §2.7 时发现一段虚构推演——"若 ARM32 省掉 `setup_mbi`、让 `get_parameters` 直接用 x86 的第二个形参（`ebx`）……会读到栈上残留值"。事实是：两侧是**两份独立文件里的同名函数**，ARM 侧签名是 `minix3/minix/kernel/arch/earm/pre_init.c:pre_init`（`pre_init(int argc, char **argv)`）与 `get_parameters(kinfo_t *cbi, char *bootargs)`，**既无 `magic` 也无 `ebx`**，根本不存在这条退路（已于 2026-10-13 整段删除，并在 §2.7 步骤表里补上"两侧形参不同"）。**跨架构同名函数不得互相套用**；写反事前先跑一次签名/定义 `grep`，确认被改动的对象确实存在；验证不过就删段，而不是改措辞。
  6. **反事实段必须有新增信息**，自问一句："删掉这段，读者会少知道什么？"——答不出就删。本轮按此删了两处：§2.8 的"把 `assert` 全部去掉（`NDEBUG`）"（不仅复述，结论还被推演证伪：重叠区间第二次切除是空操作，内存账不会被弄坏）与 §3.1 的"若提前调用 `ExitBootServices` 会怎样"（规矩与理由已由源码注释 `os/boot-shim/src/uefi_helpers.rs:169` 在同一节的代码块里给全："Both file accesses must happen before ExitBootServices because they depend on the SimpleFileSystem protocol."，报错消息另在 §5.1 与 §6.3 的诊断表里）。**"已有规定，但故意违反会怎样"不算反事实**：它不提供判断力，只把上文再说一遍。判定与第 1 条配套：先看有没有新信息，再看引导句能否具体。
- 已做 / 未做：01 篇已全清；**其它篇仍有大量该标签**，按含该词的行数排序（现测：`grep -rc 如果反过来想 rewrite-notes/*/*.md | grep -v ':0$' | sort -t: -k2 -rn`）为 06 篇 38、05 篇 26、03 篇 22、02 篇 19、07 篇 16——随各自审阅轮次顺手处理，不进独立批次；本条只负责把判据先立下来，避免新写的段落继续用套话。

### W-9 中文惯用法优先：禁止直译词与生造词（状态 OPEN，2026-10-13 登记）

- 总则（用户提出，**W-7 归入本条作为实例**）：文档是给人读的中文，术语必须用**中文惯用表达**；不得把英文词面直接搬进中文（直译），也不得临时造一个没有共识的词（生造）。自问三句：这个词在中文技术书里有没有通行写法？读者会不会按另一个意思理解（歧义）？换个说法是否更短更清楚？三句任一不过，就重写。
- 已确认的实例：
  1. **`port` 的"架构移植产物"义不译"端口"**（本轮）："端口"在中文里第一反应是网络端口，属歧义；改用"两种架构 / x86 这一侧 / 各架构不同"。已修：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 的 11 处移植义清零（提交 `2eac4586e`）。**但 `port` 的网络端口义、I/O 端口义、端口号义必须保留"端口"**（例如 `rewrite-notes/17-stage-net/06-lwip-ipsock.md` 的"特权端口"），所以**只能逐处看词义分流，禁止全局替换**；全树现测（`grep -rc 端口 rewrite-notes/*/*.md | grep -v ':0$'`）为 87 篇、504 行含该词——这只是存量规模读数，不等于待清清单（其中多数是正当词义）。
  2. **「腿」= 受控比喻**（原 W-7）：输出通路叫"腿"属仓库自造；已有通行说法（通路/出口/通道）时不生造——判据与本条一致，故不再单立。
  3. 本轮还改掉两个自造说法：把适用边界说成"射程"、把限制在低地址说成"关在"（均属临时比喻）。
- 拟落地形态（均属改规则源，需走 `prompt/README.md` 三端同步与四道派生门）：一、把"中文惯用法优先"写进 `prompt/skill/cmds/style-bible/SKILL.md` 的词汇约束段，W-7 并入；二、给 `tools/doc-style-lint.sh` 增**报告级**判据（同一行同时出现"端口"与移植线索 `arch/i386|earm|两种|换个|各自写` 时列清单，不阻断），沿用 W-1 的"增量阻断 + 存量报告"口径；三、维护一份直译/生造对照表放 `prompt/review-rules/`，由文风门引用。
- 本轮未做的：对照表没开写（不凭印象列条目，每条都要有真实案例支撑）；存量篇目随各自审阅轮次逐处判断，不进独立批次。本条只先把总判据立下来，避免新写段落继续用直译词。

### W-10 关联代码声明与 §7 参见的位置重定（状态 OPEN，2026-10-15 登记；本轮**只登记不实施**）

- 缘起：人审 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 头部第 3–4 行与 §7 参见第 2962–2965 行——同一份源清单被抄两遍，且头部那遍是 §7 的**严格子集**（C 侧 §7 多 `mpx.S`、`const.h`、`arch/i386/include/vm.h`、`system/do_getinfo.c`、`servers/{mib/kern.c,vm/pagetable.c}`；Rust 侧多 `minix-boot/platform.rs`、`kernel/memmap.rs`、`arch/paging.rs`、`kernel-image/bootface{,_a64}.rs`、`servers/vm/{vm_server,alloc_page}.rs`、`minix-types/types/boot.rs`），读者第一眼看到的是全篇最难读的长行。方向性裁决见第三节 R7：**清单收拢到 §7，头部不再长罗列**。
- 卡点是工具实现，不是契约本身：`tools/doc-code-map.sh:86-97` 用 `head -30 | grep -E '^>[[:space:]]*\*\*Rust (实现|模块)\*\*' | head -1` 定位声明——"头部必须有"与"头部只能一行"都是 `head -30` / `head -1` 这两处的结果。现测（取证在 `tmp/docfmt-01/`）：删掉 01 篇头部两行，关联代码清单 32→31，唯一掉出的是 `os/kernel/src/globals.rs`（正文 §3–§5 未出现过它），另触发 L154 的"缺头部字段"告警（规范源 §1.1 判 P1）。即**头部瘦身对机器门几乎零风险**；而声明整体移入 §7 后，连这一条也由 §7 清单覆盖住了（预演读数见本节末第 4 条）。
- 落地顺序（**不能反**，反了 01 篇改完当场 Gate 0 红）：
  1. **先改工具**（定案，2026-10-15 二次裁决；完整目标形态与逐条读数见本节末「目标形态与预演读数」）。抽取改为四级优先序合并去重：**① 语义小标题声明块**（定名 `关联 Rust 源码清单`，取该标题之下、下一个 `###`/`##` 之前的区段）→ **② 头部 `> **Rust 实现**` 存量兼容**（14 篇仍长这样，不能把已过门的篇目判红）→ **③ 正文 §3–§5 的 `os/*.rs` 引用** → **④ 全文回退**（现状保留）。两处位置限制 `head -30` / `head -1` 一并去掉；来源标注由"头部声明"改为"声明字段"（`doc-code-map.sh:140` 的 `grep -qxF ... "${TMP}.hdr"` 与 L149/L154 文案同步）。同一笔里还要修 **token 正则的边界缺陷**（现正则要求 `.rs` 后紧跟反引号 → 全树 97 处含 `.rs` 的花括号簇整条抽不到，读数与两种形态见本节末第 3 条）。**小标题要当 grep 键用，必须满足"正文不出现"**：`关联 Rust 源码清单` / `关联 C 源码清单` 现测全树 0 命中；而 `**Rust 实现**` 这个措辞在语料里有歧义——`rewrite-notes/02-stage-vm/draft/04-physical-memory.md:1126`（§3.1）与 `14-cow-mechanism.md:982`（§4.1）都拿它当正文段落小标题，所以②这一路**必须继续要求行首 `>`**，self-test 收这条负例。**不采"整章扫 §7"**：章号不统一（正式篇 `## 7. 参见`、draft 用 `## Ch6`/`## 6. 参见`），且 §7 混列 C 源/构建入口/相关篇，整章扫会把"本篇关联代码"从"实现落点"放宽成"引用面"（`os/servers/vm/src/vm_server.rs` 在 01 §7 是 VM 侧交接点，不是本篇落点）。
  2. **补门的回归保护**。`tools/doc-code-map.sh` 现无 `--self-test`（`grep -c 'self-test' tools/doc-code-map.sh` 为 0），`.github/workflows/rules-consistency.yml:86-94` 的自测腿也不含它（`grep -c doc-code-map .github/workflows/rules-consistency.yml` 为 0）——这个契约字段目前**没有任何测试**，2026-10-08 那次"§3-5 抽不到就全文回退"正是在无测试状态下打的补丁。改工具须同批补 self-test，并把这条腿加进 `rules-consistency.yml`。用例清单（逐条都是本台账已实测过的形态，接手者可直接抄为夹具）：正例——头部 `> **Rust 实现**` 声明、§7 语义小标题声明块、`{a,b}.rs` 同后缀簇、`{a.rs,b.rs}` 自带扩展名簇（现正则在这条上漏抽）、`os/kernel-image/{check-layout.sh,src/main.rs}` 混合后缀簇（现正则也漏抽）、`无（理由）` 豁免形态；反例——正文段落小标题 `**Rust 实现**`（无 `>` 前缀）不得被当声明、目录簇 `os/libs/minix-{a,b,c}` 不得被当文件、不存在路径 → 退出码 1。
  3. **再改规范源，一次改全**。`prompt/review-rules/review-doc-checklist.md` §1.1（第 63–78 行：字段名、冒号、"禁止花括号展开"、"逗号分隔"、"新文档缺失判 P1"的位置措辞）、`prompt/skill/review-doc-skill.md` §1.1（第 44–49 行，同源另一份）、`prompt/DOC-ITER-WORKFLOW.md:23`（"读目标文档**头部字段**列出的 C 真值源与 Rust 实现"→ 含 §7）、`prompt/review-rules/review-process.md` 与 `review.md` / `review-cmds.md` 中指向 `doc-code-map` 产物的表述、`tools/review-gate-check.sh:258-262`（B4.1 只断言 scan.md 含"关联代码清单"字样，改文案时别踩）、派生端 `.claude/rules/review-process.md` 与 `.claude|.codex|.trae|.agents/skills/**`。改完跑四道门：`tools/generate-derived-skills.sh` → `tools/check-review-rules.sh` → `tools/lint-review-rules.sh` → `tools/diff-trae-skills.sh --only-diff`。
  4. **最后才动文档**。头部两行**整行删除、不留指针**（用户二次裁决：头部要清爽，由工具去适应文档形态，不是反过来）；原先担心需单独安置的 `os/kernel/src/globals.rs` **不需安置**——§7 的 Rust 清单本就含它，预演已证**01 篇**对现清单零丢失（见本节末第 4 条）。**注意：这个零丢失不是普遍性质，其它篇一律先做下条的前置工序**。同阶段存量 14 篇（现测：`grep -rl '^> \*\*C 真值源' rewrite-notes/01-stage-kernel/*.md | wc -l` 为 14，`Rust 实现` 行为 13；全 `rewrite-notes/` 树也仍是 14，即该头部形态只在本 stage 成立）是否一起迁，随 01/02/03 串行迭代的结果再定——按本文件分流约束，单篇重排属文档内容层，不进本台账做批次。
- 花括号簇的判据与规则漂移（本轮已裁决采"同目录 **≥3 个文件**才用 `{a,b,c}`，1–2 个写全路径，正文散文里禁用簇"）：工具侧 `expand_braces()`（`doc-code-map.sh:58-81`，2026-10-08 加）早已支持展开，而规范源 §1.1 第 74 行仍写"禁止花括号展开（`os/x/{a.rs,b.rs}` 不被解析）"——**规则源落后于工具**，必须一并改掉。同条还写"逗号分隔"，而 14 篇实作一律用顿号（提取正则只认反引号包裹的 token，分隔符对机器无意义），一并放宽为"反引号包裹、分隔符不限"。另加一条硬约束：**簇的末位必须以共享扩展名收尾**（`{a,b}.rs` 可抽），混合后缀拆成两条写——`{check-layout.sh,src/main.rs}` 与 `{proc.rs,lib.rs}` 两种形态**当下整条抽不到**，属活的静默漏抽缺陷类，详见下节末条。
- **删头部的前置工序（七篇互检发现，2026-10-15）**：本条初稿写的"头部瘦身零风险"**只在 01 篇成立**，而且是巧合——01 篇是唯一把 §7 写成完整清单的篇目。拿七篇互检（按 `doc-code-map.sh` 同口径重算：只看 `.rs`，且头部、§7、正文 §3–§5 **三处均无**才算真丢）：`02` 真掉 **9** 条（`boot.rs`、`opensbi_helpers.rs`、`main.rs` 多个、`build.rs`）、`03` **7** 条（`lib.rs`、`globals.rs`、`memmap.rs`、`smp.rs`）、`12` 3 条（`globals.rs`、`proc.rs`、`trap_dispatch.rs`）、`05` 3 条、`06` 2 条、`13` 2 条（`proc_table.rs`、`syscall_signal.rs`），`01` **0** 条。（上一轮曾按宽口径统计成 `02` 14 项/`03` 9 项，其中含 `.ld`/`.sh`/非 `.rs` 项——那些本就不进门的能力范围，对机器无损失，按门口径以本条为准）。**故落地顺序第 4 步前必须先做一步：把头部独占项全量并入 §7 的关联清单，再跑 `tools/doc-code-map.sh {doc} --check --format=plain | wc -l` 对账迁移前后条数**——条数下降即不准删头部。（该对账命令就是本条初稿里 `os/kernel/src/globals.rs` 那个问题的通用解，不靠人工发现）
- 七篇互检的正面结论（假设成立的部分）：七篇的参见章**均为末章**、其后无二级标题（所以"小节编号顺延"不会被附录插队）；除 01 篇有 `### 7.1 符号速查` 外，`02`/`03`/`05`/`06`/`12`/`13` 的 §7 **一个小节都没有**（纯 bullet 罗列）——新建时首编号就是 `7.1`，不存在"从 7.1 挤到 7.5"的统一算法；标题关键词 `关联` 在正式篇标题里零撞车（全树唯一一处是 `todo.md:202` 的 `### 7.5 文档条目 ↔ 架构建议关联`，非正式篇）。
- 明确降级、本轮不做（取舍依据见 R7 ③④）：① §7 的渲染可读性从低于合同字段的正确性维护——可点击链接不做（现第 2962 行四篇用裸反引号，同阶段 04/05/06 的 §7 用 `[xxx.md](xxx.md)`；四篇文件均已确认存在，改链接不会触 `tools/notes-link-check.py` 断链门，但属可观感收益而非必要）；② **不给 §7 每文件补一句话说明**——每句都是新的事实断言，按真值优先序须逐条回 `minix3/` 与 `os/` 核对，成本远超结构收益；③ §7 版式只做"分组小标题 + 每条一行"的最低必要形态（声明行要放进 §7，总得是可解析的形态）。
- 本项真正的教训（建议单独立规则，需走四道派生门）：现有三端同步纪律（`prompt/README.md`、`AGENTS.md`）只管"改规范源 → 同步派生端"这一个方向，**"改工具 → 源规则没跟"没有装置拦着**，所以才攒出 §1.1 那句过期禁令。候选可执行形态：`tools/lint-review-rules.sh` 增判据，把工具内的行为声明与源规则文本做关键词对账；退一步至少在 `review-process.md` Step 5.7 Rule Discovery 加一问"本轮改过工具吗？源规则跟了吗？"。

#### 目标形态与预演读数（2026-10-15 追加；接手者落地后应复现同一组读数）

预演方式：`python3 tmp/docfmt-01/mk_target.py` 从现文生成副本 `tmp/docfmt-01/target.md`（删头部两行 + §7 换下面的形态），**未改任何正式文件**。以下所有读数均出自该副本与现工具的实测。

目标 §7 形态（以 01 篇为样板，可直接照抄；其它篇的编号与小节数按自身情况取）：

```md
## 7. 参见

### 7.1 前置与后续篇

（每条一篇，带一句它管什么；同阶段 04/05/06 的 §7 已是此形态）

### 7.2 关联 C 源码清单

- `minix3/minix/kernel/arch/i386/{head.S,pre_init.c,pg_utils.c,kernel.lds,mpx.S,memory.c,arch_smp.c}`
- `minix3/minix/kernel/arch/earm/{head.S,pre_init.c}`
- `minix3/minix/include/minix/{param.h,const.h}`
- `minix3/minix/kernel/{table.c,system/do_getinfo.c}`
- `minix3/minix/servers/{mib/kern.c,vm/pagetable.c}`
- `minix3/minix/include/arch/i386/include/vm.h`（单文件写全路径，不套簇）
- `minix3/sys/arch/i386/include/multiboot.h`
- `minix3/etc/boot.cfg.default`

### 7.3 关联 Rust 源码清单

- `os/boot-shim/src/{main,lib,loader,uefi_helpers,opensbi_helpers}.rs`
- `os/libs/minix-boot/src/{kernel_info,boot_shim,handoff,platform}.rs`
- `os/kernel/src/{lib,boot_alloc,globals,memmap,vm_handoff,misc}.rs`
- `os/arch/src/arch/{pt_alloc,paging,paging_ext}.rs`
- `os/arch/src/x86_64/paging.rs`
- `os/kernel-image/check-layout.sh`、`os/kernel-image/src/{main,bootface,bootface_a64}.rs`
- `os/servers/vm/src/{vm_server,alloc_page}.rs`
- `os/libs/minix-types/src/types/boot.rs`

### 7.4 构建与回归入口

- `os/qemu-tests/README.md`
- `os/qemu-tests/{test-shim-bootmarks-aarch64,test-riscv64-uboot,test-kernel-image-riscv64-bootface}.sh`
- `os/kernel-image/check-layout.sh`

### 7.5 符号速查（C ↔ Rust ↔ 章节）

（现 7.1 顺延为 7.5，表体不动）
```

头部结果：第 3–4 行整行删除，引用块从现有的 `> **路径约定**` 起。

1. **小标题选型实测**（`grep -rE '<候选>' rewrite-notes/ prompt/ --include=*.md` 的命中行数）：`关联Rust源码列举` 0、`关联C源码` 0、`关联.*源码.*清单` 0——三个候选键都干净；`源码清单` 19 行/16 篇（太泛，不适合当键）；`关联代码清单` 是 `doc-code-map.sh` 自己输出段的段名（prompt 命中 27 行），**不复用**，否则工具产物与文档小节同名。定名取「关联 C 源码清单」/「关联 Rust 源码清单」：按 W-9 的中文惯用法口径用"清单"不用"列举"。
2. **写法硬约束（会把门判红的一条）**：§7 下的小节层级**必须是 `###`，不得升 `##`**。`doc-code-density-check.sh` 只按 `^## ` 切章、并按章标题词表 `测试|参见|附录|概述|概念|目录|术语|导航|小结|导览|速查|总览|全景` 排除非承载章；「关联 Rust 源码清单」不含任一排除词，一旦升为 `##` 就会被当承载章并要求贴节选码，`--check` 下判 P1。预演实测：目标形态下仍报 `7. 参见 … 非承载章型(报告)` 且"✅ 承载门阻断项通过（4 个可判章均有码/命令）"。
3. **现正则的边界缺陷（本项发现的最重要事实，与本文档重排无关但必须一并修）**：`doc-code-map.sh:96` 的 token 正则要求 `.rs` 之后紧跟反引号，于是末位以 `}` 收尾的簇**整条匹配不到**。实测探针（`tmp/docfmt-01/regex_probe.txt`）：`os/kernel-image/{check-layout.sh,src/main.rs,src/bootface.rs,src/bootface_a64.rs}` 与 `os/kernel-image/{check-layout.sh,src/main.rs}`（01 篇头部第 4 行现在就长这样）两条 **0 命中**；而 `os/kernel/src/{lib,boot_alloc,globals,vm_handoff,misc}.rs`、`os/servers/vm/src/{vm_server,alloc_page}.rs` 正常命中。影响面（全树 `os/` 簇共 470 处，其中不含 `.rs` 的目录簇 48 处本就不应抽）：**含 `.rs` 却抽不到 97 处**；排除 `archive`/`draft`/`.design` 后为 **69 处、涉 20 篇正式文档**。两种漏抽形态：① 混合后缀簇（`.sh` 与 `.rs` 同簇）；② 簇内每项自带扩展名 `{proc.rs,proc_table.rs}`（06 篇头部即此形）。修法：按反引号整段取 token → `expand_braces()` → 再判 `endswith(".rs")`（先剥 `:符号` 尾巴）；文档侧约定"簇末位以共享扩展名收尾、混合后缀拆两条"。
4. **目标形态后的清单覆盖（两种正则都跑过，接手者应复现）**：声明块 现正则 22 条 / 修正则 25 条（差的 3 条即第 3 条的边界缺陷）｜正文 §3–§5 31 条｜并入 **35 条**；对比现工具清单 32 条 → **零丢失**（原头部独占的 `os/kernel/src/globals.rs` 已由 §7.3 覆盖），**新增 3 条**（`os/arch/src/arch/paging.rs`、`os/libs/minix-boot/src/platform.rs`、`os/servers/vm/src/alloc_page.rs`——都是 §7 本来就列了、头部没列的）。存在性 25/25 通过。
5. **其它四道门在目标形态下的读数**：`doc-code-map.sh`（现工具）对头部已删的副本仍打"⛔ 缺 `Rust 实现` 字段"告警且清单降到 31 条 → **这就是"先改工具后改文档"不可反的实际后果**；`doc-style-lint.sh` exit 0、0 命中；`anchor-resolve.sh --check` → `anchors=103 resolved=103 zero-def=0 multi-def=0`（与原文基线逐字相同，即本次重排不动锚点）；`notes-link-check.py` 报 4 处断链——**这是副本落在 `tmp/` 导致的相对路径解析结果**，四篇目标文件均已确认存在于 `rewrite-notes/01-stage-kernel/`，落位后须实跑复验。
6. **复现命令**（逐条可重放）：`python3 tmp/docfmt-01/mk_target.py` → `bash tools/doc-code-map.sh tmp/docfmt-01/target.md --check --format=plain | wc -l` → `bash tools/doc-code-density-check.sh tmp/docfmt-01/target.md` → `bash tools/doc-style-lint.sh tmp/docfmt-01/target.md` → `bash tools/anchor-resolve.sh --check tmp/docfmt-01/target.md`；正则探针在 `tmp/docfmt-01/regex_probe.txt`（用 `grep -oE '`os/[A-Za-z0-9_/.{},-]+\.rs(:[^`]*)?`'` 重放）。

### W-11 正文符号锚点改「短锚 + 链接」双通道，保真度标签去"逐字"腔（状态 OPEN，2026-10-15 登记；本轮**只登记不实施**）

- 缘起：人审 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §2.6.2 反事实段（现第 1050 行）——一句话说清机制，中间埋着五条全路径锚点。用户提出改用 `[短锚](全路径)` 形态：读者读论证，路径信进链接。方向裁决见第三节 R6。
- 目标重新框定（2026-10-15 二次澄清）：**链接是否可点不在射程内**；两项目标是——渲染后长路径被隐藏（读者体感），且全路径仍以文本形式留存于源文件（工具与 workflow 可用）。这个澄清直接推翻了本条初稿"必须先改 `anchor-resolve.sh`"的判断（错在以为锚点只能待在反引号里）——`extract_anchors()` 的正则 `[A-Za-z0-9_./-]+\.(rs|c|h|S|asm|ld):…` 并不排除 `(...)` 上下文，所以**链接目标里的 `全路径:符号` 现在就被正常识别**。
- 六种写法的实测对账（探针文件保留在 `tmp/docfmt-01/probe-hide.md`、`probe-p1b.md`、`probe-p6.md`、`probe-anchor.md`、`probe-links.md`）：

| 形态 | 写法 | `anchor-resolve` | `notes-link-check` | 判定 |
|---|---|---|---|---|
| P0 现状 | 行内反引号裹全路径锚点 | resolved ✅ | 0 断链 ✅ | 工具好、读者差（本项要治的） |
| P1 | `[`lin_lin_copy`](minix3/…/memory.c:lin_lin_copy)` | resolved @ :149 ✅ | BROKEN ⚠️（目标带 `:symbol`） | 可用，但要配套消噪 |
| **P1b（推荐）** | `i386/memory.c` 的 `[`lin_lin_copy`](minix3/…/memory.c:lin_lin_copy)`——**文字不带冒号** | `anchors=1 resolved=1 multi-def=0` ✅ | BROKEN ⚠️（同 P1） | 读者可见一级文件名与符号名，长度只有现在的 1/5 |
| P2 | `[`i386/memory.c:lin_lin_copy`](全路径:同)`——文字带冒号短锚 | `multi-def=1` ❌（短锚触发现行消歧判据） | BROKEN ⚠️ | 不采；除非将来真改 `anchor-resolve` |
| P3 脚注式 | `[`lin_lin_copy`][a1]` + 文末 `[a1]: minix3/…/memory.c:lin_lin_copy` | resolved ✅ | **0 断链** ✅（定义行不被当链接） | **已评估否决（2026-10-15 七篇互检后定）**，理由见下条 |
| P6 | `[`lin_lin_copy`](minix3/…/memory.c)`——目标不带符号名 | **`anchors=0`** ❌ | 0 断链 ✅ | **禁用**：门看着绿（工具自带提示"0 锚却误判干净"），实则 184 条锚点整篇蒸发 |

- **要动的工具（初稿只计一处，七篇互检后定为两处，见下下条）**：其一，`tools/notes-link-check.py` 解析前剥目标尾部 `:symbol`（与 `anchor-resolve` 已有的 `sed 's/:.*$//'` 口径一致），否则 1700 条改造会把断链扫描变成噪音。为何这算必修：CI 现况只在 `rules-consistency.yml:94` 跑它的 `--self-test`（不扫 `rewrite-notes/`），但 `review-process.md:2304` 把"断链检查"列为人工质量步骤——人工执行时满屏假断链等于该步失效，属 W-10 记过的"门静默失效"同族；它同样没有针对本形态的用例，补 self-test 时一并收正/反例。其二（互检新增）：`doc-code-map.sh` 的 token 抽取要认 `](os/**.rs)` 链接目标，否则纯路径一旦链接化就从关联代码清单里消失。
- **P3 否决的完整理由（七篇互检得出，2026-10-15）**：① id 表规模其实可接受——01 篇 184 次出现对应 **95 个唯一 `path:symbol`**（复用 1.9 次）、06 篇 183/50 个文件、12 篇 83/14，不是 1700 个 id 的量级；但② **悬空 id 两道门全盲**：实测故意写 `[`kmain`][ghost]`（无定义），`notes-link-check.py` 不计它（报的 1 条是另一行的 `:symbol` 目标）、`anchor-resolve.sh` 得 `anchors=1 resolved=1`（只数到同行真锚点），而渲染会把 `[kmain][ghost]` 原样打印给读者；③ 要把它接住就得新增"id 存在性"判据——那是**新工具能力**，比 P1b 只需把 `notes-link-check` 的解析口径向 `anchor-resolve` 已有的 `sed 's/:.*$//'` **对齐**更贵。结论：P3 不是"零工具改动"的退路，而是把盲区从一类换成另一类；本条保留作为已评估否决记录，不作降级路径。
- **七篇互检的第二处撞墙（跨 W-10/W-11 的耦合漏洞，最重要的一条）**：正文里的长路径不止带符号锚点一种。实测分布（带符号 vs 纯路径）：`01` 110/78、`02` 66/62、`03` 85/58、`06` 138/75、`12` 83/40、`13` 64/49——**纯路径引用共 362 处**（如 `minix3/etc/boot.cfg.default`、`os/kernel/src/lib.rs`），它们同样打断阅读，同样在改造射程内。但实测证明：**链接目标里的 `os/**.rs` 会被 `doc-code-map.sh` 完全漏掉**（探针 `tmp/docfmt-01/probe-lonly.md`：三个候选只回 1 条，两条链接形态蒸发）。即若只按本条初稿改 `notes-link-check`、把纯路径也链接化，就会把 W-10 刚建立的关联代码清单大面积打漏。处置（定）：**同一批里 `doc-code-map.sh` 的 token 抽取必须同时认 `](os/**.rs)` 链接目标**（正则加一条分支，与 W-10 的簇边界修复并成一笔），否则链接化范围只能限定在带符号锚点、纯路径保持反引号——那就是"问题只解一半"。工具改动清单因此从 1 处变 **2 处**：`doc-code-map.sh`（认链接目标 + 修簇边界）、`notes-link-check.py`（剥 `:symbol`）；`anchor-resolve.sh` 仍不动。
- **第三处：行号坐标形态不在改造射程**。实测 `(os|minix3)/….(c|rs|h):数字` 形态共 **259 处**（如 `minix3/…/pre_init.c:196-216`），而 `anchor-resolve.sh` 根本不抽它们（实测同一条里只数到真锚点：`anchors=1 resolved=1`）。它们属 `style-bible` 的"锚点受众分层"管辖（坐标外移 `.review/{tool}/{stage}/{doc-stem}.anchors.md` 底账），**不得混进链接化批次**：一是它们本来对锚点门透明、改了也看不见成效；二是 GitHub 要真能点得靠 `#L196-L216`，而行号会漂移，与"符号锚点是唯一不随代码漂移的定位手段"的既有裁决直接相冲。迁移脚本需把它单独排除并单独计数。
- **第四处：表格单元里的锚点需单独约定**。实测含锚表格行：`01` 36、`03` 18、`06` 14、`12` 12、`13` 10、`02` 11。表格里已经挤着路径与括号注释，再叠 `i386/memory.c` 的 `[`lin_lin_copy`](…)` 这种"前缀词 + 链接"会让单元更膨胀。约定（待规范源采纳）：**表内只写 `[`lin_lin_copy`](全路径:lin_lin_copy)`，不加一级目录前缀词**（一级目录信息已在目标里）；目标不含 `|`，故无管道符冲突。
- 渲染环境事实：`mdbook.yml` 只 build `book/`（产物路径 `./book`），`rewrite-notes/` 不走 mdBook → 读者侧是 GitHub 与本地预览器；GitHub 对 `/path` 按仓库根解析、对 `../../../path` 按当前文件目录解析，两种都能点。**CommonMark 陷阱**：直接写"文字 `[全路径]`"（无圆括号目标）是 shortcut reference link，没有同名引用定义时**原样打印成 `[...]`**，反而更脏——必须写 `[文字](目标)` 完整形式。
- 短锚保留一级目录的判据（实测 `find minix3 -name` 歧义度）：`memory.c` 4 份、`protect.c` 6 份、`main.c` 74 份，而 `pg_utils.c`/`pre_init.c`/`head.S` 各 2 份且正是 earm 与 i386 同名——本书核心论证常做 earm↔i386 对照，一级目录名（`i386/`、`earm/`、`system/`）承担真实消歧语义，**不得再往下砍到裸文件名**。
- 应用范围（用户裁决）：全部正文；**两处例外**——① §7 参见（清单章本就应给全路径，且 W-10 定的机器抽取要从 §7.2/§7.3 读全路径）；② 代码块内的保真度题注（现第 1056–1057 行那种），那是审查坐标最密处，拆成两半反而伤可 grep 性。
- 保真度标签词表（2026-10-15 定：`代码引用` / `节选` / `有改写` / `签名示意` / `反例（勿这样写）`）：
  - **先更正一处数据误读**：本条初稿说"`未删节` 库里已存在 83 行 → 改名是统一到已有词"——按"是否用在代码块题注里"重切后这句不成立：`未删节` 在题注内 **0 行**，那 82 行全是附注（`逐字引用，未删节：…`、`逐字引用（66-74 行，未删节）`）。题注内的真实分布：`逐字引用` **94**、`节选` **47**、`签名示意` 4、`教学简化` 1、`未删节` 0、`反面示例` **0**。即**库里事实上的第二类标签是 `节选`**（全文 194 行 / 14 篇，如 `/* 节选（保留与分配语义直接相关的行），minix3/…/pg_utils.c:add_memmap。 */`），而 `DOC-ITER-WORKFLOW.md:134` 已把 `节选` 列进分类清单——词表已在漂移。
  - 因此定名按"改到库内既成事实"而非"引入新词"：`逐字引用` → **`代码引用`**（需改题注 94 处）；**`节选` 原样保留**（47 处一行不动）；`教学简化` → **`有改写`**（它与 `节选` 的区分是"删了没改" vs "改了"，不可合并）；`签名示意` 保留；`未删节` 退回附注角色（R5① 本来就允许"未删节或起止符号"作措辞级补充），不升为标签。全文口径的波及面仍须一并扫：`逐字引用` 共 539 行 / 32 篇（题注外还有叙述用法，改名逐处要读句子判语义）。
  - **`反面示例` 采 C 方案（改名 + 写清边界），不摘除**：定义职责很具体（`review-doc-checklist.md:651`："教"为什么不要这样写"｜必须标 ❌ 并给出正确版本"），且它确实在同一根轴上——保真度问的是"这块码能不能跟源文件 diff"，它的答案是"不能，因为它不存在于源文件"。零使用的真相是职责被**反事实散文**（W-8 那套：`把这三个数字改成由调用者传入会怎样`）接管了，正文 16 行"反例"全是名词用法而非题注标签。所以改名 `反例（勿这样写）` 并补一句边界：**"仅用于代码并非仓库真实片段的块；反事实论证走 W-8，不用此标签"**——不写边界迟早有人在散文上错贴该标签，再被 `checks/doc.md:421` 的"分类缺失 → P2"误伤。（选 C 而非 B 的额外理由：四分类里摘掉一类后，真出现虚构反面代码块时题注无词可用，而"必须标 ❌ 并给正确版本"这条约束会无处挂靠。）
  - 关键事实（决定这是**词汇层改动而非契约破坏**）：**没有任何工具把标签字串当匹配关键字**——`grep -rn 逐字引用 tools/` 只命中 `tools/doc-snippet-extract.sh:5` 的注释；`doc-ref-check.sh` 对账的是章节引用 `§N.M`；`doc-code-density-check.sh` 的承载判据是 `^> ` 加 `.c:`/`.rs:`，均不看标签词。
  - 定义处共 **16 个文件**（`grep -rl 逐字引用 prompt/ tools/ .claude/ .codex/ .trae/ .agents/ AGENTS.md`）：规范源 `prompt/review-rules/review-doc-checklist.md`、`review-patterns.md`、`prompt/skill/review-patterns-skill.md`、`prompt/skill/review-scan/checks/{doc,patterns}.md`、`prompt/DOC-ITER-WORKFLOW.md`、`tools/doc-snippet-extract.sh`；派生端 `.claude/skills/review-scan/checks/{doc,patterns}.md`、`.codex/skills/review-patterns-skill/SKILL.md`、`.codex/skills/review-scan/checks/{doc,patterns}.md`、`.trae/skills/review-patterns-skill/SKILL.md`。历史取证文件（`prompt/REVIEW-TRACE-*`、`prompt/doc-optim-*.md`）与 R5 裁决行**不改字**（存档），只在旁加注新旧词映射。
- 落地顺序（沿用 W-10 的纪律，不能反）：① **先改两处工具**——`notes-link-check.py`（剥目标尾部 `:symbol` 再解析）与 `doc-code-map.sh`（token 抽取同时认 `](os/**.rs)` 链接目标，与 W-10 的簇边界修复并成一笔）；两者各补 self-test（正例：剥后命中真文件、链接目标里的 `.rs` 能进清单；负例：真断链仍须报 BROKEN、目录簇不得被当文件）。② 同步规范源与派生端共 16 个定义处（标签改名）+ 锚点写法条款入 `review-doc-checklist.md` 与 `DOC-ITER-WORKFLOW.md` 的锚点纪律门，跟四道派生门；③ 文档侧按 W-1 的"增量阻断 + 存量报告"逐篇迁（现测规模：01 篇正文 184 条、全 stage 正式篇约 1700 条：01 184、06 183、08 177、05 154、04 122、09 117、03 116、10 115、13 112、11 109、02 80、12 83、07 75、14 54），随 01/02/03 串行迭代推进；**行号坐标（259 处）单列不参与**。**每篇迁完必复跑 `anchor-resolve --check` 与 `doc-code-map.sh --check` 两个门并对账条数**：前者条数降即 P6 型静默丢锚，后者条数降就是纯路径链接化把关联代码清单打漏——不接受"门绿了"当交付。④ **标签改名、锚点改形、工具改动分三笔提交**：各自的 diff 要能独立回滚，混在一笔里 reviewer 无法分辨哪一半出问题。
- 本轮未做：不动 `notes-link-check.py`、不动 `doc-code-map.sh`、不动 `anchor-resolve.sh`、不动 16 个定义处、不动任何正式文档。

## 二、被活规则引用、必须留存的格式/纪律定义

### F-A `gate-evidence-*` 统一格式（原 todo_plan 0.5；`review-process.md` Step 3.6 指向本定义）

scan.md 用围栏块，名字 `gate-evidence-{type}`：

```gate-evidence-{type}
command: {实际执行的完整命令}
exit: {退出码}
stdout_key: {关键输出 3-5 行，或计数摘要}
artifact: {磁盘产物路径；无产物写 none}
date: {YYYY-MM-DD}
note: {可选：环境限制 / N/A 理由 / 工具缺口}
```

约定：type ∈ {0, A, B, C, D, D-6, E, G, H, code, unsafe, qemu, test, dep, ...}；同一 review 内同一 type 只写一个块；`command` 必须可重放原文。
另注意 Gate 0 的 9 个锚段精确段名硬编码在 `tools/verify-check.py:GATE0_ANCHORS`（`## Skill Invocation Log`、`## Blocker Gates Status`、`## Step 0: 预检结果`、`## Step 1: C Source Ground Truth Lookup`、`## Step 1.5: Coverage Enumeration`、`## Step 2: Diff Extraction`、`## Step 3.5: Precision Check`、`## Issue List`、`## Artifact Inventory`），scan.md 命名须与其一致（见 REVIEW-TRACE-pm-09-fullrun.md F-2）。

### F-B 模式库三副本同步纪律（原 EXECUTION-LOG §7；patterns.md 两处指向本定义）

| 位置 | 定位 |
|---|---|
| `prompt/review-rules/review-patterns.md` | 全量权威源（编号与条数以此为准，副本不双写计数） |
| `prompt/skill/review-patterns-skill.md` | review 时被调用的技能源（须随权威源同步补条） |
| `.claude/skills/review-scan/checks/patterns.md` | Claude 编排器按领域分片的高频子集 |

教训（2026-10-07，模式 85 漏同步事件）：**门绿不等于事对**——`generate-derived-skills.sh --check` 只比"派生 vs 技能源"，看不出"规则源改了、技能源没跟"。改权威源后必须手工同步技能源，再跑同步脚本。

### F-C 会话产物落盘纪律（原 agents-workflow-optim TODO-3，生效中；`rewrite-notes/README.md` 指向本定义）

1. 任何会话/脚本不得向仓库根写任何产物（log/bin/patch/临时 md 一律不行）；仓库根只允许配置与入口文档。
2. 落盘进 `tmp/` 功能子目录：运行日志 → `tmp/log/`；评审/工具取证 → `tmp/evidence/<日期-主题>/`；QEMU/DTB/内存转储 → `tmp/bin/`；NK4C 探针 → `tmp/nk4a/`；一次性脚本用完即删。父目录需先建。
3. `tmp/` 已整域 `.gitignore`（2026-10-07 裁决），写入物无入库风险；代价是不受版本库保护，重要取证须及时转入 `.review/` 或正式文档。

## 三、用户裁决存档（不可重演的历史决定）

| # | 议题 | 裁决（2026-10-07 批次） |
|---|---|---|
| R1 | workflow 优化轮范围 | TODO-2「抽象语义而非描述机制」为主 + 地基审计；TODO-1 只做能力落地，存量正文降噪开独立批次（→ W-1） |
| R2 | CLAUDE.md 去留 | 实测后真删：验证 Claude Code 原生加载 AGENTS.md → 独有内容并回规范源 → 改写生效引用 → 单独 commit 删除（已完成；入口以 `AGENTS.md` 为准） |
| R3 | todo_plan 21 项未闭环验收 | 逐条补跑并留证据（已完成：12 勾选、9 标明未闭环/作废理由；存活项承接于本台账） |
| R4 | 外部参考（get-shit-done、高星 Rust skill） | 借原则，不搬结构 |
| R5 | 读者体验与正确性装置的取舍（2026-10-12 批次，人审 01 篇第 1 章引出） | ① 代码块的保真度分类标签（逐字引用 / 教学简化 / 反面示例 / 签名示意）**不得因"像废话"删除**：它对读者是微小噪音，对文档正确性是可 diff 的钩子；可改进的是措辞（逐字引用块写"未删节"或起止符号，禁"及邻近字段"式含糊尾巴）。② 可读性缺口的处置：本轮**只登记开放项 W-6，不动规则源与工具**。③ 存量扫描：暂不回扫存量文档，只防新增（与 W-1"增量阻断 + 存量报告"的既有口径一致） |
| R6 | 正文锚点的可读性与保真度标签用词（2026-10-15 批次，人审 01 篇 §2.6.2 引出，承接 W-11） | ① 目标只有两项：**渲染后长路径被隐藏** + **全路径仍留存于源文本供工具与 review 使用**；**链接可点性不在射程内**。采 **P1b**：`i386/memory.c` 的 [`lin_lin_copy`](minix3/…/memory.c:lin_lin_copy)`（链接文字不带冒号，全路径锚点放目标），实测 `anchors=1 resolved=1 multi-def=0` 且 **`anchor-resolve.sh` 无需改动**；配套只需给 `notes-link-check.py` 剥目标尾部 `:symbol`。禁用形态：文字带冒号短锚（P2，触发 MULTI）、目标不带符号名（P6，`anchors=0` 静默丢锚）。**P3 脚注式已评估否决**（悬空 id 两道门全盲，接住它需新增工具能力，比 P1b 的口径对齐更贵）。七篇互检后另补两条射程边界：**纯路径引用（362 处）也在射程内，但 `doc-code-map.sh` 认不到链接目标里的 `os/**.rs`**（实测三个候选只回 1 条）→ 本项工具改动仍为 **2 处**（`doc-code-map.sh` 认链接目标 + `notes-link-check.py` 剥符号），`anchor-resolve.sh` 不动；**行号坐标（259 处 `path.c:196-216` 型）单列不参与**（对锚点门透明、且 GitHub 行锚会漂移，属坐标外移底账的射程）。② 短锚**保留一级目录且不带冒号**（承担 earm/i386 与 `system/` 的真实消歧），不砍到裸文件名。③ 适用范围 = 全部正文；例外 = §7 参见与代码块内题注。④ 保真度标签词表定名 `代码引用` / `节选`（原样保留）/ `有改写` / `签名示意` / `反例（勿这样写）`，并注明 `未删节` 退回附注角色（七篇互检发现它在题注内 **0 处**，先前"库里已有 83 行先例"的说法只成立到全文口径）；`反面示例` 采**改名 + 写清与 W-8 反事实段的边界**而不摘除（保真度轴上它仍自洽，摘掉后真出现虚构代码块时"标 ❌ 并给正确版本"这条约束会无处挂靠）。⑤ 本轮**仅登记 W-11，不实施**；标签改名、锚点改形、工具改动分三笔提交（各自可独立回滚） |
| R7 | 关联代码清单的位置与形态（2026-10-15 批次，人审 01 篇头部与 §7 引出，承接 W-10） | ① 意图：同一份源清单不在头部与 §7 各抄一遍，**收拢到 §7**；**头部不留指针、不再有任何源码清单**（同日二次裁决：头部要清爽，由工具去适应文档形态，不是反过来）。② 前提：`doc-code-map.sh` 的 `head -30` / `head -1` 只是当前实现，**允许改抽取逻辑**，但顺序必须是"改工具 → 补 self-test 与 CI 腿 → 改规范源 → 最后动文档"，且**该改的一次改全**，不允许再出现规则源落后于工具。③ 同目录源文件收拢采"≥3 个用花括号簇、1–2 个写全路径、正文散文禁用簇"；另补硬约束：簇末位以共享扩展名收尾，混合后缀拆两条（原形 `{check-layout.sh,src/main.rs}` 与 `{proc.rs,lib.rs}` 现均整条抽不到，见 W-10 第 3 条）。③b §7 小节改用语义名「关联 C 源码清单」/「关联 Rust 源码清单」（全树 0 命中、适合当 grep 键），层级必须 `###`。④ §7 渲染可读（含改可点击链接）优先级**低于**合同字段的正确性维护；§7 不补每文件说明。⑤ 本轮**仅登记 W-10，不实施**；实施前需计划，由用户对 01、02、03 三篇做串行迭代（每轮检测上轮结果与轨迹）以确保意图完整落地。⑥ 七篇互检后补的硬工序（2026-10-15）：**"删头部零丢失"不是普遍性质**（仅 01 篇成立，因它是唯一把 §7 写成完整清单的篇目）——其余六篇头部有 §7 与正文均无的独占项（按 `doc-code-map.sh` 同口径的真丢数：`02` 9 条、`03` 7 条、`05` 3 条、`12` 3 条、`06` 2 条、`13` 2 条，`01` 0 条），所以删头部前必须先把头部独占项全量并入 §7 关联清单，并用 `doc-code-map.sh --check --format=plain | wc -l` **对账迁移前后条数不降** |

## 四、review 流程实证检验索引（本台账的"过程正常完备且正确"证据链）

- 第一轮：`prompt/REVIEW-TRACE-vfs-08-dryrun.md`（vfs/08 干跑，流程侧修复 F5-F8 的来源）。
- 第二轮：`prompt/REVIEW-TRACE-pm-09-fullrun.md`（pm/09 全量实跑，含 Gate A 工具阻断→修复→重跑、Gate G 机检 FAIL→重组→跨 session 独立验证 7/7 的完整轨迹；流程空档 F-1~F-8 待据此修订规范源）。
- 第三轮：01 篇 CH1-CH2 人工审阅（2026-10-12；本轮无独立 REVIEW-TRACE 文件，过程记录就在本台账的 W-6、W-7 与 R5，正文改动落在 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`、`02-higher-half-kernel.md` 与 `rewrite-notes/08-stage-is/04-is-data-acquisition.md`）。本轮的特征是：**八处"读不懂 / 讲不通"全部由人发现，工具零拦截**——这就是 W-6 要补执行装置的依据。
- 前两轮均收敛；F-2/F-3/F-4/F-5 四条建议改规范源时，按 `prompt/README.md` 三端同步流程执行。
