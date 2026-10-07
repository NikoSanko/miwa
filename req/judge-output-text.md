---
type: Requirement
id: FR-046
---
`judge` 自身の出力は text 形式のみとする。判定結果は、エッジごとに 1行の `<verdict> <edge id> score=<0.00〜1.00>` として stdout に出力する。

verdict は次のいずれかとする。
- `consistent`
- `review`（閾値未満。判断は呼び出し元に任せる）
- `skipped`（本文超過）

スコアは小数第 2 位まで表示する。スコアが存在しない `skipped` は `score=-` とする。出力順は、エッジ ID の昇順とする。
