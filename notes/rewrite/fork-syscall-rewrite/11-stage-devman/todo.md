# 11-stage-devman Rust 实现架构级 Review TODO（第一轮）

> **来源**：2026-09-15 code-excellence 首轮架构审查（用户指令：查漏补缺优先，再做架构深审；scan-only 轮，未改任何生产代码）。
> **范围**：一等对象 `os/servers/devman/src/`（18 文件 4694 行，78 个单元测试）；客户端面 `os/libs/minix-devman-client/`（3 文件 395 行，6 个测试）与 `os/libs/minix-sys/src/devman_client.rs`（423 行）+ `usb_model.rs`（437 行，12 个测试）；C ground truth `minix3/minix/servers/devman/`（4 .c + devman.h/devinfo.h 共 1013 行）与 `minix3/minix/lib/libdevman/`（generic.c 275 行 + usb.c 301 行）。libvtreefs 使用面（1642 行）经文档引用核对，未逐行重审（上轮 CONVERGED 覆盖）。
> **方法**：三向对账（C ↔ 15 篇文档 ↔ Rust，缺生产件与缺语义分档）→ 四层深审（整体组合 → 模块边界 → trait seam → 函数与数据结构，每层回答"如果今天重写会怎么设计"）→ 对照 Redox（用户态驱动 + scheme + pcid，联网核验 2026-09-15）/ Linux 驱动核心（sysfs 属性 + uevent + udev）/ Rust 社区惯例，每项改进给出至少两个候选方案。
> **定位**：不复写 plan.md；跨 stage 条目唯一入口是 `../edge_todo.md`，本文档 §6 只留双向指针。
> **状态（2026-09-15）**：首轮 scan-only 完成。**定性结论**：语义面质量高——四条 DEVMAN 消息、事件队列、输出缓冲、设备树、引用计数级联、RS 握手、客户端序列化逐项对得上 C，且修复了 C 的 switch 无 break 级联缺陷（`[ARCH:A-3]`，dispatch.rs:1-13）；缺口集中在两处——**生产执行半整层缺席**（传输、装配、wire 编码，与 STATE.md 既有 backlog 一致），以及**失败路径的三处新缺陷**（错误处理引入了 C 不存在的中间状态）。**定量**：P1 × 5（其中 DM-P1-1 是正确性缺陷登记，按 code-excellence 边界不修，交 todo-fix）、P2 × 2、P3 × 3；edge 新登记 2 条（E-DMWIRE、E-DMCLIENT）、增补 4 处。

---

## 0. 审查结论速览

devman 的现状是"语义库完备、服务器未出生、出生时会有三处摔倒点"。84 个单元测试全部跑在纯函数与注入 seam 上，消息语义、树操作、事件协议与 C 逐行对得上；但 `main.rs:35-37` 是自旋停车，`VTreeFs::run`（vtreefs/mod.rs:322）与 `Server::handle_other`（server.rs:91）两条分派面并存且互不相通——真 handler 在后者，前者的 message_hook 五臂全空（hooks.rs:152-164）。更紧迫的是失败路径：一次同名重复 ADD 会让 id 分配器泄漏一个号，此后**所有** ADD 永久返回 EINVAL（DM-P1-1）；事件行长度预算没有按 C 的方式扣除前缀字节，使 Rust 的成功/失败分界与 C 不同，并让"设备已入树但事件从未发出"的半登记状态变得可达（DM-P1-4）。本轮把执行半接线并入既有 backlog 挂 edge（E-DMWIRE），把三处失败路径缺陷与两处装配架构问题立为 stage 内条目。

| 级别 | 条目 | 一句话 | 状态 |
|---|---|---|---|
| DM-P1-1 | ADD 失败路径泄漏设备 id | 一次重名/池满 ADD 后，后续所有 ADD 永久 EINVAL（正确性缺陷，登记不修） | ✅ 2026-09-15（发布点前统一回滚 `unwind_staged` + `rollback_id`；§2 DM-P1-1 Fix #1） |
| DM-P1-2 | 双分派面统一 + Reply 错误通道 | run/message_hook 空臂 vs handle_other 两套真相源；Reply 枚举吞错误 | ✅ 2026-09-15（Server::run 统一循环 + Incoming/错误载荷 Reply + message_hook 死链退役；§2 DM-P1-2 Fix #5） |
| DM-P1-3 | 启动序列裁决 | Server::new 急切建树 vs C 懒 mount 触发；FirstGuard/InitCtx 成死置 | ✅ 2026-09-15（懒建树：`Option<DeviceTree>` 即守卫，ensure_devices 首挂触发；InitCtx/init_hook/FirstGuard 回调仪式随 A-1 退役；§2 DM-P1-3 Fix #6） |
| DM-P1-4 | 事件行预算记账对齐 + 半登记状态 | 预算未扣 "ADD "/"REMOVE " 前缀；事件失败留已入树无事件的设备 | ✅ 2026-09-15（ADD_STRING/REMOVE_STRING/EVENT_ID_SUFFIX_LEN 常量 + generate_child_path 预发布构行；§2 DM-P1-4 Fix #2） |
| DM-P1-5 | 文件表下沉，消灭全局 static | files.rs 唯一 unsafe 的 AssumeSyncCell 静态 + cookie 双表 → 内容挂 inode | ⬜ |
| DM-P2-1 | ADD 回复的 DEVICE_ID 无出口 | apply_reply 只写 RESULT 且清零其余字；C 回复是 RESULT+DEVICE_ID 双字 | ✅ 2026-09-15（apply_reply_with_id(msg, res, Option\<i32\>) 原语化；§3 DM-P2-1 Fix #3） |
| DM-P2-2 | handle_other 位置参数类型化 | word2/word3 裸参数 → DevmanMsg 枚举（协议入类型系统） | ✅ 2026-09-15（DevmanMsg::classify 解码 + handle_other(source, Option\<DevmanMsg\>)；§3 DM-P2-2 Fix #4） |
| DM-P3-1 | 死代码批次 | Attribute.data / Device.info / init / find_device / set_static_text 等 | ⬜ |
| DM-P3-2 | 测试与卫生批次 | wire 构造 helper 四份复制；is_dir 重复；parse_device 双返回 | ⬜ |
| DM-P3-3 | 每读分配 Buf + 吞错为 EOF | event_queue 每次读新建 4097 字节缓冲；ENOMEM 静默变 EOF | ⬜ |

**基线命令（2026-09-15 实测）**：

```bash
cargo test -p minix-devman          # test result: ok. 78 passed; 0 failed
cargo test -p minix-devman-client   # test result: ok. 6 passed; 0 failed
cargo test -p minix-sys             # test result: ok. 125 passed; 0 failed（devman 相关 12）
cargo test -p minix-types           # test result: ok. 197 passed; 0 failed
cargo clippy -p minix-devman -p minix-devman-client   # 两 crate 0 警告
bash tools/design-coverage-check.sh fork-syscall-rewrite --stage 11-stage-devman   # 15/15 PASS
```

---

## 1. 覆盖矩阵（查漏补缺结论）

### 1.1 测试对账现状

全量测试名对账由上轮 Codex review 完成并 CONVERGED（`.review/codex/devman/STATE.md`，15/15 篇，2026-09-04），本轮复核两点：其一，README.md 测试计数表声称的 78 与实测一致；其二，逐文件清点后的**缺测面**融入本 todo 各条目验证节——`server.rs:174-202` 的 `answer_bind`/`answer_unbind`（Server 级 RS 回复路径）无直接测试（bind.rs 只测了 response 半段的纯函数）；`hooks.rs:152-164` 空臂 hook 无测试（功能上已被 handle_other 取代，随 DM-P1-2 一并处置）；`event_queue.rs:78-83` `read_static` 的 Buf ENOMEM 错误路径无测试；`files.rs` 对 Events 类 cookie 的 `unregister` 无测试；`main.rs` 无测试（停车循环，随接线条目补装配测试）。

### 1.2 三向矩阵：C ↔ 文档 ↔ Rust

逐符号对账（基准 = devman C 服务端 4 .c + libdevman 2 .c 的全部对外符号）的结论：

- **服务端语义面无缺口**。`do_add_device` ↔ `add_device.rs:36 do_add`（含 STATIC-only 属性循环、devman_id 文件、出生引用与成员引用两笔账，device.c:365/394 对应）；`do_del_device` + get/put/del 级联 ↔ `del_device.rs`（BOUND→ZOMBIE 单向转换、REMOVE 事件先于状态变更、父引用级联，device.c:424-515 对应）；`do_bind_device`/`do_unbind_device` ↔ `bind.rs`（RS-only 门、owner 转发、驱动非 0/19 结果处理，bind.c:7-104 对应）；`devman_event_read` 的两读排空协议 ↔ `event_queue.rs:56`（device.c:142-168 对应）；`devman_generate_path` 的预算递归 ↔ `device_tree.rs:197`；buf 的 skip/cap 漏斗 ↔ `buf.rs`（buf.c:15-128 对应）。
- **有意行为修正均有 [ARCH] 标注且成立**：switch 无 break 的级联（main.c:46-58）→ 单处理器分派（dispatch.rs:1-13）；`panic` 家族 → `Err`（A-7）；`add_inode` 失败不检查（device.c:373-375）→ 错误传播。DM-P1-1 指出其中一处修正引入了新失败模式，见该条目。
- **客户端与 USB 建模面无缺口**：`serialize_dev` 字节布局 ↔ `minix-sys/src/devman_client.rs:101 encode_device`（含 subsystem_offset/req_nr 两个 C 从未初始化的字段的卫生零处理）；`devman_handle_msg` 的 ENODEV 不可分辨语义 ↔ 同文件 :224；USB 属性拼写的逐字节锁定测试齐全（usb_model.rs:301）。
- **生产件缺口整层一致**（与 STATE.md backlog 吻合，非新发现）：main 装配 + 生产传输（P1-6）、`Transport`/`ClientTransport`/`RsTransport` 各自只有一个测试实现（P1-1T/P1-10/P1-12）、SefHooks 生产实现等 minix-sef（OQ-1）。这些挂 `../edge_todo.md` E-DMWIRE / E-ISWIRE 增补，不在本 todo 重复立项。
- **本轮新发现的缺口两处半**：(1) ADD 成功回复的 DEVICE_ID 双字在 ipc 面无出口（DM-P2-1，新）；(2) 事件行预算记账与 C 分叉（DM-P1-4，新）；半处是 dirent/stat 的 wire 编码——代码与文档都说"属传输层"，但该传输层就是 VFS 协议本身，归 E-REQWIRE 增补，不算 stage 内缺口。
- **devmand 消费者**（doc 13）确认为外部契约不实现，与 plan 边界一致。

### 1.3 存量开口承接

`.review/codex/devman/STATE.md` Open Issues 的去向：P1-6（main park → 生产传输）、P1-1T/P1-10/P1-12（三个 transport trait 的生产实现）→ 并入 `../edge_todo.md` **E-DMWIRE**（新登记）；OQ-1（SefHooks/minix-sef）→ **E-ISWIRE** 增补（devman 为 minix-sef 第三消费方）；OQ-2（ReadHookFn len 参）已决保留、OQ-3（设备名空格拒绝）已决落地，均为闭环，本 todo 不再携带。执行各 edge 条目时按其约定同步回写本文件。

---

## 2. P1 条目（stage 内）

### DM-P1-1 ADD 失败路径泄漏设备 id：一次失败 ADD 堵死此后所有 ADD（正确性缺陷登记）

**问题**：`do_add` 的执行顺序是先 `alloc_id` 再做一切可能失败的步骤（add_device.rs:55 分配，:75-82 `fw.add` 失败映射 ENOMEM，:93 `add_static` 失败同样传播），而 id 槽位的合法性检查在 `DeviceTree::insert`：`(id.0 as usize) != self.devices.len()` 即 EINVAL（device_tree.rs:170-174），`alloc_id` 无条件 `next_id += 1`（device_tree.rs:155-159）。于是任何一次在 alloc 之后、insert 之前失败的 ADD（最现实的是**同名重复注册**：`fw.add` 返回 EEXIST → 映射 ENOMEM，add_device.rs:82）都会让 `next_id` 前进而 `devices` 长度不动，产生一个永久 id 空洞。下一个 ADD 拿到跳号的 id，insert 的密集检查永假——**此后该服务器上的所有 ADD 全部 EINVAL**。C 的对应路径没有这个状态：`next_device_id++` 同样先于 `add_inode`（device.c:371 vs :373），但 C 根本不检查 `add_inode` 失败（返回 NULL 直接赋给 `dev->inode.inode` 继续跑），不存在"失败但已跳号"的回复语义。这是 Rust 引入错误传播后新产生的失败模式，触发条件是驱动重试注册（USB 重枚举的标准行为）。

**影响**：正常重试路径可触发的永久拒绝服务。驱动侧（minix-sys/src/devman_client.rs:144 `add_device`）收到 ENOMEM 后按 C 客户端语义是 panic，Rust 侧返回 `ClientError::Rejected`；无论哪种，第二次同名注册之后设备注册面整体报废。

**修复方向**（登记供 todo-fix 执行，本条目本身不修）：
- **方案 A（推荐）**：把 `alloc_id` 挪到 `fw.add`（目录 inode 创建）成功之后、构造 Device 之前——inode 创建是失败概率最高的一步（EEXIST/ENOMEM/ENAMETOOLONG），让它发生在任何不可回退状态之前。`add_static` 循环中失败仍会留下已创建的目录 inode，需要补一个失败时 `fw.delete(dir_ino)` 的补偿删除（InodeTree::delete 已存在且对零引用节点立即回收，inode.rs:423-451）。
- **方案 B**：保留现状顺序，把 insert 的密集检查放宽为"id 未被占用且 ≥ len 或等于 len"（即允许空洞存在）。不推荐：空洞永久占据 Vec 槽位，且"next_id 与 Vec 长度耦合"的不变量被打破后，每个读 id 的地方都要重新论证。
- 无论哪个方案，补一个**失败后可继续注册**的回归测试：重名 ADD → Err；改名 ADD → Ok。当前测试集四条 add 用例（add_device.rs:206-308）全部单发成功或单发失败，无重试序列。

**验证**：`cargo test -p minix-devman` 新增重试序列测试；故意构造重名 → 改名 → 第三次注册成功且 id 连续。

### DM-P1-2 双分派面统一：run/message_hook 与 handle_other 两套真相源 + Reply 无错误通道

**问题**：同一个"非文件系统消息"入口存在两套并行机制。(a) `VTreeFs::run`（vtreefs/mod.rs:322-355）的 `Request::Other` 臂调 `self.other` → `FsHooks::fire_message` → `devman_message_hook`（hooks.rs:152-164），而该 hook 五个 match 臂**全部是空占位体**——若生产接线按 doc 01 的 C 形状（run 循环 + message_hook）走，DEVMAN_ADD 等四条消息会被静默吞掉。(b) 真正的 handler 装配在 `Server::handle_other`（server.rs:91-170），完全绕过 run。同时 `run` 的 Reply 枚举没有错误通道：Mount/Lookup/Readdir 失败被吞成 `Mounted(Ino(0))`/`Found(Ino(0))`/空表（vtreefs/mod.rs:327/335/343），Read 失败走 `err_marker` 五字节哨兵（vtreefs/mod.rs:362-365，注释自认"VecTransport-level tests assert on this marker"）——错误语义在测试替身上能表达、在真实传输上没有对应物（fsdriver 回复本有 status 字）。C 的参照是单一 `fsdriver_task` 分派表（table.c 17 槽，vtreefs/mod.rs:316-321 自引），没有第二入口。

**影响**：生产接线时选错入口就是静默丢消息；err_marker 哨兵会被不知情的实现者当成真实数据协议。这是"出生前必须拆掉的一颗雷"，属于 STATE.md P1-6 的前置结构问题。

**建议**：
- **方案 A（推荐）**：`Server` 吞并 run——新增 `Server::run(&mut self, transport: &mut impl Transport)`，把 `Request::Other` 臂改为调 `self.handle_other`（把 m_type/m_source 连同解出的载荷词传入），其余臂不变；`Reply` 枚举每个变体改带 `Result<_, Errno>`（`Mounted(Result<Ino, Errno>)` 等），删除 `err_marker` 与 `VTreeFs::other`/`devman_message_hook`/`MessageHookFn`/`FsHooks.message_hook` 整条死链（hooks.rs:63、75、121-125、152-164）。这同时消灭 DM-P1-3 之外的最后一块钩子残留，与 Redox `Scheme` trait 的单循环多 opcode 形状对齐（redox-scheme 的 scheme crate：一个事件循环 match 全部请求码，无旁路 hook）。
- **方案 B（否决）**：保留双面，用文档钉死"传输必须把 Other 路由到 handle_other"。两个真相源之间的约束靠注释维持，正是 translate 防线要消灭的形状；且空臂 hook 留在代码里就是给下一个接线者挖的坑。
- 前置说明：本条动 Reply 形状只影响测试替身与未来传输层（尚未存在），外部可观察行为零变化，属 Refactor。

**验证**：`cargo test -p minix-devman` 78 条基线全绿 + 新增一条端到端用例：VecTransport 脚本里塞一条 DEVMAN_ADD_DEV 的 Other 请求，断言设备入树且回复携带 DEVICE_ID；grep 确认 `err_marker`/`devman_message_hook` 零残留。

### DM-P1-3 启动序列裁决：急切建树与懒 mount 两套机制并存，FirstGuard/InitCtx 成死置

**问题**：C 的树是**懒**建的——`run_vtreefs` 阻塞等 VFS mount → `fs_mount` 调 `init_hook`（mount.c:24-25）→ `static int first` 守卫（main.c:37-42）→ `devman_init_devices`。Rust 的 `Server::new` 在构造时就调 `DeviceTree::new`（server.rs:59-66），树在没有任何 mount 请求时就存在；而为此场景预制的 `FirstGuard`（hooks.rs:227-243）与 `InitCtx::request_init`（hooks.rs:36-55）在生产装配里**零消费**——`VTreeFs::mount` 会触发 `fire_init`（vtreefs/mod.rs:174-178），`request_init` 置的 flag 没有任何生产读者（grep 仅测试 `init_hook_wires_to_first_guard`，hooks.rs:318-332）。两套启动形态各自留了一半：急切建树占了真装配位，懒触发机制占了测试位。时序差异本身外部不可观察（mount 是第一个到达的消息，mount 回复前 init_hook 已同步执行完），所以这是"二选一并删除另一套"的自由度，但现状是两套都留。

**影响**：接线实现者面对 Server::new（已建树）与 mount 臂（声称触发建树）会不知道信哪个；FirstGuard 的存在让人误以为 mount 触发路径是活的。

**建议**：
- **方案 A（推荐）**：改为 C 同构的懒初始化——`Server::new` 只建 `VTreeFs`，`DeviceTree::new` 与 events 文件注册挪进"首个 mount 请求"臂：`fire_init(ctx)` 后检查 `ctx.init_requested()` + `FirstGuard::enter()`，成立才建树（FirstGuard::enter 恰好就是 C 的 `static int first` 逐字对应物，hooks.rs:236-243）。DM-P1-2 的 `Server::run` 是承载它的自然位置。好处：文档 01/02 描述的启动主线保持为真，mount 失败（EINVAL）时树不会先建出来，`DeviceTree::new` 的 ENOMEM 有正确的报告时机（mount 臂内），与 C `panic("init_inodes failed")` 的位置（vtreefs.c init 序列，hooks.rs:266-267 引）对应。
- **方案 B**：接受急切建树为定案，删除 `FirstGuard`/`InitCtx::request_init` 机制与 `VTreeFs::mount` 的 fire_init 调用，`[ARCH: New]` 标注（doc + design + code 三处）。不推荐：要动三处标注与两篇文档，换来的简化不如方案 A 自然——FirstGuard 本来就是 C 语义的忠实建模，问题只是装配没接上。
- 两案共同的验收点：mount 前对 `devices`/`events` 的 lookup 必须是 ENOENT（C：inode 尚不存在）。

**验证**：`cargo test -p minix-devman`：新增"mount 前 lookup devices → ENOENT；mount → 建树 → lookup → Ok；二次 mount → 不重复建树（事件队列 cookie 不变）"序列测试；grep `FirstGuard` 生产调用方非零（方案 A）或零残留（方案 B）。

### DM-P1-4 事件行长度预算记账与 C 分叉 + 事件失败半登记状态

**问题**：两层偏差互相放大。(a) **预算记账**：C 的事件行构造是先 `strncpy` 前缀再让 `devman_generate_path` 在**已含前缀**的 buf 上做预算检查（`strlen(buf) + strlen(name) + strlen(sep) + 1 > len`，device.c:61；ADD 路径 buf 起点是 `"ADD "` 4 字节，REMOVE 是 `"REMOVE "` 7 字节，devman.h:44-45；预算同为 `DEVMAN_STRING_LEN - 11` = 117，device.c:91/:124）。该检查自带 1 字节 NUL 余量，故 C 的 buf 总长（含前缀）封顶 116，最终行 116+11=127 字符+NUL，恰好装满 `char data[128]`（devman.h:67）——C 的有效路径预算是 ADD 112 / REMOVE 109。Rust 的 `generate_path` 检查同构（device_tree.rs:223，同样留 NUL 余量），预算只量裸路径（调用传 117，add_device.rs:109、del_device.rs:125）——路径上限 116，前缀与 id 后缀另行拼接（add_device.rs:110-115、del_device.rs:126-131），行最长 4+116+11=131（ADD）/ 7+116+11=134（REMOVE）。后果有两段：路径 117 及以上，Rust 在 generate_path 以 ENOMEM 失败、C 以 panic 失败（device.c:93-95，同界失败，可接受）；**路径 113..116（ADD）/ 110..116（REMOVE）时两者分叉**——C 在 generate_path 就失败（panic），Rust 却通过 generate_path、直到 `Event::new` 的 128 上限（structs.rs:83）才以 ENAMETOOLONG 失败，成功/失败分界与 C 不同。(b) **半登记状态**：`Event::new` 失败发生在 `tree.insert` 与 `get_device(parent)`（add_device.rs:102-106）之后，错误一路传到 `handle_other` 回给驱动，但设备已经入树、父引用已经加上、目录已经可见——驱动收到失败回复，设备却存在且永无事件（devmand 永远看不到它）。C 没有这个中间态：panic 意味着进程死亡，不存在"回复失败但设备留下"的世界。

**影响**：输入空间极窄（路径 >112 字节，约 104+ 字符的设备名），但一旦命中就是行为分叉 + 不可回收的僵尸设备。同类问题在 `do_del` 侧较轻（generate_path 失败时尚无状态变更，等价 C panic → Err，可接受）。

**建议**：
- **方案 A（推荐）**：预算按 C 扣减前缀——`generate_path` 调用改为 `DEVMAN_STRING_LEN - 11 - 4`（ADD）与 `DEVMAN_STRING_LEN - 11 - 7`（REMOVE），使路径上限回到 112/109、行恒 ≤127，`Event::new` 的拒绝分支退化为不可达断言。同时把事件行的构建挪到 `tree.insert` **之前**（路径可以从父设备路径 + 新名字直接拼出，不需要设备先入树——C 的 generate_path 只依赖框架 inode 名字链），失败即整体失败、无任何状态残留。这个顺序调整同时是 DM-P1-1 方案 A 的组成部分。
- **方案 B**：维持预算 117，把 `Event::new` 的上限视为对 C 的有意收紧（接受 113..117 段行为差异），仅在 `do_add` 失败分支补回滚（unlink_child + remove + put parent）。不推荐：行为分叉仍存在（C panic vs Rust 错误回复本可统一为"同界失败"），且回滚代码比顺序调整更易腐。
- 文档同步：07 §3.4 与 03 §3.5 关于"hardening / never truncated"的描述需按定案更新（预算数字与失败时点）。

**验证**：`cargo test -p minix-devman`：边界序列测试——路径长 112/113/110/111 的四个名字在 ADD 与 REMOVE 下的成功/失败与 C 一致（≤ 界成功、> 界失败且**零状态残留**：tree.get 为 None、父 refcount 不变、无事件残留）。

### DM-P1-5 文件表下沉：消灭 crate 内唯一的 unsafe 全局 static 与 cookie 双表

**问题**：可读文件的内容管理是三件套——`static FILES: AssumeSyncCell<RefCell<FileStore>>`（files.rs:107-108，crate 内唯一 unsafe 块 ：118）、以 `usize` 索引为 cookie 的旁表（`FileBinding.cookie`，structs.rs:105）、以及 `dispatch_read` 的查表分派（files.rs:133-150）。这套形状是从 C 的 `devman_inode { read_fn, data }` 直译来的，但 C 用它是为了绕"回调无法携带环境"的 C 限制；Rust 里这恰恰是类型系统能直接表达的东西：**内容就是 inode 的载荷**。C 的两个 `read_fn` 目标本质上只有两种（事件队列、静态文本），与 `Inode` 一一对应。旁表还引入了第二个生命周期问题：`del_device` 要按 `Attribute.binding.cookie` 手工 unregister（del_device.rs:97-99），框架删除与旁表注销的成对纪律靠调用方维持——DM-P1-1 的失败路径正是这种成对纪律出错的温床。

**影响**：unsafe 与单线程论证的存在本身就是维护税（lib.rs:8-14 的单线程模型文档有一半是为这个 static 服务的）；inode 与内容分离意味着"删了 inode 但忘 unregister"类缺陷永远可写。

**建议**：
- **方案 A（推荐）**：内容挂 inode——`Inode` 增加 `content: InodeContent` 字段，`enum InodeContent { Dir, Static(String), Events(EventQueue) }`（框架层通用概念：C 的 read_fn+data 本义就是 per-inode 内容，procfs 同款）。`register_file`/`FileStore`/cookie/unsafe static 全部删除；`add_static` 变成一次 `fw.add`（content 随建）；`del_device` 的属性清理收缩为 `fw.delete`（内容随 inode 释放，成对纪律由所有权保证）；`VTreeFs::read` 直接 match `node.content`，`ReadHookFn`/`fire_read`/`dispatch_read` 整条 hook 链删除。与 C 的对照是**语义同构**而非偏离：read_fn 指针 + void* data 的运行时多态被 enum+match 的封闭多态替代，恰是模式 65 的标准修复路径。InodeTree 已有的 `cbdata: usize` 字段（inode.rs:113，C i_cbdata 对应物）在 devman 内不再有用户，随本条一并评估删除（框架是 devman 专属内联，无第二消费者，vtreefs/mod.rs:7-10 自注）。
- **方案 B（保守）**：保留 cookie 机制，仅把 FILES static 移为 `Server` 字段、`ReadHookFn` 从 fn 指针改为 Server 可达的枚举分派。unsafe 消失了，双表与成对纪律问题原样保留。仅当方案 A 的测试改写量被判定过大时退而取之。
- 文档同步：06 篇（event-buf 的 files/cookie 机制）与 08 篇（unregister 扩展）需按定案更新；这是本 todo 中唯一动 02 篇框架结构的条目，`Inode` 形状变化在 02 §3 的 design 侧同步。

**验证**：`cargo test -p minix-devman` 78 条基线改造后全绿（其中 files.rs 的 2 条测试转为 InodeTree content 语义）；`grep -c unsafe` 全 crate 归零；读路径端到端测试（events 排空 + 静态文本 + 偏移语义）数值与现状逐字节相同。

---

## 3. P2/P3 条目（stage 内）

### DM-P2-1 ADD 成功回复的 DEVICE_ID 双字在 ipc 面无出口

**问题**：C 的 ADD 成功回复是双字——`msg->DEVMAN_DEVICE_ID = dev->dev_id`（m4_l2，device.c:270）加 `do_reply` 写 RESULT（m4_l1，device.c:213-218）。Rust 的 `apply_reply(msg, res)` 只写 m4l1 并把其余字**清零**（ipc/message.rs:64-75，注释自认 hygienic zeroing），没有任何函数能产出"RESULT + DEVICE_ID"双字回复；`OutAction::Reply{outcome: Ok(DeviceId)}`（server.rs:33-37）携带的 id 在 ipc 面没有落点。相位表自己都写了"(`DEVICE_ID` on ADD success)"（ipc/message.rs:13），代码却表达不了。
**影响**：传输实现者到时只能绕过 apply_reply 手拼 union，05 篇锁定的"回信戳"契约出现第二个真相源。
**建议**：方案 A——`apply_reply` 增加 `apply_reply_with_id(msg, res, id: Option<DeviceId>)`（或拆两个函数），ADD 臂专用；DEL/BIND/UNBIND 用现有单字版。方案 B——OutAction 的消费方自行拼装。推荐 A：05 篇的相位表就是按"本模块是角色视图"写的（ipc/message.rs:16-17），双字变体是它承诺过的表达力。
**验证**：单测断言 reply 的 m4l1=0 且 m4l2=id、m_type=DEVMAN_REPLY，与 device.c:270/:215 的字段逐一对上。

### DM-P2-2 handle_other 位置参数类型化

**问题**：`handle_other(m_type: i32, source: Endpoint, body: &[u8], word2: i32, word3: Endpoint)`（server.rs:91-98）——m_type 裸 i32、word2/word3 无语义名，与 ipc/message.rs 辛苦建立的"角色视图"（grant_id/device_id/request_endpoint，ipc/message.rs:29-57）脱节：分派器已经知道 Handler 枚举，参数却还是同一批裸词。模式 16/17 的精神（裸整数表达语义 ❌）在模块内部也没贯彻到底。
**建议**：方案 A——`enum DevmanMsg { Add { body: Vec<u8> }, Del { device: DeviceId }, Bind { device: DeviceId, driver: Endpoint }, Unbind { device: DeviceId, driver: Endpoint } }`，由传输分类器构造（分类器还不存在，正好一起定形状），`handle_other(&mut self, source: Endpoint, msg: DevmanMsg)`。方案 B——保留签名加 doc。推荐 A，与 DM-P1-2 的接线条目同批实施（同一调用点改造，不二次返工）。
**验证**：dispatch 四分支测试改为构造枚举；`cargo test -p minix-devman` 全绿。

### DM-P3-1 死代码批次（每项：为何死 + 消除影响）

| 对象 | 锚点 | 为何死 | 消除影响 |
|---|---|---|---|
| `Device.info: Option<ParsedDevice>` | structs.rs:131；唯一写入点 add_device.rs:70 恒 `None` | 全 crate 无读者；C 侧 `dev->info` 只写（device.c:369）只 free（:515）零读取——持有序列化描述的职责已由 wire 即时解析替代 | 删字段；行为零变化 |
| `Attribute.data` | add_device.rs:154 写入，全 crate 无读者（07 注释声称"attr readers see full data"，add_device.rs:157-160——并不存在 attr reader） | 读路径走 FileStore.text（即 C 的 st_inode.data 截断文本）；Attribute 的 data 无任何消费者 | 删字段（binding 保留） |
| `pub fn init() {}` | lib.rs:41，无调用方（main.rs 不调） | C 无对应物；占位从未兑现 | 删除 |
| `DeviceTree::find_device` | device_tree.rs:148-150，仅测试调用（:316） | C 双函数（`_find_dev` 递归 + 包装）在 Rust 是同一函数的别名 | 删除，测试改用 `find` |
| `FileStore::set_static_text` | files.rs:94-104，无生产调用方（仅本文件测试 ：168） | C 无属性更新路径（attrs add-only，devman_dev_add_static_info 只有 add 方向，device.c:313-339）；07 也不更新 | 删除；若 DM-P1-5 定案则随 files.rs 整体消失 |
| `devman_message_hook` 空臂族 | hooks.rs:152-164 + MessageHookFn :63 + fire_message :121-125 | 真路径是 handle_other（DM-P1-2 已立项），空臂是 07-09 落地后的残留 | 随 DM-P1-2 处置，不单列 |
| `VTreeFs::unsupported` | vtreefs/mod.rs:312-314，仅自身测试调用 | 显式 ENOSYS 占位无调用方；未接线的槽位"不存在代码"本就是封闭枚举的天然表达 | **OQ-1 上交**：AI 倾向删（表驱动下该状态无代码位置，函数本身是一枚"文档令牌"）；保留派理由是显式契约自述。本轮不擅自删 |
| `PNAME_MAX_LEN` | inode.rs:31 定义、mod.rs:32 再导出，零功能引用 | C 值锚点常量，仅文档用途 | P3 卫生批随裁：移入注释或保留（轻微，不单独立项） |

### DM-P3-2 测试与卫生批次

(a) wire 构造 helper 四份复制：`wire_usb`（server.rs:222、add_device.rs:184）与 `wire_one`（del_device.rs:161、bind.rs:150）是同一 `serialize_dev` 布局的四份手搓版，且与 minix-sys 的 `encode_device`（devman_client.rs:101）第五份并存——`#[cfg(test)]` 公共 helper（或直接对偶消费 `encode_device`，让 parser 与 encoder 互为验证）收一处。(b) `is_dir` 双份（vtreefs/mod.rs:373 与 vtreefs/inode.rs:43）。(c) `parse_device` 返回 `(i32, ParsedDevice)` 双表达（wire.rs:110/144-151）：首元素在唯一生产调用点被丢弃（server.rs:101 `Ok((_, p))`），`ParsedDevice.parent` 已是同值——改为直接返回 `ParsedDevice`。三项皆行为零变化。

### DM-P3-3 每读分配 Buf 与吞错为 EOF

`EventQueue::read_oldest` 与 `read_static` 每次调用新建 4097 字节 `Buf`（event_queue.rs:61/79），C 是复用静态缓冲（buf.c:8-9）；`files.rs:133-150` 的 `dispatch_read` 对错误 `unwrap_or_default()`，ENOMEM 静默变 EOF（files.rs:140-144）。现实影响趋近于零（单线程小服务器，分配廉价；ENOMEM 不可达），方案 A 是 DM-P1-5 落地后 `Buf` 自然归属 EventQueue/读路径所有、复用即顺手；方案 B 是维持现状并留注释。倾向随 DM-P1-5 顺带处理，不独立排期。

---

## 4. 分层审视记录（如果今天重写会怎么设计）

| 层 | 现状 | 重写裁决 | 依据 |
|---|---|---|---|
| 整体组合 | devman = VTreeFS 内联（A-1）+ 设备树 + 事件面 + RS 契约 + 客户端（minix-sys），四 trait seam 注入 | **维持**。与 Redox 对照：Redox 无中央设备管理器——pcid 按配置表 spawn 驱动进程、设备以 scheme 命名空间呈现（doc.redox-os.org/book/drivers.html）；Minix3 的 devman = 设备树 FS + 事件文件 + RS bind 握手，恰是 Linux sysfs 属性 + uevent + udev 的微内核同构，架构形态本身是三个参照系里最标准的 | devmand ↔ udev、events ↔ uevent 的对应使"设备生命周期以文件事件广播"这一核心设计无须重审 |
| 模块边界 | DeviceTree（服务面）与 InodeTree（框架面）双树，`Device.binding` 手工同步 | **维持双树**（C 同构：devman_device.inode ↔ vtreefs inode），同步纪律的薄弱点（cookie 成对、失败残留）由 DM-P1-1/P1-4/P1-5 三个条目收敛；文件内容按 DM-P1-5 并入框架面后，"旁表"这一第三账本消失 | 删掉任何一个树都会把设备语义或框架职责搅在一起；C 作者的分层面是对的 |
| trait seam | Transport / SefHooks / RsTransport / ClientTransport 四 seam，各只有测试实现 | **维持形状、收紧数量**：SefHooks 若 minix-sef 长期不到场则按模式 80 处置（E-ISWIRE 增补里带裁决点）；Transport 与 Server::run 的统一见 DM-P1-2；不预建更细的抽象（Redox 的 scheme trait 之所以成立，是因为它有几十个实现者；这里每个 seam 只有一个未来实现者，形状以消费者倒推即可） | 模式 79-82 过零违例；唯一疑点是 SefHooks，已挂 edge |
| 函数与数据结构 | DeviceState/DeviceId/Ino/Endpoint newtype、Errno 贯通、unsafe 仅 files.rs 一处 | **维持**，unsafe 归零由 DM-P1-5 承接；`Buf` 的 skip/cap 漏斗与 C 逐行对得上（buf.c:61-84 ↔ buf.rs:53-71）是全 crate 质量最高的直译段，无需动 | 模式 16/17/65 扫描：无裸整数哨兵、无 C 式签名残留、unsafe 密度 1/4694 行（消 DM-P1-5 后为 0） |

---

## 5. Open Questions

| OQ ID | 对象 | 两侧方案 | AI 倾向 |
|---|---|---|---|
| OQ-1 | `VTreeFs::unsupported` 去留（DM-P3-1 表） | 删：封闭枚举下无代码位置，函数是文档令牌 / 留：显式 ENOSYS 契约自述 | 倾向删，证据已足，随 DM-P3-1 批次由用户点头后执行 |

（minix-devman-client 处置不设 OQ——它涉 16-stage-drivers 引用，已直接登记 `../edge_todo.md` E-DMCLIENT 走跨 stage 裁决。）

---

## 6. 跨 stage 双向指针

跨 stage 条目唯一入口是 `../edge_todo.md`，本轮登记/增补（2026-09-15）：

| edge 条目 | 类型 | 一句话 | 与本 todo 的关系 |
|---|---|---|---|
| E-DMWIRE | 新登记 | devman 生产接线四缺：server transport、请求分类器（VFS 面挂 E-REQWIRE）、装配半、client/RS 侧生产传输 | 承接 STATE.md P1-6/P1-1T/P1-10/P1-12；实施时消费 DM-P1-2/P1-3 的结构定案 |
| E-DMCLIENT | 新登记 | `minix-devman-client` 孤儿 crate 处置（与 minix-sys::devman_client 职责重叠、零依赖、16-stage-drivers 文档引用） | DM-P3-1 的跨 stage 部分 |
| E-REQWIRE | 增补 | devman 是 VTreeFS dirent/stat/REQ_* wire 的第三消费方 | §1.2 "半处缺口"的归属地 |
| E-ISWIRE | 增补 | devman SefHooks 是 minix-sef 第三消费方（OQ-1 承接） | §1.3 存量开口 |
| E-DSWIRE | 增补 | devman 客户端 `init` 的 `ds_retrieve_label_endpt` 消费面 | minix-sys ds.rs 落地时的接线清单 |
| E5 (h) | 增补 | devman 生命周期联调面（mount → 注册 → 事件 → bind → 删除） | E-DMWIRE 通电后的端到端验收 |
