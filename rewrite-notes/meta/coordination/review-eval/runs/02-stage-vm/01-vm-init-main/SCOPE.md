# 02-stage 01 doc 内容范围裁决（V3.12）

## 输入

- `rewrite-notes/02-stage-vm/plan.md` §2 阶段表与 §3.4 边界表；
- 五份 rerank：`doc_rerank_HY4.md`、`doc_rerank_deepseek.md`、`doc_rerank_glm.md`、
  `doc_rerank_muse.md`、`doc_rerank_qwen.md` 的 01 契约；
- 现有 `rewrite-notes/02-stage-vm/01-vm-init-main.md`（830 行，作为素材而不是事实基线）；
- C 真源 `minix3/minix/servers/vm/` 与 Rust `os/servers/vm/src/`。

## 共识

五份 rerank 与 plan 对 01 的交集：

- 01 讲 **VM 从内核交接到进入主循环之前** 的启动链；
- 必须覆盖：`main()`、`is_first_time()`、`init_vm()` 顺序、启动参数/KernelInfo、
  `get_mem_chunks` 调用点、`__minix_init` 分界、`mem_add_total_pages` 调用点、
  `exec_bootproc`/boot 进程地址空间、SEF 生命周期与 RS_INIT 握手、VM 自身 libc 边界；
- 不覆盖：`vmproc` 字段与表查找（02/03）、ACL 内部（04）、物理分配器内部（05/06）、
  页表结构与操作（07/08）、区域/memtype（11–14）、主循环分发细节（15）、
  页错误/CoW（16/17）、LU 服务完整流程（25）。

## 分歧与取舍

| 分歧 | 方案 | 取舍 |
|---|---|---|
| HY4 把 01 改成“执行模型”，启动链另立 02 | 执行模型是理解启动顺序的前提 | **采纳其内容，不采纳拆篇**：在 01 增加“执行模型四条前提”一节，详细机制仍归 15/16/17 |
| qwen 把 SEF 与 boot exec 拆到 03 | 01 只留 init_vm 十二步 | **不采纳**：plan §2 与其余三份 rerank 都把 SEF/exec 放在 01；拆走会让 01 无法回答“VM 怎样变成可运行” |
| deepseek 把 SIGKMEM/LU/restart 全放进 01 | 启动链完整 | **部分采纳**：SIGKMEM 信号路径与 LU 入口放 01，LU 状态机细节归 25 |
| muse 把链接脚本/构建放进 01 | 启动产物完整 | **不采纳**：构建细节不承担启动语义，放 00/99 的参见 |
| glm 要求“SEF 双实现候选与裁决时点” | 设计对比 | **采纳其问题意识，不写裁决时点**：只讲当前 Rust 简化实现与 C 的语义对应，不写过程编号 |

## 最终范围

`01-vm-init-main.md` = 5 个内容章 + 过渡：

1. 问题与执行模型：鸡生蛋、五阶段依赖链、四条执行前提（单线程事件循环、不能缺页、
   只改不在运行的进程、fail-fast）。
2. C 源码分析：`main`/`is_first_time`、`init_vm` 十五步、`init_proc`/`exec_bootproc`、
   SEF 生命周期、`map_service`、VM 自身 libc 边界。
3. Rust 设计决策：BootParams、is_first_time 门控、init 分阶段、SEF 简化、exec_bootproc 策略、
   total_pages 校准、VM 自身内存、CALLMAP 编译时分发。
4. 实现详解：boot.rs、main.rs、global.rs、vm_server.rs。
5. 测试要点：boot/vm_server 测试、宿主边界、真机门缺口。
6. 过渡：到 02/03/15。

## 生成约束

- 按 V3.12 TR-0..TR-6 组织：每章先问题入口，示例/失败对照在前，小结/术语在后；
- 按 V3.7 出版标准：不写过程编号、commit、DEFERRED/TODO、迭代叙事；
- 事实断言必须带 C/Rust 锚点；不确定的写“当前实现”而不是“已落地/已修复”的过程话。
