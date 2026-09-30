---
type: Requirement
id: FR-049
---
`judge --format text`（既定）は、1エッジ1行で `<verdict> <edge id> score=<0.00〜1.00>` を stdout に出力する。スコアは小数第2位まで表示する。reason がある場合は、同じ行の末尾にタブ区切りで reason を付ける。出力順はエッジ ID の昇順とする。スコアが存在しないエッジ（FR-047 の本文超過）は `score=-` と表示する。
