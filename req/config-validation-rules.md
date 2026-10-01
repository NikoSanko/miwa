---
type: Requirement
id: FR-020
---
`.miwa/config.yaml` を読み込む各コマンドは、`judge.threshold` が 0 以上 1 以下の範囲外の場合、`judge.max_body_bytes` が 1 以上の整数でない場合、`docs` / `requirements` / `tests` のいずれかが空配列の場合、および `diff_base` が空文字列の場合、設定エラーとして実行を停止する（exit 1）。
