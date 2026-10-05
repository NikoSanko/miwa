---
type: Requirement
id: FR-077
---
文書エントリのリンク文が、リンク先の文書のタイトル（FR-071）と一致しないとき、`index` 種別の finding を理由 `TitleMismatch` で報告する。文書に要約があり、エントリの要約（FR-069）がそれと一致しないとき、理由 `DescriptionMismatch` で報告する。どちらも target は文書のパスとし、比較はエスケープを戻した文字列の完全一致で行う。文書に要約がない場合は `DescriptionMismatch` を出さず、比較もしない（`MissingDescription` だけが出る）。同じ文書を指すエントリが複数あれば、そのすべてを比較する。ディレクトリエントリ（FR-070）にも同じ規則を適用し、照合先は子 `index.md` のフロントマターから決めるディレクトリのタイトルと要約（FR-071）とする。リンク文がディレクトリのタイトルと一致しなければ `TitleMismatch`、ディレクトリに要約があってエントリの要約が一致しなければ `DescriptionMismatch` を報告し、ディレクトリに要約がなければ `DescriptionMismatch` は出さない（`MissingDirectoryDescription` だけが出る。FR-079）。ディレクトリエントリの `TitleMismatch` / `DescriptionMismatch` の target は `<索引のパス> -> <リンク先>` とする（`MissingDirectoryDescription` の target は子の索引のパス。FR-079）。
