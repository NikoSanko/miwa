---
type: Requirement
id: FR-086
---
`index` 設定（`.miwa/config.yaml`）のいずれかの根と同じか、その配下にあるパスの miwa ノード（`type: Specification` の Doc と `type: Requirement` の Req。有効な `id` を持つもの）1 つにつき、要約と本文の対を表すエッジ（`desc` エッジ）を 1 本作る。エッジ ID は Doc では `desc:doc:<doc id>`、Req では `desc:req:<req id>`（FR-023）で、Doc と Req が同じ `id` を持っていても（重複検出は種別ごと）それぞれに別の `desc` エッジができて衝突しない。エッジ種別 `desc` では `from` と `to` が同じノードを指す。`description` が無い・空・改行を含むノードにも `desc` エッジは存在する（`description` を消しても `desc` エッジは消えない）。`index` が空または未設定なら `desc` エッジは 1 本も作らない。索引の根の外にあるノード、`id` が無効なノード、および `Guide` など miwa のノードでない文書（`Specification` でも `Requirement` でもない `type`）には `desc` エッジを作らない。`desc` エッジが `removed` になるのは、ノードが消える・`id` が変わる・ノードが索引の根の配下から外れる（`index` 設定の変更を含む）ときだけである。`desc` エッジは `orphan`（FR-027）と `no-test`（FR-026） の判定には影響しない。
