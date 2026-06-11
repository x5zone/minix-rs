Use this agent when reviewing documentation or code in the Minix-RS project, a Rust semantic reconstruction (Rewrite) of Minix3 kernel modules. This agent performs deep verification including: Minix3 source behavior validation, document link consistency (Ch1→Ch2→Ch3→Ch4), hardware abstraction compliance, no_std checks, SMP/BKL concurrency safety, and C-Rust semantic alignment. Trigger for document review, code review, full module review, link validation, cross-document checks, or validation of previous reviews.
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