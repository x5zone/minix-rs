### Review 范围声明
- **Review 模式**：完整 Review（文档 + 代码）
- **目标文档**：notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/08-slab-allocator.md
- **关联 C 源**：minix3/minix/servers/vm/slaballoc.c
- **关联 Rust 代码**：os/servers/vm/src/global.rs, os/servers/vm/src/direct_map.rs, os/servers/vm/src/vm_server.rs
- **同目录文档范围**：notes/rewrite/fork-syscall-rewrite/02-stage-vm/*.md
- **执行步骤**：Step 1-6（按 review-process.md 执行）

---

## AI Review 输出（Demo）

### 摘要
- **文档/代码**：`08-slab-allocator.md` / `slaballoc.c`, `global.rs`
- **Review 类型**：完整 Review（文档 + 代码）
- **发现问题数**：P0=0, P1=3, P2=2

### 问题清单
| 优先级 | 位置 | 问题描述 | 依据（源码/规则） | 建议修复 |
|--------|------|---------|------------------|---------|
| P1 | `08-slab-allocator.md` §3.1 | 文档断言“完全不实现专用 slab”需补充 benchmark/metrics 触发条件 | 文档主张 (notes/.../08-slab-allocator.md §3.1) | 在 Ch3 增加量化触发条件（如 alloc/s 的阈值、profiling 指标） |
| P1 | `slaballoc.c` & `08-slab-allocator.md` §2.1 | 文档未在 Ch2 明确列出 `GETSLAB` 导致的稀疏 `slabs` 数组影响 | `slaballoc.c` GETSLAB 宏（minix3/…/slaballoc.c#L85） | 在 Ch2 增补位图/索引使用统计与内存占用示例 |
| P1 | `global.rs` | `VmAllocator` bump 实现在注释与代码间存在微差（文档强调已实现 sub-page 管理） | `os/servers/vm/src/global.rs` 实现（line ~120） | 在文档中明确说明当前是 bump arena（已实现 sub-page 管理）并引用 global.rs 行号作为证据 |
| P2 | `08-slab-allocator.md` 文档可读性 | 部分长表格和宏解释需缩短为要点 | 文档风格优化 | 简化部分宏解释，移动细节到附录 |
| P2 | 测试覆盖示例（文档 §6） | 建议把部分测试代码直接放入 `os/servers/vm/tests` 以便 CI 验证 | 文档建议 | 提供测试文件模版并在 CI 中启用 |

### 链路验证结果
- **Ch3→Ch1&2**：✅ 说明设计（不实现 slab）能追溯到 Ch1/Ch2 的历史与动机，但需补充量化阈值（见 P1）
- **Ch4→Ch3**：✅ `global.rs` 的 `VmAllocator` 实现匹配 Ch3 的 bump allocator 设计（见证据）
- **测试覆盖**：❌ 部分测试为文档片段，未在代码仓库中完全实现（需把示例测试文件加入 `os/servers/vm/src`）
- **代码→Ch4**：✅ `slaballoc.c` 的 MEMPROTECT/`vm_pagelock` 行为在 C 源中证实，Rust 实现已移除该机制（`global.rs` 與 `direct_map.rs` 证明 Direct Map 与 bump 策略）

### 确认清单
- [ ] 所有 P1 问题已修复或已注释说明
- [ ] 将示例测试文件移入仓库并通过 CI
- [ ] 文档中增加 YAML manifest（见建议）

---

## 证据与执行命令（最小可重复命令）
- 查找 C 源 `slaballoc.c` 并定位函数：
```bash
rg "void \*slaballoc|void slabfree|vm_pagelock" -n minix3/minix/servers/vm
``` 
- 关键代码位置（已定位）：
  - `minix3/minix/servers/vm/slaballoc.c`[minix3/minix/servers/vm/slaballoc.c#L259-L340]
  - `minix3/minix/servers/vm/slaballoc.c`[minix3/minix/servers/vm/slaballoc.c#L406-L520]
  - `minix3/minix/servers/vm/pagetable.c`[minix3/minix/servers/vm/pagetable.c#L403-L420]

- 查找 Rust 全局分配器实现：
```bash
rg "struct VmAllocator|register_page_alloc|global_allocator" -n os/servers/vm
```
  - 证据文件：`os/servers/vm/src/global.rs`[os/servers/vm/src/global.rs#L120-L212]

---

## 自动化 Issue/JSON 模板（示例）
{
  "title": "P1: 增补 slab 文档中的量化触发条件",
  "labels": ["review","P1","docs"],
  "body": "在 `08-slab-allocator.md` Ch3 中增加 profiling 阈值（例如: phys_block allocs/sec > 100k 或 active_pages > 1024）以决定引入专用 slab。参见 `os/servers/vm/src/global.rs` 行 120-190 的 VmAllocator 实现作为当前基线。"
}
