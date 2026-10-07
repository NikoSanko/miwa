---
type: Requirement
id: FR-020
---
`.miwa/config.yaml` を読み込む各コマンドは、次のいずれかに該当する場合、設定エラーとして実行を停止する（exit 1）。
- `judge.threshold` が 0 以上 1 以下の範囲外である
- `judge.max_body_bytes` が 1 以上の整数でない
- `docs` / `requirements` / `tests` のいずれかが空配列である
- `diff_base` が空文字列である
