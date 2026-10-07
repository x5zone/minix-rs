# 迁移验收门取证（Phase 6）

> **创建**: 2026-10-07。取证脚本 `tmp/pre-migrate-snapshot-20261007/run-gates.sh`（一次性工具，未入库，
> 内容随本文附上），原始输出同目录 `gates.txt`。每条门都是「命令 + 输出 + 期望」三段式，
> 审阅方可逐条复跑，不必相信本文的判读。

## 门的来源与判据

八道门继承自六份迁移计划（HY4 的门 M1–M8 与 MiMo 的不变量 I1–I6 是同一套东西的两种编号），
本次按以下方式落地：

| 门 | 判据 | 结果 |
|---|---|---|
| M1 内容完整性 | 按映射表逐条比 sha256；**缺失必须为 0，且每一处内容差异都要能由合同前缀替换与已登记的重写动作完全解释** | 缺失 0；Phase 2 时点逐条 2428/2428 全等；截至终态 2095 全等 + 333 有差异（180 由前缀替换完全解释，其余为链接重定基与 Phase 7 入口文档重写，全部有登记；冻结区 0 处被改） |
| M1b 纯移动形态 | 移动 commit 的 diff 只有改名、零增删行 | `1102 files changed, 0 insertions(+), 0 deletions(-)` ✓ |
| M2 零残留 | 必改全域旧路径命中文件数为 0（白名单两个） | **0** ✓（见下方取证） |
| M3 断链不恶化 | 基线映射进新坐标后，迁移引入的新断链为 0 | **0** ✓（452 行基线 → 去重 288 条 → 迁移后 257 条，消失 31 条可解释） |
| M4 工具可用 | 自测与真实调用退出码不为用法错误（2） | 全过 ✓（覆盖率/门控工具返回 1 是「发现缺失」的业务结果） |
| M5 静默失效探测 | 故意埋坏链/坏锚点/坏文风，必须被抓到 | 三类各抓到一次 ✓（过程与撤销记录见执行日志 Phase 6 节） |
| M6 编译与测试 | `cargo check --workspace --tests` 零 error；`cargo test --workspace` 与迁移前**同一失败集合** | check 绿；test 见文末「一项未结观察」（迁移方原判「偶发」，三家审计实测为确定性集合，已改判） |
| M7 git 卫生 | 无未提交残留（迁移自身产物）、无空目录、旧树消失 | ✓（取证时暂存区仅剩 Phase 7 待提交项） |
| M8 三端规则一致 | 派生无漂移 + 规则校验 + 链接对读 | 四条全 0 ✓ |


## 门 M1 内容完整性：三圈账闭合
```
新树磁盘文件数   : 2428
.review 归档件数 : 3
基线清单行数     : 2428
映射表行数       : 2428
期望：新树 + 归档 = 基线 = 映射表 = 2428
```

## 门 M1b 纯移动 commit 形态（e90e4173d）
```

 1102 files changed, 0 insertions(+), 0 deletions(-)
```

## 门 M2 必改全域旧路径残留（冻结区与白名单除外）
```
# 0 命中时 grep 无输出，计数写在这里作为可读证据：
命中文件数（期望 0）；白名单两个：MIGRATION.md 是对照表本体，settings.local.json 按用户裁决冻结
```

## 门 M2b 白名单内的刻意保留项计数
```
rewrite-notes/MIGRATION.md 旧路径行 : 21
.claude/settings.local.json 旧路径行: 38（按裁决冻结）
tools/anchor-* 基线旧路径残留        : 0（Phase 5 已换完）
```

## 门 M3 断链对账
```
迁移前断链 452 行 → 去重并映射进新坐标 288 条；迁移后 257 条
迁移引入的新断链 = 0（期望 0）
消失或修好 = 31 条（重定基修正 + 树变浅后反而解析成功）
```

## 门 M4 工具可用（自测 + 真实调用；rc<=1 视为通过，rc=2 是用法错误）
```
  OK  rc=0  python3 tools/notes-link-check.py --self-test
  OK  rc=0  bash tools/doc-style-lint.sh --self-test
  OK  rc=0  bash tools/anchor-resolve.sh --self-test
  OK  rc=0  bash tools/anchor-migrate.sh --self-test
  OK  rc=0  bash tools/unsafe-audit.sh --self-test
  OK  rc=1  bash tools/design-coverage-check.sh 01-stage-kernel
  OK  rc=0  bash tools/design-coverage-check.sh rewrite --stage 02-stage-vm
  OK  rc=1  bash tools/review-gate-check.sh claude 08-stage-is 00-is-overview m3
```

## 门 M7 git 卫生
```
未提交跟踪改动数: 10
  M  AGENTS.md
  M  CLAUDE.md
  A  migrate_notes_plan/REVIEW-PROMPT.md
  M  redesign-notes/README.md
  A  redesign-notes/vm/README.md
  M  rewrite-notes/00-master-plan/README.md
  M  rewrite-notes/MIGRATION.md
  M  rewrite-notes/README.md
   M rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md
  A  study-notes/README.md
新树空目录数    : 0
notes 伞目录    : 已消失
```

## 门 M8 三端规则一致
```
  rc=0  bash tools/generate-derived-skills.sh --check
  rc=0  bash tools/check-review-rules.sh
  rc=0  bash tools/lint-review-rules.sh
  rc=0  bash tools/diff-trae-skills.sh --only-diff
```

## tag 与提交链
```
  notes/pre-migrate-20261007 -> bb8a90e05  notes 目录迁移前基线（旧布局 fork-syscall-rewrite 的最后状态）
  迁移期间的提交：
    90ab7c744 chore(migrate): notes 迁移 Phase 5——两份锚点基线换路径前缀，行数逐行对账
    c399d33ae feat(tools,migrate): notes 迁移 Phase 4——工具链适配两层路径模型 + 规则三端同步
    c2ef81f79 refactor(notes): 引用重写 Phase 3——必改域旧路径归零，相对链接按映射表重定基 138 处
    e90e4173d refactor(notes): 结构迁移——notes 三区搬到仓库根，剥除 fork-syscall-rewrite 包装层（纯移动，零内容改动）
    5ae87f503 docs(migrate): notes 迁移 Phase 1 映射表定稿——2428 行 path-map.tsv + 19 条规则合同
    d29b0f191 chore(migrate): notes 迁移 Phase 0 全貌留存——pre-tag + 四份清单 + 断链基线 + 执行日志
```

## 判读要点（供审阅方对照）

1. **门 M1 与 M2 的关系**：M1 证明「内容没变」，M2 证明「引用没漏」。两者都要过——
   只过 M1 说明搬对了没改坏内容，但引用可能还指着旧路径；只过 M2 可能是把内容一起改了来凑归零。
2. **门 M2 的两个白名单**：`rewrite-notes/MIGRATION.md`（21 行旧路径，它本身是旧→新对照表，
   不含旧路径就没用）；`.claude/settings.local.json`（38 行，历史授权命令列表，按用户裁决冻结，
   新命令会重新走授权）。除这两个之外，必改全域必须为 0。
3. **树内三个冻结区的命中数应与迁前基线相同**：`rewrite-notes/evidence/` 4 个文件、
   `rewrite-notes/archive/legacy-fork-bak/` 8 个文件、各 stage `.design/` 56 个文件。
   这些是**故意保留**的旧路径原文（取证日志、备份、设计快照），不是漏改。
4. **门 M3 里"消失 31 条"不是被删掉的记录**：树从四层变两层，一部分原本 `../../..` 越界的链接
   在新深度下正好解析得到；另有 138 处相对链接被按映射表重定基到正确目标。
   逐条清单在 `tmp/pre-migrate-snapshot-20261007/relink-report.tsv`。
5. **门 M4 的 rc=1**：`design-coverage-check.sh` 与 `review-gate-check.sh` 返回 1 表示
   「发现了缺失」（缺快照、缺 STATE），是业务结果；只有 2 才是用法/路径错误。
   旧参数 `fork-syscall-rewrite` 现在会给出「已退役 + 对照表位置」的报错，而不是静默找不到目录。

## 一项观察的改判：全量测试的失败面是确定性的，不是偶发

迁移方原判（本文件上一版）：「三遍各有一个 crate 异常、每遍不是同一个 crate、单跑全绿，判为负载相关的偶发抖动」。
**这个判读被三家独立审计实测推翻**，迁移方接受改判：

- 崩溃对象固定：`minix-driver-rt --lib` 在工作区构建下确定性 SIGSEGV（退出码 139），
  直接运行该二进制 10 次全崩；单包构建走另一个二进制，连续多次 14/14 通过——
  差异来自 cargo 的 **特性统一（feature unification）**，不是随机性。
  迁移方看到的「换 crate」是三遍命令形态不同（第三遍加了 `--exclude minix-driver-rt`），fail-fast 又让后面的套件不再跑。
- 用 `--no-fail-fast` 的全量运行失败集合固定：MiMo 测得 11 个测试目标失败（9 个进程异常 + 3 条断言失败，
  其中 `minix-ds` 的 `test_sys_kernel_hosted_reports_eio` 期望 `Err(EIO=5)` 实得 `Err(38=ENOSYS)`），
  宿主默认并行、`-j1`、容器 `minix-ci:1.94` 四种形态下**同一集合**；11 个 crate 逐个单跑全绿。
- 迁移前对照：GLM 与 MiMo 各自把 `notes/pre-migrate-20261007` 的受控内容导出到 `/tmp` 独立目录，
  用同一条容器命令跑，失败集合与迁移后**逐条相同**（含同一断言、同一期望值）。
  这条对照把「与迁移无因果」从推断变成实测。

迁移方的四条依据仍然成立（`os/` 只有注释改动、测试不读被搬文档、单跑全绿、check 零 error），
但「偶发」这个描述是错的，正确表述是「**先于迁移存在、由构建配置决定的既有缺陷**」。
失败集中在 hosted 内核半可链接面与 `minix-driver-rt` 的 `asynsend` 累积用例，
已作为独立的代码健壮性问题登记进 `rewrite-notes/coordination/NK4C-WORKLOG.md` 的待办（与迁移无关，另案处理）。

权威配方提示：本仓库的编译门面在容器里跑（docker `minix-ci:1.94`、`-m 2g -j 1`），
宿主直跑同样能复现该集合，所以复现时不必依赖容器。
