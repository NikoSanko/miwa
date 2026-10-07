---
type: Requirement
id: FR-011
---
doc から `requirements` 領域内を指すリンクのうち、次のいずれかの場合、そのエッジは `broken` として報告される。
- リンク先ファイルが存在しない
- ファイルは存在するが、フロントマターの `type` が `Requirement` でない

test のマーカー `[req id]` が指す `id` の req が存在しない場合も、同様に `broken` として報告される。
