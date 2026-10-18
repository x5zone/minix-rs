# 并发任务板 A — 三线·命令面（`os/commands/`）

> **这份文件是什么**：`2026-10-18` 把全仓库开放项按**领地互斥**切成三份（A/B/C）中的一份，交给一个独立 agent 并发推进。切分依据是 `coordination/CONCURRENT-WORK-PROTOCOL.md` §2 的四线所有权清单，不是按难度或按阶段切——**三份文件的可写路径两两不相交**，这是并发不打架的唯一硬保证。
> **配套件**：`PARALLEL-20261018-B-LIBDRV.md`（二线 库与驱动）、`PARALLEL-20261018-C-KERN-QEMU.md`（一线 内核/引导，**独占真机窗**）。
> **本板的领地**：`os/commands/`（含 `os/commands/games/text-games`、`os/commands/usr-bin/textfilter`、`os/commands/bin/*`）、`rewrite-notes/18-stage-commands/`、本文件。
> **条目状态词**（五值，见协议 §3）：`☐` 未开工 / `🔄` 进行中（已领取，别人别碰）/ `⏸` 等待（必须注明等谁）/ `✅` 完成（必须带日期与提交号）/ `🚫` 维持登记但不排期（必须写明为什么现在不做）。

---

## 0. 你（接手的 agent）开工前的四件事

1. **先复核锚点再领**。下表每一行的「登记处」是切板那一刻的行号快照；`18-stage-commands/todo.md` 是追加式长文件，别的批次一插行号就漂。领之前跑：
   `grep -n "<条目ID>" rewrite-notes/18-stage-commands/todo.md`
   ——「看不到锚点」不等于「没有锚点」，也不等于「账已清」（判例：批次三百五十三凭猜定位，四处全不命中）。
2. **用 `tools/claim.sh` 领，别手搓 worktree**：`bash tools/claim.sh claim <条目ID> qoder`（owner 名请固定用 `qoder`；另外两板分别用 `zcode-ds`、`zcode-glm`，三个 owner 名互不重叠，`claim.sh list` 一眼能看出谁在做什么）。
3. **一次只修一条**。顺手修旁边的 TODO 属于协议 §2 的「不许替别人实现他的前置」，尤其不许顺手改 `os/libs/` 或 `os/kernel/`（那是 B、C 的领地，见第 2 节禁碰清单）。
4. **门读数必须 L1**。没有附命令与输出的「✅ 通过」一律作废；推断（L3）算 FAIL，除非写明回退理由。

## 1. 任务表（按 P0→P1→P2 排，同一轮只做一行）

| # | 条目 ID | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| A1 | `BSD-HEADER-COMPAT-REF` | `18-stage-commands/todo.md:129`、`:173` | `☐` | 把 BSD 头补位面（`sys/cdefs.h`/`sys/featuretest.h` 一类宏与垫片）做出来，让 `minix3/games/fortune/fortune/fortune.c` 能在宿主真链出参照件。**手法照批次二百六十七/二百六十九** | 参照件能编译并跑出九形读数；本仓 `minix3/include/` 只有半套头是已登记事实（批次三百六十六） |
| A2 | `GETOPT-REPORT-BYTE-LETTER`（乙路切片） | `:210`、`:1592` | `☐` **非单轮量级** | 每轮只换**一个消费者**：`GetoptReport::line` 从 `String` 改成交回字节串，先做 textfilter 里最小的一个壳。**本轮切片边界写进账，别一次扫全表** | 现量规模：114 构造点 / 23 文件 / 39 薄壳 / 97 行门断言（`:210`）。每切片都要重跑全包门与负控 |
| A3 | `SPLIT-STREAMING-MEMORY` 片B | `:330`（片A `-b` 已落）、`:687`、片计划 `:712`–`:716` | `⏸ 先读码` | 行模式流式化。**前置：先读 `minix3/usr.bin/split/split.c:245-320` 的跨缓冲保留语义**（`:721`–`:724` 已写明），读完再动 | `split_face` 子集 26/0；跨片边界用 od 读数钉，不用人述 |
| A4 | `CKSUM-HASH-DIGEST` | `:1245` | `☐` | 八个算法的摘要本体 + `MD5 (file) = <digest>` 报文形。体量大，可按算法族拆 2–3 轮 | 每加一族补一枚端到端门；参照件读数走 A1 的补位面或 C 板的真机窗，**不许拿宿主 glibc 输出当真值** |
| A5 | `CKSUM-HASH-STATE` 剩余片 / `CKSUM-FILE-LEGS` | `:1219`、`:1268` | `☐` | `-a`/`-o` 状态诊断的下一片；`Cannot read %s` 一族报文 | 同上 |
| A6 | `UNIQ-WRITE-ERRLEG` + `SPLIT-CLOSE-HOST-VISIBILITY` + `SPLIT-N-STDIN-FSTAT-SIZE` + `SPLIT-READ-STAT-ERRLEG` 读腿 | `:1153`、`:1160`、`:1433`、`:1136` | `☐` | 输出腿报文形与 `close` 失手；`fstat(stdin)` 的规模语义。**宿主可见性受限的那几形要按判据写明，不硬造门** | `minix-textfilter` 整包 1076/0（68 target，退码 0） |
| A7 | `TR-EQUIV-OOBCH-SET2` | `:1112` | `☐` | `\0` 填充 vs 末字节重复的两形对账 | `tr_face` 门 + 逐形 A/B |
| A8 | `COLRM-ERRLEG-ORDER-UNOBSERVABLE` | `:1316` | `☐` 收尾型 | 判据在宿主不可观测——按**判据不适用**收尾并保留证据（同 `TEXTGAMES-EMIT-ERRLEG` 的 wtf 半），**不降级成 DEFERRED 充数** | 需要的是论证 + 可重跑的读数，不是新门 |
| A9 | `TEXTGAMES-EMIT-ERRLEG` 的 `wtf` 半 | `:185`（首现）、`:221`、`:226`、`:234`、`:296` | `☐` 收尾型 | `fortune` 半已落（`:226`），`wtf` 半按「判据不适用」收口，保留 `file` 那枚事实（`wtf` 是 shell 脚本，不在「C 源码」这一类里） | 同上 |
| A10 | 台账卫生：命令面状态摘要重立 | `18-stage-commands/todo.md:3`、`coordination/TODO-LEDGER-OPEN.md:128`、已登记待核 `:159` | `☐` | 那行还是 `2026-10-08` 快照 `9f752834c` 的「未闭合 `open` 2 + 占位 5：命令面尚未接真实系统调用返回」。**做一次逐壳清点**（`find os/commands -path '*/src/bin/*.rs' | wc -l` 一类），再按现数翻摘要 | 清点命令与读数必须留在账里；单轮改摘要等于拿本轮口径覆盖别轮的账 |
| A11 | `REV-NONASCII-OPTLETTER`（集外字母在 C 打**一个字节**，本壳按 `char` 取） | `:2503` | `☐` | 与 A2 `GETOPT-REPORT-BYTE-LETTER` **同族**，最好与 A2 同一人连续做，一次改完两处的报文形 | 逐形 A/B 对参照件 |
| A12 | `RANDOM-DENOM-LEGS` 的三处已声明不追余项（非有限的 `as` 饱和、末单位残差、下溢 `ERANGE`） | `:643`（首现）、`:760`、`:1096`、`:1087` | `🚫 维持登记但不排值` | 要动就得先给「为什么不追」的裁决；否则保持现状并写明理由 | **不许默默降级**：`🚫` 行必须自带理由与出处 |
| A13 | 批次三百二十六的负控补账 | `:1602` | `☐` | 那一轮只跑了 2 枚（门是 6 枚）。**续跑 `GETOPT-REPORT-BYTE-LETTER` 的人一并补**，每枚注入都要编得过、要咬住 | 控制机判据三条见本文件第 4 节 |

**不在本板的已闭项**（别去翻案，闭单勿领）：`SPLIT-NONUTF8-DROP`、`SPLIT-TAIL-NOEOL-NEGCTL`、`FORTUNE-READ-ERRLEG`、`FORTUNE-OPEN-ERRLEG`、`FORTUNE-HOSTED-FILE-SEAM`、`RANDOM-STDOUT-EMIT-RC`、`REV-STDIN-ERRLEG`、`COLRM-IO-ERRLEG`、`SPLIT-CLOSE-ERRLEG`、`SPLIT-WRITE-ERRNO`、`JOT-SEED-CAST-UNIT-TEST`、`BIN-SUPPORT-HOSTED-ARGV-NONUTF8`、`COLUMN-TAIL-WARN-COUNT`、`JOT-PREC-MINUS-ONE-FORMAT`、`TRUNCATE-RELATIVE-WRAP`、六枚负控欠账（批次三百七十已清空）。

## 2. ⛔ 禁碰清单（越界要先在 C 板的跨界登记行里挂账，本板默认不越界）

| 不可写 | 属于谁 | 为什么硬 |
|---|---|---|
| `os/libs/` 全部 crate（含 `minix-sys`、`minix-types`、`minix-rt`） | B 板（`minix-boot` 已单独确权给 C 板） | 命令面的 `emit`/`warn_line`/`err_line` 都从这里来，改了会让 B/C 的门同时变红 |
| `os/kernel/`、`os/arch/`、`os/plat/`、`os/boot-shim/`、`os/qemu-tests/` | C 板 | 真机面 |
| `os/servers/`、`os/fs/`、`os/net/`、`os/drivers/` | B 板（fs/net 的服务端腿） | `open`-on-dir 那两枚（`COMM-DIR-OPERAND` `:2143`、`CMP-DIR-OPERAND` `:2088`）**真值在服务器端与真机**，命令面只是观察者 → 见第 5 节 |
| `rewrite-notes/01-stage-kernel/*`、`00-master-plan/*` | C 板 + **当前活跃的文档线程** | 该目录今天有未提交在制品 |
| `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`new_edge4.md` | 编排线收敛 | 协议 §3：进度只记线文件与本板，不回写权威台账（多写者必覆盖） |
| `os/commands/` 之外的 `Cargo.toml` / `Cargo.lock` | 共读 | 加依赖 = 跨界，先登记 |

## 3. 门配方与基线（先复跑，再声称）

- 宿主（工作目录 `os/`， fresh shell 要 `export PATH="$HOME/.cargo/bin:$PATH"`）：
  `ulimit -v 3145728; cargo test --offline -j 1 -p minix-textfilter` → 基线 **1076 passed / 0 failed**（68 target 段，退码 0）；`-p minix-text-games` → **60 passed / 0 failed**（lib 30 + `random_face` 25 + `fortune_face` 5）。
- 权威容器编译门（**在仓库根执行**，挂载仓库根并把工作目录指到 `os/`；claim 树里把 `-w` 换成 `/work/.wt/<id>-<owner>/os`）：
  `docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:/work" -v "$PWD/tmp/ci-cargo:/cargo" -v "$PWD/tmp/ci-target:/tgt" -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/tgt -w /work/os -m 2g minix-ci:1.94 cargo test -j 1 -p <pkg>`
  `-u` 不是可省的：不带它，容器内 root 会把挂载的 `os/target` 写成 root 属主，`release` 就删不掉那棵树（判例 C-103，本会话刚回收了 3.5G 的这类残骸）。
- **在线/离线先秒判**（别再拿 `--offline` 冒充在线门）：
  `docker run --rm minix-ci:1.94 sh -lc 'curl -sS -o /dev/null -w "%{http_code}\n" --max-time 10 https://index.crates.io/config.json'`
  → `200` 走在线门；`000` 或 curl 退码 `28` 是**宿主透明代理的假 IP 面（`198.18.0.0/15`）没起来**，等它或修它，不改配方（根因见批次三百六十八）。
- clippy / rustfmt：只判**本次触及件**，`cargo clippy --all-targets -p <pkg>` 与 `rustfmt --edition 2021 --check`。

## 4. 负控（变异控制）的三条硬判据

一枚注入**编不过就不是负控**。判定必须同时满足：① 注入编译通过（日志里无 `error[`、无 `could not compile`）；② 失败数 > 0；③ **期望的那一枚门出现在变红名单里**。此外注入前取整文件 md5、跑完整文件还原复验、再复跑同一目标确认基线归零；还原一律用内存/临时副本，**不用 `git checkout --`**（在途工作会一起没了）。

**两个坑本板刚踩过，直接抄现法**：① cargo 报的是带模块前缀的全名（`column::tests::test_...`），拿裸名求交集必然落空；② `---- 名字 stdout ----` 那类详情行也是缩进行，算进名单会把读数从 2 枚虚报成 30 枚。可抄的实现：`tmp/negctl-369/ctl369.py`（批次三百七十的四枚全咬住就是它跑的）。

## 5. 需要真机（QEMU）的条目：不在本板做，走 C 板的窗

`COMM-DIR-OPERAND`（`:2143`，且 `:1473`–`:1476` 已改标「需平台裁决 + 真机」）、`CMP-DIR-OPERAND`（`:2088`，同一行也提到 `COMM`）、`JOT-PREC-RANDOM-AB-ON-MACHINE`（`:1214`）——**这三条在本板标 `⏸ 等 C 板真机窗`**，不要自己起 QEMU。需要时按 C 板第 4 节的令牌配方排队；拿到读数后回本板把状态翻 `✅` 并附日期与提交号。

## 6. 需人裁决，不要自己拍

- `FORTUNE-DAT-FORMAT`（`:173`）：C 的读档前置是配对的 `.dat` 与定界串（`fortune.c:1064-1068`、`:268-269`），而本仓 `fortune` 是按行切格言、取模抽取的**重新设计过的过滤器**。要不要实现 `.dat` 是 `Refactor`/`[ARCH]` 级裁决。
- `FORTUNE-EMPTY-ERRLEG`（`:185`）：卡在上一条后面，那句「空库」措辞不是措辞问题。
两条都**保持 `☐/🔄 不降级`**，写清「等什么」；把 TODO 改成 DEFERRED 充数 = P0-process-violation。

## 7. 每轮的收尾动作（顺序固定）

在自己 claim 树内：一笔提交（改动 + 本板状态列 + 批次条目）→ 回主树 `git merge --ff-only claim/<ID>-qoder` → `bash tools/claim.sh release <ID> qoder` → 跑收尾五道校验：
`bash tools/todo-reconcile-scan.sh`；`python3 tools/notes-link-check.py rewrite-notes/`；`bash tools/todo-staleness-check.sh rewrite-notes/coordination/TODO-LEDGER-OPEN.md`；`bash tools/doc-style-lint.sh --diff`；`bash tools/check-staged-inflight.sh`。
**若 ff-merge 因别的线先落地而失败**：把 `rewrite` 合进你的 claim 分支（`git -C .wt/<id>-qoder merge rewrite`），只在你自己的分支里解冲突，再 ff 回主线；**绝不在主树 `checkout`/`reset --force`**（C-35 判例）。

## 8. 本板与另两板的依赖边（谁等谁）

| 本板条目 | 等谁 | 等什么 |
|---|---|---|
| A1 `BSD-HEADER-COMPAT-REF` | 不等 | 它反而是 B/C 若干「参照件对账」的前置 |
| A4/A5 `CKSUM-*` | A1 | 有参照件才敢定报文形 |
| A6 的 `close`/`fstat` 腿 | B 板（`minix-sys` 的 wrapper 面） | 若发现客户端缺号，**登记跨界，不代 B 改** |
| 第 5 节三条 | C 板 | 真机窗与 E5 读数 |
