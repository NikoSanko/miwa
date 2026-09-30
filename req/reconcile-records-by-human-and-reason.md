---
type: Requirement
id: FR-037
---
`reconcile`（`--edge` / `--all` いずれも）がロックに書き込むエントリの `by` は常に `human` とする。`--reason <text>` を指定した場合、その文字列をエントリの `reason` として記録する。`--reason` を指定しない場合、`reason` は記録しない。
