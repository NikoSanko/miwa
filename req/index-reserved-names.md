---
type: Requirement
id: FR-067
---
`index.md` と `log.md` は予約名とし、`docs` と `requirements` のどちらのルート配下でも、フロントマターに `type` を持っていても doc にも req にもノードとして扱わない。したがって、これらのファイルは `id` の重複・欠落による `invalid` の対象にならず、doc → req のリンクの起点にもならず、`orphan` や `no-test` の判定にも現れない。この挙動は設定で変えられず、`index` 設定の有無にかかわらず常に適用する。
