# C-66 补扫+增量 triage 记录（484b5a12b..合并时点 + 历史缺口 8 处）

## 边界实测
- git 提交 86 笔（续-284~341 + T13）；NK4C-WORKLOG delta +732；misc_concepts 148→160
- 新文档：R32 评审群/QEMU-ENVIRONMENTS/ADDRESS-CONSTANT-AUDIT+scan 工具/接续PROMPT/TRANSIENT-PTE delta
- 三件 untracked 档案（SD 总册 1405/RETRO-AUDIT 203/TRANSIENT-PTE）在窗口内转 tracked（上轮 G10 闭合）
- 零增量：FIXLOG 四份（mtime 09-20~23）、.review STATE
- 历史缺口：new_laptop_migrate 8 件全读（目录实有 8 件非 5 件——route 2 勘误）；.review 扩面 38 文件；
  NK4B 后半/HANDOFF×2/REGRESSION×2/TODO 正文/AI-chats 中段/edge1-3 他线行全读

## 主树基线 P12 新命中分诊（本轮真实信号）
- lib.rs:3842 sfence.vma（riscv 专属指令）在 finish_and_restore（:3663，通用名、无 cfg 门）内
  ——第四发误报形态：「门在调用图可达性里」（x86 构建下函数不可达故宿主全绿；启发式无法静态看调用图）
- 处置=人工复核后 --update-baseline（rebase 后执行）；不动启发式（第四发与前三发不同类，
  架构前缀/cfg 窗/注释排除均无法覆盖调用图可达性）
- 附带：full gate 在 claim 分支上的 FAIL 即该命中的活体负例演示（基线收编前真拦）

## P17 设计裁决
- report 式（镜像 P10）：os/target_smp 122 + os/tmp 23 = 145 工件清理归 NK4-C 收线批次，
  出生即红违反「不让新门出生即红」纪律，故不阻断；清零后可转强制门
- refs/original 残留检查：实测当前 0，归 git 取证线（.wt 外纪律）；大 blob 体积扫描较重不入 gate

## 锚点抽验
- 四路 119 条；抽验 8 组：6 组逐字吻合，2 组行号偏 1（trae STATE 458→457、NK4B 2336→2337）
  ——全部实测重钉后才写入报告 §11（C-65 教训沿用）

## 增量门 triage
- P10 新增裸引 0；P13 代码面符号 21 个零虚构（同 C-65 方法）
- 零新增 #[test]：P2_TESTS +0（T13 审计交付零钉扎已立为 §11.2-7 形态）
