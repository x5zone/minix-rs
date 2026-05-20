# Readability Checklist (§3.1-3.4 + §3.6)

## §3.1: Text Fluency

- [ ] No grammar errors, wrong sentences, or ambiguous sentences?
- [ ] Long sentences (>40 chars) overused? Can they be split?
- [ ] Pronoun references clear ("it", "this", "above" — what do they refer to)?
- [ ] Passive voice overused? Technical docs prefer active voice
- [ ] Same concept uses consistent terminology (not mixing "page directory/PD/page table root")?
- [ ] Punctuation standard? Chinese/English punctuation not mixed?

## §3.2: Logic Organization & Information Flow

- [ ] Clear logical connections between paragraphs? (cause-effect, progression, contrast, transition)
- [ ] Any "jumps" — reader must fill in intermediate steps?
- [ ] Complex concepts follow "general before specific" or "simple before complex" order?
- [ ] Code explanations follow "show overview, then explain line-by-line" or "pose problem, then show solution"?
- [ ] Uneven information density — some paragraphs too dense, others too sparse?
- [ ] Section titles accurately summarize content? No "clickbait" titles?

## §3.3: Redundancy Control

- [ ] **Within document**: Same concept explained multiple times? Is repetition necessary?
- [ ] **Across documents**: Concept A detailed in doc X, does doc Y only need brief + reference?
- [ ] "Copy-paste for completeness" large blocks of repetition?
- [ ] Code comments and document text saying the same thing? (Keep one detailed, other references)
- [ ] Can "see [xxx.md](xxx.md)" replace large expansions?

**Judgment**:
- Same concept repeated >2 times in one document → check if necessary
- Cross-document repetition >3 lines → should be brief + reference
- Code block and corresponding text saying same thing → keep text, add brief comment to code

## §3.4: Reader Experience

- [ ] New concepts introduced with sufficient context?
- [ ] Technical terms defined/explained on first use?
- [ ] Assumed reader knowledge — are assumptions reasonable?
- [ ] Complex processes have "conclusion first, details later" guidance?
- [ ] Long documents have table of contents or anchors for navigation?
- [ ] Key conclusions visually highlighted (bold, blockquote, table)?
- [ ] Code-to-text ratio reasonable? (Pure code no explanation = bad; pure text no code = also bad)
- [ ] **Document code block comments use Chinese** (Chinese document, aids readability); Rust source comments use English (community convention)
- [ ] Abstract concepts have concrete examples or analogies?
- [ ] Analogies appropriate? Won't introduce new misunderstandings?

**Judgment**:
- New concept first appears, no explanation within 3 lines → P2
- Complex process without "TL;DR" or "conclusion first" → P2
- 20+ consecutive lines of pure text without code/table/list → P2
- 30+ consecutive lines of pure code without explanation → P2

## §3.6: P2 Execution Strategy

P2 checks are numerous and subjective. This strategy ensures P2 checks produce real output:

1. **Quick scan first**: Browse full document, note first impression (~30s). If feels good, mark "no obvious readability issues" and skip detailed check
2. **Sample if needed**: If issues suspected, sample 3 spots (beginning, middle, end), check §3.1-3.4
3. **Focus on top 3 high-frequency problems**:
   - **Terminology inconsistency** (same thing called 3 different names)
   - **Information jumps** (reader must fill in gaps)
   - **Bare code without explanation** (>30 lines code with no text)
4. **Minimal P2 output**: Only output actual issues found, not per-item pass/fail
5. **Batch processing**: If doc >500 lines, sample 1-2 paragraphs per chapter
