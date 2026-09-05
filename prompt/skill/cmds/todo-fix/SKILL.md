---
name: "todo-fix"
description: "从 todo.md 选一个 stub/deferred/待修项完整实现：先讲明白是什么为什么，再讲怎么修（多方案对比 Linux/Redox/OS 理论），最后实施。当用户说修一个 TODO/实现 stub/deferred/随机选个 todo 时调用。"
---

# todo-fix

## 目标
从 todo.md 选一个 TODO 完整实现并收尾。一次只修一个（批量需用户显式说）。

## 执行（三段式，顺序固定）
按 `prompt/review-rules/review-cmds.md` §七：
1. 讲明白：TODO 是什么、为什么修（现状 grep 证据）；
2. 讲怎么修：多方案对比 Linux/Redox/OS 理论最佳实践，优中选优 + 理由；
3. 实施：代码+文档+测试一并，`cargo test -p {crate}`，重跑受影响 Gate，todo.md 标注已完成，git commit。

## 硬约束
⛔ DEFERRED 不是修复——把 TODO 改成 DEFERRED 充数 = P0-process-violation；降级必须给"依赖未解除"具体论证并登记。
⛔ 修复顺序 P0→P1→P2。⛔ 不顺手修旁边的 TODO（记录不执行）。

## 强制门
fix-guard（读±5行/grep确认/单条修复/写fix-status）+ translate 防线 + 测试名对账门。

## 产物
实现代码 + 文档同步 + 测试 + todo.md 标注 + commit。
