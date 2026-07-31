---
name: review-core-semantics-skill
description: Minix-RS Review 核心语义定义与行为契约表模板。定义核心语义不变性原则（IPC/生命周期/错误/权限/地址空间），提供全量行为契约表（8 字段标准化）、IPC 契约表、生命周期契约表模板。在 Step 2 Diff Extraction 时调用此 Skill 识别 Top 5 语义差异（3 语义偏移 + 2 覆盖缺口）。
---

# Minix-RS Review 核心语义定义与行为契约表

> 本 Skill 定义核心语义对齐标准，是 Review 的最高优先级检查项。
> **详见**：[review-core-semantics.md](../review-rules/review-core-semantics.md)

---

## 一、核心语义定义

### 1.4 核心术语扩展

> 配合 [review-core-semantics.md §1.4](../review-rules/review-core-semantics.md) 的术语统一。

**Refactor 定义**（Refactor ≠ Redesign）：
- **Refactor (代码 Refactor)**：在不改外部行为的前提下，重写代码内部表达（如改命名、抽函数、改数据结构、改错误表达）。语义范围 = 代码。
- **Refactor (设计 Refactor)**：在不改变外部语义的前提下，修正或优化 **design 文档本身**（如细化接口、补充错误码策略、调整不变量表述、补全 trait 设计）。语义范围 = design doc。
- 两者均**不改 IPC/生命周期/错误/权限/地址空间语义**，仅改表达层。

**判定 P0 vs 设计 Refactor**：
- 代码偏离 design 但 design 自己错了 → **设计 Refactor**（改 design）+ 代码随之对齐
- 代码偏离 design 且 design 没错 → **P0-design-deviation**（改代码）
- 代码未偏离 design，但 design 本身缺关键决策 → **P0-design-missing**（补 design）

### 1.5 核心语义判定优先级

> 当 Minix3 行为、design 文档、代码实现、文档叙事发生冲突时，按以下优先级判定：

**优先级链**：`Minix3 源码行为 > design doc > Rust 代码 > 设计/技术文档`

**冲突矩阵**：

| # | 冲突情况 | 判定 | 处理 |
|---|---------|------|------|
| 1 | Minix3 与 design 不一致 | ✅ 改 design + 标 ARCH（如有架构演进） | design Refactor |
| 2 | Minix3 与 Rust 代码不一致 | ❌ P0-core-semantics | 改代码 |
| 3 | design 与 Rust 代码不一致 | ❌ P0-design-deviation | 改代码（design 是契约） |
| 4 | design 缺关键决策（如未定义 trait） | ❌ P0-design-missing | 补 design，再写代码 |
| 5 | 技术文档与 design 不一致 | ❌ P1-doc-design-drift | 改技术文档 |
| 6 | design 自身表述模糊 | ✅ 设计 Refactor | 改 design 表述 |

### 1.1 什么是核心语义

核心语义是 Minix3 模块**外部可观察行为**的精确定义：

| 维度 | 内容 | 验证方式 |
|------|------|---------|
| IPC 协议 | 消息格式、调用号、参数布局、返回值 | grep message 结构 + 调用号 |
| 生命周期语义 | 资源创建、使用、释放的顺序和条件 | 追踪 alloc → use → free 路径 |
| 错误恢复语义 | 错误码含义、重试策略、状态回滚 | 对比 errno 返回路径 |
| 调度/权限语义 | 进程隔离、内存保护、调度策略 | 验证权限检查和调度点 |
| 地址空间语义 | 虚拟地址布局、物理地址映射、共享内存 | 对比页表操作和地址计算 |

### 1.2 核心语义不变性原则（违反任一 = P0）

1. **IPC 接口不变**：消息格式、调用号、参数顺序、返回值类型
2. **生命周期不变**：资源创建/释放的时序和条件
3. **错误语义不变**：相同输入产生相同错误码
4. **权限语义不变**：相同操作需要相同权限
5. **地址空间语义不变**：相同虚拟地址映射到相同物理地址（相同配置下）

**允许变化**（架构演进）：内部数据结构 / 状态表达（int→enum）/ 错误处理（errno→Result）/ 内存管理（手动→RAII）/ 位宽和页表层级

### 1.3 核心语义违反判定

| 情况 | 判定 |
|------|------|
| C 返回 ENOMEM，Rust 返回 `Err(AllocError)` | ✅ 允许（errno→Result 演进） |
| C 返回 ENOMEM，Rust 返回 `Err(InvalidParam)` | ❌ P0（错误码语义改变） |
| C 在函数 A 中 alloc，Rust 在函数 B 中 alloc | ❌ P0（生命周期改变） |
| C 检查 `super_user`，Rust 不检查 | ❌ P0（权限语义改变） |
| C 用 `u32` 地址，Rust 用 `u64` 地址 | ✅ 允许（32→64 位演进） |
| C 消息字段顺序 `[a, b, c]`，Rust `[a, c, b]` | ❌ P0（IPC 协议改变） |

---

## 二、全量行为契约表模板

### 2.1 行为契约表字段（标准化 8 字段）

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

### 2.2 行为契约表模板（核心函数，8 字段）

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
- {字段}: {C 行为} vs {Rust 行为} → {允许的架构演进 / P0 违反}

**验证命令**:
```bash
rg "C_FUNC_NAME" minix3/minix/servers/{module}/ --type c -n
rg "rust_func_name" os/servers/{module}/src/ --type rust -n
```
```

### 2.3 简化行为契约表（非核心函数，8 字段精简版）

| C 函数 | Rust 函数 | 输入 | 输出 | 错误码 | 时序 | 生命周期 | 架构 | 匹配? |
|--------|----------|------|------|--------|------|---------|------|-------|
| `func_a` | `func_a` | `(int x)` | `int` | `EINVAL` if x<0 | 无约束 | 无 | N/A | ✅ |

---

## 三、IPC 协议契约表模板

### 3.1 IPC 契约字段

| 字段 | 说明 | Evidence 级别 |
|------|------|-------------|
| 调用号 | Minix3 call number | [DIRECT] |
| 消息类型 | 请求/响应消息结构体名 | [DIRECT] |
| 参数布局 | 消息字段名、类型、顺序 | [DIRECT] |
| 返回值 | 响应消息中的返回字段 | [DIRECT] |
| 权限检查 | 调用前的权限验证 | [MEDIUM] |
| 副作用 | 状态修改、后续 IPC | [MEDIUM] |
| 错误码 | 所有可能 errno | [DIRECT] |

### 3.2 IPC 契约表模板

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

**验证命令**:
```bash
rg "#define.*{CALL_NAME}" minix3/minix/include/ -n
rg "struct.*msg" minix3/minix/servers/{module}/ -n
```
```

---

## 四、生命周期契约表模板

### 4.1 生命周期契约字段

| 字段 | 说明 | Evidence 级别 |
|------|------|-------------|
| 资源名 | 资源类型名 | [DIRECT] |
| 创建点 | 分配/创建的函数和条件 | [DIRECT] |
| 使用点 | 访问/修改的函数 | [MEDIUM] |
| 释放点 | 释放/销毁的函数和条件 | [DIRECT] |
| 所有权 | 谁持有所有权、所有权转移规则 | [INFERRED] |
| 不变量 | 生命周期内的不变量 | [INFERRED] |
| 错误路径 | 创建/使用失败时的释放策略 | [MEDIUM] |

### 4.2 生命周期契约表模板

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

**验证命令**:
```bash
rg "alloc|create|new" minix3/minix/servers/{module}/ --type c -n | grep RESOURCE
rg "free|destroy|drop" minix3/minix/servers/{module}/ --type c -n | grep RESOURCE
```
```

---

## 五、Review 执行流程（核心语义部分）

### 5.1 核心语义 Review 步骤

1. **提取核心函数清单**：
   ```bash
   rg "^[a-z_].*\w+\(.*\)\s*$" minix3/minix/servers/{module}/*.c -n
   ```
   筛选语义范围内的核心函数（排除 static 辅助函数）。

2. **为每个核心函数填写行为契约表**（使用 §2.2 模板）

3. **提取 IPC 调用清单**：
   ```bash
   rg "#define.*CALL" minix3/minix/include/ -n | grep {module}
   rg "message.*msg" minix3/minix/servers/{module}/ -n
   ```

4. **为每个 IPC 调用填写契约表**（使用 §3.2 模板）

5. **提取资源清单**：
   ```bash
   rg "alloc|malloc|new" minix3/minix/servers/{module}/ --type c -n
   rg "free|destroy" minix3/minix/servers/{module}/ --type c -n
   ```

6. **为每个资源填写生命周期契约表**（使用 §4.2 模板）

7. **对比验证**：
   - 对每个契约表的"匹配?"列，grep 验证 Rust 实现
   - 任何 ❌ 必须说明：是允许的架构演进，还是 P0 违反

### 5.2 在 Review 流程中的位置

```
Step 1: Ground Truth Lookup
  ↓
Step 1.5: Coverage Enumeration
  ↓
Step 2: Diff Extraction ← 使用本 Skill 的行为契约表识别 Top 5 语义差异（3 语义偏移 + 2 覆盖缺口）
  ↓
Step 2.5: Link Validation
  ↓
Step 3: Sanity Check ← 验证契约表的"匹配?"列
  ↓
...
Step 6: Action Items ← 对 P0 违反生成具体修改项
```

---

## 六、Pass condition

- 所有核心函数有行为契约表
- 所有 IPC 调用有契约表
- 所有核心资源有生命周期契约表
- 所有"匹配?"列经验证
- 任何 ❌ 有明确判定（允许演进 / P0 违反）

**⛔ 核心函数无契约表 → P0。契约表字段缺失 → P1。**

---

## 七、模块核心语义清单（模板）

> 实际 Review 时，从 C 源码动态生成。

### VM 模块（示例）

| 核心函数 | 语义范围 | 优先级 | 契约表状态 |
|---------|---------|--------|-----------|
| `alloc_mem` | 内存分配 | P0 | 待填写 |
| `free_mem` | 内存释放 | P0 | 待填写 |
| `map_vm` | 地址映射 | P0 | 待填写 |
| `vm_fork` | Fork 语义 | P0 | 待填写 |
| `vm_exit` | Exit 语义 | P0 | 待填写 |
| `vm_brk` | Brk 语义 | P0 | 待填写 |

### PM 模块（示例）

| 核心函数 | 语义范围 | 优先级 | 契约表状态 |
|---------|---------|--------|-----------|
| `do_fork` | 进程创建 | P0 | 待填写 |
| `do_exit` | 进程退出 | P0 | 待填写 |
| `do_exec` | 执行新程序 | P0 | 待填写 |
| `do_waitpid` | 等待子进程 | P0 | 待填写 |
| `do_kill` | 信号发送 | P0 | 待填写 |

### VFS 模块（示例）

| 核心函数 | 语义范围 | 优先级 | 契约表状态 |
|---------|---------|--------|-----------|
| `vfs_open` | 文件打开 | P0 | 待填写 |
| `vfs_close` | 文件关闭 | P0 | 待填写 |
| `vfs_read` | 文件读取 | P0 | 待填写 |
| `vfs_write` | 文件写入 | P0 | 待填写 |
| `vfs_ioctl` | 设备控制 | P0 | 待填写 |
