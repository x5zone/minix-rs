# 07-stage-ds Rust 实现架构级 Review TODO（第一轮）

> 来源：2026-09-14 首轮架构级审查（cmd-04 主体 + full-review 的 coverage 强制方法查漏补缺）。本轮 scan-only，未改任何生产代码与测试。
> 范围：`os/servers/ds/` 全部 Rust 代码（18 文件，3572 行，crate 名 `minix-ds`）+ 契约面 `os/libs/minix-types` DS 段（types/com.rs、ipc/message.rs）+ minix-sys 依赖形态；C ground truth `minix3/minix/servers/ds/`（main.c 132 + store.c 679）+ `minix3/minix/lib/libsys/ds.c`（219）+ `minix3/minix/tests/ds/`（347）。
> 方法：先查漏补缺（C 22+4 符号 ↔ 13 篇文档 ↔ Rust 三向矩阵，coverage-extract 全量化重跑），再按「组合层 → 服务器内部 → 内核接缝 → wire 层 → 测试」五层深审，对照 Redox（联网核验情况见 §6，诚实标注）/OS 理论/Rust 社区惯例。设计基线 = 13 篇文档 + plan.md §5.3/§4（A-1..A-10）。
> 定位：不复写 plan.md；跨 stage 条目唯一入口是 `../edge_todo.md`，本文档只留双向指针（§5）。文档改写进度仍由 plan.md §6.1 承载。
> 状态（2026-09-14）：**判定层质量高、执行半整层缺席、三个行为级真发现**——stage 内新登记 5 条 P1 + 3 条 P2 + 3 条 P3，P0 为零；edge 新登记 E-DSWIRE 一条 + 两处增补。最大单一发现是 retrieve.rs 整个模块是孤儿文件（8 个测试从未编译运行）；其次是订阅匹配的类型预门缺掩码（C 语义被 IN_USE 公共位击穿）与删除通知半丢失。plan.md 的 A-1/A-8 与 §3.5 测试基线已过时（§3 逐条复核）。

---

## 0. 审查结论速览

一句话总结：DS 的判定层（verdict/plan/apply 纯函数分层）是全仓同类服务器里完成度与质量都最高的——错误码全走 Reject 枚举单点渲染、192 字节跨服 ABI 用偏移断言锁死、每个 unsafe 读都带臂纪律论证。但「三向对账」揭开了三层账：**一层编译账**（retrieve.rs 没被 lib.rs 声明，268 行实现与 8 个测试对编译器不存在，测试 89 声明实跑 81）；**一层语义账**（订阅匹配的类型预门漏了 DSF_MASK_TYPE 掩码，IN_USE 公共位让门恒真；删除路径绕过通知环，C 的「删也要叫醒订阅者」契约在接缝层丢失）；**一层接线账**（主循环 transport、grant/datacopy/notify 发送、minix-sys 客户端模块全部缺席——其中 A-2 正则引擎的延迟决策被证实阻塞了全部真实客户端场景，VFS 的订阅 pattern 全是元字符）。

| 级别 | 条目 | 一句话 | 状态 |
|------|------|--------|------|
| P0 | （无） | 判定层四臂门序与 C 逐一重对，零 P0；核对依据见 §4.0 | — |
| P1 | P1-1 | **retrieve.rs 孤儿模块**：lib.rs 未声明，268 行 + 8 测试从未编译 | ✅ 已修复 2026-09-15（Fix #1，见 §9） |
| P1 | P1-2 | **类型预门缺 DSF_MASK_TYPE 掩码**：IN_USE 公共位使门恒真，跨类型误置位/误通知 | open |
| P1 | P1-3 | **删除通知半丢失**：apply_delete 绕过通知环且不产出补发素材，C 的删除唤醒契约断 | open |
| P1 | P1-4 | **transport + handler 粘合 + grant/datacopy/notify 接线**（stage 内 seam，通电挂 E-DSWIRE） | open |
| P1 | P1-5 | **A-2 regex 引擎决策**：BadPattern 拒绝与 C 行为分歧，真实客户端 pattern 全含元字符 | open |
| P2 | P2-1 | plan.md staleness 批次（A-1 已解决、A-8 半过时、§3.5 基线失真） | open |
| P2 | P2-2 | A-3 堆策略决策（随 P1-4(c) transport 设计一并定） | open |
| P2 | P2-3 | A-6 SEF/Live-Update 显式状态迁移设计 | open |
| P3 | P3-1 | 卫生批次：fmt 22 处（13 文件）+ clippy 2 条 + entry_matches 死参数 | open |
| P3 | P3-3 | C 源 bug 标注（模式 78）：label 级联不 free 堆，Rust 超集修复未标注 | open |
| P3 | P3-3 | boot.rs:90 的「§4.3」引用漂移（06 篇钩子实际在 D4） | open |

验证命令基线（2026-09-14 实测，后续修复轮以此为对照）：
- `cargo test -p minix-ds`：**81 passed / 0 failed**（源码声明 89 个 `#[test]`，差额 8 = 孤儿 retrieve.rs，P1-1 证据）
- `cargo clippy -p minix-ds --all-targets`：本体 **2 条**（slots.rs:51、slots.rs:81 可省略生命周期；其余告警来自 minix-sys/minix-types 依赖，归 edge E-MINSYS-HYGIENE）
- `cargo fmt --check -p minix-ds`：**22 处 diff，涉及 13 个文件**（P3-1）
- `tools/design-coverage-check.sh fork-syscall-rewrite --stage 07-stage-ds`：首轮 FAIL（00/99 缺六件套）→ Step 0.3 生成 → 复跑 **ALL DOCS COMPLETE**

---

## 1. 覆盖矩阵：查漏补缺结论

### 1.1 三向矩阵浓缩（C ↔ 文档 ↔ Rust，四档判定）

工具产物：`.review/claude/fork-syscall-rewrite/scans/07-stage-ds-SYMBOLS.md`（coverage-extract 全量重跑，ds-semantic-map.json 从 22 条扩到 84 条映射）。C 侧 24 个可提取符号中 20 个文档覆盖、4 个「缺口」全是 include guard（`_DS_INC_H`/`_SYSTEM`/`_DS_PROTO_H`/`_DS_STORE_H_`，非语义项，WONTFIX）。语义判定四档：

| C 语义单元（store.c/main.c 锚） | 文档 | Rust 判定 |
|---|---|---|
| 六槽位原语（:11-107） | 04 ✅ reviewed | **已实现**（slots.rs，含双表双币种索引） |
| ds_getprocname/ds_getprocep（:110-138） | 05 | **已实现 + 有意偏离已声明**（identity.rs:59-63：C 的 panic → Option，判定权上交调用方） |
| check_auth（:143-153） | 05 | **已实现**（auth.rs，名字 tenter 化） |
| get_key_name（:158-181） | 07 | **判定半已实现**（check_key_len）；拷贝+尾钉半缺口（→ P1-4(b)） |
| check_sub_match / update_subscribers（:186-224） | 10 | **判定半已实现带 P1-2 缺陷**；发送半缺口（→ P1-4(e)） |
| map_service / sef_cb_init_fresh（:229-282） | 06 | **判定半已实现**（boot.rs，通知环留钩 D4）；rproctab 拉取缺口（→ P1-4(f)） |
| do_publish（:287-378） | 07 | **判定半已实现**（publish.rs 六拒两落法）；堆+grant+通知缺口（→ P1-4(c)/P2-2） |
| do_retrieve / do_retrieve_label（:383-454） | 08 | **已实现但未编译**（retrieve.rs 孤儿，→ P1-1）；拷贝缺口 → P1-4(d) |
| do_subscribe（:456-534） | 10 | **判定半已实现带 P1-5 偏离**（LiteralMatcher + BadPattern vs regexec） |
| do_check（:536-578） | 10 | **已实现**（check.rs，plan/apply 拆分保「拷贝成功才消费」序 :575） |
| do_delete（:583-651） | 09 | **判定半已实现带 P1-3 缺陷**（通知半丢失）；堆交还是 C 超集（P3-2） |
| do_getsysinfo（:653-678） | 11 | **判定半已实现**；sys_datacopy 缺口（→ P1-4(f)） |
| main 循环骨架 + sef_local_startup（main.c:28-104） | 01 | **判定半已实现**（dispatch.rs/sef.rs）；transport 全缺口（→ P1-4） |
| libsys/ds.c 18 API（ds.c:7-219） | 12 | **契约镜像已实现**（client.rs 三契约）；minix-sys 传输半缺口（→ **E-DSWIRE**） |
| DS_SNAPSHOT / do_snapshot / map 系死 API | 02/12 A-7 | **排除**（两侧一致：dispatch.rs:19-21 死号拒认，com.rs 无常量） |

### 1.2 执行半缺口（查漏的真正产出，全部已定级）

沿 C 主循环逐信清点，Rust 侧「只有判定、没有执行」的六处：主循环收发（main.c:45-88 vs main.rs:27-28 空转）、key grant 拷贝（store.c:167-172）、STR/MEM 值 grant 拷贝（store.c:336-350）、retrieve/check 的回拷（store.c:409-416、:561-563、:441-444）、ipc_notify 发送（store.c:222）、rproctab 拉取与 sys_datacopy 整表搬运（store.c:267-269、:672-675）。全部收敛进 P1-4 六子面；客户端半（minix-sys ds.rs）归 edge E-DSWIRE；联调归 E5(f)。

---

## 2. 新条目

### P1-1 retrieve.rs 是孤儿模块：lib.rs 未声明，268 行实现与 8 个测试从未编译

**问题**：`os/servers/ds/src/lib.rs:64-78` 声明了 15 个 `pub mod`，模块文件却有 16 个——`retrieve.rs`（268 行：RetrieveReject/RetrieveHit/truncated_len/plan_retrieve/plan_retrieve_label）不在其中。自相矛盾的是 lib.rs 自己的模块导览（lib.rs:45-46）明确写着「[`retrieve`] — the retrieve verdict: bounds, lookup, gate, length」，08-ds-retrieve.md 更是整篇以 retrieve.rs 为实现归属（:73、:83-87、:124）。后果链：`cargo test -p minix-ds` 只跑 81 个测试（源码声明 89，差额恰为 retrieve.rs 的 8 个）；08 篇全部行锚从未被编译器或测试验证过； Gate D 的「核心算法非 stub」对该臂不成立——不是 stub，是**不存在于编译单元**。

**影响**：08 篇语义（DS_RETRIEVE / DS_RETRIEVE_LABEL 两臂）在 Rust 侧实际处于「写了但没上线」状态；后续任何对 retrieve.rs 的修改都不会触发重编译错误或测试回归，是静默腐烂的温床。

**建议**：方案 A（推荐，一行接线）：lib.rs 增 `pub mod retrieve;`（按字母序插 identity 之后），跑全量测试点亮 8 个孤儿测试，然后按 08 篇 §4.2/§5 复核行锚（模块从未参与编译，文档锚点可能因当时的行号快照而漂移）。方案 B（否决）：删 retrieve.rs 等 08 篇改写时重写——268 行已实现且测试齐备的判定层没有删除的道理。
**验证**：`cargo test -p minix-ds` 基线 81 → **89 passed**；`grep -c "pub mod" os/servers/ds/src/lib.rs` = 16；08 篇 §4.2 行锚逐条 rg 实测。

### P1-2 订阅匹配的类型预门缺 DSF_MASK_TYPE 掩码：IN_USE 公共位使门恒真

**问题**：`subscribe.rs:241` 的类型预门写的是 `entry.flags.intersects(sub.flags)`，注释声称「as the sweep does (:210): disjoint arms never meet」。C 原文是 `ds_subs[i].flags & dsp->flags & DSF_MASK_TYPE`（store.c:210）——**两侧都先掩到类型位再判交**。Rust 漏了掩码：订阅席 flags = `IN_USE | type_mask`（subscribe.rs:210），条目 flags = `IN_USE | type | priv 门`，两者恒有公共位 IN_USE（0x001，com.rs:193），所以 `intersects` **恒真，这道门从不拒绝任何东西**。后果：TYPE_U32 条目发布时，TYPE_STR 订阅者照样通过门进入 pattern 匹配 → 匹配成功即被置位（apply_update notify.rs:91）并被加入唤醒清单（:93-96）；initial_scan 同染（notify.rs:126 经同一 entry_matches）。C 里 `flags & flags & 0xFF0 == 0` 会把这类跨类型配对全部拦下。

**影响**：发布/订阅的核心过滤语义（「订这个类型的更新」）失效一半——pattern 兜底掩盖了大多数场景（键名不同仍不匹配），但同 pattern 跨类型时订阅者会收到不该收的通知与 check 条目。这是行为级 bug，且测试未覆盖：test_entry_matches_gates 与全部 notify 测试的 fixture 都是同类型配对（subscribe.rs:435-454、notify.rs:189-219），门失效不可见。

**影响面二次确认**：10-ds-subscribe-check.md:54 把 C 的掩码门写进了设计基线（「跳过类型不交（`:210`，订阅掩码 ∩ 条目类型，空则无缘）」）——Rust 同时偏离了 C 与自家设计文档。

**建议**：方案 A（推荐）：`subscribe.rs:241` 改 `entry.flags.intersection(sub.flags).intersection(DsFlags::from_bits_truncate(DSF_MASK_TYPE))` 判空——与 retrieve/delete/publish 各臂取类型臂的既有惯法（`flags.intersection(from_bits_truncate(DSF_MASK_TYPE))`，publish.rs:131 同式）完全一致，一个词表。方案 B：给 DsFlags 补 `type_bits()` 助手方法（minix-types 侧改，动共享 crate 归 edge ①，为一处调用不值）。修复必须带测试：跨类型配对（TYPE_U32 条目 × TYPE_STR 订阅、同 pattern）断言不置位不唤醒；新增测试名同步登记 10 篇 §5 表（Gate E 对账）。
**验证**：`rg -n "intersects\(sub.flags\)" os/servers/ds/src/` 修后零命中；新测试跑绿；`cargo test -p minix-ds` 基线不动（修 P1-1 后为 89）。

### P1-3 apply_delete 丢失删除通知半：C 的「删也要叫醒订阅者」契约断在接缝层

**问题**：C 的 update_subscribers（store.c:198-224）是位图半 + 通知半的合体：无论 set=1 还是 set=0，**每个类型相交且匹配通过的订阅者都会被 ipc_notify（:222）**——删除（set=0）同样逐个叫醒。do_delete 对 label 级联的每个受害条目调一次 update_subscribers(…,0)（store.c:628），对受害者本体再调一次（:642）。Rust 侧 `apply_update(set=false)`（notify.rs:50-99）忠实镜像了这个双半语义（test_delete_clears_bit_and_still_wakes 钉住了它），**但 apply_delete 根本不调用它**——delete.rs:222-230 的 `clear_notify_bit` 直接把被清下标从所有订阅者的位图上抹掉（不看类型、不看匹配、不产出唤醒清单），delete.rs:127-131 的注释声称这「observationally identical to C's per-victim update_subscribers(…, 0)」。这个等价声明只对**位半**成立（只有匹配过的订阅者才可能持位，直清是超集无外观差），对**通知半**不成立：C 会叫醒的订阅者，Rust 一个都不会叫。

**影响**：结构上更糟的是素材也被销毁了——DeleteEffect（delete.rs:114-122）只返回三个计数（cleared_entries/cleared_subs/heap_buffers），**既没有被清条目的下标清单，也没有受害者条目的快照**，调用方想补跑 apply_update(set=false) 都没有输入（通知环需要读受害者条目的类型与键来定「谁匹配」，这正是 09 篇 :64 自己写的「先环后旗：环要读条目的类型和键」）。transport 落地后删除路径将静默丢通知。

**影响面二次确认**：09-ds-delete.md:75 的 D4 设计记录（「清位等价化……直清该下标全表 == 逐个匹配清的效果」）与代码同病——只论证了位半；而同篇 :64 明明白色写着 C 语义是「`update_subscribers(dsp, 0)`（`:642`，清位 + 通知，10 细讲）」。设计记录与自己对 C 的描述脱节。

**建议**：方案 A（推荐，保 C 语义 + 复用现成 SweepStats 面板）：DeletePlan 增「受害者快照」字段（plan 阶段 entry 还在表里，`*entry` 是 Copy，顺手拍下）；apply_delete 对每个待清下标先调 `apply_update(store, subs, slot, false, engine, &mut out)` 再清席（受害者本体与级联受害者同一待遇），DeleteEffect 增 `notify_out: usize`（或直接携带端点数组切片计数）——delete.rs 现在没有 engine 参数，签名增 `&impl PatternMatcher`，与 notify.rs 的调用面一致。方案 B（最小改）：apply_delete 只增「被清下标清单」输出（`&mut [EntrySlot]` + 计数），通知完全留给调用方对每个下标调 apply_update——职责更干净（delete 不碰引擎），但调用方多一步循环，且必须保证先通知后清席（受害者快照仍需 plan 携带，否则清完就没得读了）。两案都必须修 09 篇 D4 的等价声明（改为「位半等价 + 通知半由 apply_update 承接」）。**注意先后**：通知要在清席前发（C 的 update_subscribers 读的是还没清的条目，store.c:628 在 flags=0 之前），快照从 plan 来而不是从表来。
**验证**：新测试：一个匹配订阅者 + 一次 label 级联删除 → 断言唤醒清单含订阅者端点、位图被清；`rg -n "observationally identical" os/servers/ds/src/` 修后零命中；09 篇 D4 同步改写并登记测试名（Gate E）。

### P1-4 transport + handler 粘合 + grant/datacopy/notify 接线（执行半总项，stage 内 seam，真实通电挂 E-DSWIRE）

**问题**：DS 服务器不能服务——main.rs:27-28 是空转 loop（注释自白「a registry that cannot receive yet must not pretend otherwise」），dispatch.rs 只有纯函数 triage/should_reply，六处内核接缝全缺（§1.2 清单）。判定层各模块的「frontier 注释」（如 publish.rs:13-16 的「三个边界之外」）已经把边界划好，缺的是接缝本体与装配线。

**建议**：方案 A（推荐，SCHED 同构）：双 trait seam——`DsTransport`（receive/reply，含 EDONTREPLY 抑制语义 should_reply 的消费点）+ `DsKernel`（safecopy_from/safecopy_to/datacopy/notify，签名即 C 对应内核调用的窄镜像）；`DsServer` 单一所有者持两表（对照 os/servers/sched/src/server.rs 的 SchedServer [ARCH S-11] 与 MockIpc/MockKernel 测试基建）。方案 B（否决）：free-function 步骤流 + 共享 static 表（贴 C 形）——单线程下可行，但 transport 不可注入，执行半零可测，判定层的纯函数投资作废一半。方案 C（否决）：等 E1/E2 真实 wrapper 再写——VM（T9 step1-3）与 SCHED（P1-3，Fix #1-#4）两轮先例都证明 seam+mock 先行、通电解耦挂 edge 是既定打法。

六个子面（执行轮可逐个领取，每个独立提交）：
- (a) 主循环装配：receive → triage → 分派 → reply（main.c:45-88 逐拍），含 notify 拒绝的警告打印语义与 64 轮类失败上限的决策（SCHED 先例 server.rs）；
- (b) key grant 拷贝：get_key_name 的 safecopyfrom（store.c:167-172）+ **尾钉指派**——C 在 :178 做 `key_name[DS_MAX_KEYLEN-1]='\0'`，client.rs 的 terminate 是客户端半，服务器半的尾钉目前无主，必须在 transport 设计时显式落位；
- (c) STR/MEM 值拷贝与堆：safecopyfrom 到 MemBody.data（store.c:336-350）——与 P2-2 堆策略耦合，建议 (c) 与 P2-2 同轮定案；
- (d) 回拷三处：retrieve 的 safecopyto（store.c:409-416）、retrieve_label 的 key 回拷（:441-444）、check 的 key 回拷（:561-563，失败不消费——apply_check 已按此序拆好）；
- (e) 通知发送：apply_update/initial_scan 产出的端点 → ipc_notify 循环（store.c:222）；连带兑现 boot.rs D4（boot.rs:89-91）声明的「apply_boot_map 之后统一扫一轮」（map_service 通知环，store.c:246）与 P1-3 修复后的删除通知；
- (f) getsysinfo 整表搬运（sys_datacopy，store.c:672-675——image_bytes() 已备）与 rproctab 拉取（store.c:267-269 → BootService 数组喂 apply_boot_map），含 apply_boot_map 的 Err → panic 判定归属（对照 store.c:275-277 的 panic，落 sef 层）。
**验证**：每子面 mock 测试 + `cargo test -p minix-ds`；真实通电验收 = E5(f) 三链；minix-sys 客户端半与内核对端见 `../edge_todo.md` E-DSWIRE。

### P1-5 A-2 regex 引擎决策：延迟已证实阻塞全部真实客户端场景

**问题**：C 的订阅是 `regcomp("^" + pattern + "$", REG_EXTENDED)` + regexec（store.c:487-498、:190-193）。Rust 现状 = LiteralMatcher 精确匹配 + needs_full_engine 检出元字符 → BadPattern 拒绝（subscribe.rs:92-112），Subscription 存源文本不存编译态（subscription.rs:32-39，[ARCH A-2] 已声明）。本轮新证据把「待决策」升级为「已阻塞」：**grep 全 C 树，每一个真实订阅调用都含元字符**——VFS `ds_subscribe("drv\\.[bc]..\\..*", DSF_INITIAL|DSF_OVERWRITE)`（main.c:441，字符类 + 点 + 星）、input `"drv\\.inp\\..*"`（input.c:665）、storage/filter `"drv\\.blk\\..*"`（main.c:385）、i2c 动态拼 regex（i2c.c:452）。即：当前 LiteralMatcher + BadPattern 组合下，VFS 移植版在启动第 441 行就会拿到 EINVAL 然后 panic（main.c:442「can't subscribe to driver events」）。字面 pattern 在真实 C 客户端里一个都没有。

**建议**：方案 A（推荐）：自研最小锚定 ERE 匹配器，实现 PatternMatcher trait（接缝已备好：subscribe.rs:70-82 的 check/matches 两方法就是为它留的）。范围收窄的依据是 C 的 `^…$` 全锚定已经把「搜索语义」削掉了，剩下的是**结构匹配**：字面、`.`、`*`、`+`、`?`、`[类]`（含取补与区间）、`|`、分组与转义——真实客户端只用到其中五样；no_std 可写（预估数百行，回溯实现对 80 字节 lane 足够），regcomp 拒绝语义对齐 EINVAL。方案 B：regex crate（needs alloc feature）——语义完备但引入依赖树与分配器需求，与 A-3 堆决策（尚无全局分配器）耦合，为一个 80 字节 lane 的匹配不值。方案 C：维持 BadPattern + ARCH 偏离三处标注——零成本但 VFS 场景永不复原，违背 Rewrite 保持外部行为的底线，只配当方案 A 未落地时的诚实过渡（过渡期内 10/12 篇必须把偏离写透）。无论何案，needs_full_engine 的元字符清单要与新引擎的语法面严格一致（`subscribe.rs:107-112`）。
**验证**：表驱动测试：真实客户端四个 pattern（上列）× 对应驱动标签名全部判中、非匹配名全部判不中；BadPattern 只对真语法错误发生；测试名登记 10 篇 §5。

### P2-1 plan.md staleness 批次（A-1 已解决入档、A-8 半过时、§3.5 基线失真）

**问题**：plan.md 三处与 2026-09-14 实况脱节（Step 0.7 staleness 复核结论）：(1) A-1「minix-types 尚无 DsReq/DsReply 类型，需新增」（plan.md:88、:175）**已解决**——MessDsReq/MessDsReply 在 minix-types/src/ipc/message.rs:1448/:1481（repr(C) + 56 字节布局断言 message.rs:3746），常量与 DsFlags 也在（com.rs:118-133、:191-232）；真正的余部是 SI_DATA_STORE 与 NOTIFY_MESSAGE 两个常量（已增补进 edge E-MINTYPES-SYS）。(2) A-8「当前 minix-sys 是 stub，sendrec/notify 为 todo!()」（plan.md:182）半过时——minix-sys 现有 13 模块 6284 行、零 todo!()，真实缺口是**没有 ds.rs**（E-DSWIRE）。(3) §3.5 测试基线「当前为 stub（lib.rs: pub fn init() {}……无任何测试）」（plan.md:165，另 :6 与 :332 的「当前为 stub」字样）已失真——实际 18 文件 3572 行、89 声明测试（81 在跑，差额即 P1-1）。
**建议**：一次批次更新 plan.md §2 表 02/12 行、§4 的 A-1/A-8 行、§3.5、§8 参见——A-1 改「已解决（message.rs:1448/:1481）+ 余部 SI_DATA_STORE/NOTIFY_MESSAGE 挂 E-MINTYPES-SYS」，A-8 改「minix-sys 非 stub；缺 ds.rs 客户端模块 → E-DSWIRE」，§3.5 改现状基线；06-ds-boot-mapping.md 之外各篇提及 A-1 的同步（grep "A-1" 命中 11 篇 :69 的「A-1 余部」表述随 E-MINTYPES-SYS 落地时更新）。
**验证**：`rg -n "当前为 stub|尚无 DsReq|是 stub" notes/rewrite/fork-syscall-rewrite/07-stage-ds/plan.md` 修后零命中。

### P2-2 A-3 堆策略决策（与 P1-4(c) 同轮定案）

**问题**：MemBody 只有形状（store.rs:24-33：指针 + length + reallen，注释明言「谁持有谁分配/释放」，A-3 待设计）。C 的 publish 路径 malloc/free（store.c:336-350：新建 malloc、超长 free+malloc、拷贝失败 free 回滚），delete/overwrite free；Rust 的 apply_delete 已经把「交还描述符」做成了 C 超集（heap_out，P3-2）。决策点：分配器从哪来。
**建议**：方案 A：专用固定池（cap 上限，池块给 MemBody.data；delete 的 heap_out 交还路径与池回收天然对接，内存预算显式可控——DS 是 128 席定长表，池上限可以算出来）。方案 B：no_std 全局分配器（alloc crate）——通用但预算不可见，且整 crate 至今零分配（连测试都是 #[global_allocator] System 例外门，main.rs:8-10），为一个功能开全局分配的口子要慎重。方案 C：维持形状延迟——publish STR/MEM 臂在 transport 落地前无从触发，诚实延迟可接受。推荐：**随 P1-4(c) transport 设计时一并定**（分配生命周期与 grant 拷贝路径绑定，拆开决策必返工），届时按 A/B 出对比表（对照 C 的 malloc 语义与 Redox 无对应物的说明，见 §6）。
**验证**：决策记录落 07/09 篇 [ARCH A-3] 三处一致；publish/delete 堆路径测试。

### P2-3 A-6 SEF/Live-Update 显式状态迁移设计

**问题**：sef.rs:33-37 的 LiveUpdateHook::DsStateTransfer 只是命名承诺；C 的魔法插桩（libmagicrt/magic_ds.c 直接遍历 ds_store/ds_subs 静态内存）在 minix-rs 无对应物，plan.md A-6 已定向「显式序列化/反序列化（参照 03-stage-rs 的 state_data.rs 先例）」。
**建议**：方案 A（推荐）：显式字节级导出/导入两张表——因为 DataEntry 已是 repr(C) 192 字节且偏移锁死（store.rs:99-116），订阅表虽非 repr(C) 但不进镜像（11 篇域），序列化实际上就是「条目表按 C 布局镜像 + 订阅表按自有布局镜像」两段 memcpy 等价物，实现小、可对 IS/RS 消费者复用 image_bytes() 的纪律。方案 B：状态重放（重启后靠 RS 重发布重建）——否决：STR/MEM 堆体与订阅位图是客户端 private 状态，重放不了。落点：06 篇改写时与 STATEFUL 重启语义一并成文，兑现 main.rs:17-20 注释的承诺。
**验证**：迁移往返测试（迁移前后 image_bytes() 逐字节相等 + 订阅位图相等）；01/06 篇 [ARCH A-6] 标注。

### P3-1 卫生批次：fmt 22 处 + clippy 2 条 + entry_matches 死参数

**问题**：`cargo fmt --check` 22 处 diff（auth/boot/check/client/delete/getsysinfo/notify/publish/slots/subscribe 十三文件位）；clippy 本体 2 条——slots.rs:51、:81 的 `get<'a>` 显式生命周期可省略；subscribe.rs:229-246 的 `entry_matches` 带两个不工作的参数（`store: &DsStore` 被 `let _ = store;` 压掉、`_entry_index: usize` 从未用）——是 sweep 设计的残留物，签名在说谎。
**建议**：fmt 全量应用 + 删两处显式生命周期；entry_matches 签名瘦身随 P1-2 修复同函数顺带（调用方 notify.rs 两处同步）。若 P1-3 选方案 A，engine 参数进签名时一并定稿。
**验证**：`cargo fmt --check -p minix-ds` 零 diff；`cargo clippy -p minix-ds --all-targets` 本体 0 条。

### P3-2 C 源 bug 标注（模式 78）：label 级联不 free 堆，Rust 超集修复未声明

**问题**：C 的 do_delete 对 label 级联受害条目只做 `update_subscribers + flags = 0`（store.c:624-631），**不 free 其 STR/MEM 堆体**——C 源码 bug（内存泄漏）；受害者本体走 :635 的 free。Rust 的 apply_delete 对级联受害者同样交还堆描述符（delete.rs:174-183 take_heap_buffer），是超集修复。按模式 78（MINIX3 BUG 未标注），这个「比 C 严/比 C 对」必须在两处留字据，否则覆盖率对账时会误判 Rust 多做了事。
**建议**：delete.rs 级联分支补一行 MINIX3 BUG 注释（引 store.c:624-631 泄漏点）+ 09 篇 §3 对应行补标注。
**验证**：`rg -n "MINIX3 BUG" os/servers/ds/src/delete.rs notes/rewrite/fork-syscall-rewrite/07-stage-ds/09-ds-delete.md` 命中。

### P3-3 boot.rs 的「§4.3」引用漂移

**问题**：boot.rs:89-91 注释说通知环钩子「stands documented (§4.3)」——06-ds-boot-mapping.md 的钩子实际记录在设计决策 D4（:79「通知环留钩不定空线」），:105 的 §4.3 是「不变量」节。指错了门牌。
**建议**：boot.rs:90 注释改指「06 篇 D4」；一行改动，随任何触碰 boot.rs 的轮次顺带。
**验证**：`rg -n "§4.3" os/servers/ds/src/boot.rs` 修后零命中。

---

## 3. 存量复核：plan.md A-1..A-10 staleness 结论（2026-09-14 实测）

| # | plan.md 原状态 | 本轮复核 | 处置 |
|---|---|---|---|
| A-1 消息类型 | 缺口（:88、:175） | **大半已解决**：MessDsReq/MessDsReply（message.rs:1448/:1481，56B 断言 :3746）+ 调用号（com.rs:118-133）+ DsFlags（com.rs:191-232）。余部 = SI_DATA_STORE（getsysinfo.rs:27 本地）与 NOTIFY_MESSAGE（dispatch.rs:94-96 硬编码）→ **E-MINTYPES-SYS 增补** | P2-1 入档 |
| A-2 正则引擎 | 待决策（:176） | **升级 P1-5**：真实客户端 pattern 全含元字符（VFS main.c:441 等），BadPattern 现状阻塞主场景 | P1-5 |
| A-3 动态内存 | 待设计（:177） | 未动（MemBody 形状已定 store.rs:24-33；delete 交还路径已备成 C 超集） | P2-2 |
| A-4 静态数组表 | 待设计（:178） | **已解决**：DsStore/DsSubs = `[Option<_>; N]` + EntrySlot/SubSlot newtype（store.rs:83、subscription.rs:66、slots.rs） | 入档（随 P2-1） |
| A-5 订阅位图 | 可复用（:179） | **已解决**：minix-types Bitmap 复用（subscription.rs:42） | 入档 |
| A-6 SEF/LU 状态迁移 | 缺口（:180） | 未动（sef.rs:33-37 仅命名承诺；RS state_data.rs 先例可循） | P2-3 |
| A-7 死 API 排除 | 排除（:181） | **已落实**：dispatch.rs:19-21 死号拒认 + com.rs:110-112 注释 + 无 DS_SNAPSHOT 常量 | 无 |
| A-8 客户端库归属 | 待实施（:182） | **半过时**：minix-sys 非 stub（13 模块 6284 行）但无 ds.rs；ds crate 的 minix-sys 依赖零引用（死依赖）；client.rs 契约镜像已就位 | **E-DSWIRE** + P2-1 入档 |
| A-9 错误码 | 已具备（:183） | **已解决**：Errno + 各 Reject::errno() 单点渲染（publish.rs:70-80 等），禁自创成立 | 无 |
| A-10 192B 布局 ABI | 待决策（:184） | **已解决（兼容案）**：repr(C) + 192B + 三偏移断言（store.rs:99-116）；IS dmp_ds.c 消费者契约在 03 篇 §1.6/11 篇成文 | 无 |

---

## 4. 架构分层深审结论（五层）

### 4.0 P0 自检与漏检自检

四臂门序与 C 逐一重对：publish 六步（publish.rs:115-173 对照 store.c:297-326，含 label 数字门与 BadType 后置的「席仍干净」论证）、retrieve 五步（retrieve.rs:95-135 对照 :393-427）、delete 五步 + 级联（delete.rs:72-111 对照 :593-645，owner 判定绕过 PRIV 门的有意差异已按 C 注明 delete.rs:33-37）、subscribe 六步（subscribe.rs:155-191 对照 :466-508，取席先于取键的 C 序保留并注明 :165-167）。错误码断言：ESRCH/EAGAIN/EEXIST/EPERM/EINVAL 全对账（各臂 test_errno_mapping）。漏检自检（收敛规则，抽三处重验无新发现）：do_check 的「拷贝成功才清位」（store.c:575 在 safecopyto 之后；Rust plan/apply 拆分 + check.rs:107-110 注释，序正确）；is_notify 的无符号比较陷阱（com.h:93 vs dispatch.rs:94-96，wrapping_sub as u32 镜像准确）；do_publish 的 overwrite 门走 check_auth 而非裸名比对（store.c:321-322 vs publish.rs:153-157，等价——tenter 化的名字）。

### 4.1 L0 组合层

bin/lib 双目标 + 判定层 16 模块的形状健康，单线程事件循环模型在各模块头显式声明（审 DD 代码时按此标尺，未误用内核标尺）。两笔账：模块导览与声明集自相矛盾（P1-1，导览说了谎）；依赖面 minix-types 健康、minix-sys 是死依赖（E-DSWIRE 附带处置）。**传输基建上移判定**（触发 06-stage-sched/todo.md V2 §4.1 预留条件）：DS 是第 4 个用户态服务器 seam 消费者（VM KernelGateway / RS 五域 supertrait / SCHED 双 trait / DS 待建），本轮判定**暂不上移**——四台服务器 seam 形状各异且各有理由，DS 面最窄（receive/send + safecopy 家族），上移等 E1 trap 层落定后随 E-DSWIRE 执行时评估一次，字据留在 E-DSWIRE 建议段。

### 4.2 L1 服务器内部

**正面确认三处卓越设计**：(1) plan/apply + facts-or-refusal 形状贯穿全 crate——Reject 枚举穷尽 C 拒绝路径并自述 errno（publish.rs:70-80），命中类型携带席位与长度，与 06 轮 SCHED Fix #9 的 OccupiedSlot 同族；(2) union 纪律——DataBody 两臂单写单读，5 处 unsafe 读全部带 SAFETY 注释指认臂的文档化用途（slots.rs:176-179、identity.rs:67-70、retrieve.rs:117-127、delete.rs:210-213），「enum Body 替代」评估否决（判别字破坏 192B ABI，03 篇 D2 :94 已论证，维持）；(3) 首适序即镜像序的论证链（slots.rs:16-18 [ARCH A-4] → getsysinfo.rs:13-16 消费端呼应）把一个「碰巧的实现细节」提升成了受保护的契约。**缺陷**：类型预门（P1-2）与死参数（P3-1）都在 entry_matches——L1 本轮唯一的真伤。

### 4.3 L2 内核接缝

缺口本体即 P1-4 六子面（§1.2），这里只记 translate 防线检查与有意偏离：C 的两个全局变量 who_e/callnr 被 Incoming 携带而非镜像（dispatch.rs:62-69，好）；EDONTREPLY 保留为 should_reply 谓词（dispatch.rs:103-105，好）；四处有意偏离全部有注释与文档落点——ds_getprocep panic→Option（identity.rs:59-63，判定权上交）、sweep 跳过 stale 订阅者（notify.rs:12-17，C 会 panic 自杀，可用性优先）、Subscription 存源文本（subscription.rs:32-39）、check 的 reply_owner 延迟判定（check.rs:70-75）。另有一处 C 源 bug 未标注（P3-2 级联泄漏）。

### 4.4 L3 wire 层

镜像三件套全部有布局断言：MessDsReq/MessDsReply repr(C) + 56 字节 + 字段偏移（message.rs:1434-1447 文档块、:3746 测试）；DsFlags 逐位对账 C ds.h（com.rs:191-232 + 对账测试段）；DataEntry 192B + key/owner/body 三偏移断言（store.rs:99-116）——A-10 以「兼容案」闭合，IS dmp_ds.c 消费者契约（getsysinfo.rs:11-16 的论证）是本轮读到的最好的「为什么这个断言不能松」的成文。client.rs 三契约（grant 尺寸方向 / NUL 钉尾 / 回信复用请求栏）与 ds.c 对账一致（key_grant 测试 :147-163 对 ds.c:13-19）。唯一缺口：服务器半的 key 尾钉无主（P1-4(b) 设计点）。

### 4.5 L4 测试架构（五维）

81 在跑测试全部带 C 锚点注释（虚构：零）；errno 映射各臂独立钉一遍是有意重复（冗余：可控）；无效：零；自身正确性：抽验断言与 C 语义一致（§4.0 三处）。缺口四件：类型门拒绝路径无测试（P1-2 的隐性帮凶）、删除通知无测试（P1-3）、BadPattern 传播到 handler 的路径无测试（随 P1-5）、transport 层零测试（随 P1-4 mock）。**孤儿账**：retrieve.rs 8 个测试声明在案从未运行（P1-1）——「测试数量声明」类文档断言（08 篇）因此全部待复验。联调：C tests/ds/ 347 行契约零承接（E5(f)）。Gate E 抽验：04 篇 §5 测试表 10/10 与 slots.rs 实有对齐；10 篇抽样 test_meta_patterns_need_full_engine 对齐。

---

## 5. 边界条目双向指针（唯一入口：../edge_todo.md）

| edge 条目 | 来源 | 一句话 | 07 侧关联 |
|---|---|---|---|
| E-DSWIRE（新登记） | 本轮 §1.2 | minix-sys 缺 ds.rs 客户端模块 + DS transport 真实通电 + 联调零覆盖，三缺一注册 | P1-4 的执行半出口；P2-1 的 A-8 入档 |
| E-MINTYPES-SYS（增补） | 本轮 §3 A-1 行 | SI_DATA_STORE / NOTIFY_MESSAGE 并入 minix-types 常量收敛轮 | getsysinfo.rs:27、dispatch.rs:94-96 改消费 |
| E5 增补 (f) | 本轮 §4.5 | DS 发布/订阅联调三链（C tests/ds 契约） | P1-4 通电验收面；P1-5 的元字符用例 |
| E-MINSYS-HYGENE（已有） | 03-stage-rs §22 | minix-sys clippy 卫生 5 条 | 本轮 clippy 输出中依赖侧告警归此，不属 DS |

---

## 6. 对照参考

**Redox（联网核验，2026-09-14，诚实标注）**：Redox 把「注册中心」表达为 scheme 文件语义（服务即路径，redox-scheme crate 为 scheme 守护进程库，crates.io 可核验）；其 GitLab 上的 data-store 专项 crate 今日不可抓取（GitLab 需 JS 渲染、crates.io 无此 crate）[待验证]。对本项目的可执行结论：**Redox 参照只在 transport seam 的 daemon 主循环形状层面有效**；DS 的 192B 镜像直读契约（IS dmp_ds.c 不解析直接解释内存）使「scheme 化重构」不成立——保持 C 外部行为是硬约束，这与 11 篇的论证互为印证。

**OS 理论**：发布/订阅注册中心的最小正确性 = 通知不丢不冒。C 用「无条件 notify + 位图消费」的 at-most-once 简单换正确（update_subscribers 对每个匹配者必叫，订阅者靠 check 位图去重）；P1-3 的教训是这套语义里**删除与发布同权**——删了也要叫，订阅者靠 ENOENT 自行消化。任何「等价化优化」都必须枚举位、通知、顺序三个可观察面（§7 规则候选 1 的来源）。

**Rust 社区**：bitflags + 总集枚举判决 + plan/apply 纯函数分层是 no_std 事件循环服务器的正面惯法（与标准库 Entry API 的「先判后动」同族）；本 crate 的「事实（&[u8;80] lane，Copy）与判决（enum，单点 errno 渲染）分离」值得作为全仓用户态服务器的范式参照。反例教材是 P1-2：`intersects` 的「交集即成员」直觉在共享状态位（IN_USE）面前不成立——**成员判断必须先归约到语义子空间（掩码）再判交**。

---

## 7. Rule Discovery（Step 5.7）与 Gate 证据

**规则候选（同型 ≥2 例才提案，本轮三条各 2 例）**：
1. **「等价声明必须逐可观察面枚举」**：delete.rs:127-131 与 09 篇 D4（:75）同病——只论证位半、静默通知半；06 轮 P2-1（注释声称不存在的规则）是其注释版姊妹。检查法：凡「observationally identical / 等价 / equivalent」字样，强制列出 C 副作用完整清单（状态位、通知/消息、错误路径、顺序）逐项打勾。
2. **「导览引用的模块必须存在」**：lib.rs:45-46 导览提及 retrieve 而 :64-78 未声明；08 篇 :73 同样描述该模块。机检法：模块导览注释中的 `[\`xxx\`]` 引用集合 ⊆ `pub mod` 集合，可写成 CI grep 一行。与 06 轮 P2-2（实现归属未声明）同族——都是「文档说有、编译器说无」。
3. **「延迟决策须附消费者阻塞评估」**：A-2 延迟时未核查真实客户端（VFS main.c:441 等 pattern 全含元字符），延迟被当成了免费态。检查法：凡登记 DEFERRED/待决策，必须回答「当前已有哪些消费者会被此延迟阻塞、阻塞后果是什么」，写进决策记录。

**Gate 证据（本轮实测）**：
- **Step 0**：预检 FAIL（00/99 缺 2×3 快照，H.1+H.6 CRITICAL）→ Step 0.3.2/0.3.3/0.3.4 嵌入生成六件 → `tools/design-coverage-check.sh fork-syscall-rewrite --stage 07-stage-ds` 复跑 **ALL DOCS COMPLETE**（H.1/H.6 归零）。
- **Gate A**：ds-semantic-map.json 22→84 条全量扩展；SYMBOLS.md 生成于 `.review/claude/fork-syscall-rewrite/scans/07-stage-ds-SYMBOLS.md`（24 C 符号：20 文档覆盖 83.3%，4 缺口 = include guard WONTFIX；Rust 覆盖 19 名称匹配 + 语义映射全量在册）。
- **Gate D**：判定层核心算法非 stub（全 crate 零 todo!/unimplemented!）；FAIL 项 = retrieve 未编译（P1-1）；trait 单实现检查：PatternMatcher 当前 1 实现（LiteralMatcher）——它是 seam 占位且 A-2 决策即将添第二实现（P1-5），登记不立项。
- **Gate E**：04 篇 §5 测试表 10/10 对齐；10 篇抽样 1/1；**89 声明 vs 81 运行**差额定位（retrieve 8）为本轮 Gate E 的主发现。
- **Step -0.5**：check ✅；clippy 本体 2 条；fmt 22 处；`cargo test -p minix-ds` 81 passed / 0 failed（scan-only 基线，收尾复测不变）。
- **锚点纪律**：关键行锚全部 rg/sed 实测——lib.rs:45-46/:64-78、subscribe.rs:241/:210 注释、notify.rs:91-96、delete.rs:114-131/:174-183/:222-230、identity.rs:59-63、boot.rs:89-91、getsysinfo.rs:27、main.rs:25-28、message.rs:1448/:1481/:3746、com.rs:118-133/:191-232、store.c:210/:222/:246/:267-269/:336-350/:624-631/:672-675、main.c:441-442（VFS）、ds.c:13-19、input.c:665、filter main.c:385、i2c.c:452、kpriv.rs:827、endpoint.rs:67。

**收敛评估**：首轮 stage 内 0 P0 / 5 P1 / 3 P2 / 3 P3 + edge 1 新 2 增补。新发现占比健康（判定层从未被架构级审过；检索类发现——孤儿模块、类型门、删除通知——均为首轮只见）。修复成本粗估：P1-1 ≈5 分钟（一行 + 复验锚）；P1-2/P1-3 各一轮 todo-fix（含测试与文档同步）；P1-4 为多轮 campaign（六子面）；P1-5 含引擎实现（最大单件）。建议下一轮触发条件：P1-1..P1-3 修复后做一次轻量验证轮（非时间驱动例行轮）。

---

## 8. 建议推进顺序

1. **P1-1 retrieve.rs 接线**（成本最低收益最大：一行声明 + 89 测试点亮 + 08 篇锚点复验）。
2. **P1-2 类型门掩码 + P1-3 删除通知**（两个行为级 bug，各带新增测试；P3-1 的死参数与 P3-2 标注顺带）。
3. **P1-5 A-2 regex 决策与实现**（可独立先行；解锁 E5(f) 元字符用例与 VFS 同款场景）。
4. **P1-4 transport 六子面**（最大 campaign，方案 A；与 E-DSWIRE 的 minix-sys ds.rs 协同排期，(c) 子面与 P2-2 堆决策同轮）。
5. **P2-1 plan.md 入档批 + P3-3 引用漂移**（轻量，随任意一轮顺带亦可）。
6. **P2-3 A-6 SEF 迁移设计**（落点在 06 篇改写轮）。

每次修复遵循 fix-guard（修前读目标行 ±5、grep 确认现状、一次一条、修后 grep 验证并记录），修完跑 `cargo test -p minix-ds` 对照 §0 基线（当前 **81 passed**；P1-1 后应 89）。

---

## 9. 修复记录（执行轮；一次一个 TODO，每条一个提交）

> 首轮（2026-09-14）scan-only 未修；执行轮自 2026-09-15 起。基线：81 passed（scan 轮实测）。

### ✅ Fix #1: P1-1 — retrieve.rs 孤儿模块接线（2026-09-15）

- **File**: `os/servers/ds/src/lib.rs:74`（publish 与 sef 之间，字母序）
- **Before**: `pub mod publish;` 直接连 `pub mod sef;`（15 个 `pub mod`，retrieve 缺席）
- **After**: 插入 `pub mod retrieve;`（16 个，与导览注释 lib.rs:45-46 及文件集一致）
- **Verified**: `cargo test -p minix-ds` **81 → 89 passed**（8 个孤儿测试点亮，零失败）；`rg -c "pub mod" lib.rs` = 16；Gate E——08 篇 §5 测试表 8/8 与 retrieve.rs:180-262 实有对齐，符号级锚（RetrieveReject:28 / RetrieveHit:57 / truncated_len:76 / plan_retrieve:95 / plan_retrieve_label:143）rg 实测全命中，08 篇无行号引用无需修正。
