# 第 5 章摘要：测试：四条不变量各自的证据

## 本章回答什么

四条不变量都能被“测试通过”背书，但不同测试证明的东西不同：宿主测试证明形状与负控，真机门证明某条链能走到终点，二者都不证明任意畸形输入会被拒绝。本章把 I1–I4 拆成证据矩阵：每条不变量有哪些宿主证据、哪些真机证据、各自不证明什么，以及一条失败链该用哪个路标定位。

## 证据矩阵（§5.1，第 16–30 行）

- I1（值可跨镜像）：宿主证据是 `handoff_v1` 的 encode/decode round-trip、`KernelInfo` 字段台账、`store_kernel_info` 落地区测试；真机证据是 `test-shim-bootmarks-x86.sh`（x86 v1 交接）与 `test-riscv64-uboot-shim.sh` 6/6。不证明任意畸形 wire 都被拒绝，负控只覆盖已知变体。
- I2（来源可追溯）：宿主证据是 `SourceRecord`/`audit_source_coverage` 正负控、DTB 收集器 40 槽上限；真机证据是 riscv64/aarch64 直核的 DTB memmap 启动。UEFI 腿的 source 由分类器直写 wire，宿主只测分类器。
- I3（所有权明确）：宿主证据是 `BootStorage` 容量/半提交/one-shot 测试、`BootReport` 生命周期编译检查；真机证据覆盖五条腿，但有覆盖深度差异（见下）。不证明所有调用点都遵守 one-shot。
- I4（校验先于消费）：宿主证据是 `KernelInfo::validate_*` 的 `should_panic` 负控与布局/跨度测试；真机证据是 QEMU 各腿启动门。不证明所有畸形布局，只覆盖接收侧已构造的负例。
- 证据强度分三档：`[结构保证]` 由类型/常量/编译期断言直接保证；`[观测]` 是单片真机或探针读数，不构成对所有环境的保证；`[预防性不变量]` 是为防已知风险而设、机制尚未证实的约束。表内未标档的按 `[观测]` 读。

## 宿主测试边界（§5.2，第 31–34 行）

- `cargo test -p minix-boot --lib` 189/0；`-p boot-shim --lib` 136/0；`-p minix-kernel --features mock --lib` 866/0/8；`-p minix-elf --lib` 27/0。
- 真机门：`test-shim-bootmarks-x86.sh` 与 `test-riscv64-uboot-shim.sh` 是发送/接收全链；`test-kernel-image-riscv64-bootface` 是直核；aarch64 直核由 `test-smp-aps-aarch64` 到 boot/SMP 标记。
- 宿主测试证明形状与负控，不证明真机链能走通；真机门证明某条链能到终点，不证明任意畸形输入被拒绝。

## 机制到证据（§5.3，第 35–48 行）

- 深拷不变量（I1/I3）：接收侧消费前，载荷已在私有存储；证据同 §5.1 的 I1/I3 行。
- 来源覆盖（I2）：每个投影字节有来源记录；证据同 §5.1 的 I2 行。
- 校验先于消费（I4）：建表/分配之前完成校验；证据同 §5.1 的 I4 行，顺序由第 4 章的门链表与代码走查保证。
- 零化读数：候选①“页表不重放”与候选②“被当空闲复用清零”都未排除，因此“入口尽早深拷”按 `[预防性不变量]` 读，不是已证机制。

## 真机门链（§5.4，第 49–62 行）

- x86_64 UEFI：`test-shim-bootmarks-x86.sh`；关键路标 `kernel loaded (entry staged)`、`12 boot modules loaded`、`[raw] entering kernel image v1`、`resuming high-half boot`、`entering scheduler`。
- aarch64 UEFI：`test-shim-bootmarks-aarch64.sh`；前两条固定标记 + 第三条变体（默认看 `memmap source`，`DTB_CFGTABLE=1` 看 `DTB preflight failed`）。
- riscv64 U-Boot shim：`test-riscv64-uboot-shim.sh` 的 6/6 路标（`uboot leg enter`、`table built`、`kernel + 12 modules loaded`、`[raw] entering kernel image v1`、`resuming high-half … riscv64`、`entering scheduler`）。
- riscv64 直核：`test-kernel-image-riscv64-bootface.sh` 只验证表址与模块定位失败的错误路径，不证明走到 `kmain`。
- riscv64 直载测试链（不是生产腿）：`test-riscv64-uboot.sh`，判据 `### TEST_RESULT: PASS hello-boot-riscv64 ###`；宿主 U-Boot 不兼容时 SKIP（exit 2），不是链失败。
- aarch64 直核不在五条门链内：由 `test-smp-aps-aarch64.sh` 用 `-kernel` 驱动，判据是 SMP 交付（`ap-arrived`/`ap-irq-ready`），不是启动路标门。

## 缺口与关闭记录（§5.5，第 63–72 行）

- 范围内没有已登记但未关闭的测试缺口；覆盖度限制在 §5.1 的“不证明什么”列明。
- aarch64 直核的用户链 Data Abort 是已登记的 A2 遗留，不是测试缺口，单列不计入证据矩阵。
- 三条由现有证据覆盖的旁证：C 段归属由 `check-layout.sh` 的 L10 覆盖；riscv64 载体的 PC 高半判据在 `kmain_verify` 的 PASS 条件内；生产 `kmain` 入口的 ABI 栈形由三份 `jump_to_kmain` 的静态断言覆盖。
- 第 6 章的“step1+2 之后没有发送侧路标”是已知诊断边界，冻结裁决明确不为此增加打印，由 QEMU `-d int` 覆盖。
- 第 6 章的窗口断言关闭记录指 `BOOT_IDENTITY_MAP_END` 的 Step 1 断言。

## 下游能拿走什么

- 每条不变量的宿主证据、真机证据、覆盖深度与“不证明什么”；引用证据时按这张矩阵，不把某条腿的门当成全链证明。
- 失败链定位用 §5.4 的关键路标；aarch64 直核只到 SMP 交付，不能拿它证明 `kmain`。

- 四条不变量各有自己的证据层，不能互相借用；门覆盖不等于成功路径覆盖。
- I3 的真机证据按覆盖深度分三条：四条腿有启动门链；aarch64 UEFI 只到发送侧分类点；aarch64 直核只到 SMP 交付。
- riscv64 直核门只验证 fail-closed 错误路径，不证明走到 `kmain`；把它当成功链证据是误读。
- 五条生产腿、五条真机门链与“四条有门链的腿”成员不同，换算见第 3 章的计数表。
- 本章不给新测试，只给证据归属与边界；缺口按“不证明什么”列读。
