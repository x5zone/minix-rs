# DESIGN-D2 r1 — 01-rs-boot-init

只读评审。范围：文档 `rewrite-notes/03-stage-rs/01-rs-boot-init.md` §3.1/§3.4/§3.5/§3.6 与 `os/servers/rs/src/boot.rs`、`os/servers/rs/src/trap_api.rs`、`os/servers/rs/src/main.rs`、`os/servers/rs/src/lib.rs`、`os/servers/rs/Cargo.toml`。

## CHECK 1 PASS

对照 `rewrite-notes/03-stage-rs/01-rs-boot-init.md` §3.1（619-638 行）、§3.4（694-751 行）与 `os/servers/rs/src/boot.rs:554-1050`，三项结构变化都有说明：

- **RProcTable**：文档 §3.4 的 `BootInit` 字段 `table: RProcTable` 带 C 映射与边界（`01-rs-boot-init.md:703`：「C: rproc/rprocpub 表（字段语义归 02；本模块只做生命周期编排）」）；`into_state` 移交运行期的说明在 `01-rs-boot-init.md:722-727`，与 `boot.rs:888`（字段）、`boot.rs:919-930`（`into_state` 逐字段搬入 `ServerState`）一致。`RProcTable::new()` 的表重置在 `boot.rs:967`，对应文档 §3.4 的 step0「表重置」。
- **step0**：文档 `01-rs-boot-init.md:710` 明确列出 `self.step0_prepare(sys)?; // main.c:178-237（前置：配置/HZ/grant/复位/计数核对/表重置）`；代码 `boot.rs:937-944` 的调用序列为 step0→step1→step2→step3→step4，`boot.rs:949-982` 的 step0 实际覆盖 `get_machine`/`get_hz`/`shutting_down` 复位/`validate_tables`/表重置/`grant_read`，与文档括注逐项对应。
- **self_update 无生产调用点**：`boot.rs:1258-1327` 定义 `self_update`，`boot.rs:937-944` 的 `init_fresh` 未调用；全仓 grep 的调用点只有 `boot.rs:1963`、`boot.rs:2020` 两处测试，且两测试都在 `#[cfg(feature = "live-update")]` 下（`boot.rs:1934`、`boot.rs:1991`）。文档 §3.6 的 `01-rs-boot-init.md:825` 已把这一点写成明确差异。

次要观察（不影响判定，置信度高）：文档 `01-rs-boot-init.md:696` 说 `init_fresh()`「内部按 §2.3 顺序调用四个私有方法」，而 `boot.rs:938-942` 实际调用五个私有方法（`step0_prepare` + `step1_set_attrs`..`step4_finish`）。同节代码块 `01-rs-boot-init.md:709-716` 已展示 step0，结构本身不会误导读者，但「四个」应改为「前置 step0 + 四个步骤」或「五个私有方法」。

## CHECK 2 FAIL

FINDING D2-1 — §3.5 的 SysApi 面把 `get_image`/`grant_direct` 写在错误的域面，且「boot 模块不直接调用 minix-sys」与代码矛盾。

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:758-764`（`pub trait SysApi` 注释块内的「get_image（main.c:196）/ grant_direct（main.c:185）」）；连带 `01-rs-boot-init.md:831`（§3.7 把 `KernelApi::grant_read` 标为「`SysApi` 面」）；连带 `01-rs-boot-init.md:755`（「Rust 侧的 boot 模块不直接调用 `minix-sys`」）。
- 问题：实际 `SysApi`（`os/servers/rs/src/boot.rs:71-116`）只有 `get_machine`/`get_hz`/`get_ticks`/`privctl`/`getpriv`/`setalarm`/`sys_kill`/`sys_update`/`diagctl_stacktrace`/`diag_write` 十个方法，`get_image` 与 `grant_direct` 都不在其中，任何域面也没有同名方法。真实结构是：
  - `get_image` 完全不在 `KernelApi` 域面内：它是 `BootTables::acquire_from`（`boot.rs:600-620`）内部对 `minix_sys::syscall::sys_getimage` 的自由函数调用（`boot.rs:614`），参数是 `minix_sys::syscall::KernelCallTransport`（`boot.rs:601`），生产由 `main.rs:62` 用 `DirectKernelCallTransport` 直接调用。这正是 §3.7 没有涵盖、而 §3.1 提到的 `acquire_from` 路径。
  - `grant_direct` 的 trait 入口是 `grant_read`，位于 **`IpcApi`**（`boot.rs:228-325` 的面，`boot.rs:298`），生产实现 `trap_api.rs:639-642` 转 `minix_sys::grant::GrantTable::grant_direct`。`TrapKernelApi` 的 `SysApi` 实现块 `trap_api.rs:242-336` 没有 grant 方法。
- 证据：`boot.rs:71-116`、`boot.rs:228-325`、`boot.rs:298`、`boot.rs:600-620`、`boot.rs:614`；`trap_api.rs:530-646`（`impl IpcApi for TrapKernelApi`，`grant_read` 在 639，`grant_direct` 在 642）；`main.rs:62`。
- 影响：读者按 §3.5 会以为实现 `SysApi` 就能拿到启动映像与 grant 创建；实际 `MockKernelApi`/`TrapKernelApi` 的 SysApi 测试替身根本不覆盖这两项，`test_domain_face_implementable_in_isolation`（文档 `01-rs-boot-init.md:1039` 引用的测试）也无法验证这两条路径。§3.7 的「`SysApi` 面」还会把 grant 的实现责任人指错。
- 最小修复：把 `get_image`/`grant_direct` 两行从 §3.5 的 `SysApi` 注释块移出；在 §3.5 增一句：「`GET_IMAGE` 走 `BootTables::acquire_from` 的内核调用（不经 `KernelApi` 域面）；grant 创建入口是 `IpcApi::grant_read`（生产实现转 `GrantTable::grant_direct`）」，并同步修改 `01-rs-boot-init.md:755` 的「boot 模块不直接调用 minix-sys」与 `01-rs-boot-init.md:831` 的「`SysApi` 面」为 `IpcApi`。若设计意图确实是让二者进 `SysApi`，则代码需加方法并调整 `TrapKernelApi`/测试，但当前 `main.rs` 与测试都按现状工作，改文档是成本更低的修复。
- 置信度：高。

FINDING D2-2（次要，可与 D2-1 同批修）— 生产组装的落点描述不准。

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:755`（「生产入口在 `main.rs` 用 `TrapKernelApi`（直接陷入内核）组装这些域面」）。
- 问题：`main.rs:86` 只调用 `RsServer::new(tables)`；`TrapKernelApi` 的组装在 `lib.rs:238-241`（`alloc::boxed::Box::new(trap_api::TrapKernelApi::new())`），`main.rs:89-91` 的注释也说明由构造函数完成。生产实现身份（`TrapKernelApi`，`trap_api.rs:242/338/370/499/530` 五面全实现）与文档一致，仅「在 main.rs 组装」这一句不准确。
- 最小修复：把「在 `main.rs` 用 `TrapKernelApi` 组装」改为「`RsServer::new`（`lib.rs`）默认组装 `TrapKernelApi`，`main.rs` 只构造 `RsServer`」。
- 置信度：高。

## CHECK 3 PASS

对照 `rewrite-notes/03-stage-rs/01-rs-boot-init.md` §3.6 与代码/C 源：

- C 默认 yes 的引用准确：`01-rs-boot-init.md:816`「`USE_LIVEUPDATE` 默认 yes，`share/mk/bsd.own.mk:1499`」——实际文件 `minix3/share/mk/bsd.own.mk:1499` 正落在 MINIX 专有的「default yes unless MK* == no」块（1487-1512 行）内，`minix3/minix/lib/libsys/Makefile:137-138` 据此加 `-DUSE_LIVEUPDATE`。
- 差异被明确写成行为差异：`01-rs-boot-init.md:825`「当前 `init_fresh` 末尾没有调用 `self_update`，它只由 feature 测试覆盖；默认 Rust boot 比 C 默认 yes 少这一尾步，这是接 18 前的已知差异。」不是含糊带过。
- 代码与之一致：`os/servers/rs/Cargo.toml:26-30` 的 `default = []`、`live-update = []`（默认关）；`boot.rs:1258` 的 `#[cfg(feature = "live-update")]`；`boot.rs:937-944` 无 `self_update` 调用；仅 `boot.rs:1934/1963`、`boot.rs:1991/2020` 的 feature 测试调用。
- 置信度：高。

## SUMMARY

- CHECK 1 PASS：`BootTables`/`BootInit` 的 RProcTable、step0、`self_update` 无生产调用点三项结构变化都被说明，且与 `boot.rs:554-1050` 对齐。唯一措辞瑕疵：§3.4 说「四个私有方法」，实际 `init_fresh` 调用五个（含 step0），不影响结论。
- CHECK 2 FAIL：§3.5 的 `SysApi` 注释声称包含 `get_image`/`grant_direct`，但 `boot.rs:71-116` 的 `SysApi` 两者都没有；`get_image` 是 `BootTables::acquire_from`（`boot.rs:600-620`）里的 `minix_sys::syscall::sys_getimage` 自由函数调用，grant 入口是 `IpcApi::grant_read`（`boot.rs:298`，生产 `trap_api.rs:639-642`）。§3.5「boot 模块不直接调用 minix-sys」与 `boot.rs:601/614` 矛盾，§3.7 又把 grant_read 标成 `SysApi` 面，需一并修正。次要问题是组装地点写成 main.rs，实际在 `lib.rs:238-241`。
- CHECK 3 PASS：`USE_LIVEUPDATE` 的 C 默认 yes 引用（`minix3/share/mk/bsd.own.mk:1499`）准确，Rust 默认关（`Cargo.toml:26-30`）与「少这一尾步」的差异在 §3.6 被明确写成行为差异。
- 结论：2 PASS / 1 FAIL，阻断项为 D2-1。
