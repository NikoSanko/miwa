---
type: Requirement
id: FR-044
---
Jev のスコアが設定 `judge.threshold` 以上のエッジは、`consistent` として扱い、エージェントに渡さない。スコアが `judge.threshold` 未満のエッジは `review`（エージェントに委任する対象）とする。比較は「スコア >= 閾値」とし、スコアが閾値と等しい場合は `consistent` となる。
