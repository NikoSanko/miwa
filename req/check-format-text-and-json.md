---
type: Requirement
id: FR-032
---
`check --format text`（既定）は人が読める形式で finding を出力する。`check --format json` は同じ finding の集合を機械可読な JSON で出力する。両形式は同じ finding 集合・同じ順序を表す。JSON は `{"findings": [...]}` の形で、各要素は `kind` / `target_kind` / `target` / `reason` / `detail` を持つ。
