---
type: Requirement
id: FR-011
---
doc から `requirements` 領域内を指すリンクのうち、リンク先ファイルが存在しない場合、またはファイルは存在するがフロントマターの `type` が `Requirement` でない場合、そのエッジは `broken` として報告される。test のマーカー `[req id]` が指す `id` の req が存在しない場合も、同様に `broken` として報告される。
