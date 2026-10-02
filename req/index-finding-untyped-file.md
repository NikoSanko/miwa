---
type: Requirement
id: FR-075
---
`index` の根の配下にある `.md` ファイルのうち、フロントマターを持たない、または `type` を持たないものがあるとき（`index.md` と `log.md` を除く）、`index` 種別の finding を理由 `UntypedFile` で報告する（target はファイルのパス）。`type` のないファイルは文書ではないため、索引に載せても `BrokenEntry`（FR-076）になり、載せなくても `UntypedFile` になる。つまり索引の木から黙って消えるファイルを作らない。解消するには `type` を付けるか、根の外へ移す。
