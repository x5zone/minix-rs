# NK4-C glm 会话交接（2026-10-02，续-153→190）

本文件是 2026-10-02 会话的交接登记页。会话主线、生产码改动清单、探针台账与验证配方已写入 `NK4C-WORKLOG.md` 的「§1.120续-190-交接」节（commit 54ad18056），本页不重复，只登记一项遗留欠账。

## TODO：riscv 生产改动的追溯 CodeReview（交接后由专人补做，非本文档作者执行）

2026-10-02 会话共有 37 笔提交（区间 `8a9a44933..54ad18056`）。其中约 6 笔生产码改动入库时未经 CodeReview 子代理评审——仅续-153 一笔在提交前完成评审（评审结论 REQUEST_CHANGES，三件意见全部落实后入库，见该提交信息）。以下各笔按项目纪律（生产码改动须过 CodeReview 加全验证链）属于欠账，交接后应逐笔补做追溯评审与回归验证。各笔 commit hash 已按 `git log --oneline 8a9a44933..HEAD` 现查核对：

| 续号 | 文件 | 内容 | commit |
|---|---|---|---|
| 续-160 | `os/servers/vm/src/region/region_map.rs` | `find` 的 iter 兜底（rm-fallback 打点）与 `find_mut` 的 pop_first 全量重建（rm-repair 打点），防御性绕过 BTreeMap range 与 iter 不一致 | 934af1bb2 |
| 续-168 | `os/servers/vm/src/boot.rs` | `read_boot_params` 的 free list 扣减第一层：按 handoff root_paddr 扣 64KB | a10c6d52e |
| 续-175 | `os/servers/vm/src/boot.rs` | 扣减第二层：扩展为全部 boot 模块区 + root 页（flat_map 多区间拆分） | cb11d2eef |
| 续-180 | `os/servers/vm/src/boot.rs` | 扣减第三层：扩展为整池 [0x82000000, 0x84000000) | af7085f0a |
| 续-187 | `os/kernel/src/vm_handoff.rs` | `classify` 不再把 VM 模块 blob reclaim 进 free list（全量保留） | e6aa68f07 |
| 续-179/186 | `os/servers/rs/src/process_table.rs` + `os/servers/vm/src/vm_server.rs` | `by_endpoint` 索引补上界守卫（412 行）；`handle_kernel_memreq` 对 target=VM 走 vm_self_map 自填路径 | 33ed4eb45 |

补做评审时建议的回归口径：host 测试按包集跑（`cargo test -p minix-kernel -p minix-arch -p minix-boot -p minix-types`，勿用 --workspace），已知基线 minix-vm 531 / minix-rs 351（出处：33ed4eb45 提交信息）；真机判据与当前失败尾（`cause_sig: sig manager 8`）见 WORKLOG「续-190-交接」节第三部分。

另有两项与上述欠账同源的登记（正文有据，均未执行）：RegionMap 兜底/重建与 VM 自填路径属防御性代码，若追溯评审后判定腐坏根因已消除，可评估是否保留（续-160 打点语义为「不掩盖」）；探针滚除须过 C-61 pattern-gate（台账见 WORKLOG 续-190 交接第二节）。
