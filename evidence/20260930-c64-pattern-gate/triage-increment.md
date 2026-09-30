# C-64 增量 triage 记录（a1b8b1663..c9e6ada3d）

## 边界实测
- git 提交：117 笔（含 C-62/C-63 自身交付与登记/销账簿记提交；NK4-C 续-60~77c 为主体）
- NK4C-WORKLOG.md delta：+3225/-3（git diff --stat 实测）
- riscv-reviewlog.md：+1579（全文件新增于区间内；C-61 §1 语料清单未含，用户原话「REVIEWLOG」即此文件）
- 零增量：FIXLOG 四份（末次改动 09-20~09-23）/ .zcode/plans（最新 09-27 17:04，早于 C-63 交付 18:19）/ .review STATE（≤09-18）

## 增量门漂移 triage（有界法）
- P7/P8/P12/P14：全量模式本就对账整树——主树 HEAD 实跑全部 PASS、存量全在基线 ⇒ 增量 117 笔零新增存量
- P10：git diff 区间新增行 todo/new_todo 裸引 Fix #N = 2 处（本报告 A2 目录行自身事故举例，非新增待办）
- P13（*.rs 代码面）：新增行反引号全大写符号 20 个，全仓存在性核查零虚构
  （首轮提取含文档内容行得 78 个、7 个 MISS 均为 worklog 引用的一次性探针名 NK63ENTER/NK63TXT/
   NK4C8_FORK_SENT 与笔记标识符 NOTES_BASE/P2_TESTS/objdump 伪影 O0000/SYS_VREAD_REPLY——
   文档化引用属门设计豁免面，非代码虚构）
- P11（锚点）：--diff 模式对目录文档行整体豁免（合法自指，见 gate 内注释）；§9 新增锚点
  memory.c:608 / proc.c:195 / proc.c:1143 人工逐一实测存在且落窗
  （proc.c:195=stop_local_timer / proc.c:1143=MF_REPLY_PEND 门 / i386 memory.c:608=EDOM 首检查）

## 过程事故如实记
- `pattern-gate.sh --diff a1b8b1663..HEAD` 全区间跑挂起（零 CPU 50 分钟，进程卡在 token 提取后的
  逐符号 grep 阶段）——已 TaskStop，改用上述有界 triage。教训：--diff 大区间（117 笔含 3000+ 行
  worklog）不是本门的设计工作面（门面向交付 diff），大区间对账应拆有界命令。
- shell cwd 持久停 worktree 导致一次主树文件读取失败 + 一次 gate 全量误跑主树（交接文件教训②
  当轮两次现世）——均以显式 cd 纠正，未产生错误结论。

## 锚点抽验（防虚构）
- 五路 agent 146 条候选，抽验 8 组逐字引文全通过：NK4C-WORKLOG 行 8014/8038/6219/8033（shootdown
  翻案链）、riscv-reviewlog 行 1261（fence.i 截断）/1323（签名级吞错）、MIGRATION 行 137（入口分诊）、
  VEC-CAP-GLM 行 245（归属未定）；1 处行号偏 2（8031→8033）已校正。
- 15 个新入列 P2 测试：grep fn 全部在位（各 1 命中）+ git log -S 钉出处提交。
