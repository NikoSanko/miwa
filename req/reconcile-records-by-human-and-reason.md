---
type: Requirement
id: FR-037
---
`reconcile` は、`--edge` / `--all` のいずれでも、ロックに書き込むエントリの `by` を常に `human` とする。`--reason <text>` を指定した場合は、その文字列をエントリの `reason` として記録する。`--reason` を指定しない場合、`reason` は記録しない。
