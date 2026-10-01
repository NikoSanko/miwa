---
type: Requirement
id: FR-046
---
`judge` 自身の出力は text 形式のみで、判定結果を1エッジ1行 `<verdict> <edge id> score=<0.00〜1.00>` として stdout に出力する。verdict は `consistent`、`review`（閾値未満。判断は呼び出し元に任せる）、`skipped`（本文超過）のいずれか。スコアは小数第2位まで表示し、スコアが存在しない `skipped` は `score=-` とする。出力順はエッジ ID の昇順。
