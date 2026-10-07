# NK4-A 停止令交接文档（2026-09-21）

> 基线：`940ad8363`（接手前最后 commit）。本文档覆盖 `940ad8363..f1041b3f1` 共 7 个 commit。
> 任务来源：`notes/rewrite/fork-syscall-rewrite/HANDOFF-NK4A-boot-first-light.md` 任务1（首亮翻绿）。
> 翻绿判据：串口出现 `minix-rs rc: minimal boot script marker`（`os/etc/rc:10`）。**未达成。**

---

## §1 当前可观测状态

### 1.1 最新一次冒烟输出（原文）

**说明**：本轮（fix27 系列）没有再跑 `test-cmd-smoke.sh`——全部精力在完整镜像
（`cargo run -p xtask --release -- image --release` + QEMU 引导）的启动链取证上。
最新一次完整观测是 **fix37 轮**（PF 探针版镜像），日志
`tmp/nk4a/serial_fix37.log`（112 行），原文如下：

```
BdsDxe: loading Boot0002 "EFI Internal Shell" from Fv(...)
BdsDxe: starting Boot0002 "EFI Internal Shell" from Fv(...)
UEFI Interactive Shell v2.2
EDK II
UEFI v2.70 (Ubuntu distribution of EDK II, 0x00010000)
Mapping table
      FS0: Alias(s):F0a:;BLK0:
          PciRoot(0x0)/Pci(0x1,0x1)/Ata(0x0)
     BLK1: Alias(s):
          PciRoot(0x0)/Pci(0x1,0x1)/Ata(0x0)
Press ESC in 5 seconds to skip startup.nsh ...（倒计时略）
Shell> echo -off
boot-shim: prepare_boot enter
boot-shim: root+bump allocated
boot-shim: loading kernel.elf from ESP…
boot-shim: kernel loaded (entry staged)
boot-shim: 12 boot modules loaded
boot-shim: locating platform sources…
boot-shim: reserved-big ty=BOOT_SERVICES_DATA base=900000 pages=e80
boot-shim: reserved-big ty=BOOT_SERVICES_DATA base=1de79000 pages=715
boot-shim: reserved-big ty=BOOT_SERVICES_DATA base=1ed2e000 pages=400
boot-shim: reserved-big ty=BOOT_SERVICES_DATA base=1f7ff000 pages=601
boot-shim: memmaps conv=12 reserved=122
boot-shim: memmap ok (final pre-EBS snapshot)
boot-shim: building KernelInfo…
boot-shim: exiting boot services…
boot-shim: [raw] boot services exited
boot-shim: [raw] pt_alloc registered, entering kernel arch_boot
kernel: arch_boot arg=0x000000001fe962b8 blen=0x0000000000000000
kernel: arch_boot reslen=0x000000000000007a r0=0x0000000000000000+0x0000000000001000
kernel: arch_boot entered
kernel: entering validate
kernel: v0 enter
kernel: v1 asserts ok
kernel: v2 fallback region ok
kernel: v3 pt_alloc ok
kernel: step0 validate ok
kernel: step1+2 mappings ok
kernel: step4 DM coverage ok
kernel: arch_boot_impl done, jumping to kmain
kernel: preA rdi=0x000000001daf2120 kinfo=0x000000001daf2120 rsp=0xffff8000003cef18 blen=0x0000000000000000
kernel: kmain Phase A enter
kernel: kmain A.1 validate ok
kernel: kmain A.1b console+kinfo ok
kernel: kmain A.2a memmap copy ok
kernel: kmain A.2b module cuts ok
kernel: kmain A/A.2 memmap+modules ok
kernel: kmain A.5 platform ok
kernel: kmain B clock+intr ok
kernel: kmain B.5 kerninfo page ok
kernel: diag pc=0x0000000000238d00 pa=0x000000001fe3e000 flags=0x000000000000000d
kernel: vm_handoff free n=0x0000000000000009 deducted=0x0000000000000013
  base=0x0000000000001000 size=0x000000000009f000
  base=0x0000000000100000 size=0x0000000000100000
  base=0x0000000000808000 size=0x0000000000003000
  base=0x000000000080c000 size=0x0000000000004000
  base=0x00000000001780000 size=0x000000001a3f5000
  base=0x000000001bb95000 size=0x0000000001d49000
  base=0x000000001d980000 size=0x0000000000034000
  base=0x000000001dc19000 size=0x000000000001c000
nk4a: ident-windows res=0x000000000000007a zero=0x0000000000000000 low=0x0000000000000001 maxend=0x00000000fee01000 n=0x000000000000000c
kernel: kmain C proc_init ok
kernel: kmain D memory_init ok

MINIX-RS 0.1.0 (rust rewrite) — scheduling live
nk4a: pick->0x0000000000000008
nk4a: sa0-0x0000000000000008 root=0x0x000000001de78000 cur=0x0x000000001de78000
nk4a: probe text va=0x0x000000001dab5000 -> pa=0x0x000000001dab5000 fl=0x0x000000000000041b
nk4a: probe stk va=0x0xffff8000003ce000 -> pa=0x0x00000000003ce000 fl=0x0x000000000000041b
nk4a: probe dm va=0x0xffff808000200000 -> pa=0x0x0000000000200000 fl=0x0x0000000000000413
nk4a: sa1-after cr3=0x0x000000001de78000
nk4a: pre-restore-
nk4a: birth enternk4a: birth s3 runtime oknk4a: birth s5 -> mainnk4a: vm enternk4a: params read oknk4a: server new oknk4a: relocate oknk4a: vm slot oknk4a: setaddr nr=0x0000000000000006 flags=0x0x0000000000018080 runnable=no queued=no
nk4a: bootinh-clear nr=0x0000000000000006 flags=0x0x0000000000008080 runnable=no queued=no
nk4a: exec ds ok ...（12 个 boot 模块 exec ok，逐条略，全文见日志）...
nk4a: exec init oknk4a: init donenk4a: run enternk4a: pick->0x0000000000000002
nk4a: sa0-0x0000000000000002 root=0x0x00000000035b2000 cur=0x0x000000001de78000
nk4a: probe text va=0x0x000000001dab5000 -> pa=0x0x000000001dab5000 fl=0x0x000000000000001b
nk4a: probe stk va=0x0xffff8000003ff000 -> pa=0x0x00000000003ff000 fl=0x0x0000000000000013
nk4a: probe dm va=0x0xffff808000200000 -> pa=0x0x0000000000200000 fl=0x0x0000000000000013
nk4a: cr3-done-0x0x00000000035b2000
nk4a: pf#0 rip=0x000000001dab2b12 err=0x0000000000000000
nk4a: pf#1 rip=0x000000001dad427b err=0x0000000000000000
nk4a: pf#2..11 rip=0x000000001dad427b err=0x0000000000000000（同址重复，探针限 12 次后静默）
```

### 1.2 最后出现的路标（逐行）

| # | 最后几行路标 | 出处 |
|---|---|---|
| 1 | `nk4a: cr3-done-0x35b2000` | `os/kernel/src/vm_handoff.rs`（switch_to 的 CR3 写回后探针） |
| 2 | `nk4a: pf#0 rip=0x1dab2b12 err=0` | `os/kernel/src/trap_dispatch.rs:96-110`（PF 探针） |
| 3 | `nk4a: pf#1..11 rip=0x1dad427b err=0`（之后 Triple fault） | 同上 |

### 1.3 当前挂点

- **首个 #PF**：`rip=0x1dab2b12`（fix35 轮同位为 `0x1dac2b12`，差异 = UEFI 加载基漂移），
  `CR2=0x10`（int_fix35.log `v=0e e=0000 IP=1dac2b12 CR2=0x10`）。即内核在 VM(proc 2)
  页表上、`cr3-done` 之后，**解引用了一个空指针 + 0x10 偏移**（非规范低地址）。
- **递归点**：`rip=0x1dad427b`，err=0，共 4686 次（int_fix35.log 聚合计数），每轮 SP 递减
  ~0x1d0，直至内核栈耗尽 → Double Fault → Triple Fault → QEMU 复位。
- **源码定位状态**：未完成。两个 rip 都无法离线换算成符号（BOOTX64.EFI 是 PE32+ 且
  `objdump -t` 无符号表，加载基每次运行漂移）。已确认的事实边界：
  - PF 能到达 kernel body（探针打印了 12 条），即 asm stub → `x86_trap_dispatch`
    （`os/arch/src/x86_64/trap_stub.rs:86`）→ TRAP_DISPATCH 指针 →
    `x86_trap_dispatch_body`（`os/kernel/src/trap_dispatch.rs:86`）这条链是通的；
  - 内核态（is_user=false → is_nested=true）的 vector 14 在
    `os/arch/src/arch/exception_dispatcher.rs:166-168` → `:202-204` 返回
    `ExceptionOutcome::KernelPanic(14)`；
  - KernelPanic 臂（`os/kernel/src/trap_dispatch.rs:331-357`）第一件事是 console 逐字段
    dump，然后 `panic!` → minix-rt handler → `kernel_panic_diagnostic`
    （`os/kernel/src/lib.rs:2037-2065`）→ `util_stacktrace`。
  - **串口上没有出现 `trap: vector` 与 `kernel panic:` 任何一行** → 递归故障发生在
    KernelPanic 臂的 console dump 之前，或在 panic 渲染路径内部（渲染路径自身解引用
    同一个 null+0x10）。`1dad427b` 与 `1dab2b12` 相距约 0x1769 字节，两者应同在
    PF 处理调用树内。 strongest 未验证候选：`util_stacktrace` 的栈走查、
    `minix_types::run_panic_diagnostic_hook` 的钩子状态、BKL section 相关路径。

---

## §2 当前根因假设

**主假设（置信度 ~70%）**：cr3-done 后内核继续执行 `switch_to`/调度返回路径的剩余代码，
该路径解引用了一个在此时刻仍为 null 的全局指针（对象字段偏移 0x10）；随后 PF 处理走
KernelPanic → panic 渲染链，渲染链（或其重入的 trap 路径）又解引用同一个 null 指针，
形成 #PF→handler→#PF 递归直至栈尽。页表机制本身已闭环（三个 probe 全 hit、
`fl=0x1b` 带执行位、12 个模块 exec ok），**这是内核运行期逻辑 bug，不再是 fix27 域问题**。

支撑证据（逐条）：
1. `nk4a: cr3-done-0x35b2000`（serial_fix37 L99）——CR3 切换成功返回，此后内核仍在执行
   （否则不会有后续串口输出）。
2. `probe text/stk/dm` 全 hit 且 `fl=0x1b`（serial_fix37 L96-98；`PageFlags` 位定义
   P|W|E|G = 0x1B，`os/arch/src/arch/paging.rs:122`）——身份窗口的取指/数据/栈访问语义
   正确，fix27e 生效。
3. `pf#0 ... rip=0x1dab2b12 err=0` + int_fix35 `CR2=0x10`——首故障是 supervisor 读
   非规范空地址，不是缺映射（err 位 0=not present 但 0x10 不可能是合法 VA，属程序性
   空指针而非页表遗漏；页表遗漏的高半区地址会打印真实 VA）。
4. `pf#1..11` 全部同址 `0x1dad427b`、err=0、int_fix35 显示 SP 单调递减 4686 次——
   处理器自身故障并递归，排除了"随机踩内存"（地址、偏移、模式完全确定性）。
5. 串口无 `trap: vector` / `kernel panic:`——递归点在 KernelPanic 臂 console dump
   之前或渲染路径内（§1.3 边界）。

**备选假设**（按置信度降序）：
- B2（~20%）：`exception_frame_of` / `smp_state_boot_unchecked`（trap_dispatch.rs:233-252）
  在 PF 重入时把 body 自身的某个静态（如 BKL 状态、CPU local 表）以 null 中间态解引用——
  与主假设同类，但递归点在 dispatcher 前段而非 panic 渲染段。
- B3（~5%）：首故障指令在 `pre-restore` 之后的 context restore asm 路径里引用了
  GDT/TSS 相对 null 的字段——无直接证据，仅因 int_fix35 现场 TR/GDT 均指向低段 .bss
  而保留。
- B4（~5%）：identity window 缺页导致的"合法" #PF 被误判——基本排除：CR2=0x10 不在任何
  reserved/窗口的候选里（serial_fix37 `maxend=0xfee01000`、`low=1` 已把 4MiB 以下全部
  拒绝入窗），且 err=0 的 I/D 位组合与取指违例（err=0x11，fix33 已修）不同。

**距离 rc marker 还差的步骤**：① 定位并修复 null+0x10 解引用（本挂点）→ ② 内核完成
switch_to 进入用户态 VM/PM/init → ③ init `execve /etc/rc` → ④ rc marker 打印。当前卡①。

---

## §3 每个 commit 的详细解释

（按时间序；`git log --oneline 940ad8363..f1041b3f1` 可复核）

### 1) `d697c481e` WIP fix(edge1,nk4a,fix27a): 类型/协议层——handoff v6 身份窗口字段 + KernelInfo.reserved_regions 全构造点同步
- 解决什么问题：身份窗口机制需要把"固件报告的占用内存"从 boot-shim 传到内核再传到
  页表构造，第一步是给这条链加类型：`KernelInfo` 新增 `reserved_regions: &[MemoryRegion]`
  字段、handoff 协议升 v6、全部 37 个 `KernelInfo` 构造点（含 qemu-tests 测试内核、
  mock 工厂）同步补齐字段。
- 关键改动意图：纯 additive——没有任何守卫/断言被删；测试内核构造点填 `&[]` 或固定
  数组保持既有行为。
- 事后评价：**对**。若重做仍会这么拆（类型层先行让每个下游 commit 可独立编译验证）。

### 2) `77d50669b` WIP fix(edge1,nk4a,fix27b): 身份窗口数据源+构造+映射
- 解决什么问题：三端落码——shim `build_memmaps` 产出双清单（conventional + reserved，
  非 MMIO 非 conventional 的全部条目进 reserved）；内核 `vm_handoff::build_identity_windows`
  （`os/kernel/src/vm_handoff.rs:446`）做"过滤(<4MiB 地板)→排序→合并重叠→加 arch MMIO"
  产出窗口数组；`map_kernel` 新增第 4 段按窗口做身份映射（paging.rs:621-646）。
- 关键守卫：窗口生产者合同"page-aligned/sorted/non-overlapping"，消费者
  `debug_assert!` 校验（paging.rs:639）——**未放松**。`MAX_CANDIDATES=128` + assert。
- 事后评价：**方向对，参数与前提错了两处**（128 太小、默认全 reserved 可用——分别由
  fix27d/fix27e 修正），这些错误是当时信息不足下合理的先行假设。

### 3) `ffff852f6` WIP feat(edge1,nk4a,fix22-26): VM 侧拆分机制与 handoff 消费
- 解决什么问题：VM 服务器侧消费 handoff v6：adopt 内核复制的页表根、Direct Map 真值 v5、
  身份窗口接线（VM 给被接管进程建页表时把窗口并入）、新 crate `bootmark`（启动路标）。
- 关键改动意图：`vm_server.rs` +181 行是 handoff 解析与窗口传递主逻辑；无删守卫。
- 事后评价：**对**。宿主测试全绿，真机 `exec ds/rs/pm/...` 12 模块 ok 证明机制有效。

### 4) `9e115387e` WIP chore(edge1,nk4a): 取证路标与历轮迭代累积
- 解决什么问题：可观测性。kmain/arch_boot/switch_to/probe 路标组；**boot-shim panic
  handler 从静默 `loop {}` 改为串口可见化**（`os/boot-shim/src/main.rs:72-111`）；
  syscall/trap/crt0 诊断迭代；`minix-sys/build.rs` 改动。
- 放松的守卫（如实）：panic handler 行为变化属"从吞到放"，是增强不是放松；
  `kernel_panic_diagnostic` 的防重入位 `PANIC_DIAG_ACTIVE`（lib.rs:2060）会让第二次进入
  panic 诊断时跳过 stacktrace——这是防刷屏的有意行为变化，不是放松检查。
- 事后评价：**对**，且是本 session 全部定位能力的地基。

### 5) `dd484552c` docs(edge1): NK4-A 交接文件补基线记录
- 纯文档（HANDOFF 文件 +6 行，记录基线 = 940ad8363）。**对**。

### 6) `a9f8681fc` WIP fix(edge1,nk4a,fix27cde): 身份窗口三连环修正真机贯通
- 解决什么问题（三件事一条 commit，事后看应该拆三条）：
  - **fix27c**：reserved 载荷在 shim UEFI-pool 堆上，higher-half 跳转后同一 VA 读回全零
    （真机 `zeroed=127` 实锤）。修法：`store_kernel_info`（`os/kernel/src/lib.rs:486`）把
    载荷深拷进 .bss 固定数组 `RESERVED_REGION_STORE[256]`（`os/kernel/src/globals.rs:296`）
    并 repoint slice；`bkl_protected_impls!` 名单**新增** `minix_boot::MemoryRegion`
    （write-once-after-boot 合同）；
  - **fix27d**：QEMU i440fx 在 1TB 处报 `RESERVED base=0xfd00000000 pages=0x300000` 芯片组
    洞 + 过滤后仍 128 条撞 `MAX_CANDIDATES=128`。修法：shim 侧用 conventional 最大 end
    作 `ram_top`，全高于它的 reserved 描述符丢弃（uefi_helpers.rs）；`MAX_CANDIDATES`
    128→256（vm_handoff.rs）；
  - **fix27e**：身份窗口用 `kernel_read_write()`（无 EXECUTABLE），OVMF 已开 EFER.NXE →
    CR3 切换后首条取指 #PF err=0x11。修法：新构造器 `PageFlags::kernel_executable_writable()`
    （paging.rs:122，P|W|E|G），段 4 改用它，mock 断言（paging.rs:1141）同步。
- 放松的守卫（如实）：`MAX_CANDIDATES` 128→256 是放宽，但它配合 ram_top 过滤是"兜底网
  放大"而非删除检查；窗口 flags 从 RW 改 RWX 是**安全性让步换可用性**，加了 [ARCH]
  注记（paging.rs:635-636：按节切窗需 loader 段级信息）。
- 事后评价：**三处修复都必要且真机验证有效**；混在一条 commit 是错误（不利于回滚归因），
  重做会拆三条。

### 7) `f1041b3f1` WIP(stop): 停止令全量落盘
- trap_dispatch.rs 的 PF 探针（真机已命中递归点）+ 142 项历轮遗留 untracked 文件。
- 事后评价：PF 探针本身**对**（一发命中）。untracked 全量入库是执行停止令的指令动作；
  其中 `.trae/documents/`、`AI-chats/`、`notes/` 下非本 session 产物是否该入库请仓库
  主人复核（见 §5-⚠3）。

---

## §4 问题编年史

| # | 症状 | 诊断 | 修法 | 结果 |
|---|------|------|------|------|
| 1 | 真机 `ident-windows n=2`（窗口只有 2 条，预期含全部 reserved） | reserved 到达内核但全被 push 拒绝 | 加分因计数探针（zeroed/below_floor/max_end） | 探针建设完成，引出 #2 |
| 2 | 分因显示 `zeroed=127`（载荷全零） | 载荷在 shim UEFI-pool 堆（`Box::leak`），higher-half 后内核页表不复放 UEFI 1:1 映射 → 同 VA 读零 | fix27c：.bss 深拷 + repoint（用 `ptr::copy` 容忍 kmain 二次 store 时源=目的的自拷贝） | **解决**（下轮 zero=0） |
| 3 | E0503 编译错（闭包可变捕获 vs 循环后读取） | push 闭包借用 zeroed 等 | 循环后 `drop(push)` | 解决；**教训**：宿主 check 判读混乱导致漏检，image 构建 exit 101 才暴露 |
| 4 | `reserved-big` 打印 `RESERVED base=0xfd00000000 pages=300000`；128 条撞 assert | QEMU 芯片组 1TB 高洞 + 上限过小 | fix27d：ram_top 过滤 + MAX_CANDIDATES 256 | **解决**（n=12） |
| 5 | `e=0x11` 取指保护违例（IP=CR2=0x1daf0d9d，int_fix33） | 窗口无 EXECUTABLE 位而 OVMF 开 NXE；窗口混 code/data/heap，RWX 是唯一 sound 公共集 | fix27e：`kernel_executable_writable()` + [ARCH] 注记 | **解决**：真机首达 cr3-done，probe 全 hit |
| 6 | E0277：`MemoryRegion` 不满足 BklProtected sealed 惯例 | static 类型需显式获批 | `bkl_protected_impls!` 加条目（合同注记） | 解决 |
| 7 | E0609：`no field error_code on TrapFrame` | 字段实名 `errcode` | 改字段名 | 解决；**教训**：宿主 dev/mock 组合**不编译** `cfg(not(mock))` 分支，字段错只有 uefi-target（xtask image）构建暴露 |
| 8 | fix36 轮跑了旧镜像浪费一次 QEMU | IMG=1（构建失败）未检查就跑 | 之后每次确认 IMG=0 + strings 验证 EFI | 流程修正 |
| 9 | paging.rs 编辑残留 8 行旧循环体 | SearchReplace original_text 覆盖不足 | 读现场后二次清除 | 解决 |
| 10 | cr3-done 后 `v=0e IP=1dac2b12 CR2=0x10` + handler 树 `1dae6869/1dad427b` 递归 4686 次 → Triple | 新挂点：内核运行期空指针解引用（非页表机制），PF→KernelPanic→渲染路径再故障 | 部署 PF 探针（rip/errcode 前 12 次） | **诊断中（未解决）**：探针一发命中递归点 `1dad427b`；源码定位在停止令时未完成 |
| 死胡同登记 | ①曾按 PE ImageBase 0x140000000 离线换算 RIP→符号：加载基每次运行漂移，**此路不通**，唯一可行路线是真机探针自报 rip + 源码二分；②曾怀疑"MMIO 类型过滤名不匹配"（uefi 0.33 Debug 打印名与实名不同导致 `MMIO/MMIO_PORT_SPACE` 排除是否生效存疑）：后由 reserved 计数与 n=12 证实过滤生效，非根因 | | |

---

## §5 自我批判

### 最没把握的 3 处

1. **fix27e 身份窗口 RWX**（`os/arch/src/arch/paging.rs:637`）。整段低物理内存（含
   reserved 区）在 VM 页表上"可写且可执行"。这是为了让 boot 走远的实用让步：UEFI 内存图
   没有 W^X 意图信息，窗口又混装 code/data/heap。风险：任何内核态写漏洞升级为可注入
   执行。重做方案：让 loader（boot-shim）传递自身镜像的段表（text/data/bss 的 PA 区间），
   窗口按节拆成 RX / RW-only 两类——需要扩 `KernelInfo`，但信息 shim 手里现成。
2. **fix27d 的 ram_top 过滤判据**（uefi_helpers.rs）。用"conventional 最大 end"作内存顶
   剔除其上全部 reserved。QEMU/常规 PC 上正确（真机已验证），但假设有边界：若某固件的
   高地址 reserved 设备内存（APEI/PMEM/热插拔窗口）与 conventional 交错分布，会把合法
   描述符一起丢弃。验证方式只有 QEMU 单点，未跑过其他 machine type（q35/物理机）。
3. **fix27c 深拷贝的隐式合同**（`os/kernel/src/lib.rs:486-500`）。`store_kernel_info`
   假设：单线程 boot 期调用、src.len() ≤ 256（assert 钉住）、kmain 二次 store 时
   源 slice 可能恰等于 store 自身（用 `ptr::copy` 容忍同区间）。这三条只有注释和一条
   assert 保护；若未来有人在线程化上下文调 `store_kernel_info` 或改 store 数组长度与
   `MAX_CANDIDATES` 脱钩，会静默产生悬垂 slice 或 UB。两处 256 是**手工对齐的两个常量**
   （globals.rs:296 与 vm_handoff.rs MAX_CANDIDATES），无编译期链接——最该钉一条 const
   assert 而没钉。

### 后来者优先复核清单

- `os/kernel/src/trap_dispatch.rs:96-110` PF 探针与 `:331-357` KernelPanic 臂：递归点
  定位必须沿这两段的真实执行序走，注意我留下的"串口无 `trap: vector`"这一关键否定证据。
- `os/kernel/src/globals.rs:296` `RESERVED_REGION_STORE` + `bkl_protected_impls!` 的
  `minix_boot::MemoryRegion` 条目：write-once-after-boot 合同是否会被后续代码破坏。
- `os/boot-shim/src/uefi_helpers.rs` build_memmaps 的三路分发（conventional / MMIO 排除 /
  ram_top 过滤）：与 uefi 0.33 `MemoryType` 实际枚举值再核一遍（打印名与实名不一致陷阱）。

### 是否为"让 boot 走远"放松过检查——逐条如实

- **没有删除或注释掉任何 assert/panic/守卫**。
- 放宽 2 处：① `MAX_CANDIDATES` 128→256（容量放宽，配套真因修复仍在）；② 窗口 flags
  RW→RWX（安全性放宽换功能，有 [ARCH] 标注，mock 断言同步更新）。
- 行为变化 1 处：`PANIC_DIAG_ACTIVE` 防重入会跳过二次 stacktrace——为串口可读性设计，
  非绕过失败信号。
- 灰色 1 处：`bkl_protected_impls!` 新增 `MemoryRegion` 是扩了"无 BKL 读共享"的名义面，
  理由是 boot 后只读，但这依赖纪律而非类型系统。
- 另：停止令 commit `f1041b3f1` 把 142 项**非本 session 产生**的 untracked 文件一并入库
  （执行"git add -A"指令的副作用），其中可能含不应入库的杂物，请复核后 `git rm --cached`
  分流。

---

## §6 环境与陷阱清单

| 陷阱 | 验证方式 |
|------|----------|
| **宿主 `cargo check -p minix-kernel` 不编译 `cfg(not(feature="mock"))` 代码块**——路标/探针里的字段名、类型错误宿主全绿，只有 uefi-target 构建暴露 | fix36→fix37：`frame.error_code` 错误宿主 CHK=0，`xtask image` IMG=1 才报 E0609 |
| **镜像构建失败（IMG≠0）时 `run_fix8.sh` 会跑旧镜像**，产出假进展 | `grep 新marker 日志` 为空 + 检查 `$?`；此后每轮先确认 IMG=0，或 `strings BOOTX64.EFI \| grep 新符号` |
| QEMU 残留进程导致下一轮起不来/串口互踩 | 每次 QEMU 前 `pgrep -f '[q]emu-system-x86'` 输出必须 0 |
| 宿主构建 OOM（链接期内存峰值） | `ulimit -v 3145728` 下构建；超限表现为静默 kill |
| 终端长输出直接判读会截断/错乱 | 一律 `> /tmp/xx.log 2>&1` 或 `tmp/nk4a/*.log` 落盘再 grep |
| BOOTX64.EFI 无符号表（`objdump -t` 空）+ UEFI 加载基每次运行漂移（同函数 RIP 两轮差 0x10000） | PE 头解析 + 两轮日志对比；结论：RIP→符号必须真机探针自报，离线换算死路 |
| uefi 0.33 `MemoryType` 的 Debug 打印名 ≠ 枚举实名（如打印 BOOT_SERVICES_DATA 实为 MMIO 系），grep 日志时按打印名找会误判过滤没生效 | reserved-big 打印 vs 类型实名核对 |
| 宿主测试（mock）下 EarlyConsole 是真实端口写，测试进程 SIGSEGV | 所有路标 `#[cfg(not(feature = "mock"))]` gate（lib.rs/trap_dispatch.rs 路标段惯例注释） |
| `bash tmp/nk4a/run_fix8.sh` 必须从仓库根调用（内部相对路径） | 从 os/ 内调用直接 No such file |
| ld script 改动不触发重链（前轮记录，本 session 未独立复现） | 沿用规避：改 ld 后 `touch` 关联 .rs 强制重链；标记为"继承知识，待复核" |
| SearchReplace 编辑 paging.rs 曾因 original_text 覆盖不足留下悬挂代码 | 大范围编辑后必 `sed -n`/Read 复读改动区 ±20 行 |

---

## §7 如果继续，下一步怎么做（按优先级）

1. **二分递归点 `0x1dad427b`**：在 `x86_trap_dispatch_body` 的 vector==14 非 mock 分支
   里，于 `ExceptionDispatcher::handle` 调用前后、KernelPanic 臂 console dump 前后各加
   一条限次 raw 路标（复用 PF_MARK 模式）。预期成功判据：串口出现新的
   `nk4a: pfpre`/`pfpost`/`kppre` 路标且 `pf#n` 的 rip 在两条路标之间跳变——即把递归
   锁定到"body 前段 / handle 内 / KernelPanic dump 内 / panic 渲染链内"之一。
2. **若是 panic 渲染链内（最可能，因串口无 `trap: vector`）**：读
   `os/kernel/src/stacktrace.rs` 的 `util_stacktrace` 与 `minix-rt` panic handler 的
   formatter 路径，找解引用偏移 0x10 的空指针（候选：钩子指针状态、栈帧链头、
   CPU id 读取）；修复后重建镜像跑 QEMU。预期判据：`pf#` 消失，出现
   `trap: vector 14 ...`（若首故障仍在则至少 KernelPanic 诊断完整打印，暴露首故障
   现场 rsp 附近内容）。
3. **修掉首故障 `0x1dab2b12`（CR2=0x10）本体**：cr3-done 之后的代码即
   switch_to 尾部 → context restore → 用户态跳转路径。在 `vm_handoff.rs` switch_to
   腿路与 `scheduler_loop` 入口加成对路标。预期判据：路标越过原 rip 位置继续打印
   （如 `nk4a: user-enter`），或 `pf#0` 的 rip 移到更靠后的调用点。
4. **首亮验证**：pf 全消后按判据链推进——VM/PM 用户态 IPC 活跃 → init execve
   `/etc/rc` → 串口出现 `minix-rs rc: minimal boot script marker`。出现该行即任务1
   翻绿。
5. **task1-close（不得跳过）**：路标大清除裁决（本 session 新增：`reslen/r0`、
   `reserved-big`、`memmaps conv=`、`ident-windows` 分因 marker、`probe text/stk/dm`、
   `cr3-done`、`pf#` 探针——全部是临时件）+ FIXLOG edge1 #9 续（fix27cde 全案 +
   两条 [ARCH]）+ `cargo clippy` 全量对账（基线 `tools/clippy_base.txt`）+ 宿主测试对账
   （基线 kernel 806 / arch 241）+ 分逻辑单元重提交。

---

## 评审问答

**Q1. boot-shim 原来的 `#[panic_handler]`（无声 loop）被你删除了——现在 panic 由谁接管？接管方在 EBS 前后各是否能输出？**

前提修正：**没有删除**，是把同一个 handler 从静默改为可见化
（`os/boot-shim/src/main.rs:72-111`，commit `9e115387e`）。现在 panic 由该
`#[panic_handler]` 接管（内核镜像与 shim 同 bin 链接，Phase A 前所有 panic 都走它）。
输出能力：EBS 前——`raw_serial_line`（COM1 0x3F8 端口直写）与 `uefi::println!` 双通道
均可用；EBS 后——仅 `raw_serial_line` 可用（`[raw] boot services exited` 路标实证其
EBS 后有效）。handler 走 raw 通道，故 EBS 前后都能打出 panic 位置+消息，随后尝试
`minix_types::run_panic_diagnostic_hook`（钩子在 `arch_boot` 入口注册，注册前会打印
"hook not registered"），最后 `spin_loop` 驻留。

**Q2. 你新增的每一处 unsafe block 的 SAFETY 理由一句话复述。**

按本 session 三个 fix commit 的核心新增逐条列（完整清单见
`git diff 940ad8363..HEAD -- os/*.rs` grep `^\+.*unsafe`，其中大量条目属于 fix22-26 的
页表读写函数，一并列出）：
- `store_kernel_info`（lib.rs:486）：单线程 boot 期、载荷拷入 'static .bss 数组、长度有
  assert、自拷贝情形用允许重叠的 `ptr::copy`——无悬垂无竞争。
- `RESERVED_REGION_STORE` 的 `&mut *get()`：同上，boot 期 write-once，后续只读，符合
  BklProtected 名单里新批的合同。
- PF 探针（trap_dispatch.rs:95-110）：不新增 unsafe block——所在函数本就是
  `unsafe extern "C"`，契约（frame 指向被打断栈上的活 TrapFrame）由 asm stub 入口保证；
  探针只读字段 + 端口写。
- `cr2` 读（trap_dispatch.rs:370）：`mov r, cr2` 无内存副作用，options(nomem, nostack)
  正确声明。
- shim panic handler（main.rs:72-111）：全部为 safe 栈缓冲操作，无新增 unsafe。
- fix22-26 页表腿（`read_pte_dm`/`write_pte_dm` 各处，diff 行号 517-649）：DM 窗口已
  建立且覆盖目标 PA（`step4 DM coverage ok` 路标即其自证），地址由 root_paddr+索引算出，
  页对齐。
- `jump_to_kmain` 调用点（arch_boot）：换页+跳特权级 asm，never returns 契约由
  kmain 尾部循环保证。
- `try_smp_state` 裸指针解引用（kernel_panic_diagnostic 内）：static 地址恒定，
  Option 判空后才用，失败回退 BSP=0——诊断路径禁 expect 的契约实现。

**Q3. `assert_bootstrap_outside_memmap` 现在挂在哪个位置、保护什么、为什么这个位置是对的？**

位置：`os/boot-shim/src/uefi_helpers.rs:201-202` 调用（定义 :357），挂在 memmap 最终
快照取得之后（`memmap ok (final pre-EBS snapshot)` 路标之后）、`build_kernel_info`
之前，两条分别断言页表根页与 bump 区不与快照中任何 conventional 区间重叠。
保护什么：shim 的页表根 + bump 分配区是 LOADER_DATA 分配；若固件把它们的页报成
conventional，内核 A2 分类（conventional → 空闲内存移交 VM PMM）会把**正在使用的页表
页发给 PMM 当自由内存复用**（设计文档 07-paging_init_design §6.0-A2 的失效模式）。
为什么位置对：① 必须在快照后——快照前分配内容未定，断言对象不存在；② 必须在
KernelInfo 构造/EBS 前——此刻 shim 还能 `uefi::println!` 报告断言失败，且错误图尚未
被任何下游消费；EBS 后既无输出通道也无回退余地。fail-fast 把最坏的静默内存腐蚀转成
开机即见的 panic。

**Q4. 你改动前后，内核传给 VM PMM 的 conventional 内存图有什么语义差别？**

**conventional 条目的集合与语义没有变化**：`build_memmaps` 中所有
`MemoryType::CONVENTIONAL` 描述符依旧逐条进 `regions`（fix27d 的 ram_top 计算读取
conventional 仅为求最大 end，不修改该列表；`git show a9f8681fc` 对 uefi_helpers.rs 的
diff 可复核——过滤分支只作用于 reserved 腿）。serial_fix37 `memmaps conv=12` 与前轮
一致佐证。真正的语义增量在 conventional 之外：新增平行的 reserved 清单 + 身份窗口
派生数据（handoff v6），以及内核 free 列表侧既有的 bootstrap/路标页扣除
（serial_fix37 `free n=0x9 deducted=0x13`，那是 fix27 之前就有的路径）。一句话：
VM PMM 看到的"可分配空闲内存"定义未动；动的是"不可动占用内存"多了一份经真机三连环
修正后可靠的描述，供被接管进程页表建身份窗口用。

**Q5. 你的 vm_handoff/身份窗口机制有没有对应的测试？测试钉住了哪些不变量？**

有，宿主 3 个：
- `test_identity_windows_clip_sort_merge`（`os/kernel/src/vm_handoff.rs:824`）钉住：
  4MiB 地板以下条目全部丢弃；输出按 base 排序；重叠区间合并（0x1da00000+0x3000 与
  0x1da02000+0x2000 → 单条 0x4000）；每条窗口 base/size 页对齐、base ≥ 4MiB；任意两条
  不重叠（map_kernel 免 AlreadyMapped 的生产者合同）；arch MMIO 区恒在。
- `test_identity_windows_empty_reserved_still_has_mmio`（:863）钉住：reserved 为空
  （OpenSBI/宿主形态）时只剩 MMIO 窗口，不炸。
- `test_mock_map_kernel` 段 4 断言（`os/arch/src/arch/paging.rs:1141`）钉住：身份窗口
  flags == `kernel_executable_writable()`（fix27e 的 NX 教训被编译期钉死）。

**测试缺口（如实）**：fix27c 深拷贝落位/自拷贝分支无宿主测试；fix27d ram_top 过滤在
shim 侧（uefi bin 无法宿主单测，只有真机 reserved-big 路标验证）；
`RESERVED_REGION_STORE` 长度 256 与 `MAX_CANDIDATES` 256 的一致性无编译期检查（§5-⚠3）；
MAX_CANDIDATES 耗尽 panic 分支无测试。

---

*本文档为停止令唯一产出物。其后的仓库状态 = `f1041b3f1` + 本 docs commit。*
