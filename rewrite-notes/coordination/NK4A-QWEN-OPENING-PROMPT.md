# NK4-A qwen 接手开局 Prompt（复制本文件全文给 agent）

> 用法：用户把下面分隔线之后的全部内容粘贴给新会话。

---

你在 /home/xzhao/github/minix-rs 工作（rewrite 分支）。你接手 minix-rs
操作系统重写项目 NK4-A 任务（生产启动链首次翻绿）的执行阶段。你的能力
有限制，所以本 prompt 把任务拆得很细：**严格按步骤执行，每步完成后再进
下一步，不要跳步，不要自由发挥扩大范围**。

## 0. 开局动作（按序，缺一不可）

1. 读 `rewrite-notes/coordination/NK4A-TODO.md` 全文（任务书：
   状态快照、精确前沿、任务分解 A-E、技术陷阱、记录格式、常用命令、
   禁止事项）。它引用的 FIXLOG 路径与命令都可直接使用。
2. 读 `.review/zcode/edge1/FIXLOG.md` 的最后 300 行（「Fix #9 迭代8-18
   补记」——前人修复链与取证方法都在里面，你的修复也要这样记）。
3. 现场确认（全部通过才动手）：
   ```bash
   cd /home/xzhao/github/minix-rs
   pgrep -f '[q]emu-system' && pkill -f '[q]emu-system'
   git log --oneline -3     # 应看到 4a6570d7f 或更新
   git status --short       # AI-chats/daily.todo.md 与 tmp/nk4a/vars.fd 的改动是既有的，勿动
   ```
4. 在 `rewrite-notes/coordination/NK4A-QWEN-WORKLOG.md` 顶部
   写一段会话开场（日期、你从哪个 commit 开始），然后进入 Task A。

## 1. 总目标与停止点

- 目标：真机串口出现 `minix-rs rc: minimal boot script marker`。
- 当前断点（已实证）：RS 已深入运行自己的 main，VM 在服务某次页故障时
  静默停滞——串口最后事件是 `nk4a: vm-pf recv` **没有**跟随
  `nk4a: vm-pf bytes`，RS 停 RTS_PAGEFAULT，无 panic。头号嫌疑：VM 的
  `handle_pagefault` 走了 `Ok(Suspended)`（挂到 VFS I/O 队列，但 VFS
  未启动，死等）。
- 到达目标后**立即停止**并记录（Task E）；探针清理、run_all 接线、
  账本更新不是你的工作，做了算越权。

## 2. 十条铁律（违反即返工）

1. 修任何代码前：读目标行 ±5 行 + grep 确认现状；一次只修一个缺陷；
   修后 grep 验证。
2. 一个逻辑单元一个 commit；commit 前宿主测试必须全绿。
3. 宿主测试计数不得下降（基线：kernel 808 / arch 241 / VM 525 /
   RS 350 / minix-rt 57 / minix-sys 315）。
4. 修复必须对照 C 源（`minix3/minix/` 只读），commit 信息与 FIXLOG 写明
   C 对位（文件:行）。
5. 每个修复在 FIXLOG 追加一条（格式见 TODO §6.2，**必须含「测试
   （防回归）」小节**，写明判别性断言）；每个 Task 在 WORKLOG 追加一节
   （格式 TODO §6.1）。
6. 新增探针必须限次（AtomicUsize cap）、必须 `#[cfg(not(feature =
   "mock"))]` 门、注释标「task1-close 裁决删除」。
7. 禁止：push、reset/rebase/force-push、动 `minix3/`、动分支
   `nk4a-agent-wip`/`nk4a-review-docs`、动 `os/etc/rc` 与冒烟脚本、
   删除既有探针、整仓 cargo fmt、删改 FIXLOG 历史。
8. 真机构建 IMG-EXIT 必须 = 0 才跑 QEMU；QEMU 起跑前必须
   `pkill -f '[q]emu-system'`（方括号写法）。
9. 修复验证必须真机**两次独立复跑**（PIT 抢占时序敏感，单次通过不算）。
10. 不假完成：被卡住就如实写 BLOCKED（含证据与已试方案），不要用
    stub/绕过/删检查来制造"完成"。同一问题尝试 3 轮仍无定性 → 停下
    写 BLOCKED，转入下一个独立任务或结束会话。

## 3. 常用命令（照抄；细节与更多命令见 TODO §7）

```bash
# 宿主测试（在 os/ 目录；换 -p 跑其他包）
cd /home/xzhao/github/minix-rs/os
docker run --rm -v "$PWD:/work" -w /work -m 2g minix-ci:1.94 cargo test -j 1 -p minix-vm

# 组装镜像（必须 IMG-EXIT=0）
cd /home/xzhao/github/minix-rs/os
(ulimit -v 3145728; cargo run -q -p xtask -- image --arch x86_64 --release); echo IMG-EXIT=$?

# 跑真机（RUN 换成轮次名，如 c17a；串口写到 tmp/nk4a/serial_<RUN>.log）
pkill -f '[q]emu-system'
rm -f /home/xzhao/github/minix-rs/tmp/nk4a/serial_<RUN>.log
bash /home/xzhao/github/minix-rs/tmp/nk4a/run-qemu.sh <RUN>
sleep 45
grep -v "vs0x" /home/xzhao/github/minix-rs/tmp/nk4a/serial_<RUN>.log | tail -60
pkill -f '[q]emu-system'
```

探针写法（VM 侧，跟着 vm_server.rs 里既有 `nk4a: rcv` 探针抄）：
```rust
#[cfg(not(feature = "mock"))]
{
    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
    static P_LOG: AtomicUsize = AtomicUsize::new(0);
    if P_LOG.fetch_add(1, AtomicOrd::Relaxed) < 8 {
        crate::bootmark::mark(&alloc::format!("nk4a: <名字> a={:#x}\n", a));
    }
}
```
内核侧直接用 `crate::ipc::probe_mark("nk4a: <名字>\n")`（全局 8 次上限，
省着用）。

## 4. 任务清单（严格按序；每个 Task 的详细步骤/判据/文件在 TODO §4）

- **Task A（当前）**：定位并修复 VM 页故障服务停滞。步骤 A1-A8：
  A1 读 vm_server.rs 1740-1860 列出跳过 bytes 打印的出口 →
  A2 读 cow_exec_pf.rs + memtype.rs 回答「ANON 区域能否 Suspended」→
  A3 查 VM 帧池大小写进 WORKLOG → A4 每个非成功出口加限次探针 →
  A5 复跑定性（**结论先写 WORKLOG 再修**）→ A6 按定性修（对照 C 的
  minix3/minix/servers/vm/pagefaults.c）→ A7 宿主全绿 + 真机两次复跑 →
  A8 FIXLOG+WORKLOG 补记。
- **Task B（条件触发）**：仅当 A 后真机出现 `boot.rs:1054` panic 才做，
  步骤见 TODO §4。
- **Task C**：RS boot 完成推进（init_fresh step0-4 全过、进入 run()
  主循环）。新断点一律走 TODO §8 取证循环。
- **Task D**：init execve /etc/rc 推进。
- **Task E**：rc marker 出现 → 记录 → 停止。

## 5. 会话结束前的交付检查清单

- [ ] WORKLOG 每个 Task 一节，状态字段不撒谎；
- [ ] FIXLOG 每个修复一条，含「测试（防回归）」小节；
- [ ] 所有 commit 在 rewrite 分支本地（未 push）；
- [ ] 真机证据 serial 日志已归档：
      `git add -f tmp/evidence/<目录>/`
      （*.log 被忽略必须 -f），commit 信息说明对应轮次；
- [ ] 最终汇报：以 Task 粒度列出 状态/根因/修法/commit/证据行，
      含未决问题与下一步建议。

---

上面分隔线之前的内容是给用户的使用说明，不用粘贴。
