# notes 目录迁移计划 — qwen

> 独立方案，基于仓库实测证据制定，未参考其它 AI 的答案。
> 模式：本文件只是**计划**，不含任何实际迁移动作。

---

## 0. TL;DR（结论先行）

1. 目标结构可行，但**真正的难点不是 `git mv`，而是 `{module}` 这一层被工具链当作语义参数硬依赖**。砍掉 `fork-syscall-rewrite/` 层 = 路径模型从**三层 `notes/rewrite/{module}/{stage}`** 降为**两层 `rewrite-notes/{stage}`**，必须先改规范源与工具，再动目录。
2. **迁移前必须打 tag**，并配合**专用分支 + 独立 worktree**（并发隔离纪律）。tag 是硬分界点：历史 review 产物、跨 AI session 里的旧路径都靠它回溯。
3. 引用面实测：`fork-syscall-rewrite` **1671 个文件**、`notes/rewrite` **1612 个文件**、`notes/study` 7、`notes/redesign` 3。**但其中约 1174 个在 `.review/` —— 实测 `.review/` 未被 git 跟踪（`.gitignore:27`，`git ls-files .review/` = 0），是纯本地产物，直接 `rm -rf` 重建即可，不进迁移风险面**（详见 §5 R3）。
3b. 真正的迁移面只看 git 跟踪内容：`os/` 1487（含 95 处代码注释）、`notes/` 778、`tools/` 48、`prompt/` 29。其中仅 `tools/anchor-*-baseline.txt`（进 git、以 doc 全路径为 key）须**重生成**。
4. 机器生成的产物（anchor baseline、DESIGN-INDEX）**必须重新生成，不能文本替换**。
5. 建议分 **7 个 Phase**，每个 Phase 有可执行的验收门（§7），任一门不过 → 停在该 Phase，不进下一步。

---

## 1. 目录角色（对齐用户定义）

| 现状 | 目标 | 语义 | 处理 |
|------|------|------|------|
| `notes/rewrite/fork-syscall-rewrite/` | `rewrite-notes/` | 当前重写主干文档区，单一真值，**最重** | 砍 `{module}` 层 + 去 `notes/rewrite` 前缀 |
| `notes/redesign/` | `redesign-notes/` | rewrite 稳定后的探索区，**允许多个互斥并行分支**（如 vm-in-kernel vs ipc-trap-accel） | 提到顶层，内部按分支建子目录（未来） |
| `notes/study/` | `study-notes/`（或直接删） | 早期 AI 生成的 minix3 源码学习笔记 | 用户判定"可删/可精简"→ 二选一，见 §6 决策点 D3 |
| `book/` | 不变 | 定稿后从 notes 抽取的成品电子书预留位，当前空 | **本次不动**（下游，内容未定） |

`notes/` 壳层：study 删/迁、rewrite 与 redesign 提到顶层后**空心化**，本次退役。

---

## 2. 实测证据（迁移成本的地基）

> 排除 `os/target/`、`.git/`、`minix3/`（原始 C 源，不改）。

### 2.1 引用分布（按文件数）

| 关键词 | 命中文件数 | 主要分布 |
|--------|-----------|----------|
| `fork-syscall-rewrite` | **1671** | notes 内部链接、`.review/` 产物、tools、os 注释、prompt 规范 |
| `notes/rewrite` | **1612** | 同上 |
| `notes/study` | 7 | 少量交叉引用 |
| `notes/redesign` | 3 | 少量交叉引用 |
| prompt/.claude/.codex/.trae 规范源 | 39 | 定义路径约定，**必须最先改** |

### 2.2 结构性事实（决定工具怎么改）

- **`fork-syscall-rewrite` 是唯一含 stage 的 module**：其下 19 个 `*-stage-*` 目录；`notes/rewrite/` 下其它目录 `concepts/`(0 stage)、`archive_bak/`(0 stage) 均非 module。→ `{module}` 层取值恒为单一值，**退化为冗余层，可安全塌缩**。
- **stage 目录内含特殊子目录**（随 stage 一起 `git mv`，但工具引用它们）：
  - `.design/`：**18 个**（可复用快照，`design-coverage-check.sh` / `review-gate-check.sh` 强依赖）
  - `draft/`：**17 个**（被内部链接 `../02-stage-vm/draft/...` 引用）
  - `tmp_design_and_todo/`：0（已清理）
- **`{module}` 是语义参数，不是普通字符串**（CLAUDE.md L299）：
  > `{module}` = first directory under `notes/rewrite/` (e.g. `fork-syscall-rewrite`)

### 2.3 工具链硬依赖（实测命中）

| 工具 | 依赖形式 | 三层假设点 |
|------|----------|-----------|
| `tools/design-coverage-check.sh` | `MODULE_DIR="notes/rewrite/${MODULE}"`，参数 `<module>` | 收 stage → 遍历 `.design/` |
| `tools/review-gate-check.sh` | `MODULE_DIR="notes/rewrite/${MODULE}"`，推断 stage | Step 0.3 门 |
| `tools/design-index-update.sh` | 维护 `notes/rewrite/{module}/{stage}/.design/DESIGN-INDEX.md` | 路径写死前缀 |
| `tools/review-init.sh` | 自动算 `{module}`/`{doc-stem}` 建目录 | `.review/{tool}/{module}/` |
| `tools/doc-style-lint.sh` | 增量门限定 `git diff -- notes/rewrite` | 前缀写死 |
| `tools/coverage-extract/coverage-extract.py` | SYMBOLS 覆盖率枚举引用 stage 路径 | — |
| `tools/anchor-suspect-baseline.txt` / `anchor-unresolved-baseline.txt` | **key = 完整 doc 路径** | 机器生成，须重生成 |
| `.codex/` `.claude/` `.trae/` skills | Step 0 的 4 条 `ls notes/rewrite/{module}/{stage}/.design/...` + 引用 `{module}` | 派生自 prompt/ |

> **CI workflows / `book/SUMMARY.md` / `os/*.rs` 编译**：CI 命中 0、SUMMARY 命中 0 —— 干净。os 代码有 **95 处** `.rs` 注释引用 `notes/rewrite/fork-syscall-rewrite/...`（文档字符串，不影响编译，但属活引用须改）。

### 2.0 git 跟踪事实（划定真正迁移面）

| 目录 | `git ls-files` 跟踪数 | 处理方式 |
|------|----------------------|----------|
| `.review/` | **0**（`.gitignore:27` 显式忽略，"auto-generated, not committed"） | **不迁移**：`rm -rf` 后重跑 review 自然按新布局长出 |
| `os/` | 1487 | 95 处注释活引用须改（Phase 3） |
| `notes/` | 778 | 主体，内部链接须改（Phase 3） |
| `tools/` | 48 | 工具链路径模型 + 2 个 baseline 须改/重生成 |
| `prompt/` | 29 | 规范源，最先改（Phase 1） |

> **结论**：`.review/` 从风险面剔除。历史 review 产物**全部放弃、重建**（成本仅为 token/时间，非风险）。唯一进 git 且以 doc 全路径为 key 的机器产物是 `tools/anchor-suspect-baseline.txt` / `anchor-unresolved-baseline.txt`，须**重生成**（Phase 4）。

---

## 3. 目标结构与"三层→两层"塌缩

### 3.1 目录终态

```
rewrite-notes/
    00-master-plan/
    01-stage-kernel/
        16-smp.md
        .design/            ← 随 stage 一起搬
        draft/              ← 随 stage 一起搬
    ... 19 个 stage ...
    20-redesign/            ← 注：现有 fork-syscall-rewrite 内即有 20-redesign；见 §6 D2
    misc/                   ← notes/rewrite/*.md 杂项归此（§6 D1）
    concepts/               ← 原 notes/rewrite/concepts/
    _archive/               ← 原 notes/rewrite/archive_bak/（删除候选）
    README.md

redesign-notes/             ← 扁平起步，未来按互斥分支建子目录
    <branch>/xx.md          （例：vm_in_kernel/, ipc_trap_accel/）

study-notes/  或  （删除）
```

### 3.2 路径改写映射表（rewrite map）

这是所有文本替换的**唯一真源**。所有 sed/脚本以此表为准，顺序敏感（先长后短）。

| # | 旧片段 | 新片段 | 说明 |
|---|--------|--------|------|
| M1 | `notes/rewrite/fork-syscall-rewrite/` | `rewrite-notes/` | 最常见：module 层塌缩 + 去前缀，**必须先于 M3** |
| M2 | `notes/rewrite/{module}/` | `rewrite-notes/` | 规范/技能里的模板占位（字面量），人工核对 |
| M3 | `notes/rewrite/` | `rewrite-notes/` | 兜底前缀（在 M1 之后运行） |
| M4 | `notes/study/` | `study-notes/`（或删除项） | 依 D3 |
| M5 | `notes/redesign/` | `redesign-notes/` | — |
| M6 | `notes/rewrite/<file>.md`（14 个杂项） | `rewrite-notes/misc/<file>.md` | 依 D1，先定归属再改链 |

> **M1 必须排在 M3 前**，否则 `notes/rewrite/` 会先把 `notes/rewrite/fork-syscall-rewrite/` 部分吃掉，产生 `rewrite-notes/fork-syscall-rewrite/` 残留。

### 3.3 塌缩对工具 `{module}` 参数的影响（关键）

塌缩后 `notes/rewrite/{module}/{stage}` → `rewrite-notes/{stage}`，`{module}` 不再存在于路径。三种方案：

- **方案 A（推荐）**：把工具的 `{module}` 参数**语义降级为可选分组常量**。因为 module 唯一，`MODULE_DIR` 直接 = `rewrite-notes`。命令签名保留 `design-coverage-check.sh <stage>`（参数含义从 module 变 stage），或保留 module 位置但忽略/填 `rewrite`。**改动最小、语义最清晰**。
- 方案 B：引入伪 module 目录 `rewrite-notes/rewrite/`（保住三层不改工具）——**与"减层级"目标矛盾，否决**。
- 方案 C：`{module}` 复用为 `{stage}` —— 会让 `.review/{tool}/{module}/` 语义漂移成大改，**否决**。

`.review/{tool}/{module}/` 状态目录：因 `.review/` **未被 git 跟踪**（§2.0），**无历史产物需迁移** —— 本地 `rm -rf .review/` 即可。新结构下只需把 `review-init.sh` + 派生技能的输出路径改为统一新键（实测旧 `.review/` 三套键法并存：按 stage / 按短模块名 / 按 `fork-syscall-rewrite`，本次顺势统一为 **`.review/{tool}/{stage}/`**）。重跑 review 即按新布局重建。

---

## 4. 迁移执行顺序（7 Phase）

> 原则：**规范源 → 目录骨架 → 活引用 → 机器产物 → 历史产物 → 验收**。
> 顺序错会返工：先改目录再改规范，工具会一边跑一边报错；先 sed 历史产物再定归属，杂项链会指错。

### Phase 0 — 冻结与清点（不改内容）
- [ ] 建**专用分支** + **独立 worktree**（并发隔离纪律，避免与在跑的 review session 抢文件）。
- [ ] **打 tag**：`git tag pre-notes-migration-20260922`（硬分界点，见 §5 R5）。
- [ ] 冻结当前所有 `.review/` 机器产物基线：跑一次 `tools/anchor-resolve.sh` / `design-coverage-check.sh` 存档当前 pass 状态，作为迁移后对照。
- [ ] 生成完整旧路径清单：`git grep -lE "notes/(rewrite|study|redesign)|fork-syscall-rewrite"` → 存 `migrate_notes_plan/inventory-before.txt`。

### Phase 1 — 改规范源（真源先行，工具与技能派生自它）
- [ ] 改 `CLAUDE.md`：L15 目录说明、L24/25 `.design`/`tmp` 约定、L41/43 Step 0 预检、L76 doc 归属、**L299 `{module}` 定义**（塌缩说明）。
- [ ] 改 `prompt/review-rules/*`（含 `review-cmds.md`）里的路径模板与 4 条 `ls` 预检命令。
- [ ] 跑 `tools/generate-derived-skills.sh` **重新派生** `.codex/` `.trae/` skills（不要手改派生文件）。
- [ ] 跑 `tools/check-review-rules.sh` 验证规范一致性。

### Phase 2 — 目录骨架 `git mv`
- [ ] `git mv notes/rewrite/fork-syscall-rewrite/* rewrite-notes/`（含 `.design/`、`draft/` 随 stage 走）。
- [ ] 杂项 `git mv notes/rewrite/*.md rewrite-notes/misc/`（依 D1）；`concepts/`、`archive_bak/→_archive/`。
- [ ] `git mv notes/redesign redesign-notes`；依 D3 处理 `notes/study`（删 or 迁 `study-notes`）。
- [ ] 删空壳 `notes/`（README.md、TODO.md 先归位：TODO 建议移 `rewrite-notes/TODO.md`，见 D5）。
- [ ] 全部用 `git mv` 保 history，**不 `cp`+`rm`**。

### Phase 3 — 机械改写活引用（用 §3.2 rewrite map，按 M1→M6 顺序）
- [ ] `tools/*.sh`、`tools/*.py`：改 `MODULE_DIR` 构造 + `.review` 输出路径（依 §3.3 方案 A + D4）。
- [ ] `os/**/*.rs` 95 处注释：`sed` 后 `cargo build` + `cargo clippy` 验证不破坏编译。
- [ ] `notes`（现 `rewrite-notes`）内部**绝对路径链接** M1/M3 改写（02/03 stage 各 200+ 处，命中最密）。
- [ ] **内部相对链接 `../NN-stage-x/...` 大多自动存活**（module 在 stage 之上，塌缩不影响同层 stage 互链）；但**从 stage 指向原 `notes/rewrite/*.md` 杂项的相对链**会因杂项移入 `misc/` 而变深 → 须逐一修。
- [ ] `book/`、`AI-chats/`、`examples/` 等若有引用一并处理。

### Phase 4 — 重新生成机器产物（禁止 sed）
- [ ] 重跑 `tools/anchor-resolve.sh` 重生成 `anchor-*-baseline.txt`（这些以完整 doc 路径为 key，sed 会与新路径体系不一致，且工具生成锚点有已知命名错误 —— 见既有 pitfall）。
- [ ] 重跑 `tools/design-index-update.sh` 对每个 stage 刷新 `.design/DESIGN-INDEX.md`。
- [ ] 重跑 `tools/coverage-extract/coverage-extract.py` 刷新 SYMBOLS 覆盖。

### Phase 5 — 处理历史 `.review/`（未跟踪，直接放弃）
- [ ] 本地 `rm -rf .review/`（`.gitignore:27` 已忽略，无版本库影响）。
- [ ] 无需 sed/冻结：新结构下重跑 review 会按 `.review/{tool}/{stage}/` 自然重建（成本 = token/时间）。

### Phase 6 — 验收门（§7），全绿才进 Phase 7
- [ ] 跑 §7 全部 G1–G7。

### Phase 7 — 收尾
- [ ] 更新 `AGENTS.md`（Codex 入口镜像 CLAUDE.md 的目录段）。
- [ ] commit（单个语义化 commit 或按 Phase 分 commit，见 D7）；合并回主分支。

---

## 5. 风险登记册

| ID | 风险 | 证据 | 缓解 |
|----|------|------|------|
| R1 | `{module}` 语义参数塌缩改错，全线 review 工具崩 | CLAUDE.md L299 + 6 工具硬编码 | §3.3 方案 A；Phase 1 先改规范；每个工具改完 smoke test |
| R2 | sed 误伤（`notes/rewrite/` 前缀吃掉 `fork-syscall-rewrite` 产生残留） | 1600+ 命中 | rewrite map **顺序敏感**（M1 先于 M3）；先在 `--dry-run` 看 diff 量 |
| R3 | ~~`.review/` 1174 文件 sed 后语义污染 or 冻结后断链~~ | **实测 `.review/` 未进 git（跟踪数 0）** | **非风险**：本地 `rm -rf .review/`，重跑 review 按新布局重建（D6 已定） |
| R4 | anchor baseline sed 后与工具再生不一致；工具生成的锚点名本就有误 | baseline 文件 + 已知 pitfall | Phase 4 **重生成**非替换 |
| R5 | 迁移期与其它在跑的 AI/review session 抢文件 | 多 AI 并发纪律 | Phase 0 专用 worktree + 领用（claim）隔离；迁移期间暂停其它 session |
| R6 | 无回滚锚点，出问题时无法界定"迁移前"状态 | — | Phase 0 **打 tag** + inventory-before.txt |
| R7 | `.design/`(18)、`draft/`(17) 子目录漏搬 or 被隐藏目录规则跳过 | find 命中 | `git mv` 显式带；注意 `.design` 是 dot 目录，glob 易漏，用 `git mv` 整 stage 目录 |
| R8 | 内部相对链接改杂项后指错（`../foo.md`→需 `../misc/foo.md`） | draft/stage 交叉链 | Phase 3 用 G3 断链扫描兜底 |
| R9 | os 注释 sed 后引入编译问题（字符串跨行/raw 串） | 95 处 | 改后 `cargo build`+`clippy` 门（G4） |
| R10 | `book/` 未来引用旧结构；现无内容 | SUMMARY 命中 0 | 本次不动 book；定稿时按新结构写 |

---

## 6. 待用户拍板的决策点（Open Decisions）

| ID | 决策 | 选项 | 备注 |
|----|------|------|------|
| **D1** | 14 个杂项 `notes/rewrite/*.md` 归属 | 全进 `rewrite-notes/misc/`（推荐）/ 按 rewrite vs redesign 二选一拆进两个区 | 用户已倾向"先 misc 后整理"；建议默认 misc |
| **D2** | `fork-syscall-rewrite/20-redesign` 去向 | **已定：并入 `redesign-notes/`**（仅 2 文件 / 7 引用；跨层污染、分布式一致性本就是探索议题） | 主干 stage 不应含"未定方向" |
| **D3** | `notes/study` | **已定：删除**（真实活引用仅 ~5 处，删目录 + 改指即可） | 不必迁 `study-notes/` |
| **D4** | `.review/{tool}/` 新分组键 | **已定：`.review/{tool}/{stage}/`**（旧三套键法实测已乱，顺势统一） | `.review/` 未跟踪，无历史包袱 |
| **D5** | `notes/TODO.md`、`notes/README.md` | TODO→`rewrite-notes/TODO.md`；README→`rewrite-notes/README.md` | 壳层退役后须有去处 |
| **D6** | `.review/` 历史产物 | **已定：全部放弃、本地删除重建**（未进 git，成本仅 token） | 不再是决策项，R3 已降为非风险 |
| **D7** | commit 粒度 | 按 Phase 分 commit（推荐，易 bisect/回滚）/ 单 commit | — |
| **D8** | 顶层命名 | `rewrite-notes`(用户示例) / `rewrite-book` / 其它 | 命名一致套用到 redesign/study |

---

## 7. 验收门（每门 = 一条可执行命令 + 期望）

| 门 | 命令 | 通过判据 |
|----|------|----------|
| **G1 残留扫描** | `git grep -nE "notes/rewrite\|fork-syscall-rewrite\|notes/study\|notes/redesign" -- ':!.review' ':!os/target'` | 除刻意冻结的 `.review/` 外，命中 **0** |
| **G2 目录骨架** | `ls rewrite-notes/01-stage-kernel/.design >/dev/null && ls -d rewrite-notes/*stage* \| wc -l` | `.design` 存在；stage 数 = **19** |
| **G3 断链扫描** | 遍历 markdown `](../*.md)` 目标存在性（新增一次性脚本或用 `lychee`） | 相对/绝对内链**全部可解析**，0 断链 |
| **G4 编译门** | `cargo build` + `cargo clippy`（os/ 根） | 无新增 error；clippy 与迁移前告警数持平（parity 纪律） |
| **G5 规范一致性** | `tools/check-review-rules.sh` | PASS |
| **G6 工具 smoke** | `tools/design-coverage-check.sh <新参数>`、`review-gate-check.sh`、`doc-style-lint.sh` 各跑一遍 | 无路径错误，产出与 Phase 0 基线一致 |
| **G7 锚点基线** | 重跑 `tools/anchor-resolve.sh` + 对比 `anchor-*-baseline.txt` | 无新增 unresolved（依 R4 重生成而非 sed） |

**任一 G 不过 → 停在该 Phase，不进下一步；触发回滚（§8）。**

---

## 8. 回滚

- **快回滚**：`git reset --hard pre-notes-migration-20260922`（tag 锚点，R6）。
- **细回滚**：按 Phase commit（D7）`git revert` 定位到具体破坏步骤。
- inventory-before.txt 提供"迁移前旧路径全集"，用于事后核对是否漏改/错改。

---

## 9. qwen 方案的独此关注点（供其它方案查漏对照）

1. **`{module}` 不是字符串，是工具语义参数**（CLAUDE.md L299）——迁移的真正难点在此，多数"改个名"思路会漏。
2. **M1 必须排在 M3 前**的顺序敏感 rewrite map（否则前缀互吃产生残留）。
3. **机器产物重生成 ≠ 文本替换**（anchor baseline / DESIGN-INDEX / SYMBOLS）。
4. **`.review/` 未进 git（跟踪数 0），全部放弃重建**（非风险）；仅 `tools/anchor-*-baseline.txt` 进 git 需重生成（非 sed）。
5. **`.design`(18) / `draft`(17) 随 stage 整体 `git mv`**，警惕 dot 目录被 glob 漏。
6. **tag + 专用 worktree + 迁移期暂停并发 session**（R5/R6），这是多 AI 协作下最容易翻车的时序点。
7. **杂项归 misc 后，指向杂项的相对链接变深须逐修**（R8），G3 断链门兜底。
8. **CI / book/SUMMARY 实测干净**（0 命中），可排除在改写范围外，省工。

---

*生成：qwen ｜ 依据：仓库实测 grep/find 证据 + CLAUDE.md 规范源*
