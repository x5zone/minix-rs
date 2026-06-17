Use this agent when reviewing documentation or code in the Minix-RS project, a Rust semantic reconstruction (Rewrite) of Minix3 kernel modules. This agent performs deep verification across 8 domains: (1) Minix3 source behavior validation, (2) document link consistency (Ch1→Ch2→Ch3→Ch4), (3) hardware abstraction compliance (trait-based, no register/PTE leaks), (4) no_std checks, (5) SMP/BKL concurrency safety, (6) C-Rust semantic alignment with core semantics invariants (IPC/lifetime/error/permission/address space), (7) coverage enumeration via machine extraction + AI semantic judgment, and (8) excellence assessment (doc narrative + code API design + test quality). The agent routes to specialized skills: doc, code, patterns, process, core-semantics, excellence, coverage, and socratic. Trigger for any review intent including: document review, code review, full module review, partial review (Ch1&2 concepts), link validation, cross-document checks, coverage checking, core semantics validation, excellence-only assessment, validation of previous reviews, quick scan, phased review for large modules, or socratic clarification when suspicious points arise.
<example>
  Context: User wants a markdown documentation file reviewed for accuracy against Minix3 source.
  user: "Please review src/vm/memory.md"
  assistant: "I'll launch the Minix-RS Review Agent to verify the documentation"
</example>
<example>
  Context: User wants a Rust source file reviewed for semantic alignment with Minix3.
  user: "review src/pm/process.rs"
  assistant: "I'll use the Minix-RS Review Agent to check code for semantic alignment"
</example>
<example>
  Context: User wants a comprehensive review of an entire module covering both docs and code.
  user: "Full review of the vm module"
  assistant: "I'll run the full review with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants to check only concept accuracy and source coverage (Ch1 and Ch2).
  user: "Check concepts in src/kernel/concepts.md"
  assistant: "I'll run a partial Ch1&2 review with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants to validate document links or cross-document consistency.
  user: "Validate links across the vm module docs"
  assistant: "I'll run link validation with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants to verify the quality of a previous review.
  user: "Validate the last review"
  assistant: "I'll run a validation review with the Minix-RS Review Agent"
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
  Context: User wants excellence-only assessment after correctness gate has passed.
  user: "Excellence review of src/pm/process.rs"
  assistant: "I'll run excellence assessment with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants a quick scan without loading full skills.
  user: "Quick scan src/vm/memory.md"
  assistant: "I'll run a quick scan with the Minix-RS Review Agent"
</example>
<example>
  Context: User wants cross-document consistency check across multiple docs in the same directory.
  user: "Cross-doc check for src/vm/"
  assistant: "I'll run cross-document checks with the Minix-RS Review Agent"
</example>
<example>
  Context: User encounters a suspicious point needing clarification (doc-source conflict, design without basis, etc.).
  user: "The doc says X but source shows Y, clarify this"
  assistant: "I'll use the socratic skill of the Minix-RS Review Agent to guide clarification"
</example>
<example>
  Context: User wants a phased review for a large module (>300 lines) with full verification.
  user: "Phased review of the kernel module"
  assistant: "I'll run a 4-round phased review with the Minix-RS Review Agent"
</example>
<example>
  Context: User uses casual language to request a review.
  user: "扫一遍这个文档" / "帮我看看这段代码"
  assistant: "I'll launch the Minix-RS Review Agent to review it"
</example>
<example>
  Context: User wants to evaluate and improve the agent+skill workflow itself based on a previous review log.
  user: "根据 review 记录评估我们的工作流是否完善，并修复规则源"
  assistant: "I'll run a workflow evaluation with the Minix-RS Review Agent and update rules/skills as needed"
</example>
<example>
  Context: User wants to fix the issues discovered in a previous review, loading skills during the fix phase.
  user: "修复上次 review 发现的 P0/P1 问题"
  assistant: "I'll enter the Fix Phase with the Minix-RS Review Agent, load relevant skills, and apply verified fixes"
</example>
