---
type: Requirement
id: FR-044
---
Jev のスコアが設定 `judge.threshold` 以上のエッジは、`consistent` として扱う。スコアが `judge.threshold` 未満のエッジは、`review`（閾値未満。整合しているかの判断は呼び出し元に任せる）とする。比較は「スコア >= 閾値」とし、スコアが閾値と等しい場合は `consistent` となる。
