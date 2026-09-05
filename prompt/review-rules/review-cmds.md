# Review 任务命令规范（review-cmds）

> **定位**：面向任务的一级调度入口。一个 cmd = 一个单一目标 + 明确范围 + 明确边界。
> **来源**：`AI-chats/cmds.md` 的 21 条实际用法归纳（2026-09-05）+ [review-profiles.md](review-profiles.md) 对账（对账表见 §八）。
>
> **元原则：注意力是 review 质量的第一约束。** 给 AI 一次布置多个目标，每个目标的完成质量都会下降
> （实证：① 章节级 review 优于整文档 review；② 单一目标 cmd 优于多目标混合——这正是"明明有那么多
> review 规则，一个 full-review 就该完事"却不 work 的原因）。因此：
> ① 每个 cmd 只做一件事；② 默认范围是 chapter（章节）级；③ 跨 cmd 组合由用户显式串联，系统不隐式展开。
>
> **与 Profile 的关系**：Profile 是"规则模块加载矩阵"（机器视图），cmd 是"任务入口"（人的视图）。
> 本文件的 §八 给出旧 Profile → 新 cmd 的对账；新任务一律从 cmd 入口进入，旧 Profile ID 保留为别名。

---

## 〇、scope 参数（所有 cmd 共用）

| scope | 含义 | 何时用 | 每轮推进方式 |
|---|---|---|---|
| `section` | 单个小节 | 修补级、单点问题 | 一轮完成 |
| `chapter` | 单个大章节（**默认推荐**） | 用户实证的最优 review 粒度 | 一轮完成 |
| `doc` | 单文档 | 文档完成后按流程走一遍 | 一轮完成；长文档（>1500 行）建议先拆 chapter 多轮 |
| `range` | 文档区间（如 01~14） | 阶段性回归 | **逐 doc 推进**（每次专注一个 x.md），禁止一轮吞下整个区间 |
| `dir` | 整目录 | 阶段收尾、覆盖度专项 | 逐 doc 推进 + coverage 穷举先行 |

> **为什么默认 chapter**：review 的信噪比随上下文增大而衰减。章节级范围让每个检查项（grep 验证、
> 逐行对照、锚点抽查）都有足够的注意力预算。整文档 review 是"多轮章节 review 的汇总"，不是一轮。

---

## 一、通用强制门（所有 cmd 的最小集，任何 cmd 不可裁剪）

| # | 门 | 内容 | 规则出处 |
|---|---|------|---------|
| 1 | **锚点纪律门** | 事实断言必须带 `file:line` 或来源锚点；给不出锚点 → 显式标 `[待验证]`，禁止静默写成事实 | 模式 83 |
| 2 | **测试名对账门**（触碰代码，或文档含测试声称时） | 文档 §5 声称的每个测试 fn 必须 grep 命中；测试数量对账（声称 vs 实测） | Gate E + Step 4.5a |
| 3 | **文风门**（文档类 cmd） | 技术博客文风；禁黑话/缩写/文言文/压缩简写；禁开发文档味 | §六 style-bible + 模式 64 |
| 4 | **translate 防线**（代码类 cmd） | 对照 Redox / Linux / OS 理论 / Rust 社区最佳实践；禁止 1:1 翻译 C 结构 | 模式 16/17/65 + review.md §Rewrite |
| 5 | **fix-guard**（修复类动作） | 修前读目标行 ±5 行、grep 确认现状、一次只修一条、修后 grep 验证 + 写 fix-status | [fix-guard](../../.claude/rules/fix-guard.md) |
| 6 | **文档-代码同步** | 改代码必同步文档（Ch3/Ch4/§5），改文档涉及实现断言必核对代码 | review.md §双向闭环 |
| 7 | **产物标注** | 每修一项标注一项（scan.md / todo.md / 修复文档），避免"修没修说不清" | review-process.md §修复阶段 |
| 8 | **Ground Truth 链** | Minix3 C 源码 > design doc > Rust 代码 > 技术文档；冲突时先判哪边对，不默认改代码 | review.md §优先级链 |

---

## 二、cmd `full-review`（文档+代码 全面 review + 修复 + 覆盖度）

**目标**：修错误和 bug，关注覆盖度。对应"文档和代码完成后，按流程走一遍"。

**scope**：`chapter`（默认）/ `doc` / `range` / `dir`

**加载模块**：review.md + review-doc-checklist + review-code-checklist + review-patterns + review-process（Step 按范围裁剪）+ review-core-semantics；scope ∈ {`range`, `dir`} 时强制加载 review-coverage-skill（Gate A）。

**执行**：按 [review-process.md](review-process.md) Step 0-7，范围裁剪规则：

| scope | 制品 | Gates |
|---|---|---|
| `section` / `chapter` | 产物写入对话或 `{doc-stem}-claude-report.md`；SYMBOLS 限定章节涉及符号子集 | B/C/D/E/H 中适用于本章节者；Gate 0/STATE 简化为对话内声明 |
| `doc` | 完整 scan.md + VERIFY-CHECK.md + STATE.md | 0/A/B/C/D/D-6/E/G/H |
| `range` / `dir` | review-scan 编排器接管，逐 doc 出制品 | 同 doc + Gate A 强制 |

**不做边界（明确排除，发现即记录、不当场做）**：
- ❌ 不做卓越性重构（正确性为主；发现卓越性问题 → 记录到 todo，引导 `code-excellence` / `style-fix`）
- ❌ 不做文风重写（发现开发文档味 → 记录，引导 `style-fix`）
- ❌ 不自动新建文档补覆盖（发现覆盖缺口 → 记录 + 建议，由用户决定）

**产物**：P0/P1/P2 问题清单（每条带 file:line + 反查维度）+ 修复标注 + scan/VERIFY-CHECK（`doc` 及以上）。

---

## 三、cmd `style-fix`（文档文风与教学性修复）

**目标**：提升教学性，for 读者。修文风、去开发文档味、章节重组、细节深度归位。

**scope**：`section`（默认）/ `chapter` / `doc`

**加载模块**：review-doc-checklist §3（可读性）+ review-doc-excellence + review-patterns（模式 64 开发文档味 + 47-57 叙事概念族）+ §六 style-bible。

**执行**（三步审计 → 修复）：
1. **章节组织审计**：每个大章节下的小章节组织是否合理清晰？知识点归属是否正确？
2. **逐节开发文档味审计**：读者关心知识点，不关心"怎么改出来的"；应像教科书技术博客讲解，而非说明书/迭代记录。
3. **细节深度审计**：本文档是否过于展开本应属于后续文档的细节？（可读该文档 todo 历史辅助判断）
4. **修复**：按审计结果重写/重组；低级错误不讲，有教学价值的用"如果 X 设计会有 Y 问题，所以用 Z"重组话术。

**不做边界**：
- ❌ 不改代码语义（文档与代码不一致处 → 记录，交 `full-review`）
- ❌ 不做 C 覆盖度判断（发现知识点缺口 → 记录，交 `full-review`）
- ❌ 不讲低级错误的历史（除非有教学价值并重组话术）

**产物**：三类审计结果 + 修复稿 + 自检（裸概念复述测试：把 Ch1 的函数名换成 [XXX]，文档是否仍可读）。

---

## 四、cmd `code-excellence`（代码卓越度 + 死代码消除）

**目标**：从不同层级思考代码设计（整体 → 模块 → 函数），再三比对（Redox / Linux / OS 理论 / Rust 社区），追求最佳设计。**死代码消除是显式子目标**，不是副产品。

**scope**：`file` / `module` / `dir`（卓越性审视按模块组织，逐文件落地）

**加载模块**：review-code-checklist + review-code-excellence（§16-21）+ review-patterns（16-29 代码族 + **79-82 架构抽象族**）+ review.md（Rewrite / Refactor / [ARCH] 三层术语）。

**执行**：
1. **分层审视**：整体架构 → 模块边界 → trait 设计 → 函数实现；每层问"如果今天重写会怎么设计"。
2. **多方案对比**：每个改进点至少 2 个候选方案 + 对照 Redox/Linux/OS 理论 + 选择理由；架构级改动标注 `[ARCH: ...]`（doc + design + code 三处一致）。
3. **死代码消除子轮**：过渡实现（被新设计取代）/ 过渡 trait（Rust 类型系统可直接表达）/ 过渡 alias（兼容旧 C 命名）/ 注释掉的代码 / 无主 TODO——**每项必须说明"为何是死代码 + 消除后影响"**；拿不准的标 OQ 上交用户，不擅自删。
4. **文档同步**：每项代码改动对应更新文档 Ch3/Ch4（设计变了文档必须跟着变）。

**不做边界**：
- ❌ 不改外部可观察行为（Rewrite 边界；行为变化 = Architectural Evolution，需用户显式批准）
- ❌ 不修正确性 bug（卓越建立在正确之上；发现 bug → 记录，交 `full-review`）

**产物**：设计对比表（旧设计 vs 新设计 vs Redox 参照）+ 死代码清单（含理由与影响）+ 文档同步记录 + `cargo test -p {crate}` 通过。

---

## 五、cmd `test-audit`（测试 5 维专项）

**目标**：审查测试本身。测试是正确性的另一支柱——但测试自己也会错、会冗余、会虚构。

**scope**：`file` / `module` / `doc`（对账文档 §5 测试描述）

**加载模块**：review-patterns（35-40 测试族 + 83 锚点纪律）+ review-code-excellence §21（测试质量）+ review-process 的 Step 4.5 / 4.5a / Gate E 规则。

**5 个维度**：
1. **完备性**：核心逻辑分支、边界条件（空/最大/最小/边界值）、错误路径（Err 变体/errno）、资源管理、并发场景（若适用）是否都有测试？
2. **测试自身正确性**：断言是否真的对应被测行为？mock/fixture 是否正确？**"测试代码就是错的，啥都测不出来"是本项目发生过的真实事故，重点排查**：assert 永真、测试调用与描述不符、fixture 与生产路径脱节。
3. **冗余**：多个测试测同一行为、可参数化合并。
4. **无效**：空函数体、`#[ignore]` 无理由、`assert!(true)`、`unimplemented!()` 残留、skip 不说明原因。
5. **虚构**（文档-代码双向对账）：文档 §5 声称的测试 fn 在代码中找不到；代码中有测试但文档不描述；测试名与文档描述不符。

**强制门**：测试名对账门（本 cmd 的主门，全量执行，非抽样）。

**不做边界**：
- ❌ 不修功能代码（被测对象的问题 → 记录，交 `full-review`）
- ❌ 不补新功能测试（发现缺口 → 输出缺口清单，修复走 `todo-fix`）

**产物**：5 维审查报告 + 文档-代码测试一致性矩阵 + 必修项清单（错误测试/虚构测试优先）。

---

## 六、cmd `style-bible`（文风宪法）

**定位**：所有文档产出的**硬约束声明**，可单独加载（muse/opencode 场景），也是其它文档类 cmd 的前置。

1. **文风**：技术博客——详细、清晰、透彻、深入浅出；讲本质机制（WHY → WHAT → HOW）；"触及读者灵魂的一针见血的本质讲解"。
2. **绝对禁止**：黑话/行话简称、压缩简写、文言文、字词缩略——**文档和中间结果都适用**。历史教训（muse 事故）："双校验、十处理函数的纯计算部分、四十五表项、三十九名单"这种输出完全不可读，且让人无法相信思维链和代码的正确性。
3. **禁开发文档味**：不写"旧版/最初/后来/我们改成"；用假设性推理替代（模式 64）。
4. **Ch1 主语 = CPU / OS / 机制 / 矛盾**，不是函数名或结构体名。
5. **中间结果同样清晰详细**：scan.md、todo、阶段产物禁止速记黑话——"换个 AI 能看明白"是可读性标准。
6. **锚点纪律**（模式 83）：事实断言必须带锚点；给不出 → 标 `[待验证]`。

**不做边界**：无（本 cmd 是约束声明，不产生任务）。

---

## 七、cmd `todo-fix`（修一个 TODO）

**目标**：从 todo.md 选一个 stub / deferred / 待修项，完整实现并收尾。

**scope**：**单 TODO**。一次只修一个；批量必须用户显式说"修 N 个"。

**加载模块**：review.md + fix-guard + 按 TODO 主题动态加载——代码类：code-checklist + patterns（16-29 + 79-82）；文档类：doc-checklist + patterns（64 族）；测试类：test-audit 5 维门。

**执行**（三段式，顺序固定）：
1. **讲明白**：这个 TODO 是什么、为什么需要修（背景 + 现状 grep 证据）。
2. **讲怎么修**：思考多个设计方案，对比 Linux / Redox / OS 理论最佳实践，优中选优，给出选择理由。
3. **实施**：代码 + 文档 + 测试一并修改；`cargo test -p {crate}` 通过；重跑受影响 Gate；todo.md 标注已完成；git commit。

**硬约束**：
- ⛔ **DEFERRED 不是修复**。把 TODO 改成 DEFERRED 充数 = 糊弄（P0-process-violation）。降级为 DEFERRED 必须给出"依赖未解除"的具体论证（哪个子系统、哪个条目阻塞）并登记到 todo 的 DEFERRED 区。
- ⛔ 修复顺序 P0 → P1 → P2；P0 未清不得标 CONVERGED。
- ⛔ 不顺手修旁边的 TODO（发现 → 记录不执行，保持单目标）。

**产物**：实现代码 + 文档同步 + 测试 + todo.md 标注 + commit。

---

## 八、Profile 对账表（旧 → 现役）

> 旧 Profile ID **保留为别名**（历史 STATE.md / scan.md 中的"Profile C"等引用可追溯）；新任务一律用 cmd 入口。review-profiles.md 中的模块加载矩阵仍是 cmd 加载规则的细节来源。

| 旧 Profile | 一句话 | 去向 |
|---|---|---|
| A | 新文档审阅 | `full-review`（scope=doc，构造模式） |
| B | 代码 PR 质量 | `full-review`（scope=doc，代码侧重） |
| C | 完整 review（<300 行） | `full-review`（scope=doc） |
| D | 快速口诀扫描 | `full-review`（scope=chapter，快速模式） |
| E | 跨文档联动 | 并入 `full-review`（scope=range）Step 4 |
| F | 链路验证专项 | 并入 `full-review`（scope=doc）Step 3.5a |
| G | 局部 Ch1&2 | `full-review`（scope=chapter） |
| H/I/J/K | 分阶段完整 review | `full-review`（scope=doc）分轮执行，轮次划分沿用 H-K 定义 |
| O | 卓越性专项 | **拆分**：文档卓越 → `style-fix`；代码卓越 → `code-excellence` |
| P | 覆盖率专项 | `full-review`（scope=dir/range，coverage 模块强制） |
| R | Design-First review | `full-review`（Step 0.3 / 1.6 / Gate H 生效路径） |
| AG | 自动生成文档检查 | `full-review`（scope=doc，仅基础正确性维度） |

**缺口填补记录**：文风修复（原无）、测试专项（原无）、TODO 修复（原无）——分别由 `style-fix` / `test-audit` / `todo-fix` 补上，这是本次重组的主增量。

---

## 九、触发词映射（口语 → cmd）

| 用户说 | 进入 |
|---|---|
| "深度全面 review 该文档/章节和关联代码"、"回归 review" | `full-review` |
| "对照 redox 找改进点/可重构设计，全部修复"、"死代码" | `code-excellence` |
| "文风 / 教学性 / 开发文档味 / 章节组织" | `style-fix` |
| "测试完备性 / 虚构测试 / 测试质量 / 冗余测试" | `test-audit` |
| "修一个 TODO / 实现 stub/deferred / 随机选个 todo" | `todo-fix` |
| "覆盖度 / 遗漏 / 未实现 + 对照 C 源码" | `full-review`（coverage 模块强制） |
| "禁止黑话/缩写/文言文"（或目标模型为 muse） | `style-bible` 前置 + 所属任务 cmd |

**组合示例**（用户显式串联，系统不隐式展开）：
- 写完一个文档 → `full-review`（scope=chapter）× 每章 → `style-fix` → `test-audit`
- 阶段收尾 → `full-review`（scope=dir，coverage 强制）→ `code-excellence`（module）→ `test-audit`
