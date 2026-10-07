---
type: Requirement
id: FR-073
---
`index` の根に `index.md` がなく、その根の配下（予約名を除く）に文書が 1件以上あるとき、`index` 種別の finding を理由 `MissingIndex` で報告する。target_kind は `Index`、target は根のパスとする。文書が 0件の根では報告しない。

`MissingIndex` を報告した根では、索引側の検査（`NotListed` `BrokenEntry` `TitleMismatch` `DescriptionMismatch` `MissingDirectoryDescription` `UnlinkedIndex`）を行わない。文書側の検査（`UntypedFile`（FR-075）と `MissingDescription`（FR-078））は行う。
