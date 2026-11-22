# 冷读报告：`01-boot-shim-bootstrap.md`

读者画像：能读 C；知道引导器把控制权交给内核、物理/虚拟地址与页表的基本概念；能读 Rust 语法与所有权；不知道本项目的内部类型，也不知道三架构引导路径。只读了这一篇正文，没有读源码、其它文档、diff 或评审记录。

HARD-REJECTIONS: 0
VERDICT: READY

> 复检说明：正文写于旧版（P3 修复前）。当前文档 `19c0ad9a` 中 B1 已消失，最终结论以文末“## 复检（19c0ad9a）”为准。

## 五件学习任务

### 1. 引导交接的形状

**材料在哪。** §1.1 的三份数据表（可用内存清单、内核位置记录、交接结构）先给了形状和消费者；§1.2 的 I1–I4 给了四条判据；§3.2.1 的 `KernelInfo` 代码与字段表给了接收侧进程内结构；§3.2.3 的 `BootHandoffV1` 与 arena 说明给了跨镜像 wire；§3.3 给了两张内存表与 `SourceRecord`；§3.7 给了所有权；§4.4 给了 `BootReport<'_>` 到 `KernelInfo` 的构造与逐腿字段来源；§4.5 给了六个 `.bss` 落地区与容量表。

**够不够。** 对“形状”够：我知道交接的载荷是 `memmap`、`reserved_regions`、`boot_modules`、`platform_sources`、`param_buf`、`source_records` 六类，加上 `kern_virt_base` / `kern_phys_base` / `kern_size` 与 `authority`；三条跨镜像腿把载荷写进一个固定八页 arena 的固定分区，描述符只记 `byte_len`，地址由 arena 基址加协议偏移推导；两个直核腿借用 `BootReport` 后深拷；接收侧统一落进六个 `.bss` 落地区，再从私有存储重建切片。对“逐字节重建”不够：arena 各分区的起始偏移与每条记录的字节布局没有一张完整的表，只有信封 128 B、`WIRE_END = 7872`、source 分区至 29184 这几个数；`MemoryRegion`、`BootModule`、`SourceRecord` 的字段宽度也没有集中列出。如果目标是理解“交接是什么、为什么不能传指针”，材料够；如果目标是照文档写一个解码器，不够。

### 2. C 基线

**材料在哪。** §2.1 的 `boot.cfg` 三条指令、Multiboot 识别头、两个魔术值、`load_mods` 必须排在 `multiboot` 前面；§2.2 的 `head.S` 入口、设栈、`push` 次序与 SysV 栈对齐；§2.3 的 `get_parameters` 整份拷贝、`mb_set_param`、`rootdevname` 的完整落链；§2.4 的 `add_memmap` / `cut_memmap`、细表与粗档、`alloc_lowest` / `pg_alloc_page`、移交 VM 时的 16 条截断、模块与 unpaged 段的归属；§2.5 的 `overlaps` 与“内核当第 M+1 个模块”；§2.6 的模块装载三层检查；§2.7 的 ARM32 `setup_mbi` 对照；§2.8 的调用树与 D1–D5。

**够不够。** 够，而且是全文最强的一段。C 代码引用加上逐行解码，加上反事实推演，能让我这个 C 读者独立复述“引导器给什么、`pre_init` 怎么把它变成 `kinfo`、内存账怎么切、模块重叠怎么查、ARM32 为什么自己造一份 `mbi`”。§2.3 的 `mb_set_param` 删除扫描只对首条记录生效、§2.4.2.3 的 `alloc_lowest` 把长度传给结束地址参数，这两个是很具体的教学点，能让我看到 C 的真实边界行为。缺点是深水区太长（尤其 §2.3 的 `rootdevname` 链和 §2.4.2.1 的槽位顺序表），主线会被淹没；但材料本身够。

### 3. Rust 设计选择

**材料在哪。** §3.1 的协议换代与“三否一采”；§3.1.1 的接线状态表；§3.2 的 `KernelInfo`、wire、arena、字段台账、`GET_KINFO` 收窄、`param_buf` 十条可观察分歧；§3.3 的两张表与来源记录；§3.4 的 bootstrap 区间删除、页表分配器换手、`free_upper_idx`；§3.5 的 `FileLoader` 与 crate 依赖方向；§3.6 的五条生产腿、三个入口、`PreparedJump`、直核与跨镜像的分工、计数换算；§3.7 的所有权与门链。

**够不够。** 对“为什么这样选”够：按值交接、固定宽度 wire、来源记录、没有 unpaged 段、注册式页表分配器、`FileLoader` 接缝、单向 crate 依赖，每条都有理由，而且多数有反事实（保留 Multiboot、把 boot-shim 链进内核、保留 6 页静态池、借 PML4 槽位、让 `minix-kernel` 依赖 boot-shim）。对“一条线读完”不够：同一个主题在 §3.1 / §3.2 / §3.6 / §3.7 反复出现，术语和计数到 §3.6 才集中，读者需要自己搭地图。材料够，组织不够。

### 4. 实现路径

**材料在哪。** §4.1 的入口与 Step 0–4 总览；§4.2 的 `prepare_and_enter_kernel_image` 九个动作、时间拍、约束、A+D、EBS 配对、五步描述符分类、`MaxAddress` 界；§4.3 的 `FileLoader`、模块装载、`MODULE_NAMES`、A/B/C/D 防线；§4.4 的 `BootReport` → `KernelInfo`、逐腿字段来源、`boot_validate_and_prepare`、门表；§4.4.1 的门链；§4.5 的六个落地区、容量表、`land_and_store`；§4.6 的 `BootAlloc` `init`/`resume`、arena 生命周期、两个 bump 实例；§4.7 的 C→Rust 函数落点；§4.8 的 Direct Map 概念入口。

**够不够。** 对 UEFI 主线够：我可以沿 `main` → `prepare_and_enter_kernel_image` → A+D → 12 个模块 → 平台源与参数 → arena → EBS 配对 → 报告与断言 → `enter_kernel_image_v1` → 接收侧走一遍。九动作表与代码引用能对上，A+D 的“先预订 LMA 跨度再分配自举页”也有明确理由。非 UEFI 路径（riscv64 U-Boot shim、两个 `bootface`）比较薄，多处指向 02 篇或只给入口名；如果目标是“五条腿都走一遍”，不够；如果目标是“理解交接怎么落地”，够。动作 ID 与 Step 0–4 的编号冲突是额外的负担，但绕得过去。

### 5. 测试与诊断

**材料在哪。** §5.1 的四条不变量证据矩阵；§5.2 的宿主测试边界；§5.3 的“机制 → 需要证明什么”；§5.4 的五条真机门链与关键路标；§5.5 的缺口与关闭记录；§6.1 的 EBS 分界、两种裸串口写法、里程碑清单；§6.2 的 panic 路径；§6.3 的失败模式与信号；§6.4 的可复制命令（QEMU、`-d int`、GDB）。

**够不够。** 大部分够：我知道每条不变量有哪些宿主证据与真机证据、每条证据“不证明什么”、串口停在哪一行说明走到哪、panic 为什么不能分配也不能调固件服务、怎么用 `-d int` 和 GDB 抓现场。不够于“校验到底什么时候跑”：§3.7 与 §4.4.1 对最后一跳的归属相反（见 B1）——这是旧版判断；`19c0ad9a` 中两条链已对称，见文末复检。另外 §5.2 的宿主测试计数没有日期或 commit，我只能把它当成“某次读数”，不能判断是否仍对应当前代码。

## Blockers

### B1. §3.7 与 §4.4.1：`check_boot_postconditions` 的最后一次复核归属相反

**位置。** §3.7“所有权与校验门链（I3/I4）”的“校验的逐腿顺序”段；§4.4.1“门链与逐腿执行序”的开头段。

**引文 A（§3.7）：**

> “校验的逐腿顺序（§4.4.1 的权威表）：直核面 `validate_requiring_sources` → `validate_before_tables` → `check_boot_postconditions` → `store_boot_report` → `boot_validate_and_prepare` → `kmain` 的 `validate` → `check_boot_postconditions` 复核；v1 接收面 `validate_requiring_sources` → `check_boot_postconditions` → `resume_boot_pt_alloc` → `boot_validate_and_prepare`（在 `resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 `validate`，不经过 `validate_before_tables`。”

**引文 B（§4.4.1）：**

> “直核面是 **strict（`validate_requiring_sources`，两个 `bootface` 在调 `arch_boot` 之前显式调用）→ pre-table（2）→ postconditions（3）→ `store_boot_report`（`arch_boot_finish`，完整落一次）→ boot_validate（1，`arch_boot_impl` 开头）→ kmain validate**；v1 接收面是 **strict → postconditions（3）→ `resume_boot_pt_alloc` → boot_validate（1，`resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 `validate` → `check_boot_postconditions` 复核**”

**为什么是 blocker。** 两份都自称是按 §4.4.1 权威表给出的逐腿顺序，但最后一次 `check_boot_postconditions` 复核被放在相反的腿上：§3.7 说直核面有、v1 接收面没有；§4.4.1 说 v1 接收面有、直核面没有。§4.4.1 还单独说“接收面的 3 有可见调用点：`arch_boot_resume_v1` 在 `kinfo.validate_requiring_sources()` 之后、`boot_alloc::resume_boot_pt_alloc` 之前调用 `kinfo.check_boot_postconditions()`”——这是第一次调用；如果后面还有一次“复核”，就是第二次，但 §3.7 说 v1 没有。作为学习者，我无法判断 v1 接收面到底跑一次还是两次，也无法判断直核面是否有最后一次复核。I4 是“校验先于消费”，门链顺序是这一节的核心；这里的矛盾会让我把门链记错，而且我无法靠上下文自行裁决——两处都用了“权威表”的语气。

**我需要什么才能继续。** 一份逐腿的调用序，写明“谁、在哪个函数、调用几次、第一次是不是最后一次”，并把“复核”明确成“同一个门的第二次调用”或“另一个门的名字”。两条链对齐之后，这一节的其余部分（每条门挡什么 bug）我都能学。

## Optional improvements

这些不让我停住，但会让我多读一遍或读得更慢；按我读到的顺序列。

1. **§2.3 / §2.4.2.3：把深水区标成可选。** `mb_set_param` 的删除扫描和 `alloc_lowest` 的四前提推演都很有价值，但它们是“C 的边界行为”，不是交接主线。建议在 §2.3 开头加一句“第一遍可以跳过删除扫描与 §2.4.2.3 的四前提，先看 `get_parameters` 的五段输出和 `cut_memmap` 的两段补回”，主线会清楚很多。

2. **§2.4.2.1：槽位顺序表需要一张图或一个带具体槽值的例子。** 现在表格的单元格是“若 m+1 已被占用且其后有空槽…… → 断开；m+1 空闲时不断开”这类条件句，我能接受结论“C 不维护序”，但无法用这张表自己复算一次。给一个 5 槽数组、两次 `cut_memmap` 前后的槽位快照，会比条件表更直观。

3. **§3.2.1：字段计数历史太吵。** 当前结构是 10 个字段，代码已经给出；前面的“16→13→14→10”和“22 个 C 字段”是另一本账。建议用一句“历史计数只用于回查，当前以代码与字段表为准”带过，把正文留给 10 个字段的语义。

4. **§3.2.3 / §3.6 / §3.7：把 v1 接收路径的“谁解码、谁落盘”写成一句确定的话。** §3.6 的“接收侧先把六个分区直接解码进 `store_kernel_info` 的六个 `.bss` 落地区”可以读成“`store_kernel_info` 负责解码”；§3.7 说 `arch_boot_resume_v1` 先解码、`store_kernel_info` 只是之后对同一组地址的包装调用。建议在 §3.6 那一句直接写“`arch_boot_resume_v1` 解码进六个 store；`store_kernel_info` 随后只做包装，源与目标同址时跳过复制”。

5. **§4.1 / §4.2.1：给“Step 0–4”与“动作 1–9 / 时间拍 1–10”一张换算表。** 现在有两套编号，而且“2b”在两边都出现。文档已经提醒“不要混读”，但学习者需要的是映射，不是提醒。两列表格（Step 0/1/2/2b/2c/3/4 ↔ 动作 ID / 时间拍）能省掉一次来回翻页。

6. **§4.2.1：动作 ID 不按执行序，读者要多读一遍。** “动作 2a”在时间拍 1，“动作 1”在时间拍 2，这是有意为之，但我会先按动作 ID 读一遍、发现不对、再按时间拍读第二遍。建议把时间拍放第一列、动作 ID 放第二列，或者直接把动作按执行序重新编号。

7. **§4.4 与 §3.6 的 `arch_boot_finish` 引用不一致。** §3.6 的版本有 `register_panic_diagnostic()`，§4.4 的“节选”没有，而且省略说明只说“略去函数体中的诊断打印与注释”。这会让我短暂怀疑直核面到底注册不注册 panic 钩子；§6.2 才把它说回来。建议 §4.4 保留这一行，或在省略说明里点名“`register_panic_diagnostic` 也在此处，已略”。

8. **§5.2：给宿主测试计数加日期或 commit。** 188/0、136/0、866/0/8、27/0 是很有用的读数，但没有时间锚点。学习者没法判断这些数字是否还对应当前代码；加一个“读数日期 + commit”就能让它们变成可复核的证据。

9. **§5.1 的 I3 行太长，建议按腿拆表。** 现在一行里同时说了“四条腿有启动门链”“aarch64 直核只到 SMP 交付”“riscv64 `bootface` 与 aarch64 UEFI 的 DTB_CFGTABLE 形态只验证 fail-closed”“走到 `kmain` 的证据覆盖 x86 UEFI 与 riscv64 U-Boot shim”。拆成“腿 / 宿主证据 / 真机证据 / 不证明什么”四列，会比一个长句好读。

10. **§6.1：把“v1 腿门上这组打印两次”写清楚。** 这句可以读成“v1 发送腿自己打两次”，但表里的“适用腿”列写的是“两个直核面 + 三条 v1 发送腿 + v1 接收腿”。建议改成“v1 发送腿在跳转前打一次，v1 接收腿在解码后再打一次；出现第二组才是接收端已开始跑”。

11. **§3.6 的计数换算表来得太晚。** “五条生产腿 / 三条 v1 腿 / 两个直核面 / 三类链 / 真机门链五条”这些词从 §1 就开始用，换算表在 §3.6 末尾才出现。建议在 §1.0 的术语里放一个三行的小图例，或在 §3.1 开头先给一句“本篇主计数是五条生产腿 = 两条 UEFI 链 + U-Boot shim + 两个直核面；其余计数见 §3.6 末表”。

12. **§4.8 的 Direct Map 只有概念入口，建议补一张三格小图。** “DM 窗口 / free list / `reserved_regions`”三者的关系，我是从 §3.2.1、§3.4.3、§4.6 拼出来的；§4.8 只给了定义和“可访问不等于可分配”。如果目标是让学习者在交接阶段就带着正确的内存模型，一张“谁能分配、谁只能访问”的小图会比一段文字更省力。

## 复检（19c0ad9a）

**实际读的文档。** `/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`，sha256 `19c0ad9a306cc1e950b657ad6c6d1c34f3a0aa18256f238b048e7dbd9efc83dc`，2,951 行。协调者此前提到的 `f707a4e5` 已被这一版取代；本节按 `19c0ad9a` 复核。只重读了 §3.7 的“校验的逐腿顺序”段（第 1849 行）与 §4.4.1 的开头段（第 2379 行），没有重读全文。

### §3.7 当前句

> “校验的逐腿顺序（§4.4.1 的权威表）：直核面 `validate_requiring_sources` → `validate_before_tables` → `check_boot_postconditions` → `store_boot_report` → `boot_validate_and_prepare` → `kmain` 的 `validate` → `check_boot_postconditions` 复核；v1 接收面 `validate_requiring_sources` → `check_boot_postconditions` → `resume_boot_pt_alloc` → `boot_validate_and_prepare`（在 `resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 `validate` → `check_boot_postconditions` 复核，不经过 `validate_before_tables`。”

### §4.4.1 当前句

> “腿间实际执行序要说清：直核面是 **strict（`validate_requiring_sources`，两个 `bootface` 在调 `arch_boot` 之前显式调用）→ pre-table（2）→ postconditions（3）→ `store_boot_report`（`arch_boot_finish`，完整落一次）→ boot_validate（1，`arch_boot_impl` 开头）→ kmain validate → `check_boot_postconditions` 复核**；v1 接收面是 **strict → postconditions（3）→ `resume_boot_pt_alloc` → boot_validate（1，`resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 `validate` → `check_boot_postconditions` 复核**，且**不经过** pre-table（`resume_additive` 体内没有 `validate_before_tables`）。接收面的 3 有可见调用点：`arch_boot_resume_v1` 在 `kinfo.validate_requiring_sources()` 之后、`boot_alloc::resume_boot_pt_alloc` 之前调用 `kinfo.check_boot_postconditions()`。”

### B1 已消失

两条链现在都把最后一次 `check_boot_postconditions` 复核放在 `kmain` 的 `validate` 之后：

- 直核面：`… → boot_validate → kmain validate → check_boot_postconditions 复核`。
- v1 接收面：`… → boot_validate → store_kernel_info → kmain 的 validate → check_boot_postconditions 复核`。

旧版中“§3.7 把最后一次复核放在直核面、v1 没有”与“§4.4.1 把最后一次复核放在 v1、直核面没有”的对立不再存在。两条链在“有没有最后一次复核”这一点上对称；差异只剩包装调用的次序（直核面先 `store_boot_report` 再 `boot_validate_and_prepare`，v1 接收面反过来），而这一点 §3.7 第一段已经明确解释，和门链不矛盾。

### 新的硬阻断

无。重读范围内没有发现会让学习者停住或理解错的新矛盾。

有一处小问题，但不到硬阻断：§4.4.1 的“接收面的 3 有可见调用点”只点名了 `resume_boot_pt_alloc` 之前的那次 `check_boot_postconditions`；链上 `kmain` 的 `validate` 之后还有一次“复核”，它的调用点没有单独点名。句子说的是“有可见调用点”，不是“只有一次”，所以不构成矛盾；学习者按链仍能记住两次调用。作为可选改进，建议补一句“`kmain` 的 `validate` 之后还有一次复核，调用点与第一次不同”。

### 更新后的判断

RECHECK: PASS
HARD-REJECTIONS: 0
VERDICT: READY

正文第 5 件任务里“不够于‘校验到底什么时候跑’”一段是旧版判断，以本节为准：按当前两条对称的门链，我可以记住“两次 `check_boot_postconditions` + `kmain` 的 `validate` + 第二次复核”，测试与诊断这条学习任务的材料现在够用。其余四件任务的判断不变。
