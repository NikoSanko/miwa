---
type: Requirement
id: FR-032
---
`check --format text`（既定）は、人が読める形式で finding を出力する。`check --format json` は、同じ finding の集合を機械可読な JSON で出力する。両形式は、同じ finding 集合を同じ順序で表す。JSON は `{"findings": [...]}` の形で、各要素は `kind` / `target_kind` / `target` / `reason` / `detail` を持つ。
