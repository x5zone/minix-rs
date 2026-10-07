# 13-stage-ipc Rust 实现架构级 Review TODO

> **状态（2026-09-16）**：R1 实施轮完成——20 条全部处理完毕（19 条修复闭合 + IPC-D-4 的 .design 快照半归下轮 doc-review 流程件，正文已交付）。**本轮共 15 个提交，测试从 83 → 单元 100 + 集成 4（104 passed / 0 failed），clippy 本 crate 零告警。**
> **来源**：13-stage-ipc 首轮代码扫描（查漏补缺 + 架构卓越度，2026-09-16）。入口：code-excellence（scope=dir）+ full-review 的 Gate A 覆盖穷举。
> **归档**：各条目的完整正文（是什么/为何/方案对比/修复记录）在 [archive/todo-R1-archive-2026-09-16.md](./archive/todo-R1-archive-2026-09-16.md)，修复提交号以本表为检索权威。
> **跨 stage 条目**：`../edge_todo.md` 的 E-IPCWIRE（生产面接线八缺）、E-RMIBWIRE（MIB 客户端，代码半已闭环）、E5（联调面）。
> **执行约定**：后续新发现的 IPC 条目追加在本表之后（R2 轮起），沿 todo-fix 三段式一次一条一提交；提交前核对 HEAD 归属（R1 期间曾发生并行线程提交穿插，见 git 历史 8c1a3313d）。

## 0 完成速览（级别 | 条目 | 一句话 | 修复提交）

| 级别 | 条目 | 一句话 | 提交 |
|---|---|---|---|
| P1 | IPC-P1-1 | 服务层 IpcService + IpcBoundary 边界缝组装七调用/进程事件/MIB/收尾；CallHandler 签名升级 &mut Message（C 就地写回） | 36e33530d |
| P1 | IPC-P1-2 | do_semop 检查顺序对齐 C——validate_ops 收回 perm，权限先于越界/撤销（sem.c:693） | 662a1765d |
| P1 | IPC-P1-3 | IPC_SET 落账：apply_set（sem+shm）+ 共享 SetOptions，掩膜替换保状态位 | 66f83e580 |
| P1 | IPC-P1-4 | shm RMID 落账：mark_destroy 置 SHM_DEST，立即清拍契约入文档 | 05f73a048 |
| P1 | IPC-P1-5 | shmat/shmdt 落账：record_attach/record_detach；C 的 shmdt 刷 atime 怪癖原样保留 | 7c8a5fdee |
| P1 | IPC-P1-6 | sweep 换算 wrapping_sub 对齐 C u8 回绕——rc==0 保段不毁段（shm.c:187） | 48837e2d3 |
| P2 | IPC-P2-1 | 判定/效果分离维持；分工注释落 server.rs 模块头（随 P1-1） | 36e33530d |
| P2 | IPC-P2-2 | 本地 trait 改名 EventLoopTransport；send 拆 send_reply/send_async（sendnb vs asynsend3） | 00d98307f |
| P2 | IPC-P2-3 | 死代码消除：11 处零使用常量 + needs_cancel + ack_type + 恒真返回值 | 527634f65 |
| P3 | IPC-P3-1 | server 去 Box、migrate_block 去克隆、classify 注释核实 | 68d801722 |
| T | IPC-T-1 | migrate_block 挂起计数迁移测试 | a8ad04cb6 |
| T | IPC-T-2 | next_seq 十五位环绕测试（sem 三点 + shm 单点） | 056631b17 |
| T | IPC-T-3 | tests/integration.rs 四场景 + into_parts + 移除死依赖 minix-sys | 1878466bb |
| T | IPC-T-4 | sweep rc==0 环绕测试（随 IPC-P1-6） | 48837e2d3 |
| T | IPC-T-5 | run() 连续失败 panic 路径测试 | dda493544 |
| D | IPC-D-1 | README/plan 空壳表述更新为现状 | c50b848a0 |
| D | IPC-D-2 | plan:17 A-1 失效注记 + A-5 部分完成 | c50b848a0 |
| D | IPC-D-3 | A-7 锚点 :713-722→:729-739（风险行 + 契约表两处） | c50b848a0 |
| D | IPC-D-4 | 00/99 篇改写定稿 + plan §6.1 finalized（快照归下轮 doc-review） | 956d4ae2a |
| D | IPC-D-5 | doc 03 三处 MountTable 失真修正 + mib_tree.rs:16 注释成真（随 P1-1） | 36e33530d |

## 1 R1 期间的关键裁决（正文详见归档）

1. **do_semop 顺序分歧**（P1-2）：C 注释（sem.c:693）与 doc 06 都规定权限先于序号校验，代码拆分丢了顺序——裁决"文档对、代码漂移"，validate_ops 收回 perm 以签名钉序。
2. **C 整数回绕是契约**（P1-6）：u8 `rc-1` 回绕使零引用段存活；饱和减是过度修正。
3. **C 的 shmdt 刷 atime 怪癖**（P1-5）：POSIX 说该刷 dtime，ground truth 刷的是 atime——怪癖即契约，测试钉住。
4. **判定/效果分离维持**（P2-1）：Linux 的锁结构是 SMP 机制不搬；Redox scheme daemon 直改风格作对照不作模板；服务层是薄序列器，不发明判断。
5. **边界缝形状**（P1-1）：单一 IpcBoundary trait 约 18 动词（唤醒须 handler 内发出，效果枚举无法表达）；生产实现（thin minix-sys 包装）随 E-IPCWIRE。
6. **死依赖 minix-sys 移除**（T-3 顺带）：零引用依赖在并行线程 E1 在途期间阻塞本 crate 构建；MIB 接线时回归。

## 2 规则发现（R1，Step 5.7）

1. **纯函数化会吃掉检查顺序**：C 入口拆纯函数时，errno 优先级是隐式契约——拆分前抄 C 的检查顺序清单，为每个优先级对写回归测试（IPC-P1-2 实例）。
2. **C 的整数环绕是契约不是缺陷**：小类型回绕语义被"修好"即行为分叉（IPC-P1-6 实例）；与 translate 防线互补——它防照搬，这条防过度修正。
3. **文档-代码一致 ≠ 代码-ground truth 一致**：查漏必须以 C 源为锚点直查代码，文档只在两侧都过完后用来解释分歧归属（P1-3/4/5 三例皆"文档对、代码缺"）。
