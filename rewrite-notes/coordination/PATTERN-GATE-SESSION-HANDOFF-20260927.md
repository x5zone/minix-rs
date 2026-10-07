# PATTERN-GATE SESSION HANDOFF（2026-09-30 写，供新电脑无缝恢复）

> **用途**：本机迁移，旧电脑上的会话记忆（含 ZCode memory）不会跟随。本文件是 pattern-gate 工作
> 线（C-61/C-62/C-63 三轮）的**全量会话交接记录**，随 git 走到新电脑。配合文末「恢复 prompt」使用。

---

## 1. 任务是什么

用户指令（原话要点）：「从 FIXLOG、WORKLOG、REVIEWLOG 等历史 LOG 中扫描并发掘模式，扫描得到测试，
避免以后再出现类似的问题。扫描需要仔细仔细再仔细。测试代码以 worktree 模式加入，避免影响别的线程开发。」

后续追加指令：①这是**增量任务**（日志持续增长，需定期重扫）；②语料补充 `.zcode/plans/` 等会话产物；
③迁移前完成小节点终止，写本交接文件 + 恢复 prompt。

**已交付物**：`tools/pattern-gate.sh`（15 项机械检查 P1-P15）+ `tools/pattern-gate-baseline.txt`
（存量豁免）+ `rewrite-notes/coordination/PATTERN-SCAN-REPORT-20260923.md`（模式目录 +
§7/§8 增量节）+ `tmp/evidence/2026092{3,7}-c6{1,2,3}-pattern-gate/`（判别证据）。**全部已入 git。**

## 2. 三轮交付记录（commit 均已合入 rewrite）

| 轮 | claim | 交付 commit | 合并 | 销账 | 内容 |
|---|---|---|---|---|---|
| C-61 首轮 | claim/C-61-zcode-glm | 3643c9331 | merge 1cfc59c4e | 0123763ec | gate P1-P13 + 基线 21 keys + 模式目录（六族 34 模式）+ selftest 18 例 + 真树变异 6 组 |
| C-62 增量轮 | claim/C-62-zcode-glm | 5aa2d74f0 + 378a52370 | merge 0859580eb | f7b682890 | NK4-C 274 笔增量扫描；P14（RTS 裸操作对账）+ P15（vendored 防线）+ P12 修正 + P2_TESTS +7；**顺手执行 NK4C R3 报告 §7.3 待办 1**（os/.dockercargo 158 文件解除跟踪，378a52370 是补笔，见 §6 教训①） |
| C-63 补扫轮 | claim/C-63-zcode-glm | 162b1cd07 | FF 入 rewrite | b01b0a572 | 用户点名缺口闭合：.review/{claude,codex,trae} 1689 md 提取 52 条 + .zcode/plans 35 文件 60+ 条 + NK4C-WORKLOG 正文 53 条；P2_TESTS +1；报告 §8；~/.zcode/cli 划界不扫 |

**工作流程（三轮同构，新电脑续做照此）**：`grep new_edge4.md §2 尾部取空号 → tools/claim.sh claim
C-NN zcode-glm → cd .wt/ 专属树 → 登记 §2 一行并 commit → 树内改 → 三绿验证（selftest / full /
--diff）+ 真树负例变异 + doc-style-lint --diff 0 error → git commit --only <显式文件> → 主树 merge →
主树终验 → §2 销账 → claim.sh release`。

## 3. 系统现状快照（2026-09-30 02:35 实测）

- **rewrite @ f7739a18f**（NK4-C 续跑线仍在活跃推进，续-66~73 含真修复——**主树可能有其未提交在制，
  见到脏文件不要动**）。pattern-gate 三轮交付均在此历史中，`git log --all --grep="C-61\|C-62\|C-63"` 可查。
- **claim 全部释放**（`tools/claim.sh list` 为空）。
- **gate 检查清单（15 项）**：P1 FIXLOG 完整性 / P2 防回归测试存在性（**28 个名字**）/ P3 权威常量派生 /
  P4 os-tests [[test]] 对账 / P5 冒烟 marker↔emit / P6 CI 跨架构 build 门 / P7 退出码吞没 / P8 CRLF /
  P9 会话产物防线 / P10 FIXLOG 序号裸引（report+diff 门）/ P11 C 锚点（diff 门）/ P12 共享路径 asm
  架构门 / P13 虚构符号（diff 门）/ P14 RTS 裸操作 / P15 vendored 目录防线。P7/P8/P12/P14 走基线对账。
- **基线 118 keys**（P7×3 + P8×6 + P12×3 + P14×106）。注：旧汇报里的「222/219」是中间轮含重复计数，
  以实测 118 为准。
- **判别证据**：selftest 24 例全绿；真树负例变异（P3/P4/P5/P6/P7/P14/P15 注入违规全现形）；
  `tmp/evidence/2026092{3,7}-c6{1,2,3}-pattern-gate/` 三套日志。
- **运行**：`bash tools/pattern-gate.sh`（全量）/ `--diff [RANGE]`（增量门）/ `--update-baseline` /
  `--self-test`。退出码 0/1/2。P1 仅主树有意义（.review 不入 git，worktree 下 SKIP 属预期）。

## 4. 模式目录（详见 PATTERN-SCAN-REPORT §2/§7/§8）

**六族**：A 账面（FIXLOG 膨胀/撞号/失真/虚构符号/锚点漂移）；B wire（化石常量/车道错位/符号域/u32
截断/canned 恒绿/宿主不可达/布局算术）；C 测试有效性（断言虚构/空转/自身缺陷/夹具漂移/伪验证/删除
无守卫）；D 流程并发（裸 commit 与 amend/reset 冲离/target 互踩/退出码吞/check 不查 asm/修复动作
制造漂移）；E 内核语义（收口缺失死锁/上下文撕裂/移植语义/探针上限/穷举不全/无定性不修/单侧半/容量≠
拓扑）；F 冒烟环境（marker 无 emit/字节漂移/CRLF/误入库/工具缺位/证据命令伪影）。

**三轮累计新增高价值形态**（§7.2/§8.2 全录）：消息号=枚举序数三连发、幽灵 CPU（容量≠拓扑）、
`.is_ok()` 丢载荷语义、多唤醒互覆单槽、异步槽栈局部 UB、多页虚拟传输逐页 walk、容量修复布局副作用、
观测扰动翻转、panic=0 假阴性、修复动作制造漂移、gate 只查存在性不查内容、C 怪癖防「好心修错」等。

**三条核心结论**：①canned/mock 双方各自为政恒绿是 wire 缺陷最大温床（E5 族联调线束的存在意义）；
②验证通过 ≠ 架构正确（在偏离 C 的架构内打对补丁终被结构性取代）；③const 断言必须作为常量来源
而非事后证明。

## 5. 用户已裁决事项（勿重开）

- **不接 CI**（新增 workflow 跑 gate 被否，仅本地脚本）；**新模式暂不登记** review-patterns.md（85 模式
  之外的 10+ 新发现只在目录文档，登记需同步三端派生+check-review-rules.sh，另开任务）。
- `~/.zcode/cli`（2.1G 宿主全局跨项目 CLI 数据）**划界不扫**；`.zcode/plans/`（项目内）已全量扫。
- run_all.sh 三处吞退出码（P7 基线）待 P6 清理；CRLF 基线 6 处建议 NK4-C 收线时统一转 LF。

## 6. 本会话教训全集（新电脑续做前必读）

1. **`git commit --only <path>` 对「index 删除 + 工作树 untracked」路径会丢弃删除**（取工作树内容
   语义）——378a52370 补笔的由来。纯 index 删除用裸 commit 前**必须** `git diff --cached --name-only`
   核对（专属 worktree 内）。
2. **shell cwd 持久停在 worktree 时，一批「主树诊断」会静默跑在 worktree 头上**——曾误判「主树被切
   分支」。跨 worktree 每条命令显式绝对路径 cd。
3. **变异脚本的还原用备份/还原，禁 `git checkout -- <file>`**——会冲掉自己未提交的在制（P15 假 FAIL
   一次）。
4. `git add -f` 才能 add evidence 的 *.log（.gitignore 有 `*.log`）；`*.log` 忽略规则同时是 F4 家族防线。
5. 文风门：rewrite-notes 正文**行内日期**（SL-4）与裸「模式 N」术语（SL-7）都是 error——报告引用既有
   模式用「名称+编号」形态，日期用 commit 截断面替代。
6. P12 启发式（asm/rdmsr 需邻近 target_arch 门）已修正为「排除注释行 + 窗口 12 行」，仍有残余误报
   空间——FAIL 先人工复核再进基线。
7. baseline key = `检查名|路径|cksum(行内容)`：对行号漂移免疫、对内容改动敏感（改被豁免行=重报）。
8. 每**新增**防回归测试（修新事故时）要**人工追加**进 pattern-gate.sh 的 `P2_TESTS` 数组——点名式
   清单不追加就没有删除守卫。
9. 增量轮方法论（三轮验证有效）：先实测基线对比圈定增量（日志行数/git log 区间）→ 精读增量 →
   复跑 gate 找漂移 → 对照模式目录判新 → 机械化判定（强形态进 gate，语义级进报告）→ worktree
   交付三绿。
10. **shell cwd 三条追加教训（C-64 当轮三次现世）**：①前台 shell 的持久 cwd 与后台命令**不共享**——
   后台命令里 cd 主树，下一条前台命令仍在旧 cwd（gate 全量曾误跑主树旧脚本）；②`claim.sh release`
   必须显式 cd 主树后再执行——在 worktree cwd 内跑 release，worktree 删除会把 shell 自己活埋
   （本轮 release 中断在 branch -D 之前，销账提交滞留 claim 分支，靠 cherry-pick 补账）；
   ③spawn ENOENT 自救法：用 Write 工具重建同路径目录让持久 cwd 重新可解析，再全部显式 cd。
11. `pattern-gate.sh --diff` 大区间（117 笔含 3000+ 行 worklog delta）会挂起（token 提取后的逐符号
   grep 无进展、零 CPU 50 分钟）——大区间对账改用有界命令（P7/P8/P12/P14 全量本就对账整树；
   P10/P13 按 -- '*.rs' 等路径过滤后手工提取 token 复查）。

## 7. 后续工作路线

- **增量扫描节奏**：FIXLOG/WORKLOG 大幅变动后（参考本轮：+274 笔提交触发一轮），按 §6.9 流程开新
  C 编号重扫。NK4-C 续跑线（续-66~73）的增量够一轮小扫。
- 报告 §6 遗留：CI 接线（已否）、review-patterns 登记（已缓）、run_all.sh 吞码清理、CRLF 批量转 LF。
- NK4-C 的 task1-close 探针大裁决（`git grep -n 'nk4a:'`）是 NK4-C 线自己的活，gate 不介入。

## 8. 新电脑恢复步骤

1. clone 仓库（旧电脑不再有增量数据；确保新电脑仓库含 f7739a18f 之后的历史）。
2. `.review/` **不入 git**——新电脑若无此目录，P1 会 SKIP（属预期，非故障）；若旧机 .review 需要保留，
   由用户另行整目录拷贝。
3. 读本文件 → 读 `PATTERN-SCAN-REPORT-20260923.md`（§2 目录 + §7/§8 增量）→ `bash tools/pattern-gate.sh
   --self-test` 确认 24 例全绿 → `bash tools/pattern-gate.sh` 看主树现状（NK4-C 新提交可能产生新漂移，
   正常，按增量轮处理）。
4. 协作规则：`new_edge4.md` §1（并发规则）+ `tools/claim.sh`（锁=分支）；构建铁律 docker minix-ci:1.94
   `-m 2g -j 1`；整仓 `cargo fmt` 禁止（rustfmt nightly 单文件核验）。
5. 续做入口：按 §6.9 增量轮流程，或从文末 prompt 的「第一步」开始。

## 9. 附录：新电脑恢复 prompt

见交付回复文本（同一段 prompt 也请用户保存为独立文本文件）。核心一句话：**你是 minix-rs 历史日志
模式回归门（pattern-gate）工作线的接手者，读 PATTERN-GATE-SESSION-HANDOFF-20260927.md 恢复全部
上下文，按 §6.9 增量轮方法论继续。**
