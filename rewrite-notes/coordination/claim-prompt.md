# minix-rs 并发领用任务 Prompt（通用版，粘贴给任何 agent 工具）

> 用法：整段粘贴给 agent，最后附一行指派（示例：`你的线：new_edge3，owner：muse，条目自选或指定 NS3`）。
> 所有 agent 面对同一份本地文件，可能并发。本 prompt 与 `tools/claim.sh`、`new_edge1-4.md` 配套。

---

你在 `/home/xzhao/github/minix-rs` 工作——这是 Minix3 内核模块的 Rust 语义重写（**Rewrite not Translate**：保持外部行为，内部用 Rust 类型系统重表达；x86-64/riscv64/aarch64，no_std）。总目标：**三架构 QEMU 启动 minix-rs 并运行 18-stage-commands 全部程序**。工作条目按线分账于 `rewrite-notes/new_edge1-4.md`（edge1=内核/arch、edge2=共享库/驱动、edge3=服务器/FS/命令、edge4=编排台）。可能有其他 AI 正在对同一份文件并发工作，以下纪律全部硬性。

## 0. 开工前必读（按序读完才许动手）
1. `CLAUDE.md`（项目规范）与 `AGENTS.md`（若有，你的工具入口）
2. `.claude/rules/review-core.md` + `review-process.md` + `fix-guard.md`
3. `rewrite-notes/coordination/new_edge4.md` §1（并发/领取/构建规则）与 §6（OQ 队列——你不得代决的事项）
4. 你被指派线的 `new_edgeX.md` 全文（所有权清单 + 条目表 + 状态列 + 已闭单勿领）
5. 所领条目的权威描述（`edge_todo.md` 2026-09-20 节的 E- 条目 / 对应 stage todo）

## 1. 领取（防双领，硬锁）
- 开工任何条目前：`tools/claim.sh claim <条目ID> <owner>`（owner = 你的工具/模型名：glm/qwen/HY4/muse/deepseek/…）
- 命令 FAIL = 已被领走，**立即换条目**，不许等待抢或代做他人条目
- 同一时刻只持有一个条目；领取后在你线的 new_edgeX.md 状态列标 🔄 + 日期
- 完成 → 提交 → `tools/claim.sh release <ID> <owner>` → 状态列 ✅ + 日期 + commit hash

## 2. 执行纪律（违反任何一条即返工）
- **todo-fix 三段式**：动手前先讲清楚"是什么/为什么"（含 C 对位锚点 minix3/…），再给至少两个方案对比（Linux/Redox/OS 理论），说明取舍后才实施
- **fix-guard**：修前读目标行 ±5 行（禁止凭记忆/报告改），grep 确认现状，一次只修一条，修后 grep 验证
- **锚点纪律**：一切事实断言带 `file:line`；锚点会漂移，动手前重读目标行
- **Rewrite not translate**：写代码、改文档、更新 todo 一律禁止把 C 源码 1:1 直译——C 对位以注释/锚点注明，代码与叙述必须是 Rust 侧语义（对照模式 16/65 translate 防线）。更新 todo 时写"Rust 侧事实 + 验证命令"，不复述 C 叙述；Rust 已修复的 C bug 注释标 `// MINIX3 BUG:`（模式 78）
- **硬约束**：`#![no_std]`（除 `#[cfg(test)]`）；错误映射 Minix3 errno，不自造错误码；硬件细节藏 trait 后（OS 层不见 CR3/PTE 位）；用户态服务器=单线程事件循环（Rc/RefCell 合法），内核=SMP+BKL（Arc+Mutex/Atomic，临界区内不 sleep/不 IPC）
- **技能加载**：若你的工具支持 skill（Skill 工具/斜杠命令/`.agents/skills/`），按任务**显式调用**：修 TODO/stub = `todo-fix`；代码质量/死代码 = `code-excellence`；review/扫错 = `full-review`；写任何文档 = `style-bible`（禁黑话/缩写/文言文/压缩简写，断言带锚点）。没有 skill 机制的，读 `prompt/skill/` 下同名源文件再动手

## 3. 所有权与共享文件
- 只改你线所有权清单内的文件；越界需求先在 `new_edge4.md` §2 认领板登记，登记后才动，动完销账
- 共享文件（`edge_todo.md`、`os/Cargo.toml`、`os/qemu-tests/run_all.sh`、`tools/`）：碰前在 new_edge4 §2 登记一句话。**`edge_todo.md` 的编辑半归 edge4 角色批量代写**——领取锁只锁实施权；你线新发现的跨 stage 条目写进自己文件的"新登记"小节，由 edge4 收敛
- FIXLOG（`.review/zcode/edgeN/FIXLOG.md`）：只写增量，一次会话一条 `Fix #N`；追加前自检 `grep -c "^# edge3 线修复日志" FIXLOG.md` 必须为 1（edge3 编号从 #122 续，#117-119 历史撞号引用带限定词）

## 4. 构建与验证（优先级硬性，无例外）
1. **Docker 第一优先**（docker 隔离一切 panic 崩 WSL，不只 OOM；宿主直跑无"更快"豁免）：
   ```
   docker run --rm --memory=2g --memory-swap=2g -u $(id -u):$(id -g) \
     -v /home/xzhao/github/minix-rs/os:/work -w /work \
     -v /home/xzhao/.cache/minix-rs-docker/cargo-home:/cargo-home -e CARGO_HOME=/cargo-home \
     -v /home/xzhao/.cache/minix-rs-docker/target:/work/target \
     minix-ci:1.94 cargo test -j 1 -p <crate>
   ```
   并发会话再加 `-v /home/xzhao/.cargo/registry:/usr/local/cargo/registry`。三架构 check 用 `minix-ci:1.94-arch`。
2. 仅 docker 不可用才回退宿主 `ulimit -v 3145728`（KB）+ `-j 1`，并在汇报中注明
- 完成定义：所改 crate 测试全绿 + clippy 对账零新增 + `cargo check` 无新错误；宿主与真机行为差异（`real-trap` 门控）如实注明
- QEMU 真机验证：先确认当前无其他会话在跑（`tools/claim.sh list` + 问用户）——串口判定测试并发会互相污染
- 整仓 `cargo fmt` 禁止（工具链漂移污染 diff），新代码 `rustfmt --check <file>` 单文件核验

## 5. 提交与收尾
- 自己的改动自己 commit：一逻辑单元一 commit，message 沿仓库既有风格（`feat/fix/docs(scope): 中文摘要`），不混他人未跟踪文件
- commit 后：状态列 ✅（日期+hash）→ `release` → 若解锁他线条目，到 new_edge4 §3 状态板勾行
- 交付汇报必须含：条目 ID、commit hash、测试证据（命令+通过计数）、文档同步点、遗留与登记项（DEFERRED 不算修，如实登记）

## 6. 红线
- `minix3/` 是 ground truth，**只读不改**；冲突时按 `Minix3 C 行为 > design > Rust 代码 > 文档` 判
- 不假完成：stub/DEFERRED 诚实登记；测试不绿不标 ✅；不重开已闭账（动手前 grep edge_todo.md 该条目的最新进度注记——E-SYSCALL-SIGN/E-CMDSYSFACE 等已闭环，勿按过期主张重做）
- 遇 new_edge4 §6 的 OQ 或无法判定的语义分歧：登记上交用户，不代决
