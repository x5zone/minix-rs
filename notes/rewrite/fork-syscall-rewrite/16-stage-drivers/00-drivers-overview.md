# 00-drivers-overview — 驱动子系统总览

> **状态**: 已展开（2026-09-17 全量扫描后定稿；`.design/` 快照随本版生成）
> **前置依赖**: 无
> **本篇不覆盖**: 一切机制细节（01~24 各篇）。
> **参见**: `plan.md`（覆盖契约）、`99-global-concepts.md`（请求常量全集）

## 1. 概念：五十七个单线程进程撑起的硬件边界

Minix3 把所有硬件都赶到用户态进程后面：五十七个驱动 server，每个是一个单线程事件循环，加上十一个它们共用的框架库（合计二百九十个 C 文件、十五万五千行）。驱动进程的核心姿势与 Minix3 的微内核哲学一致：**不碰内核数据、不发特权指令，一切硬件访问经由内核的授权机制（grant 拷贝、IRQ 挂钩、端口授权）向内核申请**。这五十七个进程彼此独立，一个崩溃只死一个，重则由再孵化服务器拉起来。

## 2. C 源码分析：启动顺序决定学习顺序

驱动没有统一的启动入口，但有一条硬因果链（`kernel/table.c:44-64`）：

```
boot image 只带两个驱动：
  ├─ memory（MEM_PROC_NR，table.c:58）← 05：/dev/ram* 与 /dev/imgrd 是根文件系统的地基本
  └─ tty（TTY_PROC_NR，table.c:59）   ← 06：控制台是系统说第一句话的嘴
init 起，/etc/rc 放开其余驱动（RS 运行时加载）：
  ├─ log（诊断缓冲）     ← 08
  ├─ readclock（RTC）    ← 10
  ├─ pci（总线枚举，设备驱动加载的前置）← 11
  ▼ 之后一切设备按需出现：
  virtio_blk → ahci/at_wini → 其余存储（14~17）
  pckbd（输入事件桥）      ← 13
  网卡 → 协议栈（22/23 → 17-stage-net）
  usb → audio → fb → 杂项（18~21、24）
```

正因为只有 memory 和 tty 在 boot image 里，这两条线是整个子系统的最小可启动闭环；其余驱动都可以在它们站稳之后按任意次序插入。

## 3. Rust 设计决策

### 3.1 策略在库、传输在 bin

每个驱动 crate 拆成两半：纯策略库（状态机、请求解析、错误映射，测试用注入的传输替身）与服务二进制（真实的端口读写与内核消息）。这一刀的理由是可测性：C 驱动直调内核原语，离开内核一行测试都跑不了；Rust 策略库的几百个测试全部跑在宿主机上。

### 3.2 公共运行时统一循环壳

五十七个 bin 共享同一个事件循环骨架：宣告（数据Store发布标签）、收包、分类、分发。骨架由 `minix-driver-rt` 提供——传输行为注入、SEF 生命周期切点内嵌于传输实现（`[ARCH: 驱动服务运行时统一]`）。输入服务器自写的循环是这一设计的第一个实测样板。

### 3.3 框架库按协议家族分立

字符、块、网络、音频、I2C 五族各有框架库（`minix-chardriver`/`minix-blockdriver`/`minix-netdriver`/`minix-audiodriver`/`minix-i2cdriver`），每族一套钩子表：C 的函数指针表在 Rust 里变成带默认体的行为定义，缺省钩子的行为与 C 的空指针逐一对应。块的调用方一侧另有 `minix-bdev` 客户端库（04）。共享的服务器状态机（打开设备表、循环动作、收包结果策略）在 `minix-driver-rt::core` 单点实现。

### 3.4 线格式归策略库

字节级契约——virtio 环布局、CBW/CSW/CDB、USB 消息槽位——以 `#[repr(C)]` 类型与布局常量落在本 stage 的库内（`[ARCH: policy/transport 边界重划]`）；物理内存的连续分配是唯一的外部依赖（edge E-DMABUF，VM 侧契约）。

## 4. 错误处理

子系统的错误公理只有一条：驱动对外的每个错误都映射 Minix3 的 errno 值（ENXIO/EINVAL/EIO/EAGAIN/EBUSY/EPERM/ENOTTY），逐场景的映射表在各篇错误处理节。

## 5. 测试

六框架库与三十一策略 crate 合计四百余个测试全部宿主机可跑；每篇文档第 5 节给出各自的对账（测试名、数量、对应 C 行为）。集成面（真实中断到事件循环）属多进程联调，随通电批次验证。

## 6. 过渡

下一章进入框架：01 讲字符驱动如何用一张十钩子的表服务七种请求——它是 tty、pty、log、random、readclock、audio 的共同骨架。

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/plan.md`：覆盖契约与架构清单。
- `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/99-global-concepts.md`：请求常量全集。
- `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/01-chardriver-framework.md`：框架第一篇。
