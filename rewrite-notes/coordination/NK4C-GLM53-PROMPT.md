# NK4-C → GLM5.3 交接 Prompt（续-35·修模式② pt_bind）

> 将本文件整份复制给 GLM5.3 作为任务起点。你的上下文有限——**不要通读 WORKLOG 全文**，按下方「必读清单」定向读。

---

## 1. 项目一句话 + 环境

- **项目**：minix-rs = 用 Rust 重写 MINIX3 微内核（分支 `rewrite`），目标 x86_64 / aarch64 / riscv64 三架构跑通完整 OS。
- **workspace**：`/home/xzhao/github/minix-rs`（Rust workspace 根；OS 侧代码在 `os/` 下）。
- **硬约束**：`no_std` + `alloc`。VM server 包名 `minix-vm`（lib 名 `minix_vm`）。生产代码禁用只在 test 下可用的 API；`#[cfg(test)]`/`#[cfg(not(test))]` 分支要成对维护。

---

## 2. 你的任务（本轮唯一目标·不缩小）

**修复 aarch64「失败模式②」，让 aarch64 QEMU 打出 rc marker `minix-rs rc: minimal boot script marker`，且 x86_64 不回归。**

- 失败模式② = exec 成功后，子进程对首文本页 VA `0x200000` **无限重复缺页**（`memreq ... start=0x200000 ok=1` 洪流水位：基线 ~74015 次）。
- **失败模式①（间歇 ~4GiB OOM / Heisenbug）本轮明确不修**——它是与②正交的寄存器通路/陈旧栈值问题，不要在它上面耗时间。

---

## 3. 根因方向（已定谳·勿推翻方向）

**根因**：`os/servers/vm/src/exit.rs::handle_procctl_clear`（VMPPARAM_CLEAR 腿）在 `free_page_table()`（释放旧页表）+ `init_page_table()`（建新页表）**换根之后，缺少 C `exit.c:137` 的 `pt_bind` 对位腿 = 未重发 `sys_vmctl_set_addrspace`**。

后果：内核 `p_seg.phys_root` 仍指向已被 torn-down 的旧根 A；VM 把新镜像映射进新根 B；CPU 走 A → 永不命中 → 对 `0x200000` 无限 refault。

**旁证（同路径其它腿都做了 setaddr，惟 exec clear 腿跳过）**：
- fork：`os/servers/vm/src/fork.rs:395-401` — `gateway.sys_vmctl_set_addrspace(child_endpoint, child_root_phys, 0)`
- boot：`os/servers/vm/src/vm_server.rs:774-794` — `exec_bootproc` 内发 setaddr
- 内核侧权威：`os/kernel/src/syscall.rs:2911`（`vmctl_set_addr_space` 写 `p.p_seg.phys_root`）是 bootstrap 之外**唯一**写 phys_root 的入口；`dispatch_exec` 全程不碰 phys_root。

**为何 x86 未暴露、aarch64 暴露**：x86 `destroy()` 已接 `free_pt_page` 归还页表页 → `pt_new` 大概率复用同一物理页（A==B 巧合正确）；aarch64 `destroy()`（`os/arch/src/arm64/paging.rs:609-619`）只 `write_bytes(root,0,512)` 清零、**从不归还页表页** → B≠A → 分歧暴露为活锁。

---

## 4. WIP 实现（已写好·方向真机坐实·但 x86 回归未解）

**位置**：`tmp/nk4a/nk4c35-pt-bind-wip.patch`（untracked·134 行 / +53 −12 / 3 文件）。先 `git apply` 或直接照它改。

WIP 做了：
- `exit.rs`：`handle_procctl_clear` 加 `gateway: &mut dyn KernelGateway` 参数；`init_page_table()` 后取
  `<crate::pagetable::PageTable as crate::pagetable::Paging>::root_paddr(proc.page_table_mut()).0`
  并 `gateway.sys_vmctl_set_addrspace(endpoint, new_root_phys, 0)`（对位 fork/boot 写法）。
- `dispatcher.rs`：`dispatch_procctl` 从 ctx 解构出 `gateway` 并传入 `handle_procctl_clear`。
- `vm_server.rs`：测试 `test_vfs_transid_routes_to_procctl_clear` 装 `MockGateway`（默认 `DirectKernelCallTransport` 无法服务 setaddr→返 InternalError）+ 断言 `addrspace_sets` 恰发一次（target ep·virt=0）。

**WIP 已验证（真机·勿推翻方向）**：
- ✅ `cargo test -p minix-vm --lib`：**531 passed / 0 failed**。
- ✅ aarch64 `image --release` 编译通过。
- ✅ aarch64 QEMU 95s：**0x200000 风暴归零（0 vs 基线 74015）**·`sas-send` pt_bind 腿触发 11 次·exec 出的子真正取指运行新镜像·历史首次达 `init-state SingleUser`。

**⚠ x86 回归未解（你要解决的核心问题）**：装 WIP 后 x86 QEMU 在 `os/kernel/src/trap_dispatch.rs:955` 处 kernel page fault（vector 14）。
推测：x86 上 `init_page_table` 后新根**仅有用户态映射、无内核态映射**（kernel text/data），提前把这空根 bind 给内核 → 内核自身缺页。

**对照 C**：Minix3 `exit.c:137 pt_bind` 时序在 `do_rm`/`do_new` 内部；i386 的 kernel PDE **全进程共享**（`pagetable.c`），Direct Map 下内核永远可见，故 C 的 bind 不要求新根完整。minix-rs aarch64/x86_64 **无此共享 PDE** → 需等效方案。

---

## 5. 待你裁决的设计问题（何时/如何 bind 才双架构安全）

主要问题句：**`handle_procctl_clear` 末尾调 `sys_vmctl_set_addrspace` 后，新页表根是否已包含内核态映射？若否，应在何处确保补映射？**

候选方案（择一或组合，以真机为准）：
1. **补内核映射再 bind**（调整顺序：确认 `init_page_table` 的 `#[cfg(not(test))]` 分支确有 `map_kernel`（见 `os/servers/vm/src/vmproc/vmproc_handle.rs:386`）把内核窗口映进新根后，**再**发 setaddr；若 x86 新根确缺内核映射则须补齐）。
2. **走 x86 分支保留旧行为**（`#[cfg(target_arch=...)]` 分道·破坏统一性·次选）。
3. **在 `dispatch_exec` 末尾（而非 clear 末尾）补发 setaddr**（此时镜像已装载·新根完整·bind 安全）。

> 先静态读码 + 单跑 x86 判别新根是否含内核映射，再动手。不要在证据不足下盲改顺序。

---

## 6. 必读清单（定向读·控制上下文）

1. `tmp/nk4a/nk4c35-pt-bind-wip.patch` — WIP 全量。
2. `rewrite-notes/coordination/NK4C-WORKLOG.md` 文末 **§1.120续-34 / 续-34b / 续-35 交接节**（顶部「当前状态」有导读）。
3. C 对位：MINIX3 `exit.c:137`（`pt_bind`）、`pagetable.c`（i386 共享 kernel PDE）。
4. 旁证：`os/servers/vm/src/fork.rs:395-401`、`os/servers/vm/src/vm_server.rs:774-794`。
5. 换根本体：`os/servers/vm/src/exit.rs`（`handle_procctl_clear`）、`os/servers/vm/src/vmproc/vmproc_handle.rs:349`（`init_page_table`·test/非test 分支）。

---

## 7. 硬纪律（不可违反）

- **三件套（每个含代码改的单元必跑）**：
  1. `cd os && cargo test`（host mock：kernel 823 / arch 243 / vm-lib 531 全绿）；
  2. `cargo fmt`（+ 相关 target clippy 无新告警）；
  3. **aarch64 与 x86_64 双跑守 marker**：
     - `cd os && cargo run -q -p xtask -- image --arch aarch64 --release` 后 `timeout 95 cargo run -q -p xtask -- qemu --arch aarch64 --serial ../tmp/nk4a/<name>.serial`
     - x86_64 同理 `--arch x86_64`（单核 `-smp 1` 是已知绿态，别引入回归）。
- **fix-guard**：改任何一行前先读它 ±5 行上下文；**一次只改一条腿**，改完即验证，别一次堆多处。
- **探针 TEMP 回滚**：装的取证/诊断探针是临时的，收口前必须 `git checkout` 回滚，`grep <探针前缀> os/`=0。
- **commit 禁 `git add -A` / `git add .`**：只 `git add <明确路径>`。绝不碰 `AI-chats/daily.todo.md`。WIP patch（`tmp/`）保持 untracked。
- **未坐实不成修**：没有真机/测试证据支撑的假设不能进生产改动。
- **WORKLOG 续-36**：把你的分析 + 修复过程追加到 `NK4C-WORKLOG.md` 文末（标题含结果），方便原主线之后接手复验 + 撰写教学文档。

---

## 8. 产出格式

- 每轮在 WORKLOG 追加一节：`## §1.120续-NN …`，**标题里写清结果**（定谳/修复/回滚等）。
- **修复 commit message** 必须含：根因 / 方案 / 验证（哪些测试 + 哪个架构跑出了 marker + x86 不回归）。
- 每个含代码改的 commit 走一次 code-review。

---

## 9. 完成判据

- aarch64 QEMU 串口出现 `minix-rs rc: minimal boot script marker`；
- x86_64（单核）marker 不回归；
- 三件套全绿；WORKLOG 续-36 落笔；探针回滚、工作树净、commit 用明确路径。

> ⚠️ 目标不缩小：这只是终目标① 的 aarch64 一环；① 全部达成 = 三架构 rc marker，其后还有 18-stage 命令面与 minix3 tests 上机。
