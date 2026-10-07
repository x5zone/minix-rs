# Minix-RS Review 核心语义定义与行为契约表模板

> 本文件定义 Minix-RS Review 的**核心语义对齐标准**，并提供**全量行为契约表模板**。
> 核心语义是 Rewrite 的"外部可观察行为"的精确定义，是 Review 的最高优先级检查项。
> **强制规则**：每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。

---

## 一、核心语义定义

### 1.1 什么是"核心语义"

核心语义是 Minix3 模块**外部可观察行为**的精确定义，包括：

| 维度 | 内容 | 验证方式 |
|------|------|---------|
| **IPC 协议** | 消息格式、调用号、参数布局、返回值 | grep Minix3 message 结构 + 调用号定义 |
| **生命周期语义** | 资源创建、使用、释放的顺序和条件 | 追踪 alloc/create → use → free/destroy 路径 |
| **错误恢复语义** | 错误码含义、重试策略、状态回滚 | 对比 Minix3 errno 返回路径 |
| **调度/权限语义** | 进程隔离、内存保护、调度策略 | 验证权限检查和调度点 |
| **地址空间语义** | 虚拟地址布局、物理地址映射、共享内存 | 对比页表操作和地址计算 |

### 1.2 核心语义不变性原则

**Rewrite 必须保持的核心语义不变**：

1. **IPC 接口不变**：消息格式、调用号、参数顺序、返回值类型
2. **生命周期不变**：资源创建/释放的时序和条件
3. **错误语义不变**：相同输入产生相同错误码
4. **权限语义不变**：相同操作需要相同权限
5. **地址空间语义不变**：相同虚拟地址映射到相同物理地址（在相同配置下）

**允许变化**（架构演进）：
- 内部数据结构组织
- 状态表达方式（int → enum）
- 错误处理方式（errno → Result）
- 内存管理方式（手动 → RAII）
- 位宽和页表层级（32→64, 2→4 级）

### 1.3 核心语义违反 = P0

以下任何一项违反，直接 P0：

| 违反类型 | 示例 | 判定 |
|---------|------|------|
| IPC 协议改变 | 改变消息字段顺序或类型 | P0 |
| 生命周期改变 | 提前释放资源导致 use-after-free | P0 |
| 错误码改变 | Minix3 返回 ENOMEM，Rust 返回 EINVAL | P0 |
| 权限改变 | Minix3 检查 root，Rust 不检查 | P0 |
| 地址空间改变 | 改变虚拟地址布局 | P0 |

### 1.4 核心术语扩展（Refactor 定义）

> **背景**：解决历史"Redesign 术语冲突"。本节统一术语后，跨文档沟通无歧义。

#### Refactor（重构）

**定义**：Rust 实现偏离 design 时，修复 Rust 代码使其回到 design 路径。Refactor 是 Rewrite 内部纠错，不涉及架构变化。

**与 Rewrite 的关系**：Refactor 是 Rewrite 内部的纠错动作，不涉及架构变化。

**典型场景**：
- design 已规定用 `enum Syscall + match`，但实现用了函数指针数组
- design 已规定用 `&'static dyn Trait`，但实现用了 `enum` 暴露类型
- design 已规定 DirectMapArch 替代 MemoryInitArch，但实现两者并存

**与 Architectural Evolution 的关系**：
- **Refactor**（含 design Refactor 与 code Refactor）：
  - **code Refactor**：design 不变，修 code（对应 P0-design-deviation，触发于 review Step 1.6.2）
  - **design Refactor**：design 自身不抓本质/漏概念，先修 design 再修 code（对应 P0-design-missing/wrong，触发于 review Step 1.6.3）
- **Architectural Evolution**：跨越 Minix3 语义边界的设计变更（如 32 位临时窗口 → 64 位 direct_map），需用户显式批准

**Review 中的判定**：
- **P0-design-deviation** → code Refactor 触发（design 正确，仅 code 偏离，修 code 回到 design）
- **P0-design-missing** → design Refactor 候选（design 未规定，先补 design 再修 code）
- **P0-design-wrong** → design Refactor 必须（design 本身错，先 redesign 再修 code）
- **Architectural Evolution 候选** → 跨越 Minix3 语义边界的设计变更，升级用户裁决

### 1.5 §核心语义判定优先级

> **目的**：当 design 与 Minix3 C 源码矛盾时，按以下优先级判定，避免"凭印象反转优先级"。

**判定优先级**（从高到低）：
1. **Minix3 C 源码**（ground truth，优先级最高）
2. **design.md**（非 bagging）/ **design-final.md**（bagging）（如果 design 是基于 Minix3 的合理 Rewrite）
3. **当前 Rust 实现**（如果实现符合 design）
4. **文档**

**冲突处理（判定矩阵）**：

> **P0 六类定义详见 [review.md §P0 六分类](review.md)**。以下为 design↔code 状态到 P0 类型的映射：

| design vs Minix3 | code vs design | 判定 |
|----------------|---------------|------|
| 一致 | 偏离 | **P0-design-deviation**（code Refactor：修 code） |
| 一致 | 一致 | ✅ PASS |
| 不一致 | 偏离 | **P0-design-wrong**（design Refactor 必须：先修 design 再修 code） |
| 缺失 | 缺失 | **P0-design-missing**（design Refactor：先补 design 再补 code） |
| 缺失 | 实现但符合 Minix3 | **P0-design-missing**（补 design）|

> **配套规则**：[review.md §Design First 原则](review.md) + [review.md §Ground Truth 优先级](review.md#ground-truth-优先级)。

---

### 1.6 跨架构抽象的四条公理（架构差异的裁决依据）

> **为什么要写进规范源**：这四条最初在「三架构待决项清单」PD-33 的第六、七轮讨论里定稿，
> 长期只存在于案卷与会话中，规范源内检索「公理」命中为 0。后果是每遇到一个新架构差异都要从头
> 把原则重新推导一遍（本次六轮讨论本质上是在重新发明它）。本节把它固化为可判定规则。

- **公理一：trait 抽象的是 OS 需要的语义，不是硬件的形状。** 硬件表示（寄存器名、页表根数量、
  指令名）不得出现在 OS 语义层的接口里；它们允许出现在架构边界与架构实现内部。
- **公理二：同一个 OS 语义实体，可以由不同架构用不同硬件形态实现。** 逻辑地址空间在 x86_64 与
  riscv64 是单个页表根，在 aarch64 是两个根（低半与高半各一个）。两者都是合法实现，因为 OS 语义
  只要求「每个进程有自己的地址翻译空间」。
- **公理三：不为消除架构差异而制造伪统一。** 只有当多个架构对 OS 暴露的语义确实相同时才共享同一个
  trait；语义不同就分成两个接口各自实现。示范判据：**「两个页表根是硬件事实；Minix 的地址空间才是
  OS 语义。正确的抽象不是把两个根伪装成一个根，而是根本不把『根的数量』当作 OS 语义。」**
- **公理四：架构实现可以复杂，OS 层必须简单。** 一句话版：**抽象语义，不抽象硬件形状；
  trait 管它要表达什么（what），实现管它怎么做到（how）。**

**配套三问**（遇到任何架构差异先按顺序问，这是可复述的判据）：

1. 这是 OS 关心的语义，还是硬件的实现事实？
2. 把硬件名词全部删掉，还能说清这个接口是什么吗？
3. 有没有两个硬件实现共同满足同一个 OS 语义？有 → 这正是 trait 存在的理由；没有 → 不该共享 trait。

**三条边界**（防止原则被念成新的教条，与公理同等强制）：

- 架构边界 trait **自身**可以出现硬件名词；被禁止的是它泄漏进 OS 语义层。判断对象是「哪一层在用它」，
  不是「这个词能不能出现」。
- **启动契约不在抽象射程内**：它是平台把系统送入 OS 的方式，不是 OS 运行时机制，强行统一反而违反公理
  （实证：PD-07 裁「不统一」、PD-14 裁「自引导定终态」都由这条推出）。
- 公理三同时禁止两个方向：既禁止「把两个根伪造成一个根」，也禁止「为了将来的漂亮提前改现在的契约」。
  维持当前简单实现、把抽象推迟到出现第二个真实使用者之后，**不算违反公理**（PD-18 撤掉预防性不透明票据
  即此判例）。反向误伤测试见 `prompt/review-rules/review-patterns.md` 模式 85。

**依赖射程的唯一定义**（任何守卫、任何检查项都按此判，不得回退成过宽版本）：OS 语义层**允许**依赖
架构边界接口（仓内的 `*Arch` trait）；**禁止**的是直接依赖具体架构实现模块、具体架构类型、
CSR 与汇编原语，以及 `x86_64` / `aarch64` / `riscv64` 三个实现符号。「不得导入任何架构专属符号」
这种写法会误杀四层模型自己设计的合法依赖关系。

**已知待裁决的豁免面**（实测登记，本节不改代码）：按上述定义扫描，用户态有 4 处直接导入具体架构实现
（`os/servers/vm/src/vmdm_bridge.rs` 三处与 `os/servers/vm/src/alloc_page.rs` 一处，均指向
`minix_arch::riscv64::paging::vmdm::*`），内核通用层另有约 65 处子模块导入。豁免形态三选一
（把 VM 定性为边界层成员 / 收进 `*Arch` 接口 / 建立白名单并定期回看）尚未裁决，登记在
`rewrite-notes/coordination/PENDING-DECISIONS-3ARCH-PARITY.md` 的 PD-33「冻结前合规实测」段。

## 二、全量行为契约表模板

> 对每个 C 函数，定义其完整行为契约。Review 时逐项验证 Rust 实现是否满足契约。

### 2.1 行为契约表字段（标准化 8 字段）

> **全量行为契约表**与 **Top 5 行为契约表**（Gate B 必须）统一使用下方标准化 8 字段模板；非核心函数可使用 §2.3 简化模板（仅函数名+输入+输出+错误码）。
> 注：8 字段是早期 11 字段的标准化合并（边界条件并入输入契约；不变量并入时序契约），当前文档仅保留 8 字段模板。

| # | 字段 | 说明 | Evidence 级别 |
|---|------|------|-------------|
| 1 | 函数名 | C 函数名 → Rust 函数名（含语义范围） | [DIRECT] |
| 2 | 输入契约 | 参数类型、取值范围、前置条件、**边界条件**（极端输入、空指针、零长度） | [DIRECT] |
| 3 | 输出契约 | 返回值含义、取值范围 | [DIRECT] |
| 4 | 副作用 | 全局状态、IPC 调用、硬件操作 | [MEDIUM] |
| 5 | 错误码 | 所有可能 errno 及触发条件 → Rust Error 变体 | [DIRECT] |
| 6 | 时序契约 | 调用顺序约束、并发约束、**不变量**（执行前后保持） | [INFERRED] |
| 7 | 生命周期 | 分配/释放责任、所有权转移、RAII | [MEDIUM] |
| 8 | 架构演进 | 32→64 位变化、类型变化、ARCH 标记 | [DIRECT] |

### 2.2 行为契约表模板（Top 5 必须 8 字段；非核心函数可简化）

> 对每个核心函数填写此表。**Gate B 强制要求 Top 5 行为契约表为 8 字段 × 5 函数**。
> 非核心函数可使用 §2.3 简化模板（仅函数名+输入+输出+错误码）。

```markdown
### 函数: {C 函数名} → {Rust 函数名}

| # | 字段 | C 行为 | Rust 实现 | 匹配? | Evidence |
|---|------|--------|----------|-------|----------|
| 1 | 函数名 | `c_func` (语义范围: 模块/功能) | `rust_func` (语义范围: 模块/功能) | ✅/❌ | [DIRECT] |
| 2 | 输入契约 | {参数类型/范围/前置条件/边界} | {参数类型/范围/前置条件/边界} | ✅/❌ | [DIRECT] |
| 3 | 输出契约 | {返回值含义/取值范围} | {返回值含义/取值范围} | ✅/❌ | [DIRECT] |
| 4 | 副作用 | {全局状态/IPC/硬件} | {全局状态/IPC/硬件} | ✅/❌ | [MEDIUM] |
| 5 | 错误码 | {errno 列表+条件} | {Error 变体+条件} | ✅/❌ | [DIRECT] |
| 6 | 时序契约 | {调用顺序/并发/不变量} | {调用顺序/并发/不变量} | ✅/❌ | [INFERRED] |
| 7 | 生命周期 | {alloc/free 责任} | {RAII/所有权} | ✅/❌ | [MEDIUM] |
| 8 | 架构演进 | {32位行为} | {64位行为/ARCH} | ✅/❌/N/A | [DIRECT] |

**C 源码位置**: `minix3/minix/servers/{module}/FILE.c:LINE`
**Rust 源码位置**: `os/servers/{module}/FILE.rs:LINE`

**差异说明**（如有 ❌）:
- {字段}: {C 行为} vs {Rust 行为} → {是否为允许的架构演进 / 是否为 P0 违反}

**验证命令**:
```bash
rg "C_FUNC_NAME" minix3/minix/servers/{module}/ --type c -n
rg "rust_func_name" os/servers/{module}/src/ --type rust -n
```
```

### 2.3 简化行为契约表（非核心函数）

> 非核心函数（如辅助函数、工具函数）使用简化表。

| C 函数 | Rust 函数 | 输入 | 输出 | 错误码 | 匹配? | Evidence |
|--------|----------|------|------|--------|-------|----------|
| `func_a` | `func_a` | `(int x)` | `int` | `EINVAL` if x<0 | ✅ | [DIRECT] |

---

## 三、模块核心语义清单（按模块）

> 每个模块的核心语义清单在 Review 时根据 C 源码动态生成。以下为模板。

### 3.1 VM 模块核心语义清单（示例模板）

> 实际 Review 时，从 `minix3/minix/servers/vm/` 提取核心函数，填写行为契约表。

| 核心函数 | 语义范围 | 优先级 | 契约表状态 |
|---------|---------|--------|-----------|
| `alloc_mem` | 内存分配 | P0 | 待填写 |
| `free_mem` | 内存释放 | P0 | 待填写 |
| `map_vm` | 地址映射 | P0 | 待填写 |
| `vm_fork` | Fork 语义 | P0 | 待填写 |
| `vm_exit` | Exit 语义 | P0 | 待填写 |
| `vm_brk` | Brk 语义 | P0 | 待填写 |

### 3.2 PM 模块核心语义清单（示例模板）

| 核心函数 | 语义范围 | 优先级 | 契约表状态 |
|---------|---------|--------|-----------|
| `do_fork` | 进程创建 | P0 | 待填写 |
| `do_exit` | 进程退出 | P0 | 待填写 |
| `do_exec` | 执行新程序 | P0 | 待填写 |
| `do_waitpid` | 等待子进程 | P0 | 待填写 |
| `do_kill` | 信号发送 | P0 | 待填写 |

### 3.3 VFS 模块核心语义清单（示例模板）

| 核心函数 | 语义范围 | 优先级 | 契约表状态 |
|---------|---------|--------|-----------|
| `vfs_open` | 文件打开 | P0 | 待填写 |
| `vfs_close` | 文件关闭 | P0 | 待填写 |
| `vfs_read` | 文件读取 | P0 | 待填写 |
| `vfs_write` | 文件写入 | P0 | 待填写 |
| `vfs_ioctl` | 设备控制 | P0 | 待填写 |

---

## 四、IPC 协议契约表模板

> 对每个 IPC 调用，定义其协议契约。

### 4.1 IPC 消息契约字段

| 字段 | 说明 | Evidence 级别 |
|------|------|-------------|
| **调用号** | Minix3 中定义的 call number | [DIRECT] grep |
| **消息类型** | 请求/响应消息结构体名 | [DIRECT] grep |
| **参数布局** | 消息字段名、类型、顺序 | [DIRECT] grep struct |
| **返回值** | 响应消息中的返回字段 | [DIRECT] grep |
| **权限检查** | 调用前的权限验证 | [MEDIUM] 需读代码 |
| **副作用** | 状态修改、后续 IPC | [MEDIUM] 需读代码 |
| **错误码** | 所有可能的 errno | [DIRECT] grep error path |

### 4.2 IPC 契约表模板

```markdown
### IPC: {调用名} (call number: {NUMBER})

| 字段 | Minix3 | minix-rs | 匹配? | Evidence |
|------|--------|----------|-------|----------|
| 调用号 | `{NUMBER}` | `{NUMBER}` | ✅/❌ | [DIRECT] |
| 请求消息 | `struct msg_req` | `struct MsgReq` | ✅/❌ | [DIRECT] |
| 参数布局 | `{field1: type, field2: type}` | `{field1: type, field2: type}` | ✅/❌ | [DIRECT] |
| 返回值 | `{field: type}` | `{field: type}` | ✅/❌ | [DIRECT] |
| 权限检查 | `{check}` | `{check}` | ✅/❌ | [MEDIUM] |
| 副作用 | `{side effects}` | `{side effects}` | ✅/❌ | [MEDIUM] |
| 错误码 | `{errno list}` | `{Error variants}` | ✅/❌ | [DIRECT] |

**C 源码位置**: `minix3/minix/servers/{module}/FILE.c:LINE`
**Rust 源码位置**: `os/servers/{module}/FILE.rs:LINE`

**验证命令**:
```bash
rg "#define.*{CALL_NAME}" minix3/minix/include/ -n
rg "struct.*msg" minix3/minix/servers/{module}/ -n
```
```

---

## 五、生命周期契约表模板

> 对每个资源，定义其生命周期契约。

### 5.1 生命周期契约字段

| 字段 | 说明 | Evidence 级别 |
|------|------|-------------|
| **资源名** | 资源类型名 | [DIRECT] |
| **创建点** | 分配/创建的函数和条件 | [DIRECT] grep alloc |
| **使用点** | 访问/修改的函数 | [MEDIUM] 需追踪引用 |
| **释放点** | 释放/销毁的函数和条件 | [DIRECT] grep free |
| **所有权** | 谁持有所有权、所有权转移规则 | [INFERRED] 需分析 |
| **不变量** | 生命周期内的不变量 | [INFERRED] |
| **错误路径** | 创建/使用失败时的释放策略 | [MEDIUM] 需读 error path |

### 5.2 生命周期契约表模板

```markdown
### 资源: {资源名}

| 字段 | C 行为 | Rust 实现 | 匹配? | Evidence |
|------|--------|----------|-------|----------|
| 创建点 | `{func}() at FILE:LINE` | `{func}() at FILE:LINE` | ✅/❌ | [DIRECT] |
| 使用点 | `{func1}, {func2}` | `{func1}, {func2}` | ✅/❌ | [MEDIUM] |
| 释放点 | `{func}() at FILE:LINE` | `Drop::drop` at FILE:LINE | ✅/❌ | [DIRECT] |
| 所有权 | `{owner} 持有` | `{Owner} struct 持有` | ✅/❌ | [INFERRED] |
| 不变量 | `{invariant}` | `{invariant}` | ✅/❌ | [INFERRED] |
| 错误路径 | `{error handling}` | `{error handling}` | ✅/❌ | [MEDIUM] |

**C 源码位置**: `minix3/minix/servers/{module}/FILE.c:LINE`
**Rust 源码位置**: `os/servers/{module}/FILE.rs:LINE`

**验证命令**:
```bash
rg "alloc|create|new" minix3/minix/servers/{module}/ --type c -n | grep RESOURCE
rg "free|destroy|drop" minix3/minix/servers/{module}/ --type c -n | grep RESOURCE
```
```

---

## 六、Review 执行流程（核心语义部分）

### 6.1 核心语义 Review 步骤

1. **提取核心函数清单**：
   ```bash
   rg "^[a-z_].*\w+\(.*\)\s*$" minix3/minix/servers/{module}/*.c -n
   ```
   筛选出语义范围内的核心函数（排除 static 辅助函数）。

2. **为每个核心函数填写行为契约表**：
   - 使用 §2.2 模板
   - 每个字段标注 evidence 级别
   - C 源码位置必须精确到行号

3. **提取 IPC 调用清单**：
   ```bash
   rg "#define.*CALL" minix3/minix/include/ -n | grep {module}
   rg "message.*msg" minix3/minix/servers/{module}/ -n
   ```

4. **为每个 IPC 调用填写契约表**：
   - 使用 §4.2 模板

5. **提取资源清单**：
   ```bash
   rg "alloc|malloc|new" minix3/minix/servers/{module}/ --type c -n
   rg "free|destroy" minix3/minix/servers/{module}/ --type c -n
   ```

6. **为每个资源填写生命周期契约表**：
   - 使用 §5.2 模板

7. **对比验证**：
   - 对每个契约表的"匹配?"列，grep 验证 Rust 实现
   - 任何 ❌ 必须说明：是允许的架构演进，还是 P0 违反

### 6.2 核心语义违反的判定标准

| 情况 | 判定 |
|------|------|
| C 返回 ENOMEM，Rust 返回 `Err(AllocError)` | ✅ 允许（errno→Result 演进） |
| C 返回 ENOMEM，Rust 返回 `Err(InvalidParam)` | ❌ P0（错误码语义改变） |
| C 在函数 A 中 alloc，Rust 在函数 B 中 alloc | ❌ P0（生命周期改变） |
| C 检查 `super_user`，Rust 不检查 | ❌ P0（权限语义改变） |
| C 用 `u32` 地址，Rust 用 `u64` 地址 | ✅ 允许（32→64 位演进） |
| C 消息字段顺序 `[a, b, c]`，Rust `[a, c, b]` | ❌ P0（IPC 协议改变） |

---

## 七、与现有 Review 文件的关系

| 文件 | 角色 | 与本文件关系 |
|------|------|-------------|
| [review.md](review.md) | 核心原则 | 本文件细化"核心语义不变"原则 |
| [review-doc-checklist.md](review-doc-checklist.md) | 文档检查 | 本文件提供行为契约表模板 |
| [review-code-checklist.md](review-code-checklist.md) | 代码检查 | 本文件提供语义对齐标准 |
| [review-patterns.md](review-patterns.md) | 错误模式 | 本文件定义 P0 违反模式 |
| [review-process.md](review-process.md) | 执行流程 | 本文件在 Step 2（Diff Extraction）使用 |

### 在 Review 流程中的位置

```
Step 1: Ground Truth Lookup
  ↓
Step 1.5: Coverage Enumeration
  ↓
Step 2: Diff Extraction ← 使用本文件的行为契约表
  ↓
Step 2.5: Link Validation
  ↓
Step 3: Sanity Check
  ↓
...
```

**具体使用**：
- Step 2（Diff Extraction）: 使用行为契约表识别 Top 5 语义差异（3 语义偏移 + 2 覆盖缺口，Gate B 8 字段 × 5 函数）
- Step 3（Sanity Check）: 验证契约表的"匹配?"列
- Step 6（Action Items）: 对 P0 违反生成具体修改项

---

## 八、Pass condition

- 所有核心函数有行为契约表
- 所有 IPC 调用有契约表
- 所有核心资源有生命周期契约表
- 所有"匹配?"列经验证
- 任何 ❌ 有明确判定（允许演进 / P0 违反）

**⛔ 核心函数无契约表 → P0。契约表字段缺失 → P1。**
