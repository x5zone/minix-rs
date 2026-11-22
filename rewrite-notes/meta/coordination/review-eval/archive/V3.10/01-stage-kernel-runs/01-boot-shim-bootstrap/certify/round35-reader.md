# R35 certify-reader 独立回读回执
- 结论：PASS
- 核验条数：12
- CLOSED：12；STILL-BROKEN：0
- 时间：2026-11-19T21:39:04+08:00

## 1. §3.4.1「内存图生产的四层责任」第 3 层
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1648` 原文摘录：「政策侧把描述符按类型与属性分成 Available（可分配）、ReservedMapped（保留但可访问）、Excluded（不可触碰）、Special（延迟处理）四档……**现行实现把 Special 展开成两档落点**：MMIO 类走 `DevicePath`，其余按属性与类型表走 `Side`（侧桶：既不进 free list 也不进保留表）。因此实现侧的完整落点是五类——可分配 / `ReservedMapped` / `Excluded` / `DevicePath` / `Side`，判定是五步（不是三条规则），逐步判据见 §4.2.2 的表。」四档政策、五类落点、Special 展开为 DevicePath 与 Side、五步判定四要素齐备。

## 2. §5.1 的 M6 行
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:3151` 原文摘录：「真机侧只有 x86 UEFI 门（到 `entering scheduler`）、riscv64 U-Boot shim 门（6/6 路标）与 aarch64 SMP AP 门跑到 Step 1 之后——aarch64 UEFI 门止于 EBS 后的 `memmap source`（在 `enter_kernel_image_v1` 之前），riscv 直核 `bootface` 门是故意的模块定位失败路径（在 `arch_boot` 之前），二者都不覆盖 Step 1。」覆盖列只把三条跑到 Step 1 之后的门链计入，并显式点名两条不覆盖的门链。

## 3. §5.1 的 M14 行
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:3159` 原文摘录：「riscv64 U-Boot 直载测试链不在覆盖列——它的测试内核直构 `KernelInfo`、绕过 `prepare_boot`，只证明 U-Boot 的 `fatload`/`bootelf` 交棒透明，不经过 v1 握手」。该链已从覆盖列移出，并写明其证明范围仅限交棒透明。

## 4. §6.3 的「串口最后一行通常是 `kernel: step1+2 mappings ok`」
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:3450` 原文转述摘录：「串口最后一行通常是 `kernel: step1+2 mappings ok`（卡在建根的后半段：Step 1/2 映射已过，但 Step 2b/2c/3 与跳转仍在其后）」。括号内已换成要求的新表述，该停点不再以「建根完成」描述，而是明确为「建根的后半段」并列出仍在其后的 Step 2b/2c/3 与跳转。

## 5. §3.4.2 开头的两个计数
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1657` 原文摘录：「**先分清两个计数**：这 21 个是 C `kinfo_t` → Rust `KernelInfo` 的累计退场台账（C 有、Rust 不再保留的全部字段）；而 §3.5/§3.9 与 §3.4.1 说的「字段删除」是一次具体裁决——从 `KernelInfo` 里删掉 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个，使字段数由 16 变成 13、`authority` 加入后为 14。」累计退场台账与单次字段删除裁决已分开陈述，数字 16→13→14 明写。

## 6. §4.1 的 1024/256 预算段
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2295` 原文摘录：「两个直核面走的是同一片注册给 `pt_alloc` 的 bump 池，覆盖范围按 `bootface.rs` 的常量注释是「Sv39 root + identity low half + high half + two DM windows」——**DM 窗口也算在这 256 页里**」。同段前文写明 1024 页与 256 页分别属于跨镜像腿与两个直核面，并说明两者不共享常量。第 2933 行的「预算在 UEFI/U-Boot 腿是 1024 页（两个直核面是 256 页）」与之一致。

## 7. 全文的「四条架构」错误计数
- 判定：CLOSED
- 证据：命令 `grep -nE '四(条|个|种)架构|四个架构' /home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 输出 `(none)`；反向抽查 `grep -n '三架构'` 命中第 1402、1442、1876、1884、2078、2088、2223、2295、2986、3025、3027 等行，均为「三架构」或「三个架构」的正确计数。

## 8. 全文裸文件名 `OPEN-DEBT.md` / `suggestions.md`
- 判定：CLOSED
- 证据：命令 `grep -nE '(^|[^/])(OPEN-DEBT|suggestions)\.md' /home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 输出 `(none)`；全量命中三处，均带完整前缀：第 1653 行与第 2677 行的 `rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/OPEN-DEBT.md`，第 2933 行的 `rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/suggestions.md`。

## 9. §3.4 的 `KinfoStruct` 48 字节
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1685` 原文摘录：「布局：`freepde_start`/`user_sp`/`vir_kern_start` 三个 `i64` = 24 B，`nr_procs`/`nr_tasks` 两个 `i32` = 8 B，`release`/`version` 两个 `[u8; 6]` = 12 B，再加 4 B 对齐 padding，合计 48 B」。字段宽度加法与 padding 均已给出，数值 24+8+12+4=48 自洽。

## 10. 全文「第 NN 个字段」序数短语
- 判定：CLOSED
- 证据：命令 `grep -nE '第 ?[0-9]+ ?个(字段|域|成员)|第 ?[一二三四五六七八九十]+ ?个(字段|域|成员)'` 只命中第 359 行的「`kinfo` 的第一个字段也叫 `mbi`」，是中文数词的普通叙述，不是 `第 NN 个字段` 的数字形式；命令 `tools/doc-ref-check.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 输出「✅ doc-ref-check: rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md 内部章节引用均可解析」，退出码 0，说明该行未被误判为章引用。会触发误判的数字序号短语已清除。

## 11. §2.7 的 30MB 间隔
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1330` 原文摘录：「这个间隔由两个常量相减得来——0 号模块的物理基址 `MB_MODS_BASE = 0x82000000`（`earm/pre_init.c:182`）减去内核物理基址 `_kern_phys_base = 0x80200000`（`earm/kernel.lds:4`），即 `0x1E00000` = 30 MiB。」两个来源常量与 30 MiB 的换算均已写明。

## 12. §4.6 的 mock 分叉段
- 判定：CLOSED
- 证据：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2933` 原文摘录：「**宿主 mock 是实现差异**：`MockPaging::map_huge` 走 `map_range`，在**任意已映射**的虚拟页上返回 `AlreadyMapped`，而三架构真实实现只在槽位已是 1 GiB 叶时返回——因此这条臂在 mock 下是重叠窗口的常态返回、在真机上不可达。」该臂的指代（Step 1 `map_huge` 的 `AlreadyMapped` 分支）在前句给出，mock 常态与真机不可达的区分由「任意已映射」对「槽位已是 1 GiB 叶」直接支撑，读者无须外部知识即可理解。

结论：回读验证通过，12 条逐条落证。
