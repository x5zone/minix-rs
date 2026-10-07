# notes 目录迁移 · 独立审计报告（DS 侧）

- **审计对象**：仓库 `/home/xzhao/github/minix-rs`，分支 `rewrite`，迁移提交链
  `notes/pre-migrate-20261007`（`bb8a90e05`）到 `notes/post-migrate-20261007`（`964e3e28e`），
  审计时点 HEAD = `77be8773a`。
- **审计方**：DS（未参与迁移执行的独立会话）。
- **审计纪律**：全程只读。唯一两处主动变更是在交接件第 6 项要求的正向埋点（文风门埋一行、
  断链检查器埋一份探针文档），两者都已逐字节复原并用 `sha256sum` 与 `git status` 复核；
  另在 `/tmp` 建过一次临时 worktree 做新检出验证，验证后已卸载（`git worktree list` 回到仅主工作树）。
- **审计方法**：不采信叙述，全部结论用可复跑命令取得；对交接件的判据本身也做了复核，
  发现其中两条判据与迁移自身记录相矛盾（见 P1-2、P2-1），已按实测给出修正后的判据。
- **产物**：本报告 + `evidence/`（每项一条原始输出）+ `audit_*.py`（本次自写的对账脚本，不依赖迁移方脚本）。

---

## 一、结论

**可交付。没有 P0（内容丢失、改错、历史断裂、门失效四类全部不成立）。
有 2 条 P1 与 8 条 P2，全部为非阻断问题，其中 P1-1 建议在下一次文档维护窗口顺手修掉。**

一句话概括审计所见：迁移的 2428 条映射逐条兑现（内容、跟踪状态、改名历史三方守恒），
三棵新树的引用与链接在迁移前后可解析性零恶化，
全部质量门经正向埋点证明仍然有效，`.review` 冻结区 1728 件逐字节未动，
编译面零错误且代码改动确实只落在注释里。发现的问题集中在**叙述与口径**层面
（审计材料的一处判据错误、一组变量名漏改、若干文档数字口径不齐），不影响仓库内容与工具功能。

## 二、判定摘要

| 交接件项目 | 判据 | 结果 |
|---|---|---|
| 第 0 项 边界 | 在制两件不被代提交、未被改动 | 通过（提交内容与迁前一致；6 增 2 删仍留在工作树；两个文件的磁盘 sha256 与迁前清单一致） |
| 第 1 项 内容完整性 | 零丢失、零不可解释改动 | 通过（0 缺失、2095 逐字节相同、333 处差异全部归因，详见第三节） |
| 第 2 项 快照完整 | 清单与 tar 未被事后改动 | 通过（7 项 sha256 全 OK；tar 卷 2428/2428 逐字节忠实；交接件计数命令需加目录过滤，见 P2-1） |
| 第 3 项 改名历史 | 跟踪数守恒、历史可续读 | 通过（纯移动提交 1102 改名 0 增删；1102 → 1105 = 1102 + 3 个新入口文档；`git log --follow` 可读旧历史） |
| 第 4 项 引用重写 | 必改域零残留、冻结区不改 | 通过（必改域仅剩两个白名单文件；三个冻结区命中数与迁前基线逐一相等；`.review` 1728 件逐字节未动） |
| 第 5 项 链接与锚点 | 不引入新断链 | 通过（迁移方脚本复跑「新增 0」；本审用更强判据复核现状 373 条断链，全部是迁前即断的既存断链；锚点基线行数与行号有效性抽查通过） |
| 第 6 项 门有效性 | 静默失效探测 | 通过（三类埋点全抓到并已复原；规则三端一致；工具在新布局真实可用；新检出可用） |
| 第 7 项 编译面 | 零 error、代码语义不受影响 | 通过（docker `minix-ci:1.94`：`cargo check --workspace --tests` 退出码 0、零 error；两遍 `cargo test --workspace` 均 102 套件 / 876 用例通过；`os/` 改动非注释新增行仅 1 行且属 README 列表项；唯一崩溃的 `minix-driver-rt --lib` 经查为既有配置性缺陷，源码零改动，见 P2-8） |
| 第 8 项 语义裁决 | 12 条裁决执行不走样 | 通过（逐条复核，另发现 `{rw-module}` 一族漏改，见 P1-1） |
| 自加项 | 目录卫生、新检出、文档事实抽查 | 0 空目录、0 嵌套错位、旧树彻底消失；新检出工具可用；文档事实抽查发现两处口径问题（P2-3、P2-4） |

## 三、逐项证据

### 第 0 项 · 边界

- 已跟踪未提交改动只有 `rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md` 一处（`M`）。
- 该文件在迁移提交里的内容与 pre-tag 内容逐字节相同（`git show` 两侧 `diff` 空），
  工作树里维护者那处改动仍是 6 增 2 删，未被代提交。
- 两块在制内容的磁盘 sha256 与迁前清单一致：
  `TODO-3ARCH` = `9b3152dd…`、`PENDING-DECISIONS` = `4cdde71c…`，都等于 `manifest.notes.disk.tsv` 记录值。
- 未跟踪件与交接件清单一致（`new_laptop_migrate/` 13 件、`agents-workflow-optim.md`、
  `PENDING-DECISIONS-3ARCH-PARITY.md`、`p7.c`），迁移没有顺手提交它们。

证据：`evidence/item0-boundary.txt`。

### 第 1 项 · 内容完整性（本次审计的核心）

**先修正交接件的判据**。交接件第 1 项要求「按映射表逐条比 sha256，期望 一致 2428 / 0 / 0」。
这条判据按字面不成立，原因是迁移本身在第 3 阶段（引用重写）与第 7 阶段（入口文档重写）
**有意**改写了树内 333 个文件的内容——把「一致 2428」当作验收线，任何独立审计方都会得到
333 条假 P0。正确的判据是：**逐条比对必须无一缺失，且每一处差异都能被合同（19 条映射规则的
前缀替换）与已记录的重写操作完全解释**。本审计按修正后的判据执行，结果如下。

- 三圈账闭合：三棵新树磁盘文件 2428 = 映射表 2425 + 迁移期新增入口文档 3；映射表 2428 条
  与迁前清单 2428 条逐行对应；按类型 1102（跟踪）+ 1325（忽略）+ 1（未跟踪）与清单的
  `git_status` 列完全一致。
- 逐条 sha256：**缺失 0**；逐字节相同 2095；有差异 333。
- 333 处差异逐条归因（脚本先按合同变换再比对，残差逐行分类）：
  - 180 个文件可由前缀替换完全解释（变换后逐字节相等）。
  - 153 个残差文件里，变化行 = 396 行前缀替换 + 109 行链接目标重定基 + 143 行结构性编辑。
  - 143 行结构性编辑只落在 5 个文件，全部有记录：
    `rewrite-notes/README.md` 133 行（第 7 阶段明文重写为入口文档，新链接逐一核过、目标存在）、
    `redesign-notes/README.md` 7 行（明文加状态段）、
    `rewrite-notes/00-master-plan/README.md` 1 行（明文改的 20 号编号那一行）、
    `rewrite-notes/coordination/new_edge4.md` 1 行（叙述里的路径前缀 `notes/redesign/` → `redesign-notes/`）、
    `rewrite-notes/misc/project-plan.md` 1 行（`notes/redesign/process-model.md` → `redesign-notes/process-model.md`；
    该目标在迁前就不存在，属既存悬空引用被机械换前缀，坏度不变）。
- 无一个被忽略或被跟踪以外的文件被改动：333 处差异全部是跟踪文件（`git-mv` 类），
  1325 个被忽略文件与 1 个未跟踪文件的内容与迁前逐字节相同。
- 合同本身未被事后改动：`path-map.tsv` 的 sha256 = `c43f82c5…`，与第 1 阶段执行日志记录值一致。
- 链接级独立复核（自写脚本，按「旧文本解析 → 映射表推期望新目标 → 现文本解析比对」）：
  迁前指向被搬文件的 1193 条链接中 1170 条解析到映射表目标，23 条例外全部在被重写的
  `rewrite-notes/README.md` 内（旧链接被新链接取代，新链接目标均存在）；
  指向未搬文件（`os/`、`minix3/`）的 21 条链接全部解析到同一绝对目标；
  迁前即悬空的 384 条链接文本零改动。

证据：`evidence/item1a-accounting.txt`、`item1b-hash-reconcile.txt`、`item1d-classify.txt`、
`item1d-mech-list.tsv`、`item1d-residual-list.tsv`、`item1e-per-file-class.tsv`、
`item1e-structural.tsv`、`item1h-summary-v2.txt`、`item1k-linktarget-count.txt`。

### 第 2 项 · 快照与 tar

- `sha256sum -c snapshot.sha256` 七项全部 OK——四份清单与 tar 卷本身在被审之前没有被改动过。
- tar 卷 sha256 = `39e68026187bc72fa9dd12672c0d0881bbda4146deb0ad7d9646ee98dcd86eb1`，
  与记录值一致；全卷可读（`tar -tzf` 退出码 0）。
- 文件级条目计数：卷内 `notes/` 文件 2428、`.design` 文件 1162、`.review` 文件 1728，
  与磁盘逐一对上；615 MB 的 `serial_c9a.log` 在卷内且字节数正确。
- **本审加做的一项更强验证**：把 tar 卷里的 `notes/` 全量解到 `/tmp/tarverify`（1.3 GB），
  对 2428 条逐条重算 sha256 与迁前清单比对，结果 **一致 2428 / 缺失 0 / 不一致 0**——
  这份恢复源对迁移前状态是逐字节忠实的，不只是「能解开」。
- 未跟踪件与 `PENDING-DECISIONS` 都在卷内（抽查命中）。
- 迁移方自己登记的局限仍在：tar 卷放在仓库 `tmp/` 内、整域不入库，磁盘上只有一份拷贝，
  建议验收通过后尽快复制到仓库外（P2-7）。
- 交接件第 2 项的计数命令有一处口径问题：`tar -tzf … | grep -c '^notes/'` 会把 108 个目录条目
  一起数进去（得 2536），需要加 `grep -v '/$'` 才是 2428（详见 P2-1）。

证据：`evidence/item2-snapshot-tar.txt`、`item2b-tar-files-only.txt`、`item2c-tar-spotcheck.txt`、
`item2d-tar-content-full.txt`。

### 第 3 项 · 改名历史与跟踪守恒

- 纯移动提交 `e90e4173d`：`1102 files changed, 0 insertions(+), 0 deletions(-)`——移动边界零内容改动成立。
- 跟踪数守恒：迁前 `notes` 树 1102 个跟踪文件，现在三棵树 1105 个，差值恰好是 3 个迁移期新建的
  入口文档（`rewrite-notes/MIGRATION.md`、`redesign-notes/vm/README.md`、`study-notes/README.md`）。
- 第 2 阶段抓到的「108 个文件静默退跟踪」修复经复核仍在索引里：`.log` 106 个、`.out` 2 个，
  与迁前计数逐一相等。
- `git log --follow` 可跨迁移读到旧历史（抽查 3 例：`16-smp.md`、`rewrite-notes/README.md`、
  `rewrite-notes/coordination/edge3.md`）。
- 区间 diff 里出现 2 个 `D` 与 5 个 `A`，逐个查明：2 个 `D` 是改名相似度跌破 git 阈值后的展示形态
  （`rewrite-notes/README.md` 内容全重写；`edge3.md` 共 118 行、改了 38 行，相似度降到两成多），
  目标文件都在、`--follow` 可续；5 个 `A` = 这 2 个加 3 个新建文档。没有文件被顺手退出跟踪。
- 三棵树的磁盘构成：2428 = 1105 跟踪 + 1322 忽略 + 1 未跟踪，与迁前的
  （1102 + 1325 + 1）逐项可解释。

证据：`evidence/item3-rename-history.txt`、`item3b-AD-details.txt`、`item3c-rename-threshold.txt`、
`item3d-conservation.txt`。

### 第 4 项 · 引用重写

- 必改域（三棵树、`os/`、`tools/`、`prompt/`、三端派生、`CLAUDE.md`、`AGENTS.md`、根 `README.md`）
  字节级扫描（绕开本机 `ugrep` 对无效 UTF-8 文件静默吞输出的已知问题）：
  只剩两个白名单文件有旧路径——`.claude/settings.local.json` 38 行（按裁决冻结的授权命令历史）、
  `rewrite-notes/MIGRATION.md` 21 行（旧新对照表本体）。其余全部为零。
- 树内三个历史区保留旧路径原文，命中数与迁前基线逐一相等：
  `evidence/` 4 个文件、`archive/legacy-fork-bak/` 8 个文件、各 `.design/` 快照 56 个文件。
- **本审加做的一项验证**：把 tar 卷里的 `.review/`（1728 件）全量解出与磁盘现状逐字节比对，
  结果 1728 件全部一致、0 件内容不同、0 件缺失；磁盘上只多出 4 个文件，正是
  `PATH-MAPPING.md`（新写的映射说明）与搭车归档的 3 件评审产物。**冻结裁决是逐字节落实的**，
  不只是「看起来没改」。
- 裸名提及（不带路径形态的 `fork-syscall-rewrite`）保留 98 行：树内 73 行与迁移方清单逐行一致
  （全是历史叙述）；树外 25 行里大部分是迁移期新写的「已退役」说明，另有 3 处陈旧示例（P2-5）。

证据：`evidence/item4-ref-residue.txt`、`item4b-bare-mentions.txt`、`item4c-bare-detail.txt`、
`item4d-bare-diff.txt`、`item4e-bare-provenance.txt`、`item4g-review-frozen-verify.txt`。

### 第 5 项 · 链接与锚点

- 迁移方脚本 `compare-links.py` 原样复跑：迁移前 452 行基线 → 去重并映射进新坐标 288 条 →
  迁移后 257 条，**迁移引入的新断链 = 0**。本审另用「Phase 3 提交时点」的树重算过链接目标变化：
  133 行、139 个实例，与执行日志「138 处」在同一口径上吻合（差 1 属计数粒度）。
- **本审使用更强的判据再验一遍**：对迁移后全部 373 条断链（三棵树最新扫描），逐条回查
  「该目标在迁移前的旧坐标下是否可解析」。结果：**可解析 0 条**——373 条全部是迁前即断的
  既存断链；冻结区 7 条同类；3 个新建入口文档零断链。也就是说没有任何一条链接在迁移前可用、
  迁移后变坏。中间过程里我第一版对账脚本曾报出 132 条、82 条「疑似新增」，
  逐个查证后确认全部是「迁前即断、链接文本未动、因树变浅导致解析坐标平移」的假阳性
  （例如 `../../../minix3/…` 从指向树内的不存在路径变成指向仓库外的不存在路径），已在证据文件里留痕。
- 锚点基线：`anchor-suspect-baseline.txt` 874 行、`anchor-unresolved-baseline.txt` 6609 行，
  行数与迁前一致，文件内旧路径残留 0；suspect 清单涉及的 101 个文档全部存在，
  且其中 0 个文档在迁移区间内行数发生过变化（意味着清单里的文档自身行号仍然有效）；
  随机抽 5 条锚点核对，行号全部落在有效范围且该行内容与锚点语义相符。

证据：`evidence/item5-links-anchors.txt`、`item5c-independent-link-diff-v2.txt`、
`item5d-page-cache-probe.txt`、`item5f-new-dump.txt`、`item5h-broken-verdict.txt`、
`broken-links.now.txt`、`item5i-anchors.txt`。

### 第 6 项 · 质量门正向埋点（防静默失效）

- 文风增量门：往 `rewrite-notes/misc/misc.md` 追加一行含裸日期的文本，`--diff` 门抓到
  `[SL-4] 2026-01-01`（第 816 行）并以退出码 1 失败；门打印的检查范围是
  「限 rewrite-notes redesign-notes study-notes」，说明第 4 阶段修的正是这条静默失效路径。
  随后按备份逐字节复原，sha256 与埋点前一致、`git status` 该文件无变化。
- 断链检查器：放了 `99-probe.md` 带一条坏链，报告抓到 1 条；探针文件已删除。
- 规则三端一致与工具可用：`check-review-rules.sh` 输出 consistent（退出码 0）；
  `generate-derived-skills.sh --check` 无漂移；`design-coverage-check.sh 01-stage-kernel` 返回 1
  （业务结果：发现缺快照，不是用法错误）；旧参数 `fork-syscall-rewrite` 给出「已退役」可读报错；
  四把工具（断链、文风、锚点、unsafe）自测全部 PASS。
- **本审加做**：在 `/tmp` 建了一次全新 worktree 检出 HEAD，确认
  `tools/notes-layout.conf` 已入库、能被 `source`、三棵树变量可读、覆盖率工具在该检出上返回
  业务结果 1；验证后 worktree 已卸载，`git worktree list` 回到初始状态。
- 另做了两个工具的只读真实调用：`review-gate-check.sh claude 08-stage-is 00-is-overview m3`
  在新布局命中 `.design/` 三份快照并把状态目录解析为 `.review/claude/08-stage-is/`；
  `design-coverage-check.sh rewrite --stage 02-stage-vm` 报「ALL DOCS COMPLETE」。

证据：`evidence/item6-precheck.txt`、`item6a-lint-probe.txt`、`item6b-6c-gates.txt`、
`item9c-fresh-checkout.txt`、`item9f-gate-tools.txt`。

### 第 7 项 · 编译面与代码语义

- `os/` 区间改动 90 个文件 = 88 个 `.rs` + 1 个 `.toml` + 1 个 `.md`。
  非注释新增行合计 1 行，是 `os/libs/minix-types/README.md` 的 markdown 列表项；
  `.toml` 的唯一改动是一行注释里的路径。
- docker `minix-ci:1.94`、`-m 2g`、`-j 1`、容器内保持宿主 uid：
  `cargo check --workspace --tests` 退出码 0、零 error（475 条警告，全部是既有的死代码类告警）。
- `cargo test --workspace -q -j 1` 跑了两遍，两遍结果相同：102 个测试套件通过、876 个用例通过；
  另有一个套件 `minix-driver-rt --lib` 的测试进程 SIGSEGV（信号 11），两遍都停在同一个用例
  `kernel::kernel_transport_tests::test_asynsend_accumulates_without_flush`。
  本审把这件事查到了底（详见 P2-8）：它**不是抖动，而是构建配置确定的**——
  工作区构建与该 crate 的单包构建在特性统一（feature unification）下产生两个不同的测试二进制，
  工作区那个直接运行 10 次全部崩溃（退出码 139），单包那个连续运行 3 次全部通过；
  而 `minix-driver-rt` 的源码在本次迁移里**零改动**（`git diff … -- os/libs/minix-driver-rt` 为空），
  与迁移无因果。迁移方把它写成「环境性抖动 / load 相关」不准确，但「与迁移无关」的结论成立。
- 语义面：`os/`、`book/`、`minix3/`、`AI-chats/`、`.github/`、`opencode.json` 之外的顶层目录改动
  分布与迁移自述一致（改动只落在三棵树、`os/` 注释、`tools/`、`prompt/`、三端派生、
  `CLAUDE.md`、`AGENTS.md`）。

证据：`evidence/item7a-static.txt`、`item7b-check.log`、`item7c-test.log`、`item7-run.out`、
`item7d-toml-and-history.txt`、`item7e-single-crate-runs.txt`、`item7f-counts.txt`。

### 第 8 项 · 语义裁决执行核对

- 目录骨架（根级三目录、`book/` 不动）、`study-notes/` 184 件原样、`rewrite-notes/` 内部结构
  （stage 目录名零修改 + `misc/`、`coordination/`、`evidence/`、`archive/`）、
  `redesign-notes/` 按主题分桶（`architecture/` 10、`ipc/` 2、`fork/` 1、根 `README.md`）、
  锚点基线「前缀替换 + 行数对账」（874 / 6609 不变）逐条在树上复核成立。
- `.review/` 冻结：扫描到 1179 个文件、86907 处旧路径命中（未被批量改写），
  `.review/archive/notes-fork-syscall-rewrite-2026-09/` 3 件在位，`.review/PATH-MAPPING.md` 存在；
  逐字节核验见第 4 项。
- 未跟踪件裁决：15 件未跟踪文件保持未跟踪；双 tag 在本地存在，远端无 `notes/*` tag
  （`git ls-remote --tags origin 'refs/tags/notes/*'` 空输出）。
- `{module}` 复核：剩余 152 处全部按 Minix3 模块名解释（`coverage-extract.py {module}`、
  `minix3/minix/servers/{module}`、`os/servers/{module}/src/`、`{module}-semantic-map.json`），
  另 5 处是迁移期新写的消歧声明句（「以下 `{module}` 指 Minix3 模块名，不是评审状态目录键 `{stage}`」），
  与执行日志自述一致。
- **但复核同时发现一类漏改**：状态目录键的另一个拼写 `{rw-module}` 有 77 行没有随
  `{module}` → `{stage}` 一起处理（P1-1）。

证据：`evidence/item8-semantic-decisions.txt`、`item8b-scope-tags.txt`、`item9g-rw-module.txt`、
`item9h-rw-module-provenance.txt`。

### 自加项 · 目录卫生与文档事实抽查

- 三棵树 0 空目录、0 嵌套同名目录（不存在 `evidence/evidence` 一类错位）；`notes/` 伞目录
  在磁盘与 git 树里都已消失。
- `tools/notes-layout.conf` 已入库；实际 `source` 它的脚本是 5 个（P2-2 记录了叙述差 1 处）；
  六个相关脚本 `bash -n` 全部通过。
- `rewrite-notes/MIGRATION.md` 事实抽查：映射表方向、改名归档两项、coordination 分组计数、
  「刻意没做的事」清单与实测一致；两处数字口径问题记 P2-3、P2-4。

证据：`evidence/item9-independent-checks.txt`、`item9b-checks2.txt`、`item9d-stage-residue.txt`、
`item9e-stage-residue-scopes.txt`。

---

## 四、问题清单

### P0（内容丢失、改错、历史断裂、门失效）

**无。**

### P1（引用漏改、文档与现实不一致）

**P1-1　状态目录键改名的覆盖不完整：`{rw-module}` 拼写族 77 行未处理。**

- 现象：`{rw-module}` 与新的 `{stage}` 是同一个概念（都取「笔记树根下第一级目录名」），
  但迁移只把 `{module}` 换成了 `{stage}`，`{rw-module}` 这一族原样保留。同一个文件里两套词汇并存，
  例如 `prompt/review-rules/review-process.md`：第 403 行的定义段用 `{stage}`，
  第 1240 行的命令模板仍写 `--output .review/{tool}/{rw-module}/scans/SYMBOLS.md`。
- 影响：无功能故障（二者同义，示例产出的路径形状仍然正确，工具把 `--output` 当普通路径接收）；
  但它与迁移自己宣布的「状态目录键由 `{module}` 换成 `{stage}`」相矛盾，将来会产生第三种叫法。
- 规模：77 行 / 9 个文件，分布为 `prompt/`（源）4 个文件 + `.claude/` 2 个 + `.codex/` 2 个 + `.trae/` 2 个。
- 定位（源文件）：`prompt/README.md:472-502`、`prompt/skill/review-process-skill.md:647-686`、
  `prompt/skill/review-coverage-skill.md:20-97 / 222 / 362`、
  `prompt/review-rules/review-process.md:1240-1291`；派生副本按 `tools/generate-derived-skills.sh` 同步即可。
- 建议：一次性把 `{rw-module}` 替换为 `{stage}`（或若确有区分意图，在定义段显式声明
  `{rw-module}` 恒等于 `{stage}`），重跑派生同步与 `check-review-rules.sh`。
- 证据：`evidence/item9g-rw-module.txt`、`item9h-rw-module-provenance.txt`。

**P1-2　审计交接件第 1 项的判据与迁移自身的有意重写冲突。**

- 现象：`REVIEW-PROMPT.md` 第 1 项要求「按映射表逐条比 sha256，期望一致 2428 / 0 / 0」，
  并称这可证明「零内容改动」；但迁移第 3 阶段按裁决重写了引用（树内 333 个文件受影响）、
  第 7 阶段重写了入口文档，所以字面执行必然得到 333 条「不一致」。
- 影响：审计材料缺陷，不是仓库内容缺陷。独立审计方若字面采信，会得出错误的 P0 结论——
  这正好违背该材料「换工具换模型也能独立开工」的设计目标。
- 修正建议：把判据改为「缺失必须为 0，且每一处差异必须能由合同前缀替换与已记录的重写操作
  完全解释」；或在判据里注明「移动边界纯净性」的复核点在第 2 阶段提交 `e90e4173d`
  （那里确实是 2428/2428 逐字节相同）。
- 证据：`evidence/item1b-hash-reconcile.txt`、`item1d-classify.txt`、`item1e-per-file-class.tsv`。

### P2（叙述、口径与建议）

1. **交接件第 2 项 tar 计数命令未过滤目录条目**：`grep -c '^notes/'` 得 2536（含 108 个目录），
   期望值 2428 需加 `grep -v '/$'`；`.design` 同理（1180 → 1162）。底层数据本身完全正确。
   证据：`evidence/item2b-tar-files-only.txt`。
2. **执行日志动作 4.1 的叙述差一处**：称「六个脚本改为 `source` 该文件」，
   实际 `source` 的是 5 个（`check-review-rules.sh` 不 source 它，它改的是规则文本里的字面断言，
   行为正确）。证据：`evidence/item9b-checks2.txt`。
3. **`MIGRATION.md` 第六节口径混用**：把「迁移前 452 处」（全域、原始行数）与「迁移后 257 处」
   （三棵树、去重后的 `文件+目标` 键数）并列。正确对照应是 425 → 373（原始行数）
   或 288 → 257（去重键数），两种口径下「新增 0」都成立。
   证据：`evidence/item5b-broken-count-discrepancy.txt`、`item5c-independent-link-diff-v2.txt`。
4. **`MIGRATION.md` 第五节旧编号残留数不可精确复现**：表里记 26 / 71 / 6 / 6 / 6；
   实测（三棵树含冻结区、按出现次数）为 28 / 76 / 7 / 7 / 5，其它口径更远。
   建议重新实测或把口径写进表头。证据：`evidence/item9d-stage-residue.txt`、`item9e-stage-residue-scopes.txt`。
5. **`prompt/` 域仍有陈旧示例**：`prompt/todo_plan.md:617/826`、`prompt/skill/review-coverage-skill.md:92`
   及 `.codex`/`.trae` 派生副本里仍以 `fork-syscall-rewrite` 作为参数示例（照抄会得到「已退役」报错）；
   `prompt/README.md:446` 那一处已自注「历史快照数字」，可不改。根因是「裸名保留」的清单只扫了三棵树
   （73 行），没有覆盖 `prompt/` 与派生域。建议与 P1-1 一并修。
   证据：`evidence/item4c-bare-detail.txt`。
6. **两个文件在区间 diff 里呈 `D+A` 形态**（`rewrite-notes/README.md`、`rewrite-notes/coordination/edge3.md`）：
   内容是重写超过 git 改名相似度阈值所致，目标文件都在、`git log --follow` 可续读。记录在此，
   避免后人 grep `git diff --name-status -M` 时误判为删除。证据：`evidence/item3c-rename-threshold.txt`。
7. **tar 卷单份存放**：卷在仓库 `tmp/` 内且整域不入库，与它保护的内容同命运。迁移方已自记，
   本审重申：验收通过后复制到仓库外长期保存。证据：`00-SNAPSHOT.md` 第八节。
8. **既有确定性缺陷（观察项，与迁移无关）**：`cargo test --workspace` 时
   `minix-driver-rt --lib` 的测试进程确定性 SIGSEGV，崩溃用例是
   `kernel::kernel_transport_tests::test_asynsend_accumulates_without_flush`。
   本审的定位过程与证据：
   - 两遍工作区测试崩溃点完全相同（同一二进制 `minix_driver_rt-95f9e6d027df3b0f`）；
   - 直接运行该二进制 10 次，10 次全部 SIGSEGV（退出码 139）；
   - `cargo test -p minix-driver-rt --lib` 走的是另一个二进制
     `minix_driver_rt-ee165b612ef0427a`（用 `--message-format=json` 的 executable 字段配平），
     连续 3 次 14/14 通过；`minix-ds --lib` 单跑 2 次 113/113 通过；
   - `minix-driver-rt` 的源码在迁移区间内零改动（`git diff --name-only … -- os/libs/minix-driver-rt` 为空），
     因此该崩溃与本次迁移无因果；它更像是工作区特性统一面下暴露出的真实缺陷。
   - 迁移方的 `02-VERIFY.md` 把它描述为「偶发」「load 相关」「每遍换 crate」，与本审的复现结果不符
     （本审两遍崩溃点分毫不差）；他们看到的「换 crate」来自三次调用形态不同
     （第三遍加了 `--exclude minix-driver-rt`），每次选中不同的二进制。他们第三遍观察到的
     「minix-ds 一个用例失败」本审未能复现（两遍工作区运行里 minix-ds 全过），列为未验证项。
   建议：单独排查该用例在工作区构建下的内存安全问题，与本次迁移无关。
   证据：`evidence/item7c-test.log`、`item7g-test-second.log`、`item7j-binary-repeat.txt`、
   `item7k-feature-unification.txt`、`item7l-config-pairing.txt`、`item7m-final-crash-facts.txt`、
   `item7n-driver-rt-touch.txt`。

## 五、未能核对的项与原因

1. **迁移当时的交互确认过程**：用户与执行方的逐阶段对话不在仓库里，本次只能核对「执行结果与
   执行日志文本是否自洽」，无法核对当时是否每一阶段都真的停下来等过确认。
2. **`.review/` 1728 件的语义内容**：按冻结裁决无需评估其内容正确性；本审只验证了
   「逐字节未被改动」与「新增 4 件如预期」。
3. **`git log --follow` 对全部 1102 个改名文件的逐条成立性**：抽查 3 例通过；
   全量复核的代价与收益不成比例（改名关系由 `e90e4173d` 的 1102 条 R 记录保证）。
4. **`.design/` 1162 件快照的语义正确性**：属评审工作流的存量内容，迁移只负责搬运；
   本审验证了内容逐字节未改、目录随父移动且无嵌套错位。
5. **迁移方第三遍观察到的「minix-ds 一个用例失败」**：本审两遍工作区全量测试里 minix-ds 均通过，
   未能复现该现象；按该项本就属既有稳定的观察，未再追加更多遍数。

## 六、证据文件索引（`migrate_notes_plan/DS/`）

| 文件 | 内容 |
|---|---|
| `audit_1_content.py` | 内容完整性逐条对账（sha256、类型一致性、新增文件） |
| `audit_1d_classify.py` | 333 处差异的归因分类（前缀替换 / 残差） |
| `audit_1e_lineclass.py` | 残差文件的逐行分类（前缀替换 / 链接重定基 / 结构性编辑） |
| `audit_1h_links.py` | 链接重定基的独立复核（解析坐标比对） |
| `audit_4_refs.py` | 必改域旧路径残留的字节级扫描 |
| `audit_5_links.py` | 断链集合对账（映射坐标口径） |
| `audit_5b_broken_verdict.py` | 现状断链的「迁前是否可解析」终审 |
| `evidence/item*.txt`、`item*.tsv`、`*.log` | 各步骤的原始命令输出与控制台记录 |
| `evidence/misc.md.backup` | 文风门埋点的复原备份 |

所有脚本可用 `python3 migrate_notes_plan/DS/<脚本名>` 原地复跑（只读）。
