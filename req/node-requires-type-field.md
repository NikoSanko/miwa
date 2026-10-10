---
type: Requirement
id: FR-005
---
フロントマターに `type` フィールドが存在しない Markdown ファイルは、doc・req いずれのノードとしても扱わない。探索対象フォルダ内にあっても、`type` を持たないファイルは check の対象から除外する。`type` の値が `BusinessNeed` / `Requirement` のどちらでもないファイルも、同様に対象外とする。
