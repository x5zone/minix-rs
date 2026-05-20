# Diagram Quality Checklist (§2.7)

Check diagrams against all 5 dimensions. Each dimension = ❌ / ⚠️ / ✅.

## 1. Necessity

Does the diagram convey information that plain text cannot easily express?

| Check Item | Assessment |
|-----------|-----------|
| Shows spatial/structural relationships text cannot represent? | ✅/❌ |
| Static data that could be a table instead? | ✅/❌ |
| Is the diagram the best way to communicate this information? | ✅/❌ |

Examples:
- ✅ Flow diagram showing data path across 4 components
- ❌ Static table of constant values drawn as diagram
- ✅ Graph showing resource dependency tree

## 2. Alignment Precision

ASCII art must be pixel-perfect.

| Check Item | Assessment |
|-----------|-----------|
| Box borders strictly aligned (│, ─, ┌, ┐, └, ┘)? | ✅/❌ |
| Arrow heads match target boundary exactly? | ✅/❌ |
| Multi-line text inside boxes left-aligned? | ✅/❌ |
| Column widths consistent across rows? | ✅/❌ |

Anti-patterns:
```
❌ Misaligned:
+------+  +---+
| Long |  | X |
+------+  +---+
    │        │
    ▼        ▼

✅ Aligned:
+----------+  +---+
|   Long   |  | X |
+----------+  +---+
      │          │
      ▼          ▼
```

## 3. Maintainability

| Check Item | Assessment |
|-----------|-----------|
| Can the diagram be edited by adding/deleting one row without cascading reformat? | ✅/❌ |
| Are labels short enough to fit in current column width? | ✅/❌ |
| Does the diagram have >10 unique horizontal positions? | ✅/❌ |

- >10 unique horizontal positions → consider simpler text or splitting into sub-diagrams

## 4. Information Density

| Check Item | Assessment |
|-----------|-----------|
| Does each element carry unique information? | ✅/❌ |
| Are there redundant/repeated elements? | ✅/❌ |
| Is the diagram >50% whitespace (wasted space)? | ✅/❌ |

## 5. Alternative

| Check Item | Assessment |
|-----------|-----------|
| Can a simpler table replace it? | ✅/❌ |
| Can a bullet list replace it? | ✅/❌ |
| Would splitting into 2 smaller diagrams be clearer? | ✅/❌ |

## Summary

```markdown
| Dimension | Score | Notes |
|-----------|-------|-------|
| Necessity | ✅/⚠️/❌ | |
| Alignment | ✅/⚠️/❌ | |
| Maintainability | ✅/⚠️/❌ | |
| Information Density | ✅/⚠️/❌ | |
| Alternative | ✅/⚠️/❌ | |
```

Overall: ❌≥2 → P2 issue.