---
type: Requirement
id: FR-085
---
`miwa index` は、エントリ行以外のバイト列（前書きの段落、節見出し、エントリの並び順、入れ子の子項目、BOM、改行コード）を変更しない。Markdown の再整形や AST の往復による書き換えはしない。出力は決定的であり、続けて 2 回実行すると 2 回目はすべて変更なしになる。ファイルの書き込みは、同じディレクトリの一時ファイルへ書いてから rename する。
`index.md` が UTF-8 として読めないとき、その索引は書き換えない。この場合は、パスを示して実行エラーとし、exit 1 を返す。`index` が未設定（空）のときは `index is not configured in .miwa/config.yaml` を示して実行エラーとし、exit 1 を返す。
成功時は索引ごとに 1 行を標準出力へ出し、exit 0 を返す。行は、新規作成なら `created <path>`、更新なら `updated <path>`、変更なしなら `unchanged <path>` とする。
実行後の `check` に残りうる `index` finding は、人の判断が要る `MissingDescription` `MissingDirectoryDescription` `UntypedFile` と、文書でないファイルを指す `BrokenEntry` だけである。
