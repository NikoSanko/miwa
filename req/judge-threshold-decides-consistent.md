---
type: Requirement
id: FR-044
---
一次判定のスコアが設定 `judge.threshold` 以上のエッジは verdict を `consistent`（判定者 `by` は `jev`）とし、二次判定を呼ばない。スコアが `judge.threshold` 未満のエッジは二次判定へ進む。比較は「スコア >= 閾値」とし、スコアが閾値と等しい場合は `consistent` となる。
