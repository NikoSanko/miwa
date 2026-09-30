---
type: Requirement
id: FR-005
---
Markdown ファイルのフロントマターに `type` フィールドが存在しない場合、そのファイルは doc・req いずれのノードとしても扱わない。探索対象フォルダ内にあっても、`type` を持たないファイルは check の対象から除外される。`type` の値が `Specification` / `Requirement` のどちらでもないファイルも同様に対象外とする。
