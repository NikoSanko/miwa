---
type: Requirement
id: FR-078
---
索引の根の配下の文書に `description` が無い、空、または改行を含むとき、`index` 種別の finding を理由 `MissingDescription` で報告する（target は文書のパス）。文書が索引に列挙されているかどうかにはかかわらず報告する。要約の定義は FR-071 に従う。
