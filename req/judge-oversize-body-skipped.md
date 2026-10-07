---
type: Requirement
id: FR-045
---
1 エッジの本文（両端の本文のバイト数の合計）が、設定 `judge.max_body_bytes`（既定 200,000）のバイト数を超える場合、`judge` はそのエッジについて Jev を呼ばず、`skipped` として表示する。スコアは存在しないものとして扱う。`judge.max_body_bytes` ちょうどは、超過とみなさない。
