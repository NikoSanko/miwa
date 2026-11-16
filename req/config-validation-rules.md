---
type: Requirement
id: FR-020
---
`.miwa/config.yaml` を読み込む各コマンドは、次のいずれかに該当する場合、設定エラーとして実行を停止する（exit 1）。
- 指定した `judge.threshold` が 0 以上 1 以下の範囲外である
- 指定した `judge.max_body_bytes` が 1 以上の整数でない

`judge` の節や項目を省略した場合と `null` の場合は、既定値を使う（FR-126）。
- `docs` / `requirements` / `tests` のいずれかが空配列である
- `diff_base` が空文字列である
