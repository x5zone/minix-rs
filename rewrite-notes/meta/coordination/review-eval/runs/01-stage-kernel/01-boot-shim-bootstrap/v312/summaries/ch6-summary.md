# 第 6 章摘要：诊断

## 本章回答什么

引导失败了怎么查：6.1 讲 EBS 前后两条输出通道与里程碑路标的读法，6.2 是从静默到可读的 panic 路径，6.3 列典型失败模式与对应信号，6.4 给可复制命令，6.5–6.8 给失败对照、最小示例、小结与术语。本章按用途查，不按执行顺序读。

## EBS 分界与两条通道（§6.1，第 17–66 行）

- UEFI 路径按 `ExitBootServices` 分界：EBS 之前用固件控制台（`uefi::println!` 经 ConOut），EBS 之后用裸串口（`raw_serial`/`raw_serial_line`）。U-Boot shim 与两个直核面没有 EBS 这一步，只有裸串口。
- `raw_serial` 与 `raw_serial_line` 是同一条裸串口通道的两种写法：后者写前等发送侧腾空（x86_64 读 LSR bit5，有界轮询）并在 `\n` 前补 `\r`；前者不等、不补。aarch64 上两者共用带节流的发射器，差别只剩 `\r`。
- 里程碑清单每条给输出、阶段、适用腿与源码位置；这些字符串是排查时的定位依据。清单没有独立 step3 路标——开分页夹在 `step1+2` 与 `step4` 之间，源码未单独打点。
- 关键发送侧路标：`prepare_and_enter enter (handoff v1 arena)`、`kernel loaded (entry staged)`、`12 boot modules loaded`、`param_buf bytes=…`、`[raw] boot services exited (paired)`、`memmap source: …`、`[raw] arena=…`、`[raw] entering kernel image v1`、`kernel: step1+2 mappings ok`。
- 关键接收侧路标：`### minix-rs kernel-image: resuming high-half boot (handoff v1 arena)`、`kernel: v0`–`v4`、aarch64 接收腿的 `kernel-image: step4 DM coverage (additive on active root)`。`v0`–`v4` 是 `boot_validate_and_prepare` 内部子步；`step0 validate ok` 是同一阶段在直核面的完成标记，不是另一阶段。
- `step1+2` 由 `build_bootstrap_root_and_enable` 打印：三条 v1 发送腿在 shim 进程内打，两个直核面在内核内也打同一行；它不是“已经在独立镜像里”的信号。要判断接收端是否已起，看它之后有没有 `###` 或第二组 `v0`。
- 删掉 `[raw] entering kernel image v1` 后，串口停点按三档读：`[raw] arena=…` 还没进调用；`kernel: v4` 已进调用但 Step 1/2 未完成；`kernel: step1+2` 已过 Step 1/2、跳转前。
- 已知诊断边界：第二档与第三档之间没有专用发送侧路标，CR3 切换后 shim 侧不可观测；A→C 间隙由 QEMU `-d int` 异常追踪覆盖，不为此增加打印。

## panic 路径（§6.2，第 67–84 行）

- `#[panic_handler]` 用 256 B 固定栈缓冲 `StackWriter`，不用 `format!`：EBS 之后堆不可用，UEFI 池分配器返回空。
- 超出 256 B 丢弃尾部而不是截断格式化过程；注释写明“诊断信息截断好于静默”。
- 输出只走 `raw_serial_line`（带 `\r` 与发送侧等待），不碰可能已失效的固件通道。
- 最后尝试诊断钩子：钩子静态量随 `minix-types` 编进每个二进制各自的一份，shim 侧的钩子恒未注册，兜底信息是无条件行为；内核侧直核家族在 `arch_boot` 入口注册，v1 接收侧在 `resume_additive` 的 `store_kernel_info` 之后注册。钩子未注册在直核路径才是“panic 早于注册”的真实信号。
- 在 panic handler 里调 `format!` 或固件服务会触发第二次故障，原始 panic 的消息与位置被覆盖；“不能分配、不能调固件服务、只能纯 CPU 加裸串口”这条约束由此而来。

## 典型失败模式与信号（§6.3，第 85–108 行）

- 前两行是 C 原作失败模式：`load_mods` 写在 `multiboot` 之后停在 `assert(mi_mods_count > 0)`；`head.S` 两条 `push` 对调停在 `assert(magic == MULTIBOOT_INFO_MAGIC)`。重写产品没有这两个触发点，只作第 2 章 C 对照。
- 取件失败：模块缺失报 `boot-shim: required boot module 'X' not found at <路径>`；内核 ELF 读不到报 `required file not found: /EFI/minix/kernel.elf`。
- 分配失败：bump 区间耗尽时 `BootAlloc::alloc` 返回 `AllocationFailed`，现场是 `map_huge`/`boot_pt_alloc` 失败或 `bump region not registered`；复算 `prepare_and_enter_kernel_image(N)` 预算。
- v1 解码失败：`handoff v1 envelope invalid: <变体>`，或 memmap/reserved/modules/platform/param/source 六个分区中任一段的对应行；先看 `[raw] arena=… conv=… res=…` 是否已打印。
- 发送侧走完、接收端未起：`[raw] entering kernel image v1` 之后没有 `###`/第二组 `v0`；停点按三档读（§6.1 末段），在 `enter_kernel_image_v1` 或接收侧委托链下断点。
- boot-shim 自身镜像被装载在 4 GiB 之上：Step 1 窗口断言 `end <= BOOT_IDENTITY_MAP_END`（2 MiB 上取整后）fail-closed，报可读消息。
- 自举页被报成可用：`assert_bootstrap_outside_memmap` 报 `conventional memmap [a,b) overlaps bootstrap allocation [c,d)`；核对快照点在 root/bump/arena 三次分配之后。
- 大页阶梯无档可用：`arch_boot: page-size policy: …` 或 `enter_kernel_image_v1: page-size policy: …`，变体 `NoKernelPageSize`/`FallbackUnsupported`；跑 `check-layout.sh` 的 L3/L4/L5。
- 页表切换后取指故障：QEMU `-d int` 的异常记录给 `CR2`/`EPC`；核对恒等映射范围。
- 固件不设 `WRITE_BACK`：内核跨度落 `Side`，strict 门报 `declared RAM authority … has no source records` 或跨度不在两表；看 `conv/res` 与 WB 位。
- `KernelInfo` 切片读回全零：VM 侧 “no free memory regions” 类 panic；核对 `store_kernel_info` 是否落了六块（第 4 章落地区）。

## 可复制命令（§6.4，第 109–162 行）

- 第一层串口捕获：`test-shim-bootmarks-x86.sh`、`test-shim-bootmarks-aarch64.sh`、`test-riscv64-uboot.sh`（直载测试链）、`test-riscv64-uboot-shim.sh`（6/6）、`test-kernel-image-riscv64-bootface.sh`；aarch64 直核面不在五条门内，由 `test-smp-aps-aarch64.sh` 间接驱动，判据是 SMP 交付。每条关键行见第 5 章门链表。
- 第二层异常与页表：`qemu ... -d int,mmu,cpu_reset -D qemu.log`；`int` 给异常类型与 `epc`，`mmu` 给页表遍历，`mmu` 日志量大，只用于定位单次遍历。
- 第三层 GDB：`qemu ... -s -S` + `gdb-multiarch image.elf`，先 `target remote :1234`；断点用 crate 全名（`kernel::rust_image_main`、`minix_kernel::arch_boot`、`minix_kernel::arch_boot_resume_high_half`），裸短名匹配不到 mangled 符号。
- 只用 GDB 不开 `-d int` 不够：页表切换后的故障是“取指即失败”，GDB 停下时故障已发生，只有异常记录给 `epc`/`CR2`。
- `DTB_CFGTABLE=1` 强制走 DTB 配置表分支；`SKIP` 只表示宿主 U-Boot 不兼容，不是链失败。

## 边界与下游用法

- 本章按症状查，不按执行顺序读；先确定腿与 EBS 前后，再选通道与路标。
- 不证明：没有专用发送侧路标覆盖 CR3 切换后的间隙；门链全过是实证背书，不是对任意畸形输入的证明。
- 失败定位以第 5 章的证据矩阵为准；诊断边界（不增打印的约定）见第 5 章已知边界。
