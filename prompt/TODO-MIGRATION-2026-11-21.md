# 01 / 02 阶段旧 TODO 迁移与清理（2026-11-21）

## 一、范围与边界

迁移对象是 `01-stage-kernel` 与 `02-stage-vm` 两个阶段的旧 TODO 载体：

- 阶段本地 todo：`01-todo.md`、stage `todo.md`、`08-todo.md`、`smp_todo.md`、
  `endpoint_todo.md`、`draft/06-todo.md`、`02-stage-vm/todo.md`、`draft/TODO.md`、
  `archive/todo-V11-*.md`、`archive/todo-V12-*.md`。
- 轨迹制品的 open 项：`OPEN-DEBT.md`、`suggestions.md`、`follow-ups.md`。
- 跨阶段三本账：`TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`TODO-LEDGER-INDEX.md`。

不在范围内、且原样保留：`new_edge4.md`、`edge_todo.md`、三个 `TODO-LEDGER-*` 文件本体、
全部 run 轨迹制品、`misc/legacy-notes-todo.md`、`NK4A/B`。

用户裁决的迁移口径：只迁 open 项与 archive 文件级汇总；普通 closed 项不写账本，依赖 git
历史与账本的 `origin_commit` 追溯；轨迹制品原位保留，只迁其中 open 项。

## 二、方法

1. `tools/todo-migrate.py` 对 20 个源文件做只读提取，产出 148 条候选（`manifest.tsv`）。
2. 六批只读子 agent 逐条核实，按「代码/文档是否仍存在该问题」判定
   open / closed / historical / merged_into / blocked，附命令与证据。
   覆盖结果：148 条候选全部有核实结论（closed 85、open 37、historical 22、
   review 2、blocked 1、merged_into 1）；另有核实中补获的 miss 项。
3. `tools/todo-ledger.py` 增加迁移所需可选字段（`merged_into`、`legacy_status`、
   `origin_file/origin_line/origin_commit`、`verified_by/verified_at`、`aliases`、`scope`），
   并补齐跨文件重复 ID、`--doc` 归一、零匹配报错、头注说明四项校验。
4. 写入三本 YAML 账本；旧文件先备份到 `tmp/todo-migration/`，再 `git rm`。
5. 独立回归：账本门、引用门、链接门、轨迹门、编译门、规则同步门；并对账本状态做二次复核。

## 三、结果

| 账本 | 条目 | open | blocked | closed | historical |
|---|---:|---:|---:|---:|---:|
| `todo-ledger/01-stage-kernel.yaml` | 39 | 19 | 4 | 2 | 18 |
| `todo-ledger/02-stage-vm.yaml` | 28 | 17 | 5 | 6 | 5 |
| `todo-ledger/coordination.yaml` | 46 | 37 | 0 | 1 | 8 |

`blocked` 是挂在 open 行上的标志（不是独立 status），统计上包含在 open 列内；
它是「需用户裁决、不得自行决定」的标志，共 9 条：

| ID | 待裁内容 |
|---|---|
| `D01-I-19` | 接受 2026 全量重写作为该项收口，还是对新文档重开 §3–§6 续扫 |
| `D01-PROSE-endpoint_todo-Q1-Q10` | endpoint 设计十问（SHIFT 位宽、64 位重排、解耦形式等）无裁决 |
| `K01-RULE-BUILT-NOT-WIRED` | Rule Discovery 候选是否落地为规则 |
| `K01-S-3b` | 低内存保留区由谁证明/保障：boot-shim 扩保留区，还是独立 low-memory 池 |
| `D02-RUN-C-04` | 闭环声明不可核：READER-C 编号复用，C-11..C-16 无现存回执 |
| `D02-RUN-D02-F10` | 36 条发现无逐条回执，计数 10+13+11=34 与 36 不符 |
| `D02-RUN-D02-F10a` | F10 子行，随父行判定 |
| `D02-RUN-D02-F10b` | F10 子行，随父行判定 |
| `D02-G-V11-1` | VM 的 SIGTERM/SIGSTOP 回调无 C 锚点：关闭该 gap 行还是并入 LU 批次 |

## 四、回归复核

回归阶段（含一次独立只读审计）修正了迁移本身的八类问题：

1. **假 open（3 条）**：`D02-VM-OPEN-1/2` 与 `D02-SUMMARY-open-2` 源自 `02-stage-vm/todo.md`
   的状态摘要，但装载链已于 2026-09-07 落地、暖启动状态机已于 2026-11-04 落地（A14）；
   已按代码与记录关闭，残项（`E-BOOTFRAME`/`E-FSRUNTIME`）留在 `edge_todo.md`。
2. **假 blocked（1 条）**：`COORD-P-ALL-13-L40` 的触发条件 `PD-02` unclamp 已于
   2026-10-31（C2E）满足，改为可实施 open。
3. **重复子行（2 条）**：`SD-24` 的两个机制细节子行并入 `COORD-SD-24`。
4. **索引行（3 条）**：三个阶段索引行不是独立待办，转 historical。
5. **漏项（2 条）**：`draft/00-vm-overview.md` §8.4 的 minix-vm clippy 余量与 ACPI MADT 扩展
   未在六批中；已补入 `02-stage-vm.yaml`（clippy 实测 31 warnings）。
6. **节级折叠（1 条）**：`02-stage-vm/draft/TODO.md` 的 P2 小节原有 5 项，迁移时折叠成 1 条节级 open；
   按代码现状展开为 3 项 open（InPlaceOption、VM 专用 panic、enum MemType）与 1 项 closed（可见性复查，两条合并）。
7. **文档-代码漂移（1 条）**：`09-vm-boot-protocol.md` §4.1 仍引用旧「温启动显式拒绝」实现，
   与当前 `boot_route` 四形路由不符；登记为 `D01-DOCSYNC-VM-WARM`（P1）。
8. **独立审计补漏（3 条）**：`F-R37-03` / `F-R5-03` 是 D-107 外部线程哨兵，随用户确认收口为 closed；
   `draft/06-todo.md` 的 06 篇设计快照漂移补为 open（`.design` 仅 v1 且头注用旧路径）；
   `endpoint_todo` 的 Q1-Q10 行号与别名拼写修正，重复的 §11.2 行合并。

## 五、门读数

全部在本机执行，命令与结论如下：

- `python3 tools/todo-ledger.py check` → OK；三本账 open/blocked 计数与上表一致。
- `python3 tools/todo-ledger.py check --doc 01-boot-shim-bootstrap --fail-on-open` → exit 1
  （4 条 blocked，符合「有 blocked 不得收敛」）。
- `python3 tools/todo-ledger.py check --doc 02-higher-half-kernel --fail-on-open` → exit 1
  （7 条 blocked）。
- `rg` 旧文件名（排除 `tmp/`、历史记录、迁移工具本身）→ 编号正式文档仅 `09-vm-boot-protocol.md`
  §4.1 的 2 处（已登记为 P1 漂移）；其余命中均为 `checklist.md`、`doc_rerank_*`、
  `misc/legacy-notes-todo.md` 等历史工作文档的散文溯源。
- `python3 tools/notes-link-check.py rewrite-notes/` → 226 处断链，含 7 处正式稿存量
  （02/24、04/19、05/11、05/12、09/02）与 review-eval run 制品；本次删除未新增断链。
- `python3 tools/eval-trace-check.py ... --round R37` / `--round R5` → 均因「CONVERGED 后
  再开轮」判 FAIL；该轨迹问题在迁移前已存在，run 制品未被本次改动触碰。
- `cargo check -p minix-kernel --features mock`、`-p minix-sched`、`-p minix-vm` → 全部 rc=0。
- `bash tools/check-review-rules.sh`、`bash tools/lint-review-rules.sh` → 全绿。
- 独立只读审计（另一 agent，重新解析 manifest 与六批核实表）→ 候选覆盖 148/148；
  4 条状态匹配缺口（2 条哨兵、1 条快照漂移、1 条行号/别名）已按上述修正，复查后 0 缺漏。

## 六、删除的文件与恢复

删除 10 个文件、共 10,254 行：

| 文件 | 行数 |
|---|---:|
| `rewrite-notes/01-stage-kernel/01-todo.md` | 3041 |
| `rewrite-notes/01-stage-kernel/todo.md` | 831 |
| `rewrite-notes/01-stage-kernel/08-todo.md` | 251 |
| `rewrite-notes/01-stage-kernel/smp_todo.md` | 2006 |
| `rewrite-notes/01-stage-kernel/endpoint_todo.md` | 563 |
| `rewrite-notes/01-stage-kernel/draft/06-todo.md` | 822 |
| `rewrite-notes/02-stage-vm/todo.md` | 618 |
| `rewrite-notes/02-stage-vm/draft/TODO.md` | 222 |
| `rewrite-notes/02-stage-vm/archive/todo-V11-archive-2026-09-08.md` | 1489 |
| `rewrite-notes/02-stage-vm/archive/todo-V12-archive-2026-09-09.md` | 411 |

删除提交为 `64a71ec6a`。恢复任一文件：

```bash
git show 64a71ec6a^:<原路径> > <原路径>
```

临时备份 `tmp/todo-migration/` 在回归全绿后已删除；账本每条保留
`origin_file` / `origin_line` / `origin_commit`，可定位到删除前的原文位置。

## 七、残留与后续

- 9 条 blocked 需要用户逐条裁决；在裁决前，相关篇目不得声明 CONVERGED。
- `D01-DOCSYNC-VM-WARM` 是文档-代码同步缺陷，需按当前篇 review 流程修复。
- `edge_todo.md` / `new_edge4.md` 的条目不在本次迁移范围，仍是独立活账本。
- `SD-24` 的两个细节子行以 `merged_into` 指向 open 父行，作为去重锚点；校验器只要求子行为
  closed/historical。
- 其他阶段（03 以后）的 `todo.md` 尚未迁移；`tools/todo-migrate.py` 可直接复用。
