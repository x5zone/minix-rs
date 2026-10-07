# TODO：正式文档"读者体验优先" + 技能"抽象语义而非描述机制"再强化 + 会话产物零根目录污染 + CLAUDE.md 去留

> 类型：agents 工作流优化 TODO（现含四条：TODO-1 读者体验降噪；TODO-2 设计原则入技能；TODO-3 会话产物零根目录污染；TODO-4 CLAUDE.md 去留裁决，见文末）。模式：本文件是**待办计划**，不含对 `notes/` / `AGENTS.md` / `prompt/` 的任何实际改动。
> 触发源：TODO-1 = 2026-10-06 阅读 `03-stage-rs/01-rs-boot-init.md` 时，正文里过密的数字锚点严重拖累阅读体验。TODO-2 = 2026-10-07 「三架构待决项清单」六轮讨论中，网页端对设计原则的再表述比仓内现有规则更成文、更可判定（见 TODO-2 节）。TODO-3/TODO-4 = 2026-10-07 git 清理线根目录清场时登记（26 个会话期裸 log 污染根目录；用户提议评估删除 CLAUDE.md）。

---

## 0. TL;DR（唯一主目标）

正式文档是写给**读者**看的，不是写给**审查**方便看的。复刻代码行尾那些 `/* 33 */`、`/* 16: 进程标志 */`、`/* 455 — utility.c:44 */`、`/* 13 */`（"详见第 13 篇"）——本质都是**给 agent 审查时回 `minix3/` 真源核对用的坐标**（for-agent）。它们该被**隐藏进 `.review/` 的结构化文档**，正式正文只保留读者真正需要的语义讲解。一句话：**读者体验第一，审查锚点让路。**

---

## 1. 状态与前置条件（阻塞项）

- **状态：`BLOCKED`（前置未满足，禁止开工）**
- **阻塞项（唯一硬前置）**：本 TODO 必须等到 **`tmp/migrate_notes_plan/` 目录下的 notes 迁移计划全部执行完毕并稳定**之后，才允许解锁执行。
- **为什么必须先迁移、后降噪**：
  1. notes 迁移会**重排目录结构**（去掉 `fork-syscall-rewrite/` 这一 `{module}` 层、把 `notes/rewrite/...` 提到顶层——见 `tmp/migrate_notes_plan/migrate_qwen.md` §1/§2）。若先动 `01-rs-boot-init.md` 这类文件做锚点降噪，紧接着它就要被 `git mv`，改动全冲在移动里、review 噪声翻倍。
  2. 本 TODO 里出现的所有 `path:line`（含 `01-rs-boot-init.md`、`06-todo.md:688` 等）在迁移后**会失效**。因此：

- **对"自包含"的强制要求**：本 TODO 引用的每一个反例，都必须**把被批评的原文那一行整段抄进来**（如下 §2 所示），不能只给一个 `文件名:行号` 让读者去跳——因为那个文件很可能已被挪动甚至重写。抄原文 + 说明"它为什么伤读者"，才能让这份 TODO 在迁移后依然可执行。
- **解锁条件**：满足以下全部 → 状态改 `READY`：① `migrate_notes_plan` 各 Phase 验收门通过；② notes 新目录结构下的文件路径稳定（迁移后至少 1 轮无路径变更）。

---

## 2. 现象与跨文档证据（三类伤读者体验的锚点，均自包含）

> 量化底数（2026-10-06 实测，`grep -c '/\* [0-9]' notes/rewrite/fork-syscall-rewrite/**/*.md`）：含行尾数字锚的正式文档 Top —
> `02-rs-process-table.md` **158**、`01-rs-boot-init.md` **107**、`09-rs-exec.md` **50**、`05-rs-ipc-sendmask.md` **40**、`03-rs-privilege.md` **36**、`08-rs-slot-config.md` **29**、`05-stage-vfs/02-fproc-struct.md` **24**、`02-stage-vm/01-vm-init-main.md` **23**…（仅复刻 C 代码的 `/* N */` 形式全目录就 277 处）。**这不是 01 一篇的个案，是 RS/VM/VFS 全线通病。**

### 反例①【焊死式】源行号焊进讲解注释里 → 复现 `05-stage-vfs/02-fproc-struct.md`
原文（整段抄录，迁移后照此定位）：
```c
  unsigned fp_flags;               /* 16: 进程标志 */
  pid_t fp_pid;                    /* 18: 进程 ID（PID_FREE=0 表示槽空闲） */
  endpoint_t fp_endpoint;          /* 19: 内核 endpoint（NONE 表示槽空闲） */
```
**伤在哪**：`16` / `18` / `19` 是这些字段在 C 头文件里的**源行号**，对读者毫无意义，却和真正的讲解（"进程标志""PID_FREE=0 表示槽空闲"）挤进同一对 `/* */`，读者得先在心里把数字剔除才能读到解释。**行号该走 `.review/`，`进程标志` 这类讲解该留。**

### 反例②【冗余尾锚式】已有语义注释、还再补一个裸行号 → 复现 `03-stage-rs/02-rs-process-table.md`
原文：
```c
EXTERN struct rprocpub rprocpub[NR_SYS_PROCS];  /* public entries */   /* 33 */
EXTERN struct rproc rproc[NR_SYS_PROCS];                                /* 34 */
EXTERN struct rproc *rproc_ptr[NR_PROCS];       /* mapping for fast access */ /* 35 */
```
**伤在哪**：每行末尾孤零零一个 `/* 33 */`、`/* 34 */`、`/* 35 */`——纯坐标、零讲解。整张全局变量表读下来像在看地址清单，而不像"这些变量各管什么"。

### 反例③【跨文件坐标式】行号 + 又一处 `文件:行号` → 复现 `02-stage-vm/01-vm-init-main.md`
原文：
```c
get_mem_chunks(mem_chunks);    /* 455 — utility.c:44 */
memset(vmproc, 0, sizeof(vmproc));                        /* 458 */
```
**伤在哪**：一行里同时挂"本文件第 455 行""在 utility.c 第 44 行"两个坐标。审查有用，读者只需知道"这行在取内存块"。

### 反例④【语义歧义式】裸数字到底是行号还是文档号，读者猜不出 → 复现 `03-stage-rs/01-rs-boot-init.md` §2.1 dispatch 块
原文（本次会话前形态）：
```c
	  case RS_UP:		result = do_up(&m);		break;   /* 13 */
          case RS_UPDATE: 	result = do_update(&m); 	break;   /* 16 */
	  case RS_INIT: 	result = do_init_ready(&m); 	break;   /* 12 */
              result = ENOSYS;                            /* 121 */
```
**伤在哪**：`/* 13 */` 指"详见第 13 篇"，而同块的 `/* 121 */` 是 `main.c` 真行号（`case RS_UP` 实际在 `main.c:102`，可证 13 非行号）。**同一写法两义**，读者完全无法分辨——本会话即靠回真源 grep 才识破。

> 反模式早有定论：`01-stage-kernel/06-todo.md:688` 原话——"code block + 行号 + 三段式"让文档"退化为**行号说明书**，读者体验从'学习操作系统为什么这样设计'变成'阅读本仓库 API 文档'"。**注意：该 `06-todo.md` 路径迁移后会变，故上面整段引文即为自包含凭据。**

---

## 3. 原则（受众分层）

> - **正式文档**（迁移后的 notes 主体，最终进 `book/`）→ **读者体验第一**。复刻代码只写"这行在干什么、为什么"；坐标类噪声能少则少，最多留一个**块级区间题注**（如"（本节复刻 `rs/main.c:38-131`）"）兜底可追溯。
> - **`.review/{module}/` 结构化文档** → **agent 审查载体**。逐行"正文行 ↔ C 真源 `path:line` ↔ 机制归属篇"的映射表放这里，供 review / 覆盖核对用。`AGENTS.md` 已把 `.review/` 一类目录定为"中间产物、正式文档绝不引用"，与本原则天然一致。

---

## 4. 实施步骤

### 步骤 0 —— 实施前调研（本 TODO 明确要求，先调研再动手）
解锁为 `READY` 后、正式批量降噪**之前**，必须先跨文档调研"偏审查、不面向读者"的优化点，产出一份清单（写入 `.review/reader-first-survey/`）。除已列的四类锚点外，**至少再排查以下候选**：
1. 工具派生的 `（Lnn，工具生成）` 是否也过密、可否同样外移。
2. "字段逐一开小节"的**进程表/结构体百科**（`06-todo.md:688` 点名的"运行时字段百科"）——能否合并成表 + 只给关键字段配讲解。
3. 满屏的 `file:///` 绝对路径链接墙、`path.c:func` 符号锚点密度。
4. 开发文档味标题（"Step 1 / Step 2"、"§x.y.z 字段说明"）、压缩简写、黑话（style-bible 已禁，但需实测复查）。
5. 用 `grep -c '/\* [0-9]' ` 与上面的 Top 排序作为"读者噪声热区"底数，每篇给出"降噪前密度 → 目标密度"。
> **调研产出 = 一份可勾选的 issue 清单**，逐条带**自包含原文行**（同 §2 抄录法），不依赖迁移后可能失效的 `path:line`。

### 步骤 1 —— 正文侧降噪（逐篇，Top 密度优先）
- 删除/降级复刻代码的逐行 `/* NN */`；`反例①` 那种焊死的，把行号摘出、只留中文讲解；`反例③` 那种跨文件坐标，整条外移。
- 真源可追溯靠**块级区间题注**一处兜底（`（复刻 main.c:38-131）`），不逐行。
- 拿不准的语义宁缺毋滥、不臆造（如 `do_fi` 的确切含义未核实就不写）。

### 步骤 2 —— `.review/` 侧承接
- 每模块建一份锚点底账（如 `.review/{module}/anchors.md`）：`正文锚点 → C path:line → 机制归属篇`，把步骤 1 摘出的坐标**原样搬进来**，审查时可追溯性一分不丢。

### 步骤 3 —— 固化成规范（后续动作）
- `style-bible`：加硬裁决"读者体验 > 审查便利；for-agent 坐标/归属锚点入 `.review/`，正式文档只留读者所需最小锚"。
- `review-doc-checklist.md` / `review-patterns.md`（模式 83 锚点族）：加"正文逐行坐标锚 = 可读性回归/卫生项"判据 + "锚点受众分层"子条。
- `tools/anchor-migrate.sh`：加"把代码块内坐标锚**迁出到 `.review/` 映射表**"模式，而非只在原地改写。

---

## 5. 验收门

- **读者门**：只读正式文档、从不点 `.review/` 的读者，能顺畅读懂每段代码——不再有任何数字让人猜"这是标号吗？"。
- **审查门**：可追溯性不减——每个被移出正文的坐标，在对应 `.review/{module}/anchors.md` 有行可查。抽查法：随机挑一句位置/序数断言，15 秒内在 `minix3/` grep 到支撑它的 C 行。
- **回归门**：`grep -c '/\* [0-9]'` 每篇密度显著下降至步骤 0 设定的目标；`book/` 渲染稿无 `.review/` 断链。

---

## 6. 边界与注意

1. **前置阻塞不可跳过**：`migrate_notes_plan` 未完工 → 本 TODO 保持 `BLOCKED`，不得提前动 notes（避免与目录 `git mv` 撞车、避免在会失效的路径上做无用功）。
2. **`.review/` 当前未被 git 跟踪**（`migrate_qwen.md` 实测 `.gitignore`）：若锚点底账只放 `.review/`，它是本地/会话级、不随仓库走。需裁决：接受"审查辅助本就本地"→ 保持；若要版本化 → 另设一个被跟踪的锚点产物位（本 TODO 不擅断，列为决策点 D-anchor-persistence）。
3. **搬位置、非砍可追溯**：反对的是 for-agent 噪声霸占读者版面，绝不删掉真源映射。
4. 本会话顺带出现的"并发提交要现采 `git status`""智能问答↔智能体模式切换"等，是**另一类**流程问题，与本 TODO 主目标无关，不在此展开。

---

# TODO-2：技能再强化——把"抽象语义、不描述机制"从口头原则变成可判定规则

> 类型：agents 技能与规则源优化（文档面 + 代码卓越度面都适用）。
> 来源：`notes/rewrite/fork-syscall-rewrite/PENDING-DECISIONS-3ARCH-PARITY.md`（迁移后路径会变，故下文按本文件惯例**整段抄录关键原文**，链接仅作便查）§七.7 PD-33「裁决理由·网页端对原则的再表述」段与 §8.4 合规检查表。

## T2.0 TL;DR

仓内技能（`code-excellence`、`full-review`、`style-fix` 及其规则源 `prompt/review-rules/`）**强调过这条原则但没有把它写成可判定的规则**：实测 grep（本会话，`prompt/`、`.claude/rules/`、`.agents/skills/` 全量）对「抽象语义」「语义对象≠承载对象」「trait = what」类表述**零命中**；最接近的只有 `review-patterns.md` 模式 79（trait 成员由能力差异而非调用时序决定）和 `review-doc-checklist.md` 的划分维度子句——它们是本原则的**局部推论**，不是原则本身，也没有给出遇到新型架构差异时的判据。后果：同一原则每次都要在会话里从头推导（本次六轮讨论就是在重新发明它），技能不会主动拿它去拦「把三架构写成一样」的伪统一冲动。本 TODO 把它入规则源、入检查项、入反例库。

## T2.1 要固化的原则原文（自包含抄录，网页端第三/四轮复核）

> 「这套原则已经不再是『Rust 封装硬件』的普通硬件抽象层（HAL）思路，而是在明确区分『**OS 语义对象**』与『**硬件承载对象**』。aarch64 的双页表根、不同的浮点状态形态、不同的错页地址寄存器、不同的系统调用返回寄存器、甚至不同的启动契约，都可以用同一套原则自然解释，而不需要一项项打补丁。」
>
> 「优美不是让三个实现的代码看起来一致；而是**同一层的代码拥有同一语义；不同层的差异待在该待的地方**。真正优美的抽象通常不是把所有差异塞进 trait，而是把差异留在实现里，让接口根本看不见它。」
>
> 示范案例定稿措辞：「**ARM 的两个页表根是硬件事实；Minix 的地址空间才是 OS 语义。正确的抽象不是把两个根伪装成一个根，而是根本不把『根的数量』作为 OS 语义。**」
>
> 配套三问（遇任何架构差异先问）：① 这是 OS 关心的语义，还是硬件实现事实？② 把硬件名词全部删掉，还能说清这个接口是什么吗？③ 有没有两个硬件实现共同满足同一个 OS 语义（有→这正是 trait 存在的理由）？
>
> 边界一条：启动契约（boot contract）不在抽象射程内——它是平台把系统送入 OS 的方式，不是 OS 运行时机制，强行统一反而违反原则（实证：同一清单 PD-07 裁「不统一」、PD-14 裁「自引导定终态」均由此得）。

本次会话的可引用实例（原则反过来解释了已有裁决，而非新发明）：PD-17（OS 层只见 `CpuContextArch` 的状态车道，x86_64→R10、aarch64→r1、riscv64→a1 是实现选择）、PD-05（`FpuArch` 各存本架构形态，无人强迫 aarch64 伪装成 x86 的浮点寄存器布局）、PD-25（合并判据从「同一张 C 回调表」升级为「同一个 OS 语义契约」）。反方向拦截同样成立：PD-18 撤掉了「终局用不透明票据」的预防性抽象——原则不只禁止「把两根伪造成一根」，也禁止「为漂亮提前改契约」。

## T2.2 强化点（规则源 `prompt/` 为唯一真相，改完按 `prompt/README.md` 同步派生目录并跑 `tools/check-review-rules.sh`）

1. **`prompt/review-rules/review-patterns.md` 架构抽象族（模式 79–84）新增一条总括模式**（建议编号 85 后的下一空闲号）：「机制描述伪装成语义抽象 / 为消除架构差异制造伪统一」——可验证判据 = 上面配套三问逐问作答；典型命中面 = trait 方法名或参数里出现寄存器名/页表根数量/指令名且其调用方不是架构边界层（注：arch trait **自身**可以谈硬件名，禁止的是泄漏进 OS 语义层——此边界措辞取自 PD-33 公理 1 第三轮收紧版，不可省略）。
2. **`code-excellence` 技能多方案比选模板加一项强制检查**：每个涉及跨架构接口的设计方案，除现有「≥2 方案 + 对照 Redox/Linux/OS 理论」外，必须回答配套三问并在设计对比表里单列一栏「统一的是语义还是形状」；答案为后者 → 登记为设计类 P1。
3. **`review-doc-checklist.md` / `style-bible` 文档面**：叙述架构差异时，禁止写成「为兼容某架构做的特殊处理」；先述 OS 语义对象、再述各硬件承载形态（示范句即 T2.1 第三段）；同时明确文本级「硬件名词 grep」不作守卫手段（注释/文档中解释实现不算泄漏，主守卫 = 依赖边界）——避免强化时把原则念成新的教条。
4. **守卫禁面的收窄定义（待决项清单第七轮定稿措辞，规则建设必须照此写，不可回退成「不得 import 架构专属符号」的过宽版）**：OS 语义层**允许依赖架构边界接口（仓内 `*Arch` trait）**；禁止的是直接依赖具体架构实现模块、具体架构类型、CSR/汇编原语与 `x86_64`/`aarch64`/`riscv64` 实现符号——否则守卫会误杀四层模型自己设计的合法依赖关系。**建设前先裁守卫射程**：第七轮按此定义实测，用户态已有 4 处直接导入具体架构实现（`os/servers/vm/src/vmdm_bridge.rs` 三处 + `alloc_page.rs` 一处，均调 `minix_arch::riscv64::paging::vmdm::*`），kernel 通用层另有约 65 处子模块导入——豁免形态三选一（VM 定性为边界层成员 / 收进 `*Arch` 接口 / 白名单定期回看）尚未裁，已作为重构待办登记在待决项清单 §七.7 PD-33「冻结前合规实测」段，本 TODO 建设规则时引用该段即可，不另起炉灶。
5. **与 TODO-1 的关系**：TODO-2 改的是 `prompt/` + 技能文件，**不依赖 notes 迁移**，可先于/并行 TODO-1 执行；但第 3 点若同时涉及 notes 正文叙述修正，那部分遵守 TODO-1 的 BLOCKED 前置。

## T2.3 验收门

- **grep 门**：原则关键词（如「承载」「伪统一」「语义对象」）在规则源从 0 命中变为有定义条目；新模式的验证命令可复跑。
- **拦截回归门**：拿本次两个实例当回归样本——① aarch64 双根若被抽象成 `PageTableRoot{fn root()}` 式假单根，新规则应能在 review 中命中；② PD-18 式的「为未来预裁终局」应被同一规则的边界条拦截（反向误伤测试：规则不应判 PD-18 维持冻结为违规）。
- **同步门**：`prompt/` 源变更后按既有命令同步 `.claude/` `.codex/` `.trae/` `.agents/` 派生副本，`tools/check-review-rules.sh` 通过。

---

## TODO-3：会话产物零根目录污染（2026-10-07 git 清理线登记）

> 状态：`READY`（无前置，可随时执行/固化）。触发源：2026-10-07 根目录清场发现 26 个会话期裸 log（`.e3*.log`、`.e4*.log`、`.t29~.t36*.log`、`.hs*.log`、`.gv16.log`，2026-09-07/08 写入，已删）与 QEMU/DTB 产物（`t2.bin`、`dtbfull_a64.bin`、`dtbram_a64.bin`、`vmtext3.bin`、`vmtext_gh.bin`，NK4C aarch64 实验 10-02~10-06 生成；初判留待 NK4C 确认，用户复核后裁决同为脏文件——零脚本引用，已删）。

### 现象取证

- 全仓 tracked 工具脚本 grep 这些文件名**零命中**——写入者不是仓内脚本，是当时会话在仓库根手敲的 `命令 > .xxx.log` 式重定向。
- 用户裁决：根目录不允许任何此类污染；产物一律进 `tmp/` 按功能划分的子目录。

### 规则（写进各线会话纪律；后续可固化进 review-cmds/code-excellence）

1. 任何会话/脚本**不得向仓库根写任何产物**（log、bin、patch、临时 md 一律不行）。
2. 落盘位置按用途进 `tmp/` 功能子目录：串口/运行日志 → `tmp/log/`；评审/工具运行证据 → `tmp/evidence/<日期-主题>/`（原顶层 `tmp/evidence/` 已并入此处，2026-10-07）；QEMU/DTB/内存转储等二进制产物 → `tmp/bin/`（2026-10-07 增，原堆积根目录的 5 个 .bin 已清）；NK4C 取证脚本与探针 → `tmp/nk4a/`；一次性脚本 → `tmp/` 平铺，用完即删。
3. `tmp/` 已整域 `.gitignore`（2026-10-07 裁决），写入物永无入库风险；仓库根只允许存在配置与入口文档（`.gitignore`/`AGENTS.md`/`CLAUDE.md`/`README.md`/`LICENSE`/`opencode.json`/`.qoderignore`）。

### 验收

- `ls -A` 仓库根除白名单入口外无散文件；`git status` 无根目录 untracked 噪声。
- 抽查任意会话 WORKLOG：产物路径全部在 `tmp/` 下。

---

## TODO-4：CLAUDE.md 去留裁决（2026-10-07 git 清理线登记，依赖图已查清）

> 状态：`BLOCKED`（唯一阻塞项 = 最新版 Claude Code 对 `AGENTS.md` 的原生加载行为未经实测）。触发源：用户提议 CLAUDE.md 使用低频、最新版 Claude 或可直接依赖 AGENTS.md，考虑删除。

### 依赖图（2026-10-07 `git grep -l 'CLAUDE\.md'` 实测）

- `prompt/README.md`：三运行时同源架构——`prompt/` 是唯一真相源，`CLAUDE.md` = Claude Code 适配器（项目根 + `.claude/` 自动加载），`AGENTS.md` = Codex CLI 适配器（项目根 + `.codex/` 自动加载），`.trae/skills` = Trae 适配器。三者内容同源、各自适配加载机制。
- `AGENTS.md` 开头声明"完整规范在 CLAUDE.md……本文件是其 Codex 入口"——两文件是**互指的派生对**，删除 CLAUDE.md 不是删冗余副本，是拆掉三向同步契约的一条腿。
- 生效引用（删前必须改写）：`AGENTS.md` 前言、`prompt/README.md` 架构描述、`.claude/rules/review-core.md`、`.claude/skills/` 与 `.trae/skills/` 内的 review-patterns-skill。
- 历史引用（不必改）：`notes/` 十余处、`tmp/migrate_notes_plan/` 六份计划书均为当时记录。

### 建议路径（每步可独立回退）

① 实测最新 Claude Code 是否原生加载 `AGENTS.md`（开一个空会话问它"你加载了哪个项目指令文件"即可验证）→ ② 若可加载：按 `prompt/README.md` 的同步命令把 CLAUDE.md 的独有内容并回 `prompt/` 源 + 改写三处生效引用为"以 AGENTS.md 为唯一入口" → ③ `tools/check-review-rules.sh` 过门 → ④ 删 `CLAUDE.md` 单独成 commit。② 若实测不可加载：保留 CLAUDE.md，本 TODO 关闭并注明原因。

### 验收

- Claude Code 空会话能正确加载项目指令（内容与原 CLAUDE.md 同源）；`tools/check-review-rules.sh` 通过；`git grep 'CLAUDE\.md'` 仅剩历史记录类命中。

---

## 补充（notes 目录迁移之后）：落盘规则的两句收紧 + 一条待办

### 收紧一：文档随附的取证证据也只进 `tmp/evidence/`

上文第 173 行的落盘规则此前只约束「会话产物」，没回答一个历史遗留问题：
一批被文档正文引用的串口日志与内存转储，早先躺在笔记树里（当时叫 `evidence/`），
与规则要求的 `tmp/evidence/<日期-主题>/` 形成两处并存——实测同一个 `pattern-gate` 系列被拆成两段：
c61～c63 在树内、c64～c68 在 `tmp/evidence/`。现已整体并入 `tmp/evidence/`（26 个子目录，零命名冲突），
树内文档引用取证一律写 `tmp/evidence/<日期-主题>/...`。

规则补一句：**判断落盘位置看的是产物性质，不是「有没有文档引用它」**。被引用不是留在树内的理由——
引用路径改掉即可；反之，需要长期保真、要能靠版本库证明没被改过的东西（设计文档、案卷正文、台账）才留树内。
这条判断的代价已经写在 `rewrite-notes/README.md`：那 1.28 GB 日志从此只有磁盘单份拷贝。

### 收紧二：目录类内容的归属跟着性质走

同批清理里定下的三条，供以后判断「某个目录该放哪」：

- 一次性任务的计划与留档，使命结束后进 `tmp/`（本次把迁移工作区 `migrate_notes_plan/` 与换机记录
  `new_laptop_migrate/` 都并了进去），不再占据仓库顶层；仍入 git 的历史留在提交里可 `git show`。
- 规范类文档进 `prompt/`：`agents-workflow-optim.md` 本身就是工作流规则，此前散在顶层任务目录里。
- 单一脚本不单设顶层目录：原 `scripts/update_minix.sh` 移入 `tools/`，`scripts/` 目录取消。

### 待办（状态 `TODO`，可随时执行，建议单独一批）：`tools/` 需要按功能分类

现状 40 个顶层条目混着五类东西：评审流程门（`check-review-rules.sh`、`lint-review-rules.sh`、
`review-*.sh`、`diff-trae-skills.sh`、`verify-check.py`、`review-state-validate.py`）、
锚点与基线（`anchor-*.sh` 与三份 `*-baseline.txt`）、覆盖率提取（`coverage-extract/`）、
atf/构建链（`atf-c-compat/`、`build-*.sh`、`vendor/`、`vendor-atf-toolkit*`）、
一次性扫描与分析脚本（`address-constant-scan.py`、`ns12_bin_transform.py`、`gen-*.py`、`pattern-gate.sh`）。

建议分法与注意点（本轮**不动**，避免和迁移的 tools 改动混在一起）：

1. 分子目录会牵动三类引用：`.github/workflows/*` 的调用路径、`prompt/` 与 `.claude/.codex/.trae`
   规则文本里的命令示例、基线文件里写死的相对路径（`tools/anchor-suspect-baseline.txt` 头部注释即为一例）。
   所以必须与「三端派生同步」同批做，并跑 `tools/check-review-rules.sh` 与派生 `--check` 收尾。
2. 基线文本（`*.txt`）与脚本分开放：基线是数据，不是可执行入口，混在一起会让人误当脚本调用。
3. 先清理再分类：`tools/__pycache__/`、`tools/clippy_base.txt`（本仓另有 `tools/*-baseline.txt` 命名惯例）
   这类命名与放置不一致的东西，应在分类前先统一。


---

## 补充：批量 add 之后必须跑 `tools/check-staged-inflight.sh`

四轮同因的误提交换来的硬检查（notes 目录迁移期间，每次对 `rewrite-notes/` 这类含维护者在制文件的目录
跑 `git add -A` 或 `git add -u`，都把未提交的编辑一起暂存了）。规则固化为一条命令：

```bash
git add -A -- <路径>            # 批量暂存
tools/check-staged-inflight.sh  # 断言：登记的在制文件不在暂存区里，非 0 就拒绝提交
git commit ...
```

在制清单维护在脚本的 `INFLIGHT` 数组（`--list` 可看当前登记项）。抓到时会直接打印退回命令——
把索引退回 HEAD 的内容而保留工作树编辑，不需要 `git restore`（那会连编辑一起丢掉）。
适用场景不止迁移：任何人接手一个大范围改动、而工作树里同时存在别人的未提交编辑时都该跑。
