---
type: Requirement
id: FR-079
---
ディレクトリエントリ（FR-070）のリンク先に `index.md` があり、その `index.md` のフロントマターに `description` が無い、空、または改行を含む（FR-071 の要約なしの扱い）とき、`index` 種別の finding を理由 `MissingDirectoryDescription` で報告する（target は子の索引のパス）。ディレクトリエントリの要約（SoT は子 `index.md` の `description`）が空かどうかは見ない。要約が空のエントリは、子に要約があれば `DescriptionMismatch`（FR-077）だけが出て、子にも無ければ `MissingDirectoryDescription` だけが出る。また、根以外の索引（根の子孫ディレクトリにある `index.md`）について、親の索引（FR-072）にその索引のディレクトリを指すディレクトリエントリがないとき、理由 `UnlinkedIndex` で報告する（target は子孫の索引のパス）。
