---
name: review-profiles
description: >
  Execute phased, multi-stage Minix-RS reviews to avoid attention overload.
  Splits a complete review into 4 independent stages (H→I→J→K), each loading
  only ~25-40 rules. Invoke when reviewing complex documents (>300 lines) or
  critical modules where single-pass review would miss dimensions.
---

# Phased Review Profiles

Execute a complete Minix-RS review across 4 independent stages. Each stage focuses on a specific dimension, loading only the rules needed for that stage. This avoids attention overload — the AI checks 25-40 rules per stage instead of 150+ at once.

## When to Use

- Complex documents (>300 lines) or critical modules
- Previous single-pass review (Profile C / `review-cross`) missed dimensions
- User explicitly requests staged review: "按 Profile H review xxx.md"
- User wants verifiable, per-stage outputs instead of one large report

Do NOT use for:
- Simple documents (<300 lines) → use `review-cross` (single-pass complete review)
- Single-dimension review (doc only or code only) → use `review-doc` or `review-code`

## Stage Flow

```
Profile H ──→ Profile I ──→ Profile J ──→ Profile K
 Ch1&2          Ch3&4          Code           Cross-doc
 Accuracy       Design         Quality        + Readability
 (50 rules)     (30 rules)     (75 rules)     (30 rules)
```

### Stage Dependency Graph

Each stage **depends on the previous stage's output**. Skipping a stage or executing out of order produces unreliable results.

```
Profile H (Ch1&2 Accuracy) → Profile I (Ch3&4 Design) → Profile J (Code Quality) → Profile K (Cross-doc + Readability)
                              ◄── I depends on H            ◄── J depends on I          ◄── K depends on H+I+J
```

**Key dependencies**:
- **I ← H**: Design quality judgment requires verified Ch1&2 — can't assess "design lacks basis" if concepts aren't verified
- **J ← I**: Code review needs verified design — can't judge "code matches design" if design isn't assessed
- **K ← H+I+J**: Cross-document check needs all prior verification — can't judge cross-document consistency if individual documents aren't verified

**Violation consequence**: Executing Profile I before H may approve designs based on incorrect concepts. Executing J before I may approve code that implements wrong designs.

## Quick Reference

| Profile | Focus | Input | Output | ~Time | Reference |
|---------|-------|-------|--------|-------|-----------|
| **H** | Ch1&2 Accuracy | Doc + C source | P0 concept/ref errors, coverage gaps, P1 Rust/header violations | 20-60 min | [profile-h.md](references/profile-h.md) |
| **I** | Ch3&4 Design Quality | Doc + Stage H output + Rust code | P0 scene gaps, P1 broken linkage | 15-40 min | [profile-i.md](references/profile-i.md) |
| **J** | Code Quality | Rust code + Stage I output | P0 UB/semantic drift, P1 type issues | 20-50 min | [profile-j.md](references/profile-j.md) |
| **K** | Cross-doc + Readability | All sibling .md + Stages H-J output | P2 readability, P1 cross-doc issues | 10-30 min | [profile-k.md](references/profile-k.md) |

## Usage

### Stage H: Ch1&2 Accuracy Verification

```
启动指令: "按 Profile H 对 xxx.md 做 Ch1&2 准确性验证"
```

Validates concept accuracy, C code references, data structure coverage, architecture evolution annotation, diagram quality, C source coverage completeness, Ch1&2 Rust content check (H9), document header norm check (H10), and grep evidence requirement (H11).

> Detailed checklist: [profile-h.md](references/profile-h.md)

### Stage I: Ch3&4 Design Quality Verification

```
启动指令: "按 Profile I 对 xxx.md 做 Ch3&4 设计质量验证。阶段 H 的结论是：[粘贴摘要]"
```

Validates doc-code consistency, design decision quality (traceability, scene coverage, alternatives), and chapter linkage (Ch3→Ch1&2, Ch4→Ch3, Tests→Ch3+Ch4).

> Detailed checklist: [profile-i.md](references/profile-i.md)

### Stage J: Rust Code Quality Verification

```
启动指令: "按 Profile J 对 xxx.rs 做代码质量验证。阶段 I 的结论是：[粘贴摘要]"
```

Validates rewrite quality, hardware abstraction, type safety, execution model, memory model, no_std compliance, design-code consistency, all 15 code review dimensions, C-Rust semantic alignment with leaf function grading (J4a-d), and C source reference comment verification (J8a).

> Detailed checklist: [profile-j.md](references/profile-j.md)

### Stage K: Cross-Document + Readability

```
启动指令: "按 Profile K 对 xxx.md 做跨文档联动和可读性检查。前几轮的结论是：[粘贴摘要]"
```

Validates cross-document consistency (duplicate definitions, contradictions, missing refs), overall readability (fluency, organization, redundancy, reader experience), stale design content check (K4a), and P2 execution strategy.

> Detailed checklist: [profile-k.md](references/profile-k.md)

## Other Profiles (Included for Completeness)

These profiles are covered by the dedicated skills:

| Profile | Equivalent Skill | Use Instead |
|---------|-----------------|-------------|
| A (Doc Review only) | `review-doc` | Use `review-doc` skill directly |
| B (Code Review only) | `review-code` | Use `review-code` skill directly |
| C (Complete Review, single-pass) | `review-cross` | Use `review-cross` skill (simpler docs) |
| D (Quick scan) | — | Use `review-doc` or `review-code` with oral instructions to "only flag P0" |
| E (Cross-doc only) | `review-doc` Step 11 | Use `review-doc` and focus on Step 11 |
| F (Linkage only) | `review-doc` Step 7 | Use `review-doc` and focus on Step 7 |
| G (Ch1&2 only) | `review-doc` Steps 1-5.6 | Use `review-doc` with oral instruction to limit to Steps 1-5.6 |

## H→I→J→K vs Single-Pass (review-cross)

| Dimension | Single-Pass (`review-cross`) | Staged (H→I→J→K) |
|-----------|------------------------------|-------------------|
| Total time | 40-120 min | 65-180 min (longer total) |
| Completeness | Medium (attention scatters, easy to miss) | High (per-stage focus, 25-40 rules each) |
| Verifiability | Low (hard to know which dimensions skipped) | High (each stage output independently checkable) |
| Human iteration | May need 2-3 manual cycles | Independent per stage, no iteration needed |
| Best for | Simple docs (<300 lines) | Complex docs (>300 lines) or critical modules |

## Decision Guide

- Doc < 300 lines + non-critical → `review-cross`
- Doc > 300 lines OR critical module → **H→I→J→K (this skill)**
- If single-pass review needs manual iteration → use staged review next time

## Reference Files

| File | Profile | When to Load |
|------|---------|-------------|
| [profile-h.md](references/profile-h.md) | H | Stage 1: Ch1&2 accuracy (50 rules, incl. H9 Rust check, H10 header, H11 grep evidence) |
| [profile-i.md](references/profile-i.md) | I | Stage 2: Ch3&4 design quality (30 rules) |
| [profile-j.md](references/profile-j.md) | J | Stage 3: Rust code quality (75 rules, incl. J4a-d alignment, J8a C ref comments) |
| [profile-k.md](references/profile-k.md) | K | Stage 4: Cross-doc + readability (30 rules, incl. K4a stale design, P2 strategy) |