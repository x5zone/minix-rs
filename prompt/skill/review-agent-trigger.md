Use this agent to review docs/code in Minix-RS (Rust semantic reconstruction of Minix3). Verifies 8 domains: Minix3 source behavior, doc link consistency (Ch1→Ch2→Ch3→Ch4), HW abstraction (trait-based, no register/PTE leaks), no_std, SMP/BKL concurrency, C-Rust semantic alignment (IPC/lifetime/error/permission/address space), coverage enumeration (machine + AI), and excellence assessment. Routes to specialized skills: doc, code, patterns, process, core-semantics, excellence, coverage, socratic. Profiles: A/B/G (constructive) / D (quick) / C/H-K (deep) / **R (Design-First)** — triggers when Step 0 design 预检 finds `design.md`/`design-final.md` missing, or review target is design doc, or design ↔ code alignment check (Gate H). New modes (2026-07-16): workflow evaluation/optimization, fix review findings, Step 0 design snapshot backfill (模式 69 PSMD), TODO staleness check (模式 70 CTOS), decision over-generalization (模式 71 DOG).
<example>
  Context: User wants a markdown doc reviewed for accuracy against Minix3 source.
  user: "Please review src/vm/memory.md"
  assistant: "I'll launch the Minix-RS Review Agent to verify the documentation"
</example>
<example>
  Context: User wants Rust code reviewed for semantic alignment with Minix3.
  user: "review src/pm/process.rs"
  assistant: "I'll use the Minix-RS Review Agent to check code for semantic alignment"
</example>
<example>
  Context: User wants full module review (both docs and code).
  user: "Full review of the vm module"
  assistant: "I'll run the full review with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants concept accuracy + source coverage only (Ch1&2).
  user: "Check concepts in src/kernel/concepts.md"
  assistant: "I'll run a partial Ch1&2 review with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants to check C source coverage completeness.
  user: "Check coverage for the pm module"
  assistant: "I'll run coverage enumeration with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants to verify core semantics invariants (IPC/lifetime/error/permission/address space).
  user: "Verify core semantics in src/vm/fault.rs"
  assistant: "I'll run core semantics validation with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants a quick scan without loading full skills.
  user: "Quick scan src/vm/memory.md"
  assistant: "I'll run a quick scan with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants design-first review where the design itself is the target.
  user: "review 06-design.md" / "review 06-design-final.md" / "我刚写完 design，帮我 review 一下"
  assistant: "I'll launch the Minix-RS Review Agent in Profile R (Design-First Review Mode) to verify design's completeness, correctness, and implementability"
</example>
<example>
  Context: User wants to check if code matches the design doc.
  user: "检查代码是否实现了 design" / "verify design ↔ code alignment"
  assistant: "I'll run the Minix-RS Review Agent with Profile R to verify design ↔ code alignment (Gate H)"
</example>
<example>
  Context: User wants to fix issues from a previous review, loading skills during fix phase.
  user: "修复上次 review 发现的 P0/P1 问题"
  assistant: "I'll enter the Fix Phase, load relevant skills, and apply verified fixes"
</example>
<example>
  Context: User wants to evaluate the review workflow for issues/optimizations after a session ends.
  user: "工作流有没有问题？" / "优化 review 工作流" / "判断当前 review 工作流是否有可优化的地方"
  assistant: "I'll analyze the review workflow, identify gaps (e.g. 模式 69 PSMD/70 CTOS/71 DOG), and propose optimizations with concrete fixes to prompt/ files"
</example>
<example>
  Context: User wants to backfill missing design/outline snapshots after discovering a doc never generated them.
  user: "06 缺 design 快照，补一下" / "追溯生成 06 outline.md"
  assistant: "I'll run design-coverage-check.sh, then execute Step 0.3 embedded generation (outline→outline-review→design, 独立推导非复用)"
</example>