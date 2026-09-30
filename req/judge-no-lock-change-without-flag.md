---
type: Requirement
id: FR-053
---
`judge` は `--auto-reconcile` を指定しない場合、ロックファイルを変更しない（ファイルの内容・更新時刻とも変わらない）。`--auto-reconcile` を指定した場合でも、verdict が `contradiction` または `uncertain` のエッジはロックに記録せず、既存のロックエントリも変更しない。
