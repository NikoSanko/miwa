---
type: Requirement
id: FR-051
---
`judge --auto-reconcile` は、全エッジの一次判定が完了した後、エージェントを起動する前に、`consistent` のエッジをまとめて1回の書き込みでロックに記録する。一次判定の途中で実行エラーになった場合はロックを一切変更しない。記録するエントリは `by: "jev"`、`score`（Jev のスコア）、`from` / `to`（現在の両端のハッシュ）、`at` を持ち、`reason` は記録しない。`review` / `skipped` のエッジは `judge` 自身はロックに記録しない（エージェントが `miwa reconcile` を実行した場合は、`reconcile` の仕様どおり `by: human` で記録される）。`--auto-reconcile` を指定しない場合、`judge` 自身はロックを変更しない。
