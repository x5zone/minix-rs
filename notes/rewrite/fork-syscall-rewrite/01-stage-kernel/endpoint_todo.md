# endpoint_t 演进 TODO（设计讨论输入文档）

> **本文档用途**：本文件是设计讨论的**自包含输入**，给没有代码访问权限的 GPT / 其他 AI 阅读。所有背景、前因、候选方案、关键证据、待决问题都写进正文；引用源码时只标注"逻辑位置"和"约束事实"，不假定读者能跑 grep。
>
> **目标读者**：GPT 网页端（无代码访问）+ Claude Code / Trae Code（有代码访问）；本文档应让**任何**读者都能基于正文给出可执行建议。
>
> **当前状态**：**草稿 / 待讨论**。三种候选方案都未实施；用户倾向"调整 endpoint 协议"或"减 generation 位宽"或"扩成 64 位"，尚未敲定走哪条路。
>
> **与本文档强相关的上游文档**：本 stage 内 `06-proc-init-boot-proc.md` §2.0 正在讨论"256 进程上限 / 全系统上限"问题；本文件是那条讨论线的延伸。

---

## 目录

1. [背景：项目是什么](#1-背景项目是什么)
2. [核心问题陈述：3万限制从哪来](#2-核心问题陈述3万限制从哪来)
3. [endpoint_t 在 Minix3 IPC 协议里的角色](#3-endpoint_t-在-minix3-ipc-协议里的角色)
4. [扩展容量的现实场景 vs 不需要扩展的场景](#4-扩展容量的现实场景-vs-不需要扩展的场景)
5. [候选方案 A：减少 generation 位宽（最小改动）](#5-候选方案-a减少-generation-位宽最小改动)
6. [候选方案 B：endpoint 扩成 64 位（推倒重做）](#6-候选方案-bendpoint-扩成-64-位推倒重做)
7. [候选方案 C：endpoint 协议与 slot 容量解耦（彻底重构）](#7-候选方案-cendpoint-协议与-slot-容量解耦彻底重构)
8. [三个候选方案的总评对比](#8-三个候选方案的总评对比)
9. [用户已表达的设计倾向](#9-用户已表达的设计倾向)
10. [关键证据汇总（无可信源标记的描述）](#10-关键证据汇总无可信源标记的描述)
11. [与本文档强相关的现有文件清单](#11-与本文档强相关的现有文件清单)
12. [待决问题清单（与 GPT 讨论的核心输入）](#12-待决问题清单与-gpt-讨论的核心输入)

---

## 1. 背景：项目是什么

**Minix-RS**：把 Minix 3（教学型微内核操作系统）的 C 源码**语义等价重写**成 Rust 的项目。

- **上游 C 源码（ground truth）**：Minix 3，在 `minix3/minix/` 目录，**只读不改**
- **Rust 实现**：在 `os/` 目录（kernel / arch / 各 user-space server）
- **重写原则**：不是 1:1 翻译，是**用 Rust 类型系统重新表达相同外部行为**——IPC 协议、生命周期、权限模型必须与 Minix3 一致，但内部数据结构可以重组

**与本文档强相关的两条项目约束**：

1. **`#![no_std]` + boot 期无堆**：kernel 启动阶段堆分配器尚未建立，所以 `proc[]`、`priv[]` 等核心表必须用**编译期定容静态数组**（不能用 `Vec` / `Box`）
2. **CLAUDE.md 三级术语**：
   - **Rewrite**（重写）：保持外部行为
   - **Refactor**（重构）：不改语义的重构
   - **Architectural Evolution**（架构演进）：结构性变化，需 doc + design + code 三处一致标注 `[ARCH: ...]`

**关键事实**：当前所有 user-space server 和 kernel 都在使用 Minix3 的 IPC 协议（消息 + endpoint_t 标识收/发方）。**任何 endpoint_t 改动都是 IPC ABI 变更**——所有 server 必须重编译。

---

## 2. 核心问题陈述：3万限制从哪来

### 2.1 完整推导链

```
endpoint_t = 32 位 int（C 端 typedef）
    ↓
endpoint_t 编码方案: (generation << SHIFT) | slot
    ↓
约束: generation + slot 位宽总和 ≤ 31 位（留 1 位给符号）
    ↓
当前选择: SHIFT = 15，generation = 15 位，slot = 15 位
    ↓
最大进程数 = (2^15) - 1023(kernel task) - 3(ANY/NONE/SELF) = 31 742 ≈ 3万
    ↓
当前 Minix3 默认值: NR_PROCS = 256（远低于 31 742 上限）
```

### 2.2 数字含义拆解

- **31 742** = `endpoint_t` 编码方案的**数学硬上限**
- **256** = Minix3 形态的**工程默认值**（远低于上限）
- **3万** = "扩到 31 742 后达到的真实上限"

### 2.3 三层关系

| 限制 | 来源 | 关系 |
|------|------|------|
| **256** | Minix3 主动选择 | 是 boot 期零堆 + 教学形态的结果，**可自由调大到 31 742** |
| **31 742** | endpoint_t 编码方案 | 是 **endpoint_t 位分配的数学结果**，改需要破 IPC ABI |
| **> 31 742** | endpoint_t 编码方案 | **完全不可能**除非改 endpoint 协议本身 |

### 2.4 为什么 endpoint_t 位分配是 IPC ABI

endpoint_t 出现在：

- **所有 IPC 消息** 的源/目的字段（kernel IPC path）
- **共享内存** 里描述进程身份的字段
- **用户态 server 的进程表索引**（PM/VM/VFS 各自的表）
- **系统调用参数**（如 `m_source`、`m_type` 附近的 endpoint）

也就是说——endpoint_t 不是 kernel 内部类型，**它是 IPC 协议字段**。改它意味着所有 server 重编译、IPC 消息布局重排、共享内存协议重定。

---

## 3. endpoint_t 在 Minix3 IPC 协议里的角色

### 3.1 endpoint_t 的两个分量

| 分量 | 位数 | 实际用途 |
|------|------|---------|
| **slot** | 15 位 | 表达"进程身份"（在哪个数组槽位） |
| **generation** | 15 位 | **防陈旧引用**——slot 复用时识别"是否仍指向同一个进程" |

### 3.2 generation 字段的语义

**generation 存在的唯一原因**：当一个 slot 被新进程复用时（fork 出旧进程 → 退出 → 新进程占同一个 slot），旧的 endpoint_t 仍可能在用户的内存里。**generation +1 让旧引用失效**。

例如：

```
时刻 0: 用户持有 endpoint_t = (gen=5, slot=42) → 指向进程 A
时刻 1: 进程 A 退出，slot 42 被回收
时刻 2: 新进程 B 占 slot 42，gen=6
时刻 3: 用户用 (gen=5, slot=42) → 内核发现 gen 不匹配（5 ≠ 6）→ 视为无效 endpoint
```

**没有 generation 字段**：陈旧引用会指向新进程，造成严重安全问题。

### 3.3 generation 字段在 IPC 路径中如何被使用

- **每次 IPC**：内核检查 endpoint_t 的 generation 与 `proc[slot].p_endpoint` 是否一致
- **每次 fork**：子进程继承父进程的 generation 编码
- **每次进程退出 + slot 复用**：`proc[slot].p_endpoint` 的 generation 加 1
- **用户态 server**：通常**不直接处理** generation——只用 `_ENDPOINT_P(e)` 取 slot 部分；但 server 自己维护的进程表也带 generation 用于一致性检查

### 3.4 generation 位宽的实际需求

- 物理上，一个 slot 在机器寿命内能复用 32k（=2^15）次是**不可思议的**——每秒复用一次也要连续跑 9 小时
- 实际合理估算：每 slot 复用 < 100 万次（机器寿命 / 进程平均生命周期）
- **5 位（32 次复用）就够用**——15 位是**过度设计**
- 32k → 32 次的差距 = 8 倍 reduction，可让出 10 位给 slot

### 3.5 endpoint_t 不是 Minix3 的内在约束

`endpoint.h` 第 39-43 行注释明确说：

> "The following constant defines the split between the two parts of the endpoint numbers. It can be adjusted to allow for either more processes or more per-process generation numbers. Changing it will change the endpoint number layout, and thus break binary compatibility with existing processes."

也就是说——SHIFT = 15 是 Minix3 的**工程经验值**，不是不可改的硬约束。`endpoint.h` 第 60-62 行的 `#error` 也明确说"想扩就自己调 SHIFT"。

---

## 4. 扩展容量的现实场景 vs 不需要扩展的场景

### 4.1 不需要扩展的场景（Minix3 形态）

| 场景 | 用户数 | 每用户活跃进程 | 总进程 | 3万够吗 |
|------|--------|--------------|--------|---------|
| 单用户工作站 | 1 | 100-300 | 100-300 | ✓ 极充裕 |
| 教学/研究 OS | 1-5 | 10-50 | 50-250 | ✓ 极充裕 |
| 小型服务器 | 5-30 | 10-30 | 50-900 | ✓ 充裕 |
| 中型 web（NGINX+FPM） | 1 + 服务 | 100s | 1000-5000 | ✓ 充裕 |
| 重度 web（NGINX 1000 worker） | 1 + 服务 | 数千 | 5000-30000 | △ 临界 |

### 4.2 需要扩展的场景（云 OS / 64 位原生场景）

| 场景 | 进程数 |
|------|--------|
| 容器化云（100 pod × 10 容器 × 10 进程） | 10000 |
| Linux 单机（pid_max 默认 4M） | 4 194 304 |
| fork bomb 防御 | 与 pid_max 同 |

### 4.3 Minix3 形态 vs 云 OS 形态

| 维度 | Minix3 形态 | 云 OS 形态 |
|------|------------|------------|
| **典型场景** | 教学、研究、单用户/几十用户工作站 | 服务器、数据中心、容器化 |
| **用户数** | 1-30 | 数千-数万 |
| **进程身份上限** | 256 - 31742 | 4M（Linux） |
| **真正需要 3万+ 的场景** | **几乎没有** | **常规需求** |

### 4.4 现实判断

**Minix3 形态下，3万（31 742）实际够用**——`uid_t` 与 `NR_PROCS` 正交（uid 是 PM 语义，不是 kernel 语义），所以多用户不消耗进程表；物理内存（每进程栈 64 KB × 31742 = 2 GB）才是真正的硬上限，先于进程表触顶。

**Minix3 形态下不需要扩**——但用户希望**为未来扩展（云 OS / 64 位原生场景）保留可能性**。

---

## 5. 候选方案 A：减少 generation 位宽（最小改动）

### 5.1 思路

**只动 `endpoint.h` 的 SHIFT 常量**——把 SHIFT 从 15 改成更小的数（如 5 或 8），让 generation 占用更少位，slot 占用更多位。

### 5.2 SHIFT 调整矩阵

| SHIFT | generation | slot 位数 | MAX_NR_PROCS | generation 实际含义 |
|-------|-----------|-----------|--------------|--------------------|
| **15**（当前） | 32 768 次复用 | 15 位 | **31 742** | slot 复用上限 32 768 次 |
| 12 | 4 096 次复用 | 18 位 | 261 117 | slot 复用上限 4 096 次 |
| 10 | 1 024 次复用 | 20 位 | 1 044 485 | slot 复用上限 1 024 次 |
| 8 | 256 次复用 | 22 位 | 4 177 413 | slot 复用上限 256 次 |
| 5 | 32 次复用 | 25 位 | 33 554 171 | slot 复用上限 32 次 |
| 0 | 1 次（不用 generation） | 30 位 | 1 073 741 569（10亿） | 不复用（进程死了 slot 不能用） |

### 5.3 推荐的选择

**SHIFT = 8 或 10** 是甜点：

- **SHIFT = 8（256 次复用）**：slot 数 4M，符合 Linux pid_max；足够任何现代 OS
- **SHIFT = 10（1024 次复用）**：slot 数 100万，更保守
- **SHIFT < 5**：不可取——每 slot 复用 < 32 次实际不够用（系统短时间重启就会超过）

### 5.4 改动量

| 文件 | 改动 |
|------|------|
| `endpoint.h` | 改 SHIFT = 8 或 10（1 行） |
| **所有 IPC 路径** | 字段宽度变了，**结构体对齐可能改变**，需要重新 `_ASSERT_MSG_SIZE` |
| **所有 server** | 重编译（IPC 路径不变，只是 endpoint_t 解析细节变） |
| **Message 结构体** | 大概率需要重新排版（位宽变化） |
| **共享内存协议** | 涉及 generation 检查的代码要重新审 |
| **测试** | 全 IPC / fork / exec 路径重新测 |

**估计工作量**：1-2 周（不算测试）。测试本身 1-2 周。

### 5.5 风险

- **generation = 256 次复用**——理论够，但需要论证"进程生命周期 > slot 平均复用周期"
- **破坏所有 binary**——所有 user-space server 必须重编译
- **与所有现有 endpoint_t 引用不兼容**——kernel + 所有 server 必须同步升级

### 5.6 适配度评估

| 维度 | 评分 |
|------|------|
| **改动量** | 小（1 行常量 + 全 IPC 路径重测） |
| **IPC ABI 破坏** | 中（所有 binary 重编译，但协议语义不变） |
| **slot 容量** | 巨大（4M+） |
| **未来再扩可能** | 高（仍受 32 位 int 限制） |

---

## 6. 候选方案 B：endpoint 扩成 64 位（推倒重做）

### 6.1 思路

**把 `endpoint_t` 从 `int32_t` 扩成 `int64_t`**——用更多位宽换取容量。SHIFT 可以更大（甚至单独留 32 位给 slot）。

### 6.2 位分配矩阵

| 方案 | 端点总位 | generation | slot | MAX_NR_PROCS | 备注 |
|------|---------|-----------|------|--------------|------|
| 64 位 + SHIFT=16 | 63 | 16（65536） | 47 | 140 万亿 | 极端容量 |
| 64 位 + SHIFT=20 | 63 | 20（100万） | 43 | 8 万亿 | Linux 容器场景 |
| 64 位 + SHIFT=32 | 63 | 32（40亿） | 31 | 21 亿 | 单机容量到 Linux 量级 |
| 64 位 + SHIFT=32 + 留 slot=48 | 63 | 16 | 48 | 281 万亿 | 50年内都用不完 |

### 6.3 改动量

| 文件 | 改动 |
|------|------|
| `endpoint.h` | `typedef int64_t endpoint_t;`（1 行） |
| **所有 IPC 消息** | `Message` 结构体里所有 endpoint_t 字段位宽 × 2——**结构体大小变化** |
| **共享内存** | endpoint_t 大小变了，所有共享结构重排 |
| **`kinfo` 结构** | 包含 endpoint 的字段需要重排 |
| **`_ASSERT_MSG_SIZE` 检查** | 全部需要重新跑 |
| **所有 server 代码** | **强类型转换可能编译失败**——`endpoint_t` 与 `int` 的隐式转换不再成立 |
| **测试** | 全部重测 |

**估计工作量**：1-2 月。**工作量是方案 A 的 5-10 倍**。

### 6.4 风险

- **彻底破坏 IPC ABI**——所有 binary 重编译
- **Message 结构体大小变化**——所有 IPC 调用点需要重新审视
- **共享内存协议重定**——涉及 boot info、kinfo 等结构
- **user-space C 代码无法直接 cast**——`int ep = ...; ep = endpoint_t;` 这种用法必须改
- **minix-rs Rust 端**的 `Endpoint` newtype 调整

### 6.5 适配度评估

| 维度 | 评分 |
|------|------|
| **改动量** | **大**（1-2 月） |
| **IPC ABI 破坏** | **大**（所有 Message 结构体重排） |
| **slot 容量** | **极大**（数十亿） |
| **未来再扩可能** | 100 年不需再扩 |

---

## 7. 候选方案 C：endpoint 协议与 slot 容量解耦（彻底重构）

### 7.1 思路

**endpoint_t 保持 32 位不变**——保留 generation 防陈旧引用。**slot 容量由独立机制承载**：

- 进程表 slot 仍用 32 位 int 索引
- 进程表**用稀疏数据结构**（HashMap / Radix Tree / 多级表）而不是固定数组
- endpoint 中的"slot"字段改成"slot 索引"或"全局唯一 ID"
- 物理内存按需分配（lazy commit，参考 Linux vmemmap / seL4 untyped）

### 7.2 实现选项

#### 7.2.a：保留 endpoint 32 位，slot 用稀疏 ID

```
endpoint_t = (generation << 16) | slot_id(16位)
```

slot_id 不再是数组下标，而是**全局唯一 ID**——通过 HashMap/Radix Tree 映射到物理 slot。

**优势**：slot 容量由 HashMap 容量决定，可达数百万
**劣势**：endpoint 解析需要查表（多了一次间接寻址）

#### 7.2.b：保留 endpoint 32 位，slot 用两级寻址

```
endpoint_t = (generation << 16) | slot_in_group(8) | group(8)
```

slot 寻址：先按 group 索引到大数组，再按 slot_in_group 索引到最终结构。

**优势**：保留数组下标的 O(1) 寻址性能
**劣势**：group 和 slot_in_group 的位宽分配仍受 endpoint 约束

#### 7.2.c：完全解耦——endpoint 仅含 generation，slot 独立

```
endpoint_t = generation(32)
slot_id = 独立的 slot identifier（不放在 endpoint_t 中）
```

**优势**：完全解耦，endpoint 协议与 slot 容量无关
**劣势**：**所有 IPC 路径都要传 slot_id**——更大的 Message 结构

### 7.3 改动量

| 文件 | 改动 |
|------|------|
| **整个 IPC 路径** | 重新设计 |
| **所有 proc[] 数据结构** | 改成稀疏（HashMap / 多级表） |
| **endpoint_t 含义** | 重新定义 |
| **所有 server** | 全面重写 |

**估计工作量**：**6-12 月**——这是个完整的 ARCH 演进，不是局部调整。

### 7.4 风险

- **彻底重新设计**——所有 IPC 调用方都要改
- **性能影响**：endpoint 解析多了一次查表（除非用预计算缓存）
- **boot 期复杂性**——boot 时没有 HashMap 容器，怎么初始化？
- **测试覆盖**——IPC 路径全面重测

### 7.5 适配度评估

| 维度 | 评分 |
|------|------|
| **改动量** | **极大**（6-12 月，ARCH 演进） |
| **IPC ABI 破坏** | 彻底重设计 |
| **slot 容量** | 无限（受物理内存约束） |
| **未来再扩可能** | 永远不需再扩 |
| **minix-rs Rewrite 范畴** | **不在**——超出 Rewrite，进入独立 ARCH 项目 |

---

## 8. 三个候选方案的总评对比

| 维度 | A：减 SHIFT | B：扩 64 位 | C：解耦 |
|------|------------|------------|---------|
| **改动量** | 1-2 周 | 1-2 月 | 6-12 月 |
| **IPC ABI 破坏** | 字段重排 | 字段 ×2 + 重排 | 全面重设计 |
| **slot 容量最终上限** | 4M+（Linux 量级） | 数十亿 | 物理内存 |
| **是否触及核心 IPC 协议语义** | 端点位宽 | 端点类型 | IPC 整个 |
| **minix-rs Rewrite 范畴** | **是**（IPC 路径细节调整） | **临界**（IPC ABI 变化） | **不在**（ARCH 演进） |
| **用户态 binary 重编译** | 必须 | 必须 | 必须 + IPC 协议整体调整 |
| **适合作为 minix-rs 的下一阶段吗** | **✓ 强烈推荐** | △ 视目标而定 | ✗ 应该是独立项目 |

### 推荐路径

**如果用户的真实目标是"扩展 minix-rs 的进程容量"**：

- **首选方案 A**：SHIFT = 8 或 10，给 slot 4M+，几乎所有 IPC 路径不变，只改 endpoint.h 的常量
- **次选方案 B**：如果坚持 64 位原生场景，1-2 月投入换 50 年的容量冗余

**如果用户的真实目标是"为独立 ARCH 项目铺路"**：

- 方案 C 是唯一合适的方向
- 但应该**独立项目**（如 minix-rs-evolved），不与 minix-rs Rewrite 混在一起

---

## 9. 用户已表达的设计倾向

用户（minix-rs 项目维护者）在 2026-08-30 的讨论中表达了以下倾向：

1. **"我有点想调整 endpoint 的协议了"**——倾向于**改 endpoint 编码本身**
2. **"或者减少一点 generation 位宽"**——明确提出**方案 A 类型**的修改
3. **"或者 endpoint 直接变成 64 位"**——明确提出**方案 B 类型**的修改
4. **不倾向方案 C**——目前没有提过"endpoint 协议与 slot 容量解耦"或"彻底重构 IPC"

用户的核心诉求是：**保留 Minix3 IPC 协议语义，但扩容量**——即"在 IPC 路径不变的前提下，让 slot 容量扩展到合理水平"。

**从用户倾向看，方案 A 最符合**——它"减少 generation 位宽"恰好对应"保留 IPC 语义，仅调位分配"。

---

## 10. 关键证据汇总（无可信源标记的描述）

为防止本文档因"AI 没有 grep 验证"而出错，这里列出**所有事实性断言**，并标注**可信源**与**风险**：

### 10.1 高可信度断言（已经过实测/查证）

| 断言 | 来源 | 风险 |
|------|------|------|
| endpoint_t 在 Minix3 是 `int32_t` | `endpoint.h` typedef 链 | 低（已查证） |
| `_ENDPOINT_GENERATION_SHIFT = 15` | `endpoint.h:45` | 低（已查证） |
| `MAX_NR_PROCS = 31742` | `endpoint.h:57` 计算 | 低（已查证） |
| `MAX_NR_TASKS = 1023` | `com.h` 标准定义 | 低 |
| 当前 `NR_PROCS = 256` | `sys_config.h:8` | 低 |
| endpoint_t 32 位中 generation+slot 总和 ≤ 31 位 | 数学事实 | 零风险 |
| Linux pid_max 默认 4M（systemd 2019+） | 公开资料 | 低 |
| minix3 PM 层有完整 `mp_realuid/effuid/svuid/realgid/effgid/svgid` | `mproc.h:41-43` 已 grep | 低 |
| minix-rs `os/servers/pm/src/mproc/credentials.rs` 已实现 uid 三元组 | `rg "mp_realuid\|mp_effuid"` 命中 | 低 |
| minix-rs `KProcess` 不含 uid 字段 | `rg "uid" os/kernel/src/` 0 命中 | 低 |
| minix3 `endpoint.h:39-43` 注释明确 SHIFT 可调整 | 直接 read | 零风险 |

### 10.2 中可信度断言（基于推理/经验值）

| 断言 | 推理依据 | 风险 |
|------|---------|------|
| 5 位 generation（32 次复用）"足够"实际系统 | 物理上 slot 复用率 = 进程平均生命周期 / 系统寿命，远 < 100 万次 | 中（理论值，需要在文档论证） |
| SHIFT = 8 或 10 是甜点 | slot 4M 或 100万；generation 256 或 1024 次复用 | 中（需要在具体场景下验证） |
| 31 742 在 Minix3 形态"够用" | 单用户 / 教学 / 中小型服务器场景进程数 < 5000 | 中（"够用"是相对判断） |
| 物理内存才是真正的硬上限 | 每进程栈 64 KB × 31742 ≈ 2 GB | 中（栈大小是估算） |
| 方案 B 工程量 1-2 月 | 类似 ABI 重设计经验 | 中（取决于团队规模） |
| 方案 C 工程量 6-12 月 | 涉及 IPC 全面重设计 | 中（取决于范围定义） |

### 10.3 低可信度断言（需要 GPT 进一步讨论的开放问题）

| 断言 | 风险 |
|------|------|
| 用户真的"想调整 endpoint 协议" vs "只是抱怨容量太小" | 高（用户语义模糊） |
| 用户希望保留 IPC 路径不变 vs 接受 IPC ABI 变更 | 高（影响方案选择） |
| minix-rs 项目阶段是否会扩到云 OS 场景 | 高（影响容量目标） |
| 测试覆盖的完整度（影响方案 A/B 的工作量估算） | 中 |

### 10.4 与本文档强相关的可信源链接（minix-rs 仓库内）

- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md` §2.0 第三层（讨论 256 限制的当前章节）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-todo.md`（06 的设计历史）
- `minix3/minix/include/minix/endpoint.h`（endpoint_t 定义源头）
- `minix3/minix/include/minix/com.h`（MAX_NR_TASKS 定义）
- `minix3/minix/include/minix/sys_config.h`（NR_PROCS 默认值）
- `os/libs/minix-types/src/types/com.rs`（minix-rs Rust 端 NR_PROCS 等常量定义）
- `os/servers/pm/src/mproc/credentials.rs`（minix-rs uid 实现位置）

---

## 11. 与本文档强相关的现有文件清单

### 11.1 项目根文件

- `CLAUDE.md`（项目主指令，与本文档强相关：含三级术语、execution 约束、ABI 演进规则）
- `AGENTS.md`（Codex 入口，与 CLAUDE.md 同源）
- `.design/`、`tmp_design_and_todo/`（Hidden Folder，**正式 doc 不得引用**）

### 11.2 文档目录

- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/`（本文档所在 stage）
  - `06-proc-init-boot-proc.md`（讨论 256 限制的当前文档）
  - `06-todo.md`（06 的设计 / 重构 TODO）
  - `12-ipc-core.md`（IPC 协议细节，待讨论后可能更新）
  - `99-global-concepts.md`（全局概念索引）

### 11.3 上游 C 源码（ground truth）

- `minix3/minix/include/minix/endpoint.h`（endpoint_t 定义）
- `minix3/minix/include/minix/com.h`（NR_TASKS、MAX_NR_TASKS）
- `minix3/minix/include/minix/sys_config.h`（NR_PROCS 默认值 256）
- `minix3/minix/include/minix/ipc.h`（Message 结构体定义）

### 11.4 minix-rs Rust 实现

- `os/libs/minix-types/src/types/com.rs`（Rust 端常量）
- `os/libs/minix-types/src/types/endpoint.rs`（Rust 端 Endpoint newtype）
- `os/libs/minix-types/src/ipc/`（IPC 协议相关）
- `os/kernel/src/`（kernel 实现，含 KProcess / ProcessTable / PrivTable）
- `os/servers/pm/src/mproc/`（PM 端 mproc 实现，已包含 uid 三元组）

---

## 12. 待决问题清单（与 GPT 讨论的核心输入）

下面这些问题是**真正需要 GPT 设计判断**的——AI 不能仅基于"读仓库"就回答，需要权衡：

### Q1：方案选择

**用户的真实意图**是：
- (A) 仅扩展 Minix3 形态的进程容量（推荐方案 A）？
- (B) 为 64 位原生场景铺路（推荐方案 B）？
- (C) 把 IPC 协议作为独立 ARCH 项目重构（推荐方案 C，独立项目）？
- (D) 其他？

### Q2：SHIFT 选择（若选方案 A）

如果选方案 A，**SHIFT 应该是多少**？
- SHIFT = 12（slot 262K，generation 4K）—— 保守
- SHIFT = 10（slot 1M，generation 1K）—— 推荐
- SHIFT = 8（slot 4M，generation 256）—— Linux 量级
- SHIFT = 5（slot 32M，generation 32）—— 可能不够

**generation 多少位就够用**？5 位（32 次）实际够吗？8 位（256 次）够吗？需要什么推理依据？

### Q3：方案 A 的 IPC 路径影响

方案 A 改 SHIFT，**IPC Message 结构体的对齐和填充**会变吗？哪些结构体受影响最大？需要重排吗？

### Q4：方案 B 的 Message 重排影响

方案 B 改 endpoint_t 到 64 位，**所有 Message 共享结构**都需要重排。列出**所有受影响的结构体清单**，估算工作量。

### Q5：方案 C 的"endpoint 与 slot 解耦"具体形式

方案 C 是彻底重构。**应该走 7.2.a（HashMap）、7.2.b（两级寻址）、7.2.c（完全解耦）哪条路**？各自的代价是什么？

### Q6：与 minix3 上游的兼容性

minix-rs 的目标是**保留 Minix3 IPC 协议**。如果 minix3 上游未来扩到 64 位 endpoint，**minix-rs 是跟随还是领先**？这影响方案选择。

### Q7：与 endpoint_t 演进的成本估算

**精确估算每个方案的工作量**（人月）。包括：
- 代码改动量（LOC）
- 重编译影响面（多少 binary）
- 测试覆盖范围（IPC 路径条数）
- 文档更新工作量

### Q8：方案的"不可逆性"

**哪个方案是单行道**（一旦做了就无法回退）？
- 方案 A 改 SHIFT：可以反复调（但每调一次都要重测 + 重编）
- 方案 B 改 64 位：技术上是可逆的，但所有 binary 重编成本巨大
- 方案 C 解耦：是个新协议，**几乎不可逆**

如果未来 Minix3 上游有更激进的方案，minix-rs 应该**跟随**还是**领导**？

### Q9：CLAUDE.md 三级术语判定

每个方案属于：
- **Rewrite**（不改外部行为）？
- **Refactor**（内部重构）？
- **Architectural Evolution**（结构性变化）？

**Architecture Evolution 必须 doc + design + code 三处一致标注 `[ARCH: ...]`**——这影响方案的工作流。

### Q10：阶段性策略

minix-rs 是**渐进重写**项目。如果 endpoint_t 演进分多个版本，每个版本应该按什么顺序推出？

---

**附录：本文件准备给 GPT 网页端的讨论用，所以保留了所有事实上下文——GPT 没有代码访问权限。**

**附录：本文档不是执行清单，是讨论输入。GPT 的输出应该是设计建议，不应该是行号指令。**

---

**版本**：v0.1 草稿（2026-08-30）
