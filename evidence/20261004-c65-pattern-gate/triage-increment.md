# C-65 增量 triage 记录（c9e6ada3d..484b5a12b）

## 边界实测
- git 提交 297 笔（实测 log 输出 305 行含 docs）；NK4C-WORKLOG delta +3624（续-77f~续-283）
- misc_concepts.md：81→148 条（增量三+33 与增量四+34 两轮都在区间内；提交消息「114→148」仅指增量四——route B 勘误）
- 首扫三件 untracked：TRANSIENT-PTE 795 / SD-REGISTER 1405 / RETRO-AUDIT 203；R31 报告+对账表、三份接续 PROMPT、riscv 方法论、glm 移交、plans+2
- 零增量：FIXLOG 四份（mtime 仍 09-20~23——用户提示「FIXLOG 变动大」实测为 WORKLOG 提交流）、.review STATE、edge_todo

## 主树基线 P12 FAIL 分诊（本轮 gates 的真实信号）
- 主树全量 gate 抓到 trap_dispatch.rs:1980 共享路径 asm 无架构门（基线外）——实测为 riscv64_pagefault_body
  （:1937，cfg(target_arch="riscv64") 在 :1936）内的 csrr satp 取证探针（续-154），门在 12 行窗外=启发式
  第三发误报形态（函数名架构前缀+远处 cfg 门）。处置=collect_p12 增函数名架构前缀隐式门（非进基线），
  修正后主树转绿；一个旧基线键（trap_dispatch.rs:975 cr2 探针）结构性豁免，语义核实正确
  （包含函数 x86_trap_dispatch_body）。基线键保留未删（无害）。

## 增量门漂移 triage（有界法；--diff 大区间禁跑为 C-64 教训）
- P7×3/P8×6/P14：全量对账整树全在基线 ⇒ 增量 297 笔零新增存量
- P10：新增 todo/new_todo 裸引 Fix #N = 0
- P13（*.rs 代码面）：新增行反引号全大写符号 21 个，全仓存在性核查零虚构
- workspace --all-targets 编译兜底门：grep tools/ os/xtask .github 零命中——AF-5 建议未落地，
  报告 §10.3 登记为建议不立检查（门出生即红）

## 锚点抽验（防虚构）
- 七路 agent 236 条候选；抽验 10 组：3 组逐字吻合（TRANSIENT-PTE:474 / SD-REGISTER:996 / misc:1143），
  7 组行号偏移但引文真实（agent 行号未核对）——全部按 grep 实测重钉后才写入报告 §10
  （WORKLOG 实际锚点：111/231/9440/175/10052/11188/12093/82/11781/11888；R31 残渣行=35；RETRO=172-174）。
  教训沉淀：agent 引文必须「内容 grep 复核 + 行号重钉」双步，行号不可直接引用（A5 家族在工具链上的投影）。
