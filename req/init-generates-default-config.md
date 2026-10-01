---
type: Requirement
id: FR-019
---
`init` は `.miwa/config.yaml` を既定値で生成する。既に `.miwa/config.yaml` が存在する場合、既存ファイルを上書きせず失敗（exit 1）する。 既定値は `judge.threshold` が 0.9 とする。
