---
type: Requirement
id: FR-052
---
`judge --auto-reconcile` は、全対象エッジの判定が完了してから、verdict が `consistent` のエッジをまとめて1回の書き込みでロックに記録する。途中で実行エラー（exit 1）になった場合はロックを一切変更しない。記録するエントリは `by: "jev"`、`score`（一次判定のスコア）、`from` / `to`（現在の両端のハッシュ）、`at` を持ち、`reason` は記録しない。ロックの保存形式（エッジ ID 順）は `reconcile` と同じとする。
