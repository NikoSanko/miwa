---
type: Requirement
id: FR-079
---
ディレクトリエントリ（FR-070）のリンク先に `index.md` があり、そのエントリの要約が空のとき、`index` 種別の finding を理由 `MissingDirectoryDescription` で報告する（target は `<索引のパス> -> <リンク先（pulldown-cmark が解釈した destination。`<…>` や title 属性を除く）>`）。ディレクトリには SoT となる `description` がないため、検査するのは要約が空でないことだけである。また、根以外の索引（根の子孫ディレクトリにある `index.md`）について、親の索引（FR-072）にその索引のディレクトリを指すディレクトリエントリがないとき、理由 `UnlinkedIndex` で報告する（target は子孫の索引のパス）。
