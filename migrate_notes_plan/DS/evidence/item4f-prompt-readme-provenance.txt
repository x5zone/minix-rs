=== prompt/README.md 迁移前 440-455 行 ===
4. **工具支持**：`tools/design-coverage-check.sh {module}` 自动扫描所有 stage 的 `.design/` 目录，输出缺失报告。Session 启动时跑此工具 → 报告写入 STATE.md `§启动预检` 段。

5. **决策记录豁免**：仅一次性用户明确豁免；豁免必须登记在 STATE.md `§豁免列表` 段；**不可泛化**（**模式 71 DOG 触发**）。

6. **STATE.md Resume Point 模板**：续 session 必含 `ls .design/` 预检命令（避免续 session 跳过）。

**当前快照覆盖情况**（`tools/design-coverage-check.sh fork-syscall-rewrite --stage 01-stage-kernel` 输出（历史快照数字，以工具当前输出为准））：
- Total docs: 29
- Complete (design + outline): **3**（仅 01/02/03）
- Missing outline (H.6 FAIL): 24
- Missing design (H.1 FAIL): **24**

**含义**：fork-syscall-rewrite 模块 01-stage-kernel 阶段 **22/29 文档缺 design 快照**（01-07 已审过有快照，08-25/99/00 按需生成）。按新规则，缺失时 review 自动执行 **Step 0.3 嵌入生成**（2026-07-17 变更：原"必须先附录 C 追溯生成"改为"Step 0.3 嵌入 review 流程内生成"）。

> **精简说明**：所有维度检查结果（概念/引用/结构/覆盖/设计/链路/代码/跨文档/Claims）全部并入 `scan.md` 对应章节，不再拆 10 个维度文件。


=== prompt/README.md 当前 440-455 行 ===
4. **工具支持**：`tools/design-coverage-check.sh {stage}` 自动扫描所有 stage 的 `.design/` 目录，输出缺失报告。Session 启动时跑此工具 → 报告写入 STATE.md `§启动预检` 段。

5. **决策记录豁免**：仅一次性用户明确豁免；豁免必须登记在 STATE.md `§豁免列表` 段；**不可泛化**（**模式 71 DOG 触发**）。

6. **STATE.md Resume Point 模板**：续 session 必含 `ls .design/` 预检命令（避免续 session 跳过）。

**当前快照覆盖情况**（`tools/design-coverage-check.sh fork-syscall-rewrite --stage 01-stage-kernel` 输出（历史快照数字，以工具当前输出为准））：
- Total docs: 29
- Complete (design + outline): **3**（仅 01/02/03）
- Missing outline (H.6 FAIL): 24
- Missing design (H.1 FAIL): **24**

**含义**：fork-syscall-rewrite 模块 01-stage-kernel 阶段 **22/29 文档缺 design 快照**（01-07 已审过有快照，08-25/99/00 按需生成）。按新规则，缺失时 review 自动执行 **Step 0.3 嵌入生成**（2026-07-17 变更：原"必须先附录 C 追溯生成"改为"Step 0.3 嵌入 review 流程内生成"）。

> **精简说明**：所有维度检查结果（概念/引用/结构/覆盖/设计/链路/代码/跨文档/Claims）全部并入 `scan.md` 对应章节，不再拆 10 个维度文件。

