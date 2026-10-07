---
type: Requirement
id: FR-086
---
`index` 設定（`.miwa/config.yaml`）のいずれかの根と同じか、その配下にあるパスの miwa ノード 1つにつき、要約と本文の対を表すエッジ（`desc` エッジ）を 1本作る。対象とするのは、`type: Specification` の Doc と `type: Requirement` の Req のうち、有効な `id` を持つものである。エッジ ID は、Doc では `desc:doc:<doc id>`、Req では `desc:req:<req id>` とする（FR-023）。Doc と Req が同じ `id` を持っていても（重複検出は種別ごと）、それぞれに別の `desc` エッジができるので衝突しない。エッジ種別 `desc` では、`from` と `to` が同じノードを指す。`description` が無い・空・改行を含むノードにも、`desc` エッジは存在する（`description` を消しても `desc` エッジは消えない）。次の場合は、`desc` エッジを作らない。
- `index` が空または未設定である（`desc` エッジは 1本も作らない）
- ノードが索引の根の外にある
- ノードの `id` が無効である
- `Guide` など miwa のノードでない文書（`Specification` でも `Requirement` でもない `type`）である

`desc` エッジが `removed` になるのは、次のときだけである。
- ノードが消える
- `id` が変わる
- ノードが索引の根の配下から外れる（`index` 設定の変更を含む）

`desc` エッジは、`orphan`（FR-027）と `no-test`（FR-026）の判定には影響しない。
