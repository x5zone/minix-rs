DOC: rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
CHAPTER: 第 3 章 Rust 设计决策（切片 ch3.md，586 行，sha256 5fdc609c… 已核对；摘要 53 行，sha256 a3503483…）
VERDICT: DEFICIENT

完整性（Ch4–Ch6 下游 scope 需要而摘要缺）
1. §3.2.4 整节未入摘要（切片 L265–271）。Ch5 L47 明引“读数与两条候选见 §3.2.4”；摘要缺 `res=127 zeroed=127`、15 条 conventional 在 DM 建立后长度读回 0、候选①页表不重放/②复用清零均未排除、N-5 为 print-only、深拷按 `[预防性不变量]` 读。最小修法：§3.2 段末补一条，保留两组读数与两候选的未定性。
2. §3.6 节末计数换算表未入摘要（切片 L511–528）。Ch5 L20/L22/L53 依赖成员差：四条有门链腿 = 五腿减 aarch64 直核；真机门链五条含 riscv64 U-Boot 直载测试链、不含 aarch64 直核；三类链 6。摘要只给五腿主计数。最小修法：补一条换算关系（成员名单可省）。
3. v10 删除四字段只写“删”（摘要 L19），未给下游要用的替代来源。切片 L100/L194：`kern_stack_top()` = `kern_virt_base + kern_size`、`user_sp` 读 `USER_STACK_TOP`、`syscall_entry` 取 `syscall_entry_va()`；Ch4 L260/L262、Ch6 L157 直接使用。最小修法：L19 末补三个替代来源。
4. 协议身份 v1 与信封版本 10 未分层（摘要 L21 写“wire v10”）。切片 L221–223：协议身份恒为 v1，信封版本当前 10；Ch4 L13 的“wire v1”与 Ch5 L13 的“wire”都指该协议。最小修法：L21 改为“v1 协议的信封版本 10”并点明两层版本号。
5. “两个 `.bss`”论证未成句（摘要 L18 只说接收侧落自己的 `.bss`）。切片 L76：两个分别编译的二进制各有 `.bss`，`&'static` 不保证对方页表映射；Ch6 L81 直接引 §3.2 的该论证。最小修法：L18 补一句。
6. DTB source 40 槽上限未入摘要（切片 L263）。Ch5 L27 的 I2 行以“DTB 收集器 40 槽上限”作 `[结构保证]`。最小修法：在 §3.3 或 §3.7 段补“U-Boot shim 经 `fill_source_records` 写 40 槽后序列化进 wire”。

忠实性（摘要说了正文没有的/过度简化）
7. 摘要 L23“U-Boot/直核腿声明 `DtbBootInput`”错误（切片 L104）：只有 riscv64 U-Boot shim 发送侧声明 `InputObjectId::DtbBootInput`；两个直核面在 `arch_boot` 调用点直接传 `RamAuthority::DTB`，不走 wire 声明。最小修法：改为“UEFI 发送侧 `UefiMemoryMap`、U-Boot shim 发送侧 `DtbBootInput`；直核面直接传 `RamAuthority::DTB`”。
8. 摘要 L49“`BootStorage` 承载六类载荷”绝对化（切片 L541/L585）：U-Boot shim/直核腿六类；UEFI 腿只五类（source 两阶段直写 wire，storage 的 source 切片为空）。L48 已提 source 为空，但 L49 仍会被下游当成所有腿。最小修法：L49 改为分族表述。

非阻断
9. §3.2.2 的 GET_KINFO 收窄（48 B/七字段，[ARCH]，切片 L177–185）与 `param_buf` 十条可观察分歧（L198–213）未入摘要；Ch4 L385 的 `mb_set_param` 改写、Ch6 L38 的参数读数因此缺摘要侧设计锚点。建议各压一句，未列入阻断。

通过项：五腿与三否一采、§3.1.1 接线状态、10 字段与 16→13→14→10、wire 128 B/六分区/32768/3584、两表与 `SourceRecord` 11 字段/覆盖审计、bootstrap 字段删除、分配器换手、`free_upper_idx`、取件抽象与 crate 单向、门链两序、`fill_*` 提交语义均与切片一致；未发现其他越界。
