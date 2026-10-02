---
type: Requirement
id: FR-076
---
索引の文書エントリまたはディレクトリエントリのリンク先が有効でないとき、`index` 種別の finding を理由 `BrokenEntry` で報告する。target は `<索引のパス> -> <リンク先（pulldown-cmark が解釈した destination。`<…>` や title 属性を除く）>` の形とする。detail は次のいずれかとする。FR-070 の解決不能（`empty target` `absolute path` `escapes repository` `outside index root` `self or ancestor directory`）はそのままの文言。文書エントリのリンク先が、`exclude` または既定の除外名の配下にある場合は `target is excluded from scan`、存在しない場合は `target does not exist`、フロントマターに `type` がない場合は ``target has no `type` ``、それ以外で文書として扱えない場合は `target is not a document`。ディレクトリエントリのリンク先ディレクトリに `index.md` がない場合は `directory has no index.md`。外部リンクは対象にしない。
