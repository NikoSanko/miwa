---
type: Requirement
id: FR-076
---
索引の文書エントリまたはディレクトリエントリのリンク先が有効でないとき、`index` 種別の finding を理由 `BrokenEntry` で報告する。
target は `<索引のパス> -> <リンク先>` の形とする。リンク先は、pulldown-cmark が解釈した destination とし、`<…>` や title 属性を除く。

detail は次のいずれかとする。
- FR-070 の解決不能の場合は、`empty target` `absolute path` `escapes repository` `outside index root` `self or ancestor directory` の文言をそのまま使う。
- 文書エントリのリンク先が `exclude` または既定の除外名の配下にある場合は、`target is excluded from scan` とする。
- 文書エントリのリンク先が存在しない場合は、`target does not exist` とする。
- 文書エントリのリンク先のフロントマターに `type` がない場合は、`` target has no `type` `` とする。
- 文書エントリのリンク先が、それ以外で文書として扱えない場合は、`target is not a document` とする。
- ディレクトリエントリのリンク先ディレクトリに `index.md` がない場合は、`directory has no index.md` とする。

外部リンクは対象にしない。
