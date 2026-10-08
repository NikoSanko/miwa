---
type: Requirement
id: FR-080
---
`index` 種別の finding はロックと独立であり、`reconcile`（`--edge` と `--all`）の成否や記録内容に影響しない。`index` の finding が残っていても、`reconcile --all` は成功する。

`check --gate` では、他の finding と同様に1件でもあれば exit 2 とする。`check --diff` では、他の finding と同じく基準側との集合差を取る。基準側の設定に `index` がない場合は、現在側の `index` finding がすべて新規として報告される。

`--format text` では種別を `index` と表示する。`--format json` では `kind` を `Index` とする（既存の種別と同じ表記規則）。
