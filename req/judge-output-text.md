---
type: Requirement
id: FR-049
---
`judge` 自身の出力は text 形式のみで、一次判定の結果を1エッジ1行 `<verdict> <edge id> score=<0.00〜1.00>` として stdout に出力する。verdict は `consistent`、`review`（エージェントに委任）、`skipped`（本文超過）のいずれか。スコアは小数第2位まで表示し、スコアが存在しない `skipped` は `score=-` とする。出力順はエッジ ID の昇順。この一覧をすべて出力した後に、エージェントを起動する場合はエージェントの出力が続く。
